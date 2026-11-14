//! The renderer caches masks per pixel size and tints them with the color of the
//! [`crate::DrawCommand::Icon`] that draws them, so one icon follows the theme's colors.
//! An SVG file's own colors are kept per path for [`VectorIcon::color_parts`].

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::geometry::{Color, Point, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IconPaint {
    Fill(FillRule),
    /// A stroke of this width (in the icon's coordinates) with round caps and joins.
    Stroke(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Segment {
    Line(Point),
    Quad(Point, Point),
    Cubic(Point, Point, Point),
}

#[derive(Debug, Clone, PartialEq)]
struct SubPath {
    start: Point,
    segments: Vec<Segment>,
    closed: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct Layer {
    paths: Vec<SubPath>,
    paint: IconPaint,
    /// The color the SVG file gave the path, or `None` for the drawing color
    /// (`currentColor`, and paths built with [`IconBuilder`]).
    color: Option<Color>,
}

#[derive(Debug)]
struct IconData {
    id: u64,
    view_box: Rect,
    layers: Vec<Layer>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A one-color vector icon. Cheap to clone (shared); two icons are equal when they
/// are clones of the same one.
#[derive(Clone)]
pub struct VectorIcon(Arc<IconData>);

impl fmt::Debug for VectorIcon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VectorIcon")
            .field("id", &self.0.id)
            .field("view_box", &self.0.view_box)
            .field("layers", &self.0.layers.len())
            .finish()
    }
}

impl PartialEq for VectorIcon {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}

impl Eq for VectorIcon {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconError(pub String);

impl fmt::Display for IconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for IconError {}

#[derive(Debug, Clone)]
pub struct IconBuilder {
    view_box: Rect,
    layers: Vec<Layer>,
}

impl IconBuilder {
    /// Fill `path` (SVG path data, such as `"M4 4h16v16H4z"`) with the nonzero rule.
    pub fn fill(self, path: &str) -> Result<Self, IconError> {
        self.paint(path, IconPaint::Fill(FillRule::NonZero))
    }

    /// Fill `path` with the even-odd rule (overlaps cut holes).
    pub fn fill_even_odd(self, path: &str) -> Result<Self, IconError> {
        self.paint(path, IconPaint::Fill(FillRule::EvenOdd))
    }

    /// Stroke `path` `width` wide (in the icon's coordinates), with round caps and joins.
    pub fn stroke(self, path: &str, width: f32) -> Result<Self, IconError> {
        self.paint(path, IconPaint::Stroke(width))
    }

    pub fn paint(mut self, path: &str, paint: IconPaint) -> Result<Self, IconError> {
        let paths = parse_path(path)?;
        self.layers.push(Layer {
            paths,
            paint,
            color: None,
        });
        Ok(self)
    }

    pub fn build(self) -> VectorIcon {
        VectorIcon(Arc::new(IconData {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            view_box: self.view_box,
            layers: self.layers,
        }))
    }
}

impl VectorIcon {
    /// Start an icon whose coordinates run from (0, 0) to (`width`, `height`), like an
    /// SVG `viewBox="0 0 width height"`.
    pub fn builder(width: f32, height: f32) -> IconBuilder {
        IconBuilder {
            view_box: Rect::new(0.0, 0.0, width, height),
            layers: Vec::new(),
        }
    }

    /// Read a small SVG file of the kind icon sets ship: `<path>`, `<circle>`,
    /// `<ellipse>`, `<rect>`, `<line>`, `<polyline>` and `<polygon>` elements, filled
    /// and / or stroked, with `fill`, `stroke`, `stroke-width` and `fill-rule` set on
    /// them or inherited from `<svg>` and `<g>`. Every painted part is drawn in the
    /// one color given when drawing. Transforms, gradients, masks, text and
    /// definitions (`<defs>`) are not supported; their content is skipped.
    ///
    /// The colors the file gives (`#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()` and
    /// the basic color names) are kept for [`VectorIcon::color_parts`]; `currentColor`
    /// and colors it cannot read stand for the drawing color. Opacity attributes are
    /// not read.
    pub fn from_svg(svg: &str) -> Result<VectorIcon, IconError> {
        svg::parse(svg)
    }

    /// The icon split into one-color icons, in drawing order, each with the color the
    /// SVG file gave it, or `None` where it is drawn in the drawing color
    /// (`currentColor`): drawn one over another in the same rectangle they show the
    /// icon in its own colors. Consecutive paths of the same color stay in one part;
    /// an icon in one color is one part, itself. Each call makes new icons (new
    /// [`VectorIcon::id`]s), so keep the parts rather than splitting every frame.
    pub fn color_parts(&self) -> Vec<(VectorIcon, Option<Color>)> {
        let layers = &self.0.layers;
        let Some(first) = layers.first() else {
            return vec![(self.clone(), None)];
        };
        if layers.iter().all(|l| l.color == first.color) {
            return vec![(self.clone(), first.color)];
        }
        let mut parts: Vec<(Vec<Layer>, Option<Color>)> = Vec::new();
        for layer in layers {
            match parts.last_mut() {
                Some((run, color)) if *color == layer.color => run.push(layer.clone()),
                _ => parts.push((vec![layer.clone()], layer.color)),
            }
        }
        parts
            .into_iter()
            .map(|(layers, color)| {
                let icon = VectorIcon(Arc::new(IconData {
                    id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
                    view_box: self.0.view_box,
                    layers,
                }));
                (icon, color)
            })
            .collect()
    }

    pub fn view_box(&self) -> Rect {
        self.0.view_box
    }

    /// Shared by clones; used as a cache key.
    pub fn id(&self) -> u64 {
        self.0.id
    }

    /// A coverage mask `width` × `height` pixels, one byte per pixel (0 = empty,
    /// 255 = covered), rows top to bottom. The icon's box is scaled to fit and
    /// centered, keeping its proportions.
    pub fn rasterize(&self, width: u32, height: u32) -> Vec<u8> {
        let (w, h) = (width as usize, height as usize);
        let mut mask = vec![0.0f32; w * h];
        let vb = self.0.view_box;
        if w == 0 || h == 0 || vb.width <= 0.0 || vb.height <= 0.0 {
            return vec![0; w * h];
        }
        let scale = (width as f32 / vb.width).min(height as f32 / vb.height);
        let dx = (width as f32 - vb.width * scale) / 2.0 - vb.x * scale;
        let dy = (height as f32 - vb.height * scale) / 2.0 - vb.y * scale;
        let map = |p: Point| Point::new(p.x * scale + dx, p.y * scale + dy);
        let mut layer = vec![0.0f32; w * h];
        for l in &self.0.layers {
            layer.iter_mut().for_each(|c| *c = 0.0);
            let polylines: Vec<(Vec<Point>, bool)> = l
                .paths
                .iter()
                .map(|p| (flatten(p, &map), p.closed))
                .collect();
            match l.paint {
                IconPaint::Fill(rule) => fill(&polylines, rule, w, h, &mut layer),
                IconPaint::Stroke(width) => stroke(&polylines, width * scale, w, h, &mut layer),
            }
            for (m, c) in mask.iter_mut().zip(&layer) {
                *m = m.max(*c);
            }
        }
        mask.iter()
            .map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect()
    }
}

/// Largest distance, in pixels, between a curve and the lines replacing it.
const TOLERANCE: f32 = 0.1;

fn flatten(path: &SubPath, map: &impl Fn(Point) -> Point) -> Vec<Point> {
    let mut points = vec![map(path.start)];
    let mut current = map(path.start);
    for segment in &path.segments {
        match *segment {
            Segment::Line(p) => {
                current = map(p);
                points.push(current);
            }
            Segment::Quad(c, p) => {
                let (c, p) = (map(c), map(p));
                let n = steps(&[current, c, p]);
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    let u = 1.0 - t;
                    points.push(Point::new(
                        u * u * current.x + 2.0 * u * t * c.x + t * t * p.x,
                        u * u * current.y + 2.0 * u * t * c.y + t * t * p.y,
                    ));
                }
                current = p;
            }
            Segment::Cubic(c1, c2, p) => {
                let (c1, c2, p) = (map(c1), map(c2), map(p));
                let n = steps(&[current, c1, c2, p]);
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    points.push(Point::new(
                        a * current.x + b * c1.x + c * c2.x + d * p.x,
                        a * current.y + b * c1.y + c * c2.y + d * p.y,
                    ));
                }
                current = p;
            }
        }
    }
    points
}

/// Line segments for a curve with these control points: enough that the chords stay
/// within [`TOLERANCE`] of the curve.
fn steps(points: &[Point]) -> usize {
    let (first, last) = (points[0], points[points.len() - 1]);
    let deviation = points[1..points.len() - 1]
        .iter()
        .map(|&p| distance_to_segment(p, first, last))
        .fold(0.0f32, f32::max);
    ((deviation / TOLERANCE).sqrt().ceil() as usize).clamp(1, 128)
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f32 {
    let (abx, aby) = (b.x - a.x, b.y - a.y);
    let len = abx * abx + aby * aby;
    let t = if len > 0.0 {
        (((p.x - a.x) * abx + (p.y - a.y) * aby) / len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (x, y) = (a.x + t * abx - p.x, a.y + t * aby - p.y);
    (x * x + y * y).sqrt()
}

/// Sub-scanlines per pixel row when filling. Coverage across a row is exact.
const SUBSCANLINES: usize = 5;

fn fill(polylines: &[(Vec<Point>, bool)], rule: FillRule, w: usize, h: usize, out: &mut [f32]) {
    // Every contour is closed for filling.
    let mut edges: Vec<(Point, Point)> = Vec::new();
    for (points, _) in polylines {
        for pair in points.windows(2) {
            edges.push((pair[0], pair[1]));
        }
        if let (Some(&first), Some(&last)) = (points.first(), points.last()) {
            edges.push((last, first));
        }
    }
    edges.retain(|(a, b)| a.y != b.y);
    let weight = 1.0 / SUBSCANLINES as f32;
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for row in 0..h {
        for sub in 0..SUBSCANLINES {
            let y = row as f32 + (sub as f32 + 0.5) * weight;
            crossings.clear();
            for &(a, b) in &edges {
                let (top, bottom, winding) = if a.y < b.y { (a, b, 1) } else { (b, a, -1) };
                if y >= top.y && y < bottom.y {
                    let t = (y - top.y) / (bottom.y - top.y);
                    crossings.push((top.x + t * (bottom.x - top.x), winding));
                }
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                let inside = match rule {
                    FillRule::NonZero => winding != 0,
                    FillRule::EvenOdd => winding % 2 != 0,
                };
                if inside {
                    add_span(
                        &mut out[row * w..(row + 1) * w],
                        pair[0].0,
                        pair[1].0,
                        weight,
                    );
                }
            }
        }
    }
}

fn add_span(row: &mut [f32], x0: f32, x1: f32, weight: f32) {
    let width = row.len() as f32;
    let (x0, x1) = (x0.clamp(0.0, width), x1.clamp(0.0, width));
    if x1 <= x0 {
        return;
    }
    let (first, last) = (x0.floor() as usize, (x1.ceil() as usize).min(row.len()));
    for (i, cell) in row.iter_mut().enumerate().take(last).skip(first) {
        let covered = (x1.min(i as f32 + 1.0) - x0.max(i as f32)).max(0.0);
        *cell += covered * weight;
    }
}

fn stroke(polylines: &[(Vec<Point>, bool)], width: f32, w: usize, h: usize, out: &mut [f32]) {
    // Strokes thinner than a pixel are drawn a pixel wide and fainter.
    let half = width.max(1.0) / 2.0;
    let strength = width.min(1.0);
    let mut segment = |a: Point, b: Point| {
        let reach = half + 1.0;
        let x0 = (a.x.min(b.x) - reach).floor().max(0.0) as usize;
        let y0 = (a.y.min(b.y) - reach).floor().max(0.0) as usize;
        let x1 = ((a.x.max(b.x) + reach).ceil().max(0.0) as usize).min(w);
        let y1 = ((a.y.max(b.y) + reach).ceil().max(0.0) as usize).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                let p = Point::new(x as f32 + 0.5, y as f32 + 0.5);
                let coverage = (half + 0.5 - distance_to_segment(p, a, b)).clamp(0.0, 1.0);
                let cell = &mut out[y * w + x];
                *cell = cell.max(coverage * strength);
            }
        }
    };
    for (points, closed) in polylines {
        if points.len() == 1 {
            segment(points[0], points[0]);
        }
        for pair in points.windows(2) {
            segment(pair[0], pair[1]);
        }
        if *closed && let (Some(&first), Some(&last)) = (points.first(), points.last()) {
            segment(last, first);
        }
    }
}

/// Parse SVG path data into sub-paths, arcs turned into cubic curves.
fn parse_path(data: &str) -> Result<Vec<SubPath>, IconError> {
    let mut lexer = Lexer::new(data);
    let mut paths: Vec<SubPath> = Vec::new();
    let mut current = Point::default();
    let mut start = Point::default();
    let mut last_cubic: Option<Point> = None;
    let mut last_quad: Option<Point> = None;
    let mut command: Option<char> = None;
    loop {
        lexer.skip_separators();
        let Some(c) = lexer.peek() else { break };
        let cmd = if c.is_ascii_alphabetic() {
            lexer.next();
            c
        } else {
            match command {
                // Coordinates after a move are lines.
                Some('M') => 'L',
                Some('m') => 'l',
                Some(cmd) if !matches!(cmd, 'Z' | 'z') => cmd,
                _ => return Err(IconError(format!("expected a path command at {c:?}"))),
            }
        };
        command = Some(cmd);
        let relative = cmd.is_ascii_lowercase();
        let origin = if relative { current } else { Point::default() };
        let at = |x: f32, y: f32| Point::new(origin.x + x, origin.y + y);
        let upper = cmd.to_ascii_uppercase();
        if upper != 'M' && upper != 'Z' {
            match paths.last() {
                None => return Err(IconError("path data must start with a move".into())),
                // Drawing on after a close starts a new sub-path where the last began.
                Some(path) if path.closed => paths.push(SubPath {
                    start: current,
                    segments: Vec::new(),
                    closed: false,
                }),
                Some(_) => {}
            }
        }
        let (mut cubic, mut quad) = (None, None);
        match upper {
            'M' => {
                let (x, y) = (lexer.number()?, lexer.number()?);
                current = at(x, y);
                start = current;
                paths.push(SubPath {
                    start,
                    segments: Vec::new(),
                    closed: false,
                });
            }
            'Z' => {
                if let Some(path) = paths.last_mut() {
                    path.closed = true;
                }
                current = start;
            }
            'L' => {
                let (x, y) = (lexer.number()?, lexer.number()?);
                current = at(x, y);
                segments(&mut paths).push(Segment::Line(current));
            }
            'H' => {
                let x = lexer.number()?;
                current = Point::new(if relative { current.x + x } else { x }, current.y);
                segments(&mut paths).push(Segment::Line(current));
            }
            'V' => {
                let y = lexer.number()?;
                current = Point::new(current.x, if relative { current.y + y } else { y });
                segments(&mut paths).push(Segment::Line(current));
            }
            'C' | 'S' => {
                let c1 = if upper == 'C' {
                    let (x, y) = (lexer.number()?, lexer.number()?);
                    at(x, y)
                } else {
                    reflect(last_cubic, current)
                };
                let (x2, y2) = (lexer.number()?, lexer.number()?);
                let (x, y) = (lexer.number()?, lexer.number()?);
                let (c2, end) = (at(x2, y2), at(x, y));
                segments(&mut paths).push(Segment::Cubic(c1, c2, end));
                cubic = Some(c2);
                current = end;
            }
            'Q' | 'T' => {
                let c = if upper == 'Q' {
                    let (x, y) = (lexer.number()?, lexer.number()?);
                    at(x, y)
                } else {
                    reflect(last_quad, current)
                };
                let (x, y) = (lexer.number()?, lexer.number()?);
                let end = at(x, y);
                segments(&mut paths).push(Segment::Quad(c, end));
                quad = Some(c);
                current = end;
            }
            'A' => {
                let (rx, ry, rotation) = (lexer.number()?, lexer.number()?, lexer.number()?);
                let (large, sweep) = (lexer.flag()?, lexer.flag()?);
                let (x, y) = (lexer.number()?, lexer.number()?);
                let end = at(x, y);
                let arc = arc_to_cubics(current, end, rx, ry, rotation, large, sweep);
                segments(&mut paths).extend(arc);
                current = end;
            }
            _ => return Err(IconError(format!("unknown path command {cmd:?}"))),
        }
        last_cubic = cubic;
        last_quad = quad;
    }
    Ok(paths)
}

/// The sub-path being drawn (there is one: a move came first).
fn segments(paths: &mut [SubPath]) -> &mut Vec<Segment> {
    &mut paths.last_mut().expect("moved").segments
}

/// The reflection of the previous control point about the current point (or the
/// current point itself when the previous command was not the same kind of curve).
fn reflect(control: Option<Point>, current: Point) -> Point {
    control.map_or(current, |c| {
        Point::new(2.0 * current.x - c.x, 2.0 * current.y - c.y)
    })
}

/// An SVG elliptical arc as cubic Béziers (SVG 1.1 implementation notes, F.6.5).
fn arc_to_cubics(
    from: Point,
    to: Point,
    rx: f32,
    ry: f32,
    rotation: f32,
    large: bool,
    sweep: bool,
) -> Vec<Segment> {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || from == to {
        return vec![Segment::Line(to)];
    }
    let (sin, cos) = rotation.to_radians().sin_cos();
    let (hx, hy) = ((from.x - to.x) / 2.0, (from.y - to.y) / 2.0);
    let x1 = cos * hx + sin * hy;
    let y1 = -sin * hx + cos * hy;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = (num / den).max(0.0).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let cx1 = coef * rx * y1 / ry;
    let cy1 = -coef * ry * x1 / rx;
    let cx = cos * cx1 - sin * cy1 + (from.x + to.x) / 2.0;
    let cy = sin * cx1 + cos * cy1 + (from.y + to.y) / 2.0;
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        if a.is_nan() { 0.0 } else { a }
    };
    let (ux, uy) = ((x1 - cx1) / rx, (y1 - cy1) / ry);
    let (vx, vy) = ((-x1 - cx1) / rx, (-y1 - cy1) / ry);
    let theta = angle(1.0, 0.0, ux, uy);
    let mut delta = angle(ux, uy, vx, vy);
    let tau = std::f32::consts::TAU;
    if !sweep && delta > 0.0 {
        delta -= tau;
    } else if sweep && delta < 0.0 {
        delta += tau;
    }
    let parts = (delta.abs() / (tau / 4.0)).ceil().max(1.0) as usize;
    let step = delta / parts as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f32| {
        let (s, c) = t.sin_cos();
        (rx * c, ry * s)
    };
    let derivative = |t: f32| {
        let (s, c) = t.sin_cos();
        (-rx * s, ry * c)
    };
    let place = |x: f32, y: f32| Point::new(cos * x - sin * y + cx, sin * x + cos * y + cy);
    (0..parts)
        .map(|i| {
            let (t0, t1) = (theta + step * i as f32, theta + step * (i + 1) as f32);
            let (p0, p1) = (point(t0), point(t1));
            let (d0, d1) = (derivative(t0), derivative(t1));
            let c1 = place(p0.0 + k * d0.0, p0.1 + k * d0.1);
            let c2 = place(p1.0 - k * d1.0, p1.1 - k * d1.1);
            let end = if i + 1 == parts {
                to
            } else {
                place(p1.0, p1.1)
            };
            Segment::Cubic(c1, c2, end)
        })
        .collect()
}

struct Lexer<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_separators(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == ',' {
                self.next();
            } else {
                break;
            }
        }
    }

    fn number(&mut self) -> Result<f32, IconError> {
        self.skip_separators();
        let bytes = self.text.as_bytes();
        let start = self.pos;
        let mut end = start;
        if end < bytes.len() && matches!(bytes[end], b'+' | b'-') {
            end += 1;
        }
        let mut dot = false;
        while end < bytes.len() {
            match bytes[end] {
                b'0'..=b'9' => end += 1,
                // A second dot starts the next number ("1.5.5" is 1.5 and .5).
                b'.' if !dot => {
                    dot = true;
                    end += 1;
                }
                b'e' | b'E'
                    if end + 1 < bytes.len()
                        && (bytes[end + 1].is_ascii_digit()
                            || matches!(bytes[end + 1], b'+' | b'-')) =>
                {
                    end += 2;
                    while end < bytes.len() && bytes[end].is_ascii_digit() {
                        end += 1;
                    }
                    break;
                }
                _ => break,
            }
        }
        let number = &self.text[start..end];
        self.pos = end;
        number
            .parse()
            .map_err(|_| IconError(format!("expected a number at byte {start}")))
    }

    /// An arc flag: a single `0` or `1`, which may run into what follows.
    fn flag(&mut self) -> Result<bool, IconError> {
        self.skip_separators();
        match self.next() {
            Some('0') => Ok(false),
            Some('1') => Ok(true),
            _ => Err(IconError("expected an arc flag (0 or 1)".into())),
        }
    }
}

