use ui_core::style::{FlexDirection, Size as LayoutSize, length, percent};
use ui_core::widgets::{
    Container, DIVIDER, DOCUMENT_TAB_BAR, DisclosureGroup, GridCell, GridPlacementMode, GridTrack,
    GridView, GroupBox, LEGACY_TRACK, Label, Material, NewTabRequested, PanesChanged, PathControl,
    ScrollView, ScrollerStyle, SelectionChanged, Separator, SplitPane, SplitView,
    StackDistribution, StackView, TabCloseRequested, TabMoved, TabView, TextField, Toggled,
    VisualEffectView,
};
use ui_core::{Event, Key, Message, Modifiers, PointerButton, Style, UiTree, WidgetId, access_id};
use ui_scene::{DrawCommand, Point, Rect, Size};
use ui_text::{FontOptions, TextStore};

struct Harness {
    tree: UiTree,
    text: TextStore,
    root: WidgetId,
    viewport: Size,
}

impl Harness {
    fn new(width: f32, height: f32) -> Self {
        let mut tree = UiTree::new();
        let root = tree.insert(
            None,
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                align_items: ui_core::style::AlignItems::START,
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
            viewport: Size::new(width, height),
        }
    }

    fn add_to(&mut self, parent: WidgetId, widget: impl ui_core::Widget) -> WidgetId {
        let id = self.tree.insert(Some(parent), widget);
        self.prepare();
        id
    }

    fn add(&mut self, widget: impl ui_core::Widget) -> WidgetId {
        self.add_to(self.root, widget)
    }

    fn block(&mut self, parent: WidgetId, width: f32, height: f32) -> WidgetId {
        self.add_to(
            parent,
            Container::new(Style {
                size: LayoutSize {
                    width: length(width),
                    height: length(height),
                },
                flex_shrink: 0.0,
                ..Default::default()
            }),
        )
    }

    fn filler(&mut self, parent: WidgetId) -> WidgetId {
        self.add_to(parent, Container::new(Style::default()).clip(true))
    }

    fn prepare(&mut self) {
        self.tree.prepare(self.viewport, &mut self.text);
    }

    fn resize(&mut self, width: f32, height: f32) {
        self.viewport = Size::new(width, height);
        self.prepare();
    }

    fn rect(&self, id: WidgetId) -> Rect {
        self.tree.window_rect(id).unwrap()
    }

    fn down(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.tree
            .dispatch(Event::PointerDown { pos, button }, &mut self.text);
        self.prepare();
    }

    fn up(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.tree
            .dispatch(Event::PointerUp { pos, button }, &mut self.text);
        self.prepare();
    }

    fn move_to(&mut self, pos: Point) {
        self.tree
            .dispatch(Event::PointerMove { pos }, &mut self.text);
        self.prepare();
    }

    fn click(&mut self, pos: Point) {
        self.down(pos);
        self.up(pos);
    }

    fn key(&mut self, key: Key) {
        self.tree.dispatch(
            Event::KeyDown {
                key,
                modifiers: Modifiers::default(),
            },
            &mut self.text,
        );
        self.prepare();
    }

    fn wheel(&mut self, pos: Point, delta: Point) {
        self.tree
            .dispatch(Event::Wheel { pos, delta }, &mut self.text);
        self.prepare();
    }

    /// Run animation frames at 60 FPS from `start` for `seconds`.
    fn run(&mut self, start: f64, seconds: f64) {
        let frames = (seconds * 60.0) as u32;
        for i in 1..=frames {
            self.tree
                .animate(start + f64::from(i) / 60.0, &mut self.text);
            self.prepare();
        }
    }

    fn messages(&mut self) -> Vec<Message> {
        self.tree.take_messages()
    }
}

fn values<T: 'static + Clone>(messages: &[Message]) -> Vec<T> {
    messages
        .iter()
        .filter_map(|m| m.downcast_ref::<T>().cloned())
        .collect()
}

#[test]
fn stacks_space_their_children_and_close_the_gap_of_hidden_ones() {
    let mut h = Harness::new(400.0, 300.0);
    let stack = h.add(StackView::horizontal(10.0));
    let a = h.block(stack, 40.0, 20.0);
    let b = h.block(stack, 60.0, 20.0);
    let c = h.block(stack, 30.0, 20.0);
    assert_eq!([h.rect(a).x, h.rect(b).x, h.rect(c).x], [0.0, 50.0, 120.0]);
    assert_eq!(h.rect(stack).width, 150.0);

    h.tree.set_hidden(b, true);
    h.prepare();
    assert_eq!(h.rect(c).x, 50.0, "the gap closes");
    assert_eq!(h.rect(stack).width, 80.0);
    assert!(h.tree.is_hidden(b));
    assert_eq!(h.tree.hit_test(Point::new(60.0, 10.0)), Some(c));
    h.tree.set_hidden(b, false);
    h.prepare();
    assert_eq!(h.rect(c).x, 120.0);
}

