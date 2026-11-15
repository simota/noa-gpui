use std::ops::Range;

use noa_gpui_layout::Style;
use noa_gpui_layout::style::{Dimension, Size as LayoutSize, length, percent};
use noa_gpui_scene::{Border, Direction, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::inline_label::{ItemRenamed, RenameStep, Renamer};
use super::rows::{
    EdgeScroll, Selection, SelectionMode, TextCache, TextQuery, pinned_group, reveal_rect, step,
};
use super::scroller::Scroller;
use super::table::{RowActivated, selection_fill, stripe};
use super::{Icon, focus_ring};
use crate::event::{Event, Key, Modifiers, PointerButton};
use crate::theme::{Corner, Role, mix};
use crate::view_state;
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

pub const OUTLINE_INDENT: f32 = 16.0;
const DISCLOSURE: f32 = 16.0;
const PAD: f32 = 6.0;
const ICON: f32 = 16.0;
/// The width at which a sidebar shows only its icons (a navigation rail).
pub const OUTLINE_RAIL_WIDTH: f32 = 52.0;
/// How much wider than the rail a sidebar has to be to show its titles fully; in
/// between they fade in as it widens.
const RAIL_FADE: f32 = 96.0;
const DRAG_START: f32 = 4.0;
/// The part of a row at its top and at its bottom where a drag drops between rows;
/// the middle drops into the row's item.
const DROP_EDGE: f32 = 0.25;

#[derive(Debug, Clone, Default)]
pub struct OutlineItem {
    title: String,
    icon: Option<Icon>,
    badge: Option<String>,
    expanded: bool,
    children: Vec<OutlineItem>,
}

impl OutlineItem {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    /// A 16 × 16 image or vector icon (in the accent color) before the title.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn badge(mut self, badge: impl Into<String>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn child(mut self, child: OutlineItem) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = OutlineItem>) -> Self {
        self.children.extend(children);
        self
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn badge_text(&self) -> Option<&str> {
        self.badge.as_deref()
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    pub fn child_items(&self) -> &[OutlineItem] {
        &self.children
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutlineStyle {
    #[default]
    Plain,
    Sidebar,
}

/// Emitted when an item is expanded or collapsed (by the user).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemExpanded {
    pub item: usize,
    pub expanded: bool,
}

/// Emitted after items were dragged to a new place (see [`OutlineView::reorderable`]),
/// in the terms of [`OutlineView::move_items`] and with the ids from before the move:
/// `items` were taken out, in this order, and put under `parent` (`None`: the top
/// level) from `index` on, counted among the children `parent` had left without them.
/// The view has moved them already; a [`super::RowSelectionChanged`] follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemsMoved {
    pub items: Vec<usize>,
    pub parent: Option<usize>,
    pub index: usize,
}

/// An item, flattened. Ids are positions in a depth-first walk of the items given.
#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub title: String,
    pub icon: Option<Icon>,
    pub badge: Option<String>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub depth: usize,
    pub expanded: bool,
}

pub(crate) fn flatten(items: Vec<OutlineItem>) -> (Vec<Node>, Vec<usize>) {
    fn walk(
        item: OutlineItem,
        parent: Option<usize>,
        depth: usize,
        nodes: &mut Vec<Node>,
    ) -> usize {
        let id = nodes.len();
        nodes.push(Node {
            title: item.title,
            icon: item.icon,
            badge: item.badge,
            parent,
            children: Vec::new(),
            depth,
            expanded: item.expanded,
        });
        for child in item.children {
            let c = walk(child, Some(id), depth + 1, nodes);
            nodes[id].children.push(c);
        }
        id
    }
    let mut nodes = Vec::new();
    let roots = items
        .into_iter()
        .map(|item| walk(item, None, 0, &mut nodes))
        .collect();
    (nodes, roots)
}

/// An item and the items under it while the hierarchy changes shape, with its id from
/// before (`None`: a new item).
struct Draft {
    old: Option<usize>,
    node: Node,
    children: Vec<Draft>,
}

impl Draft {
    fn new(item: OutlineItem) -> Self {
        Self {
            old: None,
            node: Node {
                title: item.title,
                icon: item.icon,
                badge: item.badge,
                parent: None,
                children: Vec::new(),
                depth: 0,
                expanded: item.expanded,
            },
            children: item.children.into_iter().map(Draft::new).collect(),
        }
    }
}

fn take_drafts(drafts: &mut Vec<Draft>, take: &impl Fn(usize) -> bool, out: &mut Vec<Draft>) {
    let mut kept = Vec::with_capacity(drafts.len());
    for mut d in drafts.drain(..) {
        if d.old.is_some_and(take) {
            out.push(d);
        } else {
            take_drafts(&mut d.children, take, out);
            kept.push(d);
        }
    }
    *drafts = kept;
}

/// The children of the draft with old id `id` (the top level for `None`).
fn draft_children(drafts: &mut Vec<Draft>, id: Option<usize>) -> Option<&mut Vec<Draft>> {
    let Some(id) = id else {
        return Some(drafts);
    };
    for d in drafts {
        if d.old == Some(id) {
            return Some(&mut d.children);
        }
        if let Some(found) = draft_children(&mut d.children, Some(id)) {
            return Some(found);
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DropAt {
    /// Into `item`, shown at visible position `pos`: as its last child (a sidebar
    /// heading: its first).
    Into { item: usize, pos: usize },
    /// Under `parent` before its child at `index` (counting the dragged items), drawn
    /// as a line at the top of visible position `line`, indented for `depth`.
    Between {
        parent: Option<usize>,
        index: usize,
        line: usize,
        depth: usize,
    },
}

#[derive(Debug, Clone, Copy)]
struct RowDrag {
    at: Point,
    /// The press kept a selection of several rows: if it is released without
    /// dragging, it selects only the pressed row.
    click: bool,
    dragging: bool,
    drop: Option<DropAt>,
}

/// A hierarchy of rows (`NSOutlineView`; with [`OutlineStyle::Sidebar`], a source list).
/// Emits [`ItemExpanded`], [`RowActivated`], [`ItemRenamed`] and [`ItemsMoved`].
///
/// Items are identified by their position in a depth-first walk of the items passed
/// to [`OutlineView::new`]; methods that change the hierarchy number them again by
/// the same walk.
pub struct OutlineView {
    nodes: Vec<Node>,
    roots: Vec<usize>,
    visible: Vec<usize>,
    selection: Selection,
    scroller: Scroller,
    style: OutlineStyle,
    row_height: f32,
    alternating: bool,
    width: Dimension,
    height: f32,
    titles: TextCache<usize>,
    badges: TextCache<usize>,
    size: Size,
    renamable: bool,
    renamer: Renamer,
    heading_at: Vec<usize>,
    floats_group_rows: bool,
    query: TextQuery,
    /// While filtering: whether each item is listed (it matches, or an item under it
    /// does) and whether it is open. The items' own `expanded` stays as it was, for
    /// when the filter is cleared.
    shown: Vec<bool>,
    filter_open: Vec<bool>,
    unmarked: bool,
    reorderable: bool,
    drag: Option<RowDrag>,
    autoscroll: EdgeScroll,
}

impl std::fmt::Debug for OutlineView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutlineView")
            .field("items", &self.nodes.len())
            .field("visible", &self.visible.len())
            .finish_non_exhaustive()
    }
}

impl OutlineView {
    pub fn new(items: Vec<OutlineItem>) -> Self {
        let (nodes, roots) = flatten(items);
        let mut view = Self {
            selection: Selection::new(SelectionMode::Multiple, nodes.len()),
            nodes,
            roots,
            visible: Vec::new(),
            scroller: Scroller::new(),
            style: OutlineStyle::Plain,
            row_height: 22.0,
            alternating: false,
            width: percent(1.0),
            height: 280.0,
            titles: TextCache::default(),
            badges: TextCache::default(),
            size: Size::default(),
            renamable: false,
            renamer: Renamer::default(),
            heading_at: Vec::new(),
            floats_group_rows: true,
            query: TextQuery::default(),
            shown: Vec::new(),
            filter_open: Vec::new(),
            unmarked: false,
            reorderable: false,
            drag: None,
            autoscroll: EdgeScroll::default(),
        };
        view.rebuild();
        view
    }

    pub fn renamable(mut self, renamable: bool) -> Self {
        self.renamable = renamable;
        self
    }

    /// Give the outline the focus too ([`crate::UiTree::set_focus`]). An item the
    /// filter hides is not renamed.
    pub fn rename_item(&mut self, cx: &mut UpdateCx, item: usize) {
        if item < self.nodes.len() && !self.is_heading(item) && self.listed(item) {
            self.start_rename(item);
            cx.request_paint();
        }
    }

    pub fn renaming(&self) -> Option<usize> {
        self.renamer.item()
    }

    pub fn rename_text(&self) -> Option<&str> {
        self.renamer.text()
    }

    pub fn set_title(&mut self, cx: &mut UpdateCx, item: usize, title: impl Into<String>) {
        if let Some(node) = self.nodes.get_mut(item) {
            node.title = title.into();
            cx.request_paint();
        }
    }

    fn start_rename(&mut self, item: usize) {
        if let Some(pos) = self.visible.iter().position(|&v| v == item) {
            self.reveal(pos);
        }
        self.renamer
            .start(item, &self.nodes[item].title, TextStyle::new(13.0));
    }

    fn rename_event(&mut self, cx: &mut EventCx, event: &Event) -> bool {
        match self.renamer.event(cx, event) {
            RenameStep::Ignored => false,
            RenameStep::Handled => true,
            RenameStep::Done {
                item,
                name,
                pass_on,
            } => {
                if let Some(name) = name
                    && !name.trim().is_empty()
                    && name != self.nodes[item].title
                {
                    self.nodes[item].title.clone_from(&name);
                    cx.emit(ItemRenamed { item, name });
                }
                !pass_on
            }
        }
    }

    pub fn sidebar(mut self) -> Self {
        self.style = OutlineStyle::Sidebar;
        self.row_height = 28.0;
        self.selection.mode = SelectionMode::Single;
        for &root in &self.roots {
            self.nodes[root].expanded = true;
        }
        self.rebuild();
        self
    }

    pub fn selection_mode(mut self, mode: SelectionMode) -> Self {
        self.selection.mode = mode;
        self
    }

    /// Pin the heading of the section being scrolled through to the top of a sidebar (on
    /// by default; `floatsGroupRows`).
    pub fn floats_group_rows(mut self, floats: bool) -> Self {
        self.floats_group_rows = floats;
        self
    }

    /// The item whose heading is pinned to the top of a sidebar, if one is (after the
    /// last layout).
    pub fn floating_heading_item(&self) -> Option<usize> {
        self.floating_heading().map(|(pos, _)| self.visible[pos])
    }

    /// The heading pinned to the top and where it is drawn (widget `y`): the last
    /// heading above the top, pushed up by the next one as it arrives.
    fn floating_heading(&self) -> Option<(usize, f32)> {
        if !self.floats_group_rows {
            return None;
        }
        let v = self.viewport();
        pinned_group(
            &self.heading_at,
            self.scroller.offset.y,
            self.row_height,
            v.y,
        )
    }

    pub fn alternating(mut self, alternating: bool) -> Self {
        self.alternating = alternating;
        self
    }

    pub fn row_height(mut self, height: f32) -> Self {
        self.row_height = height;
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

    pub fn style_kind(&self) -> OutlineStyle {
        self.style
    }

    pub fn item_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn title(&self, item: usize) -> &str {
        &self.nodes[item].title
    }

    pub fn parent(&self, item: usize) -> Option<usize> {
        self.nodes[item].parent
    }

    pub fn children(&self, item: usize) -> &[usize] {
        &self.nodes[item].children
    }

    pub fn is_expanded(&self, item: usize) -> bool {
        self.nodes[item].expanded
    }

    /// Items listed (inside expanded parents), in order.
    pub fn visible_items(&self) -> &[usize] {
        &self.visible
    }

    pub fn selected_items(&self) -> Vec<usize> {
        self.selection.ids()
    }

    pub fn lead_item(&self) -> Option<usize> {
        self.selection.lead
    }

    pub fn scroll_offset(&self) -> f32 {
        self.scroller.offset.y
    }

    pub fn visible_rows(&self) -> Range<usize> {
        let first = (self.scroller.offset.y.max(0.0) / self.row_height) as usize;
        let last = ((self.scroller.offset.y + self.viewport().height) / self.row_height).ceil();
        first.min(self.visible.len())..(last as usize).min(self.visible.len())
    }

    /// Expand or collapse `item`. While filtered, the rows listed follow too.
    pub fn set_expanded(&mut self, cx: &mut UpdateCx, item: usize, expanded: bool) {
        if self.nodes[item].expanded != expanded || self.is_open(item) != expanded {
            self.nodes[item].expanded = expanded;
            if self.filtering() {
                self.filter_open[item] = expanded;
            }
            self.after_toggle(item);
            cx.request_paint();
        }
    }

    /// Select `item` (expanding its ancestors) and scroll it into view. An item the
    /// filter hides is not selected.
    pub fn select_item(&mut self, cx: &mut UpdateCx, item: Option<usize>) {
        if let Some(item) = item {
            if !self.listed(item) {
                return;
            }
            self.open_ancestors(item);
            self.rebuild();
        }
        self.selection.set(item);
        if let Some(pos) = self.selection.lead_position(&self.visible) {
            self.reveal(pos);
        }
        cx.request_paint();
    }

    pub fn set_badge(&mut self, cx: &mut UpdateCx, item: usize, badge: Option<String>) {
        self.nodes[item].badge = badge;
        cx.request_paint();
    }

    /// How far a sidebar shows its titles: 0 at the rail width, 1 when wide enough.
    /// Always 1 for a plain outline.
    pub fn title_fade(&self) -> f32 {
        if self.style != OutlineStyle::Sidebar {
            return 1.0;
        }
        ((self.size.width - OUTLINE_RAIL_WIDTH) / RAIL_FADE).clamp(0.0, 1.0)
    }

    fn open_ancestors(&mut self, item: usize) {
        let mut parent = self.nodes[item].parent;
        while let Some(p) = parent {
            self.nodes[p].expanded = true;
            if self.filtering() {
                self.filter_open[p] = true;
            }
            parent = self.nodes[p].parent;
        }
    }

    /// List only the items whose title holds each word of `query`, in any letter case.
    /// An empty query restores the expanded state from before. Items hidden are
    /// unselected; nothing is emitted.
    pub fn set_filter_text(&mut self, cx: &mut UpdateCx, query: &str) {
        if !self.query.set(query) {
            return;
        }
        let n = self.nodes.len();
        if self.filtering() {
            self.compute_filter();
        } else {
            self.shown.clear();
            self.filter_open.clear();
            if let Some(lead) = self.selection.lead {
                self.open_ancestors(lead);
            }
        }
        self.rebuild();
        let mut listed = vec![false; n];
        for &id in &self.visible {
            listed[id] = true;
        }
        self.selection.retain(|id| listed[id]);
        if self.renamer.item().is_some_and(|id| !listed[id]) {
            self.renamer.cancel();
        }
        match self.selection.lead_position(&self.visible) {
            Some(pos) => {
                self.reveal(pos);
            }
            None => self.scroller.offset.y = 0.0,
        }
        cx.request_paint();
    }

    /// Mark the letters [`OutlineView::set_filter_text`] matched in the titles (on by
    /// default).
    pub fn marks_filter_matches(mut self, marks: bool) -> Self {
        self.unmarked = !marks;
        self
    }

    pub fn filter_text(&self) -> &str {
        self.query.text()
    }

    pub fn is_filtered(&self) -> bool {
        self.filtering()
    }

    /// Items whose own title matches the filter (every item but the sidebar headings
    /// when there is no filter).
    pub fn matching_item_count(&self) -> usize {
        (0..self.nodes.len())
            .filter(|&id| !self.is_heading(id))
            .filter(|&id| self.query.matches(&self.nodes[id].title))
            .count()
    }

    pub fn reorderable(mut self, reorderable: bool) -> Self {
        self.reorderable = reorderable;
        self
    }

    pub fn items(&self) -> Vec<OutlineItem> {
        fn item(view: &OutlineView, id: usize) -> OutlineItem {
            let node = &view.nodes[id];
            OutlineItem {
                title: node.title.clone(),
                icon: node.icon.clone(),
                badge: node.badge.clone(),
                expanded: node.expanded,
                children: node.children.iter().map(|&c| item(view, c)).collect(),
            }
        }
        self.roots.iter().map(|&r| item(self, r)).collect()
    }

    /// Replace the hierarchy with `items` (the app's data changed). An item found at
    /// the same path of titles as before keeps whether it was expanded and selected;
    /// the others are as given (new sidebar sections start open). Nothing is emitted.
    pub fn set_items(&mut self, cx: &mut UpdateCx, items: Vec<OutlineItem>) {
        let mut before: std::collections::HashMap<String, usize> = (0..self.nodes.len())
            .rev()
            .map(|id| (self.path(id), id))
            .collect();
        fn walk(
            d: &mut Draft,
            mut titles: Vec<String>,
            view: &OutlineView,
            before: &mut std::collections::HashMap<String, usize>,
        ) {
            titles.push(d.node.title.clone());
            let path = view_state::join_path(titles.iter().map(String::as_str));
            if let Some(old) = before.remove(&path) {
                d.old = Some(old);
                if !view.nodes[old].children.is_empty() {
                    d.node.expanded = view.nodes[old].expanded;
                }
            } else if titles.len() == 1 && view.style == OutlineStyle::Sidebar {
                d.node.expanded = true;
            }
            for c in &mut d.children {
                walk(c, titles.clone(), view, before);
            }
        }
        let mut drafts: Vec<Draft> = items.into_iter().map(Draft::new).collect();
        for d in &mut drafts {
            walk(d, Vec::new(), self, &mut before);
        }
        self.adopt(drafts);
        cx.request_paint();
    }

    /// Put `item` (and the items under it) under `parent` (`None`: the top level; in a
    /// sidebar, a new section) before its child at `index` (at the end when `index` is
    /// past it). Returns the new item's id; ids after it move up. The parent is not
    /// opened, and nothing is selected.
    pub fn insert_item(
        &mut self,
        cx: &mut UpdateCx,
        parent: Option<usize>,
        index: usize,
        item: OutlineItem,
    ) -> usize {
        let mut drafts = self.drafts();
        let mut new = Draft::new(item);
        if parent.is_none() && self.style == OutlineStyle::Sidebar {
            new.node.expanded = true;
        }
        let siblings = draft_children(&mut drafts, parent).expect("parent is an item");
        let at = index.min(siblings.len());
        siblings.insert(at, new);
        let map = self.adopt(drafts);
        cx.request_paint();
        let siblings = match parent {
            Some(p) => &self.nodes[map[p].expect("the parent stays")].children,
            None => &self.roots,
        };
        siblings[at]
    }

    /// Remove `items` and the items under them. The others are numbered again; those
    /// selected stay selected. Nothing is emitted.
    pub fn remove_items(&mut self, cx: &mut UpdateCx, items: &[usize]) {
        let mut gone = vec![false; self.nodes.len()];
        for &id in items {
            if let Some(g) = gone.get_mut(id) {
                *g = true;
            }
        }
        if !gone.contains(&true) {
            return;
        }
        let mut drafts = self.drafts();
        take_drafts(&mut drafts, &|id| gone[id], &mut Vec::new());
        self.adopt(drafts);
        cx.request_paint();
    }

    /// Move `items` (with the items under them) under `parent` (`None`: the top level)
    /// from `index` on, counted among the children `parent` has without them; they keep
    /// their order. Items under another of `items` move with it. The parent is opened
    /// so they are seen, and they stay selected if they were. Returns whether anything
    /// moved: nothing does when `parent` is one of the items or under one, or (in a
    /// sidebar) when sections would move or items leave the sections. Nothing is
    /// emitted (a drag emits [`ItemsMoved`]).
    pub fn move_items(
        &mut self,
        cx: &mut UpdateCx,
        items: &[usize],
        parent: Option<usize>,
        index: usize,
    ) -> bool {
        let moved = self.move_tops(items, parent, index).is_some();
        if moved {
            cx.request_paint();
        }
        moved
    }

    fn move_tops(
        &mut self,
        items: &[usize],
        parent: Option<usize>,
        index: usize,
    ) -> Option<ItemsMoved> {
        let n = self.nodes.len();
        let mut moving = vec![false; n];
        for &id in items {
            if id < n && !self.is_heading(id) {
                moving[id] = true;
            }
        }
        let tops: Vec<usize> = (0..n)
            .filter(|&id| moving[id] && !self.under(id, &moving))
            .collect();
        if tops.is_empty()
            || parent.is_some_and(|p| p >= n || moving[p] || self.under(p, &moving))
            || (parent.is_none() && self.style == OutlineStyle::Sidebar)
        {
            return None;
        }
        let siblings = match parent {
            Some(p) => &self.nodes[p].children,
            None => &self.roots,
        };
        let mut rest: Vec<usize> = siblings.iter().copied().filter(|&c| !moving[c]).collect();
        let index = index.min(rest.len());
        rest.splice(index..index, tops.iter().copied());
        if rest == *siblings {
            return None;
        }
        let mut drafts = self.drafts();
        let mut taken = Vec::new();
        take_drafts(
            &mut drafts,
            &|id| moving[id] && !self.under(id, &moving),
            &mut taken,
        );
        let siblings = draft_children(&mut drafts, parent).expect("the parent stays");
        siblings.splice(index..index, taken);
        let map = self.adopt(drafts);
        if let Some(first) = map[tops[0]] {
            self.open_ancestors(first);
            self.rebuild();
        }
        Some(ItemsMoved {
            items: tops,
            parent,
            index,
        })
    }

    /// Whether an item above `id` (not `id` itself) is marked in `marks`.
    fn under(&self, id: usize, marks: &[bool]) -> bool {
        let mut at = self.nodes[id].parent;
        while let Some(p) = at {
            if marks[p] {
                return true;
            }
            at = self.nodes[p].parent;
        }
        false
    }

    fn drafts(&self) -> Vec<Draft> {
        fn draft(view: &OutlineView, id: usize) -> Draft {
            let node = &view.nodes[id];
            Draft {
                old: Some(id),
                node: Node {
                    children: Vec::new(),
                    ..node.clone()
                },
                children: node.children.iter().map(|&c| draft(view, c)).collect(),
            }
        }
        self.roots.iter().map(|&r| draft(self, r)).collect()
    }

    /// Take `drafts` as the hierarchy, numbering the items again depth first and
    /// carrying the selection and a rename over. Returns each old id's new one.
    fn adopt(&mut self, drafts: Vec<Draft>) -> Vec<Option<usize>> {
        fn walk(
            d: Draft,
            parent: Option<usize>,
            depth: usize,
            nodes: &mut Vec<Node>,
            map: &mut [Option<usize>],
        ) -> usize {
            let id = nodes.len();
            if let Some(old) = d.old {
                map[old] = Some(id);
            }
            nodes.push(Node {
                parent,
                depth,
                ..d.node
            });
            for c in d.children {
                let c = walk(c, Some(id), depth + 1, nodes, map);
                nodes[id].children.push(c);
            }
            id
        }
        let mut map = vec![None; self.nodes.len()];
        let mut nodes = Vec::new();
        self.roots = drafts
            .into_iter()
            .map(|d| walk(d, None, 0, &mut nodes, &mut map))
            .collect();
        self.nodes = nodes;
        self.selection.remap(self.nodes.len(), &map);
        self.renamer.remap(|id| map.get(id).copied().flatten());
        self.drag = None;
        self.autoscroll.stop();
        self.refresh();
        map
    }

    fn refresh(&mut self) {
        if self.filtering() {
            self.compute_filter();
        }
        self.rebuild();
        if self.filtering() {
            let mut listed = vec![false; self.nodes.len()];
            for &id in &self.visible {
                listed[id] = true;
            }
            self.selection.retain(|id| listed[id]);
            if self.renamer.item().is_some_and(|id| !listed[id]) {
                self.renamer.cancel();
            }
        }
    }

    fn filtering(&self) -> bool {
        !self.query.is_empty()
    }

    fn compute_filter(&mut self) {
        let n = self.nodes.len();
        self.shown = vec![false; n];
        self.filter_open = vec![false; n];
        // Children come after their parents in the depth-first ids.
        for id in (0..n).rev() {
            let under = self.nodes[id].children.iter().any(|&c| self.shown[c]);
            let matched = !self.is_heading(id) && self.query.matches(&self.nodes[id].title);
            self.shown[id] = matched || under;
            self.filter_open[id] = under;
        }
    }

    fn path(&self, id: usize) -> String {
        let mut titles = vec![self.nodes[id].title.as_str()];
        let mut at = id;
        while let Some(parent) = self.nodes[at].parent {
            titles.push(self.nodes[parent].title.as_str());
            at = parent;
        }
        titles.reverse();
        view_state::join_path(titles)
    }

    /// Whether `id` shows its children: its own expanded state, or while filtered,
    /// the filter's.
    fn is_open(&self, id: usize) -> bool {
        if self.filtering() {
            self.filter_open[id]
        } else {
            self.nodes[id].expanded
        }
    }

    fn set_open(&mut self, id: usize, open: bool) {
        if self.filtering() {
            self.filter_open[id] = open;
        } else {
            self.nodes[id].expanded = open;
        }
    }

    fn listed(&self, id: usize) -> bool {
        !self.filtering() || self.shown[id]
    }

    fn has_kids(&self, id: usize) -> bool {
        self.nodes[id].children.iter().any(|&c| self.listed(c))
    }

    fn is_heading(&self, id: usize) -> bool {
        self.style == OutlineStyle::Sidebar && self.nodes[id].depth == 0
    }

    fn rebuild(&mut self) {
        fn walk(view: &OutlineView, id: usize, out: &mut Vec<usize>) {
            if !view.listed(id) {
                return;
            }
            out.push(id);
            if view.is_open(id) {
                for &c in &view.nodes[id].children {
                    walk(view, c, out);
                }
            }
        }
        let mut visible = std::mem::take(&mut self.visible);
        visible.clear();
        for &root in &self.roots {
            walk(self, root, &mut visible);
        }
        self.visible = visible;
        self.heading_at.clear();
        if self.style == OutlineStyle::Sidebar {
            let nodes = &self.nodes;
            self.heading_at.extend(
                self.visible
                    .iter()
                    .enumerate()
                    .filter_map(|(pos, &id)| (nodes[id].depth == 0).then_some(pos)),
            );
        }
    }

    /// After `item` was expanded or collapsed: re-list the rows, and move a selection
    /// that was hidden to the collapsed item.
    fn after_toggle(&mut self, item: usize) {
        self.rebuild();
        if self.is_open(item) {
            return;
        }
        let mut shown = vec![false; self.nodes.len()];
        for &id in &self.visible {
            shown[id] = true;
        }
        let selected = self.selection.ids();
        if selected.iter().all(|&id| shown[id]) {
            return;
        }
        let mut keep: Vec<usize> = selected.into_iter().filter(|&id| shown[id]).collect();
        if !self.is_heading(item) {
            keep.push(item);
        }
        self.selection.set_many(keep);
    }

    fn set_branch(&mut self, item: usize, expanded: bool) {
        self.set_open(item, expanded);
        for c in self.nodes[item].children.clone() {
            self.set_branch(c, expanded);
        }
    }

    fn toggle(&mut self, cx: &mut EventCx, item: usize, expanded: bool, branch: bool) {
        if !self.has_kids(item) {
            return;
        }
        if branch {
            self.set_branch(item, expanded);
        } else if self.is_open(item) == expanded {
            return;
        } else {
            self.set_open(item, expanded);
        }
        let before = self.selection.ids();
        self.after_toggle(item);
        cx.emit(ItemExpanded { item, expanded });
        if self.selection.ids() != before {
            cx.emit(self.selection.message());
        }
        cx.request_paint();
    }

    fn viewport(&self) -> Rect {
        let inset = if self.style == OutlineStyle::Sidebar {
            0.0
        } else {
            1.0
        };
        Rect::new(
            inset,
            inset,
            (self.size.width - 2.0 * inset).max(0.0),
            (self.size.height - 2.0 * inset).max(0.0),
        )
    }

    fn sync(&mut self, size: Size) {
        self.size = size;
        let content = Size::new(
            self.viewport().width,
            self.visible.len() as f32 * self.row_height,
        );
        self.scroller.set_geometry(self.viewport(), content);
    }

    fn position_at(&self, p: Point) -> Option<usize> {
        let v = self.viewport();
        if !v.contains(p) {
            return None;
        }
        if let Some((pos, y)) = self.floating_heading()
            && p.y >= y
            && p.y < y + self.row_height
        {
            return Some(pos);
        }
        let pos = ((p.y - v.y + self.scroller.offset.y) / self.row_height).floor();
        (pos >= 0.0 && (pos as usize) < self.visible.len()).then_some(pos as usize)
    }

    fn disclosure_x(&self, id: usize) -> f32 {
        self.indent_x(self.nodes[id].depth)
    }

    fn indent_x(&self, depth: usize) -> f32 {
        let depth = if self.style == OutlineStyle::Sidebar {
            depth.saturating_sub(1)
        } else {
            depth
        };
        let base = if self.style == OutlineStyle::Sidebar {
            10.0
        } else {
            4.0
        };
        self.viewport().x + base + depth as f32 * OUTLINE_INDENT
    }

    fn reveal(&mut self, pos: usize) -> bool {
        let rect = reveal_rect(
            &self.heading_at,
            self.floats_group_rows,
            pos,
            self.row_height,
            0.0,
            1.0,
        );
        self.scroller.reveal(rect)
    }

    fn changed(&mut self, cx: &mut EventCx) {
        cx.emit(self.selection.message());
        cx.request_paint();
    }

    /// Where the selected items would go if dropped at `p` (widget coordinates):
    /// `None` where they cannot (into themselves, or out of a sidebar's sections).
    fn drop_at(&self, p: Point) -> Option<DropAt> {
        let len = self.visible.len();
        if len == 0 {
            return None;
        }
        let v = self.viewport();
        let rh = self.row_height;
        let y = (p.y - v.y + self.scroller.offset.y).max(0.0) / rh;
        let sidebar = self.style == OutlineStyle::Sidebar;
        // The heading pinned to the top covers the rows under it.
        let pinned = self
            .floating_heading()
            .filter(|&(_, top)| p.y >= top && p.y < top + rh);
        let target = if let Some((pos, _)) = pinned {
            Some(DropAt::Into {
                item: self.visible[pos],
                pos,
            })
        } else if y >= len as f32 {
            match self.roots.last() {
                Some(&last) if sidebar => self.is_open(last).then(|| DropAt::Between {
                    parent: Some(last),
                    index: self.nodes[last].children.len(),
                    line: len,
                    depth: 1,
                }),
                _ => Some(DropAt::Between {
                    parent: None,
                    index: self.roots.len(),
                    line: len,
                    depth: 0,
                }),
            }
        } else {
            let pos = y.floor() as usize;
            let part = y - pos as f32;
            let id = self.visible[pos];
            let node = &self.nodes[id];
            let index = self.siblings(node.parent).iter().position(|&c| c == id);
            let index = index.unwrap_or(0);
            if self.is_heading(id) {
                Some(DropAt::Into { item: id, pos })
            } else if part < DROP_EDGE {
                Some(DropAt::Between {
                    parent: node.parent,
                    index,
                    line: pos,
                    depth: node.depth,
                })
            } else if part >= 1.0 - DROP_EDGE {
                if self.is_open(id) && self.has_kids(id) {
                    Some(DropAt::Between {
                        parent: Some(id),
                        index: 0,
                        line: pos + 1,
                        depth: node.depth + 1,
                    })
                } else {
                    Some(DropAt::Between {
                        parent: node.parent,
                        index: index + 1,
                        line: pos + 1,
                        depth: node.depth,
                    })
                }
            } else {
                Some(DropAt::Into { item: id, pos })
            }
        }?;
        let parent = match target {
            DropAt::Into { item, .. } => Some(item),
            DropAt::Between { parent, .. } => parent,
        };
        let inside = parent.is_some_and(|p| {
            let mut at = Some(p);
            while let Some(a) = at {
                if self.selection.is_selected(a) {
                    return true;
                }
                at = self.nodes[a].parent;
            }
            false
        });
        (!inside && (parent.is_some() || !sidebar)).then_some(target)
    }

    /// The children of `parent` (the top level for `None`).
    fn siblings(&self, parent: Option<usize>) -> &[usize] {
        match parent {
            Some(p) => &self.nodes[p].children,
            None => &self.roots,
        }
    }

    fn drop_selected(&mut self, cx: &mut EventCx, target: DropAt) {
        let selected = self.selection.ids();
        let (parent, index) = match target {
            DropAt::Into { item, .. } if self.is_heading(item) => (Some(item), 0),
            DropAt::Into { item, .. } => (Some(item), usize::MAX),
            DropAt::Between { parent, index, .. } => {
                // Counted without the items that move.
                let before = self.siblings(parent)[..index.min(self.siblings(parent).len())]
                    .iter()
                    .filter(|&&c| self.selection.is_selected(c))
                    .count();
                (parent, index - before)
            }
        };
        if let Some(moved) = self.move_tops(&selected, parent, index) {
            cx.emit(moved);
            self.changed(cx);
            if let Some(pos) = self.selection.lead_position(&self.visible) {
                self.reveal(pos);
            }
        }
        cx.request_paint();
    }

    fn paint_drop(&self, cx: &mut PaintCx, target: DropAt) {
        let v = self.viewport();
        let rh = self.row_height;
        let off = self.scroller.offset.y;
        let accent = cx.color(Role::Accent);
        match target {
            DropAt::Into { pos, .. } => {
                let top = match self.floating_heading() {
                    Some((pinned, top)) if pinned == pos => top,
                    _ => v.y + pos as f32 * rh - off,
                };
                let r = Rect::new(v.x + 2.0, top + 1.0, v.width - 4.0, rh - 2.0);
                let radius = cx.corner(Corner::Inner, 5.0, r.height);
                cx.rounded_rect(
                    r,
                    radius,
                    accent.with_alpha(0.12),
                    Border {
                        width: 2.0,
                        color: accent,
                    },
                );
            }
            DropAt::Between { line, depth, .. } => {
                let y = v.y + line as f32 * rh - off;
                let x = self.indent_x(depth) + DISCLOSURE;
                let surface = cx.color(Role::Surface);
                cx.fill_rect(Rect::new(x, y - 1.0, (v.right() - x).max(0.0), 2.0), accent);
                cx.rounded_rect(
                    Rect::new(x - 6.0, y - 3.0, 6.0, 6.0),
                    3.0,
                    surface,
                    Border {
                        width: 1.5,
                        color: accent,
                    },
                );
            }
        }
    }

    fn move_to(&mut self, cx: &mut EventCx, mut pos: usize, down: bool, extend: bool) {
        while self.is_heading(self.visible[pos]) {
            match (down, pos) {
                (true, p) if p + 1 < self.visible.len() => pos += 1,
                (false, p) if p > 0 => pos -= 1,
                _ => return,
            }
        }
        if self.selection.move_to(&self.visible, pos, extend) {
            self.changed(cx);
        }
        if self.reveal(pos) {
            self.scroller.flash(cx);
            cx.request_paint();
        }
    }

    fn key_move(&mut self, cx: &mut EventCx, down: bool) {
        let selected = self.selection.ids();
        let mut marks = vec![false; self.nodes.len()];
        for &id in &selected {
            marks[id] = true;
        }
        let tops: Vec<usize> = selected
            .iter()
            .copied()
            .filter(|&id| !self.under(id, &marks))
            .collect();
        let Some(&first) = tops.first() else { return };
        let parent = self.nodes[first].parent;
        if tops.iter().any(|&id| self.nodes[id].parent != parent) {
            return;
        }
        // Past the next listed sibling: with a filter set, the siblings it hides keep
        // their places, so the move is one the rows show.
        let siblings = self.siblings(parent);
        let at = |id: usize| siblings.iter().position(|&c| c == id).unwrap_or(0);
        let rest: Vec<usize> = siblings.iter().copied().filter(|&c| !marks[c]).collect();
        let rank = |id: usize| rest.iter().position(|&c| c == id).unwrap_or(0);
        // Places counted among the siblings left without the moving items.
        let index = if down {
            let last = at(tops[tops.len() - 1]);
            match siblings[last + 1..].iter().find(|&&c| self.listed(c)) {
                Some(&next) => rank(next) + 1,
                None => return,
            }
        } else {
            match siblings[..at(first)]
                .iter()
                .rev()
                .find(|&&c| self.listed(c))
            {
                Some(&prev) => rank(prev),
                None => return,
            }
        };
        if let Some(moved) = self.move_tops(&tops, parent, index) {
            cx.emit(moved);
            self.changed(cx);
            if let Some(pos) = self.selection.lead_position(&self.visible) {
                self.reveal(pos);
            }
        }
    }

    fn key(&mut self, cx: &mut EventCx, key: Key, modifiers: Modifiers) -> bool {
        let n = self.visible.len();
        let lead = self.selection.lead;
        match key {
            Key::ArrowUp | Key::ArrowDown
                if self.reorderable && modifiers.command && modifiers.alt =>
            {
                self.key_move(cx, key == Key::ArrowDown);
            }
            Key::Char('a') if modifiers.command => {
                if self.selection.select_all(&self.visible) {
                    self.changed(cx);
                }
            }
            Key::Enter => {
                if let Some(id) = lead {
                    if self.renamable && !self.is_heading(id) {
                        self.start_rename(id);
                        cx.request_paint();
                    } else {
                        cx.emit(RowActivated(id));
                    }
                }
            }
            Key::ArrowRight => {
                let Some(id) = lead else { return true };
                if !self.has_kids(id) {
                    return true;
                }
                if !self.is_open(id) || modifiers.alt {
                    self.toggle(cx, id, true, modifiers.alt);
                } else if let Some(pos) = self.selection.lead_position(&self.visible) {
                    self.move_to(cx, pos + 1, true, false);
                }
            }
            Key::ArrowLeft => {
                let Some(id) = lead else { return true };
                if self.is_open(id) && self.has_kids(id) {
                    self.toggle(cx, id, false, modifiers.alt);
                } else if let Some(parent) = self.nodes[id].parent
                    && !self.is_heading(parent)
                    && let Some(pos) = self.visible.iter().position(|&v| v == parent)
                {
                    self.move_to(cx, pos, false, false);
                }
            }
            Key::ArrowUp | Key::ArrowDown | Key::Home | Key::End | Key::PageUp | Key::PageDown => {
                let page = (self.viewport().height / self.row_height) as usize;
                let current = self.selection.lead_position(&self.visible);
                if let Some(pos) = step(key, current, n, page) {
                    let down = matches!(key, Key::ArrowDown | Key::PageDown | Key::Home);
                    self.move_to(cx, pos, down, modifiers.shift);
                }
            }
            _ => return false,
        }
        true
    }
    /// Row `pos` with its top at widget `y`.
    fn paint_row(&mut self, cx: &mut PaintCx, pos: usize, y: f32, look: &RowLook) {
        let RowLook {
            sidebar,
            surface,
            selected_fill,
            selected_ink,
            ink,
            muted,
            stripe,
        } = *look;
        let v = self.viewport();
        let rh = self.row_height;
        let fade = self.title_fade();
        let faded = |c: noa_gpui_scene::Color, t: f32| c.with_alpha(c.a * t);
        let rail_x = v.x + (OUTLINE_RAIL_WIDTH - ICON) / 2.0;
        let row_style = TextStyle::new(13.0);
        let heading_style = TextStyle::new(11.0).weight(700);
        let badge_style = TextStyle::new(11.0).weight(600);
        let id = self.visible[pos];
        let heading = self.is_heading(id);
        let selected = !heading && self.selection.is_selected(id);
        let row = Rect::new(v.x, y, v.width, rh);
        if selected {
            if sidebar {
                let r = Rect::new(v.x + 8.0, y + 1.0, v.width - 16.0, rh - 2.0);
                let radius = cx.corner(Corner::Inner, 5.0, r.height);
                cx.rounded_rect(r, radius, selected_fill, Border::default());
            } else {
                cx.fill_rect(row, selected_fill);
            }
        } else if self.alternating && pos % 2 == 1 {
            cx.fill_rect(row, stripe);
        }
        let (open, kids) = (self.is_open(id), self.has_kids(id));
        let node = &self.nodes[id];
        let color = if selected {
            selected_ink
        } else if heading {
            muted
        } else {
            ink
        };
        let mut x = self.disclosure_x(id);
        let mut icon_rect = None;
        if heading {
            x = v.x + 18.0;
            if fade < 1.0 && pos > 0 {
                let sep = cx.color(Role::Separator);
                let line = Rect::new(rail_x - 2.0, y + rh / 2.0, ICON + 4.0, 1.0);
                cx.fill_rect(line, faded(sep, 1.0 - fade));
            }
        } else {
            if kids && fade > 0.0 {
                let r = Rect::new(x + 5.0, y + rh / 2.0 - 4.0, 5.0, 8.0);
                let (r, dir) = if open {
                    (
                        Rect::new(x + 4.0, y + rh / 2.0 - 2.5, 8.0, 5.0),
                        Direction::Down,
                    )
                } else {
                    (r, Direction::Right)
                };
                let tint = if selected { selected_ink } else { muted };
                cx.chevron(r, dir, 1.5, faded(tint, fade));
            }
            x += DISCLOSURE;
            if let Some(icon) = node.icon.clone() {
                let ink = if selected {
                    selected_ink
                } else {
                    cx.color(Role::Accent)
                };
                let ix = rail_x + (x - rail_x) * fade;
                let rect = Rect::new(ix, y + (rh - ICON) / 2.0, ICON, ICON);
                icon.paint(cx, rect, ink);
                icon_rect = Some(rect);
                x = ix + ICON + PAD;
            }
        }
        let style = if heading { heading_style } else { row_style };
        let title = node.title.clone();
        let badge = node.badge.clone();
        let (has_children, expanded) = (kids, open);
        let mut right = v.right() - if sidebar { 16.0 } else { PAD };
        if let Some(badge) = badge.filter(|_| !heading) {
            if fade < 1.0
                && let Some(icon) = icon_rect
            {
                let dot = Rect::new(icon.right() - 3.0, icon.y - 3.0, 7.0, 7.0);
                let accent = cx.color(Role::Accent);
                cx.rounded_rect(dot, 3.5, faded(accent, 1.0 - fade), Border::default());
            }
            let (layout, size) = self.badges.get(cx.text_store(), id, &badge, badge_style);
            if let Some(layout) = layout.filter(|_| fade > 0.0) {
                let w = (size.width + 12.0).max(20.0);
                let pill = Rect::new(right - w, y + (rh - 16.0) / 2.0, w, 16.0);
                let fill = if selected {
                    selected_ink.with_alpha(0.25)
                } else {
                    mix(surface, ink, 0.12)
                };
                let ink = if selected { selected_ink } else { muted };
                let (fill, ink) = (faded(fill, fade), faded(ink, fade));
                super::badge::pill(cx, pill, 8.0, fill, Some((layout, size, ink)));
                right = pill.x - PAD;
            }
        }
        if heading && has_children {
            let dir = if expanded {
                Direction::Down
            } else {
                Direction::Right
            };
            let r = if expanded {
                Rect::new(right - 8.0, y + rh / 2.0 - 2.0, 8.0, 4.0)
            } else {
                Rect::new(right - 6.0, y + rh / 2.0 - 4.0, 4.0, 8.0)
            };
            cx.chevron(r, dir, 1.5, faded(muted, fade));
            right -= 14.0;
        }
        if fade > 0.0
            && let Some(editor) = self.renamer.editor_in(id, v)
        {
            let size = editor.shape(cx.text_store());
            editor.at = Point::new(x, y + (rh - size.height) / 2.0);
            editor.max_width = (right - x).max(0.0);
            editor.paint(cx);
            return;
        }
        let (layout, size) = self.titles.get(cx.text_store(), id, &title, style);
        if let Some(layout) = layout.filter(|_| fade > 0.0) {
            let color = faded(color, fade);
            let room = right - x;
            let ty = if heading {
                y + rh - size.height - 4.0
            } else {
                y + (rh - size.height) / 2.0
            };
            let clip = size.width > room;
            if clip {
                cx.push_clip(Rect::new(x, y, room.max(0.0), rh));
            }
            if !heading && !self.unmarked && self.filtering() {
                let mark = if selected {
                    selected_ink.with_alpha(0.3)
                } else {
                    cx.theme().accent.with_alpha(0.28)
                };
                for range in self.query.ranges(&title) {
                    let spans = cx
                        .text_store()
                        .selection_rects(layout, range.start, range.end);
                    for r in spans {
                        let r = Rect::new(x + r.x, ty + r.y + 1.0, r.width, r.height - 2.0);
                        cx.rounded_rect(r, 3.0, faded(mark, fade), Border::default());
                    }
                }
            }
            cx.text(Rect::new(x, ty, size.width, size.height), layout, color);
            if clip {
                cx.pop_clip();
            }
        }
    }
}

/// Colors resolved once per paint for [`OutlineView::paint_row`].
#[derive(Debug, Clone, Copy)]
struct RowLook {
    sidebar: bool,
    surface: noa_gpui_scene::Color,
    selected_fill: noa_gpui_scene::Color,
    selected_ink: noa_gpui_scene::Color,
    ink: noa_gpui_scene::Color,
    muted: noa_gpui_scene::Color,
    stripe: noa_gpui_scene::Color,
}

impl Widget for OutlineView {
    /// Which items with children are expanded: one line per item, `+` (expanded) or
    /// `-` (collapsed) and the item's path of titles (`Places/Projects`). Items are found
    /// again by their paths, so items added since keep their own state.
    fn save_state(&self) -> Option<String> {
        let lines: Vec<String> = (0..self.nodes.len())
            .filter(|&id| !self.nodes[id].children.is_empty())
            .map(|id| {
                let sign = if self.nodes[id].expanded { '+' } else { '-' };
                format!("{sign}{}", self.path(id))
            })
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    fn restore_state(&mut self, cx: &mut UpdateCx, state: &str) {
        let paths: std::collections::HashMap<String, usize> = (0..self.nodes.len())
            .rev()
            .map(|id| (self.path(id), id))
            .collect();
        let mut collapsed = Vec::new();
        for line in state.lines() {
            let (expanded, path) = match line.split_at_checked(1) {
                Some(("+", path)) => (true, path),
                Some(("-", path)) => (false, path),
                _ => continue,
            };
            let Some(&id) = paths.get(path) else { continue };
            if self.nodes[id].children.is_empty() || self.nodes[id].expanded == expanded {
                continue;
            }
            self.nodes[id].expanded = expanded;
            if !expanded {
                collapsed.push(id);
            }
        }
        if self.filtering() {
            // The filter decides what is open until it is cleared.
            return;
        }
        self.rebuild();
        for id in collapsed {
            self.after_toggle(id);
        }
        cx.request_paint();
    }

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
        self.sync(cx.size());
        self.renamer.begin_paint(cx.text_store());
        let sidebar = self.style == OutlineStyle::Sidebar;
        let bounds = cx.bounds();
        let surface = cx.color(Role::Surface);
        if sidebar {
            let fill = cx.color(Role::SurfaceAlt);
            cx.fill_rect(bounds, fill);
        } else {
            let outline = cx.color(Role::Outline);
            cx.rounded_rect(
                bounds,
                0.0,
                surface,
                Border {
                    width: 1.0,
                    color: outline,
                },
            );
        }
        let v = self.viewport();
        let rh = self.row_height;
        let off = self.scroller.offset.y;
        let focused = cx.is_focused();
        let (selected_fill, selected_ink) = selection_fill(cx, focused);
        let stripe = stripe(cx);
        let look = RowLook {
            sidebar,
            surface,
            selected_fill,
            selected_ink: cx.color(selected_ink),
            ink: cx.color(Role::Text),
            muted: cx.color(Role::Muted),
            stripe,
        };

        cx.push_clip(v);
        if self.alternating {
            let first = self.visible.len().max((off.max(0.0) / rh) as usize);
            let last = ((off + v.height) / rh).ceil().max(0.0) as usize;
            for pos in (first..last).filter(|p| p % 2 == 1) {
                let y = v.y + pos as f32 * rh - off;
                cx.fill_rect(Rect::new(v.x, y, v.width, rh), stripe);
            }
        }
        let range = self.visible_rows();
        self.titles.begin();
        self.badges.begin();
        for pos in range {
            let y = v.y + pos as f32 * rh - off;
            self.paint_row(cx, pos, y, &look);
        }
        if let Some((pos, y)) = self.floating_heading() {
            let fill = cx.color(Role::SurfaceAlt);
            cx.fill_rect(Rect::new(v.x, y, v.width, rh), fill);
            self.paint_row(cx, pos, y, &look);
        }
        self.titles.end(cx.text_store());
        self.badges.end(cx.text_store());
        if let Some(RowDrag {
            dragging: true,
            drop: Some(target),
            ..
        }) = self.drag
        {
            self.paint_drop(cx, target);
        }
        cx.pop_clip();
        self.scroller.paint(cx);
        if focused && !sidebar {
            focus_ring(cx, bounds, 0.0);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.sync(cx.size());
        if self.rename_event(cx, event) {
            return;
        }
        if let Event::AnimationFrame { now } = event
            && let Some(p) = self
                .autoscroll
                .step(cx, self.viewport(), &mut self.scroller, *now)
        {
            // The rows moved under the pointer: so does the drop place.
            let drop = self.drop_at(p);
            if let Some(drag) = &mut self.drag {
                drag.drop = drop;
            }
        }
        if self.scroller.event(cx, event) {
            return;
        }
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                cx.set_handled();
                let Some(pos) = self.position_at(p) else {
                    if self.selection.count() > 0 && self.viewport().contains(p) {
                        self.selection.set(None);
                        self.changed(cx);
                    }
                    return;
                };
                let id = self.visible[pos];
                let modifiers = cx.modifiers();
                if self.is_heading(id) {
                    let expanded = !self.is_open(id);
                    self.toggle(cx, id, expanded, false);
                    return;
                }
                let dx = self.disclosure_x(id);
                let rail = self.title_fade() < 0.5;
                if !rail && self.has_kids(id) && p.x >= dx && p.x < dx + DISCLOSURE {
                    let expanded = !self.is_open(id);
                    self.toggle(cx, id, expanded, modifiers.alt);
                    return;
                }
                // Pressing an already selected row keeps the selection, so the rows can
                // be dragged together; the click lands on release.
                let keep = self.reorderable
                    && self.selection.is_selected(id)
                    && !modifiers.command
                    && !modifiers.shift;
                if !keep && self.selection.click(&self.visible, pos, modifiers) {
                    self.changed(cx);
                }
                if self.reorderable && self.selection.is_selected(id) {
                    self.drag = Some(RowDrag {
                        at: p,
                        click: keep,
                        dragging: false,
                        drop: None,
                    });
                    cx.capture_pointer();
                }
            }
            Event::PointerMove { pos } => {
                let p = cx.local(*pos);
                let Some(drag) = self.drag else { return };
                let moved = (p.y - drag.at.y).abs().max((p.x - drag.at.x).abs()) >= DRAG_START;
                if moved || drag.dragging {
                    let drop = self.drop_at(p);
                    if !drag.dragging || drop != drag.drop {
                        self.drag = Some(RowDrag {
                            dragging: true,
                            drop,
                            ..drag
                        });
                        cx.request_paint();
                    }
                    let v = self.viewport();
                    self.autoscroll.track(cx, v, &self.scroller, p);
                }
                cx.set_handled();
            }
            Event::PointerUp { pos, .. } => {
                let p = cx.local(*pos);
                self.autoscroll.stop();
                let Some(drag) = self.drag.take() else { return };
                cx.set_handled();
                if drag.dragging {
                    // Where the pointer is now: the rows may have scrolled (by the
                    // wheel) since it last moved.
                    if let Some(target) = self.drop_at(p) {
                        self.drop_selected(cx, target);
                    }
                    cx.request_paint();
                } else if drag.click
                    && let Some(pos) = self.position_at(p)
                    && self.selection.click(&self.visible, pos, cx.modifiers())
                {
                    self.changed(cx);
                }
            }
            Event::KeyDown {
                key: Key::Escape, ..
            } if self.drag.is_some_and(|d| d.dragging) => {
                self.drag = None;
                self.autoscroll.stop();
                cx.set_handled();
                cx.request_paint();
            }
            Event::KeyDown { key, modifiers } if cx.is_focused() => {
                if self.key(cx, *key, *modifiers) {
                    cx.set_handled();
                }
            }
            Event::ActivateItem(i) => {
                let pos = self.visible_rows().start + i;
                if let Some(&id) = self.visible.get(pos)
                    && !self.is_heading(id)
                {
                    self.selection.set(Some(id));
                    self.changed(cx);
                }
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn text_input_area(&self) -> Option<Rect> {
        self.renamer.input_area()
    }

    /// In a rail, the title of the row under the pointer (only its icon is drawn).
    fn tooltip(&self, local: Point) -> Option<String> {
        if self.title_fade() >= 0.5 {
            return None;
        }
        let id = self.visible[self.position_at(local)?];
        (!self.is_heading(id)).then(|| self.nodes[id].title.clone())
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Tree)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        self.scroller.accessibility(node);
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        let v = self.viewport();
        self.visible_rows()
            .map(|pos| {
                let id = self.visible[pos];
                let n = &self.nodes[id];
                let mut node = accesskit::Node::new(accesskit::Role::TreeItem);
                node.set_label(n.title.clone());
                node.set_level(n.depth + 1);
                if self.has_kids(id) {
                    node.set_expanded(self.is_open(id));
                }
                node.set_selected(self.selection.is_selected(id));
                let y = v.y + pos as f32 * self.row_height - self.scroller.offset.y;
                node.set_bounds(accesskit::Rect {
                    x0: f64::from(v.x),
                    y0: f64::from(y),
                    x1: f64::from(v.right()),
                    y1: f64::from(y + self.row_height),
                });
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect()
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.titles.release(text);
        self.badges.release(text);
        self.renamer.removed(text);
    }
}
