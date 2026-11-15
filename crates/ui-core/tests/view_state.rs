use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, length, percent};
use noa_gpui_core::widgets::{
    ColumnSort, Container, DIVIDER, DisclosureGroup, InnerWindow, OutlineItem, OutlineView,
    SplitPane, SplitView, TABLE_HEADER, TabView, TableColumn, TableView, WindowManager,
    WindowPlacement,
};
use noa_gpui_core::{Event, PointerButton, Style, UiTree, ViewState, WidgetId};
use noa_gpui_scene::{Point, Rect, Size};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(800.0, 600.0);

struct Harness {
    tree: UiTree,
    text: TextStore,
    root: WidgetId,
}

impl Harness {
    fn new() -> Self {
        let mut tree = UiTree::new();
        let root = tree.insert(
            None,
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            }),
        );
        Self {
            tree,
            text: TextStore::new(FontOptions {
                locale: "en-US".into(),
                load_system_fonts: false,
            }),
            root,
        }
    }

    fn add(&mut self, widget: impl noa_gpui_core::Widget) -> WidgetId {
        let id = self.tree.insert(Some(self.root), widget);
        self.prepare();
        id
    }

    fn prepare(&mut self) {
        self.tree.prepare(VIEWPORT, &mut self.text);
    }

    fn rect(&self, id: WidgetId) -> Rect {
        self.tree.window_rect(id).unwrap()
    }

    fn dispatch(&mut self, event: Event) {
        self.tree.dispatch(event, &mut self.text);
        self.prepare();
    }

    fn drag(&mut self, from: Point, to: Point) {
        let button = PointerButton::Primary;
        self.dispatch(Event::PointerDown { pos: from, button });
        self.dispatch(Event::PointerMove { pos: to });
        self.dispatch(Event::PointerUp { pos: to, button });
    }

    fn click(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.dispatch(Event::PointerDown { pos, button });
        self.dispatch(Event::PointerUp { pos, button });
    }

    fn saved(&self) -> ViewState {
        ViewState::parse(&self.tree.view_state().to_text())
    }
}

struct Rows;

impl noa_gpui_core::widgets::TableData for Rows {
    fn row_count(&self) -> usize {
        5
    }

    fn cell(&self, row: usize, column: usize) -> std::borrow::Cow<'_, str> {
        format!("{}", (row * 7 + column * 3) % 5).into()
    }
}

fn table_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("Name", 160.0).sort(ColumnSort::Text),
        TableColumn::new("Size", 100.0).sort(ColumnSort::Number),
        TableColumn::new("Kind", 80.0).flex(1.0),
    ]
}

fn table() -> TableView {
    TableView::new(table_columns(), Rows)
        .width(500.0)
        .height(200.0)
}

#[test]
fn a_table_gets_back_the_columns_the_user_hid_and_showed() {
    let columns = || {
        vec![
            TableColumn::new("Name", 160.0),
            TableColumn::new("Size", 100.0),
            TableColumn::new("Kind", 80.0).hidden(true),
        ]
    };
    let mut h = Harness::new();
    let t = h.add(TableView::new(columns(), Rows).column_chooser(true));
    h.tree.set_autosave_name(t, "files");
    assert_eq!(h.tree.view_state().get("files"), None, "as the app made it");
    h.tree.update::<TableView, _>(t, |table, cx| {
        table.set_column_visible(cx, 1, false);
        table.set_column_visible(cx, 2, true);
    });
    let saved = h.saved();
    assert_eq!(saved.get("files"), Some("columns:3;hidden:1"));

    let mut next = Harness::new();
    next.tree.restore_view_state(saved);
    let t = next.add(TableView::new(columns(), Rows));
    next.tree.set_autosave_name(t, "files");
    let view = next.tree.get::<TableView>(t).unwrap();
    assert_eq!(view.visible_columns(), [0, 2]);

    // Every column shown, though the app hides Kind: recorded with none hidden.
    next.tree.update::<TableView, _>(t, |table, cx| {
        table.set_column_visible(cx, 1, true);
    });
    assert_eq!(
        next.tree.view_state().get("files"),
        Some("columns:3;hidden:")
    );
    let saved = next.saved();
    let mut third = Harness::new();
    third.tree.restore_view_state(saved);
    let t = third.add(TableView::new(columns(), Rows));
    third.tree.set_autosave_name(t, "files");
    let view = third.tree.get::<TableView>(t).unwrap();
    assert_eq!(view.visible_columns(), [0, 1, 2]);
}

