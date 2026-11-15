//! Writing display lists as PDF pages, for printing and saving what a screen shows
//! (a report, an invoice, a table) without a GPU.
//!
//! Everything stays vector: rounded rectangles, shapes, gradients and icons become
//! paths, and text becomes the outlines of its glyphs, so it prints sharp at any
//! size and needs no fonts in the file. The text cannot be selected or searched in a
//! viewer. Backdrops are drawn as their tint, without the blur. Images are embedded
//! when they were given with [`PdfDocument::set_image`] and skipped otherwise.
//!
//! One logical pixel is one PDF point (1/72 inch); a page is the size given to
//! [`PdfDocument::add_page`].
//!
//! ```no_run
//! # use noa_gpui_render_wgpu::PdfDocument;
//! # use noa_gpui_scene::{DisplayList, Size};
//! # use noa_gpui_text::{FontOptions, TextStore};
//! # let (list, mut text) = (DisplayList::new(), TextStore::new(FontOptions::default()));
//! let mut pdf = PdfDocument::new();
//! pdf.add_page(&list, &mut text, Size::new(595.0, 842.0), None); // A4
//! pdf.save("report.pdf").expect("write");
//! ```

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

use noa_gpui_scene::{
    Border, Color, CornerRadii, Direction, DisplayList, DrawCommand, IconPaint, ImageId,
    PathCommand, Point, Rect, Shape, Size, VectorIcon,
};
use noa_gpui_text::TextStore;
use noa_gpui_text::cosmic_text::{Command, SwashCache};

use crate::offscreen::RgbaImage;

/// Glyph outlines are taken at this many times the text's size, then scaled down, so
/// their positions are not rounded to whole points.
const TEXT_SCALE: f32 = 4.0;
/// A cubic Bézier through a quarter circle's ends and these control distances (times
/// the radius) stays within 0.03% of the circle.
const KAPPA: f32 = 0.552_284_8;

/// A PDF being built, a page at a time (see the module documentation).
pub struct PdfDocument {
    pages: Vec<Page>,
    images: HashMap<ImageId, RgbaImage>,
    /// The images the pages draw, in the order they were first drawn.
    used_images: Vec<ImageId>,
    /// Alpha values (in thousandths) the pages use, each a graphics state.
    alphas: BTreeMap<u16, ()>,
    shadings: Vec<Shading>,
    swash: SwashCache,
}

struct Page {
    size: Size,
    content: String,
}

struct Shading {
    from: Point,
    to: Point,
    start: Color,
    end: Color,
}

impl Default for PdfDocument {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PdfDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfDocument")
            .field("pages", &self.pages.len())
            .field("images", &self.images.len())
            .finish_non_exhaustive()
    }
}

