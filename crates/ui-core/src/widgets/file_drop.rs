use std::path::{Path, PathBuf};

use ui_layout::Style;
use ui_layout::style::{Dimension, Size as LayoutSize, length, percent};
use ui_scene::{Border, Direction, Point, Rect};
use ui_text::{TextStore, TextStyle};

use super::{CachedText, focus_ring};
use crate::event::{DragData, Event, Key, PointerButton};
use crate::theme::{Role, mix};
use crate::widget::{EventCx, PaintCx, Widget};

/// The files in the order the OS gave them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesDropped(pub Vec<PathBuf>);

/// The app shows the OS's open panel with these settings and hands what was chosen to
/// whatever handles [`FilesDropped`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDialogRequested {
    pub multiple: bool,
    /// File extensions to offer (lowercase, without the dot); empty for any file.
    pub extensions: Vec<String>,
}

const RADIUS: f32 = 10.0;
const TITLE_SIZE: f32 = 13.0;
const MESSAGE_SIZE: f32 = 12.0;
const ARROW: f32 = 18.0;
const GAP: f32 = 6.0;

/// A file upload area that takes files dragged in from the OS. Emits [`FilesDropped`],
/// and [`FileDialogRequested`] when clicked, since the core cannot show OS panels.
#[derive(Debug)]
pub struct FileDropZone {
    title: String,
    message: String,
    extensions: Vec<String>,
    multiple: bool,
    width: Dimension,
    height: f32,
    /// Files that would be taken are over the zone.
    active: bool,
    pressed: bool,
    shaped_title: CachedText,
    shaped_message: CachedText,
}

impl Default for FileDropZone {
    fn default() -> Self {
        Self::new()
    }
}

impl FileDropZone {
    pub fn new() -> Self {
        Self {
            title: "Drop files here".into(),
            message: "or click to choose".into(),
            extensions: Vec::new(),
            multiple: true,
            width: percent(1.0),
            height: 110.0,
            active: false,
            pressed: false,
            shaped_title: CachedText::default(),
            shaped_message: CachedText::default(),
        }
    }

    /// The first line ("Drop files here").
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// The second line, in the accent color ("or click to choose").
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Take only files with these extensions (case does not matter; a leading dot is
    /// ignored). Any file by default.
    pub fn extensions<S: AsRef<str>>(mut self, extensions: impl IntoIterator<Item = S>) -> Self {
        self.extensions = extensions
            .into_iter()
            .map(|e| e.as_ref().trim_start_matches('.').to_lowercase())
            .collect();
        self
    }

    /// On by default.
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn accepts(&self, files: &[PathBuf]) -> bool {
        !files.is_empty()
            && (self.multiple || files.len() == 1)
            && files.iter().all(|f| self.accepts_type(f))
    }

    fn accepts_type(&self, file: &Path) -> bool {
        self.extensions.is_empty()
            || file
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_lowercase)
                .is_some_and(|e| self.extensions.contains(&e))
    }

    fn set_active(&mut self, cx: &mut EventCx, active: bool) {
        if self.active != active {
            self.active = active;
            cx.request_paint();
        }
    }

    fn request_dialog(&self, cx: &mut EventCx) {
        cx.emit(FileDialogRequested {
            multiple: self.multiple,
            extensions: self.extensions.clone(),
        });
        cx.set_handled();
    }
}

impl Widget for FileDropZone {
    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: self.width,
                height: length(self.height),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let accent = cx.color(Role::Accent);
        let surface = cx.color(Role::SurfaceAlt);
        let (fill, border, width) = if self.active {
            (accent.with_alpha(0.12), accent, 2.0)
        } else if self.pressed {
            (
                mix(surface, cx.color(Role::Text), 0.08),
                cx.color(Role::Outline),
                1.5,
            )
        } else if cx.is_hovered() {
            (
                mix(surface, cx.color(Role::Text), 0.04),
                cx.color(Role::Outline),
                1.5,
            )
        } else {
            (surface, cx.color(Role::Outline), 1.5)
        };
        let radius = cx.corner(crate::theme::Corner::Container, RADIUS, bounds.height);
        cx.rounded_rect(
            bounds,
            radius,
            fill,
            Border {
                width,
                color: border,
            },
        );