#[test]
fn a_table_gets_back_the_widths_the_user_dragged_and_its_sort() {
    let mut h = Harness::new();
    let t = h.add(table());
    h.tree.set_autosave_name(t, "files");
    assert_eq!(h.tree.autosave_name(t), Some("files"));
    assert_eq!(
        h.tree.view_state().get("files"),
        None,
        "nothing arranged yet"
    );

    // Drag the edge between Name and Size 40 px right; sort by Size, then descending.
    let r = h.rect(t);
    let edge = Point::new(r.x + 1.0 + 160.0, r.y + TABLE_HEADER / 2.0);
    h.drag(edge, Point::new(edge.x + 40.0, edge.y));
    let size_header = Point::new(r.x + 1.0 + 200.0 + 50.0, r.y + TABLE_HEADER / 2.0);
    h.click(size_header);
    h.click(size_header);
    let view = h.tree.get::<TableView>(t).unwrap();
    assert_eq!(view.columns()[0].width(), 200.0);
    assert_eq!(view.sort_column(), Some((1, false)));
    let saved = h.saved();
    assert_eq!(
        saved.get("files"),
        Some("columns:3;widths:0=200;sort:1,desc")
    );

    // The next run: a new table, named after the state is restored.
    let mut next = Harness::new();
    next.tree.restore_view_state(saved);
    let t = next.add(table());
    let kind_before = next.tree.get::<TableView>(t).unwrap().column_width(2);
    next.tree.set_autosave_name(t, "files");
    next.prepare();
    let view = next.tree.get::<TableView>(t).unwrap();
    assert_eq!(view.column_width(0), 200.0);
    assert_eq!(
        view.column_width(1),
        100.0,
        "not dragged: as the app made it"
    );
    assert_eq!(
        view.column_width(2),
        kind_before - 40.0,
        "the flexible column still takes what is left"
    );
    assert_eq!(view.sort_column(), Some((1, false)));
    let sizes: Vec<String> = view
        .displayed_rows()
        .iter()
        .map(|&r| format!("{}", (r * 7 + 3) % 5))
        .collect();
    let mut sorted = sizes.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(sizes, sorted);
}

#[test]
fn a_table_whose_columns_changed_ignores_the_saved_state() {
    let mut state = ViewState::new();
    state.set("files", "columns:2;widths:0=300;sort:1,asc");
    let mut h = Harness::new();
    h.tree.restore_view_state(state);
    let t = h.add(table());
    h.tree.set_autosave_name(t, "files");
    let view = h.tree.get::<TableView>(t).unwrap();
    assert_eq!(view.columns()[0].width(), 160.0);
    assert_eq!(view.sort_column(), None);
    h.tree.restore_view_state(ViewState::from_iter([(
        "files".to_string(),
        "columns:3;widths:0=x,9=10,1=-4,2=120;sort:2,asc".to_string(),
    )]));
    let view = h.tree.get::<TableView>(t).unwrap();
    assert_eq!(view.columns()[0].width(), 160.0);
    assert_eq!(view.columns()[1].width(), 100.0);
    assert_eq!(
        view.columns()[2].width(),
        120.0,
        "a column may be given a width"
    );
    assert_eq!(view.sort_column(), None, "Kind does not sort");
}

#[test]
fn set_column_width_fixes_a_width_down_to_the_minimum() {
    let mut h = Harness::new();
    let t = h.add(table());
    h.tree
        .update::<TableView, _>(t, |v, cx| v.set_column_width(cx, 2, 10.0));
    h.prepare();
    let view = h.tree.get::<TableView>(t).unwrap();
    assert_eq!(
        view.column_width(2),
        40.0,
        "the minimum, and no longer flexible"
    );
}

fn split(h: &mut Harness) -> (WidgetId, [WidgetId; 3]) {
    let split = h.add(
        SplitView::horizontal()
            .pane(SplitPane::fixed(150.0).min(100.0).collapsible(true))
            .pane(SplitPane::flexible().min(120.0))
            .pane(SplitPane::fixed(100.0).min(80.0).rail(40.0)),
    );
    let panes = [0, 1, 2].map(|_| {
        let id = h.tree.insert(
            Some(split),
            Container::new(Style {
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            }),
        );
        h.prepare();
        id
    });
    (split, panes)
}

