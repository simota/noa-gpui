//! Taffy caches results per node, so after [`LayoutTree::mark_dirty`] only the dirty
//! nodes and their ancestors are recomputed.
//!
//! Every node is the containing block of its absolutely positioned children: an
//! `Position::Absolute` child's percentages and insets refer to its parent, whatever the
//! parent's own `position` is (Taffy follows CSS, where only a positioned ancestor counts).
//! `Position::Fixed` is treated as `Absolute` for the same reason: a node's layout is
//! always relative to its parent.

pub use taffy;
pub mod style {
    pub use taffy::geometry::{Line, Point, Rect, Size};
    pub use taffy::style::*;
    pub use taffy::style_helpers::*;
}

pub use taffy::{AvailableSpace, NodeId as LayoutId, Style};

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use noa_gpui_scene::{Rect, Size};
use taffy::TaffyTree;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NodeLayout {
    /// Position relative to the parent's border box, and the node's size (logical pixels).
    pub rect: Rect,
    /// Extent of the node's content measured from its scroll origin. Larger than
    /// `rect`'s size when the content overflows.
    pub content_size: Size,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasureInput {
    /// Dimensions already fixed by the style or the parent (border box, including the
    /// node's padding).
    pub known: taffy::Size<Option<f32>>,
    /// Space for the content (inside the node's padding). A known dimension shows up
    /// here as a definite size minus the padding.
    pub available: taffy::Size<AvailableSpace>,
}

impl MeasureInput {
    /// The width content should wrap at, if any (content box).
    pub fn wrap_width(&self) -> Option<f32> {
        match self.available.width {
            AvailableSpace::Definite(w) => Some(w),
            AvailableSpace::MinContent => Some(0.0),
            AvailableSpace::MaxContent => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

impl Axis {
    fn of<T: Copy>(self, size: taffy::Size<T>) -> T {
        match self {
            Axis::X => size.width,
            Axis::Y => size.height,
        }
    }

    fn is_main_of(self, container: &Style) -> bool {
        let row = matches!(
            container.flex_direction,
            taffy::FlexDirection::Row | taffy::FlexDirection::RowReverse
        );
        row == (self == Axis::X)
    }
}

/// Make the node a containing block for absolutely positioned children (`Relative` lays
/// out exactly like `Static` while no inset is set), and place `Fixed` boxes against
/// their parent like `Absolute` ones (see the crate documentation).
fn contain_absolute(mut style: Style) -> Style {
    style.position = match style.position {
        taffy::Position::Static => taffy::Position::Relative,
        taffy::Position::Fixed => taffy::Position::Absolute,
        other => other,
    };
    style
}

/// Larger sibling lists are not inspected; nodes in them are treated as content-sized.
const MAX_FLEX_SIBLINGS: usize = 64;
/// Ancestor chains deeper than this are treated as content-sized.
const MAX_DEPTH: usize = 32;

/// Hashes a [`LayoutId`] (a `u64` made by the tree, not by anyone outside) with one
/// multiplication. Taffy's own map of measure keys hashes with SipHash, which costs
/// more than the rest of looking up a leaf's key on every measure.
#[derive(Default)]
struct IdHasher(u64);

impl Hasher for IdHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    fn write_u64(&mut self, value: u64) {
        // Fibonacci hashing: spreads the slot index in the low bits into the high bits
        // too, which the map uses as well.
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }

    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }
}

/// A Taffy tree whose measured leaves carry a key `K` identifying the widget to measure.
pub struct LayoutTree<K> {
    taffy: TaffyTree,
    /// The measure keys of measured leaves. Kept here rather than as Taffy's node
    /// contexts, which it looks up through a SipHash map.
    keys: HashMap<LayoutId, K, BuildHasherDefault<IdHasher>>,
}

impl<K> Default for LayoutTree<K> {
    fn default() -> Self {
        let mut taffy = TaffyTree::new();
        // Taffy's pixel rounding walks the whole tree after every compute, which turns
        // a one-node change into O(tree) work. Layouts stay fractional; quads are
        // antialiased by their SDF shader, so no seams appear.
        taffy.disable_rounding();
        Self {
            taffy,
            keys: HashMap::default(),
        }
    }
}

impl<K> LayoutTree<K> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node. Nodes with a `measure_key` are leaves whose size comes from the
    /// measure function passed to [`LayoutTree::compute`].
    pub fn add(&mut self, style: Style, measure_key: Option<K>) -> LayoutId {
        let node = (self.taffy.new_leaf(contain_absolute(style)))
            .expect("taffy node allocation cannot fail");
        if let Some(key) = measure_key {
            self.keys.insert(node, key);
        }
        node
    }

    pub fn set_style(&mut self, node: LayoutId, style: Style) {
        self.taffy
            .set_style(node, contain_absolute(style))
            .expect("live layout node");
    }

    pub fn set_children(&mut self, node: LayoutId, children: &[LayoutId]) {
        self.taffy
            .set_children(node, children)
            .expect("live layout node");
    }

    pub fn add_child(&mut self, parent: LayoutId, child: LayoutId) {
        self.taffy
            .add_child(parent, child)
            .expect("live layout node");
    }

    pub fn remove(&mut self, node: LayoutId) {
        self.taffy.remove(node).expect("live layout node");
        self.keys.remove(&node);
    }

    /// Invalidate `node`'s cached layout and that of its ancestors.
    pub fn mark_dirty(&mut self, node: LayoutId) {
        self.taffy.mark_dirty(node).expect("live layout node");
    }

    pub fn is_dirty(&self, node: LayoutId) -> bool {
        self.taffy.dirty(node).expect("live layout node")
    }

    /// Lay out the tree under `root` within `available` logical pixels.
    ///
    /// `measure` is called for measured leaves whose cached size cannot be reused.
    pub fn compute(
        &mut self,
        root: LayoutId,
        available: Size,
        measure: impl FnMut(&mut K, MeasureInput) -> Size,
    ) {
        let space = taffy::Size {
            width: AvailableSpace::Definite(available.width),
            height: AvailableSpace::Definite(available.height),
        };
        self.compute_in(root, space, measure);
    }

    /// Lay out `root` at its content size (max-content on both axes) instead of filling
    /// an available area. Used for popups, which size to what they contain.
    pub fn compute_fit_content(
        &mut self,
        root: LayoutId,
        measure: impl FnMut(&mut K, MeasureInput) -> Size,
    ) {
        let space = taffy::Size {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        };
        self.compute_in(root, space, measure);
    }

    fn compute_in(
        &mut self,
        root: LayoutId,
        space: taffy::Size<AvailableSpace>,
        mut measure: impl FnMut(&mut K, MeasureInput) -> Size,
    ) {
        let Self { taffy, keys } = self;
        taffy
            .compute_layout_with_measure(root, space, |inputs, node, _, style| {
                let key = keys.get_mut(&node);
                taffy::compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known, available| match key {
                        Some(key) => {
                            let size = measure(key, MeasureInput { known, available });
                            taffy::Size {
                                width: known.width.unwrap_or(size.width),
                                height: known.height.unwrap_or(size.height),
                            }
                        }
                        None => taffy::Size::ZERO,
                    },
                )
            })
            .expect("live layout root");
    }

    /// Whether `node`'s size is independent of its content, so changes inside it can be
    /// laid out with [`LayoutTree::compute_subtree`] without touching the rest of the tree.
    ///
    /// Each axis must be fixed by something other than the node's content: a length, a
    /// percentage or flex-stretch of a parent that is itself fixed, or flexing with a fixed
    /// basis and minimum among fixed siblings. Conservative: unknown cases count as
    /// content-sized.
    pub fn is_boundary(&self, node: LayoutId) -> bool {
        let style = self.style(node);
        style.display != taffy::Display::None
            && style.position != taffy::Position::Absolute
            && style.aspect_ratio.is_none()
            && self.fixed(node, Axis::X, 0)
            && self.fixed(node, Axis::Y, 0)
    }

    fn style(&self, node: LayoutId) -> &Style {
        self.taffy.style(node).expect("live layout node")
    }

    fn fixed(&self, node: LayoutId, axis: Axis, depth: usize) -> bool {
        use taffy::style::ExpandedDimension as D;
        if depth > MAX_DEPTH {
            return false;
        }
        let style = self.style(node);
        let parent = self.taffy.parent(node);
        if let Some(parent) = parent {
            let container = self.style(parent);
            if container.display == taffy::Display::Flex
                && axis.is_main_of(container)
                && (style.flex_grow != 0.0 || style.flex_shrink != 0.0)
            {
                return self.flex_main_fixed(node, parent, axis, depth);
            }
        }
        match axis.of(style.size).expand() {
            D::Length(_) => true,
            D::Percent(_) => parent.is_none_or(|p| self.fixed(p, axis, depth + 1)),
            D::Auto => parent
                .is_some_and(|p| self.stretched(node, p, axis) && self.fixed(p, axis, depth + 1)),
            _ => false,
        }
    }

    /// Cross-axis stretch: the size comes from the flex container, not the content.
    fn stretched(&self, node: LayoutId, parent: LayoutId, axis: Axis) -> bool {
        use taffy::style::AlignItemsKeyword as K;
        let (style, container) = (self.style(node), self.style(parent));
        let align = style.align_self.unwrap_or(container.align_items);
        let margin_auto = match axis {
            Axis::X => style.margin.left.is_auto() || style.margin.right.is_auto(),
            Axis::Y => style.margin.top.is_auto() || style.margin.bottom.is_auto(),
        };
        container.display == taffy::Display::Flex
            // Wrapped lines take their cross sizes from their contents before any
            // spare space is distributed, even in a fixed-size container.
            && container.flex_wrap == taffy::FlexWrap::NoWrap
            && !axis.is_main_of(container)
            && matches!(align.keyword, K::Stretch | K::Normal)
            && !margin_auto
    }

    /// A flexing item's main size depends on its basis, its minimum, the container and
    /// its siblings: all must be fixed.
    fn flex_main_fixed(&self, node: LayoutId, parent: LayoutId, axis: Axis, depth: usize) -> bool {
        let flex_inputs_fixed = |n: LayoutId| {
            use taffy::style::{ExpandedDimension as D, ExpandedLengthPercentageAuto as M};
            let s = self.style(n);
            matches!(s.flex_basis.expand(), D::Length(_) | D::Percent(_))
                && matches!(axis.of(s.min_size).expand(), M::Length(_) | M::Percent(_))
        };
        let siblings = self.taffy.children(parent).expect("live layout node");
        siblings.len() <= MAX_FLEX_SIBLINGS
            && self.fixed(parent, axis, depth + 1)
            && siblings.iter().all(|&sibling| {
                let s = self.style(sibling);
                if s.position == taffy::Position::Absolute || s.display == taffy::Display::None {
                    true
                } else if s.flex_grow != 0.0 || s.flex_shrink != 0.0 || sibling == node {
                    flex_inputs_fixed(sibling)
                } else {
                    self.fixed(sibling, axis, depth + 1)
                }
            })
    }

    /// Re-lay out only the subtree under `node`, keeping `node` at `size`.
    ///
    /// Valid when `node` is a layout boundary (see [`LayoutTree::is_boundary`]) and nothing
    /// outside its subtree changed: its size cannot depend on its content, so the rest of
    /// the tree is unaffected. `node`'s own position is not recomputed; keep the old one.
    pub fn compute_subtree(
        &mut self,
        node: LayoutId,
        size: Size,
        mut measure: impl FnMut(&mut K, MeasureInput) -> Size,
    ) {
        // Lay the node out as a root with exactly its current size, then restore its
        // style. Percentages in its own size are resolved against its parent, which is not
        // part of this computation, hence the temporary fixed lengths.
        let style = self.style(node).clone();
        let fixed = Style {
            size: taffy::Size {
                width: taffy::Dimension::length(size.width),
                height: taffy::Dimension::length(size.height),
            },
            margin: taffy::Rect::zero(),
            ..style.clone()
        };
        self.taffy.set_style(node, fixed).expect("live layout node");
        self.compute(node, size, &mut measure);
        // Restoring the style marks the node dirty again; its children keep their caches.
        self.taffy.set_style(node, style).expect("live layout node");
    }

    pub fn layout(&self, node: LayoutId) -> NodeLayout {
        let layout = self.taffy.layout(node).expect("live layout node");
        NodeLayout {
            rect: Rect::new(
                layout.location.x,
                layout.location.y,
                layout.size.width,
                layout.size.height,
            ),
            content_size: Size::new(
                layout.scrollable_overflow_rect.right.max(layout.size.width),
                layout
                    .scrollable_overflow_rect
                    .bottom
                    .max(layout.size.height),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::style::*;
    use super::{LayoutId, LayoutTree, MeasureInput, Style};

    fn fixed(w: f32, h: f32) -> Style {
        Style {
            size: Size {
                width: length(w),
                height: length(h),
            },
            ..Default::default()
        }
    }

    #[test]
    fn flex_row_places_children() {
        let mut tree: LayoutTree<u32> = LayoutTree::new();
        let a = tree.add(fixed(10.0, 10.0), None);
        let text = tree.add(Style::default(), Some(7));
        let root = tree.add(
            Style {
                flex_direction: FlexDirection::Row,
                gap: Size {
                    width: length(4.0),
                    height: zero(),
                },
                padding: Rect::length(2.0),
                ..Default::default()
            },
            None,
        );
        tree.set_children(root, &[a, text]);
        tree.compute(root, noa_gpui_scene::Size::new(100.0, 50.0), |key, _| {
            assert_eq!(*key, 7);
            noa_gpui_scene::Size::new(30.0, 12.0)
        });

        assert_eq!(
            tree.layout(a).rect,
            noa_gpui_scene::Rect::new(2.0, 2.0, 10.0, 10.0)
        );
        let t = tree.layout(text).rect;
        assert_eq!((t.x, t.y, t.width), (16.0, 2.0, 30.0));
    }

    #[test]
    fn each_measured_leaf_gets_its_key_after_removals_and_reuse() {
        let mut tree: LayoutTree<u32> = LayoutTree::new();
        let column = Style {
            flex_direction: FlexDirection::Column,
            ..Default::default()
        };
        let root = tree.add(column, None);
        // Leaf `i` is measured as `i` pixels tall; unkeyed leaves are not measured.
        let mut leaves: Vec<_> = (1..=4)
            .map(|i| tree.add(Style::default(), Some(i)))
            .collect();
        let blank = tree.add(fixed(5.0, 3.0), None);
        let measure = |tree: &mut LayoutTree<u32>, children: &[LayoutId]| {
            tree.set_children(root, children);
            tree.mark_dirty(root);
            let mut keys = Vec::new();
            tree.compute(root, noa_gpui_scene::Size::new(100.0, 100.0), |key, _| {
                keys.push(*key);
                noa_gpui_scene::Size::new(10.0, *key as f32)
            });
            keys.sort_unstable();
            keys.dedup();
            keys
        };
        let mut children = leaves.clone();
        children.push(blank);
        assert_eq!(measure(&mut tree, &children), [1, 2, 3, 4]);
        for (leaf, height) in leaves.iter().zip(1..) {
            assert_eq!(tree.layout(*leaf).rect.height, height as f32);
        }

        // Removed leaves take their keys with them; a node made afterwards (possibly in
        // a removed one's slot) has its own key, or none.
        tree.set_children(root, &[]);
        tree.remove(leaves.remove(1));
        tree.remove(blank);
        let reused = tree.add(Style::default(), Some(9));
        let unkeyed = tree.add(fixed(5.0, 3.0), None);
        leaves.push(reused);
        let mut children = leaves.clone();
        children.push(unkeyed);
        assert_eq!(measure(&mut tree, &children), [1, 3, 4, 9]);
        assert_eq!(tree.layout(reused).rect.height, 9.0);
        assert_eq!(tree.layout(unkeyed).rect.height, 3.0);
    }

    #[test]
    fn clean_subtrees_are_not_remeasured() {
        let mut tree: LayoutTree<u32> = LayoutTree::new();
        let leaves: Vec<_> = (0..3)
            .map(|i| tree.add(Style::default(), Some(i)))
            .collect();
        let rows: Vec<_> = leaves
            .iter()
            .map(|&leaf| {
                let row = tree.add(Style::default(), None);
                tree.set_children(row, &[leaf]);
                row
            })
            .collect();
        let root = tree.add(
            Style {
                flex_direction: FlexDirection::Column,
                ..Default::default()
            },
            None,
        );
        tree.set_children(root, &rows);

        let mut measured = Vec::new();
        let run = |tree: &mut LayoutTree<u32>, measured: &mut Vec<u32>| {
            measured.clear();
            tree.compute(root, noa_gpui_scene::Size::new(100.0, 100.0), |key, _| {
                measured.push(*key);
                noa_gpui_scene::Size::new(20.0, 10.0)
            });
        };
        run(&mut tree, &mut measured);
        assert!(measured.contains(&0) && measured.contains(&2));

        tree.mark_dirty(leaves[1]);
        run(&mut tree, &mut measured);
        assert!(measured.iter().all(|&k| k == 1), "measured {measured:?}");
        assert!(!measured.is_empty());
    }

    #[test]
    fn boundaries_follow_where_sizes_come_from() {
        let mut tree: LayoutTree<u32> = LayoutTree::new();
        let rigid = |w, h| Style {
            flex_shrink: 0.0,
            ..fixed(w, h)
        };
        let root = tree.add(
            Style {
                size: Size {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            },
            None,
        );
        let sidebar = tree.add(rigid(100.0, 50.0), None);
        let list = tree.add(
            Style {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                flex_basis: length(0.0),
                min_size: Size {
                    width: length(0.0),
                    height: auto(),
                },
                ..Default::default()
            },
            None,
        );
        let row = tree.add(
            Style {
                size: Size {
                    width: percent(1.0),
                    height: length(20.0),
                },
                flex_shrink: 0.0,
                ..Default::default()
            },
            None,
        );
        let stretched_row = tree.add(
            Style {
                size: Size {
                    width: auto(),
                    height: length(20.0),
                },
                flex_shrink: 0.0,
                ..Default::default()
            },
            None,
        );
        let content_row = tree.add(Style::default(), None);
        tree.set_children(root, &[sidebar, list]);
        tree.set_children(list, &[row, stretched_row, content_row]);

        assert!(tree.is_boundary(sidebar));
        assert!(
            tree.is_boundary(list),
            "flex 1 1 0 among fixed siblings, stretched height"
        );
        assert!(tree.is_boundary(row), "100% of a fixed-width list");
        assert!(tree.is_boundary(stretched_row), "cross-axis stretch");
        assert!(!tree.is_boundary(content_row), "height comes from content");

        // Percentages of a content-sized parent are content-sized too.
        let shrink_wrapped = tree.add(Style::default(), None);
        let inner = tree.add(
            Style {
                size: Size {
                    width: percent(1.0),
                    height: length(20.0),
                },
                flex_shrink: 0.0,
                ..Default::default()
            },
            None,
        );
        tree.set_children(shrink_wrapped, &[inner]);
        assert!(!tree.is_boundary(inner));
    }

    #[test]
    fn subtree_layout_matches_full_layout() {
        let mut tree: LayoutTree<u32> = LayoutTree::new();
        let mut widths = vec![10.0_f32; 3];
        let leaves: Vec<_> = (0..3)
            .map(|i| tree.add(Style::default(), Some(i)))
            .collect();
        let rows: Vec<_> = leaves
            .iter()
            .map(|&leaf| {
                let row = tree.add(
                    Style {
                        size: Size {
                            width: percent(1.0),
                            height: length(20.0),
                        },
                        flex_shrink: 0.0,
                        justify_content: JustifyContent::CENTER,
                        ..Default::default()
                    },
                    None,
                );
                tree.set_children(row, &[leaf]);
                row
            })
            .collect();
        let root = tree.add(
            Style {
                flex_direction: FlexDirection::Column,
                padding: Rect::length(5.0),
                size: Size {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            },
            None,
        );
        tree.set_children(root, &rows);
        assert!(rows.iter().all(|&r| tree.is_boundary(r)));
        let viewport = noa_gpui_scene::Size::new(200.0, 100.0);
        let measure = |w: &[f32]| {
            let w = w.to_vec();
            move |key: &mut u32, _: MeasureInput| noa_gpui_scene::Size::new(w[*key as usize], 10.0)
        };
        tree.compute(root, viewport, measure(&widths));
        let row_before = tree.layout(rows[1]);

        widths[1] = 50.0;
        tree.mark_dirty(leaves[1]);
        let size = noa_gpui_scene::Size::new(row_before.rect.width, row_before.rect.height);
        tree.compute_subtree(rows[1], size, measure(&widths));
        let partial = tree.layout(leaves[1]);

        tree.compute(root, viewport, measure(&widths));
        assert_eq!(partial, tree.layout(leaves[1]));
        assert_eq!(
            tree.layout(rows[1]),
            row_before,
            "the boundary itself is unchanged"
        );
        assert_eq!(partial.rect.width, 50.0);
        assert_eq!(partial.rect.x, (190.0 - 50.0) / 2.0);
    }

    #[test]
    fn absolute_children_are_placed_in_their_parent() {
        let mut tree: LayoutTree<u32> = LayoutTree::new();
        let root = tree.add(
            Style {
                padding: Rect::length(10.0),
                align_items: AlignItems::START,
                ..fixed(800.0, 600.0)
            },
            None,
        );
        // A content-sized item (not `position: relative`) with a frame over it.
        let item = tree.add(
            Style {
                margin: Rect {
                    left: length(30.0),
                    ..Rect::zero()
                },
                ..Default::default()
            },
            None,
        );
        let content = tree.add(fixed(60.0, 30.0), None);
        let frame = tree.add(
            Style {
                position: Position::Absolute,
                size: Size {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            },
            None,
        );
        let pinned = tree.add(
            Style {
                position: Position::Fixed,
                inset: Rect {
                    left: length(5.0),
                    top: length(5.0),
                    right: auto(),
                    bottom: auto(),
                },
                ..fixed(4.0, 4.0)
            },
            None,
        );
        tree.set_children(item, &[content, frame, pinned]);
        tree.set_children(root, &[item]);
        tree.compute(root, noa_gpui_scene::Size::new(800.0, 600.0), |_, _| {
            noa_gpui_scene::Size::default()
        });

        assert_eq!(
            tree.layout(item).rect,
            noa_gpui_scene::Rect::new(40.0, 10.0, 60.0, 30.0)
        );
        assert_eq!(
            tree.layout(frame).rect,
            noa_gpui_scene::Rect::new(0.0, 0.0, 60.0, 30.0),
            "100% of the item, at the item's origin"
        );
        assert_eq!(
            tree.layout(pinned).rect,
            noa_gpui_scene::Rect::new(5.0, 5.0, 4.0, 4.0)
        );
    }

    #[test]
    fn content_size_reports_overflow() {
        let mut tree: LayoutTree<u32> = LayoutTree::new();
        let child = tree.add(fixed(50.0, 300.0), None);
        let scroller = tree.add(
            Style {
                overflow: Point {
                    x: Overflow::Visible,
                    y: Overflow::Scroll,
                },
                size: Size {
                    width: length(50.0),
                    height: length(100.0),
                },
                scrollbar_width: 0.0,
                ..Default::default()
            },
            None,
        );
        tree.set_children(scroller, &[child]);
        tree.compute(scroller, noa_gpui_scene::Size::new(200.0, 200.0), |_, _| {
            noa_gpui_scene::Size::default()
        });
        let layout = tree.layout(scroller);
        assert_eq!(layout.rect.height, 100.0);
        assert_eq!(layout.content_size.height, 300.0);
    }
}