#[test]
fn equal_distributions_follow_the_window_width() {
    let mut h = Harness::new(300.0, 200.0);
    let stack = h.add(
        StackView::horizontal(0.0)
            .distribution(StackDistribution::FillEqually)
            .fill_parent(true),
    );
    let cells: Vec<WidgetId> = [10.0, 50.0, 90.0]
        .into_iter()
        .map(|w| {
            let cell = h.filler(stack);
            h.block(cell, w, 10.0);
            cell
        })
        .collect();
    let widths = |h: &Harness| cells.iter().map(|&c| h.rect(c).width).collect::<Vec<_>>();
    assert_eq!(widths(&h), [100.0, 100.0, 100.0]);
    h.resize(600.0, 200.0);
    assert_eq!(widths(&h), [200.0, 200.0, 200.0]);

    let spaced = h.add(
        StackView::horizontal(0.0)
            .distribution(StackDistribution::EqualSpacing)
            .fill_parent(true),
    );
    let first = h.block(spaced, 50.0, 10.0);
    let last = h.block(spaced, 50.0, 10.0);
    assert_eq!((h.rect(first).x, h.rect(last).right()), (0.0, 600.0));
}

#[test]
fn grids_size_columns_and_span_cells_across_resizes() {
    let mut h = Harness::new(500.0, 300.0);
    let grid = h.add(
        GridView::new([
            GridTrack::Fixed(100.0),
            GridTrack::Auto,
            GridTrack::Flexible(1.0),
        ])
        .column_spacing(10.0)
        .row_spacing(5.0)
        .x_placement(GridPlacementMode::Fill)
        .fill_width(true),
    );
    let a = h.block(grid, 40.0, 20.0);
    let b = h.block(grid, 70.0, 20.0);
    let c = h.filler(grid);
    let span = h.add_to(grid, GridCell::new(2, 1));
    let d = h.block(grid, 10.0, 10.0);
    assert_eq!((h.rect(a).x, h.rect(b).x, h.rect(c).x), (0.0, 110.0, 190.0));
    assert_eq!(h.rect(c).width, 500.0 - 190.0);
    assert_eq!(h.rect(d).y, 25.0);
    assert_eq!(h.rect(span).width, 100.0 + 10.0 + 70.0);
    assert_eq!(h.rect(d).x, 190.0);
    h.resize(300.0, 300.0);
    assert_eq!(h.rect(b).x, 110.0);
    assert_eq!(h.rect(c).width, 300.0 - 190.0);
}

#[test]
fn group_boxes_put_their_content_below_the_title_and_separators_fill() {
    let mut h = Harness::new(400.0, 300.0);
    let group = h.add(GroupBox::new("Network").width(300.0));
    let inner = h.block(group, 50.0, 20.0);
    let line = h.add_to(group, Separator::horizontal());
    let r = h.rect(inner);
    assert!(r.y > 20.0, "below the title: {r:?}");
    assert_eq!(r.x, 12.0);
    assert_eq!(h.rect(line).width, 300.0 - 24.0);
    assert_eq!(h.rect(line).height, 1.0);
    let update = h.tree.accessibility_update(1.0);
    let node = &update
        .nodes
        .iter()
        .find(|(n, _)| *n == access_id(group))
        .unwrap()
        .1;
    assert_eq!(node.label(), Some("Network"));
}

fn split(h: &mut Harness) -> (WidgetId, [WidgetId; 3]) {
    let split = h.add(
        SplitView::horizontal()
            .pane(SplitPane::fixed(150.0).min(100.0).collapsible(true))
            .pane(SplitPane::flexible().min(120.0))
            .pane(SplitPane::fixed(100.0).min(80.0)),
    );
    let panes = [h.filler(split), h.filler(split), h.filler(split)];
    (split, panes)
}

#[test]
fn split_views_give_the_flexible_pane_what_resizing_the_window_changes() {
    let mut h = Harness::new(600.0, 400.0);
    let (_, [left, middle, right]) = split(&mut h);
    let widths = |h: &Harness| [left, middle, right].map(|p| h.rect(p).width);
    assert_eq!(widths(&h), [150.0, 600.0 - 250.0 - 2.0 * DIVIDER, 100.0]);
    assert_eq!(h.rect(right).right(), 600.0);
    assert_eq!(h.rect(middle).height, 400.0);
    h.resize(800.0, 300.0);
    assert_eq!(widths(&h), [150.0, 800.0 - 250.0 - 2.0 * DIVIDER, 100.0]);
    assert_eq!(h.rect(left).height, 300.0);
}