        let title_style = TextStyle::new(TITLE_SIZE).weight(600);
        let message_style = TextStyle::new(MESSAGE_SIZE);
        let title = self
            .shaped_title
            .shape(cx.text_store(), &self.title, title_style, None);
        let message =
            self.shaped_message
                .shape(cx.text_store(), &self.message, message_style, None);
        let total = ARROW + GAP + title.height + 2.0 + message.height;
        let mut y = (bounds.height - total) / 2.0;
        let cx_mid = bounds.width / 2.0;

        // An arrow pointing into a tray.
        let ink = if self.active {
            accent
        } else {
            cx.color(Role::Muted)
        };
        let stem = ARROW * 0.7;
        cx.line(
            Point::new(cx_mid, y),
            Point::new(cx_mid, y + stem),
            1.75,
            ink,
        );
        cx.chevron(
            Rect::new(cx_mid - 5.0, y + stem - 5.0, 10.0, 5.0),
            Direction::Down,
            1.75,
            ink,
        );
        let tray = y + ARROW;
        cx.line(
            Point::new(cx_mid - 9.0, tray - 4.0),
            Point::new(cx_mid - 9.0, tray),
            1.75,
            ink,
        );
        cx.line(
            Point::new(cx_mid - 9.0, tray),
            Point::new(cx_mid + 9.0, tray),
            1.75,
            ink,
        );
        cx.line(
            Point::new(cx_mid + 9.0, tray),
            Point::new(cx_mid + 9.0, tray - 4.0),
            1.75,
            ink,
        );
        y += ARROW + GAP;

        let text = cx.color(Role::Text);
        if let Some(layout) = self.shaped_title.layout() {
            let rect = Rect::new(cx_mid - title.width / 2.0, y, title.width, title.height);
            cx.text(rect, layout, text);
        }
        y += title.height + 2.0;
        if let Some(layout) = self.shaped_message.layout() {
            let rect = Rect::new(
                cx_mid - message.width / 2.0,
                y,
                message.width,
                message.height,
            );
            cx.text(rect, layout, accent);
        }
        if cx.is_focused() {
            focus_ring(cx, bounds, radius);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::DragOver {
                data: DragData::Files(files),
                ..
            } => {
                if self.accepts(files) {
                    self.set_active(cx, true);
                    cx.accept_drop();
                } else {
                    self.set_active(cx, false);
                }
            }
            Event::DragLeave => self.set_active(cx, false),
            Event::Drop {
                data: DragData::Files(files),
                ..
            } => {
                self.set_active(cx, false);
                if self.accepts(files) {
                    cx.emit(FilesDropped(files.clone()));
                    cx.set_handled();
                }
            }
            Event::HoverChanged(_) => cx.request_paint(),
            Event::PointerDown {
                button: PointerButton::Primary,
                ..
            } => {
                self.pressed = true;
                cx.capture_pointer();
                cx.request_paint();
                cx.set_handled();
            }
            Event::PointerUp {
                pos,
                button: PointerButton::Primary,
            } if self.pressed => {
                self.pressed = false;
                cx.release_pointer();
                cx.request_paint();
                if cx.contains(*pos) {
                    self.request_dialog(cx);
                }
                cx.set_handled();
            }
            Event::KeyDown {
                key: Key::Space | Key::Enter,
                ..
            }
            | Event::Activate => self.request_dialog(cx),
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Button)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.title.clone());
        node.set_description(self.message.clone());
        node.add_action(accesskit::Action::Click);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped_title.release(text);
        self.shaped_message.release(text);
    }
}