mod svg {
    use super::{FillRule, IconData, IconError, IconPaint, Layer, Lexer, NEXT_ID, VectorIcon};
    use crate::geometry::{Color, Rect};
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    /// Paint attributes, inherited from `<svg>` and `<g>`.
    #[derive(Debug, Clone)]
    struct Style {
        fill: bool,
        stroke: bool,
        /// `None` for the drawing color (`currentColor`).
        fill_color: Option<Color>,
        stroke_color: Option<Color>,
        stroke_width: f32,
        fill_rule: FillRule,
    }

    impl Default for Style {
        fn default() -> Self {
            // SVG's defaults: filled black, no stroke, 1 wide.
            Self {
                fill: true,
                stroke: false,
                fill_color: Some(Color::BLACK),
                stroke_color: Some(Color::BLACK),
                stroke_width: 1.0,
                fill_rule: FillRule::NonZero,
            }
        }
    }

    impl Style {
        fn apply(&mut self, attributes: &[(&str, &str)]) {
            for &(name, value) in attributes {
                match name {
                    "fill" => {
                        self.fill = value != "none";
                        self.fill_color = color(value);
                    }
                    "stroke" => {
                        self.stroke = value != "none";
                        self.stroke_color = color(value);
                    }
                    "stroke-width" => {
                        if let Ok(width) = Lexer::new(value).number() {
                            self.stroke_width = width;
                        }
                    }
                    "fill-rule" => {
                        self.fill_rule = if value == "evenodd" {
                            FillRule::EvenOdd
                        } else {
                            FillRule::NonZero
                        };
                    }
                    "style" => {
                        let declarations: Vec<(&str, &str)> = value
                            .split(';')
                            .filter_map(|d| {
                                let (k, v) = d.split_once(':')?;
                                Some((k.trim(), v.trim()))
                            })
                            // `style` is not a property: a declaration of that name
                            // would be followed into itself, as deep as the file says.
                            .filter(|&(k, _)| k != "style")
                            .collect();
                        self.apply(&declarations);
                    }
                    _ => {}
                }
            }
        }
    }

