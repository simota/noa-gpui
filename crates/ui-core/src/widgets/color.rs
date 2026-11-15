use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Color, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::{CachedText, ControlSize, focus_ring};
use crate::event::{Event, Key, PointerButton};
use crate::overlay::Placement;
use crate::theme::{Corner, Role};
use crate::widget::{EventCx, MeasureCx, PaintCx, UpdateCx, Widget};

/// Emitted by a [`ColorWell`] or a standalone [`ColorPicker`] when the color changes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorChanged(pub Color);

/// Hue (degrees, `0..360`), saturation and value (`0..=1`) of an opaque color.
pub fn to_hsv(color: Color) -> (f32, f32, f32) {
    let (r, g, b) = (color.r, color.g, color.b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

/// `h` is in degrees.
pub fn from_hsv(h: f32, s: f32, v: f32) -> Color {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Color::rgba(r + m, g + m, b + m, 1.0)
}

/// Formats as `#RRGGBB`.
pub fn hex_string(color: Color) -> String {
    let [r, g, b, _] = color.to_rgba8();
    format!("#{r:02X}{g:02X}{b:02X}")
}

const PAD: f32 = 12.0;
const FIELD_W: f32 = 180.0;
const FIELD_H: f32 = 140.0;
const HUE_H: f32 = 14.0;
const GAP: f32 = 10.0;
const SWATCH_GAP: f32 = 4.0;
pub const SWATCHES: [u32; 8] = [
    0xff3b30, 0xff9500, 0xffcc00, 0x34c759, 0x007aff, 0xaf52de, 0x8e8e93, 0x000000,
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Part {
    Field,
    Hue,
}

/// A hue / saturation / brightness color picker. Standalone it emits [`ColorChanged`];
/// opened by a [`ColorWell`] it reports to the well instead.
#[derive(Debug)]
pub struct ColorPicker {
    hue: f32,
    saturation: f32,
    value: f32,
    for_owner: bool,
    dragging: Option<Part>,
    hex: CachedText,
}

impl ColorPicker {
    pub fn new(color: Color) -> Self {
        let (hue, saturation, value) = to_hsv(color);
        Self {
            hue,
            saturation,
            value,
            for_owner: false,
            dragging: None,
            hex: CachedText::default(),
        }
    }

    fn for_owner(mut self) -> Self {
        self.for_owner = true;
        self
    }

    pub fn color(&self) -> Color {
        from_hsv(self.hue, self.saturation, self.value)
    }

    pub fn set_color(&mut self, cx: &mut UpdateCx, color: Color) {
        (self.hue, self.saturation, self.value) = to_hsv(color);
        cx.request_layout();
    }

    fn field(&self) -> Rect {
        Rect::new(PAD, PAD, FIELD_W, FIELD_H)
    }

    fn hue_bar(&self) -> Rect {
        Rect::new(PAD, PAD + FIELD_H + GAP, FIELD_W, HUE_H)
    }

    fn swatch_size() -> f32 {
        (FIELD_W - SWATCH_GAP * (SWATCHES.len() - 1) as f32) / SWATCHES.len() as f32
    }

    fn swatch(&self, i: usize) -> Rect {
        let size = Self::swatch_size();
        let y = self.hue_bar().bottom() + GAP;
        Rect::new(PAD + i as f32 * (size + SWATCH_GAP), y, size, size)
    }

    fn changed(&mut self, cx: &mut EventCx) {
        let color = self.color();
        if self.for_owner {
            cx.notify_owner(ColorChanged(color));
        } else {
            cx.emit(ColorChanged(color));
        }
        // The hex readout changes with the color.
        cx.request_layout();
        cx.request_paint();
    }

    fn drag_to(&mut self, part: Part, p: Point) {
        match part {
            Part::Field => {
                let f = self.field();
                self.saturation = ((p.x - f.x) / f.width).clamp(0.0, 1.0);
                self.value = 1.0 - ((p.y - f.y) / f.height).clamp(0.0, 1.0);
            }
            Part::Hue => {
                let bar = self.hue_bar();
                // Stop just short of 360 so the hue stays in 0..360.
                self.hue = ((p.x - bar.x) / bar.width).clamp(0.0, 0.9999) * 360.0;
            }
        }
    }
}

impl Widget for ColorPicker {
    fn style(&self) -> Style {
        Style::default()
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        let hex = hex_string(self.color());
        let text = self.hex.shape(cx.text, &hex, TextStyle::new(13.0), None);
        let y = self.swatch(0).bottom() + GAP;
        Size::new(FIELD_W + 2.0 * PAD, y + text.height + PAD)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let surface = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        cx.rounded_rect(
            bounds,
            10.0,
            surface,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        let field = self.field();
        let pure = from_hsv(self.hue, 1.0, 1.0);
        cx.gradient(field, 4.0, Color::WHITE, pure, false);
        cx.gradient(field, 4.0, Color::TRANSPARENT, Color::BLACK, true);
        let hairline = Border {
            width: 1.0,
            color: Color::BLACK.with_alpha(0.15),
        };
        cx.rounded_rect(field, 4.0, Color::TRANSPARENT, hairline);
        let p = Point::new(
            field.x + self.saturation * field.width,
            field.y + (1.0 - self.value) * field.height,
        );
        let ring = 12.0;
        cx.rounded_rect(
            Rect::new(p.x - ring / 2.0, p.y - ring / 2.0, ring, ring),
            ring / 2.0,
            Color::TRANSPARENT,
            Border {
                width: 2.0,
                color: Color::WHITE,
            },
        );
        cx.rounded_rect(
            Rect::new(
                p.x - ring / 2.0 - 1.0,
                p.y - ring / 2.0 - 1.0,
                ring + 2.0,
                ring + 2.0,
            ),
            ring / 2.0 + 1.0,
            Color::TRANSPARENT,
            hairline,
        );

        let bar = self.hue_bar();
        let segment = bar.width / 6.0;
        for i in 0..6 {
            let start = from_hsv(i as f32 * 60.0, 1.0, 1.0);
            let end = from_hsv((i + 1) as f32 * 60.0, 1.0, 1.0);
            let rect = Rect::new(bar.x + i as f32 * segment, bar.y, segment + 0.5, bar.height);
            cx.gradient(rect, 0.0, start, end, false);
        }
        cx.rounded_rect(bar, 3.0, Color::TRANSPARENT, hairline);
        let x = bar.x + self.hue / 360.0 * bar.width;
        cx.rounded_rect(
            Rect::new(x - 3.0, bar.y - 2.0, 6.0, bar.height + 4.0),
            3.0,
            Color::WHITE,
            hairline,
        );

        for (i, &rgb) in SWATCHES.iter().enumerate() {
            cx.rounded_rect(self.swatch(i), 4.0, Color::hex(rgb), hairline);
        }
        if let Some(layout) = self.hex.layout() {
            let size = self.hex.size();
            let y = self.swatch(0).bottom() + GAP;
            let ink = cx.color(Role::Muted);
            cx.text(Rect::new(PAD, y, size.width, size.height), layout, ink);
        }
        if cx.is_focused() && !self.for_owner {
            focus_ring(cx, bounds, 10.0);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                cx.set_handled();
                if let Some(i) = (0..SWATCHES.len()).find(|&i| self.swatch(i).contains(p)) {
                    (self.hue, self.saturation, self.value) = to_hsv(Color::hex(SWATCHES[i]));
                    self.changed(cx);
                    return;
                }
                let part = if self.field().contains(p) {
                    Part::Field
                } else if self.hue_bar().contains(p) {
                    Part::Hue
                } else {
                    return;
                };
                self.dragging = Some(part);
                cx.capture_pointer();
                self.drag_to(part, p);
                self.changed(cx);
            }
            Event::PointerMove { pos } => {
                if let Some(part) = self.dragging {
                    let p = cx.local(*pos);
                    self.drag_to(part, p);
                    self.changed(cx);
                }
                cx.set_handled();
            }
            Event::PointerUp { .. } => {
                if self.dragging.take().is_some() {
                    cx.release_pointer();
                }
                cx.set_handled();
            }
            Event::KeyDown { key, .. } => {
                match key {
                    Key::ArrowLeft => self.saturation = (self.saturation - 0.05).max(0.0),
                    Key::ArrowRight => self.saturation = (self.saturation + 0.05).min(1.0),
                    Key::ArrowDown => self.value = (self.value - 0.05).max(0.0),
                    Key::ArrowUp => self.value = (self.value + 0.05).min(1.0),
                    Key::PageUp => self.hue = (self.hue + 10.0).rem_euclid(360.0),
                    Key::PageDown => self.hue = (self.hue - 10.0).rem_euclid(360.0),
                    Key::Enter | Key::Space if self.for_owner => {
                        cx.close_popover();
                        cx.set_handled();
                        return;
                    }
                    _ => return,
                }
                self.changed(cx);
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::ColorWell)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label("Color picker");
        access_color(node, self.color());
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.hex.release(text);
    }
}

fn access_color(node: &mut accesskit::Node, color: Color) {
    let [red, green, blue, alpha] = color.to_rgba8();
    node.set_color_value(accesskit::Color {
        red,
        green,
        blue,
        alpha,
    });
    node.set_value(hex_string(color));
}

/// A swatch that opens a [`ColorPicker`] in a popover (`NSColorWell`). Emits
/// [`ColorChanged`].
#[derive(Debug)]
pub struct ColorWell {
    color: Color,
    size: ControlSize,
    open: bool,
}

impl ColorWell {
    pub fn new(color: Color) -> Self {
        Self {
            color,
            size: ControlSize::Regular,
            open: false,
        }
    }

    pub fn control_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn color(&self) -> Color {
        self.color
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Set the color without emitting.
    pub fn set_color(&mut self, cx: &mut UpdateCx, color: Color) {
        self.color = color;
        cx.request_paint();
    }

    fn open_picker(&mut self, cx: &mut EventCx) {
        if self.open {
            return;
        }
        self.open = true;
        let anchor = cx.window_rect();
        cx.open_popover(
            ColorPicker::new(self.color).for_owner(),
            Placement::Below(anchor),
        );
        cx.request_paint();
        cx.set_handled();
    }
}

impl Widget for ColorWell {
    fn style(&self) -> Style {
        Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, _cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        let k = self.size.scale();
        Size::new(44.0 * k, 24.0 * k)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let k = self.size.scale();
        let surface = cx.color(Role::Surface);
        let border = if self.open {
            Border {
                width: 2.0,
                color: cx.color(Role::Accent),
            }
        } else {
            Border {
                width: 1.0,
                color: cx.color(Role::Outline),
            }
        };
        let radius = cx.corner(Corner::Control, 6.0 * k, bounds.height);
        cx.bezel(bounds, radius, surface, border);
        let inset = 4.0 * k;
        let swatch = Rect::new(
            inset,
            inset,
            bounds.width - 2.0 * inset,
            bounds.height - 2.0 * inset,
        );
        let color = cx.color(self.color);
        cx.rounded_rect(
            swatch,
            cx.corner(Corner::Control, 3.0 * k, swatch.height),
            color,
            Border {
                width: 0.5,
                color: Color::BLACK.with_alpha(0.2),
            },
        );
        if cx.is_focused() && !self.open {
            focus_ring(cx, bounds, radius);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            // Opens on press, so the press that closes the picker (which goes no
            // further) does not reopen it.
            Event::PointerDown {
                button: PointerButton::Primary,
                ..
            }
            | Event::KeyDown {
                key: Key::Space, ..
            }
            | Event::Activate => self.open_picker(cx),
            Event::FromPopover(message) => {
                if let Some(&ColorChanged(color)) = message.downcast_ref() {
                    self.color = color;
                    cx.emit(ColorChanged(color));
                    cx.request_paint();
                }
            }
            Event::PopoverClosed => {
                self.open = false;
                cx.request_paint();
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::ColorWell)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        access_color(node, self.color);
        node.set_has_popup(accesskit::HasPopup::Dialog);
        node.add_action(accesskit::Action::Click);
    }
}