#[test]
fn a_split_view_gets_back_its_divider_and_collapsed_panes_without_animating() {
    let mut h = Harness::new();
    let (s, [left, ..]) = split(&mut h);
    h.tree.set_autosave_name(s, "main.split");
    let divider = Point::new(150.0 + DIVIDER / 2.0, 200.0);
    h.drag(divider, Point::new(230.0 + DIVIDER / 2.0, 200.0));
    assert_eq!(h.rect(left).width, 230.0);
    h.tree
        .update::<SplitView, _>(s, |v, cx| v.set_collapsed(cx, 2, true));
    let saved = h.saved();
    assert_eq!(saved.get("main.split"), Some("sizes:230,-,100;collapsed:2"));

    let mut next = Harness::new();
    let (s, [left, _, right]) = split(&mut next);
    next.tree.set_autosave_name(s, "main.split");
    next.tree.restore_view_state(saved);
    next.prepare();
    assert_eq!(next.rect(left).width, 230.0);
    assert_eq!(next.rect(right).width, 40.0, "at its rail at once");
    let view = next.tree.get::<SplitView>(s).unwrap();
    assert!(view.is_collapsed(2));
    assert!(!view.is_animating());
}

#[test]
fn a_split_view_keeps_its_minimums_and_ignores_another_pane_count() {
    let mut h = Harness::new();
    let (s, [left, ..]) = split(&mut h);
    h.tree.restore_view_state(ViewState::from_iter([(
        "s".to_string(),
        "sizes:20,-,100;collapsed:1".to_string(),
    )]));
    h.tree.set_autosave_name(s, "s");
    h.prepare();
    assert_eq!(h.rect(left).width, 100.0, "the pane's minimum");
    assert!(
        !h.tree.get::<SplitView>(s).unwrap().is_collapsed(1),
        "the flexible pane cannot collapse"
    );
    h.tree.restore_view_state(ViewState::from_iter([(
        "s".to_string(),
        "sizes:300,-;collapsed:0".to_string(),
    )]));
    h.prepare();
    assert_eq!(h.rect(left).width, 100.0);
}

#[test]
fn tabs_and_disclosure_groups_reopen_as_they_were_left() {
    let mut h = Harness::new();
    let tabs = h.add(TabView::new(["General", "Advanced", "Help"]));
    for _ in 0..3 {
        h.tree.insert(Some(tabs), Container::new(Style::default()));
    }
    let group = h.add(DisclosureGroup::new("Details"));
    h.tree.set_autosave_name(tabs, "prefs.tabs");
    h.tree.set_autosave_name(group, "prefs.details");
    h.tree
        .update::<TabView, _>(tabs, |v, cx| v.set_selected(cx, 2));
    h.tree
        .update::<DisclosureGroup, _>(group, |v, cx| v.set_expanded(cx, true));
    let saved = h.saved();

    let mut next = Harness::new();
    next.tree.restore_view_state(saved);
    let tabs = next.add(TabView::new(["General", "Advanced", "Help"]));
    let pages: Vec<WidgetId> = (0..3)
        .map(|_| {
            next.tree
                .insert(Some(tabs), Container::new(Style::default()))
        })
        .collect();
    let group = next.add(DisclosureGroup::new("Details"));
    next.tree.set_autosave_name(tabs, "prefs.tabs");
    next.tree.set_autosave_name(group, "prefs.details");
    next.prepare();
    assert_eq!(next.tree.get::<TabView>(tabs).unwrap().selected(), 2);
    assert!(next.tree.is_hidden(pages[0]));
    assert!(!next.tree.is_hidden(pages[2]), "the restored page is shown");
    assert!(
        next.tree
            .get::<DisclosureGroup>(group)
            .unwrap()
            .is_expanded()
    );
    assert!(
        !next.tree.needs_update(VIEWPORT),
        "restored as it ends: no chevron animation"
    );

    next.tree.restore_view_state(ViewState::from_iter([(
        "prefs.tabs".to_string(),
        "selected:7".to_string(),
    )]));
    assert_eq!(next.tree.get::<TabView>(tabs).unwrap().selected(), 2);
}