    const SKIPPED: [&str; 8] = [
        "defs",
        "clipPath",
        "mask",
        "symbol",
        "linearGradient",
        "radialGradient",
        "pattern",
        "title",
    ];

    pub(super) fn parse(svg: &str) -> Result<VectorIcon, IconError> {
        let mut view_box: Option<Rect> = None;
        let mut size: (Option<f32>, Option<f32>) = (None, None);
        let mut styles = vec![Style::default()];
        let mut skip_depth = 0usize;
        let mut layers = Vec::new();
        let mut rest = svg;
        while let Some(open) = rest.find('<') {
            rest = &rest[open + 1..];
            if let Some(after) = rest.strip_prefix("!--") {
                rest = after.find("-->").map_or("", |end| &after[end + 3..]);
                continue;
            }
            let close = rest
                .find('>')
                .ok_or_else(|| IconError("unclosed tag".into()))?;
            let tag = &rest[..close];
            rest = &rest[close + 1..];
            if tag.starts_with('?') || tag.starts_with('!') {
                continue;
            }
            if let Some(name) = tag.strip_prefix('/') {
                let name = name.trim();
                if skip_depth > 0 {
                    if SKIPPED.contains(&name) {
                        skip_depth -= 1;
                    }
                } else if (name == "g" || name == "svg") && styles.len() > 1 {
                    styles.pop();
                }
                continue;
            }
            let self_closing = tag.ends_with('/');
            let tag = tag.trim_end_matches('/');
            let (name, attributes) = split_tag(tag);
            if skip_depth > 0 || SKIPPED.contains(&name) {
                if SKIPPED.contains(&name) && !self_closing {
                    skip_depth += 1;
                }
                continue;
            }
            let mut style = styles.last().cloned().unwrap_or_default();
            style.apply(&attributes);
            let get = |key: &str| attributes.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
            let number = |key: &str| get(key).and_then(|v| Lexer::new(v).number().ok());
            match name {
                "svg" | "g" => {
                    if name == "svg" && view_box.is_none() {
                        if let Some(v) = get("viewBox") {
                            let mut lexer = Lexer::new(v);
                            let values = (0..4)
                                .map(|_| lexer.number())
                                .collect::<Result<Vec<f32>, _>>()?;
                            view_box = Some(Rect::new(values[0], values[1], values[2], values[3]));
                        }
                        size = (number("width"), number("height"));
                    }
                    if !self_closing {
                        styles.push(style);
                    }
                    continue;
                }
                _ => {}
            }
            let path = match name {
                "path" => get("d").unwrap_or_default().to_owned(),
                "circle" => {
                    let r = number("r").unwrap_or(0.0);
                    ellipse(
                        number("cx").unwrap_or(0.0),
                        number("cy").unwrap_or(0.0),
                        r,
                        r,
                    )
                }
                "ellipse" => ellipse(
                    number("cx").unwrap_or(0.0),
                    number("cy").unwrap_or(0.0),
                    number("rx").unwrap_or(0.0),
                    number("ry").unwrap_or(0.0),
                ),
                "rect" => {
                    let (x, y) = (number("x").unwrap_or(0.0), number("y").unwrap_or(0.0));
                    let (w, h) = (
                        number("width").unwrap_or(0.0),
                        number("height").unwrap_or(0.0),
                    );
                    let rx = number("rx").or(number("ry")).unwrap_or(0.0);
                    let ry = number("ry").unwrap_or(rx);
                    rect(x, y, w, h, rx.min(w / 2.0), ry.min(h / 2.0))
                }
                "line" => format!(
                    "M{} {}L{} {}",
                    number("x1").unwrap_or(0.0),
                    number("y1").unwrap_or(0.0),
                    number("x2").unwrap_or(0.0),
                    number("y2").unwrap_or(0.0)
                ),
                "polyline" | "polygon" => {
                    let points = get("points").unwrap_or_default();
                    let mut lexer = Lexer::new(points);
                    let mut values = Vec::new();
                    while let Ok(v) = lexer.number() {
                        values.push(v);
                    }
                    let mut d = String::new();
                    for (i, [x, y]) in values.as_chunks::<2>().0.iter().enumerate() {
                        d.push_str(&format!("{}{x} {y}", if i == 0 { 'M' } else { 'L' }));
                    }
                    if name == "polygon" && !d.is_empty() {
                        d.push('Z');
                    }
                    d
                }
                _ => continue,
            };
            let paths = super::parse_path(&path)?;
            if paths.is_empty() {
                continue;
            }
            // Lines and polylines have no inside.
            if style.fill && !matches!(name, "line" | "polyline") {
                layers.push(Layer {
                    paths: paths.clone(),
                    paint: IconPaint::Fill(style.fill_rule),
                    color: style.fill_color,
                });
            }
            if style.stroke && style.stroke_width > 0.0 {
                layers.push(Layer {
                    paths,
                    paint: IconPaint::Stroke(style.stroke_width),
                    color: style.stroke_color,
                });
            }
        }
        let view_box = match (view_box, size) {
            (Some(view_box), _) => view_box,
            (None, (Some(w), Some(h))) => Rect::new(0.0, 0.0, w, h),
            _ => return Err(IconError("the SVG has no viewBox or size".into())),
        };
        Ok(VectorIcon(Arc::new(IconData {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            view_box,
            layers,
        })))
    }

