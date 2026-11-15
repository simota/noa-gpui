//! For snapshots where there is no GPU adapter (a sandbox, a CI runner without one).
//!
//! It follows the GPU pipelines: the same distance fields for quads and shapes with a
//! one-pixel antialiasing ramp, blending in linear light as into an sRGB target, the
//! same glyph placement as glyphon and the same icon masks. Small differences remain
//! (glyph rasterization details, the backdrop blur at full rather than half size), so
//! compare its images with each other rather than with the GPU's.

use std::collections::HashMap;

use noa_gpui_scene::{
    Border, Color, CornerRadii, Direction, DisplayList, DrawCommand, ImageId, Point, Rect, Shape,
    VectorIcon,
};
use noa_gpui_text::TextStore;
use noa_gpui_text::cosmic_text::{SwashCache, SwashContent};

use crate::offscreen::RgbaImage;
use crate::renderer::Viewport;

/// A linear-light color with straight alpha.
type Linear = [f32; 4];

pub struct SoftwareRenderer {
    swash: SwashCache,
    images: HashMap<ImageId, RgbaImage>,
    next_image: u32,
    /// Keyed by (icon id, width, height) in pixels.
    icons: HashMap<(u64, u32, u32), Vec<u8>>,
}

impl Default for SoftwareRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SoftwareRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoftwareRenderer")
            .field("images", &self.images.len())
            .finish_non_exhaustive()
    }
}

impl SoftwareRenderer {
    pub fn new() -> Self {
        Self {
            swash: SwashCache::new(),
            images: HashMap::new(),
            next_image: 1 << 31,
            icons: HashMap::new(),
        }
    }

    /// Keep an image (RGBA8 in sRGB, straight alpha, rows top to bottom) for
    /// [`DrawCommand::Image`]s with the returned id.
    pub fn create_image(&mut self, width: u32, height: u32, rgba: &[u8]) -> ImageId {
        let id = ImageId(self.next_image);
        self.next_image += 1;
        self.images.insert(
            id,
            RgbaImage {
                width,
                height,
                pixels: rgba.to_vec(),
            },
        );
        id
    }

    /// Images not made with [`SoftwareRenderer::create_image`] are left out.
    pub fn render(
        &mut self,
        list: &DisplayList,
        text: &mut TextStore,
        viewport: Viewport,
        background: Color,
    ) -> RgbaImage {
        let (width, height) = (viewport.width.max(1), viewport.height.max(1));
        let mut canvas = Canvas {
            width,
            height,
            scale: viewport.scale_factor,
            pixels: vec![linear(background); (width * height) as usize],
            clip: vec![PixelRect {
                x0: 0,
                y0: 0,
                x1: width,
                y1: height,
            }],
        };
        for command in list.commands() {
            match command {
                DrawCommand::Quad {
                    rect,
                    radius,
                    color,
                    border,
                } => canvas.quad(*rect, *radius, *color, *border),
                DrawCommand::Shape { shape, color } => canvas.shape(shape, *color),
                DrawCommand::Image { rect, image } => {
                    if let Some(image) = self.images.get(image) {
                        canvas.image(*rect, image);
                    }
                }
                DrawCommand::Icon { rect, icon, color } => {
                    self.icon(&mut canvas, *rect, icon, *color);
                }
                DrawCommand::Backdrop {
                    rect,
                    radius,
                    blur,
                    tint,
                } => canvas.backdrop(*rect, *radius, *blur, *tint),
                DrawCommand::Text {
                    rect,
                    layout,
                    color,
                } => self.text(&mut canvas, text, *rect, *layout, *color),
                DrawCommand::PushClip { rect } => canvas.push_clip(*rect),
                DrawCommand::PopClip => {
                    if canvas.clip.len() > 1 {
                        canvas.clip.pop();
                    }
                }
            }
        }
        let mut pixels = Vec::with_capacity(canvas.pixels.len() * 4);
        // Most pixels have the color of the one before them (a background, a fill):
        // encode each run of one color once.
        let mut last: Option<([u32; 4], [u8; 4])> = None;
        for p in &canvas.pixels {
            let bits = p.map(f32::to_bits);
            let bytes = match last {
                Some((same, bytes)) if same == bits => bytes,
                _ => [
                    encode(p[0]),
                    encode(p[1]),
                    encode(p[2]),
                    (p[3].clamp(0.0, 1.0) * 255.0).round() as u8,
                ],
            };
            last = Some((bits, bytes));
            pixels.extend(bytes);
        }
        RgbaImage {
            width,
            height,
            pixels,
        }
    }