fn places(extra: bool) -> OutlineView {
    let mut projects = OutlineItem::new("Projects")
        .child(OutlineItem::new("noa"))
        .child(OutlineItem::new("a/b").child(OutlineItem::new("deep")));
    if extra {
        projects = projects.child(OutlineItem::new("new"));
    }
    let mut items = vec![
        OutlineItem::new("Places")
            .expanded(true)
            .child(OutlineItem::new("Home"))
            .child(projects),
    ];
    if extra {
        items.insert(
            0,
            OutlineItem::new("Recent")
                .expanded(true)
                .child(OutlineItem::new("x")),
        );
    }
    OutlineView::new(items)
}

fn titles(h: &Harness, id: WidgetId) -> Vec<String> {
    let view = h.tree.get::<OutlineView>(id).unwrap();
    view.visible_items()
        .iter()
        .map(|&i| view.title(i).to_string())
        .collect()
}

#[test]
fn an_outline_finds_its_expanded_items_by_their_titles() {
    let mut h = Harness::new();
    let o = h.add(places(false));
    h.tree.set_autosave_name(o, "sidebar");
    // Places > Projects (2), a/b (4) expanded; Places collapsed then reopened.
    h.tree.update::<OutlineView, _>(o, |v, cx| {
        v.set_expanded(cx, 2, true);
        v.set_expanded(cx, 4, true);
    });
    let saved = h.saved();
    assert_eq!(
        saved.get("sidebar"),
        Some("+Places\n+Places/Projects\n+Places/Projects/a\\/b")
    );

    // The next version adds a section at the top and an item: positions moved.
    let mut next = Harness::new();
    next.tree.restore_view_state(saved);
    let o = next.add(places(true));
    next.tree.set_autosave_name(o, "sidebar");
    next.prepare();
    assert_eq!(
        titles(&next, o),
        [
            "Recent", "x", "Places", "Home", "Projects", "noa", "a/b", "deep", "new"
        ]
    );
}

#[test]
fn collapsing_an_outline_item_on_restore_moves_a_hidden_selection_to_it() {
    let mut h = Harness::new();
    let o = h.add(places(false));
    h.tree.update::<OutlineView, _>(o, |v, cx| {
        v.set_expanded(cx, 2, true);
        v.select_item(cx, Some(3));
    });
    h.tree.restore_view_state(ViewState::from_iter([(
        "sidebar".to_string(),
        "-Places/Projects\n+Nowhere\nnonsense".to_string(),
    )]));
    h.tree.set_autosave_name(o, "sidebar");
    h.prepare();
    assert_eq!(titles(&h, o), ["Places", "Home", "Projects"]);
    assert_eq!(h.tree.get::<OutlineView>(o).unwrap().selected_items(), [2]);
}

struct Desk {
    h: Harness,
    manager: WindowManager,
}

