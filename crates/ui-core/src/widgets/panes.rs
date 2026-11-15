use noa_gpui_layout::Style;
use noa_gpui_layout::style::{
    Dimension, Display, FlexDirection, GridTemplateComponent, Rect as LayoutRect,
    Size as LayoutSize, auto, fr, length, minmax, percent,
};
use noa_gpui_scene::{Border, Point, Rect, Size};
use noa_gpui_text::{TextStore, TextStyle};

use super::{
    CachedText, Container, SelectionChanged, Separator, Toggled, focus_ring, pressed_tint,
};
use crate::animation::{Easing, Transition};
use crate::event::{Cursor, Event, Key, Message, PointerButton};
use crate::theme::{Corner, Role};
use crate::tree::{UiTree, WidgetId};
use crate::view_state;
use crate::widget::{EventCx, PaintCx, UpdateCx, Widget};

/// Seconds within which a second press counts as a double click.
const DOUBLE_CLICK: f64 = 0.4;
/// Seconds a rail pane takes to narrow or widen.
pub const RAIL_DURATION: f64 = 0.22;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitPane {
    /// The pane's length along the split, or `None` for the pane that takes the rest.
    size: Option<f32>,
    min: f32,
    collapsible: bool,
    collapsed: bool,
    rail: Option<f32>,
    /// How open a rail pane is: 1 at its size, 0 at its rail; moves over
    /// [`RAIL_DURATION`].
    open: Transition,
    shown: f32,
}

impl SplitPane {
    pub fn fixed(size: f32) -> Self {
        Self {
            size: Some(size),
            min: 0.0,
            collapsible: false,
            collapsed: false,
            rail: None,
            open: Transition::new(1.0),
            shown: 1.0,
        }
    }

    /// The pane that takes the space the others leave (one per split view).
    pub fn flexible() -> Self {
        Self {
            size: None,
            ..Self::fixed(0.0)
        }
    }

    pub fn min(mut self, min: f32) -> Self {
        self.min = min;
        self
    }

    pub fn collapsible(mut self, collapsible: bool) -> Self {
        self.collapsible = collapsible;
        self
    }

    /// Collapse to a rail `width` long instead of hiding (a fixed pane only). Implies
    /// [`SplitPane::collapsible`].
    pub fn rail(mut self, width: f32) -> Self {
        self.rail = Some(width);
        self.collapsible = true;
        self
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        let open = if collapsed { 0.0 } else { 1.0 };
        self.open.set(open);
        self.shown = open;
        self
    }

    /// The pane's length now (`None` for the flexible pane): between its rail and its
    /// size while a rail pane moves.
    fn length(&self) -> Option<f32> {
        let size = self.size?;
        Some(match self.rail {
            Some(rail) => rail + (size - rail) * self.shown,
            None => size,
        })
    }

    fn is_shown(&self) -> bool {
        !self.collapsed || self.rail.is_some()
    }
}

/// Emitted by a [`SplitView`] after a divider is dragged or a pane collapses or
/// expands, with each pane's length (0 when collapsed, its rail width when it
/// collapses to a rail).
#[derive(Debug, Clone, PartialEq)]
pub struct PanesChanged(pub Vec<f32>);

/// The thickness of a divider (the visible line is 1 px in its middle).
pub const DIVIDER: f32 = 6.0;

/// Panes side by side or stacked, with draggable dividers (`NSSplitView`). Insert one
/// child per pane, in order; exactly one pane should be [`SplitPane::flexible`]. Panes
/// should clip their content. The split view fills its parent. Emits [`PanesChanged`].
#[derive(Debug)]
pub struct SplitView {
    side_by_side: bool,
    panes: Vec<SplitPane>,
    /// The divider being dragged: visible panes before and after it, and where in the
    /// divider it was grabbed.
    dragging: Option<(usize, usize, f32)>,
    last_press: Option<(f64, usize)>,
}

impl SplitView {
    /// Panes side by side, separated by vertical dividers.
    pub fn horizontal() -> Self {
        Self {
            side_by_side: true,
            panes: Vec::new(),
            dragging: None,
            last_press: None,
        }
    }

    /// Panes stacked top to bottom, separated by horizontal dividers.
    pub fn vertical() -> Self {
        Self {
            side_by_side: false,
            ..Self::horizontal()
        }
    }

    pub fn pane(mut self, pane: SplitPane) -> Self {
        self.panes.push(pane);
        self
    }

    pub fn is_collapsed(&self, index: usize) -> bool {
        self.panes.get(index).is_some_and(|p| p.collapsed)
    }

    /// The fixed panes' lengths (`None` for the flexible pane).
    pub fn sizes(&self) -> Vec<Option<f32>> {
        self.panes.iter().map(|p| p.size).collect()
    }

    /// Collapse or restore pane `index`; a rail pane narrows or widens with an
    /// animation (call it through [`crate::UiTree::update`]).
    pub fn set_collapsed(&mut self, cx: &mut UpdateCx, index: usize, collapsed: bool) {
        if let Some(pane) = self.panes.get_mut(index) {
            pane.collapsed = collapsed;
            if self.start_rails(cx.now(), cx.motion(RAIL_DURATION)) {
                cx.request_animation_frame();
            }
            cx.request_layout();
            cx.request_paint();
        }
    }

    pub fn is_animating(&self) -> bool {
        self.panes
            .iter()
            .any(|p| p.rail.is_some() && p.open.target() != p.shown)
    }

    /// Point each rail pane's animation at its collapsed state. Returns whether one
    /// has to move. `duration` is [`RAIL_DURATION`], or 0 to reduce motion.
    fn start_rails(&mut self, now: f64, duration: f64) -> bool {
        for pane in self.panes.iter_mut().filter(|p| p.rail.is_some()) {
            let target = if pane.collapsed { 0.0 } else { 1.0 };
            pane.open
                .animate_to(target, now, duration, Easing::EaseInOut);
        }
        self.is_animating()
    }

    fn visible(&self) -> Vec<usize> {
        (0..self.panes.len())
            .filter(|&i| self.panes[i].is_shown())
            .collect()
    }

