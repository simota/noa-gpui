//! Order is meaningful: later commands paint over earlier ones. Commands never
//! carry GPU handles; images and text layouts are referenced by opaque ids that
//! the renderer and text store resolve.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::geometry::{Color, Point, Rect};
use crate::vector::VectorIcon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextLayoutId(pub u32);

/// An optional border drawn inside a quad's edge.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Border {
    pub width: f32,
    pub color: Color,
}

/// Per-corner radii in logical pixels: top-left, top-right, bottom-right, bottom-left.
pub type CornerRadii = [f32; 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    /// Round caps.
    Line { from: Point, to: Point, width: f32 },
    /// An isosceles triangle filling `rect`, pointing toward `direction`.
    Triangle { rect: Rect, direction: Direction },
    /// A five-pointed star filling `rect` (point up). `outline` > 0 draws only an
    /// outline that wide.
    Star { rect: Rect, outline: f32 },
    /// A round-capped arc of the circle centered at `center`, from 12 o'clock clockwise
    /// over `sweep` of a full turn (`0..=1`).
    Arc {
        center: Point,
        radius: f32,
        width: f32,
        sweep: f32,
    },
    /// A rounded rectangle filled with a linear gradient from the command's color (left
    /// or top edge) to `end` (right or bottom edge), interpolated in sRGB.
    Gradient {
        rect: Rect,
        radius: f32,
        end: Color,
        vertical: bool,
    },
}

impl Shape {
    pub fn bounds(&self) -> Rect {
        match *self {
            Shape::Line { from, to, width } => {
                let r = width / 2.0;
                let (x0, x1) = (from.x.min(to.x) - r, from.x.max(to.x) + r);
                let (y0, y1) = (from.y.min(to.y) - r, from.y.max(to.y) + r);
                Rect::new(x0, y0, x1 - x0, y1 - y0)
            }
            Shape::Triangle { rect, .. }
            | Shape::Gradient { rect, .. }
            | Shape::Star { rect, .. } => rect,
            Shape::Arc {
                center,
                radius,
                width,
                ..
            } => {
                let r = radius + width / 2.0;
                Rect::new(center.x - r, center.y - r, 2.0 * r, 2.0 * r)
            }
        }
    }

    pub fn translated(&self, dx: f32, dy: f32) -> Shape {
        match *self {
            Shape::Line { from, to, width } => Shape::Line {
                from: Point::new(from.x + dx, from.y + dy),
                to: Point::new(to.x + dx, to.y + dy),
                width,
            },
            Shape::Triangle { rect, direction } => Shape::Triangle {
                rect: rect.translate(dx, dy),
                direction,
            },
            Shape::Star { rect, outline } => Shape::Star {
                rect: rect.translate(dx, dy),
                outline,
            },
            Shape::Arc {
                center,
                radius,
                width,
                sweep,
            } => Shape::Arc {
                center: Point::new(center.x + dx, center.y + dy),
                radius,
                width,
                sweep,
            },
            Shape::Gradient {
                rect,
                radius,
                end,
                vertical,
            } => Shape::Gradient {
                rect: rect.translate(dx, dy),
                radius,
                end,
                vertical,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Quad {
        rect: Rect,
        radius: CornerRadii,
        color: Color,
        border: Border,
    },
    Shape {
        shape: Shape,
        color: Color,
    },
    /// An image stretched to fill `rect`.
    Image {
        rect: Rect,
        image: ImageId,
    },
    /// A vector icon drawn in one color, scaled to fit `rect` and centered.
    Icon {
        rect: Rect,
        icon: VectorIcon,
        color: Color,
    },
    /// What has been drawn so far under `rect`, blurred by `blur` logical pixels and
    /// tinted with `tint` (its alpha is the tint's strength), inside rounded corners of
    /// `radius`.
    Backdrop {
        rect: Rect,
        radius: f32,
        blur: f32,
        tint: Color,
    },
    /// A text layout placed with its top-left at `rect`'s origin.
    ///
    /// `rect` is the layout's box; it is used for culling and batching.
    Text {
        rect: Rect,
        layout: TextLayoutId,
        color: Color,
    },
    /// Restrict subsequent drawing to `rect` (intersected with the current clip).
    PushClip {
        rect: Rect,
    },
    PopClip,
}

impl DrawCommand {
    pub fn translated(&self, dx: f32, dy: f32) -> DrawCommand {
        // Built field by field in one match: cloning first and then matching again to
        // move it costs a second dispatch on every command of every frame.
        match self {
            &DrawCommand::Quad {
                rect,
                radius,
                color,
                border,
            } => DrawCommand::Quad {
                rect: rect.translate(dx, dy),
                radius,
                color,
                border,
            },
            DrawCommand::Shape { shape, color } => DrawCommand::Shape {
                shape: shape.translated(dx, dy),
                color: *color,
            },
            &DrawCommand::Image { rect, image } => DrawCommand::Image {
                rect: rect.translate(dx, dy),
                image,
            },
            DrawCommand::Icon { rect, icon, color } => DrawCommand::Icon {
                rect: rect.translate(dx, dy),
                icon: icon.clone(),
                color: *color,
            },
            &DrawCommand::Backdrop {
                rect,
                radius,
                blur,
                tint,
            } => DrawCommand::Backdrop {
                rect: rect.translate(dx, dy),
                radius,
                blur,
                tint,
            },
            &DrawCommand::Text {
                rect,
                layout,
                color,
            } => DrawCommand::Text {
                rect: rect.translate(dx, dy),
                layout,
                color,
            },
            DrawCommand::PushClip { rect } => DrawCommand::PushClip {
                rect: rect.translate(dx, dy),
            },
            DrawCommand::PopClip => DrawCommand::PopClip,
        }
    }

    /// The same command moved by `(dx, dy)` with every color's alpha multiplied by
    /// `opacity` (for fading a group in and out). Images keep their own alpha.
    pub fn translated_faded(&self, dx: f32, dy: f32, opacity: f32) -> DrawCommand {
        let mut command = self.translated(dx, dy);
        let fade = |c: &mut Color| c.a *= opacity;
        match &mut command {
            DrawCommand::Quad { color, border, .. } => {
                fade(color);
                fade(&mut border.color);
            }
            DrawCommand::Shape { shape, color } => {
                fade(color);
                if let Shape::Gradient { end, .. } = shape {
                    fade(end);
                }
            }
            DrawCommand::Text { color, .. } | DrawCommand::Icon { color, .. } => fade(color),
            DrawCommand::Backdrop { tint, .. } => fade(tint),
            DrawCommand::Image { .. } | DrawCommand::PushClip { .. } | DrawCommand::PopClip => {}
        }
        command
    }

    /// The area a drawing command may touch, or `None` for clip commands.
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            DrawCommand::Quad { rect, .. }
            | DrawCommand::Image { rect, .. }
            | DrawCommand::Icon { rect, .. }
            | DrawCommand::Text { rect, .. }
            | DrawCommand::Backdrop { rect, .. } => Some(*rect),
            DrawCommand::Shape { shape, .. } => Some(shape.bounds()),
            DrawCommand::PushClip { .. } | DrawCommand::PopClip => None,
        }
    }
}

static NEXT_REVISION: AtomicU64 = AtomicU64::new(1);

fn next_revision() -> u64 {
    NEXT_REVISION.fetch_add(1, Ordering::Relaxed)
}

/// Stands for "modified since the revision was last read": the next
/// [`DisplayList::revision`] takes a new number. (Numbers start at 1.)
const STALE: u64 = 0;

#[derive(Debug)]
pub struct DisplayList {
    commands: Vec<DrawCommand>,
    /// The revision, or [`STALE`]. Taken from the shared counter only when read, so
    /// assembling a frame from thousands of pieces does not touch it for each one.
    revision: AtomicU64,
}

impl Default for DisplayList {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
            revision: AtomicU64::new(STALE),
        }
    }
}