#[test]
fn dragging_a_divider_resizes_panes_within_their_minimums() {
    let mut h = Harness::new(600.0, 400.0);
    let (split, [left, middle, _]) = split(&mut h);
    let divider = Point::new(150.0 + DIVIDER / 2.0, 200.0);
    assert_eq!(h.tree.hit_test(divider), Some(split));
    h.down(divider);
    h.move_to(Point::new(200.0 + DIVIDER / 2.0, 200.0));
    assert_eq!(h.rect(left).width, 200.0);
    h.move_to(Point::new(10.0, 200.0));
    assert_eq!(h.rect(left).width, 100.0);
    h.move_to(Point::new(590.0, 200.0));
    assert_eq!(h.rect(middle).width, 120.0);
    h.up(Point::new(590.0, 200.0));
    let changes = values::<PanesChanged>(&h.messages());
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].0[1], 120.0);
}

#[test]
fn double_clicking_a_divider_collapses_and_restores_a_pane() {
    let mut h = Harness::new(600.0, 400.0);
    let (split, [left, middle, _]) = split(&mut h);
    let field = h.add_to(left, TextField::new("", 80.0));
    h.tree.set_focus(Some(field));
    let divider = Point::new(150.0 + DIVIDER / 2.0, 200.0);
    h.tree.set_now(1.0);
    h.click(divider);
    h.tree.set_now(1.1);
    h.click(divider);
    assert!(h.tree.get::<SplitView>(split).unwrap().is_collapsed(0));
    assert!(h.tree.is_hidden(left));
    assert_eq!(h.tree.focused(), None, "focus leaves the collapsed pane");
    assert_eq!(h.rect(middle).x, 0.0);
    assert_eq!(h.rect(middle).width, 600.0 - 100.0 - DIVIDER);
    let list = h.tree.display_list().clone();
    let r = h.rect(field);
    assert!(
        list.commands()
            .iter()
            .all(|c| !matches!(c, DrawCommand::Quad { rect, .. } if *rect == r)),
    );
    let divider = Point::new(h.rect(middle).right() + DIVIDER / 2.0, 200.0);
    h.tree.set_now(3.0);
    h.click(divider);
    h.tree.set_now(3.1);
    h.click(divider);
    assert_eq!(h.rect(left).width, 150.0);
    assert!(!h.tree.is_hidden(left));
}

#[test]
fn tab_views_show_only_the_selected_page() {
    let mut h = Harness::new(500.0, 400.0);
    let tabs = h.add(TabView::new(["General", "Advanced", "About"]).width(400.0));
    let pages: Vec<WidgetId> = (0..3)
        .map(|i| h.add_to(tabs, Label::new(format!("Page {i}"))))
        .collect();
    let shown = |h: &Harness| {
        pages
            .iter()
            .map(|&p| !h.tree.is_hidden(p))
            .collect::<Vec<_>>()
    };
    assert_eq!(shown(&h), [true, false, false]);
    // The tabs are centered at the top.
    let r = h.rect(tabs);
    h.click(Point::new(r.x + r.width / 2.0, r.y + 13.0));
    assert_eq!(shown(&h), [false, true, false]);
    assert_eq!(
        values::<SelectionChanged>(&h.messages()),
        [SelectionChanged(1)]
    );
    h.key(Key::ArrowRight);
    h.key(Key::ArrowRight);
    assert_eq!(shown(&h), [true, false, false]);
    let update = h.tree.accessibility_update(1.0);
    let exposed: Vec<bool> = pages
        .iter()
        .map(|&p| update.nodes.iter().any(|(n, _)| *n == access_id(p)))
        .collect();
    assert_eq!(exposed, [true, false, false]);
    let texts = h
        .tree
        .display_list()
        .commands()
        .iter()
        .filter(|c| matches!(c, DrawCommand::Text { .. }))
        .count();
    assert_eq!(texts, 4);
}

/// A 600-pixel-wide document tab view with a "+" button and three pages: each tab is
/// 188 pixels wide from x = 4.
fn document_tabs(h: &mut Harness) -> (WidgetId, Vec<WidgetId>) {
    let tabs = h.add(
        TabView::new(Vec::<String>::new())
            .documents()
            .new_tab_button(true)
            .width(600.0),
    );
    let pages = ["Notes.txt", "Todo.md", "Plan.md"]
        .iter()
        .map(|t| TabView::add_tab(&mut h.tree, tabs, *t, Label::new(format!("{t} text"))).unwrap())
        .collect();
    h.prepare();
    (tabs, pages)
}

fn tab_view(h: &Harness, id: WidgetId) -> &TabView {
    h.tree.get::<TabView>(id).unwrap()
}

fn doc_tab(h: &Harness, tabs: WidgetId, i: usize) -> Point {
    let r = h.rect(tabs);
    Point::new(
        r.x + 4.0 + 188.0 * i as f32 + 60.0,
        r.y + DOCUMENT_TAB_BAR / 2.0,
    )
}