    /// Each pane's current length for a split of `total` pixels (0 when collapsed).
    fn lengths(&self, total: f32) -> Vec<f32> {
        let visible = self.visible();
        let gaps = DIVIDER * visible.len().saturating_sub(1) as f32;
        let fixed: f32 = visible.iter().filter_map(|&i| self.panes[i].length()).sum();
        self.panes
            .iter()
            .map(|p| match (p.is_shown(), p.length()) {
                (false, _) => 0.0,
                (true, Some(length)) => length,
                (true, None) => (total - gaps - fixed).max(p.min),
            })
            .collect()
    }

    /// Start of each visible pane along the split, and the dividers after all but the
    /// last: `(pane before, pane after, divider start)`.
    fn dividers(&self, total: f32) -> Vec<(usize, usize, f32)> {
        let lengths = self.lengths(total);
        let visible = self.visible();
        let mut at = 0.0;
        let mut out = Vec::new();
        for pair in visible.windows(2) {
            at += lengths[pair[0]];
            out.push((pair[0], pair[1], at));
            at += DIVIDER;
        }
        out
    }

    fn main_axis(&self, p: Point) -> f32 {
        if self.side_by_side { p.x } else { p.y }
    }

    fn total(&self, cx: &EventCx) -> f32 {
        let size = cx.size();
        if self.side_by_side {
            size.width
        } else {
            size.height
        }
    }

    fn report(&self, cx: &mut EventCx, total: f32) {
        cx.emit(PanesChanged(self.lengths(total)));
    }

    fn drag_to(&mut self, a: usize, b: usize, position: f32, total: f32) {
        let lengths = self.lengths(total);
        let start = self
            .dividers(total)
            .iter()
            .find(|d| d.0 == a)
            .map_or(0.0, |d| d.2 - lengths[a]);
        let both = lengths[a] + lengths[b];
        let (min_a, min_b) = (self.panes[a].min, self.panes[b].min);
        let new_a = (position - start).clamp(min_a, (both - min_b).max(min_a));
        if self.panes[a].size.is_some() {
            self.panes[a].size = Some(new_a);
        }
        if self.panes[b].size.is_some() {
            self.panes[b].size = Some(both - new_a);
        }
    }

    /// Double-click on the divider between `a` and `b`: restore a collapsed pane next
    /// to it (between them, or beyond them at an end of the split), or collapse a
    /// collapsible neighbor (a fixed one first).
    fn toggle_collapse(&mut self, a: usize, b: usize) -> bool {
        let visible = self.visible();
        let first = visible.first() == Some(&a);
        let last = visible.last() == Some(&b);
        let hidden = (a + 1..b)
            .chain(if first { 0..a } else { 0..0 })
            .chain(if last { b + 1..self.panes.len() } else { 0..0 })
            .chain([a, b])
            .find(|&i| self.panes[i].collapsed);
        if let Some(hidden) = hidden {
            self.panes[hidden].collapsed = false;
            return true;
        }
        let candidate = [a, b]
            .into_iter()
            .filter(|&i| self.panes[i].collapsible)
            .max_by_key(|&i| self.panes[i].size.is_some());
        match candidate {
            Some(i) => {
                self.panes[i].collapsed = true;
                true
            }
            None => false,
        }
    }
}

impl Widget for SplitView {
    /// The fixed panes' lengths (`-` for the flexible pane) and the collapsed panes:
    /// `sizes:240,-,300;collapsed:2`. Restored only into a split view with the same
    /// number of panes; a restored rail pane jumps to its state without animating.
    fn save_state(&self) -> Option<String> {
        let mut out = String::from("sizes:");
        for (i, pane) in self.panes.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            match pane.size {
                Some(size) => view_state::push_number(&mut out, size),
                None => out.push('-'),
            }
        }
        out.push_str(";collapsed:");
        let collapsed: Vec<String> = (0..self.panes.len())
            .filter(|&i| self.panes[i].collapsed)
            .map(|i| i.to_string())
            .collect();
        out.push_str(&collapsed.join(","));
        Some(out)
    }

    fn restore_state(&mut self, cx: &mut UpdateCx, state: &str) {
        let Some(sizes) = view_state::field(state, "sizes") else {
            return;
        };
        let sizes: Vec<&str> = sizes.split(',').collect();
        if sizes.len() != self.panes.len() {
            return;
        }
        for (pane, size) in self.panes.iter_mut().zip(sizes) {
            if pane.size.is_some()
                && let Some(size) = view_state::parse_length(size)
            {
                pane.size = Some(size.max(pane.min));
            }
        }
        if let Some(list) = view_state::field(state, "collapsed") {
            let collapsed: Vec<usize> = list
                .split(',')
                .filter_map(|i| i.trim().parse().ok())
                .collect();
            for (i, pane) in self.panes.iter_mut().enumerate() {
                if pane.collapsible {
                    pane.collapsed = collapsed.contains(&i);
                    let open = if pane.collapsed { 0.0 } else { 1.0 };
                    pane.open = Transition::new(open);
                    pane.shown = open;
                }
            }
        }
        cx.request_layout();
        cx.request_paint();
    }

    fn style(&self) -> Style {
        let tracks: Vec<GridTemplateComponent<String>> = self
            .panes
            .iter()
            .filter(|p| p.is_shown())
            .map(|p| {
                GridTemplateComponent::Single(match p.length() {
                    Some(size) => length(size),
                    None => minmax(length(p.min), fr(1.0)),
                })
            })
            .collect();
        let mut style = Style {
            display: Display::Grid,
            size: LayoutSize {
                width: percent(1.0),
                height: percent(1.0),
            },
            ..Default::default()
        };
        if self.side_by_side {
            style.grid_template_columns = tracks;
            style.grid_template_rows = vec![GridTemplateComponent::Single(fr(1.0))];
            style.gap.width = length(DIVIDER);
        } else {
            style.grid_template_rows = tracks;
            style.grid_template_columns = vec![GridTemplateComponent::Single(fr(1.0))];
            style.gap.height = length(DIVIDER);
        }
        style
    }

    fn hides_child(&self, index: usize) -> bool {
        self.panes.get(index).is_some_and(|p| !p.is_shown())
    }

    fn cursor(&self, local: Point, size: Size) -> Option<Cursor> {
        let total = if self.side_by_side {
            size.width
        } else {
            size.height
        };
        let p = self.main_axis(local);
        let over = self.dragging.is_some()
            || self
                .dividers(total)
                .iter()
                .any(|d| p >= d.2 && p < d.2 + DIVIDER);
        over.then_some(if self.side_by_side {
            Cursor::ColResize
        } else {
            Cursor::RowResize
        })
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let size = cx.size();
        let total = if self.side_by_side {
            size.width
        } else {
            size.height
        };
        let line = cx.color(Role::Separator);
        for (_, _, at) in self.dividers(total) {
            let mid = at + DIVIDER / 2.0 - 0.5;
            let rect = if self.side_by_side {
                Rect::new(mid, 0.0, 1.0, size.height)
            } else {
                Rect::new(0.0, mid, size.width, 1.0)
            };
            cx.fill_rect(rect, line);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        let total = self.total(cx);
        match event {
            Event::AnimationFrame { now } => {
                for pane in self.panes.iter_mut().filter(|p| p.rail.is_some()) {
                    pane.shown = pane.open.value(*now);
                }
                cx.request_layout();
                cx.request_paint();
                if self.is_animating() {
                    cx.request_animation_frame();
                } else {
                    self.report(cx, total);
                }
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = self.main_axis(cx.local(*pos));
                let Some((a, b, at)) = self
                    .dividers(total)
                    .into_iter()
                    .find(|d| p >= d.2 && p < d.2 + DIVIDER)
                else {
                    return;
                };
                cx.set_handled();
                let now = cx.now();
                let double = self
                    .last_press
                    .is_some_and(|(t, d)| d == a && now - t < DOUBLE_CLICK);
                self.last_press = Some((now, a));
                if double && self.toggle_collapse(a, b) {
                    self.last_press = None;
                    cx.request_layout();
                    cx.request_paint();
                    if self.start_rails(now, cx.motion(RAIL_DURATION)) {
                        // Reported when the rail has finished moving.
                        cx.request_animation_frame();
                    } else {
                        self.report(cx, total);
                    }
                    return;
                }
                // A pane at its rail (or moving) keeps its width: double-click to open it.
                let railed = |i: usize| self.panes[i].rail.is_some() && self.panes[i].collapsed;
                if railed(a) || railed(b) || self.is_animating() {
                    return;
                }
                self.dragging = Some((a, b, p - at));
                cx.capture_pointer();
            }
            Event::PointerMove { pos } => {
                if let Some((a, b, grab)) = self.dragging {
                    let p = self.main_axis(cx.local(*pos)) - grab;
                    self.drag_to(a, b, p, total);
                    cx.request_layout();
                    cx.request_paint();
                    cx.set_handled();
                }
            }
            Event::PointerUp { .. } if self.dragging.take().is_some() => {
                cx.release_pointer();
                self.report(cx, total);
                cx.set_handled();
            }
            _ => {}
        }
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Group)
    }
}