impl PdfDocument {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            images: HashMap::new(),
            used_images: Vec::new(),
            alphas: BTreeMap::new(),
            shadings: Vec::new(),
            swash: SwashCache::new(),
        }
    }

    /// The pixels of image `id` (RGBA8, rows top to bottom, not premultiplied), for the
    /// [`DrawCommand::Image`]s that draw it: the same pixels the app gave the
    /// renderer when it made the image.
    pub fn set_image(&mut self, id: ImageId, image: RgbaImage) {
        self.images.insert(id, image);
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Add a page of `size` points showing `list`, laid out with `text`, on
    /// `background` (`None`: none, the paper's color).
    pub fn add_page(
        &mut self,
        list: &DisplayList,
        text: &mut TextStore,
        size: Size,
        background: Option<Color>,
    ) {
        let mut out = Content::default();
        // Points from the top left, y down, as the display list has them.
        out.op(format_args!("1 0 0 -1 0 {} cm", num(size.height)));
        if let Some(color) = background {
            self.fill_with(&mut out, color, |out| {
                out.rect(Rect::new(0.0, 0.0, size.width, size.height));
            });
        }
        for command in list.commands() {
            match command {
                DrawCommand::Quad {
                    rect,
                    radius,
                    color,
                    border,
                } => self.quad(&mut out, *rect, *radius, *color, *border),
                DrawCommand::Shape { shape, color } => self.shape(&mut out, shape, *color),
                DrawCommand::Image { rect, image } => self.image(&mut out, *rect, *image),
                DrawCommand::Icon { rect, icon, color } => self.icon(&mut out, *rect, icon, *color),
                DrawCommand::Backdrop {
                    rect, radius, tint, ..
                } => self.quad(&mut out, *rect, [*radius; 4], *tint, Border::default()),
                DrawCommand::Text {
                    rect,
                    layout,
                    color,
                } => self.text(&mut out, text, *rect, *layout, *color),
                DrawCommand::PushClip { rect } => {
                    out.op(format_args!("q"));
                    out.rect(*rect);
                    out.op(format_args!("W n"));
                }
                DrawCommand::PopClip => out.op(format_args!("Q")),
            }
        }
        self.pages.push(Page {
            size,
            content: out.0,
        });
    }

    /// The finished file.
    pub fn finish(&self) -> Vec<u8> {
        write_file(self)
    }

    pub fn save(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        std::fs::write(path, self.finish())
    }

    /// Paint what `path` draws in `color`: a fill, with the color's alpha.
    fn fill_with(&mut self, out: &mut Content, color: Color, path: impl FnOnce(&mut Content)) {
        if color.a <= 0.0 {
            return;
        }
        out.op(format_args!("q"));
        self.alpha(out, color.a);
        out.op(format_args!(
            "{} {} {} rg",
            num(color.r),
            num(color.g),
            num(color.b)
        ));
        path(out);
        out.op(format_args!("f"));
        out.op(format_args!("Q"));
    }

    /// Stroke what `path` draws in `color`, `width` wide, with round caps and joins.
    fn stroke_with(
        &mut self,
        out: &mut Content,
        color: Color,
        width: f32,
        path: impl FnOnce(&mut Content),
    ) {
        if color.a <= 0.0 || width <= 0.0 {
            return;
        }
        out.op(format_args!("q"));
        self.alpha(out, color.a);
        out.op(format_args!(
            "{} {} {} RG {} w 1 J 1 j",
            num(color.r),
            num(color.g),
            num(color.b),
            num(width)
        ));
        path(out);
        out.op(format_args!("S"));
        out.op(format_args!("Q"));
    }

    fn alpha(&mut self, out: &mut Content, alpha: f32) {
        let key = (alpha.clamp(0.0, 1.0) * 1000.0).round() as u16;
        if key >= 1000 {
            return;
        }
        self.alphas.insert(key, ());
        out.op(format_args!("/A{key} gs"));
    }

    /// The fill and the border do not overlap, as on the GPU: the fill covers the
    /// inside of the border, and the border is a ring, so a translucent border shows
    /// what is under the quad rather than its fill.
    fn quad(&mut self, out: &mut Content, rect: Rect, radii: CornerRadii, color: Color, b: Border) {
        let max = (rect.width / 2.0).min(rect.height / 2.0).max(0.0);
        let radii = radii.map(|r| if r.is_nan() { 0.0 } else { r.clamp(0.0, max) });
        let w = b.width.clamp(0.0, max);
        if w <= 0.0 {
            self.fill_with(out, color, |out| out.rounded_rect(rect, radii));
            return;
        }
        let (inner, inner_radii) = inset(rect, radii, w);
        // A border as wide as half the quad leaves no inside.
        let hollow = w < max;
        if hollow {
            self.fill_with(out, color, |out| out.rounded_rect(inner, inner_radii));
        }
        self.fill_even_odd_with(out, b.color, |out| {
            out.rounded_rect(rect, radii);
            if hollow {
                out.rounded_rect(inner, inner_radii);
            }
        });
    }

    /// [`PdfDocument::fill_with`] with the even-odd rule, for rings.
    fn fill_even_odd_with(
        &mut self,
        out: &mut Content,
        color: Color,
        path: impl FnOnce(&mut Content),
    ) {
        if color.a <= 0.0 {
            return;
        }
        out.op(format_args!("q"));
        self.alpha(out, color.a);
        out.op(format_args!(
            "{} {} {} rg",
            num(color.r),
            num(color.g),
            num(color.b)
        ));
        path(out);
        out.op(format_args!("f*"));
        out.op(format_args!("Q"));
    }

    /// PDF has no blur: the shadow is eight rounded rectangles grown and shrunk to the
    /// Gaussian's octiles, drawn from the largest in, so their overlap rises across the
    /// edge as the blurred one does. Each layer is as opaque as it takes for the `k`
    /// layers over a point to add up to `k / 8` of the shadow's alpha.
    fn shadow(
        &mut self,
        out: &mut Content,
        rect: Rect,
        radius: CornerRadii,
        blur: f32,
        color: Color,
    ) {
        const OCTILES: [f32; 8] = [1.534, 0.887, 0.489, 0.157, -0.157, -0.489, -0.887, -1.534];
        if blur <= 0.0 {
            self.quad(out, rect, radius, color, Border::default());
            return;
        }
        let sigma = blur / 2.0;
        let layers = OCTILES.len() as f32;
        for (i, k) in OCTILES.into_iter().enumerate() {
            // Covered so far, and what this layer must bring it to.
            let below = color.a * i as f32 / layers;
            let target = color.a * (i + 1) as f32 / layers;
            let layer = color.with_alpha((target - below) / (1.0 - below));
            let grow = k * sigma;
            let (w, h) = (rect.width + 2.0 * grow, rect.height + 2.0 * grow);
            if w <= 0.0 || h <= 0.0 {
                continue;
            }
            let grown = Rect::new(rect.x - grow, rect.y - grow, w, h);
            let radii = radius.map(|r| if r > 0.0 { (r + grow).max(0.0) } else { 0.0 });
            self.quad(out, grown, radii, layer, Border::default());
        }
    }

    fn shape(&mut self, out: &mut Content, shape: &Shape, color: Color) {
        let bounds = shape.bounds();
        let center = Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
        );
        let (hx, hy) = (bounds.width / 2.0, bounds.height / 2.0);
        match *shape {
            Shape::Line { from, to, width } => self.stroke_with(out, color, width, |out| {
                out.move_to(from);
                out.line_to(to);
            }),
            Shape::Triangle { direction, .. } => {
                let p = |x: f32, y: f32| Point::new(center.x + x, center.y + y);
                let corners = match direction {
                    Direction::Up => [p(0.0, -hy), p(hx, hy), p(-hx, hy)],
                    Direction::Down => [p(0.0, hy), p(-hx, -hy), p(hx, -hy)],
                    Direction::Left => [p(-hx, 0.0), p(hx, -hy), p(hx, hy)],
                    Direction::Right => [p(hx, 0.0), p(-hx, hy), p(-hx, -hy)],
                };
                self.fill_with(out, color, |out| out.polygon(&corners));
            }
            Shape::Star { outline, .. } => {
                let r = hx.min(hy);
                // As in the shader: the star sits a tenth of its radius low.
                let middle = Point::new(center.x, center.y + 0.1 * r);
                let corners: Vec<Point> = (0..10)
                    .map(|i| {
                        let radius = if i % 2 == 0 { r } else { r * 0.45 };
                        let angle = i as f32 * std::f32::consts::PI / 5.0;
                        Point::new(
                            middle.x + radius * angle.sin(),
                            middle.y - radius * angle.cos(),
                        )
                    })
                    .collect();
                if outline > 0.0 {
                    self.stroke_with(out, color, outline, |out| out.polygon(&corners));
                } else {
                    self.fill_with(out, color, |out| out.polygon(&corners));
                }
            }
            Shape::Arc {
                center,
                radius,
                width,
                sweep,
            } => self.stroke_with(out, color, width, |out| {
                out.arc(center, radius, sweep.clamp(0.0, 1.0));
            }),
            Shape::Gradient {
                rect,
                radius,
                end,
                vertical,
            } => {
                let max = (rect.width / 2.0).min(rect.height / 2.0).max(0.0);
                let r = radius.clamp(0.0, max);
                let (from, to) = if vertical {
                    (
                        Point::new(rect.x, rect.y),
                        Point::new(rect.x, rect.bottom()),
                    )
                } else {
                    (Point::new(rect.x, rect.y), Point::new(rect.right(), rect.y))
                };
                let index = self.shadings.len();
                self.shadings.push(Shading {
                    from,
                    to,
                    start: color,
                    end,
                });
                out.op(format_args!("q"));
                // One alpha for the whole gradient: the larger of its ends.
                self.alpha(out, color.a.max(end.a));
                out.rounded_rect(rect, [r; 4]);
                out.op(format_args!("W n /S{index} sh"));
                out.op(format_args!("Q"));
            }
            Shape::Shadow { rect, radius, blur } => self.shadow(out, rect, radius, blur, color),
        }
    }

    fn image(&mut self, out: &mut Content, rect: Rect, id: ImageId) {
        if !self.images.contains_key(&id) || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        if !self.used_images.contains(&id) {
            self.used_images.push(id);
        }
        // The image's unit square, its first row at the top of `rect`.
        out.op(format_args!(
            "q {} 0 0 {} {} {} cm /I{} Do Q",
            num(rect.width),
            num(-rect.height),
            num(rect.x),
            num(rect.bottom()),
            id.0
        ));
    }

    fn icon(&mut self, out: &mut Content, rect: Rect, icon: &VectorIcon, color: Color) {
        let vb = icon.view_box();
        if vb.width <= 0.0 || vb.height <= 0.0 {
            return;
        }
        // Scaled to fit and centered, as VectorIcon::rasterize places it.
        let scale = (rect.width / vb.width).min(rect.height / vb.height);
        let dx = rect.x + (rect.width - vb.width * scale) / 2.0 - vb.x * scale;
        let dy = rect.y + (rect.height - vb.height * scale) / 2.0 - vb.y * scale;
        let map = |p: Point| Point::new(p.x * scale + dx, p.y * scale + dy);
        for layer in icon.layers() {
            let ink = layer
                .color
                .map_or(color, |c| Color::rgba(c.r, c.g, c.b, c.a * color.a));
            let path = |out: &mut Content| {
                let mut current = Point::new(0.0, 0.0);
                for command in &layer.commands {
                    match *command {
                        PathCommand::MoveTo(p) => {
                            current = map(p);
                            out.move_to(current);
                        }
                        PathCommand::LineTo(p) => {
                            current = map(p);
                            out.line_to(current);
                        }
                        PathCommand::QuadTo(c, p) => {
                            let (c, p) = (map(c), map(p));
                            out.quad_to(current, c, p);
                            current = p;
                        }
                        PathCommand::CubicTo(a, b, p) => {
                            current = map(p);
                            out.curve_to(map(a), map(b), current);
                        }
                        PathCommand::Close => out.op(format_args!("h")),
                    }
                }
            };
            match layer.paint {
                IconPaint::Fill(rule) => {
                    if ink.a <= 0.0 {
                        continue;
                    }
                    out.op(format_args!("q"));
                    self.alpha(out, ink.a);
                    out.op(format_args!(
                        "{} {} {} rg",
                        num(ink.r),
                        num(ink.g),
                        num(ink.b)
                    ));
                    path(out);
                    let even_odd = rule == noa_gpui_scene::FillRule::EvenOdd;
                    out.op(format_args!("{}", if even_odd { "f*" } else { "f" }));
                    out.op(format_args!("Q"));
                }
                IconPaint::Stroke(width) => self.stroke_with(out, ink, width * scale, path),
            }
        }
    }

    fn text(
        &mut self,
        out: &mut Content,
        store: &mut TextStore,
        rect: Rect,
        layout: noa_gpui_scene::TextLayoutId,
        color: Color,
    ) {
        let s = TEXT_SCALE;
        let (font_system, layouts) = store.split_mut();
        let buffer = layouts.buffer(layout);
        out.op(format_args!("q"));
        // Glyph space: TEXT_SCALE times larger, y down like the page.
        out.op(format_args!("{} 0 0 {} 0 0 cm", num(1.0 / s), num(1.0 / s)));
        let mut ink_now: Option<(u32, u32, u32, u16)> = None;
        for run in buffer.layout_runs() {
            let line_y = run.line_y * s;
            for glyph in run.glyphs {
                let physical = glyph.physical((rect.x * s, rect.y * s), s);
                let ink = match glyph.color_opt {
                    Some(c) => {
                        let [r, g, b, a] = c.as_rgba();
                        Color::rgba(
                            f32::from(r) / 255.0,
                            f32::from(g) / 255.0,
                            f32::from(b) / 255.0,
                            f32::from(a) / 255.0 * color.a,
                        )
                    }
                    None => color,
                };
                if ink.a <= 0.0 {
                    continue;
                }
                let Some(commands) = self
                    .swash
                    .get_outline_commands(font_system, physical.cache_key)
                else {
                    continue;
                };
                if commands.is_empty() {
                    continue;
                }
                let key = (
                    (ink.r * 1000.0) as u32,
                    (ink.g * 1000.0) as u32,
                    (ink.b * 1000.0) as u32,
                    (ink.a * 1000.0) as u16,
                );
                if ink_now != Some(key) {
                    ink_now = Some(key);
                    let alpha = (ink.a.clamp(0.0, 1.0) * 1000.0).round() as u16;
                    if alpha < 1000 {
                        self.alphas.insert(alpha, ());
                        out.op(format_args!("/A{alpha} gs"));
                    }
                    out.op(format_args!(
                        "{} {} {} rg",
                        num(ink.r),
                        num(ink.g),
                        num(ink.b)
                    ));
                }
                // Outlines are y up from the pen position on the baseline.
                let (ox, oy) = (physical.x as f32, line_y + physical.y as f32);
                let at = |x: f32, y: f32| Point::new(ox + x, oy - y);
                let mut current = Point::new(ox, oy);
                for command in commands {
                    match *command {
                        Command::MoveTo(p) => {
                            current = at(p.x, p.y);
                            out.move_to(current);
                        }
                        Command::LineTo(p) => {
                            current = at(p.x, p.y);
                            out.line_to(current);
                        }
                        Command::CurveTo(a, b, p) => {
                            current = at(p.x, p.y);
                            out.curve_to(at(a.x, a.y), at(b.x, b.y), current);
                        }
                        Command::QuadTo(c, p) => {
                            let (c, p) = (at(c.x, c.y), at(p.x, p.y));
                            out.quad_to(current, c, p);
                            current = p;
                        }
                        Command::Close => out.op(format_args!("h")),
                    }
                }
                out.op(format_args!("f"));
            }
        }
        out.op(format_args!("Q"));
    }
}