fn doc_close(h: &Harness, tabs: WidgetId, i: usize) -> Point {
    let r = h.rect(tabs);
    Point::new(
        r.x + 4.0 + 188.0 * (i + 1) as f32 - 6.0 - 8.0,
        r.y + DOCUMENT_TAB_BAR / 2.0,
    )
}

fn messages(h: &mut Harness) -> Vec<Message> {
    h.tree.take_messages()
}

#[test]
fn document_tabs_open_select_and_fill_the_view_with_their_page() {
    let mut h = Harness::new(700.0, 400.0);
    let (tabs, pages) = document_tabs(&mut h);
    assert_eq!(tab_view(&h, tabs).selected(), 2);
    assert_eq!(
        tab_view(&h, tabs).titles(),
        ["Notes.txt", "Todo.md", "Plan.md"]
    );
    let shown: Vec<bool> = pages.iter().map(|&p| !h.tree.is_hidden(p)).collect();
    assert_eq!(shown, [false, false, true]);
    assert_eq!(h.rect(pages[2]).y, h.rect(tabs).y + DOCUMENT_TAB_BAR);
    assert_eq!(h.rect(pages[2]).x, h.rect(tabs).x);
    h.down(doc_tab(&h, tabs, 0));
    h.up(doc_tab(&h, tabs, 0));
    assert_eq!(tab_view(&h, tabs).selected(), 0);
    assert!(!h.tree.is_hidden(pages[0]));
    assert_eq!(
        values::<SelectionChanged>(&messages(&mut h)),
        [SelectionChanged(0)]
    );
    h.key(Key::End);
    assert_eq!(tab_view(&h, tabs).selected(), 2);
    h.key(Key::Home);
    h.key(Key::ArrowLeft);
    assert_eq!(tab_view(&h, tabs).selected(), 2);
}

#[test]
fn closing_a_document_tab_is_asked_for_and_done_by_the_app() {
    let mut h = Harness::new(700.0, 400.0);
    let (tabs, pages) = document_tabs(&mut h);
    h.click(doc_tab(&h, tabs, 1));
    messages(&mut h);
    h.click(doc_close(&h, tabs, 1));
    assert_eq!(
        values::<TabCloseRequested>(&messages(&mut h)),
        [TabCloseRequested(1)]
    );
    assert_eq!(tab_view(&h, tabs).tab_count(), 3);
    assert!(TabView::close_tab(&mut h.tree, &mut h.text, tabs, 1));
    h.prepare();
    assert!(!h.tree.contains(pages[1]));
    assert_eq!(tab_view(&h, tabs).titles(), ["Notes.txt", "Plan.md"]);
    assert_eq!(tab_view(&h, tabs).selected(), 1);
    assert!(!h.tree.is_hidden(pages[2]));
    assert_eq!(TabView::page(&h.tree, tabs, 1), Some(pages[2]));
    let r = doc_tab(&h, tabs, 0);
    let button = PointerButton::Middle;
    h.tree
        .dispatch(Event::PointerDown { pos: r, button }, &mut h.text);
    h.tree
        .dispatch(Event::PointerUp { pos: r, button }, &mut h.text);
    assert_eq!(
        values::<TabCloseRequested>(&messages(&mut h)),
        [TabCloseRequested(0)]
    );
    h.key(Key::Delete);
    assert_eq!(
        values::<TabCloseRequested>(&messages(&mut h)),
        [TabCloseRequested(1)]
    );
    assert!(TabView::close_tab(&mut h.tree, &mut h.text, tabs, 1));
    assert_eq!(tab_view(&h, tabs).selected(), 0);
    assert!(TabView::close_tab(&mut h.tree, &mut h.text, tabs, 0));
    assert!(!TabView::close_tab(&mut h.tree, &mut h.text, tabs, 0));
    h.prepare();
    assert_eq!(tab_view(&h, tabs).tab_count(), 0);
    assert!(h.tree.children(tabs).is_empty());
}

#[test]
fn the_new_tab_button_asks_for_a_tab() {
    let mut h = Harness::new(700.0, 400.0);
    let (tabs, _) = document_tabs(&mut h);
    let r = h.rect(tabs);
    let plus = Point::new(r.x + 4.0 + 3.0 * 188.0 + 14.0, r.y + DOCUMENT_TAB_BAR / 2.0);
    h.click(plus);
    assert_eq!(
        values::<NewTabRequested>(&messages(&mut h)),
        [NewTabRequested]
    );
    h.down(plus);
    h.up(Point::new(plus.x, plus.y + 100.0));
    assert!(values::<NewTabRequested>(&messages(&mut h)).is_empty());
}

