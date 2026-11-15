//! Widgets live in an arena addressed by generational [`WidgetId`]s, so a stale id can
//! never reach a widget that reused its slot. Each frame [`UiTree::prepare`] does only
//! the work recorded by [`Dirty`] flags:
//!
//! 1. **layout**: push changed styles to Taffy and recompute; Taffy reuses cached
//!    results for clean subtrees, and results are read back only where they can change,
//! 2. **paint**: regenerate draw commands for widgets that asked for it or resized,
//! 3. **composite**: concatenate every widget's cached commands, translated to window
//!    coordinates, into one display list, skipping subtrees outside the visible clip.
//!
//! When nothing is dirty, `prepare` returns `None` and no frame needs to be drawn.

use std::any::Any;
use std::collections::HashSet;

use noa_gpui_layout::{LayoutId, LayoutTree, NodeLayout};
use noa_gpui_scene::{DisplayList, DrawCommand, Point, Rect, Size};
use noa_gpui_text::TextStore;

use crate::clipboard::{Clipboard, MemoryClipboard};
use crate::event::{Cursor, DragData, Event, Key, Message, PointerButton};
use crate::invalidation::Dirty;
use crate::overlay::{OverlayDismissed, OverlayOptions, Placement, place};
use crate::shortcut::{Shortcut, ShortcutPressed};
use crate::styling::{StyleMeta, StyleNode, StylePatch, StyleSheet, StyleState, StyleTree};
use crate::theme::Theme;
use crate::view_state::ViewState;
use crate::widget::{EventCx, MeasureCx, PaintCx, Requests, UpdateCx, Widget};

pub const ACCESS_ROOT: accesskit::NodeId = accesskit::NodeId(0);

/// Item ids hold a flag, an 11-bit generation, a 32-bit slot and a 20-bit item.
const ACCESS_ITEM: u64 = 1 << 63;
const ACCESS_ITEM_BITS: u32 = 20;
const ACCESS_ITEM_LIMIT: usize = 1 << ACCESS_ITEM_BITS;
const ACCESS_GENERATION_SHIFT: u32 = ACCESS_ITEM_BITS + 32;
const ACCESS_GENERATIONS: u32 = 1 << (63 - ACCESS_GENERATION_SHIFT);

/// # Panics
/// If `item` is at least `1 << 20`.
pub fn access_item_id(id: WidgetId, item: usize) -> accesskit::NodeId {
    assert!(
        item < ACCESS_ITEM_LIMIT,
        "accessibility item index exceeds 20 bits"
    );
    accesskit::NodeId(
        ACCESS_ITEM
            | ((id.generation as u64) << ACCESS_GENERATION_SHIFT)
            | ((id.index as u64) << ACCESS_ITEM_BITS)
            | item as u64,
    )
}

/// Never [`ACCESS_ROOT`].
pub fn access_id(id: WidgetId) -> accesskit::NodeId {
    accesskit::NodeId(((id.generation as u64) << 32 | id.index as u64) + 1)
}

/// Ids are never reused: removing a widget invalidates its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidgetId {
    index: u32,
    generation: u32,
}

struct Node {
    widget: Box<dyn Widget>,
    style_meta: Option<Box<StyleMeta>>,
    inline_style: Option<Box<StylePatch>>,
    resolved_style: Option<Box<StylePatch>>,
    access_label: Option<Box<str>>,
    autosave: Option<Box<str>>,
    parent: Option<WidgetId>,
    /// Position among the parent's children, kept by [`UiTree::children_changed`].
    sibling_index: u32,
    children: Vec<WidgetId>,
    layout_id: LayoutId,
    layout: NodeLayout,
    /// Everything this subtree may draw, relative to the widget's origin (for culling).
    subtree_bounds: Rect,
    /// Children's bounds are ordered top to bottom (tops and bottoms non-decreasing), so
    /// the visible ones can be found by binary search.
    children_sorted_y: bool,
    /// In the widget's coordinates.
    under: Vec<DrawCommand>,
    over: Vec<DrawCommand>,
    /// `LAYOUT`: style must be re-read and the widget re-measured.
    /// `PAINT`: `under`/`over` are stale.
    dirty: Dirty,
    /// Layout results under this node may have changed in the current layout pass.
    layout_touched: bool,
    /// A widget is enabled only if all its ancestors are.
    enabled: bool,
    hidden: bool,
    hidden_by_parent: bool,
}

impl Node {
    fn is_hidden(&self) -> bool {
        self.hidden || self.hidden_by_parent || self.style().is_display_none()
    }

    fn style(&self) -> &StylePatch {
        self.resolved_style.as_deref().unwrap_or(&StylePatch::EMPTY)
    }

    fn clips_children(&self) -> bool {
        self.widget.clips_children() || self.style().clips()
    }

    fn layout_style(&self) -> noa_gpui_layout::Style {
        let mut style = widget_layout_style(self.widget.as_ref());
        if let Some(patch) = &self.resolved_style {
            patch.apply_layout(&mut style);
        }
        if self.is_hidden() {
            style.display = noa_gpui_layout::style::Display::None;
        }
        style
    }
}

/// The layout style of `widget` before stylesheets. A widget that clips its children
/// clips them in layout too: what it cuts off is not overflow that a scroll view around
/// it makes room for (`Clip` changes nothing else in layout).
fn widget_layout_style(widget: &dyn Widget) -> noa_gpui_layout::Style {
    let mut style = widget.style();
    if widget.clips_children() {
        for axis in [&mut style.overflow.x, &mut style.overflow.y] {
            if *axis == noa_gpui_layout::style::Overflow::Visible {
                *axis = noa_gpui_layout::style::Overflow::Clip;
            }
        }
    }
    style
}

pub struct Subtree {
    widget: Box<dyn Widget>,
    children: Vec<Subtree>,
    style_meta: Option<Box<StyleMeta>>,
    inline_style: Option<Box<StylePatch>>,
    access_label: Option<Box<str>>,
    autosave: Option<Box<str>>,
    enabled: bool,
    hidden: bool,
}

struct Slot {
    generation: u32,
    node: Option<Node>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpdateStats {
    pub layout: bool,
    /// Layout boundaries re-laid out on their own instead of laying out the whole tree
    /// (0 when the whole tree was laid out).
    pub partial_layouts: usize,
    pub measured: usize,
    pub layouts_read: usize,
    pub repainted: usize,
    pub commands: usize,
    pub culled: usize,
}

pub struct UiTree {
    slots: Vec<Slot>,
    free: Vec<u32>,
    len: usize,
    root: Option<WidgetId>,
    layout: LayoutTree<WidgetId>,
    viewport: Option<Size>,

    pending: Dirty,
    layout_queue: Vec<WidgetId>,
    paint_queue: Vec<WidgetId>,
    list: DisplayList,

    focused: Option<WidgetId>,
    hovered: Option<WidgetId>,
    captured: Option<WidgetId>,
    /// The last press or key came from the pointer: focus it moves shows no ring.
    pointer_input: bool,
    focus_visible: bool,
    /// Parents whose list of children changed while selectors depend on positions.
    restyle_children: Vec<WidgetId>,
    /// Where the pointer is, in window space (`None` once it left the window).
    pointer: Option<Point>,
    messages: Vec<Message>,
    stats: UpdateStats,
    /// Widgets were inserted or removed since the last layout: lay out the whole tree.
    structure_changed: bool,
    theme: Theme,
    stylesheet: StyleSheet,
    /// Bottom to top.
    overlays: Vec<Overlay>,
    animating: Vec<WidgetId>,
    /// Seconds on the application's clock.
    now: f64,
    /// Input dispatched since recording started (see [`UiTree::start_recording`]).
    recording: Option<Vec<crate::recording::Step>>,
    shortcuts: Vec<(Shortcut, WidgetId)>,
    /// The item each focus group focused last: `(group, item)`.
    group_focus: Vec<(WidgetId, WidgetId)>,
    clipboard: Box<dyn Clipboard>,
    modifiers: crate::event::Modifiers,
    drag: Option<Drag>,
    drop_accepted: bool,
    shortcut_glyphs: bool,
    tooltip: TooltipState,
    hover_card: HoverCardState,
    timers: Vec<(f64, WidgetId)>,
    /// [`Event::FocusChanged`] not yet delivered, oldest first.
    focus_events: Vec<(WidgetId, bool)>,
    reveal_focus: bool,
    reduce_motion: bool,
    /// States restored with [`UiTree::restore_view_state`], and those of named widgets
    /// removed since.
    view_state: ViewState,
}

/// How long the pointer rests on a [`Widget::hover_card`] widget before its card shows,
/// in seconds.
pub const HOVER_CARD_DELAY: f64 = 0.5;
/// How long a shown card stays after the pointer leaves it and its widget, so the
/// pointer can cross the gap between them.
pub const HOVER_CARD_CLOSE_DELAY: f64 = 0.3;

#[derive(Default)]
struct HoverCardState {
    source: Option<WidgetId>,
    due: Option<f64>,
    panel: Option<WidgetId>,
    close_due: Option<f64>,
}

/// How long the pointer rests before a tooltip shows, in seconds.
pub const TOOLTIP_DELAY: f64 = 0.8;
/// A tooltip shows at once if another one closed less than this long ago, so moving
/// along a row of buttons does not wait each time.
const TOOLTIP_GRACE: f64 = 0.5;

#[derive(Default)]
struct TooltipState {
    source: Option<(WidgetId, String)>,
    pos: Point,
    due: Option<f64>,
    overlay: Option<WidgetId>,
    closed_at: Option<f64>,
}

struct Drag {
    data: DragData,
    /// `None` for drags from outside the app.
    source: Option<WidgetId>,
    target: Option<WidgetId>,
}

impl Default for UiTree {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
            root: None,
            layout: LayoutTree::default(),
            viewport: None,
            pending: Dirty::NONE,
            layout_queue: Vec::new(),
            paint_queue: Vec::new(),
            list: DisplayList::new(),
            focused: None,
            hovered: None,
            captured: None,
            pointer_input: false,
            focus_visible: false,
            restyle_children: Vec::new(),
            pointer: None,
            messages: Vec::new(),
            stats: UpdateStats::default(),
            structure_changed: false,
            theme: Theme::default(),
            stylesheet: StyleSheet::default(),
            overlays: Vec::new(),
            animating: Vec::new(),
            now: 0.0,
            recording: None,
            shortcuts: Vec::new(),
            group_focus: Vec::new(),
            clipboard: Box::new(MemoryClipboard::default()),
            modifiers: Default::default(),
            drag: None,
            drop_accepted: false,
            shortcut_glyphs: cfg!(target_vendor = "apple"),
            tooltip: TooltipState::default(),
            hover_card: HoverCardState::default(),
            timers: Vec::new(),
            focus_events: Vec::new(),
            reveal_focus: false,
            reduce_motion: false,
            view_state: ViewState::default(),
        }
    }
}

struct Overlay {
    root: WidgetId,
    placement: Placement,
    options: OverlayOptions,
    /// Whether the overlay is a [`crate::widgets::Menu`] (the owner gets
    /// [`Event::MenuClosed`] rather than [`Event::PopoverClosed`]).
    owner: Option<WidgetId>,
    /// Whether the overlay is a [`crate::widgets::Menu`] (the owner gets
    /// [`Event::MenuClosed`] rather than [`Event::PopoverClosed`]).
    menu: bool,
    /// Focus before the overlay opened, restored when it closes if the owner does not
    /// take focus (a context menu's owner may be a plain container).
    previous_focus: Option<WidgetId>,
    /// The widget the overlay was opened from (a button showing a popover): a press on
    /// it that dismisses the overlay goes no further, so it toggles the overlay closed
    /// instead of opening it again.
    keeps_focus: bool,
    /// The widget the overlay was opened from (a button showing a popover): a press on
    /// it that dismisses the overlay goes no further, so it toggles the overlay closed
    /// instead of opening it again.
    anchor: Option<WidgetId>,
}