    fn icon(&mut self, canvas: &mut Canvas, rect: Rect, icon: &VectorIcon, color: Color) {
        // Whole physical pixels, one mask texel per pixel, as on the GPU.
        let s = canvas.scale;
        let x0 = (rect.x * s).round();
        let y0 = (rect.y * s).round();
        let w = (rect.width * s).round().max(1.0) as u32;
        let h = (rect.height * s).round().max(1.0) as u32;
        let mask = self
            .icons
            .entry((icon.id(), w, h))
            .or_insert_with(|| icon.rasterize(w, h));
        let ink = linear(color);
        let clip = canvas.clip();
        for y in 0..h {
            for x in 0..w {
                let (px, py) = (x0 as i64 + x as i64, y0 as i64 + y as i64);
                if !clip.contains(px, py) {
                    continue;
                }
                let coverage = f32::from(mask[(y * w + x) as usize]) / 255.0;
                canvas.blend(px as u32, py as u32, ink, coverage);
            }
        }
    }

    fn text(
        &mut self,
        canvas: &mut Canvas,
        store: &mut TextStore,
        rect: Rect,
        layout: noa_gpui_scene::TextLayoutId,
        color: Color,
    ) {
        let s = canvas.scale;
        let (left, top) = (rect.x * s, rect.y * s);
        let (font_system, layouts) = store.split_mut();
        let buffer = layouts.buffer(layout);
        let clip = canvas.clip();
        for run in buffer.layout_runs() {
            let line_y = (run.line_y * s).round() as i64;
            for glyph in run.glyphs {
                let physical = glyph.physical((left, top), s);
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
                let ink = linear(ink);
                let Some(image) = self.swash.get_image(font_system, physical.cache_key) else {
                    continue;
                };
                let p = image.placement;
                let x0 = i64::from(physical.x) + i64::from(p.left);
                let y0 = line_y + i64::from(physical.y) - i64::from(p.top);
                for y in 0..p.height {
                    for x in 0..p.width {
                        let (px, py) = (x0 + i64::from(x), y0 + i64::from(y));
                        if !clip.contains(px, py) {
                            continue;
                        }
                        let i = (y * p.width + x) as usize;
                        let (pixel, coverage) = match image.content {
                            SwashContent::Mask => (ink, f32::from(image.data[i]) / 255.0),
                            SwashContent::SubpixelMask => {
                                let d = &image.data[i * 4..i * 4 + 3];
                                let c = (u16::from(d[0]) + u16::from(d[1]) + u16::from(d[2])) / 3;
                                (ink, f32::from(c) / 255.0)
                            }
                            SwashContent::Color => {
                                let d = &image.data[i * 4..i * 4 + 4];
                                let c = Color::rgba(
                                    f32::from(d[0]) / 255.0,
                                    f32::from(d[1]) / 255.0,
                                    f32::from(d[2]) / 255.0,
                                    f32::from(d[3]) / 255.0 * color.a,
                                );
                                (linear(c), 1.0)
                            }
                        };
                        canvas.blend(px as u32, py as u32, pixel, coverage);
                    }
                }
            }
        }
    }
}

/// A clip rectangle in whole physical pixels (`x1`, `y1` exclusive).
#[derive(Debug, Clone, Copy)]
struct PixelRect {
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
}

impl PixelRect {
    fn contains(&self, x: i64, y: i64) -> bool {
        x >= i64::from(self.x0)
            && x < i64::from(self.x1)
            && y >= i64::from(self.y0)
            && y < i64::from(self.y1)
    }
}

struct Canvas {
    width: u32,
    height: u32,
    scale: f32,
    pixels: Vec<Linear>,
    clip: Vec<PixelRect>,
}

impl Canvas {
    fn clip(&self) -> PixelRect {
        *self.clip.last().expect("the whole canvas")
    }

    /// Like the GPU's scissor: rounded outward to whole pixels inside the target.
    fn push_clip(&mut self, rect: Rect) {
        let s = self.scale;
        let c = self.clip();
        let x0 = ((rect.x * s).floor().max(0.0) as u32).max(c.x0);
        let y0 = ((rect.y * s).floor().max(0.0) as u32).max(c.y0);
        let x1 = ((rect.right() * s).ceil().max(0.0) as u32).min(c.x1);
        let y1 = ((rect.bottom() * s).ceil().max(0.0) as u32).min(c.y1);
        self.clip.push(PixelRect {
            x0,
            y0,
            x1: x1.max(x0),
            y1: y1.max(y0),
        });
    }

    /// The physical pixels a logical rectangle (padded by one pixel for its
    /// antialiased edge) touches inside the clip.
    fn pixels_of(&self, rect: Rect) -> PixelRect {
        let s = self.scale;
        let c = self.clip();
        let x0 = ((rect.x * s).floor() - 1.0).max(c.x0 as f32) as u32;
        let y0 = ((rect.y * s).floor() - 1.0).max(c.y0 as f32) as u32;
        let x1 = ((rect.right() * s).ceil() + 1.0).min(c.x1 as f32).max(0.0) as u32;
        let y1 = ((rect.bottom() * s).ceil() + 1.0).min(c.y1 as f32).max(0.0) as u32;
        PixelRect {
            x0,
            y0,
            x1: x1.max(x0),
            y1: y1.max(y0),
        }
    }