const TAB_BAR: f32 = 26.0;
const TAB_PAD: f32 = 14.0;
pub const DOCUMENT_TAB_BAR: f32 = 30.0;
const DOC_TAB_MAX: f32 = 200.0;
const DOC_TAB_MIN: f32 = 72.0;
const DOC_INSET: f32 = 4.0;
const DOC_NEW: f32 = 28.0;
const DOC_CLOSE: f32 = 16.0;
const DOC_CLOSE_MARGIN: f32 = 6.0;
const TAB_DRAG: f32 = 4.0;

/// Emitted by a [`TabView::documents`] tab view when a tab's close button is clicked
/// (or the tab is middle-clicked, or Delete is pressed on the focused tab bar). The tab
/// stays until the app closes it with [`TabView::close_tab`], so it can first ask to
/// save changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabCloseRequested(pub usize);

/// Emitted by a [`TabView::documents`] tab view with [`TabView::new_tab_button`] when
/// its "+" button is clicked. The app opens the tab with [`TabView::add_tab`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewTabRequested;

/// Emitted when a document tab was dragged (or moved with Option+Cmd+Left / Right) from
/// position `from` to `to`. The tab view has already moved it, with its page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabMoved {
    pub from: usize,
    pub to: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TabPress {
    None,
    /// A tab of the segmented strip, chosen on release.
    Segment(usize),
    /// A document tab, at position `at` now (it moves while dragged), pressed where the
    /// pointer was at `x`, from position `from`.
    Tab {
        at: usize,
        from: usize,
        x: f32,
        dragging: bool,
    },
    Close(usize),
    New,
}

/// Pages switched by tabs (`NSTabView`). Insert one child per tab, in order. Emits
/// [`SelectionChanged`]. Open and close document tabs with [`TabView::add_tab`] and
/// [`TabView::close_tab`], which keep the pages and tabs together.
#[derive(Debug)]
pub struct TabView {
    titles: Vec<String>,
    shaped: Vec<CachedText>,
    /// Layouts of closed tabs' titles, released at the next paint.
    stale: Vec<CachedText>,
    /// The child (page) of each tab, by tab position: tabs move without moving pages.
    pages: Vec<usize>,
    modified: Vec<bool>,
    selected: usize,
    tabs: Vec<Rect>,
    press: TabPress,
    width: Dimension,
    documents: bool,
    new_button: bool,
    /// Where the pointer is over the tab view (widget coordinates), if it is.
    hover: Option<Point>,
    size: Size,
}

impl TabView {
    pub fn new<S: Into<String>>(titles: impl IntoIterator<Item = S>) -> Self {
        let titles: Vec<String> = titles.into_iter().map(Into::into).collect();
        let n = titles.len();
        Self {
            shaped: (0..n).map(|_| CachedText::default()).collect(),
            stale: Vec::new(),
            pages: (0..n).collect(),
            modified: vec![false; n],
            titles,
            selected: 0,
            tabs: Vec::new(),
            press: TabPress::None,
            width: percent(1.0),
            documents: false,
            new_button: false,
            hover: None,
            size: Size::default(),
        }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = length(width);
        self
    }

    /// Show the tabs as a document window's tab bar. Emits [`TabMoved`] and
    /// [`TabCloseRequested`].
    pub fn documents(mut self) -> Self {
        self.documents = true;
        self
    }