impl UiTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// # Panics
    /// If `parent` is `None` and a root already exists, or `parent` is not alive.
    pub fn insert(&mut self, parent: Option<WidgetId>, widget: impl Widget) -> WidgetId {
        if parent.is_none() {
            assert!(self.root.is_none(), "the tree already has a root");
        }
        let id = self.insert_node(parent, Box::new(widget));
        if parent.is_none() {
            self.root = Some(id);
        }
        id
    }

    fn insert_node(&mut self, parent: Option<WidgetId>, widget: Box<dyn Widget>) -> WidgetId {
        let index = match self.free.pop() {
            Some(index) => index,
            None => {
                self.slots.push(Slot {
                    generation: 0,
                    node: None,
                });
                self.slots.len() as u32 - 1
            }
        };
        let id = WidgetId {
            index,
            generation: self.slots[index as usize].generation,
        };
        let hidden_by_parent = parent.is_some_and(|p| {
            let parent = self.node(p).expect("parent is alive");
            parent.widget.hides_child(parent.children.len())
        });
        let mut style = widget_layout_style(widget.as_ref());
        if hidden_by_parent {
            style.display = noa_gpui_layout::style::Display::None;
        }
        let layout_id = self.layout.add(style, widget.measures().then_some(id));
        self.slots[index as usize].node = Some(Node {
            widget,
            style_meta: None,
            inline_style: None,
            resolved_style: None,
            access_label: None,
            autosave: None,
            parent,
            sibling_index: parent.map_or(0, |p| self.children(p).len() as u32),
            children: Vec::new(),
            layout_id,
            layout: NodeLayout::default(),
            subtree_bounds: Rect::default(),
            children_sorted_y: false,
            under: Vec::new(),
            over: Vec::new(),
            dirty: Dirty::PAINT,
            layout_touched: true,
            enabled: true,
            hidden: false,
            hidden_by_parent,
        });
        self.len += 1;

        if let Some(parent) = parent {
            let parent_node = self.node_mut(parent).expect("parent is alive");
            parent_node.children.push(id);
            let parent_layout = parent_node.layout_id;
            self.layout.add_child(parent_layout, layout_id);
            self.touch_ancestors(parent);
        }
        self.paint_queue.push(id);
        self.pending |= Dirty::LAYOUT | Dirty::PAINT | Dirty::COMPOSITE;
        self.structure_changed = true;
        // With the parent set, selectors can see the node's ancestors and it can inherit.
        // Under sibling-dependent selectors the whole child list is resolved once in the
        // next `prepare`, not once per insert.
        match parent {
            Some(parent) if self.stylesheet.needs_siblings() => {
                self.queue_restyle_children(parent);
            }
            _ => self.refresh_style(id),
        }
        id
    }

    pub fn remove(&mut self, id: WidgetId, text: &mut TextStore) {
        let Some(node) = self.node(id) else { return };
        let (parent, states) = (node.parent, self.interaction_targets());
        if let Some(parent) = node.parent {
            let parent_node = self.node_mut(parent).expect("parent is alive");
            parent_node.children.retain(|&c| c != id);
            self.touch_ancestors(parent);
            self.children_changed(Some(parent));
        } else if self.root == Some(id) {
            self.root = None;
        } else {
            self.overlays.retain(|o| o.root != id);
        }
        self.remove_subtree(id, text);
        self.pending |= Dirty::LAYOUT | Dirty::COMPOSITE;
        self.structure_changed = true;
        self.restyle_after_removal(parent, states);
    }

    /// Keeps the widgets and their state. The other tree must use the same
    /// [`TextStore`]: the widgets keep their text layouts.
    pub fn take_subtree(&mut self, id: WidgetId) -> Option<Subtree> {
        let node = self.node(id)?;
        let (parent, states) = (node.parent, self.interaction_targets());
        if let Some(parent) = node.parent {
            let parent_node = self.node_mut(parent).expect("parent is alive");
            parent_node.children.retain(|&c| c != id);
            self.touch_ancestors(parent);
            self.children_changed(Some(parent));
        } else if self.root == Some(id) {
            self.root = None;
        } else {
            self.overlays.retain(|o| o.root != id);
        }
        let subtree = self.detach(id);
        self.pending |= Dirty::LAYOUT | Dirty::COMPOSITE;
        self.structure_changed = true;
        self.restyle_after_removal(parent, states);
        Some(subtree)
    }

    /// Invalidate widget and item ids together. Retire a slot before the generation
    /// stored in accessibility item ids would wrap.
    fn take_node(&mut self, id: WidgetId) -> Node {
        let slot = &mut self.slots[id.index as usize];
        let node = slot.node.take().expect("alive");
        slot.generation += 1;
        if slot.generation < ACCESS_GENERATIONS {
            self.free.push(id.index);
        }
        self.len -= 1;
        node
    }

    fn detach(&mut self, id: WidgetId) -> Subtree {
        let node = self.take_node(id);
        self.layout.remove(node.layout_id);
        for state in [&mut self.focused, &mut self.hovered, &mut self.captured] {
            if *state == Some(id) {
                *state = None;
            }
        }
        let children = node.children.into_iter().map(|c| self.detach(c)).collect();
        Subtree {
            widget: node.widget,
            children,
            style_meta: node.style_meta,
            inline_style: node.inline_style,
            access_label: node.access_label,
            autosave: node.autosave,
            enabled: node.enabled,
            hidden: node.hidden,
        }
    }

    pub fn insert_subtree(&mut self, parent: Option<WidgetId>, subtree: Subtree) -> WidgetId {
        if parent.is_none() {
            assert!(self.root.is_none(), "the tree already has a root");
        }
        let Subtree {
            widget,
            children,
            style_meta,
            inline_style,
            access_label,
            autosave,
            enabled,
            hidden,
        } = subtree;
        let id = self.insert_node(parent, widget);
        self.node_mut(id).expect("just inserted").access_label = access_label;
        if let Some(name) = autosave {
            self.node_mut(id).expect("just inserted").autosave = Some(name);
        }
        if parent.is_none() {
            self.root = Some(id);
        }
        if let Some(meta) = style_meta {
            self.set_style_meta(id, *meta);
        }
        if let Some(style) = inline_style {
            self.set_inline_style(id, *style);
        }
        for child in children {
            self.insert_subtree(Some(id), child);
        }
        self.set_enabled(id, enabled);
        self.set_hidden(id, hidden);
        id
    }

    /// Only for children whose layout does not depend on their order, such as absolutely
    /// positioned inner windows.
    pub fn bring_to_front(&mut self, id: WidgetId) {
        let Some(parent) = self.parent(id) else {
            return;
        };
        let node = self.node_mut(parent).expect("parent is alive");
        if node.children.last() == Some(&id) {
            return;
        }
        node.children.retain(|&c| c != id);
        node.children.push(id);
        node.children_sorted_y = false;
        self.children_changed(Some(parent));
        self.pending |= Dirty::COMPOSITE;
    }

    fn remove_subtree(&mut self, id: WidgetId, text: &mut TextStore) {
        let mut node = self.take_node(id);
        // A named widget's last state outlives it: a window opened again under the
        // same name comes back where it was closed.
        if let Some(name) = &node.autosave
            && let Some(state) = node.widget.save_state()
        {
            self.view_state.set(name.as_ref(), state);
        }
        node.widget.removed(text);
        self.layout.remove(node.layout_id);
        for state in [&mut self.focused, &mut self.hovered, &mut self.captured] {
            if *state == Some(id) {
                *state = None;
            }
        }
        for child in node.children {
            self.remove_subtree(child, text);
        }
    }

    pub fn root(&self) -> Option<WidgetId> {
        self.root
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn contains(&self, id: WidgetId) -> bool {
        self.node(id).is_some()
    }

    pub fn parent(&self, id: WidgetId) -> Option<WidgetId> {
        self.node(id)?.parent
    }

    pub fn children(&self, id: WidgetId) -> &[WidgetId] {
        self.node(id).map_or(&[], |n| &n.children)
    }

    pub fn layout(&self, id: WidgetId) -> Option<NodeLayout> {
        Some(self.node(id)?.layout)
    }

    pub fn get<W: Widget>(&self, id: WidgetId) -> Option<&W> {
        let widget: &dyn Any = self.node(id)?.widget.as_ref();
        widget.downcast_ref()
    }

    /// Returns `None` if `id` is gone or is not a `W`.
    pub fn update<W: Widget, R>(
        &mut self,
        id: WidgetId,
        f: impl FnOnce(&mut W, &mut UpdateCx) -> R,
    ) -> Option<R> {
        let (now, reduce_motion) = (self.now, self.reduce_motion);
        let node = self.node_mut(id)?;
        let widget: &mut dyn Any = node.widget.as_mut();
        let widget = widget.downcast_mut::<W>()?;
        let mut cx = UpdateCx {
            now,
            reduce_motion,
            ..Default::default()
        };
        let result = f(widget, &mut cx);
        self.invalidate(id, cx.dirty);
        if cx.animation {
            self.request_animation(id);
        }
        if let Some(at) = cx.timer {
            self.set_timer(id, at);
        }
        Some(result)
    }

    pub fn set_clipboard(&mut self, clipboard: Box<dyn Clipboard>) {
        self.clipboard = clipboard;
    }

    pub fn clipboard(&mut self) -> &mut dyn Clipboard {
        self.clipboard.as_mut()
    }

    /// Bounds are logical pixels, with `scale_factor` applied by the root's transform.
    /// The update contains the whole tree.
    pub fn accessibility_update(&self, scale_factor: f64) -> accesskit::TreeUpdate {
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        let roots = self
            .root
            .into_iter()
            .chain(self.overlays.iter().map(|o| o.root));
        for root in roots {
            children.extend(self.collect_access(root, true, None, &mut nodes));
        }
        let mut window = accesskit::Node::new(accesskit::Role::Window);
        window.set_transform(accesskit::Affine::scale(scale_factor));
        window.set_children(children);
        nodes.push((ACCESS_ROOT, window));
        let focus = self
            .focused
            .filter(|&f| {
                self.node(f)
                    .is_some_and(|n| n.widget.accessibility_role().is_some())
            })
            .map_or(ACCESS_ROOT, access_id);
        let mut tree = accesskit::TreeInfo::new(ACCESS_ROOT);
        tree.toolkit_name = Some("noa-gpui".into());
        accesskit::TreeUpdate {
            nodes,
            tree: Some(tree),
            tree_id: accesskit::TreeId::ROOT,
            focus,
        }
    }

    /// Name `id` for assistive technology, as `aria-label` does on the Web. An empty
    /// label removes the override. The label moves with the widget in
    /// [`UiTree::take_subtree`].
    pub fn set_accessible_label(&mut self, id: WidgetId, label: impl Into<String>) {
        let label: String = label.into();
        let label = (!label.is_empty()).then(|| label.into_boxed_str());
        let Some(node) = self.node_mut(id) else {
            return;
        };
        if node.access_label != label {
            node.access_label = label;
            self.invalidate(id, Dirty::PAINT);
        }
    }

    pub fn accessible_label(&self, id: WidgetId) -> Option<&str> {
        self.node(id)?.access_label.as_deref()
    }

    /// Save what the user arranges in `id` by hand (see [`Widget::save_state`]) under
    /// `name` in [`UiTree::view_state`]. Names should be unique and stay the same between
    /// versions of the application. If restored state has an entry for `name`, `id`
    /// returns to it now. An empty name removes the widget's name. The name moves with
    /// the widget in [`UiTree::take_subtree`].
    pub fn set_autosave_name(&mut self, id: WidgetId, name: impl Into<String>) {
        let name: String = name.into();
        let Some(node) = self.node_mut(id) else {
            return;
        };
        if name.is_empty() {
            node.autosave = None;
            return;
        }
        node.autosave = Some(name.as_str().into());
        if let Some(state) = self.view_state.get(&name).map(str::to_owned) {
            self.restore_widget(id, &state);
        }
    }

    pub fn autosave_name(&self, id: WidgetId) -> Option<&str> {
        self.node(id)?.autosave.as_deref()
    }

    /// Includes restored entries no widget claims now (a closed window) and those of
    /// named widgets removed since.
    pub fn view_state(&self) -> ViewState {
        let mut state = self.view_state.clone();
        for slot in &self.slots {
            let Some(node) = &slot.node else { continue };
            if let Some(name) = &node.autosave
                && let Some(value) = node.widget.save_state()
            {
                state.set(name.as_ref(), value);
            }
        }
        state
    }

    /// Also applies to widgets named later. Entries no widget claims are kept and written
    /// again by [`UiTree::view_state`].
    pub fn restore_view_state(&mut self, state: ViewState) {
        self.view_state.merge(state);
        let named: Vec<(WidgetId, String)> = self
            .live_ids()
            .filter_map(|id| {
                let name = self.node(id)?.autosave.as_deref()?;
                Some((id, self.view_state.get(name)?.to_owned()))
            })
            .collect();
        for (id, value) in named {
            self.restore_widget(id, &value);
        }
    }

    fn restore_widget(&mut self, id: WidgetId, value: &str) {
        let (now, reduce_motion) = (self.now, self.reduce_motion);
        let Some(node) = self.node_mut(id) else {
            return;
        };
        let mut cx = UpdateCx {
            now,
            reduce_motion,
            ..Default::default()
        };
        node.widget.restore_state(&mut cx, value);
        self.invalidate(id, cx.dirty);
        if cx.animation {
            self.request_animation(id);
        }
        if let Some(at) = cx.timer {
            self.set_timer(id, at);
        }
    }

    /// Nodes for `id`'s subtree; returns the ids of the topmost exposed nodes.
    /// `labelled_by` is the nearest ancestor that names unnamed controls inside it
    /// ([`Widget::labels_content`]).
    fn collect_access(
        &self,
        id: WidgetId,
        parent_enabled: bool,
        labelled_by: Option<accesskit::NodeId>,
        nodes: &mut Vec<(accesskit::NodeId, accesskit::Node)>,
    ) -> Vec<accesskit::NodeId> {
        let node = self.node(id).expect("alive");
        if node.is_hidden() {
            return Vec::new();
        }
        let enabled = parent_enabled && node.enabled;
        let role = node.widget.accessibility_role();
        let inner_label = if role.is_some() && node.widget.labels_content() {
            Some(access_id(id))
        } else {
            labelled_by
        };
        let mut children = Vec::new();
        for &child in &node.children {
            children.extend(self.collect_access(child, enabled, inner_label, nodes));
        }
        let Some(role) = role else {
            return children;
        };
        let mut access = accesskit::Node::new(role);
        let size = node.layout.rect;
        if let Some(tip) = node
            .widget
            .tooltip(Point::new(size.width / 2.0, size.height / 2.0))
        {
            access.set_description(tip);
        }
        node.widget.accessibility(&mut access);
        if let Some(label) = &node.access_label {
            access.set_label(label.to_string());
        } else if access.label().is_none()
            && let Some(by) = labelled_by
            && (node.widget.focusable() || access.supports_action(accesskit::Action::Click))
        {
            access.set_labelled_by(vec![by]);
        }
        let origin = self.window_origin(id).unwrap_or_default();
        for (i, mut item) in node.widget.accessibility_items().into_iter().enumerate() {
            if let Some(b) = item.bounds() {
                item.set_bounds(accesskit::Rect {
                    x0: b.x0 + origin.x as f64,
                    y0: b.y0 + origin.y as f64,
                    x1: b.x1 + origin.x as f64,
                    y1: b.y1 + origin.y as f64,
                });
            }
            if !enabled {
                item.set_disabled();
            }
            let item_id = access_item_id(id, i);
            children.push(item_id);
            nodes.push((item_id, item));
        }
        if let Some(rect) = self.window_rect(id) {
            access.set_bounds(accesskit::Rect {
                x0: rect.x as f64,
                y0: rect.y as f64,
                x1: rect.right() as f64,
                y1: rect.bottom() as f64,
            });
        }
        if node.widget.focusable() && enabled {
            access.add_action(accesskit::Action::Focus);
        }
        if !enabled {
            access.set_disabled();
        }
        access.set_children(children);
        let access_id = access_id(id);
        nodes.push((access_id, access));
        vec![access_id]
    }

    pub fn accessibility_action(
        &mut self,
        request: &accesskit::ActionRequest,
        text: &mut TextStore,
    ) {
        let Some(id) = self.widget_for_access_id(request.target_node) else {
            return;
        };
        if !self.is_enabled(id) || self.is_hidden(id) {
            return;
        }
        if request.target_node.0 & ACCESS_ITEM != 0 {
            let item = (request.target_node.0 & (ACCESS_ITEM_LIMIT as u64 - 1)) as usize;
            if request.action == accesskit::Action::Click {
                self.set_focus(Some(id));
                self.deliver(id, &Event::ActivateItem(item), text);
            }
            return;
        }
        match request.action {
            accesskit::Action::Focus => self.set_focus(Some(id)),
            accesskit::Action::Click => {
                self.set_focus(Some(id));
                self.deliver(id, &Event::Activate, text);
            }
            accesskit::Action::Increment => {
                self.deliver(id, &Event::Increment, text);
            }
            accesskit::Action::Decrement => {
                self.deliver(id, &Event::Decrement, text);
            }
            _ => {}
        }
    }

    fn widget_for_access_id(&self, node: accesskit::NodeId) -> Option<WidgetId> {
        let id = if node.0 & ACCESS_ITEM != 0 {
            let raw = node.0 & !ACCESS_ITEM;
            WidgetId {
                index: (raw >> ACCESS_ITEM_BITS) as u32,
                generation: (raw >> ACCESS_GENERATION_SHIFT) as u32,
            }
        } else {
            let raw = node.0.checked_sub(1)?;
            WidgetId {
                index: raw as u32,
                generation: (raw >> 32) as u32,
            }
        };
        self.contains(id).then_some(id)
    }

    /// `index` is clamped.
    pub fn move_child(&mut self, id: WidgetId, index: usize) {
        let Some(parent) = self.parent(id) else {
            return;
        };
        let node = self.node_mut(parent).expect("parent is alive");
        let Some(from) = node.children.iter().position(|&c| c == id) else {
            return;
        };
        let child = node.children.remove(from);
        let index = index.min(node.children.len());
        node.children.insert(index, child);
        if from == index {
            return;
        }
        let parent_layout = node.layout_id;
        let children: Vec<WidgetId> = node.children.clone();
        let layouts: Vec<LayoutId> = children
            .iter()
            .map(|&c| self.node(c).expect("children are alive").layout_id)
            .collect();
        self.layout.set_children(parent_layout, &layouts);
        self.touch_ancestors(parent);
        self.children_changed(Some(parent));
        self.structure_changed = true;
        self.pending |= Dirty::LAYOUT | Dirty::COMPOSITE;
    }

    fn reparent(&mut self, id: WidgetId, new_parent: WidgetId) {
        let Some(old) = self.parent(id) else {
            return;
        };
        let old_node = self.node_mut(old).expect("parent is alive");
        old_node.children.retain(|&c| c != id);
        let old_layout = old_node.layout_id;
        let siblings: Vec<LayoutId> = old_node
            .children
            .clone()
            .iter()
            .map(|&c| self.node(c).expect("children are alive").layout_id)
            .collect();
        self.layout.set_children(old_layout, &siblings);
        self.touch_ancestors(old);
        let parent_node = self.node_mut(new_parent).expect("new parent is alive");
        let hidden = parent_node.widget.hides_child(parent_node.children.len());
        parent_node.children.push(id);
        let parent_layout = parent_node.layout_id;
        let node = self.node_mut(id).expect("alive");
        node.parent = Some(new_parent);
        node.hidden_by_parent = hidden;
        node.layout_touched = true;
        let (layout_id, style) = (node.layout_id, node.layout_style());
        self.layout.set_style(layout_id, style);
        self.layout.add_child(parent_layout, layout_id);
        self.touch_ancestors(new_parent);
        self.structure_changed = true;
        // Renumber first: restyling reads `sibling_index`.
        self.children_changed(Some(old));
        self.children_changed(Some(new_parent));
        self.after_visibility_change(id);
        self.refresh_style_subtree(id);
        self.pending |= Dirty::LAYOUT | Dirty::COMPOSITE;
    }

    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    fn drag_over(&mut self, pos: Point, text: &mut TextStore) {
        let Some(drag) = &self.drag else { return };
        let event = Event::DragOver {
            pos,
            data: drag.data.clone(),
        };
        let mut accepted = None;
        let mut current = self.hit_test(pos);
        while let Some(id) = current {
            let Some((handled, parent, accepts)) = self.deliver_drag(id, &event, text) else {
                break;
            };
            if accepts {
                accepted = Some(id);
            }
            if handled {
                break;
            }
            current = parent;
        }
        let previous = self.drag.as_ref().and_then(|d| d.target);
        if previous != accepted {
            if let Some(previous) = previous.filter(|&p| self.contains(p)) {
                self.deliver(previous, &Event::DragLeave, text);
            }
            if let Some(drag) = &mut self.drag {
                drag.target = accepted;
            }
        }
    }

    fn deliver_drag(
        &mut self,
        id: WidgetId,
        event: &Event,
        text: &mut TextStore,
    ) -> Option<(bool, Option<WidgetId>, bool)> {
        self.drop_accepted = false;
        let (handled, parent) = self.deliver(id, event, text)?;
        Some((handled, parent, std::mem::take(&mut self.drop_accepted)))
    }

    fn end_drag(&mut self, pos: Point, dropped: bool, text: &mut TextStore) {
        let Some(drag) = self.drag.take() else { return };
        let mut delivered = false;
        if let Some(target) = drag.target.filter(|&t| self.contains(t)) {
            // A target disabled since it accepted the drag takes nothing.
            let event = if dropped && self.is_enabled(target) {
                delivered = true;
                Event::Drop {
                    pos,
                    data: drag.data,
                }
            } else {
                Event::DragLeave
            };
            self.deliver(target, &event, text);
        }
        if let Some(source) = drag.source.filter(|&s| self.contains(s)) {
            self.deliver(source, &Event::DragEnd { dropped: delivered }, text);
        }
    }

    /// Replaces an earlier registration of the same shortcut.
    pub fn add_shortcut(&mut self, shortcut: Shortcut, target: WidgetId) {
        self.remove_shortcut(shortcut);
        self.shortcuts.push((shortcut, target));
    }

    pub fn remove_shortcut(&mut self, shortcut: Shortcut) {
        self.shortcuts.retain(|(s, _)| *s != shortcut);
    }

    /// Show shortcuts in menus as Mac glyphs (⌘⇧S) rather than Ctrl+Shift+S. On by
    /// default on Apple targets; the web sets it from the browser's platform, matching
    /// which key [`crate::Modifiers::command`] stands for.
    pub fn set_shortcut_glyphs(&mut self, glyphs: bool) {
        self.shortcut_glyphs = glyphs;
    }

    pub fn shortcut_glyphs(&self) -> bool {
        self.shortcut_glyphs
    }

    /// Set the clock (seconds, monotonic) used for animations. Call it before
    /// dispatching events and before [`UiTree::animate`].
    pub fn set_now(&mut self, now: f64) {
        self.now = now;
    }

    /// Keep every input event dispatched from now on, with the clock (see
    /// [`crate::recording`]). Starting again drops what was kept.
    pub fn start_recording(&mut self) {
        self.recording = Some(Vec::new());
    }

    /// The input kept since recording started or since the last call; recording goes
    /// on. Empty when not recording.
    pub fn take_recorded(&mut self) -> Vec<crate::recording::Step> {
        self.recording
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Stop recording and return what was not taken yet.
    pub fn stop_recording(&mut self) -> crate::recording::Recording {
        crate::recording::Recording {
            steps: self.recording.take().unwrap_or_default(),
        }
    }

    pub fn now(&self) -> f64 {
        self.now
    }

    /// Follow the OS "reduce motion" setting (`prefers-reduced-motion` on the Web):
    /// transitions that move, resize or fade jump to their end. Progress indicators and
    /// scrolling keep moving. Off by default.
    pub fn set_reduce_motion(&mut self, reduce: bool) {
        self.reduce_motion = reduce;
    }

    pub fn reduces_motion(&self) -> bool {
        self.reduce_motion
    }

    /// The application should keep drawing frames and call [`UiTree::animate`] before each
    /// [`UiTree::prepare`] while this is true. Widgets that asked for a frame while not
    /// visible do not count until they become visible again.
    pub fn wants_animation_frame(&self) -> bool {
        self.animating.iter().any(|&id| self.is_visible(id))
    }

    /// Widgets that are not visible keep waiting. Also shows a tooltip whose delay has
    /// passed.
    pub fn animate(&mut self, now: f64, text: &mut TextStore) {
        self.now = now;
        if self.tooltip.due.is_some_and(|due| due <= now) {
            self.show_tooltip();
        }
        if self.hover_card.close_due.is_some_and(|due| due <= now) {
            self.hide_hover_card(text);
        }
        if self.hover_card.due.is_some_and(|due| due <= now) {
            self.show_hover_card();
        }
        if self.timers.iter().any(|&(at, _)| at <= now) {
            let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.timers)
                .into_iter()
                .partition(|&(at, _)| at <= now);
            self.timers = waiting;
            let event = Event::Timer { now };
            for (_, id) in due {
                if self.contains(id) {
                    self.deliver(id, &event, text);
                }
            }
        }
        let mut ids = std::mem::take(&mut self.animating);
        ids.sort_unstable();
        ids.dedup();
        ids.retain(|&id| self.contains(id));
        let (visible, waiting): (Vec<_>, Vec<_>) =
            ids.into_iter().partition(|&id| self.is_visible(id));
        self.animating = waiting;
        let event = Event::AnimationFrame { now };
        for id in visible {
            if self.contains(id) {
                self.deliver(id, &event, text);
            }
        }
        self.deliver_focus_changes(text);
    }

    /// In window coordinates, clipped by every clipping ancestor and the window. `None`
    /// when nothing of it shows, also before the first [`UiTree::prepare`].
    pub fn visible_rect(&self, id: WidgetId) -> Option<Rect> {
        let viewport = self.viewport?;
        let mut visible = self.window_rect(id)?.intersect(&Rect::new(
            0.0,
            0.0,
            viewport.width,
            viewport.height,
        ))?;
        for ancestor in self.ancestors(id).skip(1) {
            if self.node(ancestor)?.clips_children() {
                visible = visible.intersect(&self.window_rect(ancestor)?)?;
            }
        }
        (visible.width > 0.0 && visible.height > 0.0).then_some(visible)
    }

    pub fn is_visible(&self, id: WidgetId) -> bool {
        self.visible_rect(id).is_some()
    }

    fn set_timer(&mut self, id: WidgetId, at: f64) {
        self.timers.retain(|&(_, w)| w != id);
        self.timers.push((at, id));
    }

    fn request_animation(&mut self, id: WidgetId) {
        if !self.animating.contains(&id) {
            self.animating.push(id);
        }
    }

    /// It sizes to its content. Add its children with [`UiTree::insert`] as usual.
    pub fn open_overlay(
        &mut self,
        widget: impl Widget,
        placement: Placement,
        options: OverlayOptions,
    ) -> WidgetId {
        let root = self.insert_node(None, Box::new(widget));
        self.overlays.push(Overlay {
            root,
            placement,
            options,
            owner: None,
            menu: false,
            // Restored if the overlay takes the focus and still holds it when it closes.
            previous_focus: self.focused,
            keeps_focus: false,
            anchor: None,
        });
        root
    }

    /// Close an overlay opened with [`UiTree::open_overlay`]. No message is emitted. If
    /// it held the focus, focus returns to where it was before it opened.
    pub fn close_overlay(&mut self, id: WidgetId, text: &mut TextStore) {
        if self.overlays.iter().any(|o| o.root == id) {
            self.close_owned_overlay(id, None, text);
        }
    }

    pub(crate) fn set_overlay_anchor(&mut self, overlay: WidgetId, anchor: WidgetId) {
        if let Some(o) = self.overlays.iter_mut().find(|o| o.root == overlay) {
            o.anchor = Some(anchor);
        }
    }

    pub fn set_overlay_placement(&mut self, id: WidgetId, placement: Placement) {
        let Some(index) = self.overlays.iter().position(|o| o.root == id) else {
            return;
        };
        self.overlays[index].placement = placement;
        self.position_overlay(index);
        self.pending |= Dirty::COMPOSITE;
    }

    /// Bottom to top.
    pub fn overlays(&self) -> impl Iterator<Item = WidgetId> + '_ {
        self.overlays.iter().map(|o| o.root)
    }

    fn position_overlay(&mut self, index: usize) {
        let Some(viewport) = self.viewport else {
            return;
        };
        let overlay = &self.overlays[index];
        let placement = overlay.placement;
        let root = overlay.root;
        let node = self.node_mut(root).expect("overlay root is alive");
        let size = Size::new(node.layout.rect.width, node.layout.rect.height);
        let origin = place(placement, size, viewport);
        node.layout.rect.x = origin.x;
        node.layout.rect.y = origin.y;
        let rect = Rect::new(origin.x, origin.y, size.width, size.height);
        if node.widget.overlay_placed(rect) {
            self.invalidate(root, Dirty::PAINT);
        }
    }

    /// All of them if `hit` is not in an overlay. Returns whether the press should go no
    /// further.
    fn dismiss_overlays_outside(&mut self, hit: Option<WidgetId>, text: &mut TextStore) -> bool {
        let containing = hit.and_then(|h| {
            let top = self.ancestors(h).last()?;
            self.overlays.iter().position(|o| o.root == top)
        });
        let keep = containing.map_or(0, |i| i + 1);
        let mut consume = false;
        while self.overlays.len() > keep {
            let top = self.overlays.last().expect("len > keep");
            if !top.options.light_dismiss {
                break;
            }
            consume |= top.options.consume_dismissing_press
                || top
                    .anchor
                    .is_some_and(|a| hit.is_some_and(|h| self.ancestors(h).any(|w| w == a)));
            let root = top.root;
            self.dismiss_overlay(root, text);
        }
        consume
    }

    fn dismiss_overlay(&mut self, root: WidgetId, text: &mut TextStore) {
        self.messages
            .push(Message::new(root, Box::new(OverlayDismissed)));
        self.close_owned_overlay(root, None, text);
    }

    fn open_menu_overlay(&mut self, owner: WidgetId, request: crate::widget::MenuRequest) {
        let menu = crate::widgets::Menu::new(request.items, request.min_width, request.checked)
            .opened_as(request.highlight, request.submenu, request.bar)
            .shortcut_glyphs(self.shortcut_glyphs);
        let root = self.insert_node(None, Box::new(menu));
        self.overlays.push(Overlay {
            root,
            placement: request.placement,
            options: OverlayOptions::MENU,
            owner: Some(owner),
            menu: true,
            previous_focus: self.focused,
            keeps_focus: false,
            anchor: None,
        });
        self.set_focus(Some(root));
    }

    /// Open a popover for `owner` and give its content keyboard focus. Like a menu, a
    /// press outside closes it and goes no further, so pressing the owner again only
    /// closes it.
    fn open_popover_overlay(&mut self, owner: WidgetId, request: crate::widget::PopoverRequest) {
        let root = self.insert_node(None, request.content);
        self.overlays.push(Overlay {
            root,
            placement: request.placement,
            options: OverlayOptions::MENU,
            owner: Some(owner),
            menu: false,
            previous_focus: self.focused,
            keeps_focus: false,
            anchor: None,
        });
        self.set_focus(Some(root));
    }

    /// A press outside closes it and still reaches what is under the pointer.
    fn open_popup_overlay(&mut self, owner: WidgetId, request: crate::widget::PopoverRequest) {
        let root = self.insert_node(None, request.content);
        self.overlays.push(Overlay {
            root,
            placement: request.placement,
            options: OverlayOptions {
                light_dismiss: true,
                ..OverlayOptions::default()
            },
            owner: Some(owner),
            menu: false,
            previous_focus: self.focused,
            keeps_focus: true,
            anchor: None,
        });
    }

    fn close_overlays_owned_by(&mut self, owner: WidgetId, text: &mut TextStore) {
        let Some(index) = self.overlays.iter().position(|o| o.owner == Some(owner)) else {
            return;
        };
        while self.overlays.len() > index {
            let root = self.overlays.last().expect("len > index").root;
            self.close_owned_overlay(root, None, text);
        }
    }

    fn context_menu_at(
        &self,
        id: WidgetId,
        pos: Point,
    ) -> Option<(WidgetId, Vec<crate::widgets::MenuItem>)> {
        self.ancestors(id).find_map(|a| {
            let node = self.node(a)?;
            if !node.enabled {
                return None;
            }
            let origin = self.window_origin(a)?;
            let local = Point::new(pos.x - origin.x, pos.y - origin.y);
            node.widget
                .context_menu(local)
                .filter(|items| !items.is_empty())
                .map(|items| (a, items))
        })
    }

    fn open_context_menu(
        &mut self,
        owner: WidgetId,
        items: Vec<crate::widgets::MenuItem>,
        at: Point,
    ) {
        self.open_menu_overlay(
            owner,
            crate::widget::MenuRequest {
                items,
                placement: Placement::At(at),
                min_width: 0.0,
                checked: None,
                highlight: None,
                submenu: false,
                bar: false,
            },
        );
    }

    fn overlay_of(&self, id: WidgetId) -> Option<usize> {
        let top = self.ancestors(id).last()?;
        self.overlays.iter().position(|o| o.root == top)
    }

    fn close_owned_overlay(&mut self, root: WidgetId, chosen: Option<usize>, text: &mut TextStore) {
        let Some(overlay) = self.overlays.iter().find(|o| o.root == root) else {
            return;
        };
        let (owner, menu, previous, keeps_focus) = (
            overlay.owner,
            overlay.menu,
            overlay.previous_focus,
            overlay.keeps_focus,
        );
        self.remove(root, text);
        if owner.is_none() && self.focused.is_none() {
            self.set_focus(previous.filter(|&p| self.contains(p)));
        }
        if let Some(owner) = owner.filter(|&o| self.contains(o)) {
            let takes_focus = self.node(owner).is_some_and(|n| n.widget.focusable());
            let focus = if takes_focus {
                Some(owner)
            } else {
                previous.filter(|&p| self.contains(p))
            };
            // A popup never had the focus: leave it where it is.
            if !keeps_focus {
                self.set_focus(focus);
            }
            let event = if menu {
                Event::MenuClosed { chosen }
            } else {
                Event::PopoverClosed
            };
            self.deliver(owner, &event, text);
        }
    }

    fn activate_key_equivalent(
        &mut self,
        key: Key,
        modifiers: crate::event::Modifiers,
        text: &mut TextStore,
    ) -> bool {
        let Some(scope) = self.modal_overlay().or(self.root) else {
            return false;
        };
        // The focused scope (an inner window) is searched first; other scopes never.
        let current = self.focus_scope_of_focus();
        let mut stack = vec![scope];
        if self.modal_overlay().is_none() {
            stack.extend(self.focus_scope_root());
        }
        let mut target = None;
        while let Some(id) = stack.pop() {
            let node = self.node(id).expect("alive");
            if !node.enabled || node.is_hidden() {
                continue;
            }
            if node.widget.focus_scope() {
                if Some(id) != current {
                    continue;
                }
                if let Some(modal) = self.modal_child(id) {
                    stack.push(modal);
                    continue;
                }
            }
            if node
                .widget
                .key_equivalent()
                .is_some_and(|s| s.matches(key, modifiers))
            {
                target = Some((id, Event::Activate));
                break;
            }
            if let Some(&shortcut) = node
                .widget
                .key_equivalents()
                .iter()
                .find(|s| s.matches(key, modifiers))
            {
                target = Some((id, Event::KeyEquivalent(shortcut)));
                break;
            }
            stack.extend(node.children.iter().rev());
        }
        match target {
            Some((id, event)) => {
                self.deliver(id, &event, text);
                true
            }
            None => false,
        }
    }

    /// The topmost modal overlay, which receives all input while open.
    fn modal_overlay(&self) -> Option<WidgetId> {
        self.overlays
            .iter()
            .rev()
            .find(|o| o.options.modal)
            .map(|o| o.root)
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// No frame is needed if the effective declarations are unchanged.
    pub fn set_stylesheet(&mut self, mut stylesheet: StyleSheet) {
        if let Some(viewport) = self.viewport {
            stylesheet.set_viewport(viewport.width, viewport.height);
        }
        self.stylesheet = stylesheet;
        let ids: Vec<_> = self.live_ids().collect();
        for id in ids {
            self.refresh_style(id);
        }
    }

    pub fn stylesheet(&self) -> &StyleSheet {
        &self.stylesheet
    }

    pub fn set_style_meta(&mut self, id: WidgetId, meta: StyleMeta) {
        let Some(node) = self.node_mut(id) else {
            return;
        };
        if node.style_meta.as_deref().cloned().unwrap_or_default() == meta {
            return;
        }
        node.style_meta = (meta != StyleMeta::default()).then(|| Box::new(meta));
        self.restyle_around(id);
    }

    /// Inline declarations outrank matching rules.
    pub fn has_inline_style(&self, id: WidgetId) -> bool {
        self.node(id)
            .is_some_and(|node| node.inline_style.is_some())
    }

    pub fn set_inline_style(&mut self, id: WidgetId, style: StylePatch) {
        let Some(node) = self.node_mut(id) else {
            return;
        };
        if node.inline_style.as_deref().unwrap_or(&StylePatch::EMPTY) == &style {
            return;
        }
        node.inline_style = (!style.is_empty()).then(|| Box::new(style));
        self.refresh_style(id);
    }

    pub fn computed_style(&self, id: WidgetId) -> Option<&StylePatch> {
        let node = self.node(id)?;
        Some(node.resolved_style.as_deref().unwrap_or(&StylePatch::EMPTY))
    }

    fn refresh_style_path(&mut self, id: Option<WidgetId>) {
        if let Some(id) = id {
            let path: Vec<_> = self.ancestors(id).collect();
            for node in path {
                self.refresh_style(node);
            }
        }
    }

    fn refresh_style_subtree(&mut self, id: WidgetId) {
        let mut nodes = Vec::new();
        self.collect_subtree(id, &mut nodes);
        for node in nodes {
            self.refresh_style(node);
        }
    }

    fn restyle_around(&mut self, id: WidgetId) {
        if !self.stylesheet.needs_ancestors() {
            self.refresh_style(id);
            return;
        }
        let parent = self.parent(id).filter(|_| self.stylesheet.needs_siblings());
        // The next `prepare` resolves this whole list of children anyway.
        if parent.is_some_and(|parent| self.restyle_children.contains(&parent)) {
            return;
        }
        self.refresh_style_subtree(id);
        if let Some(parent) = parent {
            // Sibling combinators look backwards: only later siblings (and their
            // descendants) can match through this node.
            let index = self.node(id).map_or(0, |node| node.sibling_index as usize);
            let later: Vec<WidgetId> = self.children(parent)[index + 1..].to_vec();
            for sibling in later {
                self.refresh_style_subtree(sibling);
            }
        }
    }

    fn interaction_targets(&self) -> [Option<WidgetId>; 3] {
        [self.focused, self.hovered, self.captured]
    }

    fn restyle_after_removal(&mut self, parent: Option<WidgetId>, before: [Option<WidgetId>; 3]) {
        let Some(parent) = parent else { return };
        if before == self.interaction_targets() || self.stylesheet.is_empty() {
            return;
        }
        self.refresh_style_path(Some(parent));
        if self.stylesheet.needs_ancestors() {
            let top = self.ancestors(parent).last().unwrap_or(parent);
            self.refresh_style_subtree(top);
        }
    }

    fn children_changed(&mut self, parent: Option<WidgetId>) {
        let Some(parent) = parent else { return };
        for index in 0..self.children(parent).len() {
            let child = self.children(parent)[index];
            self.node_mut(child).expect("child is alive").sibling_index = index as u32;
        }
        if self.stylesheet.needs_siblings() {
            self.queue_restyle_children(parent);
        }
    }

    fn queue_restyle_children(&mut self, parent: WidgetId) {
        if !self.restyle_children.contains(&parent) {
            self.restyle_children.push(parent);
        }
    }

    fn restyle_changed_children(&mut self) {
        for parent in std::mem::take(&mut self.restyle_children) {
            for child in self.children(parent).to_vec() {
                self.refresh_style_subtree(child);
            }
        }
    }

    /// `exact` is for a state that also applies to the target alone (`:focus`).
    fn refresh_style_transition(
        &mut self,
        old: Option<WidgetId>,
        new: Option<WidgetId>,
        exact: bool,
    ) {
        if self.stylesheet.is_empty() {
            return;
        }
        self.refresh_style_path(old);
        self.refresh_style_path(new);
        if !self.stylesheet.needs_ancestors() {
            return;
        }
        // Other nodes can match through a node's state. The ancestors on both paths
        // keep theirs, so only the top of each side's own part has to be walked.
        let path = |id: Option<WidgetId>| -> Vec<WidgetId> {
            id.map(|id| self.ancestors(id).collect())
                .unwrap_or_default()
        };
        let (old_path, new_path) = (path(old), path(new));
        let mut tops = Vec::new();
        for (own, other) in [(&old_path, &new_path), (&new_path, &old_path)] {
            tops.extend(own.iter().rev().find(|id| !other.contains(id)).copied());
        }
        if exact {
            tops.extend(old);
            tops.extend(new);
        }
        for top in tops {
            self.restyle_around(top);
        }
    }

    fn set_captured(&mut self, id: Option<WidgetId>) {
        if id != self.captured {
            let old = std::mem::replace(&mut self.captured, id);
            self.refresh_style_transition(old, id, false);
        }
    }

    fn style_state(&self, id: WidgetId) -> StyleState {
        let in_path = |target: Option<WidgetId>| {
            target.is_some_and(|target| self.ancestors(target).any(|ancestor| ancestor == id))
        };
        let focused = self.focused == Some(id);
        StyleState {
            hovered: in_path(self.hovered),
            focused,
            focus_within: in_path(self.focused),
            focus_visible: focused && self.focus_visible,
            active: in_path(self.captured),
            disabled: !self.is_enabled(id),
            root: self.parent(id).is_none(),
        }
    }

    fn refresh_style(&mut self, id: WidgetId) {
        let Some(node) = self.node(id) else { return };
        let parent_style = node
            .parent
            .and_then(|parent| self.node(parent))
            .map_or(&StylePatch::EMPTY, Node::style);
        // Nothing can apply to most nodes of a tree that is not styled.
        if self.stylesheet.is_empty()
            && node.inline_style.is_none()
            && node.resolved_style.is_none()
            && parent_style.is_empty()
        {
            return;
        }
        let inline = node.inline_style.as_deref().unwrap_or(&StylePatch::EMPTY);
        let mut resolved = self.stylesheet.resolve_in(self, id, inline);
        if node.widget.inherits_style() {
            resolved.inherit_from(parent_style);
        }
        let old = node.style();
        if *old == resolved {
            return;
        }
        let dirty = if old.affects_layout_differently(&resolved) {
            Dirty::LAYOUT
        } else {
            Dirty::PAINT
        };
        let visibility = old.is_display_none() != resolved.is_display_none();
        let inherited = old.inherited_differs(&resolved);
        self.node_mut(id).expect("node is alive").resolved_style =
            (!resolved.is_empty()).then(|| Box::new(resolved));
        self.invalidate(id, dirty);
        if dirty == Dirty::PAINT {
            // Opacity and clipping are applied when the frame is put together.
            self.pending |= Dirty::COMPOSITE;
        }
        if visibility {
            self.after_visibility_change(id);
        }
        if inherited {
            for child in self.children(id).to_vec() {
                self.refresh_style(child);
            }
        }
    }

    /// Every widget is repainted.
    pub fn set_theme(&mut self, theme: Theme) {
        if theme == self.theme {
            return;
        }
        self.theme = theme;
        let live: Vec<WidgetId> = self.live_ids().collect();
        for id in live {
            self.invalidate(id, Dirty::PAINT);
        }
    }

    /// Enable or disable `id` and its subtree. Disabled widgets draw dimmed, are skipped
    /// by hit testing (pointer events go to the nearest enabled ancestor) and cannot
    /// take focus; a disabled widget loses focus, hover and pointer capture.
    pub fn set_enabled(&mut self, id: WidgetId, enabled: bool) {
        let Some(node) = self.node_mut(id) else {
            return;
        };
        if node.enabled == enabled {
            return;
        }
        node.enabled = enabled;
        let mut subtree = Vec::new();
        self.collect_subtree(id, &mut subtree);
        for &widget in &subtree {
            self.invalidate(widget, Dirty::PAINT);
            self.refresh_style(widget);
        }
        if self.stylesheet.needs_siblings() {
            self.restyle_around(id);
        }
        if !enabled {
            if self.focused.is_some_and(|f| subtree.contains(&f)) {
                self.set_focus(None);
            }
            if self.hovered.is_some_and(|h| subtree.contains(&h)) {
                self.set_hovered(None);
            }
            if self.captured.is_some_and(|c| subtree.contains(&c)) {
                self.set_captured(None);
            }
        }
    }

    /// Hide or show `id`: a hidden widget and its subtree take no space in the layout
    /// (siblings close the gap, as in a stack view), are not drawn, hit, focused or
    /// exposed to assistive technology. A hidden widget loses focus.
    pub fn set_hidden(&mut self, id: WidgetId, hidden: bool) {
        let Some(node) = self.node_mut(id) else {
            return;
        };
        if node.hidden == hidden {
            return;
        }
        node.hidden = hidden;
        self.after_visibility_change(id);
    }

    pub fn is_hidden(&self, id: WidgetId) -> bool {
        self.ancestors(id)
            .any(|a| self.node(a).is_some_and(Node::is_hidden))
    }

    fn after_visibility_change(&mut self, id: WidgetId) {
        self.invalidate(id, Dirty::LAYOUT | Dirty::PAINT);
        self.structure_changed = true;
        if !self.is_hidden(id) {
            self.queue_shown_paint(id);
        } else {
            self.drop_state_inside(id);
        }
    }

    fn queue_shown_paint(&mut self, id: WidgetId) {
        let mut subtree = Vec::new();
        self.collect_subtree(id, &mut subtree);
        for w in subtree {
            if self.node(w).is_some_and(|n| n.dirty.contains(Dirty::PAINT)) {
                self.paint_queue.push(w);
                self.pending |= Dirty::PAINT | Dirty::COMPOSITE;
            }
        }
    }

    fn sync_hidden_children(&mut self, parent: WidgetId, changed: &mut Vec<WidgetId>) {
        let node = self.node(parent).expect("alive");
        let updates: Vec<(WidgetId, bool)> = node
            .children
            .iter()
            .enumerate()
            .filter_map(|(i, &child)| {
                let hide = node.widget.hides_child(i);
                (self.node(child)?.hidden_by_parent != hide).then_some((child, hide))
            })
            .collect();
        for (child, hide) in updates {
            let node = self.node_mut(child).expect("alive");
            node.hidden_by_parent = hide;
            let (layout_id, style) = (node.layout_id, node.layout_style());
            self.layout.set_style(layout_id, style);
            self.layout.mark_dirty(layout_id);
            changed.push(child);
            if !hide {
                self.queue_shown_paint(child);
            } else {
                self.drop_state_inside(child);
            }
        }
    }

    fn drop_state_inside(&mut self, id: WidgetId) {
        let mut subtree = Vec::new();
        self.collect_subtree(id, &mut subtree);
        if self.focused.is_some_and(|f| subtree.contains(&f)) {
            self.set_focus(None);
        }
        if self.hovered.is_some_and(|h| subtree.contains(&h)) {
            self.set_hovered(None);
        }
        if self.captured.is_some_and(|c| subtree.contains(&c)) {
            self.set_captured(None);
        }
    }

    pub fn is_enabled(&self, id: WidgetId) -> bool {
        self.ancestors(id)
            .all(|a| self.node(a).is_some_and(|n| n.enabled))
    }

    fn collect_subtree(&self, id: WidgetId, out: &mut Vec<WidgetId>) {
        out.push(id);
        for &child in self.children(id) {
            self.collect_subtree(child, out);
        }
    }

    fn live_ids(&self) -> impl Iterator<Item = WidgetId> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.node.as_ref().map(|_| WidgetId {
                index: index as u32,
                generation: slot.generation,
            })
        })
    }

    fn node(&self, id: WidgetId) -> Option<&Node> {
        let slot = self.slots.get(id.index as usize)?;
        (slot.generation == id.generation)
            .then_some(())
            .and(slot.node.as_ref())
    }

    fn node_mut(&mut self, id: WidgetId) -> Option<&mut Node> {
        let slot = self.slots.get_mut(id.index as usize)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.node.as_mut()
    }

    pub fn invalidate(&mut self, id: WidgetId, dirty: Dirty) {
        let Some(node) = self.node_mut(id) else {
            return;
        };
        let mut queue_layout = false;
        let mut queue_paint = false;
        if dirty.contains(Dirty::LAYOUT) && !node.dirty.contains(Dirty::LAYOUT) {
            node.dirty |= Dirty::LAYOUT;
            queue_layout = true;
        }
        // New layout usually means new content (e.g. text), so repaint the widget too.
        if dirty.intersects(Dirty::LAYOUT | Dirty::PAINT) && !node.dirty.contains(Dirty::PAINT) {
            node.dirty |= Dirty::PAINT;
            queue_paint = true;
        }
        if queue_layout {
            self.layout_queue.push(id);
            self.touch_ancestors(id);
        }
        if queue_paint {
            self.paint_queue.push(id);
        }
        if !dirty.is_empty() {
            // A layout request also repaints the widget (see above), even if its size
            // ends up unchanged, so the paint pass must run too.
            let paint = if dirty.contains(Dirty::LAYOUT) {
                Dirty::PAINT
            } else {
                Dirty::NONE
            };
            self.pending |= dirty | paint | Dirty::COMPOSITE;
        }
    }

    /// A touched node's ancestors are always touched too, so the walk stops at the first
    /// touched node.
    fn touch_ancestors(&mut self, id: WidgetId) {
        let mut current = Some(id);
        while let Some(id) = current {
            let node = self.node_mut(id).expect("ancestors are alive");
            if node.layout_touched {
                break;
            }
            node.layout_touched = true;
            current = node.parent;
        }
    }

    pub fn needs_update(&self, viewport: Size) -> bool {
        // Focus can be lost while a frame is put together (a hidden page held it); the
        // next `prepare` tells the widgets.
        !self.pending.is_empty() || self.viewport != Some(viewport) || !self.focus_events.is_empty()
    }

    /// `viewport` is in logical pixels. Returns `None` when nothing changed since the last
    /// call.
    pub fn prepare(&mut self, viewport: Size, text: &mut TextStore) -> Option<&DisplayList> {
        self.deliver_focus_changes(text);
        self.stats = UpdateStats::default();
        let root = self.root?;
        let resized = self.viewport != Some(viewport);
        if resized {
            self.pending |= Dirty::LAYOUT | Dirty::COMPOSITE;
            if self
                .stylesheet
                .set_viewport(viewport.width, viewport.height)
            {
                let ids: Vec<_> = self.live_ids().collect();
                for id in ids {
                    self.refresh_style(id);
                }
            }
        }
        self.restyle_changed_children();
        if self.pending.is_empty() {
            return None;
        }
        if self.pending.contains(Dirty::LAYOUT) {
            self.run_layout(root, viewport, resized, text);
        }
        if std::mem::take(&mut self.reveal_focus) {
            self.reveal_focused();
        }
        if self.pending.contains(Dirty::PAINT) {
            self.run_paint(text);
        }
        self.assemble(root, viewport);
        self.pending = Dirty::NONE;
        Some(&self.list)
    }

    /// The display list assembled by the last [`UiTree::prepare`] that returned `Some`.
    pub fn display_list(&self) -> &DisplayList {
        &self.list
    }

    pub fn stats(&self) -> UpdateStats {
        self.stats
    }

    fn run_layout(&mut self, root: WidgetId, viewport: Size, resized: bool, text: &mut TextStore) {
        self.viewport = Some(viewport);
        let mut changed = Vec::new();
        // Hiding a child can restyle (it loses focus or hover) and so queue more layout:
        // take the queue until it stays empty.
        while !self.layout_queue.is_empty() {
            for id in std::mem::take(&mut self.layout_queue) {
                if let Some(node) = self.node_mut(id)
                    && node.dirty.contains(Dirty::LAYOUT)
                {
                    node.dirty.remove(Dirty::LAYOUT);
                    let (layout_id, style) = (node.layout_id, node.layout_style());
                    self.layout.set_style(layout_id, style);
                    self.layout.mark_dirty(layout_id);
                    changed.push(id);
                    self.sync_hidden_children(id, &mut changed);
                }
            }
        }
        // Take the flag before `||`: a resize must not leave it set for the next frame.
        let structure_changed = std::mem::take(&mut self.structure_changed);
        let full = resized || structure_changed;
        let boundaries = if full {
            None
        } else {
            self.enclosing_boundaries(&changed)
        };
        self.stats.layout = true;

        let mut resized_widgets = Vec::new();
        match boundaries {
            Some(boundaries) => {
                self.stats.partial_layouts = boundaries.len();
                for boundary in boundaries {
                    self.layout_boundary(boundary, text, &mut resized_widgets);
                }
            }
            None => {
                let root_layout = self.node(root).expect("root is alive").layout_id;
                let Self {
                    slots,
                    layout,
                    stats,
                    ..
                } = self;
                layout.compute(root_layout, viewport, |id, input| {
                    stats.measured += 1;
                    let node = slots[id.index as usize]
                        .node
                        .as_mut()
                        .expect("measured widget is alive");
                    node.widget.measure(
                        &mut MeasureCx {
                            text: &mut *text,
                            style: node.resolved_style.as_deref().unwrap_or(&StylePatch::EMPTY),
                        },
                        input,
                    )
                });
                read_back(
                    &mut self.slots,
                    &self.layout,
                    root,
                    resized,
                    &mut self.stats,
                    &mut resized_widgets,
                );
                self.layout_overlays(resized, text, &mut resized_widgets);
            }
        }
        for id in resized_widgets {
            self.invalidate(id, Dirty::PAINT);
        }
    }

    fn layout_overlays(&mut self, force: bool, text: &mut TextStore, resized: &mut Vec<WidgetId>) {
        for index in 0..self.overlays.len() {
            let root = self.overlays[index].root;
            let root_layout = self.node(root).expect("overlay root is alive").layout_id;
            // A drawer is laid out in the window's area below its top; other overlays at
            // their content size.
            let area = match (self.overlays[index].placement, self.viewport) {
                (Placement::Drawer { top, .. }, Some(viewport)) => {
                    Some(Size::new(viewport.width, (viewport.height - top).max(0.0)))
                }
                _ => None,
            };
            let Self {
                slots,
                layout,
                stats,
                ..
            } = self;
            let measure = |id: &mut WidgetId, input| {
                stats.measured += 1;
                let node = slots[id.index as usize]
                    .node
                    .as_mut()
                    .expect("measured widget is alive");
                node.widget.measure(
                    &mut MeasureCx {
                        text: &mut *text,
                        style: node.resolved_style.as_deref().unwrap_or(&StylePatch::EMPTY),
                    },
                    input,
                )
            };
            match area {
                Some(area) => layout.compute(root_layout, area, measure),
                None => layout.compute_fit_content(root_layout, measure),
            }
            read_back(
                &mut self.slots,
                &self.layout,
                root,
                force,
                &mut self.stats,
                resized,
            );
            self.position_overlay(index);
        }
    }

    /// The outermost layout boundaries enclosing every changed widget, or `None` if some
    /// change is not inside a boundary (the whole tree must then be laid out).
    fn enclosing_boundaries(&self, changed: &[WidgetId]) -> Option<Vec<WidgetId>> {
        // In the order first found; the set answers membership, so many changes in one
        // frame (thousands of rows updated) cost linear time, not quadratic.
        let mut boundaries: Vec<WidgetId> = Vec::new();
        let mut found: HashSet<WidgetId> = HashSet::new();
        for &id in changed {
            let boundary = self.ancestors(id).skip(1).find(|&a| {
                self.layout
                    .is_boundary(self.node(a).expect("alive").layout_id)
            })?;
            if Some(boundary) == self.root {
                return None;
            }
            if found.insert(boundary) {
                boundaries.push(boundary);
            }
        }
        if boundaries.is_empty() {
            return None;
        }
        // A boundary inside another one is laid out as part of the outer one.
        let outer: Vec<WidgetId> = boundaries
            .iter()
            .copied()
            .filter(|&b| !self.ancestors(b).skip(1).any(|a| found.contains(&a)))
            .collect();
        Some(outer)
    }

    fn layout_boundary(
        &mut self,
        boundary: WidgetId,
        text: &mut TextStore,
        resized: &mut Vec<WidgetId>,
    ) {
        let node = self.node(boundary).expect("alive");
        let (layout_id, rect) = (node.layout_id, node.layout.rect);
        {
            let Self {
                slots,
                layout,
                stats,
                ..
            } = self;
            layout.compute_subtree(
                layout_id,
                Size::new(rect.width, rect.height),
                |id, input| {
                    stats.measured += 1;
                    let node = slots[id.index as usize]
                        .node
                        .as_mut()
                        .expect("measured widget is alive");
                    node.widget.measure(
                        &mut MeasureCx {
                            text: &mut *text,
                            style: node.resolved_style.as_deref().unwrap_or(&StylePatch::EMPTY),
                        },
                        input,
                    )
                },
            );
        }

        // The boundary keeps its position; only its content extent can change.
        let content_size = self.layout.layout(layout_id).content_size;
        let node = self.node_mut(boundary).expect("alive");
        node.layout_touched = false;
        if node.layout.content_size != content_size {
            node.layout.content_size = content_size;
            resized.push(boundary);
        }
        let count = node.children.len();
        let old_bounds = node.subtree_bounds;
        for i in 0..count {
            let child = self.node(boundary).expect("alive").children[i];
            read_back(
                &mut self.slots,
                &self.layout,
                child,
                false,
                &mut self.stats,
                resized,
            );
        }
        update_subtree_bounds(&mut self.slots, boundary);

        // Walk up while a drawn extent changed: the changed child may now be out of
        // order among its siblings, and ancestors' culling bounds may only need to grow
        // to cover the new content.
        let mut changed = self.node(boundary).expect("alive").subtree_bounds != old_bounds;
        let mut child = boundary;
        while changed && let Some(parent) = self.parent(child) {
            if !keeps_y_order(&self.slots, parent, child) {
                self.node_mut(parent).expect("alive").children_sorted_y = false;
            }
            let parent_node = self.node(parent).expect("alive");
            if parent_node.clips_children() {
                break;
            }
            let offset = parent_node.widget.child_offset();
            let child_node = self.node(child).expect("alive");
            let grown = child_node.subtree_bounds.translate(
                child_node.layout.rect.x - offset.x,
                child_node.layout.rect.y - offset.y,
            );
            let parent_node = self.node_mut(parent).expect("alive");
            let union = parent_node.subtree_bounds.union(&grown);
            changed = union != parent_node.subtree_bounds;
            parent_node.subtree_bounds = union;
            child = parent;
        }
    }

    fn run_paint(&mut self, text: &mut TextStore) {
        let (focused, hovered) = (self.focused, self.hovered);
        for id in std::mem::take(&mut self.paint_queue) {
            // A hidden widget was not laid out (or measured) and is not drawn. It stays
            // dirty and goes back in the queue when it is shown (`queue_shown_paint`).
            if self.is_hidden(id) {
                continue;
            }
            let disabled = !self.is_enabled(id);
            let Self { slots, theme, .. } = self;
            let Some(node) = slots
                .get_mut(id.index as usize)
                .filter(|s| s.generation == id.generation)
                .and_then(|s| s.node.as_mut())
            else {
                continue;
            };
            if !node.dirty.contains(Dirty::PAINT) {
                continue;
            }
            node.dirty.remove(Dirty::PAINT);
            let Node {
                widget,
                under,
                over,
                layout,
                resolved_style,
                ..
            } = node;
            for (commands, above) in [(under, false), (over, true)] {
                commands.clear();
                let mut cx = PaintCx {
                    size: Size::new(layout.rect.width, layout.rect.height),
                    content_size: layout.content_size,
                    focused: focused == Some(id),
                    hovered: hovered == Some(id),
                    disabled,
                    theme,
                    style: resolved_style.as_deref().unwrap_or(&StylePatch::EMPTY),
                    commands,
                    text: &mut *text,
                };
                if above {
                    widget.paint_over(&mut cx);
                } else {
                    if !widget.paints_style_box() {
                        paint_style_box(&mut cx);
                    }
                    widget.paint(&mut cx);
                }
            }
            self.stats.repainted += 1;
        }
    }

    fn assemble(&mut self, root: WidgetId, viewport: Size) {
        self.list.clear();
        let window = Rect::new(0.0, 0.0, viewport.width, viewport.height);
        emit(
            &self.slots,
            root,
            Point::default(),
            window,
            1.0,
            &mut self.list,
            &mut self.stats,
        );
        for overlay in &self.overlays {
            emit(
                &self.slots,
                overlay.root,
                Point::default(),
                window,
                1.0,
                &mut self.list,
                &mut self.stats,
            );
        }
        self.stats.commands = self.list.len();
    }

    /// The topmost widget under `pos`, respecting clips and scroll offsets.
    /// Overlays are tested first, topmost first; below a modal overlay nothing else is
    /// reachable (the modal overlay's root receives the event instead).
    pub fn hit_test(&self, pos: Point) -> Option<WidgetId> {
        self.hit_test_with(pos, false)
    }

    /// [`UiTree::hit_test`], but disabled widgets count too, for pointing at them in an
    /// inspector (where their `:disabled` styles matter).
    pub fn hit_test_including_disabled(&self, pos: Point) -> Option<WidgetId> {
        self.hit_test_with(pos, true)
    }

    fn hit_test_with(&self, pos: Point, disabled: bool) -> Option<WidgetId> {
        let root = self.root?;
        let viewport = self.viewport?;
        let window = Rect::new(0.0, 0.0, viewport.width, viewport.height);
        for overlay in self.overlays.iter().rev() {
            if overlay.options.pass_through {
                continue;
            }
            if let Some(found) = hit(
                &self.slots,
                overlay.root,
                Point::default(),
                window,
                pos,
                disabled,
            ) {
                return Some(found);
            }
            if overlay.options.modal {
                return Some(overlay.root);
            }
        }
        hit(&self.slots, root, Point::default(), window, pos, disabled)
    }

    /// In window coordinates.
    pub fn window_origin(&self, id: WidgetId) -> Option<Point> {
        let node = self.node(id)?;
        let mut p = Point::new(node.layout.rect.x, node.layout.rect.y);
        let mut current = node.parent;
        while let Some(parent) = current {
            let parent = self.node(parent).expect("ancestors are alive");
            let offset = parent.widget.child_offset();
            p.x += parent.layout.rect.x - offset.x;
            p.y += parent.layout.rect.y - offset.y;
            current = parent.parent;
        }
        Some(p)
    }

    /// In window coordinates.
    pub fn window_rect(&self, id: WidgetId) -> Option<Rect> {
        let origin = self.window_origin(id)?;
        let size = self.node(id)?.layout.rect;
        Some(Rect::new(origin.x, origin.y, size.width, size.height))
    }

    /// In window coordinates. Reflects the last [`UiTree::prepare`].
    pub fn text_input_area(&self) -> Option<Rect> {
        let focused = self.focused?;
        let area = self.node(focused)?.widget.text_input_area()?;
        let origin = self.window_origin(focused)?;
        Some(area.translate(origin.x, origin.y))
    }

    /// What the input method may ask about the focused widget's text, in window
    /// coordinates (see [`Widget::ime_context`]).
    pub fn ime_context(&self, text: &TextStore) -> Option<crate::ImeContext> {
        let focused = self.focused?;
        let context = self.node(focused)?.widget.ime_context(text)?;
        let origin = self.window_origin(focused)?;
        Some(context.translate(origin.x, origin.y))
    }

    pub fn focused(&self) -> Option<WidgetId> {
        self.focused
    }

    pub fn hovered(&self) -> Option<WidgetId> {
        self.hovered
    }

    /// Focus `id`, or clear focus with `None`. A widget that is gone, not focusable or
    /// disabled or hidden cannot take focus; the request is then ignored.
    pub fn set_focus(&mut self, id: Option<WidgetId>) {
        if let Some(target) = id
            && !(self.node(target).is_some_and(|n| n.widget.focusable())
                && self.is_enabled(target)
                && !self.is_hidden(target))
        {
            return;
        }
        if let Some(target) = id
            && let Some(group) = self.focus_group_of(target)
        {
            self.group_focus.retain(|(g, _)| *g != group);
            self.group_focus.push((group, target));
        }
        if id != self.focused {
            let old = std::mem::replace(&mut self.focused, id);
            for changed in [old, id].into_iter().flatten() {
                self.invalidate(changed, Dirty::PAINT);
            }
            self.focus_events.extend(old.map(|o| (o, false)));
            self.focus_events.extend(id.map(|n| (n, true)));
            self.focus_visible = !self.pointer_input;
            self.reveal_focus = id.is_some() && self.focus_visible;
            self.refresh_style_transition(old, id, true);
        }
    }

    /// Scroll every ancestor of the focused widget that scrolls (innermost first) so
    /// the widget is in view: keyboard focus that moves out of sight is lost to the
    /// person following it (WCAG 2.4.11).
    fn reveal_focused(&mut self) {
        let Some(focused) = self.focused else {
            return;
        };
        let ancestors: Vec<WidgetId> = self.ancestors(focused).skip(1).collect();
        for ancestor in ancestors {
            // Window coordinates, read again after each inner scroll.
            let (Some(target), Some(origin)) =
                (self.window_rect(focused), self.window_origin(ancestor))
            else {
                return;
            };
            let (now, reduce_motion) = (self.now, self.reduce_motion);
            let node = self.node_mut(ancestor).expect("ancestors are alive");
            let offset = node.widget.child_offset();
            let rect = Rect::new(
                target.x - origin.x + offset.x,
                target.y - origin.y + offset.y,
                target.width,
                target.height,
            );
            let layout = node.layout;
            let mut cx = UpdateCx {
                now,
                reduce_motion,
                ..Default::default()
            };
            if node.widget.reveal(
                &mut cx,
                Size::new(layout.rect.width, layout.rect.height),
                layout.content_size,
                rect,
            ) {
                self.invalidate(ancestor, cx.dirty);
            }
        }
    }

    fn deliver_focus_changes(&mut self, text: &mut TextStore) {
        // A widget that keeps moving focus in its handler cannot loop forever.
        for _ in 0..64 {
            if self.focus_events.is_empty() {
                return;
            }
            for (id, focused) in std::mem::take(&mut self.focus_events) {
                if self.contains(id) {
                    self.deliver(id, &Event::FocusChanged(focused), text);
                }
            }
        }
        self.focus_events.clear();
    }

    /// While a modal overlay is open, focus cycles inside it; otherwise through the main
    /// tree and then the open overlays.
    pub fn focus_next(&mut self, reverse: bool) {
        let mut order = Vec::new();
        match (self.modal_overlay(), self.focus_scope_root()) {
            (Some(modal), _) => self.collect_focusable(modal, &mut order),
            (None, Some(scope)) => {
                for &child in self.children(scope) {
                    self.collect_focusable(child, &mut order);
                }
            }
            (None, None) => {
                let roots: Vec<WidgetId> = self
                    .root
                    .into_iter()
                    .chain(self.overlays.iter().map(|o| o.root))
                    .collect();
                for root in roots {
                    self.collect_focusable(root, &mut order);
                }
            }
        }
        if order.is_empty() {
            return;
        }
        let current = self
            .focused
            .and_then(|f| order.iter().position(|&id| id == f));
        let next = match (current, reverse) {
            (None, false) => 0,
            (None, true) => order.len() - 1,
            (Some(i), false) => (i + 1) % order.len(),
            (Some(i), true) => (i + order.len() - 1) % order.len(),
        };
        self.set_focus(Some(order[next]));
    }

    /// Ctrl+Tab (Shift for backwards): like Tab, but out of the focus scope (inner
    /// window) that has the focus, which Tab cycles inside, to the next focus stop after
    /// it. Without a scope it is Tab, for widgets that take Tab themselves (a terminal).
    /// The keyboard can always leave a component it entered (WCAG 2.1.2). An open
    /// modal overlay keeps the focus, as with Tab.
    fn focus_out(&mut self, reverse: bool) {
        let scope = self.focused.and_then(|focused| {
            self.ancestors(focused)
                .filter(|&a| self.node(a).is_some_and(|n| n.widget.focus_scope()))
                .last()
        });
        let (None, Some(scope)) = (self.modal_overlay(), scope) else {
            self.focus_next(reverse);
            return;
        };
        let mut order = Vec::new();
        let roots: Vec<WidgetId> = self
            .root
            .into_iter()
            .chain(self.overlays.iter().map(|o| o.root))
            .collect();
        for root in roots {
            self.collect_focusable(root, &mut order);
        }
        let inside: Vec<bool> = order
            .iter()
            .map(|&id| self.ancestors(id).any(|a| a == scope))
            .collect();
        let (Some(first), Some(last)) = (
            inside.iter().position(|&i| i),
            inside.iter().rposition(|&i| i),
        ) else {
            return;
        };
        let n = order.len();
        let next = (1..n)
            .map(|step| {
                if reverse {
                    (first + n - step) % n
                } else {
                    (last + step) % n
                }
            })
            .find(|&i| !inside[i]);
        if let Some(next) = next {
            self.set_focus(Some(order[next]));
        }
    }

    fn focus_scope_of_focus(&self) -> Option<WidgetId> {
        let focused = self.focused?;
        self.ancestors(focused)
            .find(|&a| self.node(a).is_some_and(|n| n.widget.focus_scope()))
    }

    fn modal_child(&self, scope: WidgetId) -> Option<WidgetId> {
        modal_child_of(&self.slots, self.node(scope)?)
    }

    fn focus_scope_root(&self) -> Option<WidgetId> {
        let scope = self.focus_scope_of_focus()?;
        Some(self.modal_child(scope).unwrap_or(scope))
    }

    /// A focus group contributes one stop: the item it focused last, or its first item.
    fn collect_focusable(&self, id: WidgetId, out: &mut Vec<WidgetId>) {
        let node = self.node(id).expect("alive");
        if !node.enabled || node.is_hidden() {
            return;
        }
        if node.widget.focus_group() {
            let mut items = Vec::new();
            self.collect_group_items(id, &mut items);
            let remembered = self
                .group_focus
                .iter()
                .find(|(g, _)| *g == id)
                .map(|(_, item)| *item)
                .filter(|item| items.contains(item));
            out.extend(remembered.or(items.first().copied()));
            return;
        }
        if node.widget.focusable() {
            out.push(id);
        }
        for &child in &node.children {
            self.collect_focusable(child, out);
        }
    }

    fn collect_group_items(&self, id: WidgetId, out: &mut Vec<WidgetId>) {
        let node = self.node(id).expect("alive");
        if !node.enabled || node.is_hidden() {
            return;
        }
        if node.widget.focusable() {
            out.push(id);
        }
        for &child in &node.children {
            self.collect_group_items(child, out);
        }
    }

    fn focus_group_of(&self, id: WidgetId) -> Option<WidgetId> {
        self.ancestors(id)
            .skip(1)
            .find(|&a| self.node(a).is_some_and(|n| n.widget.focus_group()))
    }

    fn focus_in_group(&mut self, forward: bool) -> bool {
        let Some(focused) = self.focused else {
            return false;
        };
        let Some(group) = self.focus_group_of(focused) else {
            return false;
        };
        let mut items = Vec::new();
        self.collect_group_items(group, &mut items);
        let Some(i) = items.iter().position(|&id| id == focused) else {
            return false;
        };
        let n = items.len();
        let next = if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        };
        self.set_focus(Some(items[next]));
        true
    }

    fn update_hover(&mut self, id: Option<WidgetId>, text: &mut TextStore) {
        let old = self.hovered;
        if id == old {
            return;
        }
        self.set_hovered(id);
        if let Some(old) = old.filter(|&o| self.contains(o)) {
            self.deliver(old, &Event::HoverChanged(false), text);
        }
        if let Some(new) = id {
            self.deliver(new, &Event::HoverChanged(true), text);
        }
    }

    fn set_hovered(&mut self, id: Option<WidgetId>) {
        if id != self.hovered {
            let old = std::mem::replace(&mut self.hovered, id);
            for changed in [old, id].into_iter().flatten() {
                self.invalidate(changed, Dirty::PAINT);
            }
            self.refresh_style_transition(old, id, false);
        }
    }

    /// Deliver an input event. Pointer events go to the widget under the pointer (or
    /// the one capturing it), keyboard and text events to the focused widget; each then
    /// bubbles to ancestors until handled. Unhandled Tab moves focus.
    pub fn dispatch(&mut self, event: Event, text: &mut TextStore) {
        if let Some(steps) = &mut self.recording
            && let Some(step) = crate::recording::Step::new(self.now, &event)
        {
            steps.push(step);
        }
        self.dispatch_event(event, text);
        self.deliver_focus_changes(text);
    }

    fn dispatch_event(&mut self, event: Event, text: &mut TextStore) {
        match &event {
            Event::PointerMove { pos }
            | Event::PointerDown { pos, .. }
            | Event::PointerUp { pos, .. } => self.pointer = Some(*pos),
            Event::PointerLeave => self.pointer = None,
            _ => {}
        }
        match &event {
            Event::PointerDown { .. } => self.pointer_input = true,
            Event::KeyDown { .. } => self.pointer_input = false,
            _ => {}
        }
        match &event {
            Event::PointerMove { pos } if self.drag.is_some() => {
                self.drag_over(*pos, text);
            }
            Event::PointerUp { pos, .. } if self.drag.is_some() => {
                self.end_drag(*pos, true, text);
                self.set_captured(None);
            }
            Event::KeyDown {
                key: Key::Escape, ..
            } if self.drag.is_some() => {
                let pos = Point::default();
                self.end_drag(pos, false, text);
                self.set_captured(None);
            }
            Event::DragOver { pos, data } => {
                match &mut self.drag {
                    // The OS reports the files of a drag one after the other: what is
                    // dragged is what the latest event says.
                    Some(drag) if drag.source.is_none() => drag.data = data.clone(),
                    Some(_) => {}
                    None => {
                        self.drag = Some(Drag {
                            data: data.clone(),
                            source: None,
                            target: None,
                        });
                    }
                }
                self.drag_over(*pos, text);
            }
            Event::Drop { pos, data } => {
                if self.drag.is_none() {
                    self.drag = Some(Drag {
                        data: data.clone(),
                        source: None,
                        target: None,
                    });
                    self.drag_over(*pos, text);
                }
                if let Some(drag) = &mut self.drag {
                    drag.data = data.clone();
                }
                self.end_drag(*pos, true, text);
            }
            Event::DragLeave => self.end_drag(Point::default(), false, text),
            Event::PointerMove { pos } => {
                let hit = self.hit_test(*pos);
                self.update_hover(hit, text);
                if self.captured.is_none() && self.drag.is_none() {
                    self.track_tooltip(*pos, hit, text);
                    self.track_hover_card(hit);
                }
                if let Some(target) = self.captured.or(hit) {
                    self.bubble(target, &event, text);
                }
                if self.drag.is_some() {
                    self.drag_over(*pos, text);
                }
            }
            Event::PointerDown { pos, button } => {
                self.hide_tooltip(true, text);
                let hit = self.hit_test(*pos);
                self.hover_card.due = None;
                if self.hover_card.panel.is_some() && !self.in_hover_card(hit) {
                    self.hide_hover_card(text);
                }
                if !self.overlays.is_empty() && self.dismiss_overlays_outside(hit, text) {
                    return;
                }
                // Dismissing may have removed what was under the pointer.
                let hit = self.hit_test(*pos);
                // A press outside a modal overlay does nothing (focus stays inside it).
                if let Some(modal) = self.modal_overlay()
                    && hit == Some(modal)
                    && !self.window_rect(modal).is_some_and(|r| r.contains(*pos))
                {
                    return;
                }
                self.update_hover(hit, text);
                let focus = hit.and_then(|h| {
                    self.ancestors(h)
                        .find(|&a| self.node(a).is_some_and(|n| n.widget.focusable()))
                });
                let in_popup = hit
                    .and_then(|h| self.overlay_of(h))
                    .is_some_and(|i| self.overlays[i].keeps_focus);
                if focus.is_some() || !in_popup {
                    self.set_focus(focus);
                }
                // A right click, or a Ctrl-click on a Mac (Ctrl is not the command key
                // there), opens the context menu of the innermost widget that has one.
                let secondary = *button == PointerButton::Secondary
                    || (*button == PointerButton::Primary
                        && self.modifiers.ctrl
                        && !self.modifiers.command);
                if secondary
                    && let Some(hit) = hit
                    && let Some((owner, items)) = self.context_menu_at(hit, *pos)
                {
                    self.open_context_menu(owner, items, *pos);
                    return;
                }
                if let Some(target) = hit {
                    let observers: Vec<WidgetId> = self
                        .ancestors(target)
                        .filter(|&a| self.node(a).is_some_and(|n| n.widget.observes_presses()))
                        .collect();
                    for observer in observers {
                        self.deliver(observer, &Event::PressWithin, text);
                    }
                    if self.contains(target) {
                        self.bubble(target, &event, text);
                    }
                }
            }
            Event::PointerUp { pos, .. } => {
                if let Some(target) = self.captured.or_else(|| self.hit_test(*pos)) {
                    self.bubble(target, &event, text);
                }
                self.set_captured(None);
            }
            Event::PointerLeave => {
                self.hide_tooltip(false, text);
                self.track_hover_card(None);
                self.update_hover(None, text);
                if let Some(target) = self.captured {
                    self.bubble(target, &event, text);
                }
            }
            Event::Wheel { pos, .. } => {
                self.hide_tooltip(true, text);
                self.hide_hover_card(text);
                if let Some(target) = self.hit_test(*pos) {
                    self.bubble(target, &event, text);
                }
            }
            Event::KeyDown { key, modifiers } => {
                self.modifiers = *modifiers;
                self.hide_tooltip(true, text);
                let handled = match self.key_target() {
                    Some(target) => self.bubble(target, &event, text),
                    None => false,
                };
                if handled {
                    return;
                }
                match key {
                    Key::Tab if modifiers.ctrl => {
                        self.focus_out(modifiers.shift);
                        return;
                    }
                    Key::Tab => {
                        self.focus_next(modifiers.shift);
                        return;
                    }
                    Key::ContextMenu | Key::F10
                        if (*key == Key::ContextMenu || modifiers.shift)
                            && let Some(focused) = self.focused
                            && let Some(rect) = self.window_rect(focused)
                            && let Some((owner, items)) = self.context_menu_at(
                                focused,
                                Point::new(rect.x + 1.0, rect.y + 1.0),
                            ) =>
                    {
                        self.open_context_menu(owner, items, Point::new(rect.x, rect.bottom()));
                        return;
                    }
                    Key::ArrowLeft | Key::ArrowUp | Key::ArrowRight | Key::ArrowDown
                        if self.focus_in_group(matches!(key, Key::ArrowRight | Key::ArrowDown)) =>
                    {
                        return;
                    }
                    _ => {}
                }
                if *key == Key::Escape && self.hover_card.panel.is_some() {
                    self.hide_hover_card(text);
                    return;
                }
                if *key == Key::Escape
                    && let Some(top) = self.overlays.last()
                    && top.options.light_dismiss
                {
                    let root = top.root;
                    self.dismiss_overlay(root, text);
                    return;
                }
                if self.activate_key_equivalent(*key, *modifiers, text) {
                    return;
                }
                if let Some(&(shortcut, target)) = self
                    .shortcuts
                    .iter()
                    .find(|(s, _)| s.matches(*key, *modifiers))
                    && self.contains(target)
                    && self.modal_overlay().is_none()
                {
                    self.messages
                        .push(Message::new(target, Box::new(ShortcutPressed(shortcut))));
                }
            }
            Event::Text(_) | Event::Composition { .. } | Event::ImeSelect(_) => {
                if let Some(target) = self.key_target() {
                    self.bubble(target, &event, text);
                }
            }
            Event::ModifiersChanged(modifiers) => self.modifiers = *modifiers,
            // Generated by the tree itself; delivered without bubbling.
            Event::HoverChanged(_)
            | Event::AnimationFrame { .. }
            | Event::Timer { .. }
            | Event::DragEnd { .. }
            | Event::Activate
            | Event::ActivateItem(_)
            | Event::MenuClosed { .. }
            | Event::KeyEquivalent(_)
            | Event::PressWithin
            | Event::PopoverClosed
            | Event::FromPopover(_)
            | Event::FromOwner(_)
            | Event::FocusChanged(_)
            | Event::Increment
            | Event::Decrement => {}
        }
    }

    /// Where keys and text go: the focused widget, or the topmost modal overlay's root if
    /// focus is not inside it (nothing below a modal overlay gets keys).
    fn key_target(&self) -> Option<WidgetId> {
        match self.modal_overlay() {
            Some(modal)
                if !self
                    .focused
                    .is_some_and(|f| self.ancestors(f).any(|a| a == modal)) =>
            {
                Some(modal)
            }
            Some(_) => self.focused,
            None => {
                // A widget modal within the focused scope takes the keys unless focus
                // is already inside it.
                let focused = self.focused?;
                let modal = self
                    .focus_scope_of_focus()
                    .and_then(|scope| self.modal_child(scope))
                    .filter(|&modal| !self.ancestors(focused).any(|a| a == modal));
                Some(modal.unwrap_or(focused))
            }
        }
    }

    /// When the next timer is due (seconds on the tree's clock): a pending tooltip or
    /// hover card, or a widget's [`EventCx::request_timer`](crate::EventCx::request_timer).
    /// The app should draw a frame then, which calls [`UiTree::animate`]. `None` when
    /// nothing is waiting.
    pub fn next_timer(&self) -> Option<f64> {
        [
            self.tooltip.due,
            self.hover_card.due,
            self.hover_card.close_due,
        ]
        .into_iter()
        .flatten()
        .chain(self.timers.iter().map(|&(at, _)| at))
        .min_by(f64::total_cmp)
    }

    pub fn tooltip_overlay(&self) -> Option<WidgetId> {
        self.tooltip.overlay
    }

    /// The closed hand while a widget's drag-and-drop is under way; the arrow over a
    /// disabled widget or outside the window. The platform reads it after each input
    /// event and frame.
    pub fn cursor(&self) -> Cursor {
        let Some(pos) = self.pointer else {
            return Cursor::Default;
        };
        if self.drag.as_ref().is_some_and(|d| d.source.is_some()) {
            return Cursor::Grabbing;
        }
        let ask = |id: WidgetId| {
            let node = self.node(id)?;
            let origin = self.window_origin(id)?;
            let local = Point::new(pos.x - origin.x, pos.y - origin.y);
            let size = Size::new(node.layout.rect.width, node.layout.rect.height);
            node.widget.cursor(local, size)
        };
        if let Some(captured) = self.captured {
            if !self.is_enabled(captured) {
                return Cursor::Default;
            }
            return ask(captured).unwrap_or_default();
        }
        let Some(hit) = self.hit_test(pos) else {
            return Cursor::Default;
        };
        if !self.is_enabled(hit) {
            return Cursor::Default;
        }
        self.ancestors(hit).find_map(ask).unwrap_or_default()
    }

    fn track_tooltip(&mut self, pos: Point, hit: Option<WidgetId>, text: &mut TextStore) {
        self.tooltip.pos = pos;
        let found = hit.and_then(|h| {
            self.ancestors(h).find_map(|a| {
                let origin = self.window_origin(a)?;
                let local = Point::new(pos.x - origin.x, pos.y - origin.y);
                let tip = self.node(a)?.widget.tooltip(local)?;
                (!tip.is_empty()).then_some((a, tip))
            })
        });
        if found == self.tooltip.source {
            return;
        }
        self.hide_tooltip(false, text);
        let Some(found) = found else { return };
        self.tooltip.source = Some(found);
        let recent = self
            .tooltip
            .closed_at
            .is_some_and(|t| self.now - t < TOOLTIP_GRACE);
        if recent {
            self.show_tooltip();
        } else {
            self.tooltip.due = Some(self.now + TOOLTIP_DELAY);
        }
    }

    /// Close the tooltip and cancel a pending one. With `keep_source` (a click, a key,
    /// scrolling) it does not come back until the pointer moves to another tooltip.
    fn hide_tooltip(&mut self, keep_source: bool, text: &mut TextStore) {
        self.tooltip.due = None;
        if !keep_source {
            self.tooltip.source = None;
        }
        if let Some(overlay) = self.tooltip.overlay.take() {
            self.remove(overlay, text);
            self.tooltip.closed_at = Some(self.now);
        }
    }

    fn show_tooltip(&mut self) {
        self.tooltip.due = None;
        let Some((source, tip)) = self.tooltip.source.clone() else {
            return;
        };
        if !self.contains(source) {
            self.tooltip.source = None;
            return;
        }
        let pos = self.tooltip.pos;
        let cursor = Rect::new(pos.x, pos.y, 0.0, 16.0);
        let overlay = self.open_overlay(
            crate::widgets::Tooltip::new(tip),
            Placement::Below(cursor),
            OverlayOptions::TOOLTIP,
        );
        self.request_animation(overlay);
        self.tooltip.overlay = Some(overlay);
    }

    pub fn hover_card_overlay(&self) -> Option<WidgetId> {
        self.hover_card.panel
    }

    fn in_hover_card(&self, hit: Option<WidgetId>) -> bool {
        let state = &self.hover_card;
        hit.is_some_and(|h| {
            self.ancestors(h)
                .any(|a| Some(a) == state.panel || Some(a) == state.source)
        })
    }

    fn track_hover_card(&mut self, hit: Option<WidgetId>) {
        if self.hover_card.panel.is_some() {
            if self.in_hover_card(hit) {
                self.hover_card.close_due = None;
            } else if self.hover_card.close_due.is_none() {
                self.hover_card.close_due = Some(self.now + HOVER_CARD_CLOSE_DELAY);
            }
            return;
        }
        let found = hit.and_then(|h| {
            self.ancestors(h).find(|&a| {
                self.node(a)
                    .is_some_and(|n| n.enabled && n.widget.hover_card().is_some())
            })
        });
        if found != self.hover_card.source {
            self.hover_card.source = found;
            self.hover_card.due = found.map(|_| self.now + HOVER_CARD_DELAY);
        }
    }

    fn show_hover_card(&mut self) {
        self.hover_card.due = None;
        let Some(source) = self.hover_card.source.filter(|&s| self.contains(s)) else {
            self.hover_card.source = None;
            return;
        };
        let edge = self.node(source).and_then(|n| n.widget.hover_card());
        let card = self.children(source).get(1).copied();
        let (Some(edge), Some(card), Some(anchor)) = (edge, card, self.window_rect(source)) else {
            return;
        };
        let panel = self.open_overlay(
            crate::widgets::HoverCardPanel::new(),
            Placement::Edge { anchor, edge },
            OverlayOptions::PASSIVE,
        );
        self.reparent(card, panel);
        self.request_animation(panel);
        self.hover_card.panel = Some(panel);
    }

    fn hide_hover_card(&mut self, text: &mut TextStore) {
        self.hover_card.due = None;
        self.hover_card.close_due = None;
        if let Some(panel) = self.hover_card.panel.take() {
            let card = self.children(panel).first().copied();
            if let (Some(card), Some(source)) =
                (card, self.hover_card.source.filter(|&s| self.contains(s)))
            {
                self.reparent(card, source);
            }
            self.remove(panel, text);
            self.hover_card.source = None;
        }
    }

    /// `id` followed by its ancestors up to the root.
    pub fn ancestors(&self, id: WidgetId) -> impl Iterator<Item = WidgetId> + '_ {
        std::iter::successors(Some(id), |&id| self.parent(id))
    }

    fn bubble(&mut self, start: WidgetId, event: &Event, text: &mut TextStore) -> bool {
        let mut current = Some(start);
        while let Some(id) = current {
            let Some((handled, parent)) = self.deliver(id, event, text) else {
                return false;
            };
            if handled {
                return true;
            }
            current = parent;
        }
        false
    }

    /// Call `id`'s event handler and apply its requests. Returns whether it handled the
    /// event and its parent, or `None` if `id` is gone.
    fn deliver(
        &mut self,
        id: WidgetId,
        event: &Event,
        text: &mut TextStore,
    ) -> Option<(bool, Option<WidgetId>)> {
        let origin = self.window_origin(id)?;
        let (focused, hovered, captured, now, modifiers, reduce_motion) = (
            self.focused == Some(id),
            self.hovered == Some(id),
            self.captured == Some(id),
            self.now,
            self.modifiers,
            self.reduce_motion,
        );
        let Self {
            slots, clipboard, ..
        } = self;
        let node = slots
            .get_mut(id.index as usize)
            .filter(|s| s.generation == id.generation)?
            .node
            .as_mut()?;
        let mut cx = EventCx {
            id,
            origin,
            size: Size::new(node.layout.rect.width, node.layout.rect.height),
            content_size: node.layout.content_size,
            focused,
            hovered,
            captured,
            now,
            reduce_motion,
            modifiers,
            text: &mut *text,
            clipboard: clipboard.as_mut(),
            requests: Requests::default(),
        };
        node.widget.event(&mut cx, event);
        let parent = node.parent;
        let requests = cx.requests;

        self.invalidate(id, requests.dirty);
        if requests.focus {
            self.set_focus(Some(id));
        }
        match requests.capture {
            Some(true) => self.set_captured(Some(id)),
            Some(false) if self.captured == Some(id) => self.set_captured(None),
            _ => {}
        }
        if requests.animation {
            self.request_animation(id);
        }
        if let Some(at) = requests.timer {
            self.set_timer(id, at);
        }
        if requests.accept_drop {
            self.drop_accepted = true;
        }
        if requests.close_children {
            self.close_overlays_owned_by(id, text);
        }
        if let Some(request) = requests.menu {
            self.open_menu_overlay(id, request);
        }
        if let Some(chosen) = requests.close_menu {
            self.close_owned_overlay(id, chosen, text);
        }
        if let Some(request) = requests.popover {
            self.open_popover_overlay(id, request);
        }
        if let Some(request) = requests.popup {
            self.open_popup_overlay(id, request);
        }
        if !requests.to_popup.is_empty()
            && let Some(popup) = self
                .overlays
                .iter()
                .find(|o| o.owner == Some(id) && !o.menu)
                .map(|o| o.root)
        {
            for payload in requests.to_popup {
                let event = Event::FromOwner(crate::event::PopoverMessage(payload));
                self.deliver(popup, &event, text);
            }
        }
        if !requests.to_owner.is_empty() || requests.close_popover {
            let overlay = self.overlay_of(id);
            let (root, owner) = match overlay {
                Some(i) => (Some(self.overlays[i].root), self.overlays[i].owner),
                None => (None, None),
            };
            if let Some(owner) = owner {
                for payload in requests.to_owner {
                    let event = Event::FromPopover(crate::event::PopoverMessage(payload));
                    self.deliver(owner, &event, text);
                }
            }
            if let Some(root) = root.filter(|_| requests.close_popover) {
                self.close_owned_overlay(root, None, text);
            } else if requests.close_popover
                && overlay.is_none()
                && self.node(id).is_some_and(|n| n.widget.modal())
            {
                let scope = self.parent(id);
                self.remove(id, text);
                if self.focused.is_none() {
                    self.set_focus(scope);
                }
            }
        }
        if let Some(delta) = requests.move_overlay
            && let Some(index) = self.overlay_of(id)
        {
            let rect = self
                .node(self.overlays[index].root)
                .expect("overlay root is alive")
                .layout
                .rect;
            let at = Point::new(rect.x + delta.x, rect.y + delta.y);
            let root = self.overlays[index].root;
            self.set_overlay_placement(root, Placement::At(at));
        }
        if let Some(data) = requests.drag
            && self.drag.is_none()
        {
            self.drag = Some(Drag {
                data,
                source: Some(id),
                target: None,
            });
        }
        let clicked = requests
            .messages
            .iter()
            .any(|m| m.is::<crate::widgets::ButtonClicked>());
        self.messages.extend(
            requests
                .messages
                .into_iter()
                .map(|payload| Message::new(id, payload)),
        );
        if clicked && let Some(index) = self.overlay_of(id) {
            let root = self.overlays[index].root;
            if self
                .get::<crate::widgets::Popover>(root)
                .is_some_and(crate::widgets::Popover::closes_when_clicked)
            {
                self.dismiss_overlay(root, text);
            }
        }
        if requests.remove {
            self.remove(id, text);
        }
        Some((requests.handled, parent))
    }

    pub fn take_messages(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.messages)
    }
}