/// A page's content stream.
#[derive(Default)]
struct Content(String);

impl Content {
    fn op(&mut self, args: std::fmt::Arguments<'_>) {
        let _ = self.0.write_fmt(args);
        self.0.push('\n');
    }

    fn move_to(&mut self, p: Point) {
        self.op(format_args!("{} {} m", num(p.x), num(p.y)));
    }

    fn line_to(&mut self, p: Point) {
        self.op(format_args!("{} {} l", num(p.x), num(p.y)));
    }

    fn curve_to(&mut self, a: Point, b: Point, p: Point) {
        self.op(format_args!(
            "{} {} {} {} {} {} c",
            num(a.x),
            num(a.y),
            num(b.x),
            num(b.y),
            num(p.x),
            num(p.y)
        ));
    }

    /// A quadratic curve from `from`, as the cubic PDF draws.
    fn quad_to(&mut self, from: Point, c: Point, p: Point) {
        let a = Point::new(
            from.x + 2.0 / 3.0 * (c.x - from.x),
            from.y + 2.0 / 3.0 * (c.y - from.y),
        );
        let b = Point::new(p.x + 2.0 / 3.0 * (c.x - p.x), p.y + 2.0 / 3.0 * (c.y - p.y));
        self.curve_to(a, b, p);
    }