    /// Put a "+" button after the document tabs, which emits [`NewTabRequested`].
    /// Cmd+T (Ctrl+T off macOS) also requests a tab while the document bar is focused.
    pub fn new_tab_button(mut self, shown: bool) -> Self {
        self.new_button = shown;
        self
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn select_index(mut self, index: usize) -> Self {
        self.selected = index.min(self.titles.len().saturating_sub(1));
        self
    }

    pub fn set_selected(&mut self, cx: &mut UpdateCx, index: usize) {
        if index < self.titles.len() && index != self.selected {
            self.selected = index;
            cx.request_layout();
            cx.request_paint();
        }
    }

    pub fn tab_count(&self) -> usize {
        self.titles.len()
    }

    pub fn titles(&self) -> &[String] {
        &self.titles
    }

    pub fn set_title(&mut self, cx: &mut UpdateCx, index: usize, title: impl Into<String>) {
        if let Some(t) = self.titles.get_mut(index) {
            *t = title.into();
            cx.request_layout();
            cx.request_paint();
        }
    }

    /// Mark tab `index` as holding unsaved changes: a dot shows in place of its close
    /// button until the pointer is over it.
    pub fn set_modified(&mut self, cx: &mut UpdateCx, index: usize, modified: bool) {
        if let Some(m) = self.modified.get_mut(index)
            && *m != modified
        {
            *m = modified;
            cx.request_paint();
        }
    }

    pub fn is_modified(&self, index: usize) -> bool {
        self.modified.get(index).copied().unwrap_or(false)
    }

    /// The position among the tab view's children of the page of tab `index`. Pages
    /// keep their place when tabs move.
    pub fn page_index(&self, index: usize) -> Option<usize> {
        self.pages.get(index).copied()
    }

    /// Move tab `from` (with its page) to position `to`, which is clamped. The selected
    /// tab stays selected. Nothing is emitted.
    pub fn move_tab(&mut self, cx: &mut UpdateCx, from: usize, to: usize) {
        if self.reorder(from, to) {
            cx.request_layout();
            cx.request_paint();
        }
    }

    pub fn page(tree: &UiTree, tabs: WidgetId, index: usize) -> Option<WidgetId> {
        let child = tree.get::<TabView>(tabs)?.page_index(index)?;
        tree.children(tabs).get(child).copied()
    }

    pub fn add_tab(
        tree: &mut UiTree,
        tabs: WidgetId,
        title: impl Into<String>,
        page: impl Widget,
    ) -> Option<WidgetId> {
        tree.get::<TabView>(tabs)?;
        let child = tree.children(tabs).len();
        let page = tree.insert(Some(tabs), page);
        let title = title.into();
        tree.update::<TabView, _>(tabs, |view, cx| {
            view.titles.push(title);
            view.shaped.push(CachedText::default());
            view.pages.push(child);
            view.modified.push(false);
            view.selected = view.titles.len() - 1;
            cx.request_layout();
            cx.request_paint();
        });
        Some(page)
    }

    /// Close tab `index` of the tab view `tabs` and remove its page. When it was
    /// selected, the tab after it is selected (the one before it when it was the last).
    /// Returns whether there was such a tab.
    pub fn close_tab(
        tree: &mut UiTree,
        text: &mut TextStore,
        tabs: WidgetId,
        index: usize,
    ) -> bool {
        let Some(page) = Self::page(tree, tabs, index) else {
            return false;
        };
        tree.remove(page, text);
        tree.update::<TabView, _>(tabs, |view, cx| {
            let child = view.pages.remove(index);
            for p in &mut view.pages {
                if *p > child {
                    *p -= 1;
                }
            }
            view.titles.remove(index);
            view.modified.remove(index);
            view.stale.push(view.shaped.remove(index));
            if index < view.selected || view.selected >= view.titles.len() {
                view.selected = view.selected.saturating_sub(1);
            }
            if let TabPress::Tab { .. } | TabPress::Close(_) = view.press {
                view.press = TabPress::None;
            }
            cx.request_layout();
            cx.request_paint();
        });
        true
    }

    fn select(&mut self, cx: &mut EventCx, index: usize) {
        if index < self.titles.len() && index != self.selected {
            self.selected = index;
            cx.emit(SelectionChanged(index));
            cx.request_layout();
            cx.request_paint();
        }
        cx.set_handled();
    }

    fn reorder(&mut self, from: usize, to: usize) -> bool {
        let n = self.titles.len();
        let to = to.min(n.saturating_sub(1));
        if from >= n || from == to {
            return false;
        }
        let selected = self.selected;
        let title = self.titles.remove(from);
        self.titles.insert(to, title);
        let shaped = self.shaped.remove(from);
        self.shaped.insert(to, shaped);
        let page = self.pages.remove(from);
        self.pages.insert(to, page);
        let modified = self.modified.remove(from);
        self.modified.insert(to, modified);
        self.selected = if selected == from {
            to
        } else if from < selected && to >= selected {
            selected - 1
        } else if from > selected && to <= selected {
            selected + 1
        } else {
            selected
        };
        true
    }

    fn document_layout(&self, width: f32) -> (Vec<Rect>, Option<Rect>) {
        let n = self.titles.len();
        let button = if self.new_button { DOC_NEW } else { 0.0 };
        let room = (width - 2.0 * DOC_INSET - button).max(0.0);
        let each = if n == 0 {
            0.0
        } else {
            let fit = room / n as f32;
            if fit >= DOC_TAB_MIN {
                fit.min(DOC_TAB_MAX)
            } else {
                fit
            }
        };
        let tabs: Vec<Rect> = (0..n)
            .map(|i| Rect::new(DOC_INSET + i as f32 * each, 0.0, each, DOCUMENT_TAB_BAR))
            .collect();
        let new = self.new_button.then(|| {
            let x = DOC_INSET + n as f32 * each;
            Rect::new(
                x + (DOC_NEW - 22.0) / 2.0,
                (DOCUMENT_TAB_BAR - 22.0) / 2.0,
                22.0,
                22.0,
            )
        });
        (tabs, new)
    }

    fn close_rect(tab: Rect) -> Rect {
        Rect::new(
            tab.right() - DOC_CLOSE_MARGIN - DOC_CLOSE,
            tab.y + (tab.height - DOC_CLOSE) / 2.0,
            DOC_CLOSE,
            DOC_CLOSE,
        )
    }

    fn document_hit(&self, p: Point) -> TabPress {
        let (tabs, new) = self.document_layout(self.size.width);
        if new.is_some_and(|r| r.contains(p)) {
            return TabPress::New;
        }
        match tabs.iter().position(|r| r.contains(p)) {
            Some(i) if Self::close_rect(tabs[i]).contains(p) => TabPress::Close(i),
            Some(i) => TabPress::Tab {
                at: i,
                from: i,
                x: p.x,
                dragging: false,
            },
            None => TabPress::None,
        }
    }

    fn paint_documents(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let bar = Rect::new(0.0, 0.0, bounds.width, DOCUMENT_TAB_BAR);
        let well = cx.color(Role::SurfaceAlt);
        let surface = cx.color(Role::Surface);
        let line = cx.color(Role::Separator);
        let ink = cx.color(Role::Text);
        let muted = cx.color(Role::Muted);
        cx.fill_rect(bar, well);
        cx.fill_rect(
            Rect::new(0.0, DOCUMENT_TAB_BAR - 1.0, bounds.width, 1.0),
            line,
        );
        let (tabs, new) = self.document_layout(bounds.width);
        let hover = self.hover;
        let style = TextStyle::new(12.0);
        for (i, tab) in tabs.iter().enumerate() {
            let selected = i == self.selected;
            let hovered = hover.is_some_and(|p| tab.contains(p));
            if selected {
                let r = Rect::new(tab.x, 3.0, tab.width, DOCUMENT_TAB_BAR - 3.0);
                let radius = cx.corner(Corner::Control, 6.0, r.height);
                cx.rounded_rect(
                    r,
                    radius,
                    surface,
                    Border {
                        width: 1.0,
                        color: line,
                    },
                );
                cx.fill_rect(
                    Rect::new(tab.x + 1.0, DOCUMENT_TAB_BAR - 2.0, tab.width - 2.0, 2.0),
                    surface,
                );
            } else {
                if hovered || matches!(self.press, TabPress::Tab { at, .. } if at == i) {
                    let tint = crate::theme::mix(well, ink, 0.06);
                    let r = Rect::new(tab.x + 1.0, 4.0, tab.width - 2.0, DOCUMENT_TAB_BAR - 8.0);
                    let radius = cx.corner(Corner::Control, 5.0, r.height);
                    cx.rounded_rect(r, radius, tint, Border::default());
                }
                if i + 1 < tabs.len() && i + 1 != self.selected {
                    cx.fill_rect(
                        Rect::new(tab.right() - 0.5, 8.0, 1.0, DOCUMENT_TAB_BAR - 16.0),
                        line,
                    );
                }
            }
            let close = Self::close_rect(*tab);
            let text = cx.text_store();
            let size = self.shaped[i].shape(text, &self.titles[i], style, None);
            if let Some(layout) = self.shaped[i].layout() {
                let left = tab.x + 10.0;
                let room = (close.x - 4.0 - left).max(0.0);
                cx.push_clip(Rect::new(left, 0.0, room, DOCUMENT_TAB_BAR));
                cx.text(
                    Rect::new(
                        left,
                        (DOCUMENT_TAB_BAR - size.height) / 2.0 + 1.0,
                        size.width,
                        size.height,
                    ),
                    layout,
                    if selected { ink } else { muted },
                );
                cx.pop_clip();
            }
            let over_close = hover.is_some_and(|p| close.contains(p));
            if self.modified[i] && !hovered {
                let dot = Rect::new(close.x + 4.0, close.y + 4.0, 8.0, 8.0);
                cx.rounded_rect(dot, 4.0, muted, Border::default());
            } else if selected || hovered {
                if over_close || self.press == TabPress::Close(i) {
                    let tint = crate::theme::mix(surface, ink, 0.12);
                    cx.rounded_rect(close, 4.0, tint, Border::default());
                }
                let c = Point::new(close.x + DOC_CLOSE / 2.0, close.y + DOC_CLOSE / 2.0);
                let d = 3.5;
                cx.line(
                    Point::new(c.x - d, c.y - d),
                    Point::new(c.x + d, c.y + d),
                    1.4,
                    muted,
                );
                cx.line(
                    Point::new(c.x - d, c.y + d),
                    Point::new(c.x + d, c.y - d),
                    1.4,
                    muted,
                );
            }
        }
        if let Some(r) = new {
            if hover.is_some_and(|p| r.contains(p)) || self.press == TabPress::New {
                let tint = crate::theme::mix(well, ink, 0.1);
                cx.rounded_rect(r, 5.0, tint, Border::default());
            }
            let c = Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0);
            cx.line(
                Point::new(c.x - 5.0, c.y),
                Point::new(c.x + 5.0, c.y),
                1.5,
                muted,
            );
            cx.line(
                Point::new(c.x, c.y - 5.0),
                Point::new(c.x, c.y + 5.0),
                1.5,
                muted,
            );
        }
        if cx.is_focused() {
            match tabs.get(self.selected) {
                Some(tab) => {
                    let r = Rect::new(tab.x + 2.0, 5.0, tab.width - 4.0, DOCUMENT_TAB_BAR - 8.0);
                    focus_ring(cx, r, 5.0);
                }
                None => focus_ring(cx, bar, 0.0),
            }
        }
    }