    /// Blend `color` over the pixel with `coverage` (ALPHA_BLENDING in linear light).
    fn blend(&mut self, x: u32, y: u32, color: Linear, coverage: f32) {
        let a = color[3] * coverage.clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let p = &mut self.pixels[(y * self.width + x) as usize];
        for i in 0..3 {
            p[i] = color[i] * a + p[i] * (1.0 - a);
        }
        p[3] = a + p[3] * (1.0 - a);
    }

    /// Shade every pixel of `bounds` with `shade(local point from the center, half
    /// size)`, which gives a color and the signed distance in logical pixels.
    ///
    /// `inside`, if given, is `(half size, color)` of a box around the center where
    /// `shade` gives that color, a pixel or more inside the edge (so the pixel is fully
    /// covered): those pixels are blended in that color without calling `shade`.
    fn fill(
        &mut self,
        bounds: Rect,
        inside: Option<(Point, Linear)>,
        mut shade: impl FnMut(Point, Point) -> (Linear, f32),
    ) {
        let area = self.pixels_of(bounds);
        let s = self.scale;
        let half = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let center = Point::new(bounds.x + half.x, bounds.y + half.y);
        let local_x = |x: u32| (x as f32 + 0.5) / s - center.x;
        // The columns whose pixels are inside on the rows that are: `local_x` grows
        // with `x`, so they are one run, the same on every row, where
        // `local_x(x).abs() <= inner.x`.
        let columns = match inside {
            Some((inner, _)) => {
                let start = first_not(area.x0..area.x1, |x| local_x(x) < -inner.x);
                let end = first_not(area.x0..area.x1, |x| local_x(x) <= inner.x);
                start..end.max(start)
            }
            None => 0..0,
        };
        for y in area.y0..area.y1 {
            let py = (y as f32 + 0.5) / s - center.y;
            let mut shade_pixel = |canvas: &mut Self, x: u32| {
                let (color, d) = shade(Point::new(local_x(x), py), half);
                let coverage = (0.5 - d * s).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    canvas.blend(x, y, color, coverage);
                }
            };
            match inside {
                Some((inner, color)) if py.abs() <= inner.y => {
                    for x in area.x0..columns.start {
                        shade_pixel(self, x);
                    }
                    for x in columns.clone() {
                        self.blend(x, y, color, 1.0);
                    }
                    for x in columns.end..area.x1 {
                        shade_pixel(self, x);
                    }
                }
                _ => {
                    for x in area.x0..area.x1 {
                        shade_pixel(self, x);
                    }
                }
            }
        }
    }

    fn quad(&mut self, rect: Rect, radii: CornerRadii, color: Color, border: Border) {
        let max = (rect.width / 2.0).min(rect.height / 2.0).max(0.0);
        let radii = radii.map(|r| r.clamp(0.0, max));
        let (fill, edge) = (linear(color), linear(border.color));
        let s = self.scale;
        // Away from the corners and the border by more than a pixel, the distance
        // below is at most minus that much: every pixel there is covered, in the color
        // the border's inner edge gives (`mix` at 1, worked out once). The margin
        // grows with the coordinates, beyond what rounding them can move the distance.
        // A radius that is not a number covers nothing: then no pixel is inside.
        let corner = radii
            .into_iter()
            .fold(0.0, |m: f32, r| if r.is_nan() || r > m { r } else { m });
        let magnitude = rect.x.abs() + rect.y.abs() + rect.width.abs() + rect.height.abs();
        let reach = corner + border.width.max(0.0) + 1.0 + 1.0 / s + magnitude * 1e-5;
        let inner = Point::new(rect.width / 2.0 - reach, rect.height / 2.0 - reach);
        let inside_color = if border.width > 0.0 {
            mix(edge, fill, 1.0)
        } else {
            fill
        };
        self.fill(rect, Some((inner, inside_color)), |p, half| {
            let d = rounded_box(p, half, radii);
            if border.width > 0.0 {
                let inner = (0.5 - (d + border.width) * s).clamp(0.0, 1.0);
                (mix(edge, fill, inner), d)
            } else {
                (fill, d)
            }
        });
    }

    fn shape(&mut self, shape: &Shape, color: Color) {
        let bounds = shape.bounds();
        let ink = linear(color);
        let center = Point::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
        );
        match *shape {
            Shape::Line { from, to, width } => {
                let a = Point::new(from.x - center.x, from.y - center.y);
                let b = Point::new(to.x - center.x, to.y - center.y);
                self.fill(bounds, None, |p, _| (ink, segment(p, a, b) - width / 2.0));
            }
            Shape::Triangle { direction, .. } => {
                self.fill(bounds, None, |p, h| {
                    (ink, pointing_triangle(p, h, direction))
                });
            }
            Shape::Star { outline, .. } => {
                let half = outline / 2.0;
                self.fill(bounds, None, |p, h| {
                    let r = h.x.min(h.y);
                    let mut d = star(Point::new(p.x, p.y - 0.1 * r), r, 0.45);
                    if half > 0.0 {
                        d = (d + half).abs() - half;
                    }
                    (ink, d)
                });
            }
            Shape::Arc {
                radius,
                width,
                sweep,
                ..
            } => {
                self.fill(bounds, None, |p, _| {
                    (ink, arc(p, radius, sweep) - width / 2.0)
                });
            }
            Shape::Gradient {
                radius,
                end,
                vertical,
                ..
            } => {
                let max = (bounds.width / 2.0).min(bounds.height / 2.0).max(0.0);
                let radii = [radius.clamp(0.0, max); 4];
                self.fill(bounds, None, |p, h| {
                    let along = if vertical { p.y / h.y } else { p.x / h.x };
                    let t = (along * 0.5 + 0.5).clamp(0.0, 1.0);
                    // Mixed in sRGB, as the shader does, then converted.
                    let c = Color::rgba(
                        color.r + (end.r - color.r) * t,
                        color.g + (end.g - color.g) * t,
                        color.b + (end.b - color.b) * t,
                        color.a + (end.a - color.a) * t,
                    );
                    (linear(c), rounded_box(p, h, radii))
                });
            }
            Shape::Shadow { radius, blur, .. } => {
                let s = self.scale;
                self.fill(bounds, None, |p, h| {
                    let alpha = shadow_coverage(p, h, radius, blur, s);
                    // Coverage comes with the color: the distance says "inside".
                    ([ink[0], ink[1], ink[2], ink[3] * alpha], -1.0)
                });
            }
        }
    }

    fn image(&mut self, rect: Rect, image: &RgbaImage) {
        if image.width == 0 || image.height == 0 || rect.width <= 0.0 || rect.height <= 0.0 {
            return;
        }
        let s = self.scale;
        let c = self.clip();
        let x0 = ((rect.x * s).floor().max(c.x0 as f32)) as u32;
        let y0 = ((rect.y * s).floor().max(c.y0 as f32)) as u32;
        let x1 = ((rect.right() * s).ceil().min(c.x1 as f32)).max(0.0) as u32;
        let y1 = ((rect.bottom() * s).ceil().min(c.y1 as f32)).max(0.0) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                let px = (x as f32 + 0.5) / s;
                let py = (y as f32 + 0.5) / s;
                if !rect.contains(Point::new(px, py)) {
                    continue;
                }
                let u = (px - rect.x) / rect.width;
                let v = (py - rect.y) / rect.height;
                let texel = sample(image, u, v);
                self.blend(x, y, texel, 1.0);
            }
        }
    }

    fn backdrop(&mut self, rect: Rect, radius: f32, blur: f32, tint: Color) {
        let s = self.scale;
        // The GPU blurs at half size with a sigma in half-size texels.
        let sigma = (blur * s / 2.0 / 2.0).clamp(0.5, 12.0) * 2.0;
        let reach = (sigma * 2.5).ceil() as i64;
        let area = self.pixels_of(rect);
        if area.x1 <= area.x0 || area.y1 <= area.y0 {
            return;
        }
        let kernel: Vec<f32> = (-reach..=reach)
            .map(|k| (-0.5 * (k * k) as f32 / (sigma * sigma)).exp())
            .collect();
        let total: f32 = kernel.iter().sum();
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        let at = |pixels: &[Linear], x: i64, y: i64| {
            pixels[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize]
        };
        let (bx0, bx1) = (i64::from(area.x0), i64::from(area.x1));
        let (by0, by1) = (i64::from(area.y0) - reach, i64::from(area.y1) + reach);
        let bw = (bx1 - bx0) as usize;
        let mut rows = Vec::with_capacity(bw * (by1 - by0) as usize);
        for y in by0..by1 {
            for x in bx0..bx1 {
                let mut sum = [0.0; 4];
                for (k, weight) in (-reach..=reach).zip(&kernel) {
                    let p = at(&self.pixels, x + k, y);
                    for i in 0..4 {
                        sum[i] += p[i] * weight;
                    }
                }
                rows.push(sum.map(|v| v / total));
            }
        }
        let tint_color = linear(tint);
        let half = Point::new(rect.width / 2.0, rect.height / 2.0);
        let center = Point::new(rect.x + half.x, rect.y + half.y);
        let r = radius.min(half.x.min(half.y)).max(0.0);
        for y in i64::from(area.y0)..i64::from(area.y1) {
            for x in bx0..bx1 {
                let mut sum = [0.0; 4];
                for (k, weight) in (-reach..=reach).zip(&kernel) {
                    let row = (y + k - by0) as usize;
                    let p = rows[row * bw + (x - bx0) as usize];
                    for i in 0..4 {
                        sum[i] += p[i] * weight;
                    }
                }
                let blurred = sum.map(|v| v / total);
                let p = Point::new(
                    (x as f32 + 0.5) / s - center.x,
                    (y as f32 + 0.5) / s - center.y,
                );
                let d = rounded_box(p, half, [r; 4]);
                let coverage = (0.5 - d * s).clamp(0.0, 1.0);
                let mut color = mix(blurred, tint_color, tint.a);
                color[3] = 1.0;
                self.blend(x as u32, y as u32, color, coverage);
            }
        }
    }
}