#[test]
fn dragging_a_document_tab_moves_it_with_its_page() {
    let mut h = Harness::new(700.0, 400.0);
    let (tabs, pages) = document_tabs(&mut h);
    h.tree
        .update::<TabView, _>(tabs, |t, cx| t.set_modified(cx, 0, true));
    let from = doc_tab(&h, tabs, 0);
    h.down(from);
    h.move_to(Point::new(from.x + 2.0, from.y));
    assert_eq!(
        tab_view(&h, tabs).titles()[0],
        "Notes.txt",
        "not yet dragged"
    );
    h.move_to(doc_tab(&h, tabs, 1));
    h.move_to(doc_tab(&h, tabs, 2));
    h.up(doc_tab(&h, tabs, 2));
    let view = tab_view(&h, tabs);
    assert_eq!(view.titles(), ["Todo.md", "Plan.md", "Notes.txt"]);
    assert_eq!(view.selected(), 2, "the dragged tab stays selected");
    assert!(view.is_modified(2) && !view.is_modified(0));
    assert_eq!(TabView::page(&h.tree, tabs, 2), Some(pages[0]));
    assert!(!h.tree.is_hidden(pages[0]));
    let moved = values::<TabMoved>(&messages(&mut h));
    assert_eq!(moved, [TabMoved { from: 0, to: 2 }]);
    let alt_cmd = Modifiers {
        alt: true,
        meta: true,
        command: true,
        ..Modifiers::default()
    };
    h.tree.dispatch(
        Event::KeyDown {
            key: Key::ArrowLeft,
            modifiers: alt_cmd,
        },
        &mut h.text,
    );
    h.prepare();
    assert_eq!(
        tab_view(&h, tabs).titles(),
        ["Todo.md", "Notes.txt", "Plan.md"]
    );
    assert_eq!(
        values::<TabMoved>(&messages(&mut h)),
        [TabMoved { from: 2, to: 1 }]
    );
    assert!(TabView::close_tab(&mut h.tree, &mut h.text, tabs, 0));
    h.prepare();
    assert!(!h.tree.contains(pages[1]), "Todo.md's page");
    assert_eq!(TabView::page(&h.tree, tabs, 0), Some(pages[0]));
    assert_eq!(TabView::page(&h.tree, tabs, 1), Some(pages[2]));
}

#[test]
fn document_tabs_describe_unsaved_changes_and_clear_the_description_after_saving() {
    let mut h = Harness::new(700.0, 400.0);
    let (tabs, _) = document_tabs(&mut h);
    let node = |h: &Harness, index| {
        h.tree
            .accessibility_update(1.0)
            .nodes
            .into_iter()
            .find(|(id, _)| *id == ui_core::access_item_id(tabs, index))
            .unwrap()
            .1
    };
    let clean = node(&h, 0);
    assert_eq!(clean.label(), Some("Notes.txt"));
    assert_eq!(clean.description(), None);
    h.tree
        .update::<TabView, _>(tabs, |t, cx| t.set_modified(cx, 0, true));
    let modified = node(&h, 0);
    assert_eq!(modified.label(), clean.label());
    assert_eq!(modified.is_selected(), clean.is_selected());
    assert_eq!(modified.description(), Some("Unsaved changes"));
    assert_eq!(node(&h, 1).description(), None);
    h.tree
        .update::<TabView, _>(tabs, |t, cx| t.set_modified(cx, 0, false));
    assert_eq!(node(&h, 0), clean);
}

#[test]
fn the_focused_document_bar_can_request_a_new_tab_from_the_keyboard() {
    let mut h = Harness::new(700.0, 400.0);
    let (tabs, pages) = document_tabs(&mut h);
    let command = Modifiers {
        command: true,
        ..Default::default()
    };
    let press = |h: &mut Harness, modifiers| {
        h.tree.dispatch(
            Event::KeyDown {
                key: Key::Char('t'),
                modifiers,
            },
            &mut h.text,
        );
        values::<NewTabRequested>(&messages(h))
    };
    h.tree.set_focus(Some(tabs));
    assert_eq!(press(&mut h, command), [NewTabRequested]);
    assert!(press(&mut h, Modifiers::default()).is_empty());
    assert!(
        press(
            &mut h,
            Modifiers {
                shift: true,
                ..command
            }
        )
        .is_empty()
    );
    assert!(
        press(
            &mut h,
            Modifiers {
                alt: true,
                ..command
            }
        )
        .is_empty()
    );
    h.tree.set_focus(None);
    assert!(press(&mut h, command).is_empty());
    for _ in pages {
        TabView::close_tab(&mut h.tree, &mut h.text, tabs, 0);
    }
    h.prepare();
    h.tree.set_focus(Some(tabs));
    assert_eq!(press(&mut h, command), [NewTabRequested]);
    h.tree.remove(tabs, &mut h.text);
    let without_button = h.add(TabView::new(["Only"]).documents());
    h.tree.set_focus(Some(without_button));
    assert!(press(&mut h, command).is_empty());
}