    fn paint_segments(&mut self, cx: &mut PaintCx) {
        let bounds = cx.bounds();
        let well = cx.color(Role::SurfaceAlt);
        let line = cx.color(Role::Separator);
        cx.rounded_rect(
            Rect::new(
                0.0,
                TAB_BAR / 2.0,
                bounds.width,
                bounds.height - TAB_BAR / 2.0,
            ),
            cx.corner(Corner::Container, 8.0, bounds.height),
            well,
            Border {
                width: 1.0,
                color: line,
            },
        );
        let style = TextStyle::new(13.0);
        let widths: Vec<f32> = (0..self.titles.len())
            .map(|i| {
                let text = cx.text_store();
                self.shaped[i]
                    .shape(text, &self.titles[i], style, None)
                    .width
                    + 2.0 * TAB_PAD
            })
            .collect();
        let total: f32 = widths.iter().sum();
        let mut x = ((bounds.width - total) / 2.0).max(0.0);
        let strip = Rect::new(x, 0.0, total, TAB_BAR);
        let surface = cx.color(Role::Surface);
        let outline = cx.color(Role::Outline);
        let radius = cx.corner(Corner::Control, 6.0, TAB_BAR);
        let inner_radius = cx.corner(Corner::Control, 5.0, TAB_BAR - 2.0);
        cx.bezel(
            strip,
            radius,
            surface,
            Border {
                width: 1.0,
                color: outline,
            },
        );
        self.tabs.clear();
        for (i, w) in widths.iter().enumerate() {
            let rect = Rect::new(x, 0.0, *w, TAB_BAR);
            let inner = Rect::new(rect.x + 1.0, 1.0, rect.width - 2.0, TAB_BAR - 2.0);
            let selected = i == self.selected;
            if selected {
                let accent = cx.color(Role::Accent);
                cx.bezel(inner, inner_radius, accent, Border::default());
            } else if self.press == TabPress::Segment(i) {
                let tint = pressed_tint(cx, surface);
                cx.rounded_rect(inner, inner_radius, tint, Border::default());
            }
            if let Some(layout) = self.shaped[i].layout() {
                let size = self.shaped[i].size();
                let ink = cx.color(if selected { Role::OnAccent } else { Role::Text });
                cx.text(
                    Rect::new(
                        rect.x + (rect.width - size.width) / 2.0,
                        (TAB_BAR - size.height) / 2.0,
                        size.width,
                        size.height,
                    ),
                    layout,
                    ink,
                );
            }
            self.tabs.push(rect);
            x += w;
        }
        if cx.is_focused() {
            focus_ring(cx, strip, radius);
        }
    }

