use noa_gpui_layout::{MeasureInput, Style};
use noa_gpui_scene::{Border, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::CachedText;
use super::badge::centered;
use crate::shortcut::Shortcut;
use crate::theme::{Role, mix};
use crate::widget::{MeasureCx, PaintCx, Widget};

pub(crate) const KEYCAP_HEIGHT: f32 = 20.0;
const CAP_PAD: f32 = 5.0;
const CAP_GAP: f32 = 3.0;
/// The keycap's thicker bottom edge.
const DEPTH: f32 = 1.5;

/// One keycap per key. [`Kbd::shortcut`] writes Mac glyphs on Apple platforms and
/// "Ctrl" "Shift" elsewhere, like menus.
#[derive(Debug)]
pub struct Kbd {
    keys: Vec<String>,
    shaped: Vec<CachedText>,
}

impl Kbd {
    pub fn new<S: Into<String>>(keys: impl IntoIterator<Item = S>) -> Self {
        let keys: Vec<String> = keys.into_iter().map(Into::into).collect();
        let shaped = keys.iter().map(|_| CachedText::default()).collect();
        Self { keys, shaped }
    }

    pub fn shortcut(shortcut: Shortcut) -> Self {
        Self::shortcut_with(shortcut, cfg!(target_vendor = "apple"))
    }

    pub fn shortcut_with(shortcut: Shortcut, glyphs: bool) -> Self {
        Self::new(shortcut.keys(glyphs))
    }

    pub fn keys(&self) -> &[String] {
        &self.keys
    }
}

impl Widget for Kbd {
    fn style(&self) -> Style {
        Style {
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn measures(&self) -> bool {
        true
    }

    fn measure(&mut self, cx: &mut MeasureCx, _input: MeasureInput) -> Size {
        shape_keycaps(cx.text, &self.keys, &mut self.shaped);
        Size::new(keycaps_width(&self.shaped), KEYCAP_HEIGHT)
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        paint_keycaps(cx, &self.shaped, Point::default());
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Label)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.keys.join("+"));
    }

    fn removed(&mut self, text: &mut TextStore) {
        for shaped in &mut self.shaped {
            shaped.release(text);
        }
    }
}

fn keycap_style() -> TextStyle {
    TextStyle::new(12.0).weight(500)
}

/// Grows `shaped` as needed.
pub(crate) fn shape_keycaps(store: &mut TextStore, keys: &[String], shaped: &mut Vec<CachedText>) {
    while shaped.len() < keys.len() {
        shaped.push(CachedText::default());
    }
    for (key, cached) in keys.iter().zip(shaped.iter_mut()) {
        cached.shape(store, key, keycap_style(), None);
    }
}

/// Rectangles are from the left edge, left to right.
fn keycaps(shaped: &[CachedText]) -> impl Iterator<Item = (Rect, Size)> + '_ {
    let mut x = 0.0;
    shaped.iter().map(move |text| {
        let size = text.size();
        let width = (size.width + 2.0 * CAP_PAD).max(KEYCAP_HEIGHT);
        let rect = Rect::new(x, 0.0, width, KEYCAP_HEIGHT);
        x += width + CAP_GAP;
        (rect, size)
    })
}

pub(crate) fn keycaps_width(shaped: &[CachedText]) -> f32 {
    keycaps(shaped).last().map_or(0.0, |(rect, _)| rect.right())
}

/// `origin` is the top-left corner. Also used by the command palette.
pub(crate) fn paint_keycaps(cx: &mut PaintCx, shaped: &[CachedText], origin: Point) {
    let surface = cx.color(Role::Surface);
    let outline = cx.color(Role::Outline);
    let edge = mix(outline, cx.color(Role::Text), 0.15);
    let ink = cx.color(Role::Text);
    for ((rect, size), cached) in keycaps(shaped).zip(shaped) {
        let rect = rect.translate(origin.x, origin.y);
        let radius = cx.corner(crate::theme::Corner::Inner, 4.0, rect.height);
        // The bottom edge first, showing below the face.
        cx.rounded_rect(rect, radius, edge, Border::default());
        let face = Rect::new(rect.x, rect.y, rect.width, rect.height - DEPTH);
        let border = Border {
            width: 1.0,
            color: outline,
        };
        cx.rounded_rect(face, radius, surface, border);
        if let Some(layout) = cached.layout() {
            cx.text(centered(face, size), layout, ink);
        }
    }
}