#[test]
fn document_tabs_expose_their_close_and_new_buttons() {
    let mut h = Harness::new(700.0, 400.0);
    let (tabs, _) = document_tabs(&mut h);
    let update = h.tree.accessibility_update(1.0);
    let labels: Vec<String> = update
        .nodes
        .iter()
        .filter_map(|(_, n)| n.label().map(str::to_owned))
        .filter(|l| l.starts_with("Close ") || l == "New Tab")
        .collect();
    assert_eq!(
        labels,
        [
            "Close Notes.txt",
            "Close Todo.md",
            "Close Plan.md",
            "New Tab"
        ]
    );
    // Their actions are the items after the tabs: Close Todo.md, then New Tab.
    for item in [4, 6] {
        h.tree.accessibility_action(
            &accesskit::ActionRequest {
                action: accesskit::Action::Click,
                target_tree: accesskit::TreeId::ROOT,
                target_node: ui_core::access_item_id(tabs, item),
                data: None,
            },
            &mut h.text,
        );
    }
    let messages = messages(&mut h);
    assert_eq!(
        values::<TabCloseRequested>(&messages),
        [TabCloseRequested(1)]
    );
    assert_eq!(values::<NewTabRequested>(&messages), [NewTabRequested]);
}

#[test]
fn disclosure_groups_show_their_content_when_expanded() {
    let mut h = Harness::new(400.0, 300.0);
    let group = h.add(DisclosureGroup::new("Details"));
    let content = h.block(group, 100.0, 50.0);
    let collapsed = h.rect(group).height;
    assert!(h.tree.is_hidden(content));
    h.tree.set_now(2.0);
    h.click(Point::new(10.0, 10.0));
    assert!(!h.tree.is_hidden(content));
    assert!(h.rect(group).height >= collapsed + 50.0);
    assert_eq!(values::<Toggled>(&h.messages()), [Toggled(true)]);
    assert!(h.tree.wants_animation_frame());
    h.run(2.0, 0.5);
    assert!(!h.tree.wants_animation_frame());
    h.tree.set_focus(Some(group));
    h.key(Key::ArrowLeft);
    assert!(h.tree.is_hidden(content));
}