fn node_at(slots: &[Slot], id: WidgetId) -> &Node {
    slots[id.index as usize].node.as_ref().expect("alive")
}

/// Copy layout results for `id`'s subtree into the nodes, descending only where results
/// can have changed. Collects widgets whose size changed (they must repaint).
fn read_back(
    slots: &mut [Slot],
    layout: &LayoutTree<WidgetId>,
    id: WidgetId,
    force: bool,
    stats: &mut UpdateStats,
    resized: &mut Vec<WidgetId>,
) {
    let node = slots[id.index as usize].node.as_mut().expect("alive");
    let new = layout.layout(node.layout_id);
    stats.layouts_read += 1;
    let size_changed = new.rect.width != node.layout.rect.width
        || new.rect.height != node.layout.rect.height
        || new.content_size != node.layout.content_size;
    if size_changed {
        resized.push(id);
    }
    node.layout = new;
    // Taffy only re-lays out a clean subtree when its available space changes, which
    // shows up as a size change here. Otherwise the subtree's relative layout is intact.
    let descend = force || node.layout_touched || size_changed;
    node.layout_touched = false;
    if !descend {
        return;
    }

    let count = node.children.len();
    for i in 0..count {
        let child = slots[id.index as usize]
            .node
            .as_ref()
            .expect("alive")
            .children[i];
        read_back(slots, layout, child, force, stats, resized);
    }

    update_subtree_bounds(slots, id);
}