    fn document_event(&mut self, cx: &mut EventCx, event: &Event) {
        let n = self.titles.len();
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let hit = self.document_hit(cx.local(*pos));
                if hit == TabPress::None {
                    return;
                }
                if let TabPress::Tab { at, .. } = hit {
                    // A tab is chosen on the press, so it can be dragged at once.
                    self.select(cx, at);
                }
                self.press = hit;
                cx.request_focus();
                cx.capture_pointer();
                cx.request_paint();
                cx.set_handled();
            }
            Event::PointerDown {
                pos,
                button: PointerButton::Middle,
            } => {
                if let TabPress::Tab { at, .. } | TabPress::Close(at) =
                    self.document_hit(cx.local(*pos))
                {
                    cx.emit(TabCloseRequested(at));
                    cx.set_handled();
                }
            }
            Event::PointerMove { pos } => {
                let p = cx.local(*pos);
                if self.hover != Some(p) {
                    self.hover = Some(p);
                    cx.request_paint();
                }
                if let TabPress::Tab {
                    at,
                    from,
                    x,
                    dragging,
                } = self.press
                {
                    let dragging = dragging || (p.x - x).abs() >= TAB_DRAG;
                    let mut at = at;
                    if dragging {
                        let (tabs, _) = self.document_layout(self.size.width);
                        let over = tabs
                            .iter()
                            .position(|r| p.x >= r.x && p.x < r.right())
                            .unwrap_or(if p.x < DOC_INSET {
                                0
                            } else {
                                n.saturating_sub(1)
                            });
                        if over != at && self.reorder(at, over) {
                            at = over;
                            cx.request_layout();
                        }
                    }
                    self.press = TabPress::Tab {
                        at,
                        from,
                        x,
                        dragging,
                    };
                    cx.request_paint();
                    cx.set_handled();
                }
            }
            Event::PointerLeave | Event::HoverChanged(false) => {
                if self.hover.take().is_some() {
                    cx.request_paint();
                }
            }
            Event::PointerUp { pos, .. } => {
                let press = std::mem::replace(&mut self.press, TabPress::None);
                if press == TabPress::None {
                    return;
                }
                cx.release_pointer();
                cx.request_paint();
                let hit = self.document_hit(cx.local(*pos));
                match press {
                    TabPress::Tab { at, from, .. } if at != from => {
                        cx.emit(TabMoved { from, to: at });
                    }
                    TabPress::Close(i) if hit == TabPress::Close(i) => {
                        cx.emit(TabCloseRequested(i));
                    }
                    TabPress::New if hit == TabPress::New => cx.emit(NewTabRequested),
                    _ => {}
                }
                cx.set_handled();
            }
            Event::KeyDown {
                key: key @ (Key::ArrowLeft | Key::ArrowRight),
                modifiers,
            } if n > 0 && cx.is_focused() => {
                let left = *key == Key::ArrowLeft;
                if modifiers.alt && modifiers.command {
                    let from = self.selected;
                    let to = if left {
                        from.saturating_sub(1)
                    } else {
                        from + 1
                    };
                    if self.reorder(from, to) {
                        cx.emit(TabMoved {
                            from,
                            to: self.selected,
                        });
                        cx.request_layout();
                        cx.request_paint();
                    }
                    cx.set_handled();
                } else {
                    let next = if left {
                        (self.selected + n - 1) % n
                    } else {
                        (self.selected + 1) % n
                    };
                    self.select(cx, next);
                }
            }
            Event::KeyDown {
                key: key @ (Key::Home | Key::End),
                ..
            } if n > 0 && cx.is_focused() => {
                self.select(cx, if *key == Key::Home { 0 } else { n - 1 });
            }
            Event::KeyDown {
                key: Key::Delete | Key::Backspace,
                ..
            } if n > 0 && cx.is_focused() => {
                cx.emit(TabCloseRequested(self.selected));
                cx.set_handled();
            }
            Event::KeyDown {
                key: Key::Char('t'),
                modifiers,
            } if self.new_button
                && cx.is_focused()
                && modifiers.command
                && !modifiers.alt
                && !modifiers.shift =>
            {
                cx.emit(NewTabRequested);
                cx.set_handled();
            }
            // Assistive technology: the tabs, then their close buttons, then "+".
            Event::ActivateItem(i) => {
                if *i < n {
                    self.select(cx, *i);
                } else if *i < 2 * n {
                    cx.emit(TabCloseRequested(*i - n));
                    cx.set_handled();
                } else if self.new_button && *i == 2 * n {
                    cx.emit(NewTabRequested);
                    cx.set_handled();
                }
            }
            _ => {}
        }
    }
}

impl Widget for TabView {
    /// The selected tab: `selected:2`.
    fn save_state(&self) -> Option<String> {
        Some(format!("selected:{}", self.selected))
    }

    fn restore_state(&mut self, cx: &mut UpdateCx, state: &str) {
        if let Some(index) = view_state::field(state, "selected").and_then(|i| i.parse().ok()) {
            self.set_selected(cx, index);
        }
    }