fn scroller(h: &mut Harness, style: ScrollerStyle, horizontal: bool) -> (WidgetId, WidgetId) {
    let scroll = h.add(
        ScrollView::new(Style {
            size: LayoutSize {
                width: length(200.0),
                height: length(100.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        })
        .horizontal(horizontal)
        .scroller_style(style),
    );
    let content = h.block(scroll, 800.0, 1000.0);
    (scroll, content)
}

/// The scroll range covers what can be seen: content a widget cuts off itself (a
/// clipping box, a breadcrumb that collapses its middle into "…") is not overflow to
/// make room for.
#[test]
fn a_scroll_range_leaves_out_content_cut_off_inside_it() {
    let mut h = Harness::new(400.0, 300.0);
    let scroll = h.add(
        ScrollView::new(Style {
            size: LayoutSize {
                width: length(200.0),
                height: length(100.0),
            },
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.0,
            ..Default::default()
        })
        .horizontal(true),
    );
    let width = |h: &Harness| h.tree.layout(scroll).unwrap().content_size.width;
    assert_eq!(width(&h), 200.0);

    let clip = h.add_to(
        scroll,
        Container::new(Style {
            size: LayoutSize {
                width: length(150.0),
                height: length(40.0),
            },
            ..Default::default()
        })
        .clip(true),
    );
    h.block(clip, 600.0, 20.0);
    assert_eq!(width(&h), 200.0, "a clipping box's children");

    let path = h.add_to(
        scroll,
        PathControl::new((0..12).map(|i| format!("Folder {i}"))).width(150.0),
    );
    assert!(
        !h.tree
            .get::<PathControl>(path)
            .unwrap()
            .collapsed()
            .is_empty()
    );
    assert_eq!(width(&h), 200.0, "a collapsed path");

    h.block(scroll, 600.0, 20.0);
    assert_eq!(width(&h), 600.0);
}

#[test]
fn shift_scrolls_sideways_and_scroll_bars_drag_and_page() {
    let mut h = Harness::new(400.0, 300.0);
    let (scroll, content) = scroller(&mut h, ScrollerStyle::Overlay, true);
    let position = |h: &Harness| h.tree.get::<ScrollView>(scroll).unwrap().scroll_position();
    h.tree.dispatch(
        Event::ModifiersChanged(Modifiers {
            shift: true,
            ..Default::default()
        }),
        &mut h.text,
    );
    h.wheel(Point::new(50.0, 50.0), Point::new(0.0, -30.0));
    assert_eq!(position(&h), Point::new(30.0, 0.0));
    h.tree
        .dispatch(Event::ModifiersChanged(Modifiers::default()), &mut h.text);

    let edge = Point::new(196.0, 5.0);
    assert_eq!(h.tree.hit_test(edge), Some(scroll));
    assert_eq!(h.tree.hit_test(Point::new(100.0, 50.0)), Some(content));
    h.click(Point::new(196.0, 80.0));
    assert_eq!(position(&h).y, 90.0);
    h.down(Point::new(196.0, 10.0 + 3.0));
    h.move_to(Point::new(196.0, 400.0));
    h.up(Point::new(196.0, 400.0));
    assert_eq!(position(&h).y, 900.0);
}

#[test]
fn legacy_scroll_bars_inset_the_content() {
    let mut h = Harness::new(400.0, 300.0);
    let (scroll, _) = scroller(&mut h, ScrollerStyle::Legacy, true);
    let update = h.tree.accessibility_update(1.0);
    let node = &update
        .nodes
        .iter()
        .find(|(n, _)| *n == access_id(scroll))
        .unwrap()
        .1;
    // Content is 800 x 1000 in a 186 x 86 area: the bars take 14 px.
    assert_eq!(node.scroll_y_max(), Some(1000.0 - 86.0));
    assert_eq!(node.scroll_x_max(), Some(800.0 - 186.0));
}

#[test]
fn past_an_edge_the_content_rubber_bands_and_springs_back() {
    let mut h = Harness::new(400.0, 300.0);
    let (scroll, _) = scroller(&mut h, ScrollerStyle::Overlay, false);
    let y = |h: &Harness| {
        h.tree
            .get::<ScrollView>(scroll)
            .unwrap()
            .scroll_position()
            .y
    };
    h.tree.set_now(1.0);
    h.wheel(Point::new(50.0, 50.0), Point::new(0.0, 100.0));
    let pulled = y(&h);
    assert!(pulled < 0.0 && pulled > -100.0, "{pulled}");
    h.run(1.0, 1.0);
    assert_eq!(y(&h), 0.0);

    let stiff = h.add(
        ScrollView::new(Style {
            size: LayoutSize {
                width: length(100.0),
                height: length(50.0),
            },
            ..Default::default()
        })
        .elastic(false),
    );
    h.block(stiff, 100.0, 500.0);
    let at = h.rect(stiff);
    h.wheel(Point::new(at.x + 10.0, at.y + 10.0), Point::new(0.0, 100.0));
    assert_eq!(h.tree.get::<ScrollView>(stiff).unwrap().offset(), 0.0);
}

#[test]
fn momentum_carries_the_scroll_on_after_the_wheel_stops() {
    let mut h = Harness::new(400.0, 300.0);
    let (scroll, _) = scroller(&mut h, ScrollerStyle::Overlay, false);
    let y = |h: &Harness| h.tree.get::<ScrollView>(scroll).unwrap().offset();
    // A quick flick: 30 px per 1/60 s.
    for i in 0..3 {
        h.tree.set_now(1.0 + f64::from(i) / 60.0);
        h.wheel(Point::new(50.0, 50.0), Point::new(0.0, -30.0));
    }
    assert_eq!(y(&h), 90.0);
    h.run(1.0 + 2.0 / 60.0, 0.5);
    let glided = y(&h);
    assert!(glided > 150.0, "{glided}");
    h.run(1.6, 3.0);
    let stopped = y(&h);
    assert!(stopped >= glided);
    assert!(
        !h.tree.wants_animation_frame(),
        "the glide and the fade end"
    );

    let still = h.add(
        ScrollView::new(Style {
            size: LayoutSize {
                width: length(100.0),
                height: length(50.0),
            },
            ..Default::default()
        })
        .momentum(false),
    );
    h.block(still, 100.0, 1000.0);
    let at = h.rect(still);
    for i in 0..3 {
        h.tree.set_now(10.0 + f64::from(i) / 60.0);
        h.wheel(Point::new(at.x + 10.0, at.y + 10.0), Point::new(0.0, -30.0));
    }
    h.run(10.0 + 2.0 / 60.0, 1.0);
    assert_eq!(h.tree.get::<ScrollView>(still).unwrap().offset(), 90.0);
}

#[test]
fn visual_effect_views_draw_a_backdrop_below_their_children() {
    let mut h = Harness::new(400.0, 300.0);
    let effect = h.add(
        VisualEffectView::new(
            Style {
                size: LayoutSize {
                    width: length(120.0),
                    height: length(80.0),
                },
                ..Default::default()
            },
            Material::Popover,
        )
        .radius(10.0),
    );
    h.add_to(effect, Label::new("On glass"));
    let list = h.tree.display_list().clone();
    let commands = list.commands();
    let backdrop = commands
        .iter()
        .position(|c| matches!(c, DrawCommand::Backdrop { rect, radius, .. } if *rect == Rect::new(0.0, 0.0, 120.0, 80.0) && *radius == 10.0))
        .expect("a backdrop");
    let label = commands
        .iter()
        .position(|c| matches!(c, DrawCommand::Text { .. }))
        .unwrap();
    assert!(backdrop < label, "the children draw on the material");
}

#[test]
fn keyboard_focus_clears_legacy_scroll_bars_and_moves_the_thumb() {
    let mut h = Harness::new(400.0, 300.0);
    let scroll = h.add(
        ScrollView::new(Style {
            size: LayoutSize {
                width: length(200.0),
                height: length(100.0),
            },
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.0,
            ..Default::default()
        })
        .horizontal(true)
        .scroller_style(ScrollerStyle::Legacy),
    );
    let fields: Vec<WidgetId> = (0..6)
        .map(|_| h.add_to(scroll, TextField::new("", 150.0)))
        .collect();
    h.block(scroll, 400.0, 20.0);
    let view = h.tree.window_rect(scroll).unwrap();
    // The vertical thumb: the small quad in the right-hand track.
    let thumb_y = |h: &Harness| {
        h.tree
            .display_list()
            .commands()
            .iter()
            .find_map(|c| match c {
                DrawCommand::Quad { rect, .. }
                    if rect.x >= view.right() - LEGACY_TRACK
                        && rect.width < LEGACY_TRACK
                        && rect.height < view.height - 2.0 * LEGACY_TRACK =>
                {
                    Some(rect.y)
                }
                _ => None,
            })
            .expect("a vertical thumb")
    };
    let top = thumb_y(&h);
    for &field in &fields {
        h.key(Key::Tab);
        assert_eq!(h.tree.focused(), Some(field));
        let r = h.tree.window_rect(field).unwrap();
        assert!(
            r.y >= view.y && r.bottom() <= view.bottom() - LEGACY_TRACK,
            "{r:?} under the track"
        );
    }
    assert!(thumb_y(&h) > top, "the thumb stayed at the top");
}

/// Keyboard focus that moves out of sight scrolls into view (WCAG 2.4.11); focus
/// taken with the pointer leaves the position alone.
#[test]
fn keyboard_focus_scrolls_into_view() {
    let mut h = Harness::new(400.0, 300.0);
    let scroll = h.add(
        ScrollView::new(Style {
            size: LayoutSize {
                width: length(200.0),
                height: length(100.0),
            },
            flex_direction: FlexDirection::Column,
            flex_shrink: 0.0,
            ..Default::default()
        })
        .horizontal(true),
    );
    let fields: Vec<WidgetId> = (0..10)
        .map(|_| h.add_to(scroll, TextField::new("", 150.0)))
        .collect();
    let row = h.add_to(scroll, Container::new(Style::default()));
    h.block(row, 400.0, 20.0);
    let right = h.add_to(row, TextField::new("", 100.0));
    let view = h.tree.window_rect(scroll).unwrap();
    let shown = |h: &Harness, id: WidgetId| {
        let r = h.tree.window_rect(id).unwrap();
        r.x >= view.x && r.y >= view.y && r.right() <= view.right() && r.bottom() <= view.bottom()
    };
    let tab = |h: &mut Harness, shift: bool| {
        h.tree.dispatch(
            Event::KeyDown {
                key: Key::Tab,
                modifiers: Modifiers {
                    shift,
                    ..Default::default()
                },
            },
            &mut h.text,
        );
        h.prepare();
    };
    let position = |h: &Harness| h.tree.get::<ScrollView>(scroll).unwrap().scroll_position();

    for &field in &fields {
        tab(&mut h, false);
        assert_eq!(h.tree.focused(), Some(field));
        assert!(shown(&h, field), "{field:?} out of view");
    }
    assert!(position(&h).y > 0.0);
    tab(&mut h, false);
    assert_eq!(h.tree.focused(), Some(right));
    assert!(shown(&h, right), "out of view sideways");
    assert!(position(&h).x > 0.0);
    for &field in fields.iter().rev() {
        tab(&mut h, true);
        assert!(shown(&h, field), "{field:?} out of view going back");
    }
    assert_eq!(position(&h), Point::new(0.0, 0.0));

    h.tree
        .update::<ScrollView, _>(scroll, |s, cx| s.set_offset(cx, 20.0));
    h.prepare();
    let fourth = h.tree.window_rect(fields[3]).unwrap();
    assert!(fourth.y < view.bottom() && fourth.bottom() > view.bottom());
    // Above the horizontal scroll bar.
    h.click(Point::new(fourth.x + 10.0, fourth.y + 6.0));
    assert_eq!(h.tree.focused(), Some(fields[3]));
    assert_eq!(position(&h), Point::new(0.0, 20.0));
}