    fn rect(&mut self, r: Rect) {
        self.op(format_args!(
            "{} {} {} {} re",
            num(r.x),
            num(r.y),
            num(r.width),
            num(r.height)
        ));
    }

    fn polygon(&mut self, points: &[Point]) {
        let Some((first, rest)) = points.split_first() else {
            return;
        };
        self.move_to(*first);
        for p in rest {
            self.line_to(*p);
        }
        self.op(format_args!("h"));
    }

    /// Corners top left, top right, bottom right, bottom left.
    fn rounded_rect(&mut self, r: Rect, radii: CornerRadii) {
        if radii.iter().all(|&radius| radius <= 0.0) {
            self.rect(r);
            return;
        }
        let [tl, tr, br, bl] = radii;
        let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
        let k = KAPPA;
        self.move_to(Point::new(x0 + tl, y0));
        self.line_to(Point::new(x1 - tr, y0));
        self.curve_to(
            Point::new(x1 - tr + tr * k, y0),
            Point::new(x1, y0 + tr - tr * k),
            Point::new(x1, y0 + tr),
        );
        self.line_to(Point::new(x1, y1 - br));
        self.curve_to(
            Point::new(x1, y1 - br + br * k),
            Point::new(x1 - br + br * k, y1),
            Point::new(x1 - br, y1),
        );
        self.line_to(Point::new(x0 + bl, y1));
        self.curve_to(
            Point::new(x0 + bl - bl * k, y1),
            Point::new(x0, y1 - bl + bl * k),
            Point::new(x0, y1 - bl),
        );
        self.line_to(Point::new(x0, y0 + tl));
        self.curve_to(
            Point::new(x0, y0 + tl - tl * k),
            Point::new(x0 + tl - tl * k, y0),
            Point::new(x0 + tl, y0),
        );
        self.op(format_args!("h"));
    }