impl Desk {
    fn new() -> Self {
        let mut h = Harness::new();
        let style = Style {
            size: LayoutSize {
                width: length(800.0),
                height: length(560.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        };
        let root = h.root;
        let manager = WindowManager::new(&mut h.tree, root, style);
        h.prepare();
        Self { h, manager }
    }

    fn open(&mut self, name: &str) -> WidgetId {
        let id = self.manager.open(
            &mut self.h.tree,
            name,
            Some(Rect::new(20.0, 20.0, 300.0, 200.0)),
        );
        self.h.tree.set_autosave_name(id, name);
        self.h.prepare();
        id
    }

    fn placement(&self, id: WidgetId) -> WindowPlacement {
        self.h.tree.get::<InnerWindow>(id).unwrap().placement()
    }
}

#[test]
fn an_inner_window_reopens_where_it_was_and_unzooms_to_its_frame() {
    let mut d = Desk::new();
    let notes = d.open("Notes");
    d.h.tree.update::<InnerWindow, _>(notes, |w, cx| {
        w.set_placement(
            cx,
            WindowPlacement::Frame(Rect::new(140.0, 90.0, 420.0, 260.0)),
        );
    });
    d.manager.zoom(&mut d.h.tree, notes);
    assert_eq!(d.placement(notes), WindowPlacement::Maximized);
    let saved = d.h.saved();
    assert_eq!(
        saved.get("Notes"),
        Some("placement:maximized;frame:140,90,420,260")
    );

    // The next run opens the window at its default frame; the name brings it back.
    let mut next = Desk::new();
    next.h.tree.restore_view_state(saved);
    let notes = next.open("Notes");
    assert_eq!(next.placement(notes), WindowPlacement::Maximized);
    next.manager.zoom(&mut next.h.tree, notes);
    assert_eq!(
        next.placement(notes),
        WindowPlacement::Frame(Rect::new(140.0, 90.0, 420.0, 260.0)),
        "the zoom button returns it to the frame it had"
    );
}

#[test]
fn a_closed_window_keeps_its_state_for_when_it_opens_again() {
    let mut d = Desk::new();
    let notes = d.open("Notes");
    d.open("Mail");
    let frame = Rect::new(60.0, 50.0, 320.0, 240.0);
    d.h.tree.update::<InnerWindow, _>(notes, |w, cx| {
        w.set_placement(cx, WindowPlacement::Frame(frame));
    });
    d.manager.close(&mut d.h.tree, &mut d.h.text, notes);
    d.h.prepare();
    let saved = d.h.saved();
    assert!(saved.get("Mail").is_some());
    assert_eq!(
        saved.get("Notes"),
        Some("placement:frame;frame:60,50,320,240")
    );
    let notes = d.open("Notes");
    assert_eq!(d.placement(notes), WindowPlacement::Frame(frame));
}

#[test]
fn entries_no_widget_claims_are_kept_and_names_can_be_removed() {
    let mut h = Harness::new();
    h.tree.restore_view_state(ViewState::from_iter([(
        "elsewhere".to_string(),
        "anything".to_string(),
    )]));
    let tabs = h.add(TabView::new(["A", "B"]));
    h.tree.set_autosave_name(tabs, "tabs");
    let state = h.tree.view_state();
    assert_eq!(state.get("elsewhere"), Some("anything"));
    assert_eq!(state.get("tabs"), Some("selected:0"));
    h.tree.set_autosave_name(tabs, "");
    assert_eq!(h.tree.autosave_name(tabs), None);
    let state = h.tree.view_state();
    assert_eq!(state.len(), 1);
}

fn long_page(h: &mut Harness) -> WidgetId {
    let scroll = h.add(
        noa_gpui_core::widgets::ScrollView::new(Style {
            flex_direction: FlexDirection::Column,
            size: LayoutSize {
                width: length(300.0),
                height: length(200.0),
            },
            ..Default::default()
        })
        .momentum(false),
    );
    for _ in 0..10 {
        h.tree.insert(
            Some(scroll),
            Container::new(Style {
                size: LayoutSize {
                    width: length(280.0),
                    height: length(100.0),
                },
                flex_shrink: 0.0,
                ..Default::default()
            }),
        );
    }
    h.prepare();
    scroll
}

#[test]
fn a_scroll_view_reopens_at_the_place_it_was_scrolled_to_within_its_content() {
    use noa_gpui_core::widgets::ScrollView;
    let mut h = Harness::new();
    let s = long_page(&mut h);
    h.tree.set_autosave_name(s, "page");
    let middle = h.rect(s);
    h.dispatch(Event::Wheel {
        pos: Point::new(middle.x + 50.0, middle.y + 50.0),
        delta: Point::new(0.0, -450.0),
    });
    let offset = h.tree.get::<ScrollView>(s).unwrap().offset();
    assert!(offset > 0.0);
    let saved = h.saved();
    assert_eq!(
        saved.get("page"),
        Some(format!("position:0,{offset}").as_str())
    );

    let mut next = Harness::new();
    next.tree.restore_view_state(saved);
    let s = long_page(&mut next);
    next.tree.set_autosave_name(s, "page");
    next.prepare();
    assert_eq!(next.tree.get::<ScrollView>(s).unwrap().offset(), offset);

    // Content that got shorter since: the position stops at its end.
    next.tree.restore_view_state(ViewState::from_iter([(
        "page".to_string(),
        "position:0,99999".to_string(),
    )]));
    next.prepare();
    assert_eq!(next.tree.get::<ScrollView>(s).unwrap().offset(), 800.0);
}