fn update_subtree_bounds(slots: &mut [Slot], id: WidgetId) {
    let node = node_at(slots, id);
    let own = Rect::new(0.0, 0.0, node.layout.rect.width, node.layout.rect.height);
    let bounds = if node.clips_children() {
        own
    } else {
        let offset = node.widget.child_offset();
        node.children.iter().fold(own, |acc, &child| {
            let child = node_at(slots, child);
            if child.is_hidden() {
                return acc;
            }
            let dx = child.layout.rect.x - offset.x;
            let dy = child.layout.rect.y - offset.y;
            acc.union(&child.subtree_bounds.translate(dx, dy))
        })
    };
    let mut sorted = true;
    let mut previous = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &child in &node.children {
        let (top, bottom) = vertical_extent(node_at(slots, child));
        if top < previous.0 || bottom < previous.1 {
            sorted = false;
            break;
        }
        previous = (top, bottom);
    }
    let node = slots[id.index as usize].node.as_mut().expect("alive");
    node.subtree_bounds = bounds;
    node.children_sorted_y = sorted;
}

fn vertical_extent(child: &Node) -> (f32, f32) {
    let top = child.layout.rect.y + child.subtree_bounds.y;
    (top, top + child.subtree_bounds.height)
}

fn keeps_y_order(slots: &[Slot], parent: WidgetId, child: WidgetId) -> bool {
    let node = node_at(slots, parent);
    if !node.children_sorted_y {
        return false;
    }
    let children = &node.children;
    let i = children
        .iter()
        .position(|&c| c == child)
        .expect("child of its parent");
    let extent = |i: usize| vertical_extent(node_at(slots, children[i]));
    let (top, bottom) = extent(i);
    let after_previous = i == 0 || {
        let (t, b) = extent(i - 1);
        t <= top && b <= bottom
    };
    let before_next = i + 1 == children.len() || {
        let (t, b) = extent(i + 1);
        top <= t && bottom <= b
    };
    after_previous && before_next
}