    /// An SVG paint's color, or `None` for `currentColor` and what is not read here
    /// (gradients, other names).
    fn color(value: &str) -> Option<Color> {
        let value = value.trim();
        if let Some(hex) = value.strip_prefix('#') {
            let digit = |i: usize| u8::from_str_radix(hex.get(i..i + 1)?, 16).ok();
            let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
            let unit = |v: u8| f32::from(v) / 255.0;
            return match hex.len() {
                3 | 4 => {
                    let short = |i| digit(i).map(|d| unit(d * 17));
                    let a = if hex.len() == 4 { short(3)? } else { 1.0 };
                    Some(Color::rgba(short(0)?, short(1)?, short(2)?, a))
                }
                6 | 8 => {
                    let a = if hex.len() == 8 { unit(byte(6)?) } else { 1.0 };
                    let (r, g, b) = (byte(0)?, byte(2)?, byte(4)?);
                    Some(Color::rgba(unit(r), unit(g), unit(b), a))
                }
                _ => None,
            };
        }
        let lower = value.to_ascii_lowercase();
        if let Some(args) = (lower.strip_prefix("rgba(").or(lower.strip_prefix("rgb(")))
            .and_then(|rest| rest.strip_suffix(')'))
        {
            let parts: Vec<&str> = args
                .split([',', ' ', '/'])
                .filter(|p| !p.is_empty())
                .collect();
            let channel = |p: &str| match p.strip_suffix('%') {
                Some(percent) => percent.parse::<f32>().ok().map(|v| v / 100.0),
                None => p.parse::<f32>().ok().map(|v| v / 255.0),
            };
            let alpha = |p: &str| match p.strip_suffix('%') {
                Some(percent) => percent.parse::<f32>().ok().map(|v| v / 100.0),
                None => p.parse::<f32>().ok(),
            };
            let (r, g, b) = (
                channel(parts.first()?)?,
                channel(parts.get(1)?)?,
                channel(parts.get(2)?)?,
            );
            let a = parts.get(3).map_or(Some(1.0), |p| alpha(p))?;
            let c = |v: f32| v.clamp(0.0, 1.0);
            return Some(Color::rgba(c(r), c(g), c(b), c(a)));
        }
        let rgb = match lower.as_str() {
            "black" => 0x000000,
            "white" => 0xffffff,
            "red" => 0xff0000,
            "green" => 0x008000,
            "lime" => 0x00ff00,
            "blue" => 0x0000ff,
            "yellow" => 0xffff00,
            "orange" => 0xffa500,
            "purple" => 0x800080,
            "gray" | "grey" => 0x808080,
            "silver" => 0xc0c0c0,
            "transparent" => return Some(Color::TRANSPARENT),
            _ => return None,
        };
        Some(Color::hex(rgb))
    }

