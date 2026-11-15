//! An overlay is a separate root opened with [`crate::UiTree::open_overlay`]. It sizes
//! to its content, is painted after the main tree and hit-tested before it. Overlays
//! opened later are on top.

use noa_gpui_scene::{Point, Rect, Size};

/// Gap between an anchor and an overlay placed next to it.
pub const ANCHOR_GAP: f32 = 4.0;

/// Where an overlay goes. Rectangles and points are in window coordinates. The result
/// is always shifted to stay inside the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// Top-left corner at a point (context menus at the pointer).
    At(Point),
    /// Below the anchor, left edges aligned; above it if there is no room below
    /// (dropdowns, popovers).
    Below(Rect),
    /// Right of the anchor, top edges aligned; left of it if there is no room on the right
    /// (submenus).
    Beside(Rect),
    /// Centered horizontally, a third of the way down (alerts).
    Center,
    /// On the `edge` side of the anchor, centered along it; on the opposite side if
    /// there is no room (popovers, whose arrow then points back at the anchor).
    Edge { anchor: Rect, edge: Edge },
    /// Centered horizontally with its top edge at `top`, as if hanging from the title
    /// bar or toolbar (sheets).
    Sheet { top: f32 },
    /// In a corner of the window, `inset` from its edges (notifications). The overlay
    /// keeps that corner when it grows or shrinks.
    Corner { corner: WindowCorner, inset: f32 },
    /// Against the `edge` side of the window, below `top` (the bottom of the title bar
    /// or toolbar): drawers. Unlike other overlays it is laid out in the window's area
    /// below `top`, so a percentage size (100% of the height for a side drawer) is a
    /// share of that area and follows the window when it is resized.
    Drawer { edge: Edge, top: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OverlayOptions {
    /// Close when the pointer is pressed outside it or Escape is pressed
    /// (menus, popovers). Emits [`OverlayDismissed`].
    pub light_dismiss: bool,
    /// A press outside that dismisses the overlay goes no further (menus). Otherwise it
    /// also reaches what is under the pointer (popovers).
    pub consume_dismissing_press: bool,
    /// Block input to everything below while open (alerts, sheets): outside presses go to
    /// the overlay's root, keys go to the overlay even if focus is elsewhere, shortcuts
    /// registered on the tree do not fire, and Tab cycles inside the overlay.
    pub modal: bool,
    /// The pointer goes through to what is below, as if the overlay were not there
    /// (tooltips).
    pub pass_through: bool,
}

impl OverlayOptions {
    pub const MENU: Self = Self {
        light_dismiss: true,
        consume_dismissing_press: true,
        modal: false,
        pass_through: false,
    };
    pub const POPOVER: Self = Self {
        light_dismiss: true,
        consume_dismissing_press: false,
        modal: false,
        pass_through: false,
    };
    pub const PASSIVE: Self = Self {
        light_dismiss: false,
        consume_dismissing_press: false,
        modal: false,
        pass_through: false,
    };
    pub const MODAL: Self = Self {
        light_dismiss: false,
        consume_dismissing_press: false,
        modal: true,
        pass_through: false,
    };
    pub const TOOLTIP: Self = Self {
        light_dismiss: false,
        consume_dismissing_press: false,
        modal: false,
        pass_through: true,
    };
}

/// Emitted (with the overlay's root as the source) when an overlay closes by light
/// dismiss, or when a [`crate::widgets::Popover::closes_on_click`] popover closes after a
/// click on a button in it. The overlay has already been removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayDismissed;

/// Top-left corner for an overlay of `size` in a window of `viewport`.
pub fn place(placement: Placement, size: Size, viewport: Size) -> Point {
    let (x, y) = match placement {
        Placement::At(p) => (p.x, p.y),
        Placement::Below(anchor) => {
            let below = anchor.bottom() + ANCHOR_GAP;
            let above = anchor.y - ANCHOR_GAP - size.height;
            let y = if below + size.height > viewport.height && above >= 0.0 {
                above
            } else {
                below
            };
            (anchor.x, y)
        }
        Placement::Beside(anchor) => {
            let right = anchor.right();
            let left = anchor.x - size.width;
            let x = if right + size.width > viewport.width && left >= 0.0 {
                left
            } else {
                right
            };
            (x, anchor.y)
        }
        Placement::Center => (
            (viewport.width - size.width) / 2.0,
            (viewport.height - size.height) / 3.0,
        ),
        Placement::Edge { anchor, edge } => {
            let below = anchor.bottom() + ANCHOR_GAP;
            let above = anchor.y - ANCHOR_GAP - size.height;
            let right = anchor.right() + ANCHOR_GAP;
            let left = anchor.x - ANCHOR_GAP - size.width;
            let fits_below = below + size.height <= viewport.height;
            let fits_right = right + size.width <= viewport.width;
            let center_x = anchor.x + (anchor.width - size.width) / 2.0;
            let center_y = anchor.y + (anchor.height - size.height) / 2.0;
            match edge {
                Edge::Bottom if fits_below || above < 0.0 => (center_x, below),
                Edge::Top if above >= 0.0 || !fits_below => (center_x, above),
                Edge::Bottom | Edge::Top => (center_x, if fits_below { below } else { above }),
                Edge::Right if fits_right || left < 0.0 => (right, center_y),
                Edge::Left if left >= 0.0 || !fits_right => (left, center_y),
                Edge::Right | Edge::Left => (if fits_right { right } else { left }, center_y),
            }
        }
        Placement::Sheet { top } => ((viewport.width - size.width) / 2.0, top),
        Placement::Drawer { edge, top } => match edge {
            Edge::Left | Edge::Top => (0.0, top),
            Edge::Right => (viewport.width - size.width, top),
            Edge::Bottom => (0.0, viewport.height - size.height),
        },
        Placement::Corner { corner, inset } => {
            let left = inset;
            let right = viewport.width - inset - size.width;
            let top = inset;
            let bottom = viewport.height - inset - size.height;
            match corner {
                WindowCorner::TopLeft => (left, top),
                WindowCorner::TopRight => (right, top),
                WindowCorner::BottomLeft => (left, bottom),
                WindowCorner::BottomRight => (right, bottom),
            }
        }
    };
    Point::new(
        x.min(viewport.width - size.width).max(0.0),
        y.min(viewport.height - size.height).max(0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_placement_centers_on_the_anchor_and_flips_when_there_is_no_room() {
        let size = Size::new(100.0, 80.0);
        let anchor = Rect::new(150.0, 100.0, 40.0, 20.0);
        let at = |edge| place(Placement::Edge { anchor, edge }, size, VIEW);
        assert_eq!(at(Edge::Bottom), Point::new(120.0, 124.0));
        assert_eq!(at(Edge::Top), Point::new(120.0, 100.0 - ANCHOR_GAP - 80.0));
        assert_eq!(at(Edge::Right), Point::new(194.0, 70.0));
        assert_eq!(at(Edge::Left), Point::new(150.0 - ANCHOR_GAP - 100.0, 70.0));
        // No room above: below instead; no room on the right: left instead.
        let corner = Rect::new(340.0, 10.0, 40.0, 20.0);
        let at = |edge| {
            place(
                Placement::Edge {
                    anchor: corner,
                    edge,
                },
                size,
                VIEW,
            )
        };
        assert_eq!(at(Edge::Top).y, 34.0);
        assert_eq!(at(Edge::Right).x, 340.0 - ANCHOR_GAP - 100.0);
        assert_eq!(
            place(Placement::Sheet { top: 28.0 }, size, VIEW),
            Point::new(150.0, 28.0)
        );
    }

    const VIEW: Size = Size::new(400.0, 300.0);

    #[test]
    fn drawers_sit_against_their_edge_below_the_top() {
        let side = Size::new(100.0, 270.0);
        let at = |edge, size| place(Placement::Drawer { edge, top: 30.0 }, size, VIEW);
        assert_eq!(at(Edge::Left, side), Point::new(0.0, 30.0));
        assert_eq!(at(Edge::Right, side), Point::new(300.0, 30.0));
        let across = Size::new(400.0, 80.0);
        assert_eq!(at(Edge::Top, across), Point::new(0.0, 30.0));
        assert_eq!(at(Edge::Bottom, across), Point::new(0.0, 220.0));
    }

    #[test]
    fn below_flips_above_when_there_is_no_room() {
        let size = Size::new(100.0, 80.0);
        let top = Rect::new(10.0, 10.0, 50.0, 20.0);
        assert_eq!(
            place(Placement::Below(top), size, VIEW),
            Point::new(10.0, 34.0)
        );
        let bottom = Rect::new(10.0, 250.0, 50.0, 20.0);
        assert_eq!(
            place(Placement::Below(bottom), size, VIEW),
            Point::new(10.0, 250.0 - ANCHOR_GAP - 80.0)
        );
    }

    #[test]
    fn beside_flips_left_and_everything_stays_in_the_window() {
        let size = Size::new(100.0, 80.0);
        let anchor = Rect::new(350.0, 280.0, 40.0, 20.0);
        // No room on the right: goes left; no room below: shifted up.
        assert_eq!(
            place(Placement::Beside(anchor), size, VIEW),
            Point::new(250.0, 220.0)
        );
        assert_eq!(
            place(Placement::At(Point::new(390.0, -5.0)), size, VIEW),
            Point::new(300.0, 0.0)
        );
        assert_eq!(
            place(Placement::Center, size, VIEW),
            Point::new(150.0, 220.0 / 3.0)
        );
    }

    #[test]
    fn corners_are_inset_from_the_window_edges() {
        let size = Size::new(100.0, 80.0);
        let at = |corner| {
            place(
                Placement::Corner {
                    corner,
                    inset: 10.0,
                },
                size,
                VIEW,
            )
        };
        assert_eq!(at(WindowCorner::TopLeft), Point::new(10.0, 10.0));
        assert_eq!(at(WindowCorner::TopRight), Point::new(290.0, 10.0));
        assert_eq!(at(WindowCorner::BottomLeft), Point::new(10.0, 210.0));
        assert_eq!(at(WindowCorner::BottomRight), Point::new(290.0, 210.0));
    }
}