/// Below this many children a linear scan is as fast as a binary search.
const BINARY_SEARCH_MIN_CHILDREN: usize = 32;

fn modal_child_of(slots: &[Slot], node: &Node) -> Option<WidgetId> {
    node.children.iter().rev().copied().find(|&c| {
        let child = node_at(slots, c);
        !child.is_hidden() && child.widget.modal()
    })
}

fn children_in_band(slots: &[Slot], node: &Node, top: f32, bottom: f32) -> std::ops::Range<usize> {
    let children = &node.children;
    if !node.children_sorted_y || children.len() < BINARY_SEARCH_MIN_CHILDREN {
        return 0..children.len();
    }
    let first = children.partition_point(|&c| vertical_extent(node_at(slots, c)).1 <= top);
    let count =
        children[first..].partition_point(|&c| vertical_extent(node_at(slots, c)).0 < bottom);
    first..first + count
}

/// Append `id`'s subtree to `list`. `parent_origin` is where the parent places children
/// (its window origin minus its scroll offset); `clip` is the visible window area.
fn emit(
    slots: &[Slot],
    id: WidgetId,
    parent_origin: Point,
    clip: Rect,
    opacity: f32,
    list: &mut DisplayList,
    stats: &mut UpdateStats,
) {
    let node = node_at(slots, id);
    let opacity = opacity * node.widget.opacity() * node.style().opacity.unwrap_or(1.0);
    if node.is_hidden() || opacity <= 0.0 {
        return;
    }
    let x = parent_origin.x + node.layout.rect.x;
    let y = parent_origin.y + node.layout.rect.y;
    if !node.subtree_bounds.translate(x, y).intersects(&clip) {
        stats.culled += 1;
        return;
    }
    // Most widgets draw nothing over their children and many nothing under them: skip
    // the empty lists (`assemble` cleared the list, which took a new revision).
    if !node.under.is_empty() {
        list.extend_translated_faded(&node.under, x, y, opacity);
    }

    let own = Rect::new(x, y, node.layout.rect.width, node.layout.rect.height);
    let clips = node.clips_children();
    // A leaf that does not clip adds nothing here (one that clips still pushes and pops
    // its clip): most widgets are leaves.
    let child_clip = if clips {
        clip.intersect(&own)
    } else if node.children.is_empty() {
        None
    } else {
        Some(clip)
    };
    if let Some(child_clip) = child_clip {
        if clips {
            list.push_clip(own);
        }
        let offset = node.widget.child_offset();
        let origin = Point::new(x - offset.x, y - offset.y);
        let band = children_in_band(
            slots,
            node,
            child_clip.y - origin.y,
            child_clip.bottom() - origin.y,
        );
        stats.culled += node.children.len() - band.len();
        for &child in &node.children[band] {
            emit(slots, child, origin, child_clip, opacity, list, stats);
        }
        if clips {
            list.pop_clip();
        }
    }
    if !node.over.is_empty() {
        list.extend_translated_faded(&node.over, x, y, opacity);
    }
}