    fn style(&self) -> Style {
        let padding = if self.documents {
            LayoutRect {
                left: length(0.0),
                right: length(0.0),
                top: length(DOCUMENT_TAB_BAR),
                bottom: length(0.0),
            }
        } else {
            LayoutRect {
                left: length(16.0),
                right: length(16.0),
                top: length(TAB_BAR + 12.0),
                bottom: length(16.0),
            }
        };
        Style {
            flex_direction: FlexDirection::Column,
            padding,
            size: LayoutSize {
                width: self.width,
                height: auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn hides_child(&self, index: usize) -> bool {
        self.pages.get(self.selected) != Some(&index)
    }

    /// A document tab's title (it may be cut short), and what its buttons do.
    fn tooltip(&self, local: Point) -> Option<String> {
        if !self.documents {
            return None;
        }
        match self.document_hit(local) {
            TabPress::Tab { at, .. } => Some(self.titles[at].clone()),
            TabPress::Close(_) => Some("Close Tab".into()),
            TabPress::New => Some("New Tab (Cmd/Ctrl+T on the tab bar)".into()),
            _ => None,
        }
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        self.size = cx.size();
        for mut stale in self.stale.drain(..) {
            stale.release(cx.text_store());
        }
        if self.documents {
            self.paint_documents(cx);
        } else {
            self.paint_segments(cx);
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        self.size = cx.size();
        if self.documents {
            self.document_event(cx, event);
            return;
        }
        let n = self.titles.len();
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } => {
                let p = cx.local(*pos);
                if let Some(i) = self.tabs.iter().position(|r| r.contains(p)) {
                    self.press = TabPress::Segment(i);
                    cx.request_focus();
                    cx.request_paint();
                    cx.set_handled();
                }
            }
            Event::PointerUp { pos, .. } => {
                if let TabPress::Segment(i) = std::mem::replace(&mut self.press, TabPress::None) {
                    cx.request_paint();
                    if self.tabs[i].contains(cx.local(*pos)) {
                        self.select(cx, i);
                    }
                }
            }
            Event::KeyDown {
                key: key @ (Key::ArrowLeft | Key::ArrowRight),
                ..
            } if n > 0 && cx.is_focused() => {
                let next = if *key == Key::ArrowLeft {
                    (self.selected + n - 1) % n
                } else {
                    (self.selected + 1) % n
                };
                self.select(cx, next);
            }
            Event::ActivateItem(i) => self.select(cx, *i),
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::TabList)
    }

    fn accessibility_items(&self) -> Vec<accesskit::Node> {
        let bounds = |r: Rect| accesskit::Rect {
            x0: f64::from(r.x),
            y0: f64::from(r.y),
            x1: f64::from(r.right()),
            y1: f64::from(r.bottom()),
        };
        let (doc_tabs, new) = if self.documents {
            self.document_layout(self.size.width)
        } else {
            (self.tabs.clone(), None)
        };
        let mut items: Vec<accesskit::Node> = self
            .titles
            .iter()
            .enumerate()
            .map(|(i, title)| {
                let mut node = accesskit::Node::new(accesskit::Role::Tab);
                node.set_label(title.clone());
                node.set_selected(i == self.selected);
                if self.documents && self.modified[i] {
                    node.set_description("Unsaved changes");
                }
                if let Some(&r) = doc_tabs.get(i) {
                    node.set_bounds(bounds(r));
                }
                node.add_action(accesskit::Action::Click);
                node
            })
            .collect();
        if self.documents {
            for (i, title) in self.titles.iter().enumerate() {
                let mut node = accesskit::Node::new(accesskit::Role::Button);
                node.set_label(format!("Close {title}"));
                if let Some(&r) = doc_tabs.get(i) {
                    node.set_bounds(bounds(Self::close_rect(r)));
                }
                node.add_action(accesskit::Action::Click);
                items.push(node);
            }
            if let Some(r) = new {
                let mut node = accesskit::Node::new(accesskit::Role::Button);
                node.set_label("New Tab");
                node.set_bounds(bounds(r));
                node.add_action(accesskit::Action::Click);
                items.push(node);
            }
        }
        items
    }

    fn removed(&mut self, text: &mut TextStore) {
        for cached in self.shaped.iter_mut().chain(self.stale.iter_mut()) {
            cached.release(text);
        }
    }
}

const HEADER: f32 = 24.0;
/// Seconds for the chevron to turn.
const TURN: f64 = 0.15;

/// A titled section whose content is shown or hidden by clicking the title (SwiftUI
/// `DisclosureGroup`). Emits [`Toggled`] with the expanded state.
#[derive(Debug)]
pub struct DisclosureGroup {
    title: String,
    shaped: CachedText,
    expanded: bool,
    /// The chevron's turn: 0 pointing right, 1 pointing down.
    turn: Transition,
    now: f64,
}

impl DisclosureGroup {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            shaped: CachedText::default(),
            expanded: false,
            turn: Transition::new(0.0),
            now: 0.0,
        }
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self.turn = Transition::new(if expanded { 1.0 } else { 0.0 });
        self
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Expand or collapse without emitting [`Toggled`]; the chevron turns as it does
    /// for a click.
    pub fn set_expanded(&mut self, cx: &mut UpdateCx, expanded: bool) {
        if expanded != self.expanded {
            self.expanded = expanded;
            self.now = cx.now();
            self.turn.animate_to(
                if expanded { 1.0 } else { 0.0 },
                self.now,
                cx.motion(TURN),
                Easing::EaseInOut,
            );
            cx.request_animation_frame();
            cx.request_layout();
            cx.request_paint();
        }
    }

    fn toggle(&mut self, cx: &mut EventCx, expanded: bool) {
        if expanded == self.expanded {
            return;
        }
        self.expanded = expanded;
        let target = if expanded { 1.0 } else { 0.0 };
        self.now = cx.now();
        self.turn
            .animate_to(target, self.now, cx.motion(TURN), Easing::EaseInOut);
        cx.request_animation_frame();
        cx.emit(Toggled(expanded));
        cx.request_layout();
        cx.request_paint();
    }
}

impl Widget for DisclosureGroup {
    /// Whether it is open: `expanded:true`. Restoring does not turn the chevron: it is
    /// already where it ends.
    fn save_state(&self) -> Option<String> {
        Some(format!("expanded:{}", self.expanded))
    }

    fn restore_state(&mut self, cx: &mut UpdateCx, state: &str) {
        let expanded = match view_state::field(state, "expanded") {
            Some("true") => true,
            Some("false") => false,
            _ => return,
        };
        if expanded != self.expanded {
            self.expanded = expanded;
            self.turn = Transition::new(if expanded { 1.0 } else { 0.0 });
            cx.request_layout();
            cx.request_paint();
        }
    }