/// The first value of `range` for which `holds` is false, given that it holds for the
/// values before it and none after (the end if it holds for every value).
fn first_not(range: std::ops::Range<u32>, holds: impl Fn(u32) -> bool) -> u32 {
    let (mut low, mut high) = (range.start, range.end.max(range.start));
    while low < high {
        let mid = low + (high - low) / 2;
        if holds(mid) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

/// Bilinear sample of an sRGB image at `u`, `v` (0..1), in linear light.
fn sample(image: &RgbaImage, u: f32, v: f32) -> Linear {
    let fx = (u * image.width as f32 - 0.5).clamp(0.0, (image.width - 1) as f32);
    let fy = (v * image.height as f32 - 0.5).clamp(0.0, (image.height - 1) as f32);
    let (x0, y0) = (fx.floor() as u32, fy.floor() as u32);
    let (x1, y1) = (
        (x0 + 1).min(image.width - 1),
        (y0 + 1).min(image.height - 1),
    );
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let texel = |x: u32, y: u32| {
        let [r, g, b, a] = image.pixel(x, y);
        [decode(r), decode(g), decode(b), f32::from(a) / 255.0]
    };
    let top = mix(texel(x0, y0), texel(x1, y0), tx);
    let bottom = mix(texel(x0, y1), texel(x1, y1), tx);
    mix(top, bottom, ty)
}

fn mix(a: Linear, b: Linear, t: f32) -> Linear {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2, 3].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear(c: Color) -> Linear {
    [to_linear(c.r), to_linear(c.g), to_linear(c.b), c.a]
}

fn decode(c: u8) -> f32 {
    to_linear(f32::from(c) / 255.0)
}

fn encode(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u8
}

// Distance fields, as in quad.wgsl.

fn length(x: f32, y: f32) -> f32 {
    (x * x + y * y).sqrt()
}

fn rounded_box(p: Point, half: Point, radii: CornerRadii) -> f32 {
    let top = if p.x > 0.0 { radii[1] } else { radii[0] };
    let bottom = if p.x > 0.0 { radii[2] } else { radii[3] };
    let r = if p.y > 0.0 { bottom } else { top };
    let qx = p.x.abs() - half.x + r;
    let qy = p.y.abs() - half.y + r;
    qx.max(qy).min(0.0) + length(qx.max(0.0), qy.max(0.0)) - r
}

/// As the quad shader's `shadow_coverage`: `p` is relative to the center of the
/// shadow's bounds, which reach `1.5 * blur` past its box.
fn shadow_coverage(p: Point, half: Point, radii: CornerRadii, blur: f32, scale: f32) -> f32 {
    let h = Point::new(
        (half.x - 1.5 * blur).max(0.0),
        (half.y - 1.5 * blur).max(0.0),
    );
    let top = if p.x > 0.0 { radii[1] } else { radii[0] };
    let bottom = if p.x > 0.0 { radii[2] } else { radii[3] };
    let corner = if p.y > 0.0 { bottom } else { top }.clamp(0.0, h.x.min(h.y));
    let sigma = blur * 0.5;
    if sigma * scale < 0.25 {
        let d = rounded_box(p, h, [corner; 4]) * scale;
        return (0.5 - d).clamp(0.0, 1.0);
    }
    let erf = |x: f32| {
        let a = x.abs();
        let r = 1.0 + (0.278_393 + (0.230_389 + 0.078_108 * (a * a)) * a) * a;
        let r = r * r;
        x.signum() * (1.0 - 1.0 / (r * r))
    };
    let gaussian = |x: f32| (-(x * x) / (2.0 * sigma * sigma)).exp() / (2.506_628 * sigma);
    let row = |x: f32, y: f32| {
        let delta = (h.y - corner - y.abs()).min(0.0);
        let curved = h.x - corner + (corner * corner - delta * delta).max(0.0).sqrt();
        let k = std::f32::consts::FRAC_1_SQRT_2 / sigma;
        0.5 * (erf((x + curved) * k) - erf((x - curved) * k))
    };
    let start = (-3.0 * sigma).clamp(p.y - h.y, p.y + h.y);
    let end = (3.0 * sigma).clamp(p.y - h.y, p.y + h.y);
    let step = (end - start) / 4.0;
    let value: f32 = (0..4)
        .map(|i| {
            let y = start + step * (i as f32 + 0.5);
            row(p.x, p.y - y) * gaussian(y) * step
        })
        .sum();
    value.clamp(0.0, 1.0)
}

fn segment(p: Point, a: Point, b: Point) -> f32 {
    let (pax, pay) = (p.x - a.x, p.y - a.y);
    let (bax, bay) = (b.x - a.x, b.y - a.y);
    let h = ((pax * bax + pay * bay) / (bax * bax + bay * bay).max(1e-6)).clamp(0.0, 1.0);
    length(pax - bax * h, pay - bay * h)
}

fn triangle(p: Point, p0: Point, p1: Point, p2: Point) -> f32 {
    let e = [
        (p1.x - p0.x, p1.y - p0.y),
        (p2.x - p1.x, p2.y - p1.y),
        (p0.x - p2.x, p0.y - p2.y),
    ];
    let v = [
        (p.x - p0.x, p.y - p0.y),
        (p.x - p1.x, p.y - p1.y),
        (p.x - p2.x, p.y - p2.y),
    ];
    let s = (e[0].0 * e[2].1 - e[0].1 * e[2].0).signum();
    let mut best = (f32::INFINITY, f32::INFINITY);
    for i in 0..3 {
        let (ex, ey) = e[i];
        let (vx, vy) = v[i];
        let h = ((vx * ex + vy * ey) / (ex * ex + ey * ey)).clamp(0.0, 1.0);
        let (qx, qy) = (vx - ex * h, vy - ey * h);
        let d = (qx * qx + qy * qy, s * (vx * ey - vy * ex));
        best = (best.0.min(d.0), best.1.min(d.1));
    }
    -best.0.sqrt() * best.1.signum()
}

fn pointing_triangle(p: Point, h: Point, direction: Direction) -> f32 {
    let pt = Point::new;
    match direction {
        Direction::Up => triangle(p, pt(0.0, -h.y), pt(h.x, h.y), pt(-h.x, h.y)),
        Direction::Down => triangle(p, pt(0.0, h.y), pt(-h.x, -h.y), pt(h.x, -h.y)),
        Direction::Left => triangle(p, pt(-h.x, 0.0), pt(h.x, -h.y), pt(h.x, h.y)),
        Direction::Right => triangle(p, pt(h.x, 0.0), pt(-h.x, h.y), pt(-h.x, -h.y)),
    }
}

fn star(p: Point, r: f32, rf: f32) -> f32 {
    let k1 = (0.809_017_f32, -0.587_785_f32);
    let k2 = (-k1.0, k1.1);
    let (mut x, mut y) = (p.x.abs(), -p.y);
    let d1 = 2.0 * (k1.0 * x + k1.1 * y).max(0.0);
    x -= d1 * k1.0;
    y -= d1 * k1.1;
    let d2 = 2.0 * (k2.0 * x + k2.1 * y).max(0.0);
    x -= d2 * k2.0;
    y -= d2 * k2.1;
    x = x.abs();
    y -= r;
    let ba = (rf * -k1.1, rf * k1.0 - 1.0);
    let h = ((x * ba.0 + y * ba.1) / (ba.0 * ba.0 + ba.1 * ba.1)).clamp(0.0, r);
    length(x - ba.0 * h, y - ba.1 * h) * (y * ba.0 - x * ba.1).signum()
}

fn arc(p: Point, r: f32, sweep: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let angle = p.x.atan2(-p.y);
    let a = if angle < 0.0 { angle + tau } else { angle };
    let end = sweep * tau;
    if a <= end {
        return (length(p.x, p.y) - r).abs();
    }
    let (ex, ey) = (r * end.sin(), -r * end.cos());
    length(p.x, p.y + r).min(length(p.x - ex, p.y - ey))
}

#[cfg(test)]
mod tests {
    use super::*;
    use noa_gpui_scene::BoxShadow;
    use noa_gpui_text::{FontOptions, TextStyle};

    fn viewport(width: u32, height: u32, scale: f32) -> Viewport {
        Viewport {
            width,
            height,
            scale_factor: scale,
        }
    }

    /// The alpha of a black shadow on white along the middle row, left to right (blended
    /// in linear light, so the pixels decode to `1 - alpha`).
    fn shadow_row(shadow: &BoxShadow, scale: f32) -> Vec<f32> {
        let mut list = DisplayList::new();
        list.box_shadow(Rect::new(20.0, 20.0, 40.0, 40.0), [8.0; 4], shadow);
        let mut text = TextStore::new(FontOptions::default());
        let image = SoftwareRenderer::new().render(
            &list,
            &mut text,
            viewport((80.0 * scale) as u32, (80.0 * scale) as u32, scale),
            Color::WHITE,
        );
        let y = (40.0 * scale) as u32;
        (0..image.width)
            .map(|x| 1.0 - decode(image.pixel(x, y)[0]))
            .collect()
    }

    #[test]
    fn a_shadow_is_solid_inside_and_fades_across_its_edge() {
        let shadow = BoxShadow {
            color: Color::BLACK,
            blur: 12.0,
            ..BoxShadow::default()
        };
        let row = shadow_row(&shadow, 1.0);
        assert!(row[40] > 0.99, "inside: {}", row[40]);
        // Half way down at the box's edge (x = 20).
        assert!((0.4..0.6).contains(&row[20]), "edge: {}", row[20]);
        assert!(row[2] < 0.01, "1.5 blur outside: {}", row[2]);
        // Falling steadily from the inside out.
        assert!(
            row[2..=40].windows(2).all(|w| w[0] <= w[1] + 1e-3),
            "{row:?}"
        );
        // Without blur the edge is sharp.
        let sharp = shadow_row(
            &BoxShadow {
                blur: 0.0,
                ..shadow
            },
            1.0,
        );
        assert!(sharp[21] > 0.99 && sharp[18] < 0.01, "{sharp:?}");
        // The offset moves it.
        let moved = shadow_row(
            &BoxShadow {
                offset: Point::new(6.0, 0.0),
                ..shadow
            },
            1.0,
        );
        assert!((moved[26] - row[20]).abs() < 0.02);
        // At 2x it is the same shadow.
        let retina = shadow_row(&shadow, 2.0);
        assert!(
            (retina[40] - row[20]).abs() < 0.05,
            "{} vs {}",
            retina[40],
            row[20]
        );
    }

    #[test]
    fn quads_fill_their_rect_with_a_border_and_respect_the_clip() {
        let mut list = DisplayList::new();
        list.push(DrawCommand::Quad {
            rect: Rect::new(10.0, 10.0, 20.0, 20.0),
            radius: [0.0; 4],
            color: Color::hex(0xff0000),
            border: Border {
                width: 2.0,
                color: Color::hex(0x0000ff),
            },
        });
        list.push(DrawCommand::PushClip {
            rect: Rect::new(0.0, 40.0, 5.0, 10.0),
        });
        list.push(DrawCommand::Quad {
            rect: Rect::new(0.0, 40.0, 20.0, 10.0),
            radius: [0.0; 4],
            color: Color::hex(0x00ff00),
            border: Border::default(),
        });
        list.push(DrawCommand::PopClip);
        let mut text = TextStore::new(FontOptions::default());
        let image = SoftwareRenderer::new().render(
            &list,
            &mut text,
            viewport(50, 50, 1.0),
            Color::hex(0xffffff),
        );
        assert_eq!(image.pixel(20, 20), [255, 0, 0, 255]);
        assert_eq!(image.pixel(10, 20), [0, 0, 255, 255]);
        assert_eq!(image.pixel(5, 5), [255, 255, 255, 255]);
        assert_eq!(image.pixel(2, 45), [0, 255, 0, 255]);
        assert_eq!(image.pixel(10, 45), [255, 255, 255, 255]);
    }

    fn canvas(width: u32, height: u32, scale: f32) -> Canvas {
        Canvas {
            width,
            height,
            scale,
            pixels: vec![linear(Color::rgba(0.2, 0.4, 0.6, 0.5)); (width * height) as usize],
            clip: vec![PixelRect {
                x0: 0,
                y0: 0,
                x1: width,
                y1: height,
            }],
        }
    }

    /// A quad drawn the plain way: the distance and the coverage of every pixel.
    fn quad_per_pixel(c: &mut Canvas, rect: Rect, radii: CornerRadii, color: Color, b: Border) {
        let max = (rect.width / 2.0).min(rect.height / 2.0).max(0.0);
        let radii = radii.map(|r| r.clamp(0.0, max));
        let (fill, edge) = (linear(color), linear(b.color));
        let s = c.scale;
        let area = c.pixels_of(rect);
        let half = Point::new(rect.width / 2.0, rect.height / 2.0);
        let center = Point::new(rect.x + half.x, rect.y + half.y);
        for y in area.y0..area.y1 {
            for x in area.x0..area.x1 {
                let p = Point::new(
                    (x as f32 + 0.5) / s - center.x,
                    (y as f32 + 0.5) / s - center.y,
                );
                let d = rounded_box(p, half, radii);
                let color = if b.width > 0.0 {
                    mix(edge, fill, (0.5 - (d + b.width) * s).clamp(0.0, 1.0))
                } else {
                    fill
                };
                let coverage = (0.5 - d * s).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    c.blend(x, y, color, coverage);
                }
            }
        }
    }

    #[test]
    fn quads_are_drawn_as_shading_every_pixel_draws_them() {
        let border = |width, color| Border { width, color };
        let ink = Color::rgba(0.9, 0.1, 0.3, 0.7);
        let rim = Color::rgba(0.1, 0.8, 0.2, 0.9);
        let quads = [
            (
                Rect::new(10.0, 10.0, 200.0, 120.0),
                [0.0; 4],
                border(0.0, rim),
            ),
            (
                Rect::new(10.3, 12.7, 180.6, 90.2),
                [8.0; 4],
                border(1.0, rim),
            ),
            (
                Rect::new(-50.0, 30.0, 400.0, 60.5),
                [0.0, 30.0, 4.0, 12.5],
                border(2.5, rim),
            ),
            (
                Rect::new(40.0, 40.0, 30.0, 30.0),
                [100.0; 4],
                border(0.0, rim),
            ),
            (Rect::new(60.0, 5.0, 3.0, 200.0), [1.0; 4], border(1.0, rim)),
            (Rect::new(0.0, 0.0, 0.0, 50.0), [0.0; 4], border(0.0, rim)),
            (
                Rect::new(5.0, 5.0, 150.0, 150.0),
                [f32::NAN, 0.0, 0.0, 0.0],
                border(0.0, rim),
            ),
            (
                Rect::new(5.0, 5.0, 150.0, 150.0),
                [4.0; 4],
                border(f32::NAN, rim),
            ),
            (
                Rect::new(20.0, 20.0, 120.0, 90.0),
                [6.0; 4],
                border(-3.0, rim),
            ),
            // Far larger than the canvas, as the background of a long scrolled list.
            (
                Rect::new(-7.0, -3.6e6, 250.0, 7.2e6),
                [10.0; 4],
                border(1.0, rim),
            ),
            (
                Rect::new(3.0e6, 3.0, 50.0, 50.0),
                [0.0; 4],
                border(0.0, rim),
            ),
        ];
        for scale in [1.0, 1.5, 2.0, 3.0] {
            let (mut fast, mut plain) = (canvas(260, 180, scale), canvas(260, 180, scale));
            for (i, &(rect, radii, border)) in quads.iter().enumerate() {
                if i == 4 {
                    fast.push_clip(Rect::new(15.5, 20.0, 70.0, 50.0));
                    plain.push_clip(Rect::new(15.5, 20.0, 70.0, 50.0));
                }
                fast.quad(rect, radii, ink, border);
                quad_per_pixel(&mut plain, rect, radii, ink, border);
                let bits = |c: &Canvas| -> Vec<[u32; 4]> {
                    c.pixels.iter().map(|p| p.map(f32::to_bits)).collect()
                };
                assert!(bits(&fast) == bits(&plain), "quad {i} at {scale}x");
            }
        }
    }

    #[test]
    fn rounded_corners_and_shapes_are_antialiased() {
        let mut list = DisplayList::new();
        list.push(DrawCommand::Quad {
            rect: Rect::new(0.0, 0.0, 40.0, 40.0),
            radius: [20.0; 4],
            color: Color::hex(0x000000),
            border: Border::default(),
        });
        list.push(DrawCommand::Shape {
            shape: Shape::Line {
                from: Point::new(50.0, 20.0),
                to: Point::new(90.0, 20.0),
                width: 4.0,
            },
            color: Color::hex(0x000000),
        });
        let mut text = TextStore::new(FontOptions::default());
        let image = SoftwareRenderer::new().render(
            &list,
            &mut text,
            viewport(100, 40, 1.0),
            Color::hex(0xffffff),
        );
        assert_eq!(image.pixel(20, 20), [0, 0, 0, 255]);
        assert_eq!(image.pixel(1, 1), [255, 255, 255, 255]);
        let edge = (0..40).map(|x| image.pixel(x, 10)[0]);
        assert!(edge.into_iter().any(|v| v > 0 && v < 255));
        assert_eq!(image.pixel(70, 20), [0, 0, 0, 255]);
        assert_eq!(image.pixel(70, 25), [255, 255, 255, 255]);
    }

    #[test]
    fn text_is_drawn_inside_its_rect_at_the_scale() {
        let mut text = TextStore::new(FontOptions::default());
        let layout = text.create("Hello", TextStyle::new(20.0), None);
        let size = text.size(layout);
        let mut list = DisplayList::new();
        list.push(DrawCommand::Text {
            rect: Rect::new(10.0, 5.0, size.width, size.height),
            layout,
            color: Color::hex(0x000000),
        });
        let image = SoftwareRenderer::new().render(
            &list,
            &mut text,
            viewport(240, 80, 2.0),
            Color::hex(0xffffff),
        );
        let dark: Vec<(u32, u32)> = (0..80)
            .flat_map(|y| (0..240).map(move |x| (x, y)))
            .filter(|&(x, y)| image.pixel(x, y)[0] < 128)
            .collect();
        assert!(dark.len() > 50, "some ink: {}", dark.len());
        let right = ((10.0 + size.width) * 2.0).ceil() as u32 + 2;
        let bottom = ((5.0 + size.height) * 2.0).ceil() as u32 + 2;
        assert!(
            dark.iter()
                .all(|&(x, y)| x >= 18 && x <= right && y >= 8 && y <= bottom)
        );
    }

    #[test]
    fn icons_and_images_are_drawn() {
        let icon = VectorIcon::builder(10.0, 10.0)
            .fill("M0 0H10V10H0Z")
            .unwrap()
            .build();
        let mut renderer = SoftwareRenderer::new();
        let image = renderer.create_image(2, 1, &[0, 0, 255, 255, 0, 0, 255, 255]);
        let mut list = DisplayList::new();
        list.push(DrawCommand::Icon {
            rect: Rect::new(0.0, 0.0, 10.0, 10.0),
            icon,
            color: Color::hex(0xff0000),
        });
        list.push(DrawCommand::Image {
            rect: Rect::new(20.0, 0.0, 10.0, 10.0),
            image,
        });
        let mut text = TextStore::new(FontOptions::default());
        let out = renderer.render(
            &list,
            &mut text,
            viewport(40, 10, 1.0),
            Color::hex(0xffffff),
        );
        assert_eq!(out.pixel(5, 5), [255, 0, 0, 255]);
        assert_eq!(out.pixel(25, 5), [0, 0, 255, 255]);
        assert_eq!(out.pixel(15, 5), [255, 255, 255, 255]);
    }
}