fn hit(
    slots: &[Slot],
    id: WidgetId,
    parent_origin: Point,
    clip: Rect,
    pos: Point,
    // Whether disabled widgets (and what is inside them) can be hit.
    disabled: bool,
) -> Option<WidgetId> {
    let node = node_at(slots, id);
    let x = parent_origin.x + node.layout.rect.x;
    let y = parent_origin.y + node.layout.rect.y;
    let own = Rect::new(x, y, node.layout.rect.width, node.layout.rect.height);
    if (!node.enabled && !disabled)
        || node.is_hidden()
        || !node.subtree_bounds.translate(x, y).contains(pos)
        || !clip.contains(pos)
    {
        return None;
    }
    let local = Point::new(pos.x - x, pos.y - y);
    if own.contains(pos)
        && node
            .widget
            .claims_pointer(local, Size::new(own.width, own.height))
    {
        return Some(id);
    }
    let child_clip = if node.clips_children() {
        clip.intersect(&own)
    } else {
        Some(clip)
    };
    if let Some(child_clip) = child_clip {
        let offset = node.widget.child_offset();
        let origin = Point::new(x - offset.x, y - offset.y);
        let band = children_in_band(
            slots,
            node,
            pos.y - origin.y,
            pos.y - origin.y + f32::EPSILON,
        );
        // A child modal within this scope is the only content that takes the pointer.
        if node.widget.focus_scope()
            && let Some(modal) = modal_child_of(slots, node)
        {
            return hit(slots, modal, origin, child_clip, pos, disabled)
                .or(own.contains(pos).then_some(id));
        }
        for &child in node.children[band].iter().rev() {
            if let Some(found) = hit(slots, child, origin, child_clip, pos, disabled) {
                return Some(found);
            }
        }
    }
    own.contains(pos).then_some(id)
}