    fn style(&self) -> Style {
        Style {
            flex_direction: FlexDirection::Column,
            gap: LayoutSize {
                width: length(0.0),
                height: length(6.0),
            },
            padding: LayoutRect {
                left: length(18.0),
                right: length(0.0),
                top: length(HEADER + if self.expanded { 4.0 } else { 0.0 }),
                bottom: length(0.0),
            },
            size: LayoutSize {
                width: percent(1.0),
                height: auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn hides_child(&self, _index: usize) -> bool {
        !self.expanded
    }

    fn paint(&mut self, cx: &mut PaintCx) {
        let ink = cx.color(Role::Muted);
        let angle = self.turn.value(self.now) * std::f32::consts::FRAC_PI_2;
        let center = Point::new(8.0, HEADER / 2.0);
        let (sin, cos) = angle.sin_cos();
        let rotate =
            |x: f32, y: f32| Point::new(center.x + x * cos - y * sin, center.y + x * sin + y * cos);
        let (a, tip, b) = (rotate(-2.0, -4.0), rotate(2.0, 0.0), rotate(-2.0, 4.0));
        cx.line(a, tip, 1.6, ink);
        cx.line(tip, b, 1.6, ink);
        let style = TextStyle::new(14.0).weight(600);
        let size = self.shaped.shape(cx.text_store(), &self.title, style, None);
        if let Some(layout) = self.shaped.layout() {
            let text = cx.color(Role::Text);
            cx.text(
                Rect::new(18.0, (HEADER - size.height) / 2.0, size.width, size.height),
                layout,
                text,
            );
        }
        if cx.is_focused() {
            focus_ring(
                cx,
                Rect::new(0.0, 0.0, 18.0 + size.width + 4.0, HEADER),
                4.0,
            );
        }
    }

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown {
                pos,
                button: PointerButton::Primary,
            } if cx.local(*pos).y < HEADER => {
                self.toggle(cx, !self.expanded);
                cx.set_handled();
            }
            Event::KeyDown { key, .. } if cx.is_focused() => {
                let target = match key {
                    Key::Space => !self.expanded,
                    Key::ArrowRight => true,
                    Key::ArrowLeft => false,
                    _ => return,
                };
                self.toggle(cx, target);
                cx.set_handled();
            }
            Event::Activate => self.toggle(cx, !self.expanded),
            Event::AnimationFrame { now } => {
                self.now = *now;
                cx.request_paint();
                if !self.turn.is_done(*now) {
                    cx.request_animation_frame();
                }
            }
            _ => {}
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Group)
    }

    fn accessibility(&self, node: &mut accesskit::Node) {
        node.set_label(self.title.clone());
        node.set_expanded(self.expanded);
        node.add_action(accesskit::Action::Click);
    }

    fn removed(&mut self, text: &mut TextStore) {
        self.shaped.release(text);
    }
}

/// [`DisclosureGroup`]s stacked in a column, optionally only one open at a time. Not a
/// widget: it keeps the ids of a column container and its groups; pass the tree's
/// messages to [`Accordion::handle`].
#[derive(Debug, Clone)]
pub struct Accordion {
    container: WidgetId,
    groups: Vec<WidgetId>,
    exclusive: bool,
}

impl Accordion {
    /// An empty accordion filling the width of `parent`. With `exclusive`, at most one
    /// group is open at a time.
    pub fn new(tree: &mut UiTree, parent: WidgetId, exclusive: bool) -> Self {
        let container = tree.insert(
            Some(parent),
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                gap: LayoutSize {
                    width: length(0.0),
                    height: length(6.0),
                },
                size: LayoutSize {
                    width: percent(1.0),
                    height: auto(),
                },
                flex_shrink: 0.0,
                ..Default::default()
            }),
        );
        Self {
            container,
            groups: Vec::new(),
            exclusive,
        }
    }

    /// Add `group` at the end and return its id; insert its content into it. In an
    /// exclusive accordion a group added open stays closed if another one is open.
    pub fn add(&mut self, tree: &mut UiTree, group: DisclosureGroup) -> WidgetId {
        let group = if self.exclusive && group.is_expanded() && self.open_group(tree).is_some() {
            group.expanded(false)
        } else {
            group
        };
        if !self.groups.is_empty() {
            tree.insert(Some(self.container), Separator::horizontal());
        }
        let id = tree.insert(Some(self.container), group);
        self.groups.push(id);
        id
    }

    pub fn container(&self) -> WidgetId {
        self.container
    }

    pub fn groups(&self) -> &[WidgetId] {
        &self.groups
    }

    pub fn is_exclusive(&self) -> bool {
        self.exclusive
    }

    /// Allow one open group at a time, or any number. Turning it on keeps only the
    /// first open group open.
    pub fn set_exclusive(&mut self, tree: &mut UiTree, exclusive: bool) {
        self.exclusive = exclusive;
        if let Some(open) = self.open_group(tree) {
            self.close_others(tree, open);
        }
    }

    pub fn open_group(&self, tree: &UiTree) -> Option<WidgetId> {
        self.groups.iter().copied().find(|&g| {
            tree.get::<DisclosureGroup>(g)
                .is_some_and(DisclosureGroup::is_expanded)
        })
    }

    /// Open or close `group` from the app (no [`Toggled`] is emitted). Opening it in an
    /// exclusive accordion closes the others.
    pub fn set_expanded(&mut self, tree: &mut UiTree, group: WidgetId, expanded: bool) {
        if !self.groups.contains(&group) {
            return;
        }
        tree.update::<DisclosureGroup, _>(group, |g, cx| g.set_expanded(cx, expanded));
        if expanded {
            self.close_others(tree, group);
        }
    }

    /// Follow a group being opened by the user. Returns whether `message` came from one
    /// of the groups.
    pub fn handle(&mut self, tree: &mut UiTree, message: &Message) -> bool {
        if !self.groups.contains(&message.source) {
            return false;
        }
        if let Some(&Toggled(true)) = message.downcast_ref() {
            self.close_others(tree, message.source);
        }
        true
    }

    fn close_others(&mut self, tree: &mut UiTree, open: WidgetId) {
        if !self.exclusive {
            return;
        }
        for &g in &self.groups {
            if g != open {
                tree.update::<DisclosureGroup, _>(g, |g, cx| g.set_expanded(cx, false));
            }
        }
    }
}
