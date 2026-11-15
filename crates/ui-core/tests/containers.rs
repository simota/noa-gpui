use std::borrow::Cow;

use noa_gpui_core::style::{
    AlignItems, FlexDirection, Position, Size as LayoutSize, auto, length, percent,
};
use noa_gpui_core::widgets::{
    Accordion, BoxResized, Container, DisclosureGroup, GroupBox, GroupBoxStyle, OutlineItem,
    OutlineView, ResizableBox, ResizeEdges, Separator, Sheet, SheetClosed, TABLE_HEADER, TABLE_ROW,
    TableColumn, TableData, TableView, Toggled,
};
use noa_gpui_core::{
    Edge, Event, Key, Message, Modifiers, PointerButton, Style, UiTree, Widget, WidgetId,
};
use noa_gpui_scene::{Point, Rect, Size};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(800.0, 500.0);

struct Harness {
    tree: UiTree,
    text: TextStore,
    root: WidgetId,
    viewport: Size,
}

impl Harness {
    fn new() -> Self {
        let mut tree = UiTree::new();
        let root = tree.insert(
            None,
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::START,
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            }),
        );
        let mut h = Self {
            tree,
            text: TextStore::new(FontOptions {
                locale: "en-US".into(),
                load_system_fonts: false,
            }),
            root,
            viewport: VIEWPORT,
        };
        h.prepare();
        h
    }

    fn add(&mut self, widget: impl Widget) -> WidgetId {
        let root = self.root;
        self.add_to(root, widget)
    }

    fn add_to(&mut self, parent: WidgetId, widget: impl Widget) -> WidgetId {
        let id = self.tree.insert(Some(parent), widget);
        self.prepare();
        id
    }

    fn sized(width: f32, height: f32) -> Container {
        Container::new(Style {
            size: LayoutSize {
                width: length(width),
                height: length(height),
            },
            flex_shrink: 0.0,
            ..Default::default()
        })
    }

    fn prepare(&mut self) {
        self.tree.prepare(self.viewport, &mut self.text);
    }

    fn frame(&mut self, now: f64) {
        self.tree.animate(now, &mut self.text);
        self.prepare();
    }

    fn dispatch(&mut self, event: Event) {
        self.tree.dispatch(event, &mut self.text);
        self.prepare();
    }

    fn rect(&self, id: WidgetId) -> Rect {
        self.tree.window_rect(id).unwrap()
    }

    fn at(&self, id: WidgetId, x: f32, y: f32) -> Point {
        let r = self.rect(id);
        Point::new(r.x + x, r.y + y)
    }

    fn click(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.dispatch(Event::PointerDown { pos, button });
        self.dispatch(Event::PointerUp { pos, button });
    }

    fn drag(&mut self, from: Point, to: Point) {
        let button = PointerButton::Primary;
        self.dispatch(Event::PointerDown { pos: from, button });
        self.dispatch(Event::PointerMove { pos: to });
        self.dispatch(Event::PointerUp { pos: to, button });
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.dispatch(Event::KeyDown { key, modifiers });
    }

    fn wheel(&mut self, pos: Point, dy: f32) {
        self.dispatch(Event::Wheel {
            pos,
            delta: Point::new(0.0, dy),
        });
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
fn a_card_is_a_roomier_group_box_with_its_title_inside() {
    let mut h = Harness::new();
    let plain = h.add(GroupBox::new("").width(200.0));
    let plain_child = h.add_to(plain, Harness::sized(10.0, 10.0));
    let card = h.add(GroupBox::card("").width(200.0));
    let card_child = h.add_to(card, Harness::sized(10.0, 10.0));
    assert_eq!(
        h.tree.get::<GroupBox>(card).unwrap().box_style(),
        GroupBoxStyle::Card
    );
    let inset = |h: &Harness, parent, child| {
        let (p, c) = (h.rect(parent), h.rect(child));
        (c.x - p.x, c.y - p.y)
    };
    assert_eq!(inset(&h, plain, plain_child), (12.0, 12.0));
    assert_eq!(inset(&h, card, card_child), (18.0, 18.0));
    assert_eq!(h.rect(card).width, 200.0);

    let titled = h.add(GroupBox::card("Account").width(200.0));
    let child = h.add_to(titled, Harness::sized(10.0, 10.0));
    let (_, top) = inset(&h, titled, child);
    assert!(top > 30.0, "{top}");
}

fn header(h: &Harness, group: WidgetId) -> Point {
    h.at(group, 30.0, 12.0)
}

fn pump(h: &mut Harness, accordion: &mut Accordion) -> Vec<Message> {
    let messages = h.messages();
    for m in &messages {
        accordion.handle(&mut h.tree, m);
    }
    h.prepare();
    messages
}

fn expanded(h: &Harness, accordion: &Accordion) -> Vec<bool> {
    accordion
        .groups()
        .iter()
        .map(|&g| h.tree.get::<DisclosureGroup>(g).unwrap().is_expanded())
        .collect()
}

#[test]
fn an_exclusive_accordion_keeps_one_group_open() {
    let mut h = Harness::new();
    let root = h.root;
    let mut accordion = Accordion::new(&mut h.tree, root, true);
    let first = accordion.add(&mut h.tree, DisclosureGroup::new("General").expanded(true));
    let second = accordion.add(&mut h.tree, DisclosureGroup::new("Privacy").expanded(true));
    let third = accordion.add(&mut h.tree, DisclosureGroup::new("Advanced"));
    for &g in &[first, second, third] {
        h.tree.insert(Some(g), Harness::sized(100.0, 40.0));
    }
    h.prepare();
    assert_eq!(expanded(&h, &accordion), [true, false, false]);
    assert_eq!(accordion.open_group(&h.tree), Some(first));

    let at = header(&h, third);
    h.click(at);
    let messages = pump(&mut h, &mut accordion);
    assert_eq!(values::<Toggled>(&messages), [Toggled(true)]);
    assert_eq!(expanded(&h, &accordion), [false, false, true]);

    let at = header(&h, third);
    h.click(at);
    pump(&mut h, &mut accordion);
    assert_eq!(expanded(&h, &accordion), [false, false, false]);

    accordion.set_expanded(&mut h.tree, second, true);
    accordion.set_expanded(&mut h.tree, first, true);
    h.prepare();
    assert_eq!(expanded(&h, &accordion), [true, false, false]);

    let other = h.add(DisclosureGroup::new("Other"));
    let at = header(&h, other);
    h.click(at);
    let messages = h.messages();
    assert!(!messages.iter().any(|m| accordion.handle(&mut h.tree, m)));
}

#[test]
fn a_non_exclusive_accordion_opens_any_number() {
    let mut h = Harness::new();
    let root = h.root;
    let mut accordion = Accordion::new(&mut h.tree, root, false);
    let a = accordion.add(&mut h.tree, DisclosureGroup::new("A").expanded(true));
    let b = accordion.add(&mut h.tree, DisclosureGroup::new("B").expanded(true));
    h.prepare();
    assert_eq!(expanded(&h, &accordion), [true, true]);
    assert!(h.rect(b).y > h.rect(a).bottom(), "stacked in a column");

    accordion.set_exclusive(&mut h.tree, true);
    h.prepare();
    assert_eq!(expanded(&h, &accordion), [true, false]);
}

#[test]
fn a_drawer_runs_along_its_edge_and_follows_the_window() {
    let mut h = Harness::new();
    let drawer = Sheet::drawer(Edge::Right, 240.0).open(&mut h.tree, 30.0);
    h.prepare();
    assert_eq!(h.rect(drawer), Rect::new(560.0, 30.0, 240.0, 470.0));
    assert_eq!(
        h.tree.get::<Sheet>(drawer).unwrap().drawer_edge(),
        Some(Edge::Right)
    );

    h.viewport = Size::new(1000.0, 600.0);
    h.prepare();
    assert_eq!(h.rect(drawer), Rect::new(760.0, 30.0, 240.0, 570.0));

    let bottom = Sheet::drawer(Edge::Bottom, 120.0).open(&mut h.tree, 30.0);
    h.prepare();
    assert_eq!(h.rect(bottom), Rect::new(0.0, 480.0, 1000.0, 120.0));
}

#[test]
fn a_drawer_slides_in_from_its_edge_and_out_again() {
    let mut h = Harness::new();
    let drawer = Sheet::drawer(Edge::Left, 200.0).open(&mut h.tree, 0.0);
    let content = h.add_to(drawer, Harness::sized(50.0, 20.0));
    h.frame(0.0);
    assert!(h.rect(content).x < 0.0, "{:?}", h.rect(content));
    h.frame(1.0);
    assert_eq!(h.rect(content).x, 20.0, "in place, inside the padding");

    Sheet::close(&mut h.tree, drawer);
    h.frame(2.0);
    h.frame(3.0);
    let closed = h
        .messages()
        .iter()
        .any(|m| m.source == drawer && m.downcast_ref::<SheetClosed>().is_some());
    assert!(closed);
    assert!(h.tree.overlays().next().is_none());
}

#[test]
fn containers_take_their_layout_from_builder_methods() {
    let mut h = Harness::new();
    let column = h.add(
        Container::column()
            .width(120.0)
            .padding(4.0)
            .gap(6.0)
            .align_items(AlignItems::END),
    );
    let a = h.add_to(column, Harness::sized(30.0, 20.0));
    let b = h.add_to(column, Harness::sized(50.0, 20.0));
    let c = h.rect(column);
    assert_eq!(c.width, 120.0);
    assert_eq!(c.height, 4.0 + 20.0 + 6.0 + 20.0 + 4.0);
    assert_eq!(h.rect(a).right(), c.right() - 4.0);
    assert_eq!(h.rect(b).right(), c.right() - 4.0);
    assert_eq!(h.rect(b).y - h.rect(a).bottom(), 6.0);

    let row = h.add(
        Container::row()
            .width(300.0)
            .height(40.0)
            .justify_content(noa_gpui_core::style::JustifyContent::SPACE_BETWEEN),
    );
    let left = h.add_to(row, Harness::sized(50.0, 20.0));
    let right = h.add_to(row, Harness::sized(50.0, 20.0));
    let r = h.rect(row);
    assert_eq!(r.height, 40.0);
    assert_eq!(h.rect(left).x, r.x);
    assert_eq!(h.rect(right).right(), r.right());
    let filled = h.add(Container::row().width(300.0));
    h.add_to(filled, Harness::sized(50.0, 20.0));
    let rest = h.add_to(filled, Container::row().grow(1.0));
    assert_eq!(h.rect(rest).width, 250.0);
}

#[test]
fn a_wrapping_row_moves_what_does_not_fit_to_the_next_line() {
    let mut h = Harness::new();
    let row = h.add(Container::wrapping_row(10.0, 6.0));
    let boxes: Vec<WidgetId> = (0..3)
        .map(|_| h.add_to(row, Harness::sized(300.0, 40.0)))
        .collect();
    let r: Vec<Rect> = boxes.iter().map(|&b| h.rect(b)).collect();
    assert_eq!(r[1].x - r[0].right(), 10.0);
    assert_eq!(r[1].y, r[0].y);
    assert_eq!(r[2].x, r[0].x, "third box wraps");
    assert_eq!(r[2].y - r[0].bottom(), 6.0);

    let one_line = h.add(Container::wrapping_row(10.0, 6.0).wrap(false));
    let a = h.add_to(one_line, Harness::sized(300.0, 40.0));
    let b = h.add_to(one_line, Harness::sized(300.0, 40.0));
    let c = h.add_to(one_line, Harness::sized(300.0, 40.0));
    assert_eq!(h.rect(a).y, h.rect(c).y);
    assert_eq!(h.rect(b).y, h.rect(c).y);
}

#[test]
fn a_wrapping_row_with_a_vertical_separator_keeps_its_lines_inside() {
    let mut h = Harness::new();
    let row = h.add(Container::new(Style {
        flex_direction: FlexDirection::Row,
        flex_wrap: noa_gpui_core::style::FlexWrap::Wrap,
        align_items: AlignItems::CENTER,
        gap: LayoutSize {
            width: length(16.0),
            height: length(8.0),
        },
        size: LayoutSize {
            width: length(700.0),
            height: auto(),
        },
        ..Default::default()
    }));
    h.add_to(row, Harness::sized(300.0, 30.0));
    let divider = h.add_to(row, Separator::vertical());
    let boxes: Vec<WidgetId> = (0..3)
        .map(|_| h.add_to(row, Harness::sized(300.0, 20.0)))
        .collect();
    let below = h.add(Harness::sized(100.0, 10.0));

    let row_rect = h.rect(row);
    assert_eq!(h.rect(divider).height, 30.0, "as tall as its line");
    for &b in &boxes {
        assert!(
            h.rect(b).bottom() <= row_rect.bottom(),
            "{:?} in {row_rect:?}",
            h.rect(b)
        );
    }
    assert_eq!(
        h.rect(below).y,
        row_rect.bottom(),
        "the next item is not overlapped"
    );
}

#[test]
fn absolute_children_cover_their_parent_not_the_window() {
    let mut h = Harness::new();
    let row = h.add(Container::wrapping_row(4.0, 0.0).wrap(false));
    h.add_to(row, Harness::sized(50.0, 20.0));
    let item = h.add_to(row, Container::new(Style::default()));
    h.add_to(item, Harness::sized(60.0, 30.0));
    let frame = h.add_to(
        item,
        Container::new(Style {
            position: Position::Absolute,
            size: LayoutSize {
                width: percent(1.0),
                height: percent(1.0),
            },
            ..Default::default()
        }),
    );
    assert_eq!(h.rect(item), Rect::new(54.0, 0.0, 60.0, 30.0));
    assert_eq!(h.rect(frame), h.rect(item));
}

#[test]
fn an_aspect_ratio_sets_the_missing_side() {
    let mut h = Harness::new();
    let video = h.add(
        Container::new(Style {
            size: LayoutSize {
                width: length(320.0),
                height: auto(),
            },
            ..Default::default()
        })
        .aspect_ratio(16.0 / 9.0),
    );
    assert_eq!(h.rect(video).height, 180.0);
    let tall = h.add(
        Container::new(Style {
            size: LayoutSize {
                width: auto(),
                height: length(100.0),
            },
            ..Default::default()
        })
        .aspect_ratio(0.5),
    );
    assert_eq!(h.rect(tall).width, 50.0);
}

#[test]
fn a_resizable_box_grows_from_its_corner_within_its_limits() {
    let mut h = Harness::new();
    let b = h.add(
        ResizableBox::new(Size::new(200.0, 120.0))
            .min_size(Size::new(100.0, 80.0))
            .max_size(Size::new(300.0, 300.0)),
    );
    let content = h.add_to(b, Harness::sized(50.0, 50.0));
    let corner = h.at(b, 196.0, 116.0);
    h.drag(corner, Point::new(corner.x + 40.0, corner.y + 20.0));
    assert_eq!(h.rect(b).width, 240.0);
    assert_eq!(h.rect(b).height, 140.0);
    assert_eq!(
        values::<BoxResized>(&h.messages()),
        [BoxResized(Size::new(240.0, 140.0))]
    );

    let corner = h.at(b, 236.0, 136.0);
    h.drag(corner, Point::new(corner.x - 500.0, corner.y + 500.0));
    assert_eq!(
        h.tree.get::<ResizableBox>(b).unwrap().size(),
        Size::new(100.0, 300.0)
    );

    let edge = h.at(b, 98.0, 40.0);
    h.drag(edge, Point::new(edge.x + 30.0, edge.y + 30.0));
    assert_eq!(h.rect(b).width, 130.0);
    assert_eq!(h.rect(b).height, 300.0);

    h.messages();
    let inside = h.at(content, 10.0, 10.0);
    h.drag(inside, Point::new(inside.x + 30.0, inside.y));
    assert_eq!(h.rect(b).width, 130.0);
    assert!(values::<BoxResized>(&h.messages()).is_empty());
}

#[test]
fn a_resizable_box_takes_only_the_edges_it_is_given() {
    let mut h = Harness::new();
    let b = h.add(
        ResizableBox::new(Size::new(200.0, 120.0)).edges(ResizeEdges {
            bottom: true,
            ..ResizeEdges::NONE
        }),
    );
    let right = h.at(b, 198.0, 60.0);
    h.drag(right, Point::new(right.x + 40.0, right.y));
    assert_eq!(h.rect(b).width, 200.0);
    let bottom = h.at(b, 100.0, 118.0);
    h.drag(bottom, Point::new(bottom.x, bottom.y + 40.0));
    assert_eq!(h.rect(b).height, 160.0);
}

/// Three groups of rows: group rows at 0, 11 and 22, ten rows after each.
struct Grouped;

impl TableData for Grouped {
    fn row_count(&self) -> usize {
        33
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        if self.is_group_row(row) {
            return if column == 0 {
                Cow::Owned(format!("Group {}", row / 11))
            } else {
                Cow::Borrowed("")
            };
        }
        Cow::Owned(match column {
            0 => format!("Row {row}"),
            _ => format!("{}", (row * 37) % 100),
        })
    }

    fn is_group_row(&self, row: usize) -> bool {
        row.is_multiple_of(11)
    }
}

fn grouped_table(h: &mut Harness) -> WidgetId {
    let columns = vec![
        TableColumn::new("Name", 200.0),
        TableColumn::new("Size", 100.0).sort(noa_gpui_core::widgets::ColumnSort::Number),
    ];
    // Ten rows tall below the header.
    let height = TABLE_HEADER + 10.0 * TABLE_ROW + 1.0;
    h.add(TableView::new(columns, Grouped).width(400.0).height(height))
}

fn table(h: &Harness, id: WidgetId) -> &TableView {
    h.tree.get::<TableView>(id).unwrap()
}

#[test]
fn the_group_being_scrolled_through_stays_at_the_top_of_a_table() {
    let mut h = Harness::new();
    let t = grouped_table(&mut h);
    assert!(table(&h, t).is_group_row(11));
    assert_eq!(table(&h, t).floating_group_row(), None, "group 0 in place");

    let middle = h.at(t, 100.0, 100.0);
    h.wheel(middle, -3.0 * TABLE_ROW);
    assert_eq!(table(&h, t).floating_group_row(), Some(0));
    h.wheel(middle, -10.0 * TABLE_ROW);
    // Row 11 (the next group) has reached the top: its own group floats now.
    let top = (table(&h, t).scroll_offset() / TABLE_ROW) as usize;
    assert!(top >= 11, "{top}");
    assert_eq!(table(&h, t).floating_group_row(), Some(11));

    let pinned = h.at(t, 50.0, TABLE_HEADER + TABLE_ROW / 2.0);
    h.click(pinned);
    assert_eq!(table(&h, t).selected_count(), 0);

    let mut h = Harness::new();
    let columns = vec![TableColumn::new("Name", 200.0)];
    let t = h.add(
        TableView::new(columns, Grouped)
            .height(TABLE_HEADER + 10.0 * TABLE_ROW + 1.0)
            .floats_group_rows(false),
    );
    let middle = h.at(t, 100.0, 100.0);
    h.wheel(middle, -3.0 * TABLE_ROW);
    assert_eq!(table(&h, t).floating_group_row(), None);
}

#[test]
fn group_rows_are_not_selected_and_keys_pass_over_them() {
    let mut h = Harness::new();
    let t = grouped_table(&mut h);
    let group = h.at(t, 50.0, TABLE_HEADER + TABLE_ROW / 2.0);
    h.click(group);
    assert_eq!(table(&h, t).selected_count(), 0);

    // Row 10 is the last of group 0; Down goes on to row 12, past group row 11.
    let row9 = h.at(t, 50.0, TABLE_HEADER + 9.5 * TABLE_ROW);
    h.click(row9);
    h.key_with(Key::ArrowDown, Modifiers::default());
    assert_eq!(table(&h, t).selected_rows(), [10]);
    h.key_with(Key::ArrowDown, Modifiers::default());
    assert_eq!(table(&h, t).selected_rows(), [12]);
    h.key_with(Key::ArrowUp, Modifiers::default());
    assert_eq!(table(&h, t).selected_rows(), [10]);
    h.key_with(Key::Home, Modifiers::default());
    assert_eq!(table(&h, t).selected_rows(), [1]);

    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    for _ in 0..12 {
        h.key_with(Key::ArrowDown, shift);
    }
    let selected = table(&h, t).selected_rows();
    assert!(
        selected.contains(&13) && !selected.contains(&11),
        "{selected:?}"
    );
    let command = Modifiers {
        command: true,
        meta: true,
        ..Default::default()
    };
    h.key_with(Key::Char('a'), command);
    assert_eq!(table(&h, t).selected_count(), 30);
    assert!(!table(&h, t).is_selected(0));
}

#[test]
fn sorting_keeps_rows_inside_their_groups() {
    let mut h = Harness::new();
    let t = grouped_table(&mut h);
    let size_header = h.at(t, 250.0, TABLE_HEADER / 2.0);
    h.click(size_header);
    let order = table(&h, t).displayed_rows().to_vec();
    for g in [0, 11, 22] {
        assert_eq!(order[g], g, "group row {g} stays");
        let mut group: Vec<usize> = order[g + 1..g + 11].to_vec();
        let sizes: Vec<usize> = group.iter().map(|r| (r * 37) % 100).collect();
        assert!(sizes.windows(2).all(|w| w[0] <= w[1]), "{sizes:?}");
        group.sort_unstable();
        assert_eq!(group, (g + 1..g + 11).collect::<Vec<_>>());
    }
}

#[test]
fn a_sidebar_pins_the_heading_of_the_section_being_scrolled_through() {
    let mut h = Harness::new();
    let section = |name: &str| {
        OutlineItem::new(name).children((0..8).map(|i| OutlineItem::new(format!("{name} {i}"))))
    };
    let o = h.add(
        OutlineView::new(vec![section("Favorites"), section("Tags")])
            .sidebar()
            .height(28.0 * 5.0),
    );
    let get = |h: &Harness| {
        h.tree
            .get::<OutlineView>(o)
            .unwrap()
            .floating_heading_item()
    };
    assert_eq!(get(&h), None);
    let middle = h.at(o, 50.0, 50.0);
    h.wheel(middle, -2.0 * 28.0);
    assert_eq!(get(&h), Some(0), "Favorites");
    // Tags (item 9, shown at position 9) has reached the top.
    h.wheel(middle, -7.5 * 28.0);
    assert_eq!(get(&h), Some(9));

    let pinned = h.at(o, 50.0, 14.0);
    h.click(pinned);
    assert!(!h.tree.get::<OutlineView>(o).unwrap().is_expanded(9));
}

#[test]
fn rows_revealed_by_the_keyboard_stay_clear_of_the_pinned_group_row() {
    let mut h = Harness::new();
    let t = grouped_table(&mut h);
    // Select row 21, the last of group 11, and walk up through the group.
    let middle = h.at(t, 100.0, 100.0);
    h.wheel(middle, -12.0 * TABLE_ROW);
    let top = (table(&h, t).scroll_offset() / TABLE_ROW) as usize;
    let row = |h: &Harness, pos: usize| {
        h.at(t, 50.0, TABLE_HEADER + (pos - top) as f32 * TABLE_ROW + 2.0)
    };
    let last = row(&h, 21);
    h.click(last);
    for _ in 0..9 {
        h.key_with(Key::ArrowUp, Modifiers::default());
        let lead = table(&h, t).lead_row().unwrap();
        let pos = table(&h, t)
            .displayed_rows()
            .iter()
            .position(|&r| r == lead)
            .unwrap();
        let y = pos as f32 * TABLE_ROW - table(&h, t).scroll_offset();
        if table(&h, t).floating_group_row().is_some() {
            assert!(
                y >= TABLE_ROW,
                "row {lead} at {y} is under the pinned group row"
            );
        }
    }
    assert_eq!(table(&h, t).selected_rows(), [12]);
}