impl StyleTree for UiTree {
    type Id = WidgetId;

    fn style_node(&self, id: WidgetId) -> StyleNode<'_> {
        static NO_META: StyleMeta = StyleMeta::EMPTY;
        let node = self.node(id).expect("node is alive");
        StyleNode {
            widget_type: node.widget.style_type(),
            meta: node.style_meta.as_deref().unwrap_or(&NO_META),
            state: self.style_state(id),
            index: node.sibling_index as usize,
            sibling_count: node.parent.map_or(1, |parent| self.children(parent).len()),
        }
    }

    fn style_parent(&self, id: WidgetId) -> Option<WidgetId> {
        self.parent(id)
    }

    fn style_previous_sibling(&self, id: WidgetId) -> Option<WidgetId> {
        let node = self.node(id)?;
        let before = (node.sibling_index as usize).checked_sub(1)?;
        self.children(node.parent?).get(before).copied()
    }
}

fn paint_style_box(cx: &mut PaintCx) {
    let style = cx.styling();
    let border_width = style.border_width.unwrap_or(0.0);
    if style.background_color.is_none() && border_width <= 0.0 {
        return;
    }
    let (fill, border_color, radius) = (
        style.background_color,
        style.border_color,
        style.border_radius.unwrap_or(0.0),
    );
    let fill = fill.map_or(noa_gpui_scene::Color::TRANSPARENT, |p| cx.color(p));
    let border = noa_gpui_scene::Border {
        width: border_width,
        color: cx.color(border_color.unwrap_or(crate::theme::Role::Outline.into())),
    };
    cx.rounded_rect(cx.bounds(), radius, fill, border);
}