impl Clone for DisplayList {
    fn clone(&self) -> Self {
        Self {
            commands: self.commands.clone(),
            // Same content, same revision.
            revision: AtomicU64::new(self.revision()),
        }
    }
}

impl DisplayList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    /// Changes whenever the list is modified, and differs between lists. Equal revisions
    /// mean identical content, so a renderer can reuse work from the previous frame.
    pub fn revision(&self) -> u64 {
        let current = self.revision.load(Ordering::Relaxed);
        if current != STALE {
            return current;
        }
        // Another thread reading the same list at once may have numbered it first:
        // keep that number, so both see the same one.
        let fresh = next_revision();
        match self
            .revision
            .compare_exchange(STALE, fresh, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => fresh,
            Err(numbered) => numbered,
        }
    }

    /// Note a modification: the next [`DisplayList::revision`] is a new one.
    fn modified(&mut self) {
        *self.revision.get_mut() = STALE;
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Keeps the allocation for reuse.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.modified();
    }

    pub fn push(&mut self, command: DrawCommand) {
        self.commands.push(command);
        self.modified();
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.push(DrawCommand::Quad {
            rect,
            radius: [0.0; 4],
            color,
            border: Border::default(),
        });
    }

    pub fn rounded_rect(&mut self, rect: Rect, radius: f32, color: Color, border: Border) {
        self.push(DrawCommand::Quad {
            rect,
            radius: [radius; 4],
            color,
            border,
        });
    }

    pub fn image(&mut self, rect: Rect, image: ImageId) {
        self.push(DrawCommand::Image { rect, image });
    }

    pub fn text(&mut self, rect: Rect, layout: TextLayoutId, color: Color) {
        self.push(DrawCommand::Text {
            rect,
            layout,
            color,
        });
    }

    pub fn extend_translated(&mut self, commands: &[DrawCommand], dx: f32, dy: f32) {
        self.commands
            .extend(commands.iter().map(|c| c.translated(dx, dy)));
        self.modified();
    }

    pub fn extend_translated_faded(
        &mut self,
        commands: &[DrawCommand],
        dx: f32,
        dy: f32,
        opacity: f32,
    ) {
        if opacity >= 1.0 {
            self.extend_translated(commands, dx, dy);
            return;
        }
        self.commands
            .extend(commands.iter().map(|c| c.translated_faded(dx, dy, opacity)));
        self.modified();
    }

    pub fn push_clip(&mut self, rect: Rect) {
        self.push(DrawCommand::PushClip { rect });
    }

    pub fn pop_clip(&mut self) {
        self.push(DrawCommand::PopClip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fading_scales_every_color_but_not_clips_or_images() {
        let quad = DrawCommand::Quad {
            rect: Rect::new(0.0, 0.0, 4.0, 4.0),
            radius: CornerRadii::default(),
            color: Color::rgba(1.0, 0.0, 0.0, 0.8),
            border: Border {
                width: 1.0,
                color: Color::rgba(0.0, 0.0, 0.0, 1.0),
            },
        };
        let gradient = DrawCommand::Shape {
            shape: Shape::Gradient {
                rect: Rect::new(0.0, 0.0, 4.0, 4.0),
                radius: 0.0,
                end: Color::WHITE,
                vertical: true,
            },
            color: Color::WHITE,
        };
        let mut list = DisplayList::new();
        let commands = [quad, gradient, DrawCommand::PopClip];
        list.extend_translated_faded(&commands, 2.0, 3.0, 0.5);
        match &list.commands()[0] {
            DrawCommand::Quad {
                rect,
                color,
                border,
                ..
            } => {
                assert_eq!(*rect, Rect::new(2.0, 3.0, 4.0, 4.0));
                assert_eq!(color.a, 0.4);
                assert_eq!(border.color.a, 0.5);
            }
            other => panic!("{other:?}"),
        }
        match &list.commands()[1] {
            DrawCommand::Shape {
                shape: Shape::Gradient { end, .. },
                color,
            } => assert_eq!((color.a, end.a), (0.5, 0.5)),
            other => panic!("{other:?}"),
        }
        assert_eq!(list.commands()[2], DrawCommand::PopClip);
        let mut opaque = DisplayList::new();
        opaque.extend_translated_faded(&commands, 0.0, 0.0, 1.0);
        assert_eq!(opaque.commands(), &commands[..]);
    }

    #[test]
    fn translating_moves_every_kind_of_command_and_keeps_the_rest() {
        let rect = Rect::new(1.0, 2.0, 3.0, 4.0);
        let color = Color::rgba(0.1, 0.2, 0.3, 0.4);
        let icon = VectorIcon::builder(16.0, 16.0)
            .fill("M0 0L16 0L16 16Z")
            .expect("a path")
            .build();
        let commands = [
            DrawCommand::Quad {
                rect,
                radius: [1.0, 2.0, 3.0, 4.0],
                color,
                border: Border { width: 2.0, color },
            },
            DrawCommand::Shape {
                shape: Shape::Line {
                    from: Point::new(1.0, 2.0),
                    to: Point::new(5.0, 6.0),
                    width: 2.0,
                },
                color,
            },
            DrawCommand::Image {
                rect,
                image: ImageId(7),
            },
            DrawCommand::Icon { rect, icon, color },
            DrawCommand::Backdrop {
                rect,
                radius: 3.0,
                blur: 8.0,
                tint: color,
            },
            DrawCommand::Text {
                rect,
                layout: TextLayoutId(9),
                color,
            },
            DrawCommand::PushClip { rect },
            DrawCommand::PopClip,
        ];
        for command in &commands {
            let moved = command.translated(10.0, -20.0);
            assert_eq!(
                moved.bounds(),
                command.bounds().map(|b| b.translate(10.0, -20.0)),
                "{command:?}"
            );
            if let (DrawCommand::PushClip { rect: a }, DrawCommand::PushClip { rect: b }) =
                (command, &moved)
            {
                assert_eq!(*b, a.translate(10.0, -20.0));
            }
            assert_eq!(&moved.translated(-10.0, 20.0), command);
            assert_eq!(&command.translated(0.0, 0.0), command);
        }
    }

    #[test]
    fn revision_changes_on_every_modification_and_is_unique() {
        let mut a = DisplayList::new();
        let b = DisplayList::new();
        assert_ne!(a.revision(), b.revision());
        let r0 = a.revision();
        a.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        let r1 = a.revision();
        assert_ne!(r0, r1);
        a.extend_translated(&[], 1.0, 1.0);
        assert_ne!(a.revision(), r1);
        let r2 = a.revision();
        a.clear();
        assert_ne!(a.revision(), r2);
        let copy = a.clone();
        assert_eq!(
            copy.revision(),
            a.revision(),
            "a clone has the same content"
        );
    }

    #[test]
    fn revision_is_stable_until_modified_however_many_times() {
        let mut a = DisplayList::new();
        let r0 = a.revision();
        assert_eq!(a.revision(), r0, "reading does not change it");
        for _ in 0..3 {
            a.push_clip(Rect::new(0.0, 0.0, 1.0, 1.0));
            a.pop_clip();
        }
        let r1 = a.revision();
        assert_ne!(r1, r0);
        assert_eq!(a.revision(), r1);
        // Cloned before its revision was read: both still agree.
        a.clear();
        let copy = a.clone();
        assert_eq!(copy.revision(), a.revision());
        assert_ne!(a.revision(), r1);
    }
}