    /// From the top of the circle, clockwise (y down) through `sweep` of a turn.
    fn arc(&mut self, center: Point, radius: f32, sweep: f32) {
        let total = sweep * std::f32::consts::TAU;
        let at = |angle: f32| {
            Point::new(
                center.x + radius * angle.sin(),
                center.y - radius * angle.cos(),
            )
        };
        let tangent = |angle: f32| Point::new(angle.cos(), angle.sin());
        let pieces = (total / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as usize;
        let step = total / pieces as f32;
        let k = 4.0 / 3.0 * (step / 4.0).tan() * radius;
        self.move_to(at(0.0));
        for i in 0..pieces {
            let (a0, a1) = (i as f32 * step, (i + 1) as f32 * step);
            let (p0, p1) = (at(a0), at(a1));
            let (t0, t1) = (tangent(a0), tangent(a1));
            self.curve_to(
                Point::new(p0.x + k * t0.x, p0.y + k * t0.y),
                Point::new(p1.x - k * t1.x, p1.y - k * t1.y),
                p1,
            );
        }
    }
}

/// `rect` and its corner radii moved `by` inward on every side.
fn inset(rect: Rect, radii: CornerRadii, by: f32) -> (Rect, CornerRadii) {
    let inner = Rect::new(
        rect.x + by,
        rect.y + by,
        (rect.width - 2.0 * by).max(0.0),
        (rect.height - 2.0 * by).max(0.0),
    );
    (inner, radii.map(|r| (r - by).max(0.0)))
}

/// A number as PDF writes it: at most three decimals, no trailing zeros.
fn num(value: f32) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let text = format!("{:.3}", value);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    match text {
        "-0" | "" => "0".to_owned(),
        t => t.to_owned(),
    }
}