    fn split_tag(tag: &str) -> (&str, Vec<(&str, &str)>) {
        let tag = tag.trim();
        let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
        let (name, mut rest) = tag.split_at(name_end);
        let mut attributes = Vec::new();
        loop {
            rest = rest.trim_start();
            let Some(eq) = rest.find('=') else { break };
            let key = rest[..eq].trim();
            let value = rest[eq + 1..].trim_start();
            let Some(quote) = value.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                break;
            };
            let value = &value[1..];
            let Some(end) = value.find(quote) else { break };
            attributes.push((key, &value[..end]));
            rest = &value[end + 1..];
        }
        (name, attributes)
    }

    fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> String {
        format!(
            "M{} {cy}A{rx} {ry} 0 1 0 {} {cy}A{rx} {ry} 0 1 0 {} {cy}Z",
            cx - rx,
            cx + rx,
            cx - rx
        )
    }

    fn rect(x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32) -> String {
        if rx <= 0.0 || ry <= 0.0 {
            return format!("M{x} {y}h{w}v{h}h{}Z", -w);
        }
        format!(
            "M{} {y}h{}a{rx} {ry} 0 0 1 {rx} {ry}v{}a{rx} {ry} 0 0 1 {} {ry}h{}a{rx} {ry} 0 0 1 {} {}v{}a{rx} {ry} 0 0 1 {rx} {}Z",
            x + rx,
            w - 2.0 * rx,
            h - 2.0 * ry,
            -rx,
            -(w - 2.0 * rx),
            -rx,
            -ry,
            -(h - 2.0 * ry),
            -ry
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(mask: &[u8], w: usize, x: usize, y: usize) -> u8 {
        mask[y * w + x]
    }

    #[test]
    fn a_filled_square_covers_its_pixels_exactly() {
        let icon = VectorIcon::builder(10.0, 10.0)
            .fill("M2 2H8V8H2Z")
            .unwrap()
            .build();
        let mask = icon.rasterize(10, 10);
        assert_eq!(coverage(&mask, 10, 5, 5), 255);
        assert_eq!(coverage(&mask, 10, 0, 0), 0);
        assert_eq!(coverage(&mask, 10, 1, 5), 0);
        assert_eq!(mask.iter().filter(|&&c| c == 255).count(), 36);
        let big = icon.rasterize(40, 40);
        assert_eq!(big.iter().filter(|&&c| c == 255).count(), 24 * 24);
    }

    #[test]
    fn edges_between_pixels_are_antialiased() {
        let icon = VectorIcon::builder(4.0, 4.0)
            .fill("M0.5 0V4H4V0Z")
            .unwrap()
            .build();
        let mask = icon.rasterize(4, 4);
        assert!((126..=129).contains(&coverage(&mask, 4, 0, 1)), "{mask:?}");
        assert_eq!(coverage(&mask, 4, 1, 1), 255);
    }

    #[test]
    fn fill_rules_decide_whether_an_inner_square_is_a_hole() {
        // Two squares wound the same way: nonzero fills the middle, even-odd does not.
        let d = "M0 0H10V10H0Z M3 3H7V7H3Z";
        let nonzero = VectorIcon::builder(10.0, 10.0).fill(d).unwrap().build();
        let evenodd = VectorIcon::builder(10.0, 10.0)
            .fill_even_odd(d)
            .unwrap()
            .build();
        assert_eq!(coverage(&nonzero.rasterize(10, 10), 10, 5, 5), 255);
        assert_eq!(coverage(&evenodd.rasterize(10, 10), 10, 5, 5), 0);
        assert_eq!(coverage(&evenodd.rasterize(10, 10), 10, 1, 5), 255);
    }

    #[test]
    fn strokes_are_round_and_as_wide_as_asked() {
        let icon = VectorIcon::builder(20.0, 20.0)
            .stroke("M5 10H15", 4.0)
            .unwrap()
            .build();
        let mask = icon.rasterize(20, 20);
        assert_eq!(coverage(&mask, 20, 10, 9), 255);
        assert_eq!(coverage(&mask, 20, 10, 8), 255);
        assert_eq!(coverage(&mask, 20, 10, 6), 0);
        // Round cap: past the end by the half width, on the line only.
        assert!(coverage(&mask, 20, 16, 9) > 200);
        assert_eq!(coverage(&mask, 20, 18, 9), 0);
    }

    #[test]
    fn path_data_parses_every_command_form() {
        let paths = parse_path(
            "m1,2 3-4 h5 v-6 c1 1 2 2 3 3 s1 1 2 2 q1 1 2 2 t1 1 a2 2 0 011.5.5 Z L1e1 2",
        )
        .unwrap();
        assert_eq!(paths.len(), 2);
        let path = &paths[0];
        assert_eq!(path.start, Point::new(1.0, 2.0));
        assert_eq!(path.segments[0], Segment::Line(Point::new(4.0, -2.0)));
        assert_eq!(path.segments[1], Segment::Line(Point::new(9.0, -2.0)));
        assert_eq!(path.segments[2], Segment::Line(Point::new(9.0, -8.0)));
        assert!(path.closed);
        assert_eq!(paths[1].start, Point::new(1.0, 2.0));
        assert_eq!(paths[1].segments, [Segment::Line(Point::new(10.0, 2.0))]);
        assert!(parse_path("L1 1").is_err());
        assert!(parse_path("M1 1 X").is_err());
    }

    #[test]
    fn arcs_end_where_asked_and_bulge_the_right_way() {
        // A half circle from (0, 5) to (10, 5) bulging up (sweep 1, clockwise on screen).
        let icon = VectorIcon::builder(10.0, 10.0)
            .fill("M0 5A5 5 0 0 1 10 5Z")
            .unwrap()
            .build();
        let mask = icon.rasterize(10, 10);
        assert_eq!(coverage(&mask, 10, 5, 2), 255);
        assert_eq!(coverage(&mask, 10, 5, 7), 0);
    }

    #[test]
    fn svg_files_read_shapes_and_inherited_paint() {
        let svg = r#"<?xml version="1.0"?>
            <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"
                 fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <!-- a comment <with> brackets -->
              <defs><clipPath id="c"><rect width="24" height="24"/></clipPath></defs>
              <circle cx="12" cy="12" r="10"/>
              <line x1="12" y1="8" x2="12" y2="12"/>
              <g fill="currentColor" stroke="none"><rect x="11" y="15" width="2" height="2" rx="1"/></g>
              <polyline points="1 1, 3 3"/>
            </svg>"#;
        let icon = VectorIcon::from_svg(svg).unwrap();
        assert_eq!(icon.view_box(), Rect::new(0.0, 0.0, 24.0, 24.0));
        let paints: Vec<IconPaint> = icon.0.layers.iter().map(|l| l.paint).collect();
        assert_eq!(
            paints,
            [
                IconPaint::Stroke(2.0),
                IconPaint::Stroke(2.0),
                IconPaint::Fill(FillRule::NonZero),
                IconPaint::Stroke(2.0),
            ]
        );
        let mask = icon.rasterize(24, 24);
        assert_eq!(coverage(&mask, 24, 7, 12), 0, "the ring is not filled");
        assert_eq!(coverage(&mask, 24, 12, 2), 255, "the ring");
        assert_eq!(coverage(&mask, 24, 12, 10), 255, "the line");
        assert!(
            coverage(&mask, 24, 12, 16) > 150,
            "the dot (a quarter of its circle)"
        );
    }

    #[test]
    fn svg_defaults_fill_black_and_size_without_a_view_box() {
        let icon = VectorIcon::from_svg(
            r#"<svg width="8" height="8"><path style="fill-rule:evenodd" d="M0 0h8v8H0z"/></svg>"#,
        )
        .unwrap();
        assert_eq!(icon.view_box(), Rect::new(0.0, 0.0, 8.0, 8.0));
        assert_eq!(icon.0.layers[0].paint, IconPaint::Fill(FillRule::EvenOdd));
        assert!(VectorIcon::from_svg("<svg><path d=\"M0 0h1\"/></svg>").is_err());
    }

    #[test]
    fn a_style_inside_a_style_is_not_followed() {
        let nested = "style:".repeat(200_000);
        let svg = format!(
            r#"<svg viewBox="0 0 2 2"><path style="{nested}fill:none" d="M0 0h2v2h-2z"/></svg>"#
        );
        assert!(VectorIcon::from_svg(&svg).is_ok());
    }

    #[test]
    fn clones_are_equal_and_separate_icons_are_not() {
        let a = VectorIcon::builder(1.0, 1.0).build();
        let b = VectorIcon::builder(1.0, 1.0).build();
        assert_eq!(a, a.clone());
        assert_ne!(a, b);
    }

    #[test]
    fn svg_colors_are_kept_per_part_and_current_color_is_left_to_the_drawer() {
        let icon = VectorIcon::from_svg(
            r##"<svg viewBox="0 0 10 10" fill="currentColor">
              <rect width="10" height="10" fill="#f80"/>
              <rect width="4" height="4" style="fill: rgb(0, 128, 255)"/>
              <rect x="1" width="2" height="2" fill="rgba(0,0,0,50%)"/>
              <rect x="6" width="4" height="4"/>
              <rect x="6" y="6" width="4" height="4"/>
              <path d="M0 9H10" stroke="#11223380" fill="none"/>
              <rect y="6" width="2" height="2" fill="url(#g)"/>
              <rect y="8" width="2" height="2" fill="green"/>
            </svg>"##,
        )
        .unwrap();
        let colors: Vec<Option<Color>> = icon.color_parts().into_iter().map(|(_, c)| c).collect();
        assert_eq!(
            colors,
            [
                Some(Color::hex(0xff8800)),
                Some(Color::rgba(0.0, 128.0 / 255.0, 1.0, 1.0)),
                Some(Color::rgba(0.0, 0.0, 0.0, 0.5)),
                // Two in currentColor in a row are one part; a gradient is read as it.
                None,
                Some(Color::hex(0x112233).with_alpha(128.0 / 255.0)),
                None,
                Some(Color::hex(0x008000)),
            ]
        );
        let parts = icon.color_parts();
        assert!(parts.iter().all(|(p, _)| p.view_box() == icon.view_box()));
        let whole = icon.rasterize(10, 10);
        let mut union = vec![0u8; 100];
        for (part, _) in &parts {
            for (u, c) in union.iter_mut().zip(part.rasterize(10, 10)) {
                *u = (*u).max(c);
            }
        }
        assert_eq!(union, whole);
    }

    #[test]
    fn a_one_color_icon_is_its_own_only_part() {
        let icon = VectorIcon::builder(4.0, 4.0)
            .fill("M0 0H4V4H0Z")
            .unwrap()
            .build();
        assert_eq!(icon.color_parts(), [(icon.clone(), None)]);
        let svg = VectorIcon::from_svg(r#"<svg viewBox="0 0 4 4"><path d="M0 0H4V4H0Z"/></svg>"#)
            .unwrap();
        assert_eq!(svg.color_parts(), [(svg.clone(), Some(Color::BLACK))]);
    }
}