/// The file: catalog, page tree, pages with their content, then the shared resources
/// (graphics states, shadings, images) and the cross-reference table.
fn write_file(doc: &PdfDocument) -> Vec<u8> {
    // Object numbers: 1 catalog, 2 pages, 3 resources, then two per page (page and
    // content), then one per shading, then two per image (image and its alpha).
    let pages_start = 4;
    let shadings_start = pages_start + 2 * doc.pages.len();
    let images_start = shadings_start + doc.shadings.len();
    let used: Vec<(ImageId, &RgbaImage)> = doc
        .used_images
        .iter()
        .filter_map(|id| doc.images.get(id).map(|image| (*id, image)))
        .collect();
    let count = images_start - 1 + 2 * used.len();

    let mut objects: Vec<Vec<u8>> = Vec::with_capacity(count);
    objects.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids: Vec<String> = (0..doc.pages.len())
        .map(|i| format!("{} 0 R", pages_start + 2 * i))
        .collect();
    objects.push(
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            kids.join(" "),
            doc.pages.len()
        )
        .into_bytes(),
    );
    let mut resources = String::from("<< ");
    if !doc.alphas.is_empty() {
        resources.push_str("/ExtGState << ");
        for alpha in doc.alphas.keys() {
            let a = num(f32::from(*alpha) / 1000.0);
            let _ = write!(resources, "/A{alpha} << /ca {a} /CA {a} >> ");
        }
        resources.push_str(">> ");
    }
    if !doc.shadings.is_empty() {
        resources.push_str("/Shading << ");
        for i in 0..doc.shadings.len() {
            let _ = write!(resources, "/S{i} {} 0 R ", shadings_start + i);
        }
        resources.push_str(">> ");
    }
    if !used.is_empty() {
        resources.push_str("/XObject << ");
        for (i, (id, _)) in used.iter().enumerate() {
            let _ = write!(resources, "/I{} {} 0 R ", id.0, images_start + 2 * i);
        }
        resources.push_str(">> ");
    }
    resources.push_str(">>");
    objects.push(resources.into_bytes());
    for (i, page) in doc.pages.iter().enumerate() {
        objects.push(
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources 3 0 R \
                 /Contents {} 0 R >>",
                num(page.size.width),
                num(page.size.height),
                pages_start + 2 * i + 1
            )
            .into_bytes(),
        );
        objects.push(stream("", page.content.as_bytes()));
    }
    for shading in &doc.shadings {
        let (c0, c1) = (shading.start, shading.end);
        objects.push(
            format!(
                "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [{} {} {} {}] \
                 /Function << /FunctionType 2 /Domain [0 1] /C0 [{} {} {}] /C1 [{} {} {}] \
                 /N 1 >> /Extend [true true] >>",
                num(shading.from.x),
                num(shading.from.y),
                num(shading.to.x),
                num(shading.to.y),
                num(c0.r),
                num(c0.g),
                num(c0.b),
                num(c1.r),
                num(c1.g),
                num(c1.b)
            )
            .into_bytes(),
        );
    }
    for (i, (_, image)) in used.iter().enumerate() {
        let mut rgb = Vec::with_capacity(image.pixels.len() / 4 * 3);
        let mut alpha = Vec::with_capacity(image.pixels.len() / 4);
        for px in image.pixels.as_chunks::<4>().0 {
            rgb.extend_from_slice(&px[..3]);
            alpha.push(px[3]);
        }
        let mask = images_start + 2 * i + 1;
        objects.push(stream(
            &format!(
                "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB \
                 /BitsPerComponent 8 /Filter /FlateDecode /SMask {mask} 0 R",
                image.width, image.height
            ),
            &crate::png::zlib_stored(&rgb),
        ));
        objects.push(stream(
            &format!(
                "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray \
                 /BitsPerComponent 8 /Filter /FlateDecode",
                image.width, image.height
            ),
            &crate::png::zlib_stored(&alpha),
        ));
    }
    debug_assert_eq!(objects.len(), count);

    let mut file = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (i, body) in objects.iter().enumerate() {
        offsets.push(file.len());
        file.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        file.extend_from_slice(body);
        file.extend_from_slice(b"\nendobj\n");
    }
    let xref = file.len();
    let mut table = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = writeln!(table, "{offset:010} 00000 n ");
    }
    let _ = write!(
        table,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    file.extend_from_slice(table.as_bytes());
    file
}

fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let mut out = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use noa_gpui_scene::BoxShadow;
    use noa_gpui_text::{FontOptions, TextStyle};

    /// The objects of a file this module wrote, checking the cross-reference table
    /// points at each.
    fn objects(file: &[u8]) -> Vec<String> {
        let find = |from: usize, needle: &[u8]| {
            file[from..]
                .windows(needle.len())
                .position(|w| w == needle)
                .map(|at| from + at)
        };
        let text =
            |range: std::ops::Range<usize>| String::from_utf8_lossy(&file[range]).into_owned();
        assert!(file.starts_with(b"%PDF-1.4\n"));
        assert!(file.ends_with(b"%%EOF\n"));
        let marker = file.windows(10).rposition(|w| w == b"startxref\n").unwrap();
        let start: usize = text(marker + 10..file.len())
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let table = text(start..file.len());
        assert!(table.starts_with("xref\n"));
        let mut lines = table.lines().skip(1);
        let count: usize = lines
            .next()
            .unwrap()
            .split(' ')
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        lines.next();
        (1..count)
            .map(|n| {
                let offset: usize = lines.next().unwrap()[..10].parse().unwrap();
                let head = format!("{n} 0 obj\n");
                assert!(file[offset..].starts_with(head.as_bytes()), "object {n}");
                let end = find(offset, b"\nendobj\n").unwrap();
                text(offset..end)
            })
            .collect()
    }

    #[test]
    fn a_shadow_is_drawn_as_eight_faint_rounded_rectangles() {
        let mut list = DisplayList::new();
        let shadow = BoxShadow {
            color: Color::rgba(0.0, 0.0, 0.0, 0.4),
            blur: 8.0,
            ..BoxShadow::default()
        };
        list.box_shadow(Rect::new(20.0, 20.0, 40.0, 40.0), [6.0; 4], &shadow);
        let mut text = TextStore::new(FontOptions::default());
        let mut pdf = PdfDocument::new();
        pdf.add_page(&list, &mut text, Size::new(100.0, 100.0), None);
        let objects = objects(&pdf.finish());
        let content = objects.iter().find(|o| o.contains("0 0 0 rg")).unwrap();
        // Drawn over one another, the eight layers add up to the shadow's alpha
        // where they all overlap.
        let alphas: Vec<f32> = content
            .split("/A")
            .skip(1)
            .filter_map(|rest| rest.split_once(" gs").map(|(key, _)| key))
            .filter_map(|key| key.parse::<f32>().ok())
            .map(|key| key / 1000.0)
            .collect();
        assert_eq!(alphas.len(), 8, "{content}");
        let covered = 1.0 - alphas.iter().map(|a| 1.0 - a).product::<f32>();
        assert!((covered - 0.4).abs() < 0.005, "{covered}");
        // The outermost reaches 1.534 sigma (6.1 points) past the box.
        assert!(content.contains("13.864 "), "{content}");
    }

    #[test]
    fn numbers_are_short_and_never_negative_zero() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(0.5), "0.5");
        assert_eq!(num(-0.0001), "0");
        assert_eq!(num(12.34567), "12.346");
        assert_eq!(num(f32::NAN), "0");
    }

    #[test]
    fn a_page_holds_its_shapes_with_alpha_and_borders_as_vector_paths() {
        let mut list = DisplayList::new();
        list.push(DrawCommand::Quad {
            rect: Rect::new(10.0, 20.0, 100.0, 40.0),
            radius: [6.0; 4],
            color: Color::rgba(0.2, 0.4, 1.0, 0.5),
            border: Border {
                width: 2.0,
                color: Color::rgba(0.0, 0.0, 0.0, 1.0),
            },
        });
        list.push_clip(Rect::new(0.0, 0.0, 50.0, 50.0));
        list.push(DrawCommand::Shape {
            shape: Shape::Line {
                from: Point::new(0.0, 0.0),
                to: Point::new(40.0, 40.0),
                width: 3.0,
            },
            color: Color::rgba(1.0, 0.0, 0.0, 1.0),
        });
        list.pop_clip();
        let mut text = TextStore::new(FontOptions::default());
        let mut pdf = PdfDocument::new();
        pdf.add_page(
            &list,
            &mut text,
            Size::new(200.0, 100.0),
            Some(Color::rgba(1.0, 1.0, 1.0, 1.0)),
        );
        pdf.add_page(
            &DisplayList::new(),
            &mut text,
            Size::new(300.0, 400.0),
            None,
        );
        let objects = objects(&pdf.finish());
        assert_eq!(objects.len(), 3 + 2 * 2);
        assert!(objects[1].contains("/Count 2"));
        assert!(objects[2].contains("/A500 << /ca 0.5 /CA 0.5 >>"));
        assert!(objects[3].contains("/MediaBox [0 0 200 100]"));
        assert!(objects[5].contains("/MediaBox [0 0 300 400]"));
        let content = &objects[4];
        assert!(content.contains("1 0 0 -1 0 100 cm"), "y down");
        assert!(content.contains("0 0 200 100 re"), "background");
        // The fill stops inside the 2-point border; the border is a ring around it.
        assert!(
            content.contains("/A500 gs\n0.2 0.4 1 rg\n16 22 m"),
            "{content}"
        );
        let ring = content
            .find("0 0 0 rg\n16 20 m")
            .expect("the ring's outer edge");
        let ring = &content[ring..];
        assert!(
            ring.contains("h\n16 22 m"),
            "the inner edge follows: {ring}"
        );
        assert!(ring.contains("f*"), "even-odd: {ring}");
        assert!(content.contains("q\n0 0 50 50 re\nW n"), "clip");
        assert!(content.contains("1 0 0 RG 3 w"));
        let lines: Vec<&str> = content.lines().collect();
        let opened = lines.iter().filter(|l| **l == "q").count();
        let closed = lines.iter().filter(|l| **l == "Q").count();
        assert!(opened > 0);
        assert_eq!(opened, closed, "balanced q / Q");
    }

    #[test]
    fn text_becomes_glyph_outlines_and_images_are_embedded_with_their_alpha() {
        let mut text = TextStore::new(FontOptions::default());
        let layout = text.create("Hi", TextStyle::new(16.0), None);
        let size = text.layouts().size(layout);
        let mut list = DisplayList::new();
        list.push(DrawCommand::Text {
            rect: Rect::new(10.0, 10.0, size.width, size.height),
            layout,
            color: Color::rgba(0.1, 0.1, 0.1, 1.0),
        });
        list.push(DrawCommand::Image {
            rect: Rect::new(50.0, 10.0, 20.0, 10.0),
            image: ImageId(7),
        });
        list.push(DrawCommand::Image {
            rect: Rect::new(80.0, 10.0, 20.0, 10.0),
            image: ImageId(8),
        });
        let mut pdf = PdfDocument::new();
        pdf.set_image(
            ImageId(7),
            RgbaImage {
                width: 2,
                height: 1,
                pixels: vec![255, 0, 0, 255, 0, 0, 255, 128],
            },
        );
        pdf.add_page(&list, &mut text, Size::new(120.0, 40.0), None);
        let objects = objects(&pdf.finish());
        let content = &objects[4];
        assert!(content.contains("0.25 0 0 0.25 0 0 cm"), "{content}");
        // Two glyphs, each a filled outline with curves.
        assert!(content.matches("\nf\n").count() >= 2, "{content}");
        assert!(content.contains(" c\n"));
        assert!(content.contains("q 20 0 0 -10 50 20 cm /I7 Do Q"));
        assert!(!content.contains("/I8"), "an image never given is skipped");
        // Objects 6 and 7: the image and its alpha.
        assert!(objects[2].contains("/XObject << /I7 6 0 R >>"));
        let image = &objects[5];
        assert!(image.contains("/Width 2 /Height 1"), "{image}");
        assert!(image.contains("/SMask 7 0 R"), "{image}");
        assert!(objects[6].contains("/DeviceGray"));
        assert_eq!(objects.len(), 7);
    }

    #[test]
    fn gradients_arcs_stars_and_triangles_become_shadings_and_paths() {
        let mut list = DisplayList::new();
        let blue = Color::rgba(0.0, 0.0, 1.0, 1.0);
        list.push(DrawCommand::Shape {
            shape: Shape::Gradient {
                rect: Rect::new(0.0, 0.0, 100.0, 20.0),
                radius: 4.0,
                end: Color::rgba(1.0, 0.0, 0.0, 1.0),
                vertical: false,
            },
            color: blue,
        });
        list.push(DrawCommand::Shape {
            shape: Shape::Arc {
                center: Point::new(50.0, 50.0),
                radius: 10.0,
                width: 2.0,
                sweep: 0.75,
            },
            color: blue,
        });
        list.push(DrawCommand::Shape {
            shape: Shape::Star {
                rect: Rect::new(0.0, 0.0, 20.0, 20.0),
                outline: 0.0,
            },
            color: blue,
        });
        list.push(DrawCommand::Shape {
            shape: Shape::Triangle {
                rect: Rect::new(0.0, 0.0, 10.0, 10.0),
                direction: Direction::Down,
            },
            color: blue,
        });
        let mut text = TextStore::new(FontOptions::default());
        let mut pdf = PdfDocument::new();
        pdf.add_page(&list, &mut text, Size::new(100.0, 100.0), None);
        let objects = objects(&pdf.finish());
        // The gradient: clipped to its rounded rectangle, an axial shading from the
        // left edge to the right one, from the color to the end color.
        assert!(
            objects[2].contains("/Shading << /S0 6 0 R >>"),
            "{}",
            objects[2]
        );
        assert!(objects[5].contains("/ShadingType 2"));
        assert!(objects[5].contains("/Coords [0 0 100 0]"));
        assert!(objects[5].contains("/C0 [0 0 1] /C1 [1 0 0]"));
        let content = &objects[4];
        assert!(content.contains("W n /S0 sh"), "{content}");
        // Three quarters of a circle from the top: three Bézier quarters, ending on
        // the left (y down, clockwise).
        let arc = &content[content.find("50 40 m").expect("starts at the top")..];
        let pieces: Vec<&str> = arc.lines().skip(1).take(3).collect();
        assert!(pieces.iter().all(|l| l.ends_with(" c")), "{pieces:?}");
        assert!(pieces[2].ends_with(" 40 50 c"), "{pieces:?}");
        assert!(arc.lines().nth(4) == Some("S"), "stroked");
        // The star: ten corners, the first at the top (a tenth of the radius low).
        let star = &content[content.find("10 1 m").expect("the star's top")..];
        assert_eq!(star.lines().take_while(|l| l.ends_with(" l")).count(), 0);
        assert_eq!(
            star.lines()
                .skip(1)
                .take_while(|l| l.ends_with(" l"))
                .count(),
            9
        );
        // The triangle pointing down: its tip at the bottom middle.
        assert!(content.contains("5 10 m\n0 0 l\n10 0 l\nh"), "{content}");
    }
}
