use std::borrow::Cow;

use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, percent};
use noa_gpui_core::widgets::{
    Aggregate, BrowserPathChanged, BrowserView, CellActionClicked, CellButton, CellButtonClicked,
    CellLook, CollectionItem, CollectionView, ColumnResized, ColumnSort, ColumnVisibilityChanged,
    Container, GroupActionClicked, GroupRowClicked, Grouping, ItemExpanded, ItemsMoved, Menu,
    OutlineItem, OutlineView, RemoteRows, RowActivated, RowSelectionChanged, RowSeparator,
    RowsMoved, Rule, RuleEditor, RuleField, RuleMatch, RuleOperator, RulesChanged, SelectionMode,
    SortChanged, TABLE_CHECK_COLUMN, TABLE_HEADER, TABLE_ROW, TableColumn, TableData, TableText,
    TableView, TextView, Tone, Truncation,
};
use noa_gpui_core::{
    Event, Key, Message, Modifiers, PointerButton, Style, Theme, UiTree, Widget, WidgetId,
};
use noa_gpui_scene::{Color, DrawCommand, Point, Rect, Size, VectorIcon};
use noa_gpui_text::cosmic_text::Family;
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(800.0, 600.0);
/// A table this tall shows exactly ten rows.
const TEN_ROWS: f32 = TABLE_HEADER + 10.0 * TABLE_ROW + 1.0;

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
                align_items: noa_gpui_core::style::AlignItems::START,
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

    fn at(&self, id: WidgetId, x: f32, y: f32) -> Point {
        let r = self.rect(id);
        Point::new(r.x + x, r.y + y)
    }

    fn modifiers(&mut self, modifiers: Modifiers) {
        self.tree
            .dispatch(Event::ModifiersChanged(modifiers), &mut self.text);
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

    fn click_with(&mut self, pos: Point, modifiers: Modifiers) {
        self.modifiers(modifiers);
        self.click(pos);
        self.modifiers(Modifiers::default());
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.tree
            .dispatch(Event::KeyDown { key, modifiers }, &mut self.text);
        self.modifiers(Modifiers::default());
        self.prepare();
    }

    fn type_text(&mut self, text: &str) {
        self.tree.dispatch(Event::Text(text.into()), &mut self.text);
        self.prepare();
    }

    fn wheel(&mut self, pos: Point, delta: Point) {
        self.tree
            .dispatch(Event::Wheel { pos, delta }, &mut self.text);
        self.prepare();
    }

    fn layouts(&self) -> usize {
        self.text.layouts().len()
    }

    fn messages(&mut self) -> Vec<Message> {
        self.tree.take_messages()
    }

    fn open_menu(&self) -> Option<WidgetId> {
        self.tree
            .overlays()
            .find(|&o| self.tree.get::<Menu>(o).is_some())
    }
}

fn values<T: 'static + Clone>(messages: &[Message]) -> Vec<T> {
    messages
        .iter()
        .filter_map(|m| m.downcast_ref::<T>().cloned())
        .collect()
}

const CMD: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
    meta: true,
    command: true,
    command_is_meta: true,
};
const SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: false,
    alt: false,
    meta: false,
    command: false,
    command_is_meta: false,
};
const ALT: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: true,
    meta: false,
    command: false,
    command_is_meta: false,
};

/// Rows computed on demand: "Item n", a number that is not in row order, and a
/// category. Counts how many cells were asked for.
struct Generated {
    rows: usize,
    asked: std::rc::Rc<std::cell::Cell<usize>>,
}

impl Generated {
    fn new(rows: usize) -> Self {
        Self {
            rows,
            asked: Default::default(),
        }
    }
}

impl TableData for Generated {
    fn row_count(&self) -> usize {
        self.rows
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        self.asked.set(self.asked.get() + 1);
        Cow::Owned(match column {
            0 => format!("Item {row}"),
            1 => format!("{}", (row * 7919) % 1000),
            _ => ["red", "Green", "blue"][row % 3].to_owned(),
        })
    }
}

fn columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("Name", 160.0).sort(ColumnSort::Text),
        TableColumn::new("Size", 100.0).sort(ColumnSort::Number),
        TableColumn::new("Kind", 120.0),
    ]
}

fn table(h: &mut Harness, rows: usize) -> WidgetId {
    h.add(
        TableView::new(columns(), Generated::new(rows))
            .width(400.0)
            .height(TEN_ROWS),
    )
}

/// Window position of the middle of displayed row `pos` (unscrolled).
fn row(h: &Harness, table: WidgetId, pos: usize) -> Point {
    h.at(
        table,
        60.0,
        TABLE_HEADER + pos as f32 * TABLE_ROW + TABLE_ROW / 2.0,
    )
}

fn get(h: &Harness, id: WidgetId) -> &TableView {
    h.tree.get::<TableView>(id).unwrap()
}

#[test]
fn a_table_of_100k_rows_shapes_only_the_rows_on_screen() {
    let mut h = Harness::new();
    let before = h.layouts();
    let source = Generated::new(100_000);
    let asked = source.asked.clone();
    let t = h.add(
        TableView::new(columns(), source)
            .width(400.0)
            .height(TEN_ROWS),
    );
    assert_eq!(get(&h, t).visible_rows(), 0..10);
    // Ten rows of three cells, plus the three titles.
    assert_eq!(h.layouts() - before, 33);
    assert_eq!(asked.get(), 30, "only visible cells are read");

    for _ in 0..50 {
        h.wheel(row(&h, t, 5), Point::new(0.0, -2000.0));
    }
    let visible = get(&h, t).visible_rows();
    assert!(visible.start > 3000, "{visible:?}");
    assert!(visible.len() <= 11);
    assert!(
        h.layouts() - before <= 3 + 11 * 3,
        "{}",
        h.layouts() - before
    );
}

#[test]
fn header_clicks_sort_and_reverse_and_unsortable_columns_ignore_them() {
    let mut h = Harness::new();
    let t = table(&mut h, 30);
    let header = |h: &Harness, x: f32| h.at(t, x, TABLE_HEADER / 2.0);

    // Size: numeric order, not text order ("100" after "99").
    h.click(header(&h, 200.0));
    assert_eq!(
        values::<SortChanged>(&h.messages()),
        [SortChanged {
            column: 1,
            ascending: true
        }]
    );
    let sizes: Vec<usize> = get(&h, t)
        .displayed_rows()
        .iter()
        .map(|&r| (r * 7919) % 1000)
        .collect();
    assert!(sizes.windows(2).all(|w| w[0] <= w[1]), "{sizes:?}");

    h.click(header(&h, 200.0));
    assert_eq!(get(&h, t).sort_column(), Some((1, false)));
    let first = get(&h, t).displayed_rows()[0];
    assert_eq!((first * 7919) % 1000, *sizes.last().unwrap());

    // Name: case-insensitive text order ("Item 10" before "Item 2").
    h.click(header(&h, 40.0));
    assert_eq!(&get(&h, t).displayed_rows()[..4], [0, 1, 10, 11]);

    h.messages();
    h.click(header(&h, 320.0));
    assert!(values::<SortChanged>(&h.messages()).is_empty());
    assert_eq!(get(&h, t).sort_column(), Some((0, true)));

    // Text order ignores case: "apple" before "Banana".
    let rows: Vec<Vec<String>> = ["cherry", "Banana", "apple"]
        .iter()
        .map(|s| vec![(*s).to_owned()])
        .collect();
    h.tree
        .update::<TableView, _>(t, |table, cx| table.set_data(cx, rows));
    assert_eq!(get(&h, t).displayed_rows(), [2, 1, 0]);
}

/// Sizes shown with their unit, sorted by the bytes behind them.
struct Sizes(Vec<(&'static str, Option<f64>)>);

impl TableData for Sizes {
    fn row_count(&self) -> usize {
        self.0.len()
    }

    fn cell(&self, row: usize, _column: usize) -> Cow<'_, str> {
        Cow::Borrowed(self.0[row].0)
    }

    fn sort_number(&self, row: usize, _column: usize) -> Option<f64> {
        self.0[row].1
    }
}

#[test]
fn number_columns_sort_by_the_data_sort_number_over_the_text() {
    let mut h = Harness::new();
    let rows = Sizes(vec![
        ("900 KB", Some(900e3)),
        ("—", None),
        ("1.2 GB", Some(1.2e9)),
        ("30 MB", Some(30e6)),
    ]);
    let t = h.add(
        TableView::new(
            vec![TableColumn::new("Size", 100.0).sort(ColumnSort::Number)],
            rows,
        )
        .width(400.0)
        .height(TEN_ROWS),
    );
    h.tree
        .update::<TableView, _>(t, |table, cx| table.sort_by(cx, 0, true));
    assert_eq!(get(&h, t).displayed_rows(), [0, 3, 2, 1]);
    h.tree
        .update::<TableView, _>(t, |table, cx| table.sort_by(cx, 0, false));
    assert_eq!(get(&h, t).displayed_rows(), [2, 3, 0, 1]);
}

#[test]
fn rows_select_singly_toggled_by_cmd_and_as_ranges_with_shift() {
    let mut h = Harness::new();
    let t = table(&mut h, 30);
    h.click(row(&h, t, 2));
    assert_eq!(get(&h, t).selected_rows(), [2]);
    h.click_with(row(&h, t, 5), CMD);
    assert_eq!(get(&h, t).selected_rows(), [2, 5]);
    h.click_with(row(&h, t, 8), SHIFT);
    assert_eq!(get(&h, t).selected_rows(), [5, 6, 7, 8]);
    h.click_with(row(&h, t, 6), CMD);
    assert_eq!(get(&h, t).selected_rows(), [5, 7, 8]);
    assert_eq!(
        values::<RowSelectionChanged>(&h.messages()).last(),
        Some(&RowSelectionChanged {
            lead: Some(6),
            count: 3
        })
    );
    h.key_with(Key::Char('a'), CMD);
    assert_eq!(get(&h, t).selected_count(), 30);
    h.click(row(&h, t, 1));
    assert_eq!(get(&h, t).selected_rows(), [1]);

    // Ranges follow the displayed order, not the data order. Sorting keeps the
    // selected row in view, so the rows on screen start at `top`.
    h.click(h.at(t, 200.0, TABLE_HEADER / 2.0)); // sort by Size
    let top = get(&h, t).visible_rows().start;
    assert!(
        get(&h, t).visible_rows().contains(
            &get(&h, t)
                .displayed_rows()
                .iter()
                .position(|&r| r == 1)
                .unwrap()
        )
    );
    let shown = get(&h, t).displayed_rows().to_vec();
    h.click(row(&h, t, 0));
    h.click_with(row(&h, t, 2), SHIFT);
    let mut expected = shown[top..top + 3].to_vec();
    expected.sort_unstable();
    assert_eq!(get(&h, t).selected_rows(), expected);
}

#[test]
fn keyboard_ranges_start_where_the_lead_moved_to_after_a_sort() {
    let mut h = Harness::new();
    let t = table(&mut h, 30);
    h.click(row(&h, t, 2));
    h.key_with(Key::ArrowDown, SHIFT);
    assert_eq!(get(&h, t).selected_rows(), [2, 3]);

    h.click(h.at(t, 200.0, TABLE_HEADER / 2.0)); // sort by Size
    let shown = get(&h, t).displayed_rows().to_vec();
    let at = |id: usize| shown.iter().position(|&r| r == id).unwrap();
    assert_ne!((at(2), at(3)), (2, 3), "the sort moves both rows");

    h.key_with(Key::ArrowDown, SHIFT);
    let (a, b) = (at(2).min(at(3) + 1), at(2).max(at(3) + 1));
    let mut expected = shown[a..=b].to_vec();
    expected.sort_unstable();
    assert_eq!(get(&h, t).selected_rows(), expected);
    assert_eq!(get(&h, t).lead_row(), Some(shown[at(3) + 1]));
}

#[test]
fn single_selection_and_no_selection_modes() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(columns(), Generated::new(10))
            .width(400.0)
            .height(TEN_ROWS)
            .selection_mode(SelectionMode::Single),
    );
    h.click(row(&h, t, 1));
    h.click_with(row(&h, t, 3), CMD);
    assert_eq!(get(&h, t).selected_rows(), [3]);
    h.key_with(Key::Char('a'), CMD);
    assert_eq!(get(&h, t).selected_rows(), [3]);

    let mut h = Harness::new();
    let t = h.add(
        TableView::new(columns(), Generated::new(10))
            .width(400.0)
            .height(TEN_ROWS)
            .selection_mode(SelectionMode::None),
    );
    h.click(row(&h, t, 1));
    h.key(Key::ArrowDown);
    assert_eq!(get(&h, t).selected_count(), 0);
    assert!(values::<RowSelectionChanged>(&h.messages()).is_empty());
}

#[test]
fn arrow_keys_move_and_extend_the_selection_and_scroll_it_into_view() {
    let mut h = Harness::new();
    let t = table(&mut h, 100_000);
    h.click(row(&h, t, 0));
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    assert_eq!(get(&h, t).selected_rows(), [2]);
    h.key_with(Key::ArrowDown, SHIFT);
    h.key_with(Key::ArrowDown, SHIFT);
    assert_eq!(get(&h, t).selected_rows(), [2, 3, 4]);
    h.key_with(Key::ArrowUp, SHIFT);
    assert_eq!(get(&h, t).selected_rows(), [2, 3]);

    h.key(Key::PageDown);
    assert_eq!(get(&h, t).lead_row(), Some(13));
    assert!(get(&h, t).visible_rows().contains(&13));
    h.key(Key::End);
    assert_eq!(get(&h, t).selected_rows(), [99_999]);
    assert_eq!(get(&h, t).visible_rows(), 99_990..100_000);
    h.key(Key::Home);
    assert_eq!(get(&h, t).visible_rows(), 0..10);
    h.messages();
    h.key(Key::Enter);
    assert_eq!(values::<RowActivated>(&h.messages()), [RowActivated(0)]);
}

#[test]
fn double_clicking_a_row_activates_it() {
    let mut h = Harness::new();
    let t = table(&mut h, 10);
    h.tree.set_now(1.0);
    h.click(row(&h, t, 4));
    h.tree.set_now(1.2);
    h.click(row(&h, t, 4));
    assert_eq!(values::<RowActivated>(&h.messages()), [RowActivated(4)]);
    h.tree.set_now(3.0);
    h.click(row(&h, t, 4));
    h.tree.set_now(4.0);
    h.click(row(&h, t, 4));
    assert!(values::<RowActivated>(&h.messages()).is_empty());
}

#[test]
fn dragging_a_header_edge_resizes_the_column_down_to_its_minimum() {
    let mut h = Harness::new();
    let t = table(&mut h, 10);
    let edge = h.at(t, 1.0 + 160.0, TABLE_HEADER / 2.0);
    h.down(edge);
    h.move_to(Point::new(edge.x + 40.0, edge.y));
    assert_eq!(get(&h, t).columns()[0].width(), 200.0);
    h.move_to(Point::new(edge.x - 400.0, edge.y));
    assert_eq!(get(&h, t).columns()[0].width(), 40.0, "the minimum");
    h.up(Point::new(edge.x - 400.0, edge.y));
    assert_eq!(
        values::<ColumnResized>(&h.messages()),
        [ColumnResized {
            column: 0,
            width: 40.0
        }]
    );
    assert_eq!(get(&h, t).sort_column(), None);
}

#[test]
fn reorderable_rows_move_together_to_where_they_are_dropped() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(columns(), Generated::new(10))
            .width(400.0)
            .height(TEN_ROWS)
            .reorderable(true),
    );
    h.click(row(&h, t, 1));
    h.click_with(row(&h, t, 2), CMD);
    // Drag both to between rows 5 and 6.
    let from = row(&h, t, 1);
    h.down(from);
    assert_eq!(get(&h, t).selected_rows(), [1, 2], "the press keeps both");
    h.move_to(Point::new(from.x, from.y + 20.0));
    let to = h.at(t, 60.0, TABLE_HEADER + 6.0 * TABLE_ROW);
    h.move_to(to);
    h.up(to);
    assert_eq!(get(&h, t).displayed_rows(), [0, 3, 4, 5, 1, 2, 6, 7, 8, 9]);
    assert_eq!(
        values::<RowsMoved>(&h.messages()),
        [RowsMoved {
            rows: vec![1, 2],
            to: 4
        }]
    );
    assert_eq!(get(&h, t).selected_rows(), [1, 2]);

    h.click(row(&h, t, 4));
    assert_eq!(get(&h, t).selected_rows(), [1]);
}

#[test]
fn replacing_the_data_keeps_the_sort_and_clears_the_selection() {
    let mut h = Harness::new();
    let t = table(&mut h, 10);
    h.click(h.at(t, 200.0, TABLE_HEADER / 2.0));
    h.click(row(&h, t, 0));
    let rows: Vec<Vec<String>> = ["30 KB", "4 KB", "1,200 KB"]
        .iter()
        .map(|s| vec![format!("n{s}"), (*s).to_owned(), String::new()])
        .collect();
    h.tree
        .update::<TableView, _>(t, |table, cx| table.set_data(cx, rows));
    h.prepare();
    assert_eq!(get(&h, t).displayed_rows(), [1, 0, 2]);
    assert_eq!(get(&h, t).selected_count(), 0);
}

#[test]
fn replacing_the_data_by_key_keeps_the_selected_rows_and_the_lead() {
    let mut h = Harness::new();
    let names = |names: &[&str]| -> Vec<Vec<String>> {
        names.iter().map(|n| vec![(*n).to_owned()]).collect()
    };
    let t = h.add(
        TableView::new(columns(), names(&["a", "b", "c", "d"]))
            .width(400.0)
            .height(TEN_ROWS),
    );
    h.prepare();
    h.tree
        .update::<TableView, _>(t, |table, cx| table.select_rows(cx, [1, 3]));
    let key = |data: &dyn TableData, row: usize| data.cell(row, 0).into_owned();
    h.tree.update::<TableView, _>(t, |table, cx| {
        table.set_data_keeping_selection(cx, names(&["x", "d", "a", "b"]), key)
    });
    h.prepare();
    assert_eq!(get(&h, t).selected_rows(), [1, 3]);
    assert_eq!(get(&h, t).lead_row(), Some(1), "\"d\" stays the lead");
}

#[test]
fn cmd_c_copies_the_selected_rows_as_tab_separated_text_in_displayed_order() {
    let mut h = Harness::new();
    let t = table(&mut h, 30);
    h.tree.clipboard().write_text("before");
    h.click(row(&h, t, 0));
    h.click_with(row(&h, t, 0), CMD);
    assert_eq!(get(&h, t).copy_text(), None);
    h.key_with(Key::Char('c'), CMD);
    assert_eq!(h.tree.clipboard().read_text().as_deref(), Some("before"));

    h.click(h.at(t, 200.0, TABLE_HEADER / 2.0));
    h.click(h.at(t, 200.0, TABLE_HEADER / 2.0));
    let shown = get(&h, t).displayed_rows().to_vec();
    h.tree
        .update::<TableView, _>(t, |table, cx| table.scroll_to_row(cx, shown[0]));
    h.prepare();
    h.click(row(&h, t, 0));
    h.click_with(row(&h, t, 1), CMD);
    assert_eq!(get(&h, t).selected_count(), 2);
    h.key_with(Key::Char('c'), CMD);
    let line = |id: usize| {
        let data = Generated::new(30);
        (0..3)
            .map(|c| data.cell(id, c).into_owned())
            .collect::<Vec<_>>()
            .join("\t")
    };
    let expected = format!("{}\n{}", line(shown[0]), line(shown[1]));
    assert_eq!(h.tree.clipboard().read_text(), Some(expected));
}

#[test]
fn typing_selects_the_next_row_that_starts_with_the_typed_text() {
    let mut h = Harness::new();
    let names = ["apple", "Banana", "blueberry", "cherry", "  avocado"];
    let rows: Vec<Vec<String>> = names.iter().map(|n| vec![(*n).to_owned()]).collect();
    let t = h.add(
        TableView::new(columns(), rows)
            .width(400.0)
            .height(TEN_ROWS),
    );
    h.click(row(&h, t, 0));
    h.messages();
    h.tree.set_now(10.0);
    h.type_text("b");
    assert_eq!(get(&h, t).selected_rows(), [1]);
    assert_eq!(
        values::<RowSelectionChanged>(&h.messages()),
        [RowSelectionChanged {
            lead: Some(1),
            count: 1
        }]
    );
    h.tree.set_now(10.5);
    h.type_text("l");
    assert_eq!(get(&h, t).selected_rows(), [2]);
    h.tree.set_now(12.0);
    h.type_text("a");
    assert_eq!(get(&h, t).selected_rows(), [4]);
    h.tree.set_now(14.0);
    h.type_text("a");
    assert_eq!(get(&h, t).selected_rows(), [0]);
    h.tree.set_now(16.0);
    h.type_text("z");
    h.tree.set_now(18.0);
    h.type_text(" ");
    assert_eq!(get(&h, t).selected_rows(), [0]);

    let t2 = table(&mut h, 30);
    h.click(h.at(t2, 200.0, TABLE_HEADER / 2.0));
    h.click(row(&h, t2, 0));
    h.tree.set_now(30.0);
    h.type_text("9");
    let lead = get(&h, t2).lead_row().unwrap();
    assert!(Generated::new(30).cell(lead, 1).starts_with('9'));
}

#[test]
fn scrolling_to_a_row_brings_it_into_view_without_selecting_it() {
    let mut h = Harness::new();
    let t = table(&mut h, 1000);
    h.tree
        .update::<TableView, _>(t, |table, cx| table.scroll_to_row(cx, 999));
    h.prepare();
    assert!(get(&h, t).visible_rows().contains(&999));
    h.tree
        .update::<TableView, _>(t, |table, cx| table.scroll_to_row(cx, 500));
    h.prepare();
    assert!(get(&h, t).visible_rows().contains(&500));
    assert!(get(&h, t).scroll_offset() > 0.0);
    assert_eq!(get(&h, t).selected_count(), 0);
}

#[test]
fn tables_and_text_views_fill_the_height_their_parent_has_left() {
    let mut text = TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: false,
    });
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
    let toolbar = Style {
        size: LayoutSize {
            width: percent(1.0),
            height: noa_gpui_core::style::length(40.0),
        },
        flex_shrink: 0.0,
        ..Default::default()
    };
    tree.insert(Some(root), Container::new(toolbar));
    let t = tree.insert(
        Some(root),
        TableView::new(columns(), Generated::new(1000)).fill_height(),
    );
    let view = tree.insert(Some(root), TextView::new(100.0).fill_height());
    let lines: String = (0..200).map(|i| format!("line {i}\n")).collect();
    tree.update::<TextView, _>(view, |v, cx| v.set_text(cx, lines));
    let height = |tree: &UiTree, id| tree.layout(id).unwrap().rect.height;

    // The two share what the 40-point toolbar leaves.
    let total = |tree: &UiTree| height(tree, t) + height(tree, view);
    tree.prepare(Size::new(800.0, 600.0), &mut text);
    assert_eq!(total(&tree), 560.0);
    assert!(height(&tree, t) > 250.0, "{}", height(&tree, t));
    let rows = tree.get::<TableView>(t).unwrap().visible_rows().len();
    let scrolled = tree.get::<TextView>(view).unwrap().scroll_y();
    assert!(scrolled > 0.0, "the caret at the end is in view");

    tree.prepare(Size::new(800.0, 400.0), &mut text);
    assert_eq!(total(&tree), 360.0);
    assert!(tree.get::<TableView>(t).unwrap().visible_rows().len() < rows);
    assert!(tree.get::<TextView>(view).unwrap().scroll_y() > scrolled);

    tree.update::<TableView, _>(t, |t, cx| t.set_height(cx, Some(100.0)));
    tree.prepare(Size::new(800.0, 400.0), &mut text);
    assert_eq!((height(&tree, t), height(&tree, view)), (100.0, 260.0));
    tree.update::<TextView, _>(view, |v, cx| v.set_height(cx, Some(50.0)));
    tree.prepare(Size::new(800.0, 400.0), &mut text);
    assert_eq!(height(&tree, view), 50.0);
}

/// Services: a name, a status badge and a Restart button per row.
struct Services {
    look: bool,
}

impl TableData for Services {
    fn row_count(&self) -> usize {
        5
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        match column {
            0 => Cow::Owned(format!("service-{row}")),
            1 => Cow::Borrowed(if row.is_multiple_of(2) {
                "running"
            } else {
                "stopped"
            }),
            _ => Cow::Borrowed("Restart"),
        }
    }

    fn cell_look(&self, row: usize, column: usize) -> CellLook {
        match column {
            _ if !self.look => CellLook::Text,
            0 => CellLook::Colored(Tone::Accent),
            1 if row.is_multiple_of(2) => CellLook::Badge(Tone::Success),
            1 => CellLook::Badge(Tone::Critical),
            _ => CellLook::Button,
        }
    }
}

fn services(h: &mut Harness, look: bool) -> WidgetId {
    let columns = vec![
        TableColumn::new("Name", 160.0),
        TableColumn::new("Status", 100.0),
        TableColumn::new("", 100.0),
    ];
    h.add(
        TableView::new(columns, Services { look })
            .width(400.0)
            .height(TEN_ROWS),
    )
}

#[test]
fn cells_draw_colors_badges_and_buttons_that_report_clicks() {
    let mut h = Harness::new();
    let t = services(&mut h, true);
    // A click on row 2's button reports the row and column and selects nothing.
    let button = h.at(t, 160.0 + 100.0 + 40.0, TABLE_HEADER + 2.5 * TABLE_ROW);
    h.click(button);
    assert_eq!(
        values::<CellButtonClicked>(&h.messages()),
        [CellButtonClicked { row: 2, column: 2 }]
    );
    assert_eq!(get(&h, t).selected_count(), 0);
    h.down(button);
    h.up(h.at(t, 40.0, TABLE_HEADER + 0.5 * TABLE_ROW));
    assert!(values::<CellButtonClicked>(&h.messages()).is_empty());
    h.click(h.at(t, 40.0, TABLE_HEADER + 1.5 * TABLE_ROW));
    assert_eq!(get(&h, t).selected_rows(), [1]);

    let quads = |h: &Harness| {
        h.tree
            .display_list()
            .commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::Quad { .. }))
            .count()
    };
    let with_looks = quads(&h);
    let mut plain = Harness::new();
    let p = services(&mut plain, false);
    plain.click(plain.at(p, 40.0, TABLE_HEADER + 1.5 * TABLE_ROW));
    assert!(
        with_looks >= quads(&plain) + 2 * 5,
        "{with_looks} vs {}",
        quads(&plain)
    );
}

const DOT: &str = r#"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="8"/></svg>"#;

/// Containers grouped by stack: a group row per stack with a count and an Up button,
/// then its containers with Stop, Restart (disabled for the last) and Logs buttons.
struct Stacks {
    icon: VectorIcon,
}

const STACKS: [(&str, bool); 5] = [
    ("web", true),
    ("web-1", false),
    ("web-2", false),
    ("db", true),
    ("db-1", false),
];

impl TableData for Stacks {
    fn row_count(&self) -> usize {
        STACKS.len()
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        let (name, group) = STACKS[row];
        Cow::Borrowed(match column {
            0 => name,
            1 if group => "2 running",
            1 => "running",
            _ => "",
        })
    }

    fn is_group_row(&self, row: usize) -> bool {
        STACKS[row].1
    }

    fn cell_look(&self, _row: usize, column: usize) -> CellLook {
        match column {
            2 => CellLook::Buttons,
            _ => CellLook::Text,
        }
    }

    fn cell_buttons(&self, row: usize, _column: usize) -> Vec<CellButton> {
        if STACKS[row].1 {
            return vec![CellButton::new("up").label("Up")];
        }
        vec![
            CellButton::new("stop")
                .icon(self.icon.clone())
                .tooltip("Stop"),
            CellButton::new("restart")
                .icon(self.icon.clone())
                .tone(Tone::Warning)
                .enabled(row != 4),
            CellButton::new("logs").label("Logs"),
        ]
    }
}

fn stacks(h: &mut Harness, configure: impl FnOnce(TableView) -> TableView) -> WidgetId {
    let columns = vec![
        TableColumn::new("Name", 120.0),
        TableColumn::new("State", 100.0),
        TableColumn::new("Actions", 160.0),
    ];
    let data = Stacks {
        icon: VectorIcon::from_svg(DOT).unwrap(),
    };
    let table = TableView::new(columns, data)
        .width(400.0)
        .height(TEN_ROWS)
        .floats_group_rows(false);
    h.add(configure(table))
}

/// Text drawn by the last paint: its string, where (relative to `id`) and whether it
/// is in a named family rather than the default sans-serif.
fn drawn_texts(h: &Harness, id: WidgetId) -> Vec<(String, Rect, bool)> {
    let origin = h.rect(id);
    h.tree
        .display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { rect, layout, .. } => {
                let line = &h.text.layouts().buffer(*layout).lines[0];
                let named = matches!(line.attrs_list().defaults().family, Family::Name(_));
                let rect = rect.translate(-origin.x, -origin.y);
                Some((line.text().to_owned(), rect, named))
            }
            _ => None,
        })
        .collect()
}

fn quads(h: &Harness) -> Vec<(Rect, Color, noa_gpui_scene::Border)> {
    h.tree
        .display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Quad {
                rect,
                color,
                border,
                ..
            } => Some((*rect, *color, *border)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_cell_of_buttons_reports_the_clicked_action_and_ignores_disabled_ones() {
    let mut h = Harness::new();
    let t = stacks(&mut h, |t| t);
    let icons = || {
        (h.tree.display_list().commands().iter())
            .filter(|c| matches!(c, DrawCommand::Icon { .. }))
            .count()
    };
    assert_eq!(icons(), 6);
    // The buttons sit side by side from the column's left edge (1 + 120 + 100 + 8):
    // icon buttons are squares as tall as the row less 4, 4 apart.
    let y = |pos: usize| TABLE_HEADER + (pos as f32 + 0.5) * TABLE_ROW;
    let stop = 229.0 + 10.0;
    let restart = 229.0 + 24.0 + 10.0;
    let logs = 229.0 + 48.0 + 10.0;
    h.click(h.at(t, stop, y(1)));
    h.click(h.at(t, logs, y(2)));
    let clicked: Vec<_> = values::<CellActionClicked>(&h.messages())
        .into_iter()
        .map(|m| (m.row, m.column, m.action.into_owned()))
        .collect();
    assert_eq!(clicked, [(1, 2, "stop".into()), (2, 2, "logs".into())]);
    assert_eq!(get(&h, t).selected_count(), 0);
    h.click(h.at(t, restart, y(4)));
    assert!(values::<CellActionClicked>(&h.messages()).is_empty());
    assert_eq!(get(&h, t).selected_rows(), [4]);
    h.down(h.at(t, stop, y(1)));
    h.up(h.at(t, restart, y(1)));
    assert!(values::<CellActionClicked>(&h.messages()).is_empty());
}

#[test]
fn cell_buttons_have_tooltips_and_names_for_assistive_technology() {
    let mut h = Harness::new();
    let t = stacks(&mut h, |t| t);
    let y = TABLE_HEADER + 1.5 * TABLE_ROW;
    let tip = |x: f32| noa_gpui_core::Widget::tooltip(get(&h, t), Point::new(x, y));
    assert_eq!(tip(229.0 + 10.0).as_deref(), Some("Stop"));
    assert_eq!(tip(229.0 + 24.0 + 10.0), None);
    assert_eq!(tip(229.0 + 48.0 + 10.0), None);

    let update = h.tree.accessibility_update(1.0);
    let row = update
        .nodes
        .iter()
        .find(|(id, _)| *id == noa_gpui_core::access_item_id(t, 1))
        .map(|(_, node)| node)
        .unwrap();
    assert_eq!(row.label(), Some("web-1, running, Stop, Logs"));
}

#[test]
fn group_rows_can_show_their_cells_and_indent_the_rows_under_them() {
    let mut h = Harness::new();
    let t = stacks(&mut h, |t| t);
    let texts = drawn_texts(&h, t);
    assert!(texts.iter().all(|(s, ..)| s != "2 running" && s != "Up"));

    let mut h = Harness::new();
    let t = stacks(&mut h, |t| {
        t.group_row_cells(true).group_member_indent(16.0)
    });
    let texts = drawn_texts(&h, t);
    let find = |name: &str| texts.iter().find(|(s, ..)| s == name).unwrap().1;
    assert!(texts.iter().any(|(s, ..)| s == "2 running"));
    assert_eq!(find("web").x, 1.0 + 8.0);
    assert_eq!(find("web-1").x, 1.0 + 8.0 + 16.0);
    let rules = quads(&h)
        .into_iter()
        .filter(|(r, ..)| r.width == 2.0 && r.height == TABLE_ROW)
        .count();
    assert_eq!(rules, 3);
    let up = h.at(t, 229.0 + 10.0, TABLE_HEADER + 0.5 * TABLE_ROW);
    h.click(up);
    let clicked = values::<CellActionClicked>(&h.messages());
    assert_eq!(
        clicked
            .iter()
            .map(|m| (m.row, m.action.as_ref()))
            .collect::<Vec<_>>(),
        [(0, "up")]
    );
    assert_eq!(get(&h, t).selected_count(), 0);
}

#[test]
fn flexible_columns_share_the_room_left_and_a_drag_fixes_their_width() {
    let mut h = Harness::new();
    let columns = vec![
        TableColumn::new("A", 100.0),
        TableColumn::new("B", 100.0).flex(1.0),
        TableColumn::new("C", 50.0).flex(3.0),
    ];
    let t = h.add(
        TableView::new(columns, Generated::new(5))
            .width(400.0)
            .height(TEN_ROWS),
    );
    // The view is 398 wide inside its border: 148 left over, in 4 shares of 37.
    let widths = |h: &Harness| {
        (0..3)
            .map(|c| get(h, t).column_width(c))
            .collect::<Vec<_>>()
    };
    assert_eq!(widths(&h), [100.0, 137.0, 161.0]);
    // Dragging B's right edge back by 37 gives it 100, fixed; C takes all that is left.
    let edge = 1.0 + 100.0 + 137.0;
    h.down(h.at(t, edge, TABLE_HEADER / 2.0));
    h.move_to(h.at(t, edge - 37.0, TABLE_HEADER / 2.0));
    h.up(h.at(t, edge - 37.0, TABLE_HEADER / 2.0));
    assert_eq!(widths(&h), [100.0, 100.0, 198.0]);
    assert_eq!(get(&h, t).columns()[1].flex_weight(), 0.0);
    assert_eq!(
        values::<ColumnResized>(&h.messages())
            .last()
            .map(|r| r.width),
        Some(100.0)
    );

    let mut h = Harness::new();
    let columns = vec![
        TableColumn::new("A", 150.0),
        TableColumn::new("B", 150.0).flex(1.0),
    ];
    let t = h.add(TableView::new(columns, Generated::new(5)).width(200.0));
    assert_eq!(widths_of(&h, t, 2), [150.0, 150.0]);
}

#[test]
fn shrinkable_columns_give_up_width_before_the_table_scrolls() {
    let name = "a-long-container-name-that-does-not-fit".to_owned();
    let rows = vec![vec!["web".to_owned(), name.clone(), "running".to_owned()]];
    let columns = || {
        vec![
            TableColumn::new("ID", 100.0),
            TableColumn::new("Name", 200.0).flex(1.0).shrink_to(50.0),
            TableColumn::new("State", 100.0).shrink_to(50.0),
        ]
    };
    let add = |h: &mut Harness, width: f32| {
        h.add(
            TableView::new(columns(), rows.clone())
                .width(width)
                .height(TEN_ROWS),
        )
    };
    let mut h = Harness::new();
    let t = add(&mut h, 502.0);
    assert_eq!(widths_of(&h, t, 3), [100.0, 300.0, 100.0]);

    // 100 short in a 300-wide view: Name can lose 150 and State 50, so they give up
    // 75 and 25, and Name's text is shortened.
    let mut h = Harness::new();
    let t = add(&mut h, 302.0);
    assert_eq!(widths_of(&h, t, 3), [100.0, 125.0, 75.0]);
    let texts = drawn_texts(&h, t);
    assert!(texts.iter().any(|(text, ..)| text.ends_with('…')));
    assert!(!texts.iter().any(|(text, ..)| *text == name));

    let mut h = Harness::new();
    let t = add(&mut h, 152.0);
    assert_eq!(widths_of(&h, t, 3), [100.0, 50.0, 50.0]);

    let mut h = Harness::new();
    let t = add(&mut h, 302.0);
    let edge = 1.0 + 100.0 + 125.0;
    h.down(h.at(t, edge, TABLE_HEADER / 2.0));
    h.move_to(h.at(t, edge + 25.0, TABLE_HEADER / 2.0));
    h.up(h.at(t, edge + 25.0, TABLE_HEADER / 2.0));
    // Name is fixed at 150; State gives up all it can (50) of the 150 short.
    assert_eq!(widths_of(&h, t, 3), [100.0, 150.0, 50.0]);
}

fn widths_of(h: &Harness, t: WidgetId, n: usize) -> Vec<f32> {
    (0..n).map(|c| get(h, t).column_width(c)).collect()
}

#[test]
fn columns_and_the_header_take_their_own_font_case_and_background() {
    let mut h = Harness::new();
    let columns = vec![
        TableColumn::new("Name", 160.0),
        TableColumn::new("Size", 100.0).font_family("monospace"),
    ];
    let t = h.add(
        TableView::new(columns, Generated::new(3))
            .width(400.0)
            .height(TEN_ROWS)
            .header_uppercase(true)
            .header_font_family("monospace")
            .header_background(Color::hex(0x123456)),
    );
    let texts = drawn_texts(&h, t);
    let named: Vec<&str> = (texts.iter())
        .filter(|(.., named)| *named)
        .map(|(s, ..)| s.as_str())
        .collect();
    assert_eq!(named.len(), 5, "{named:?}");
    assert!(named.contains(&"NAME") && named.contains(&"SIZE"));
    assert!(texts.iter().any(|(s, _, named)| s == "Item 0" && !named));
    assert!(quads(&h).iter().any(|(_, c, _)| *c == Color::hex(0x123456)));
}

#[test]
fn the_hovered_row_is_tinted_when_asked_and_forgotten_when_the_pointer_leaves() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(columns(), Generated::new(20))
            .width(400.0)
            .height(TEN_ROWS)
            .highlights_hovered_row(true),
    );
    let before = quads(&h).len();
    h.move_to(row(&h, t, 2));
    assert_eq!(get(&h, t).hovered_row(), Some(2));
    assert_eq!(quads(&h).len(), before + 1);
    h.tree.dispatch(Event::PointerLeave, &mut h.text);
    h.prepare();
    assert_eq!(get(&h, t).hovered_row(), None);
    assert_eq!(quads(&h).len(), before);

    let mut h = Harness::new();
    let t = table(&mut h, 20);
    let before = quads(&h).len();
    h.move_to(row(&h, t, 2));
    assert_eq!(get(&h, t).hovered_row(), Some(2));
    assert_eq!(quads(&h).len(), before);
}

#[test]
fn outlined_badges_are_framed_in_the_themes_status_color() {
    struct Status;
    impl TableData for Status {
        fn row_count(&self) -> usize {
            1
        }
        fn cell(&self, _: usize, _: usize) -> Cow<'_, str> {
            Cow::Borrowed("running")
        }
        fn cell_look(&self, _: usize, _: usize) -> CellLook {
            CellLook::OutlinedBadge(Tone::Success)
        }
    }
    let green = Color::hex(0x00a000);
    let mut h = Harness::new();
    h.tree.set_theme(Theme::light().with_status_colors(
        green,
        Color::hex(0xc08000),
        Color::hex(0xc00000),
    ));
    h.add(
        TableView::new(vec![TableColumn::new("Status", 120.0)], Status)
            .width(200.0)
            .height(TEN_ROWS),
    );
    let framed = quads(&h)
        .into_iter()
        .filter(|(_, fill, border)| {
            border.width == 1.0 && border.color == green.with_alpha(0.6) && fill.a < 1.0
        })
        .count();
    assert_eq!(framed, 1);
    let dark = Theme::light()
        .with_status_colors(green, green, green)
        .with_appearance(noa_gpui_core::Appearance::Dark);
    assert_eq!(
        (dark.success, dark.warning, dark.critical),
        (green, green, green)
    );
}

fn checked_table(h: &mut Harness, rows: usize) -> WidgetId {
    h.add(
        TableView::new(columns(), Generated::new(rows))
            .selection_checkboxes(true)
            .width(400.0)
            .height(TEN_ROWS),
    )
}

/// Window position of the checkbox of displayed row `pos` (unscrolled).
fn check(h: &Harness, table: WidgetId, pos: usize) -> Point {
    Point::new(
        h.at(table, 1.0 + TABLE_CHECK_COLUMN / 2.0, 0.0).x,
        row(h, table, pos).y,
    )
}

fn header_check(h: &Harness, table: WidgetId) -> Point {
    h.at(table, 1.0 + TABLE_CHECK_COLUMN / 2.0, TABLE_HEADER / 2.0)
}

/// The checkboxes assistive technology is told about: label and state.
fn check_nodes(h: &mut Harness) -> Vec<(String, noa_gpui_core::accesskit::Toggled)> {
    let update = h.tree.accessibility_update(1.0);
    (update.nodes.iter())
        .filter(|(_, node)| node.role() == noa_gpui_core::accesskit::Role::CheckBox)
        .map(|(_, node)| (node.label().unwrap().to_owned(), node.toggled().unwrap()))
        .collect()
}

/// Where the last paint drew the text of the first cell (relative to the table).
fn first_cell_x(h: &Harness, table: WidgetId) -> f32 {
    let texts = drawn_texts(h, table);
    let (_, rect, _) = texts.iter().find(|(text, ..)| text == "Item 0").unwrap();
    rect.x
}

#[test]
fn row_checkboxes_name_visible_cells_when_columns_are_hidden() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(
            vec![
                TableColumn::new("Internal", 100.0).hidden(true),
                TableColumn::new("Name", 160.0),
                TableColumn::new("Kind", 100.0),
            ],
            vec![vec![
                "hidden identity".into(),
                "Visible item".into(),
                "File".into(),
            ]],
        )
        .selection_checkboxes(true)
        .width(400.0)
        .height(TEN_ROWS),
    );
    let labels = |h: &mut Harness| check_nodes(h).into_iter().map(|n| n.0).collect::<Vec<_>>();
    assert_eq!(labels(&mut h), ["Select all rows", "Select Visible item"]);
    h.tree.update::<TableView, _>(t, |t, cx| {
        assert!(t.set_column_visible(cx, 1, false));
    });
    h.prepare();
    assert_eq!(labels(&mut h), ["Select all rows", "Select File"]);
    h.click(check(&h, t, 0));
    assert_eq!(get(&h, t).selected_rows(), [0]);
    h.tree.update::<TableView, _>(t, |t, cx| {
        assert!(t.set_column_visible(cx, 0, true));
    });
    h.prepare();
    assert_eq!(
        labels(&mut h),
        ["Select all rows", "Select hidden identity"]
    );
}

#[test]
fn a_rows_checkbox_adds_or_removes_only_that_row_and_does_not_scroll() {
    let mut h = Harness::new();
    let t = checked_table(&mut h, 30);
    assert_eq!(first_cell_x(&h, t), 1.0 + TABLE_CHECK_COLUMN + 8.0);
    h.click(row(&h, t, 2));
    assert_eq!(get(&h, t).selected_rows(), [2]);
    h.messages();
    h.click(check(&h, t, 5));
    assert_eq!(get(&h, t).selected_rows(), [2, 5]);
    assert_eq!(
        values::<RowSelectionChanged>(&h.messages()),
        [RowSelectionChanged {
            lead: Some(5),
            count: 2
        }]
    );
    h.click(check(&h, t, 2));
    assert_eq!(get(&h, t).selected_rows(), [5]);
    h.down(check(&h, t, 7));
    h.up(check(&h, t, 8));
    assert_eq!(get(&h, t).selected_rows(), [5]);
    h.click(check(&h, t, 7));
    h.click(check(&h, t, 7));
    assert_eq!(get(&h, t).selected_rows(), [5]);
    assert!(values::<RowActivated>(&h.messages()).is_empty());

    h.wheel(row(&h, t, 5), Point::new(0.0, -15.0 * TABLE_ROW));
    let offset = get(&h, t).scroll_offset();
    let top = get(&h, t).visible_rows().start;
    assert!(top > 5, "row 5 is above the view");
    h.click(check(&h, t, 3));
    assert_eq!(get(&h, t).scroll_offset(), offset);
    let under = ((offset + 3.5 * TABLE_ROW) / TABLE_ROW) as usize;
    assert_eq!(get(&h, t).selected_rows(), [5, under]);
}

#[test]
fn the_header_checkbox_selects_all_or_none_and_is_mixed_in_between() {
    use noa_gpui_core::accesskit::Toggled;
    let mut h = Harness::new();
    let t = checked_table(&mut h, 30);
    assert_eq!(
        check_nodes(&mut h)[0],
        ("Select all rows".into(), Toggled::False)
    );
    h.click(check(&h, t, 1));
    let nodes = check_nodes(&mut h);
    assert_eq!(nodes[0], ("Select all rows".into(), Toggled::Mixed));
    assert_eq!(nodes[1], ("Select Item 0".into(), Toggled::False));
    assert_eq!(nodes[2], ("Select Item 1".into(), Toggled::True));
    // The header's and those of the ten rows on screen.
    assert_eq!(nodes.len(), 11);
    h.messages();
    h.click(header_check(&h, t));
    assert_eq!(get(&h, t).selected_count(), 30);
    assert_eq!(check_nodes(&mut h)[0].1, Toggled::True);
    let messages = h.messages();
    assert_eq!(values::<RowSelectionChanged>(&messages).len(), 1);
    assert!(values::<SortChanged>(&messages).is_empty());
    assert_eq!(get(&h, t).sort_column(), None);
    h.click(header_check(&h, t));
    assert_eq!(get(&h, t).selected_count(), 0);
    assert_eq!(check_nodes(&mut h)[0].1, Toggled::False);
    h.tree.remove(t, &mut h.text);

    let single = h.add(
        TableView::new(columns(), Generated::new(5))
            .selection_checkboxes(true)
            .selection_mode(SelectionMode::Single)
            .width(400.0)
            .height(TEN_ROWS),
    );
    h.click(check(&h, single, 1));
    h.click(check(&h, single, 3));
    assert_eq!(get(&h, single).selected_rows(), [3]);
    h.click(check(&h, single, 3));
    assert_eq!(get(&h, single).selected_count(), 0);
    h.click(header_check(&h, single));
    assert_eq!(get(&h, single).selected_count(), 0);
    assert_eq!(check_nodes(&mut h).len(), 5);
    h.tree.remove(single, &mut h.text);

    let none = h.add(
        TableView::new(columns(), Generated::new(5))
            .selection_checkboxes(true)
            .selection_mode(SelectionMode::None)
            .width(400.0)
            .height(TEN_ROWS),
    );
    assert!(check_nodes(&mut h).is_empty());
    assert_eq!(first_cell_x(&h, none), 1.0 + 8.0);
}

#[test]
fn the_checkboxes_are_not_a_column_of_the_data() {
    let mut h = Harness::new();
    let t = checked_table(&mut h, 30);
    h.click(check(&h, t, 0));
    h.click(check(&h, t, 1));
    assert_eq!(
        get(&h, t).copy_text().as_deref(),
        Some("Item 0\t0\tred\nItem 1\t919\tGreen")
    );
    let tsv = get(&h, t).export_text(TableText::Tsv);
    assert_eq!(tsv.lines().next(), Some("Name\tSize\tKind"));
    assert_eq!(tsv.lines().nth(1), Some("Item 0\t0\tred"));
    h.tree.set_now(10.0);
    h.type_text("item 2");
    assert_eq!(get(&h, t).selected_rows(), [2]);
    h.messages();
    h.click(h.at(
        t,
        1.0 + TABLE_CHECK_COLUMN + 160.0 + 50.0,
        TABLE_HEADER / 2.0,
    ));
    assert_eq!(get(&h, t).sort_column(), Some((1, true)));
    assert_eq!(
        values::<SortChanged>(&h.messages()),
        [SortChanged {
            column: 1,
            ascending: true
        }]
    );
}

#[test]
fn group_rows_have_no_checkbox_and_cell_buttons_keep_their_column() {
    use noa_gpui_core::accesskit::{Action, ActionRequest, Toggled, TreeId};
    let mut h = Harness::new();
    let t = stacks(&mut h, |t| t.selection_checkboxes(true));
    let y = |pos: usize| TABLE_HEADER + (pos as f32 + 0.5) * TABLE_ROW;
    let labels: Vec<String> = check_nodes(&mut h).into_iter().map(|n| n.0).collect();
    assert_eq!(
        labels,
        [
            "Select all rows",
            "Select web-1",
            "Select web-2",
            "Select db-1"
        ]
    );
    h.click(h.at(t, 1.0 + TABLE_CHECK_COLUMN / 2.0, y(0)));
    assert_eq!(
        values::<GroupRowClicked>(&h.messages()),
        [GroupRowClicked(0)]
    );
    assert_eq!(get(&h, t).selected_count(), 0);
    h.click(header_check(&h, t));
    assert_eq!(get(&h, t).selected_rows(), [1, 2, 4]);
    assert_eq!(check_nodes(&mut h)[0].1, Toggled::True);
    h.messages();
    h.click(h.at(t, 229.0 + TABLE_CHECK_COLUMN + 10.0, y(1)));
    let clicked: Vec<_> = values::<CellActionClicked>(&h.messages())
        .into_iter()
        .map(|m| (m.row, m.column, m.action.into_owned()))
        .collect();
    assert_eq!(clicked, [(1, 2, "stop".into())]);

    // Assistive technology clicks them after the five rows: the header's, then web-1's.
    let click = |h: &mut Harness, item: usize| {
        h.tree.accessibility_action(
            &ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: noa_gpui_core::access_item_id(t, item),
                data: None,
            },
            &mut h.text,
        );
        h.prepare();
    };
    click(&mut h, 5);
    assert_eq!(get(&h, t).selected_count(), 0);
    click(&mut h, 6);
    assert_eq!(get(&h, t).selected_rows(), [1]);
}

fn tree_items() -> Vec<OutlineItem> {
    vec![
        OutlineItem::new("Animals")
            .child(
                OutlineItem::new("Mammals")
                    .child(OutlineItem::new("Cat"))
                    .child(OutlineItem::new("Dog")),
            )
            .child(OutlineItem::new("Birds").child(OutlineItem::new("Owl"))),
        OutlineItem::new("Plants").child(OutlineItem::new("Fern")),
    ]
}
// Ids, depth first: 0 Animals, 1 Mammals, 2 Cat, 3 Dog, 4 Birds, 5 Owl, 6 Plants, 7 Fern.

fn outline(h: &Harness, id: WidgetId) -> &OutlineView {
    h.tree.get::<OutlineView>(id).unwrap()
}

/// Middle of outline row `pos`; `x` from the left.
fn outline_row(h: &Harness, id: WidgetId, pos: usize, x: f32) -> Point {
    let rh = 22.0;
    h.at(id, x, 1.0 + pos as f32 * rh + rh / 2.0)
}

#[test]
fn outline_triangles_expand_and_collapse_and_option_does_a_whole_branch() {
    let mut h = Harness::new();
    let o = h.add(OutlineView::new(tree_items()).width(300.0));
    assert_eq!(outline(&h, o).visible_items(), [0, 6]);
    // The triangle of "Animals" (depth 0) is at the left.
    h.click(outline_row(&h, o, 0, 12.0));
    assert_eq!(outline(&h, o).visible_items(), [0, 1, 4, 6]);
    assert_eq!(
        values::<ItemExpanded>(&h.messages()),
        [ItemExpanded {
            item: 0,
            expanded: true
        }]
    );
    h.click(outline_row(&h, o, 1, 120.0));
    assert_eq!(outline(&h, o).selected_items(), [1]);
    assert_eq!(outline(&h, o).visible_items(), [0, 1, 4, 6]);

    h.click_with(outline_row(&h, o, 3, 12.0), ALT);
    assert_eq!(outline(&h, o).visible_items(), [0, 1, 4, 6, 7]);
    h.click_with(outline_row(&h, o, 0, 12.0), ALT);
    assert_eq!(outline(&h, o).visible_items(), [0, 6, 7]);
    assert!(!outline(&h, o).is_expanded(1));
    assert_eq!(outline(&h, o).selected_items(), [0]);
}

#[test]
fn outline_keyboard_expands_collapses_and_walks_the_hierarchy() {
    let mut h = Harness::new();
    let o = h.add(OutlineView::new(tree_items()).width(300.0));
    h.click(outline_row(&h, o, 0, 120.0));
    h.key(Key::ArrowRight); // expand Animals
    assert_eq!(outline(&h, o).visible_items(), [0, 1, 4, 6]);
    h.key(Key::ArrowRight); // already open: to its first child
    assert_eq!(outline(&h, o).lead_item(), Some(1));
    h.key_with(Key::ArrowRight, ALT); // Mammals and below
    assert_eq!(outline(&h, o).visible_items(), [0, 1, 2, 3, 4, 6]);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    assert_eq!(outline(&h, o).lead_item(), Some(3)); // Dog
    h.key(Key::ArrowLeft); // a leaf: to the parent
    assert_eq!(outline(&h, o).lead_item(), Some(1));
    h.key(Key::ArrowLeft); // open: collapse
    assert_eq!(outline(&h, o).visible_items(), [0, 1, 4, 6]);
    h.key(Key::ArrowLeft);
    h.key(Key::ArrowLeft);
    assert_eq!(outline(&h, o).visible_items(), [0, 6]);
    assert_eq!(outline(&h, o).selected_items(), [0]);
}

#[test]
fn sidebar_headings_toggle_their_section_and_are_never_selected() {
    let mut h = Harness::new();
    let items = vec![
        OutlineItem::new("Favorites")
            .child(OutlineItem::new("Desktop"))
            .child(OutlineItem::new("Inbox").badge("12")),
        OutlineItem::new("Tags").child(OutlineItem::new("Red")),
    ];
    // Ids: 0 Favorites, 1 Desktop, 2 Inbox, 3 Tags, 4 Red.
    let s = h.add(OutlineView::new(items).sidebar().width(220.0));
    assert_eq!(outline(&h, s).visible_items(), [0, 1, 2, 3, 4]);
    let rh = 28.0;
    let at = |h: &Harness, pos: usize| h.at(s, 60.0, pos as f32 * rh + rh / 2.0);
    h.click(at(&h, 0));
    assert_eq!(
        outline(&h, s).visible_items(),
        [0, 3, 4],
        "heading collapsed"
    );
    assert!(outline(&h, s).selected_items().is_empty());
    h.click(at(&h, 0));
    h.click(at(&h, 2));
    assert_eq!(outline(&h, s).selected_items(), [2]);
    h.key_with(Key::ArrowDown, SHIFT);
    assert_eq!(outline(&h, s).selected_items(), [4]);
    h.key(Key::Home);
    assert_eq!(outline(&h, s).selected_items(), [1]);
}

fn filter_outline(h: &mut Harness, o: WidgetId, query: &str) {
    h.tree
        .update::<OutlineView, _>(o, |view, cx| view.set_filter_text(cx, query));
    h.prepare();
}

#[test]
fn an_outline_filter_lists_matches_under_their_opened_parents() {
    let mut h = Harness::new();
    let o = h.add(OutlineView::new(tree_items()).width(300.0));
    assert_eq!(outline(&h, o).visible_items(), [0, 6]);
    assert!(!outline(&h, o).is_filtered());

    filter_outline(&mut h, o, "O");
    assert_eq!(outline(&h, o).visible_items(), [0, 1, 3, 4, 5]);
    assert_eq!(outline(&h, o).matching_item_count(), 2);
    assert!(
        !outline(&h, o).is_expanded(0),
        "the items' own state is kept"
    );

    filter_outline(&mut h, o, "mammal");
    assert_eq!(outline(&h, o).visible_items(), [0, 1]);
    filter_outline(&mut h, o, "d o");
    assert_eq!(outline(&h, o).visible_items(), [0, 1, 3]);
    filter_outline(&mut h, o, "zebra");
    assert_eq!(outline(&h, o).visible_items(), [] as [usize; 0]);

    filter_outline(&mut h, o, "");
    assert_eq!(outline(&h, o).visible_items(), [0, 6]);
    assert!(!outline(&h, o).is_filtered());
}

#[test]
fn a_match_picked_while_filtered_is_seen_in_its_place_once_the_filter_clears() {
    let mut h = Harness::new();
    let o = h.add(OutlineView::new(tree_items()).width(300.0));
    h.click(outline_row(&h, o, 1, 120.0)); // Plants
    assert_eq!(outline(&h, o).selected_items(), [6]);
    filter_outline(&mut h, o, "owl");
    assert_eq!(outline(&h, o).visible_items(), [0, 4, 5]);
    assert!(
        outline(&h, o).selected_items().is_empty(),
        "Plants is hidden"
    );
    h.click(outline_row(&h, o, 1, 28.0));
    assert_eq!(outline(&h, o).visible_items(), [0, 4]);
    h.click(outline_row(&h, o, 1, 28.0));
    h.click(outline_row(&h, o, 2, 120.0));
    assert_eq!(outline(&h, o).selected_items(), [5]);
    filter_outline(&mut h, o, "");
    assert_eq!(outline(&h, o).visible_items(), [0, 1, 4, 5, 6]);
    assert_eq!(outline(&h, o).selected_items(), [5]);
    assert!(!outline(&h, o).is_expanded(1));
}

#[test]
fn a_sidebar_filter_keeps_the_headings_over_their_matches() {
    let mut h = Harness::new();
    let items = vec![
        OutlineItem::new("Favorites")
            .child(OutlineItem::new("Desktop"))
            .child(OutlineItem::new("Documents")),
        OutlineItem::new("Tags")
            .child(OutlineItem::new("Red"))
            .child(OutlineItem::new("Desk")),
    ];
    // Ids: 0 Favorites, 1 Desktop, 2 Documents, 3 Tags, 4 Red, 5 Desk.
    let s = h.add(OutlineView::new(items).sidebar().width(220.0));
    filter_outline(&mut h, s, "desk");
    assert_eq!(outline(&h, s).visible_items(), [0, 1, 3, 5]);
    filter_outline(&mut h, s, "tags");
    assert_eq!(outline(&h, s).visible_items(), [] as [usize; 0]);
    assert_eq!(outline(&h, s).matching_item_count(), 0);
    h.tree
        .update::<OutlineView, _>(s, |view, cx| view.select_item(cx, Some(4)));
    assert!(outline(&h, s).selected_items().is_empty());
}

/// The hierarchy as indented titles, `+` before an expanded item with children.
fn shape(items: &[OutlineItem]) -> Vec<String> {
    fn walk(item: &OutlineItem, depth: usize, out: &mut Vec<String>) {
        let open = if item.is_expanded() && !item.child_items().is_empty() {
            "+"
        } else {
            ""
        };
        out.push(format!("{}{open}{}", "  ".repeat(depth), item.title()));
        for c in item.child_items() {
            walk(c, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    for item in items {
        walk(item, 0, &mut out);
    }
    out
}

fn outline_shape(h: &Harness, o: WidgetId) -> Vec<String> {
    shape(&outline(h, o).items())
}

/// An outline of [`tree_items`] with everything expanded and reorderable. Rows: 0
/// Animals, 1 Mammals, 2 Cat, 3 Dog, 4 Birds, 5 Owl, 6 Plants, 7 Fern.
fn open_outline(h: &mut Harness) -> WidgetId {
    let o = h.add(
        OutlineView::new(tree_items())
            .reorderable(true)
            .width(300.0)
            .height(400.0),
    );
    h.tree.update::<OutlineView, _>(o, |v, cx| {
        for item in [0, 1, 4, 6] {
            v.set_expanded(cx, item, true);
        }
    });
    h.prepare();
    o
}

/// A point in outline row `pos`, `y` down from the row's top.
fn in_row(h: &Harness, o: WidgetId, pos: usize, y: f32) -> Point {
    h.at(o, 120.0, 1.0 + pos as f32 * 22.0 + y)
}

fn drag(h: &mut Harness, from: Point, to: Point) {
    h.down(from);
    h.move_to(Point::new(from.x, from.y + 6.0));
    h.move_to(to);
    h.up(to);
}

#[test]
fn outline_items_are_inserted_removed_and_given_back() {
    let mut h = Harness::new();
    let o = open_outline(&mut h);
    h.click(outline_row(&h, o, 5, 120.0)); // Owl
    assert_eq!(outline(&h, o).selected_items(), [5]);
    h.messages();

    let fish = h.tree.update::<OutlineView, _>(o, |v, cx| {
        v.insert_item(cx, Some(0), 1, OutlineItem::new("Fish"))
    });
    h.prepare();
    assert_eq!(fish, Some(4));
    assert_eq!(
        outline_shape(&h, o),
        [
            "+Animals",
            "  +Mammals",
            "    Cat",
            "    Dog",
            "  Fish",
            "  +Birds",
            "    Owl",
            "+Plants",
            "  Fern"
        ]
    );
    // Owl moved to id 6 and is still the one selected.
    assert_eq!(outline(&h, o).selected_items(), [6]);
    assert_eq!(outline(&h, o).title(6), "Owl");
    assert_eq!(outline(&h, o).parent(4), Some(0));

    h.tree.update::<OutlineView, _>(o, |v, cx| {
        v.insert_item(
            cx,
            None,
            99,
            OutlineItem::new("Fungi").child(OutlineItem::new("Moss")),
        )
    });
    h.prepare();
    assert_eq!(outline(&h, o).items().last().unwrap().title(), "Fungi");
    assert_eq!(outline(&h, o).item_count(), 11);

    h.tree
        .update::<OutlineView, _>(o, |v, cx| v.remove_items(cx, &[1, 9]));
    h.prepare();
    assert_eq!(
        outline_shape(&h, o),
        [
            "+Animals", "  Fish", "  +Birds", "    Owl", "+Plants", "  Fern"
        ]
    );
    assert_eq!(outline(&h, o).selected_items(), [3]);
    assert_eq!(outline(&h, o).visible_items(), [0, 1, 2, 3, 4, 5]);
    assert!(h.messages().is_empty(), "changes from code emit nothing");

    h.tree
        .update::<OutlineView, _>(o, |v, cx| v.remove_items(cx, &[3]));
    assert!(outline(&h, o).selected_items().is_empty());
    assert_eq!(outline(&h, o).lead_item(), None);
}

#[test]
fn dragging_selected_items_drops_them_into_before_and_after_rows() {
    let mut h = Harness::new();
    let o = open_outline(&mut h);
    h.click(outline_row(&h, o, 2, 120.0)); // Cat
    h.messages();

    // The middle of "Birds": into it, after its children.
    let (from, to) = (outline_row(&h, o, 2, 120.0), in_row(&h, o, 4, 11.0));
    drag(&mut h, from, to);
    assert_eq!(
        outline_shape(&h, o),
        [
            "+Animals",
            "  +Mammals",
            "    Dog",
            "  +Birds",
            "    Owl",
            "    Cat",
            "+Plants",
            "  Fern"
        ]
    );
    let messages = h.messages();
    assert_eq!(
        values::<ItemsMoved>(&messages),
        [ItemsMoved {
            items: vec![2],
            parent: Some(4),
            index: 1
        }]
    );
    // The selection follows Cat to its new id.
    assert_eq!(outline(&h, o).selected_items(), [5]);
    assert_eq!(
        values::<RowSelectionChanged>(&messages)
            .last()
            .unwrap()
            .lead,
        Some(5)
    );

    // The top quarter of "Plants": before it, on the top level.
    let (from, to) = (outline_row(&h, o, 5, 120.0), in_row(&h, o, 6, 3.0));
    drag(&mut h, from, to);
    assert_eq!(
        outline_shape(&h, o),
        [
            "+Animals",
            "  +Mammals",
            "    Dog",
            "  +Birds",
            "    Owl",
            "Cat",
            "+Plants",
            "  Fern"
        ]
    );
    assert_eq!(
        values::<ItemsMoved>(&h.messages()),
        [ItemsMoved {
            items: vec![5],
            parent: None,
            index: 1
        }]
    );

    // The bottom quarter of an open item: its first child.
    h.click(outline_row(&h, o, 2, 120.0)); // Dog
    h.click_with(outline_row(&h, o, 5, 120.0), CMD); // Cat
    let (from, to) = (outline_row(&h, o, 5, 120.0), in_row(&h, o, 6, 19.0));
    drag(&mut h, from, to);
    assert_eq!(
        outline_shape(&h, o),
        [
            "+Animals",
            "  Mammals",
            "  +Birds",
            "    Owl",
            "+Plants",
            "  Dog",
            "  Cat",
            "  Fern"
        ]
    );
    assert_eq!(
        values::<ItemsMoved>(&h.messages()),
        [ItemsMoved {
            items: vec![2, 5],
            parent: Some(6),
            index: 0
        }]
    );
    assert_eq!(outline(&h, o).selected_items(), [5, 6]);
}

#[test]
fn items_are_not_dropped_into_themselves_and_escape_cancels_a_drag() {
    let mut h = Harness::new();
    let o = open_outline(&mut h);
    let before = outline_shape(&h, o);
    h.click(outline_row(&h, o, 0, 120.0)); // Animals
    h.messages();
    let (from, to) = (outline_row(&h, o, 0, 120.0), in_row(&h, o, 1, 11.0));
    drag(&mut h, from, to);
    assert_eq!(outline_shape(&h, o), before);
    assert!(values::<ItemsMoved>(&h.messages()).is_empty());

    h.click(outline_row(&h, o, 2, 120.0)); // Cat
    h.down(outline_row(&h, o, 2, 120.0));
    h.move_to(in_row(&h, o, 6, 11.0));
    h.key(Key::Escape);
    h.up(in_row(&h, o, 6, 11.0));
    assert_eq!(outline_shape(&h, o), before);
    assert!(values::<ItemsMoved>(&h.messages()).is_empty());

    h.click_with(outline_row(&h, o, 3, 120.0), CMD);
    assert_eq!(outline(&h, o).selected_items(), [2, 3]);
    h.click(outline_row(&h, o, 3, 120.0));
    assert_eq!(outline(&h, o).selected_items(), [3]);

    // Below the last row: the end of the top level.
    let (from, to) = (outline_row(&h, o, 3, 120.0), in_row(&h, o, 9, 11.0));
    drag(&mut h, from, to);
    assert_eq!(outline_shape(&h, o).last().unwrap(), "Dog");
}

#[test]
fn moves_from_code_refuse_moving_an_item_under_itself() {
    let mut h = Harness::new();
    let o = open_outline(&mut h);
    let moved = h
        .tree
        .update::<OutlineView, _>(o, |v, cx| v.move_items(cx, &[0], Some(2), 0));
    assert_eq!(moved, Some(false));
    h.tree
        .update::<OutlineView, _>(o, |v, cx| v.set_expanded(cx, 6, false));
    let moved = h
        .tree
        .update::<OutlineView, _>(o, |v, cx| v.move_items(cx, &[1, 2], Some(6), 5));
    assert_eq!(moved, Some(true));
    assert_eq!(
        outline_shape(&h, o),
        [
            "+Animals",
            "  +Birds",
            "    Owl",
            "+Plants",
            "  Fern",
            "  +Mammals",
            "    Cat",
            "    Dog"
        ]
    );
    let moved = h
        .tree
        .update::<OutlineView, _>(o, |v, cx| v.move_items(cx, &[4], Some(3), 0));
    assert_eq!(moved, Some(false));
}

#[test]
fn sidebar_items_move_within_the_sections_only() {
    let mut h = Harness::new();
    let items = vec![
        OutlineItem::new("Favorites")
            .child(OutlineItem::new("Desktop"))
            .child(OutlineItem::new("Inbox")),
        OutlineItem::new("Tags").child(OutlineItem::new("Red")),
    ];
    // Ids: 0 Favorites, 1 Desktop, 2 Inbox, 3 Tags, 4 Red.
    let s = h.add(
        OutlineView::new(items)
            .sidebar()
            .reorderable(true)
            .width(220.0),
    );
    let rh = 28.0;
    let at = |h: &Harness, pos: usize, y: f32| h.at(s, 60.0, pos as f32 * rh + y);
    h.click(at(&h, 4, 14.0)); // Red
    let (from, to) = (at(&h, 4, 14.0), at(&h, 0, 14.0));
    drag(&mut h, from, to);
    assert_eq!(
        outline_shape(&h, s),
        ["+Favorites", "  Red", "  Desktop", "  Inbox", "Tags"]
    );
    let moved = h
        .tree
        .update::<OutlineView, _>(s, |v, cx| v.move_items(cx, &[0], Some(4), 0));
    assert_eq!(moved, Some(false));
    let moved = h
        .tree
        .update::<OutlineView, _>(s, |v, cx| v.move_items(cx, &[1], None, 0));
    assert_eq!(moved, Some(false));
    h.tree.update::<OutlineView, _>(s, |v, cx| {
        v.insert_item(
            cx,
            None,
            9,
            OutlineItem::new("Shared").child(OutlineItem::new("Team")),
        )
    });
    h.prepare();
    assert_eq!(outline(&h, s).visible_items().len(), 7);
}

#[test]
fn new_items_keep_what_was_expanded_and_selected_where_their_paths_stay() {
    let mut h = Harness::new();
    let o = h.add(OutlineView::new(tree_items()).width(300.0));
    h.tree.update::<OutlineView, _>(o, |v, cx| {
        v.set_expanded(cx, 0, true);
        v.set_expanded(cx, 4, true);
        v.select_item(cx, Some(5)); // Owl
    });
    h.prepare();
    let items = vec![
        OutlineItem::new("Animals")
            .child(
                OutlineItem::new("Reptiles")
                    .expanded(true)
                    .child(OutlineItem::new("Gecko")),
            )
            .child(OutlineItem::new("Birds").child(OutlineItem::new("Owl"))),
        OutlineItem::new("Plants").child(OutlineItem::new("Fern")),
    ];
    h.tree
        .update::<OutlineView, _>(o, |v, cx| v.set_items(cx, items));
    h.prepare();
    assert_eq!(
        outline_shape(&h, o),
        [
            "+Animals",
            "  +Reptiles",
            "    Gecko",
            "  +Birds",
            "    Owl",
            "Plants",
            "  Fern"
        ]
    );
    assert_eq!(outline(&h, o).selected_items(), [4]);
    assert!(
        h.messages()
            .iter()
            .all(|m| m.downcast_ref::<ItemsMoved>().is_none())
    );
}

fn collection(h: &mut Harness, n: usize) -> WidgetId {
    let items = (0..n)
        .map(|i| CollectionItem::new(format!("Item {i}")))
        .collect();
    // 88-wide items with 12 gaps: (450 - 2 - 12) / 100 → 4 columns.
    h.add(CollectionView::new(items).width(450.0).height(300.0))
}

fn coll(h: &Harness, id: WidgetId) -> &CollectionView {
    h.tree.get::<CollectionView>(id).unwrap()
}

#[test]
fn collections_wrap_items_into_columns_and_draw_only_visible_rows() {
    let mut h = Harness::new();
    let before = h.layouts();
    let c = collection(&mut h, 10_000);
    assert_eq!(coll(&h, c).columns(), 4);
    let visible = coll(&h, c).visible_items();
    assert_eq!(visible.start, 0);
    // 300 px of 122-px rows: three rows of four.
    assert_eq!(visible.end, 12);
    assert_eq!(h.layouts() - before, 12);
    let mid = h.at(c, 200.0, 150.0);
    for _ in 0..20 {
        h.wheel(mid, Point::new(0.0, -1000.0));
    }
    assert!(coll(&h, c).visible_items().start > 500);
    assert!(h.layouts() - before <= 16);
}

#[test]
fn collection_arrows_move_by_item_and_row() {
    let mut h = Harness::new();
    let c = collection(&mut h, 10);
    // Item 1: second column, first row.
    let gap = (448.0 - 4.0 * 88.0) / 5.0;
    h.click(h.at(c, 1.0 + gap + 88.0 + gap + 40.0, 1.0 + 12.0 + 40.0));
    assert_eq!(coll(&h, c).selected_items(), [1]);
    h.key(Key::ArrowDown);
    assert_eq!(coll(&h, c).lead_item(), Some(5));
    h.key(Key::ArrowDown); // the last row has items 8, 9: to the last one
    assert_eq!(coll(&h, c).lead_item(), Some(9));
    h.key(Key::ArrowLeft);
    assert_eq!(coll(&h, c).lead_item(), Some(8));
    h.key_with(Key::ArrowUp, SHIFT);
    assert_eq!(coll(&h, c).selected_items(), [4, 5, 6, 7, 8]);
    h.key(Key::Home);
    assert_eq!(coll(&h, c).selected_items(), [0]);
    h.key(Key::ArrowRight);
    h.key(Key::ArrowRight);
    h.key(Key::ArrowDown);
    assert_eq!(coll(&h, c).lead_item(), Some(6));
    h.key(Key::ArrowDown);
    assert_eq!(coll(&h, c).lead_item(), Some(9));
}

#[test]
fn dragging_across_empty_space_selects_the_items_it_touches() {
    let mut h = Harness::new();
    let c = collection(&mut h, 10);
    // From the top-left gap to the middle of item 5 (second row, second column).
    let gap = (448.0 - 4.0 * 88.0) / 5.0;
    let start = h.at(c, 4.0, 4.0);
    h.down(start);
    h.move_to(h.at(c, 1.0 + 2.0 * gap + 88.0 + 30.0, 1.0 + 12.0 + 122.0 + 30.0));
    assert_eq!(coll(&h, c).selected_items(), [0, 1, 4, 5]);
    h.up(start);
    assert_eq!(coll(&h, c).selected_items(), [0, 1, 4, 5]);
    h.modifiers(CMD);
    h.down(h.at(c, 434.0, 4.0));
    h.move_to(h.at(c, 380.0, 60.0));
    h.up(h.at(c, 380.0, 60.0));
    h.modifiers(Modifiers::default());
    assert_eq!(coll(&h, c).selected_items(), [0, 1, 3, 4, 5]);
}

#[test]
fn replacing_collection_items_cancels_an_active_marquee() {
    let mut h = Harness::new();
    let c = collection(&mut h, 10);
    h.tree
        .update::<CollectionView, _>(c, |view, cx| view.select_items(cx, [5]));
    h.prepare();
    h.modifiers(CMD);
    h.down(h.at(c, 4.0, 4.0));
    assert_eq!(coll(&h, c).selected_items(), [5]);

    // Data can refresh while the pointer is held. The old selection contains row 5,
    // which no longer exists, and must not be reused by the next pointer move.
    h.tree.update::<CollectionView, _>(c, |view, cx| {
        view.set_items(cx, vec![CollectionItem::new("Replacement")]);
    });
    h.prepare();
    h.move_to(coll_item(&h, c, 0));
    h.up(coll_item(&h, c, 0));
    h.modifiers(Modifiers::default());
    assert!(coll(&h, c).selected_items().is_empty());
    h.click(coll_item(&h, c, 0));
    assert_eq!(coll(&h, c).selected_items(), [0]);
}

#[test]
fn replacing_collection_items_starts_a_new_click_sequence() {
    let mut h = Harness::new();
    let c = collection(&mut h, 2);
    h.click(coll_item(&h, c, 0));
    h.tree.update::<CollectionView, _>(c, |view, cx| {
        view.set_items(cx, vec![CollectionItem::new("Replacement")]);
    });
    h.prepare();
    h.messages();
    h.click(coll_item(&h, c, 0));
    assert!(values::<RowActivated>(&h.messages()).is_empty());
    h.click(coll_item(&h, c, 0));
    assert_eq!(values::<RowActivated>(&h.messages()), [RowActivated(0)]);
}

#[test]
fn invalid_collection_selection_ids_are_ignored_before_keyboard_navigation() {
    for mode in [SelectionMode::Single, SelectionMode::Multiple] {
        let mut h = Harness::new();
        let items = (0..3)
            .map(|i| CollectionItem::new(format!("Item {i}")))
            .collect();
        let c = h.add(CollectionView::new(items).selection_mode(mode).width(450.0));
        h.tree.update::<CollectionView, _>(c, |view, cx| {
            view.select_items(cx, [usize::MAX, 1, usize::MAX]);
        });
        h.prepare();
        assert_eq!(coll(&h, c).selected_items(), [1]);
        assert_eq!(coll(&h, c).lead_item(), Some(1));
        h.tree.set_focus(Some(c));
        h.key(Key::ArrowRight);
        assert_eq!(coll(&h, c).selected_items(), [2]);

        h.tree.update::<CollectionView, _>(c, |view, cx| {
            view.select_items(cx, [3, usize::MAX]);
        });
        h.prepare();
        assert!(coll(&h, c).selected_items().is_empty());
        assert_eq!(coll(&h, c).lead_item(), None);
        h.key(Key::ArrowRight);
        assert_eq!(coll(&h, c).selected_items(), [0]);
    }
}

fn coll_titles(h: &Harness, c: WidgetId) -> Vec<String> {
    coll(h, c)
        .items()
        .iter()
        .map(|i| i.title().to_owned())
        .collect()
}

/// The middle of collection item `i` (4 columns of 88 × 110 with 12-px rows apart).
fn coll_item(h: &Harness, c: WidgetId, i: usize) -> Point {
    let gap = (448.0 - 4.0 * 88.0) / 5.0;
    let (col, line) = ((i % 4) as f32, (i / 4) as f32);
    h.at(
        c,
        1.0 + gap + col * (88.0 + gap) + 44.0,
        1.0 + 12.0 + line * 122.0 + 50.0,
    )
}

#[test]
fn dragging_selected_items_of_a_collection_moves_them_where_the_line_shows() {
    let mut h = Harness::new();
    let items = (0..10)
        .map(|i| CollectionItem::new(format!("Item {i}")))
        .collect();
    let c = h.add(
        CollectionView::new(items)
            .reorderable(true)
            .width(450.0)
            .height(300.0),
    );
    h.click(coll_item(&h, c, 1));
    h.click_with(coll_item(&h, c, 2), CMD);
    h.messages();
    // Onto the left half of item 6: before it.
    let to = coll_item(&h, c, 6);
    let to = Point::new(to.x - 30.0, to.y);
    let from = coll_item(&h, c, 2);
    h.down(from);
    assert_eq!(coll(&h, c).selected_items(), [1, 2], "the press keeps both");
    h.move_to(Point::new(from.x + 10.0, from.y));
    h.move_to(to);
    h.up(to);
    assert_eq!(
        coll_titles(&h, c),
        [
            "Item 0", "Item 3", "Item 4", "Item 5", "Item 1", "Item 2", "Item 6", "Item 7",
            "Item 8", "Item 9"
        ]
    );
    let messages = h.messages();
    assert_eq!(
        values::<RowsMoved>(&messages),
        [RowsMoved {
            rows: vec![1, 2],
            to: 4
        }]
    );
    assert_eq!(coll(&h, c).selected_items(), [4, 5]);
    assert_eq!(
        values::<RowSelectionChanged>(&messages)
            .last()
            .unwrap()
            .lead,
        Some(5)
    );

    h.click(coll_item(&h, c, 4));
    assert_eq!(coll(&h, c).selected_items(), [4]);
    let end = h.at(c, 400.0, 1.0 + 12.0 + 2.0 * 122.0 + 50.0);
    let from = coll_item(&h, c, 4);
    h.down(from);
    h.move_to(end);
    h.up(end);
    assert_eq!(coll_titles(&h, c).last().unwrap(), "Item 1");
    assert_eq!(coll(&h, c).selected_items(), [9]);

    h.messages();
    let before = coll_titles(&h, c);
    let from = coll_item(&h, c, 9);
    h.down(from);
    h.move_to(coll_item(&h, c, 0));
    h.key(Key::Escape);
    h.up(coll_item(&h, c, 0));
    assert_eq!(coll_titles(&h, c), before);
    assert!(values::<RowsMoved>(&h.messages()).is_empty());
}

#[test]
fn collection_items_move_from_code_and_a_rename_follows_them() {
    let mut h = Harness::new();
    let items = (0..5)
        .map(|i| CollectionItem::new(format!("Item {i}")))
        .collect();
    let c = h.add(CollectionView::new(items).renamable(true).width(450.0));
    h.tree.update::<CollectionView, _>(c, |v, cx| {
        v.select_items(cx, [3]);
        v.rename_item(cx, 3);
    });
    let moved = h
        .tree
        .update::<CollectionView, _>(c, |v, cx| v.move_items(cx, &[3, 4], 0));
    assert_eq!(moved, Some(true));
    assert_eq!(
        coll_titles(&h, c),
        ["Item 3", "Item 4", "Item 0", "Item 1", "Item 2"]
    );
    assert_eq!(coll(&h, c).selected_items(), [0]);
    assert_eq!(coll(&h, c).renaming(), Some(0));
    let moved = h
        .tree
        .update::<CollectionView, _>(c, |v, cx| v.move_items(cx, &[0, 1], 0));
    assert_eq!(moved, Some(false));
    h.click(coll_item(&h, c, 2));
    h.down(coll_item(&h, c, 2));
    h.move_to(coll_item(&h, c, 0));
    h.up(coll_item(&h, c, 0));
    assert_eq!(coll_titles(&h, c)[2], "Item 0");
}

fn browser(h: &Harness, id: WidgetId) -> &BrowserView {
    h.tree.get::<BrowserView>(id).unwrap()
}

#[test]
fn browsers_list_the_children_of_the_selection_in_the_next_column() {
    let mut h = Harness::new();
    let b = h.add(
        BrowserView::new(tree_items())
            .width(400.0)
            .column_width(150.0),
    );
    assert_eq!(browser(&h, b).column_count(), 1);
    assert_eq!(browser(&h, b).column_items(0), [0, 6]);
    let item = |h: &Harness, column: usize, row: usize| {
        h.at(
            b,
            1.0 + column as f32 * 150.0 + 40.0,
            1.0 + row as f32 * 22.0 + 11.0,
        )
    };
    h.click(item(&h, 0, 0)); // Animals
    assert_eq!(browser(&h, b).column_count(), 2);
    assert_eq!(browser(&h, b).column_items(1), [1, 4]);
    h.click(item(&h, 1, 0)); // Mammals
    assert_eq!(browser(&h, b).path(), [0, 1]);
    assert_eq!(browser(&h, b).column_count(), 3);
    assert_eq!(
        values::<BrowserPathChanged>(&h.messages()).last(),
        Some(&BrowserPathChanged(vec![0, 1]))
    );
    // Three 150-px columns in 398 px: scrolled 52 px so the last is in view, which
    // puts the third column at 249..399 (unscrolled, x = 260 is in the second).
    h.click(h.at(b, 260.0, 11.0)); // Cat, in the third column
    assert_eq!(browser(&h, b).path(), [0, 1, 2]);
    assert_eq!(browser(&h, b).column_count(), 3);

    h.key(Key::ArrowDown);
    assert_eq!(browser(&h, b).path(), [0, 1, 3]);
    h.key(Key::ArrowLeft);
    h.key(Key::ArrowDown);
    assert_eq!(browser(&h, b).path(), [0, 4]);
    h.key(Key::ArrowRight);
    assert_eq!(browser(&h, b).path(), [0, 4, 5]);
    h.messages();
    h.key(Key::Enter);
    assert_eq!(values::<RowActivated>(&h.messages()), [RowActivated(5)]);
}

fn fields() -> Vec<RuleField> {
    vec![RuleField::text("Name"), RuleField::number("Size")]
}

fn rules(h: &Harness, id: WidgetId) -> Vec<Rule> {
    h.tree.get::<RuleEditor>(id).unwrap().rules()
}

#[test]
fn rule_rows_are_added_removed_and_typed_into() {
    let mut h = Harness::new();
    let e = h.add(RuleEditor::new(fields(), 560.0));
    assert_eq!(
        rules(&h, e),
        [Rule {
            field: 0,
            operator: RuleOperator::Contains,
            value: String::new()
        }]
    );
    let height = h.rect(e).height;
    // The + of the first rule row, at the right end.
    let row_y = |r: usize| 4.0 + (r + 1) as f32 * 36.0 + 18.0;
    h.click(h.at(e, 560.0 - 12.0 - 10.0, row_y(0)));
    assert_eq!(rules(&h, e).len(), 2);
    assert_eq!(h.rect(e).height, height + 36.0, "the editor grows");
    h.type_text("cat");
    assert_eq!(rules(&h, e)[1].value, "cat");
    h.key_with(Key::Tab, SHIFT);
    h.type_text("x");
    assert_eq!(rules(&h, e)[0].value, "x");
    assert!(values::<RulesChanged>(&h.messages()).len() >= 3);
    h.click(h.at(e, 560.0 - 12.0 - 20.0 - 8.0 - 10.0, row_y(0)));
    assert_eq!(rules(&h, e).len(), 1);
    assert_eq!(rules(&h, e)[0].value, "cat");
    h.click(h.at(e, 560.0 - 12.0 - 20.0 - 8.0 - 10.0, row_y(0)));
    assert_eq!(rules(&h, e).len(), 1);
}

/// Reconversion in a rule's value: the input method selects the word it converts
/// again, and the result takes its place.
#[test]
fn a_rule_value_can_be_converted_again() {
    let mut h = Harness::new();
    let e = h.add(RuleEditor::new(fields(), 560.0));
    h.tree.set_focus(Some(e));
    h.type_text("にほんご です");
    let context = h
        .tree
        .ime_context(&h.text)
        .expect("the value field answers");
    assert_eq!(context.text, "にほんご です");
    let word = context.base..context.base + "にほんご".len();
    h.tree.dispatch(Event::ImeSelect(word), &mut h.text);
    h.prepare();
    h.tree.dispatch(
        Event::Composition {
            text: "日本語".into(),
            cursor: None,
        },
        &mut h.text,
    );
    h.prepare();
    h.type_text("日本語");
    assert_eq!(rules(&h, e)[0].value, "日本語 です");
}

#[test]
fn rule_popups_choose_fields_operators_by_kind_and_all_or_any() {
    let mut h = Harness::new();
    let e = h.add(RuleEditor::new(fields(), 560.0));
    let row_y = 4.0 + 36.0 + 18.0;
    h.click(h.at(e, 24.0 + 30.0, row_y));
    assert!(h.open_menu().is_some());
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert!(h.open_menu().is_none());
    assert_eq!(rules(&h, e)[0].field, 1);
    assert_eq!(rules(&h, e)[0].operator, RuleOperator::Equal);
    // Operator popup → "is less than" (third numeric operator).
    h.click(h.at(e, 24.0 + 130.0 + 8.0 + 30.0, row_y));
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(rules(&h, e)[0].operator, RuleOperator::Less);
    h.click(h.at(e, 330.0, row_y));
    h.type_text("100");
    let editor = h.tree.get::<RuleEditor>(e).unwrap();
    let cell = |size: &'static str| move |f: usize| Cow::Borrowed(if f == 1 { size } else { "a" });
    assert!(editor.matches(cell("99")));
    assert!(!editor.matches(cell("100")));
    assert!(!editor.matches(cell("n/a")));

    h.click(h.at(e, 12.0 + 48.0 + 20.0, 4.0 + 18.0));
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(
        h.tree.get::<RuleEditor>(e).unwrap().match_kind(),
        RuleMatch::Any
    );
}

#[test]
fn all_needs_every_rule_and_any_needs_one() {
    let two = [
        Rule {
            field: 0,
            operator: RuleOperator::BeginsWith,
            value: "f".into(),
        },
        Rule {
            field: 1,
            operator: RuleOperator::Greater,
            value: "10".into(),
        },
    ];
    let record = |name: &'static str, size: &'static str| {
        move |f: usize| Cow::Borrowed(if f == 0 { name } else { size })
    };
    let all = RuleEditor::new(fields(), 500.0).rules_from(two.clone());
    assert!(all.matches(record("file", "20")));
    assert!(!all.matches(record("file", "5")));
    let any = RuleEditor::new(fields(), 500.0)
        .rules_from(two)
        .matching(RuleMatch::Any);
    assert!(any.matches(record("file", "5")));
    assert!(any.matches(record("doc", "20")));
    assert!(!any.matches(record("doc", "5")));
}

#[test]
fn rules_compare_text_without_case_and_ignore_empty_values() {
    let rule = |operator, value: &str| Rule {
        field: 0,
        operator,
        value: value.into(),
    };
    assert!(rule(RuleOperator::Contains, "OW").test("Owl"));
    assert!(rule(RuleOperator::BeginsWith, "ow").test("Owl"));
    assert!(!rule(RuleOperator::EndsWith, "ow").test("Owl"));
    assert!(rule(RuleOperator::Is, "owl").test("OWL"));
    assert!(rule(RuleOperator::IsNot, "cat").test("owl"));
    assert!(rule(RuleOperator::Greater, "2.5").test("3"));
    assert!(rule(RuleOperator::NotEqual, "3").test("4"));
    assert!(rule(RuleOperator::Greater, "1000").test("1,024 KB"));
    assert!(!rule(RuleOperator::Less, "5").test("n/a"));
    assert!(rule(RuleOperator::Contains, "  ").test("anything"));
}

#[test]
fn rule_editors_without_fields_have_no_editable_rows() {
    let mut h = Harness::new();
    let e = h.add(RuleEditor::new(Vec::new(), 500.0));
    assert!(rules(&h, e).is_empty());
    assert!(
        h.tree
            .get::<RuleEditor>(e)
            .unwrap()
            .text_input_area()
            .is_none()
    );
    h.tree.set_focus(Some(e));
    h.type_text("ignored");
    h.key(Key::Tab);
    assert!(rules(&h, e).is_empty());
    h.tree.accessibility_update(1.0);
}

#[test]
fn rule_editors_ignore_rules_for_unknown_fields() {
    let mut h = Harness::new();
    let valid = Rule {
        field: 1,
        operator: RuleOperator::Greater,
        value: "10".into(),
    };
    let invalid = Rule {
        field: usize::MAX,
        operator: RuleOperator::Contains,
        value: "bad".into(),
    };
    let e = h.add(RuleEditor::new(fields(), 500.0).rules_from([invalid.clone(), valid.clone()]));
    assert_eq!(rules(&h, e), std::slice::from_ref(&valid));
    h.tree.update::<RuleEditor, _>(e, |editor, cx| {
        editor.set_rules(cx, vec![invalid.clone()]);
    });
    h.prepare();
    assert_eq!(
        rules(&h, e),
        std::slice::from_ref(&valid),
        "no valid rules keeps the current rows"
    );
    h.tree.update::<RuleEditor, _>(e, |editor, cx| {
        editor.set_rules(cx, vec![valid.clone(), invalid, valid.clone()]);
    });
    h.prepare();
    assert_eq!(rules(&h, e), [valid.clone(), valid]);
}

#[test]
fn replacing_rules_releases_old_text_layouts_on_paint_and_removal() {
    let mut h = Harness::new();
    let before = h.layouts();
    let rule = |value: String| Rule {
        field: 0,
        operator: RuleOperator::Contains,
        value,
    };
    let e = h.add(RuleEditor::new(fields(), 500.0).rules_from([rule("original".into())]));
    let live = h.layouts();
    for i in 0..10 {
        h.tree.update::<RuleEditor, _>(e, |editor, cx| {
            editor.set_rules(cx, vec![rule(format!("replacement {i}"))]);
        });
        h.prepare();
        assert_eq!(
            h.layouts(),
            live,
            "replacing a row must release its old layouts"
        );
    }
    // Removal can happen before the next paint has released the retired row.
    h.tree.update::<RuleEditor, _>(e, |editor, cx| {
        editor.set_rules(cx, vec![rule("not painted".into())]);
    });
    h.tree.remove(e, &mut h.text);
    h.prepare();
    assert_eq!(h.layouts(), before);
}

#[test]
fn replacing_rules_ignores_choices_from_a_popup_for_the_old_rows() {
    let mut h = Harness::new();
    let row = Rule {
        field: 0,
        operator: RuleOperator::Contains,
        value: "old".into(),
    };
    let e = h.add(RuleEditor::new(fields(), 560.0).rules_from([row.clone(), row]));
    let second_row_y = 4.0 + 2.0 * 36.0 + 18.0;
    h.click(h.at(e, 54.0, second_row_y));
    assert!(h.open_menu().is_some());

    let replacement = Rule {
        field: 0,
        operator: RuleOperator::Contains,
        value: "replacement".into(),
    };
    h.tree.update::<RuleEditor, _>(e, |editor, cx| {
        editor.set_rules(cx, vec![replacement.clone()]);
    });
    h.prepare();
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert!(h.open_menu().is_none());
    assert_eq!(rules(&h, e), [replacement]);
}

#[test]
fn removing_a_rule_before_the_active_row_preserves_its_focus() {
    let mut h = Harness::new();
    let rows = (0..3).map(|i| Rule {
        field: 0,
        operator: RuleOperator::Contains,
        value: format!("rule {i}"),
    });
    let e = h.add(RuleEditor::new(fields(), 560.0).rules_from(rows));
    h.tree.set_focus(Some(e));
    h.key(Key::Tab);
    h.key(Key::Tab);
    assert_eq!(h.tree.get::<RuleEditor>(e).unwrap().active_row(), 2);
    h.click(h.at(e, 560.0 - 12.0 - 20.0 - 8.0 - 10.0, 4.0 + 36.0 + 18.0));
    assert_eq!(h.tree.get::<RuleEditor>(e).unwrap().active_row(), 1);
    assert_eq!(rules(&h, e)[1].value, "rule 2");
}

#[test]
fn rule_editor_cancels_composition_when_focus_leaves() {
    let mut h = Harness::new();
    let e = h.add(RuleEditor::new(fields(), 560.0).rules_from([Rule {
        field: 0,
        operator: RuleOperator::Contains,
        value: "ab".into(),
    }]));
    let other = h.add(noa_gpui_core::widgets::Button::new("Other"));
    h.tree.set_focus(Some(e));
    h.key_with(Key::Char('a'), CMD);
    h.tree.dispatch(
        Event::Composition {
            text: "仮".into(),
            cursor: Some(("仮".len(), "仮".len())),
        },
        &mut h.text,
    );
    h.prepare();
    assert!(all_drawn_texts(&h).iter().any(|text| text == "仮"));
    h.messages();

    h.tree.set_focus(Some(other));
    h.prepare();
    assert!(all_drawn_texts(&h).iter().all(|text| !text.contains('仮')));
    assert_eq!(rules(&h, e)[0].value, "ab");
    assert!(values::<RulesChanged>(&h.messages()).is_empty());
    h.tree.set_focus(Some(e));
    h.key(Key::Home);
    h.type_text("z");
    assert_eq!(rules(&h, e)[0].value, "zab");
}

#[test]
fn changing_rule_rows_cancels_the_previous_rows_composition() {
    for add_row in [false, true] {
        let mut h = Harness::new();
        let rows = ["ab", "cd"].map(|value| Rule {
            field: 0,
            operator: RuleOperator::Contains,
            value: value.into(),
        });
        let e = h.add(RuleEditor::new(fields(), 560.0).rules_from(rows));
        h.tree.set_focus(Some(e));
        h.tree.dispatch(
            Event::Composition {
                text: "仮".into(),
                cursor: Some(("仮".len(), "仮".len())),
            },
            &mut h.text,
        );
        h.prepare();
        assert!(all_drawn_texts(&h).iter().any(|text| text == "ab仮"));

        if add_row {
            h.click(h.at(e, 560.0 - 12.0 - 10.0, 4.0 + 36.0 + 18.0));
        } else {
            h.click(h.at(e, 330.0, 4.0 + 2.0 * 36.0 + 18.0));
        }
        assert_eq!(h.tree.get::<RuleEditor>(e).unwrap().active_row(), 1);
        assert!(all_drawn_texts(&h).iter().all(|text| !text.contains('仮')));
        assert_eq!(rules(&h, e)[0].value, "ab");
        h.key(Key::Home);
        h.type_text("x");
        assert_eq!(rules(&h, e)[1].value, if add_row { "x" } else { "xcd" });
        h.key_with(Key::Tab, SHIFT);
        h.key(Key::Home);
        h.type_text("z");
        assert_eq!(rules(&h, e)[0].value, "zab");
    }
}

#[test]
fn rule_editor_leaves_tab_with_the_ime_while_composing() {
    let mut h = Harness::new();
    let rows = ["ab", "cd"].map(|value| Rule {
        field: 0,
        operator: RuleOperator::Contains,
        value: value.into(),
    });
    let e = h.add(RuleEditor::new(fields(), 560.0).rules_from(rows));
    h.tree.set_focus(Some(e));
    h.tree.dispatch(
        Event::Composition {
            text: "仮".into(),
            cursor: Some(("仮".len(), "仮".len())),
        },
        &mut h.text,
    );
    h.prepare();
    for modifiers in [Modifiers::default(), SHIFT] {
        h.key_with(Key::Tab, modifiers);
        assert_eq!(h.tree.get::<RuleEditor>(e).unwrap().active_row(), 0);
        assert_eq!(h.tree.focused(), Some(e));
    }
    h.type_text("確");
    assert_eq!(rules(&h, e)[0].value, "ab確");
    assert_eq!(rules(&h, e)[1].value, "cd");
    h.key(Key::Tab);
    assert_eq!(h.tree.get::<RuleEditor>(e).unwrap().active_row(), 1);
}

const ELLIPSIS: char = '\u{2026}';
const LONG_NAME: &str = "Quarterly report for the board, final version.pdf";

struct Files;

impl TableData for Files {
    fn row_count(&self) -> usize {
        2
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        match (row, column) {
            (0, _) => LONG_NAME.into(),
            _ => "a.txt".into(),
        }
    }
}

fn all_drawn_texts(h: &Harness) -> Vec<String> {
    h.tree
        .display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { layout, .. } => Some(
                h.text
                    .layouts()
                    .buffer(*layout)
                    .lines
                    .iter()
                    .map(|l| l.text())
                    .collect::<String>(),
            ),
            _ => None,
        })
        .collect()
}

#[test]
fn cells_too_long_for_their_column_end_with_an_ellipsis_and_show_it_whole_in_a_tooltip() {
    let mut h = Harness::new();
    let table = h.add(
        TableView::new(
            vec![
                TableColumn::new("Tail", 120.0),
                TableColumn::new("Middle", 120.0).truncation(Truncation::Middle),
                TableColumn::new("Head", 120.0).truncation(Truncation::Head),
                TableColumn::new("Cut", 120.0).truncation(Truncation::None),
            ],
            Files,
        )
        .width(600.0)
        .height(TEN_ROWS),
    );
    let texts = all_drawn_texts(&h);
    let tail = texts
        .iter()
        .find(|t| t.starts_with("Quarterly") && t.ends_with(ELLIPSIS))
        .expect("the tail is cut");
    assert!(tail.chars().count() < LONG_NAME.chars().count());
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with("Quarter") && t.ends_with(".pdf") && t.contains(ELLIPSIS)),
        "the middle is cut: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with(ELLIPSIS) && t.ends_with(".pdf")),
        "the head is cut: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == LONG_NAME),
        "Truncation::None draws it whole (clipped): {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "a.txt"),
        "short text is left alone"
    );

    let view = get(&h, table);
    let row0 = TABLE_HEADER + TABLE_ROW / 2.0;
    let row1 = row0 + TABLE_ROW;
    assert_eq!(
        view.tooltip(Point::new(60.0, row0)).as_deref(),
        Some(LONG_NAME)
    );
    assert_eq!(
        view.tooltip(Point::new(180.0, row0)).as_deref(),
        Some(LONG_NAME)
    );
    assert_eq!(view.tooltip(Point::new(60.0, row1)), None);
    assert_eq!(view.tooltip(Point::new(420.0, row0)), None);
    assert_eq!(view.tooltip(Point::new(60.0, TABLE_HEADER / 2.0)), None);
}

#[test]
fn widening_a_column_shows_more_of_a_shortened_cell_until_it_is_whole() {
    let mut h = Harness::new();
    let table = h.add(
        TableView::new(vec![TableColumn::new("Name", 120.0)], Files)
            .width(700.0)
            .height(TEN_ROWS),
    );
    let shown = |h: &Harness| {
        all_drawn_texts(h)
            .into_iter()
            .find(|t| t.starts_with("Quarterly"))
            .unwrap()
    };
    let narrow = shown(&h);
    assert!(narrow.ends_with(ELLIPSIS));
    // Drag the header's right edge 120 px wider, then far enough for the whole name.
    let edge = h.at(table, 120.0, TABLE_HEADER / 2.0);
    h.down(edge);
    h.move_to(Point::new(edge.x + 120.0, edge.y));
    let wider = shown(&h);
    assert!(wider.ends_with(ELLIPSIS));
    assert!(
        wider.chars().count() > narrow.chars().count(),
        "{narrow} / {wider}"
    );
    h.move_to(Point::new(edge.x + 560.0, edge.y));
    h.up(Point::new(edge.x + 560.0, edge.y));
    assert_eq!(shown(&h), LONG_NAME);
    let row0 = TABLE_HEADER + TABLE_ROW / 2.0;
    assert_eq!(get(&h, table).tooltip(Point::new(60.0, row0)), None);
}

fn drawn_text_sizes(h: &Harness) -> Vec<(String, Size)> {
    let sizes = h
        .tree
        .display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { layout, .. } => Some(h.text.size(*layout)),
            _ => None,
        });
    all_drawn_texts(h).into_iter().zip(sizes).collect()
}

#[test]
fn a_resized_column_shortens_its_cells_as_a_table_made_that_wide_does() {
    let columns = |width| {
        vec![
            TableColumn::new("Tail", width),
            TableColumn::new("Middle", 120.0).truncation(Truncation::Middle),
            TableColumn::new("Head", 120.0).truncation(Truncation::Head),
        ]
    };
    let mut h = Harness::new();
    let table = h.add(
        TableView::new(columns(120.0), Files)
            .width(700.0)
            .height(TEN_ROWS),
    );
    let edge = h.at(table, 120.0, TABLE_HEADER / 2.0);
    h.down(edge);
    for dx in [-40.0, -37.0, 25.0, 61.5, 140.0, -40.0, 25.0, 0.0] {
        h.move_to(Point::new(edge.x + dx, edge.y));
        let width = get(&h, table).column_width(0);
        let mut fresh = Harness::new();
        fresh.add(
            TableView::new(columns(width), Files)
                .width(700.0)
                .height(TEN_ROWS),
        );
        assert_eq!(
            drawn_text_sizes(&h),
            drawn_text_sizes(&fresh),
            "{width} px wide"
        );
    }
    h.up(edge);
}

#[test]
fn resizing_a_column_shapes_only_the_candidates_for_a_shortened_cell() {
    let mut h = Harness::new();
    let table = h.add(
        TableView::new(vec![TableColumn::new("Name", 120.0)], Files)
            .width(700.0)
            .height(TEN_ROWS),
    );
    let edge = h.at(table, 120.0, TABLE_HEADER / 2.0);
    h.down(edge);
    let graphemes = LONG_NAME.chars().count();
    // A binary search over how many letters to keep tries at most this many.
    let candidates = (usize::BITS - graphemes.leading_zeros()) as u64;
    for dx in 1..=12 {
        let revision = h.text.layouts().changes();
        h.move_to(Point::new(edge.x + dx as f32 * 7.0, edge.y));
        // Each candidate made and dropped, but for the one drawn, which replaces the
        // cell's last: the whole name is not shaped again, nor the shortened one.
        let changes = h.text.layouts().changes() - revision;
        assert!(
            changes <= 2 * candidates,
            "{changes} layouts made or dropped (at most {} candidates)",
            candidates
        );
    }
    h.up(edge);
}

#[test]
fn resized_middle_and_head_columns_shorten_their_cells_as_tables_made_that_wide_do() {
    let columns = |middle, head| {
        vec![
            TableColumn::new("Tail", 120.0),
            TableColumn::new("Middle", middle).truncation(Truncation::Middle),
            TableColumn::new("Head", head).truncation(Truncation::Head),
        ]
    };
    for column in [1, 2] {
        let mut h = Harness::new();
        let table = h.add(
            TableView::new(columns(120.0, 120.0), Files)
                .width(700.0)
                .height(TEN_ROWS),
        );
        let edge = h.at(table, 120.0 * (column + 1) as f32, TABLE_HEADER / 2.0);
        h.down(edge);
        for dx in [
            -40.0, -39.0, -38.0, 30.0, -39.0, 61.5, -40.0, 30.0, 300.0, -38.0,
        ] {
            h.move_to(Point::new(edge.x + dx, edge.y));
            let view = get(&h, table);
            let (middle, head) = (view.column_width(1), view.column_width(2));
            let mut fresh = Harness::new();
            fresh.add(
                TableView::new(columns(middle, head), Files)
                    .width(700.0)
                    .height(TEN_ROWS),
            );
            assert_eq!(
                drawn_text_sizes(&h),
                drawn_text_sizes(&fresh),
                "column {column}: {middle} / {head} px wide"
            );
        }
        h.up(edge);
    }
}

#[test]
fn resizing_a_column_to_widths_it_had_shapes_no_candidates_again() {
    let mut h = Harness::new();
    let table = h.add(
        TableView::new(vec![TableColumn::new("Name", 120.0)], Files)
            .width(700.0)
            .height(TEN_ROWS),
    );
    let edge = h.at(table, 120.0, TABLE_HEADER / 2.0);
    h.down(edge);
    let sweep = |h: &mut Harness| {
        (1..=12)
            .map(|dx| {
                let revision = h.text.layouts().changes();
                h.move_to(Point::new(edge.x + dx as f32 * 7.0, edge.y));
                h.text.layouts().changes() - revision
            })
            .collect::<Vec<_>>()
    };
    let first = sweep(&mut h);
    h.move_to(edge);
    // The second time, the candidates' sizes are known: at most the drawn text is
    // replaced (its layout made and the last one dropped).
    let again = sweep(&mut h);
    assert!(again.iter().all(|&changes| changes <= 2), "{again:?}");
    assert!(
        again.iter().sum::<u64>() < first.iter().sum::<u64>(),
        "{first:?} / {again:?}"
    );
    h.up(edge);
}

/// Let the tree run animation frames for `seconds`, 60 a second, starting at `from`.
fn run_frames(h: &mut Harness, from: f64, seconds: f64) -> f64 {
    let mut now = from;
    while now < from + seconds {
        now += 1.0 / 60.0;
        h.tree.animate(now, &mut h.text);
        h.prepare();
    }
    now
}

#[test]
fn dragging_rows_past_the_bottom_scrolls_and_drops_them_below_what_was_on_screen() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(columns(), Generated::new(100))
            .width(400.0)
            .height(TEN_ROWS)
            .reorderable(true),
    );
    h.tree.set_now(1.0);
    h.click(row(&h, t, 1));
    let from = row(&h, t, 1);
    h.down(from);
    h.move_to(Point::new(from.x, from.y + 20.0));
    let below = h.at(t, 60.0, TEN_ROWS + 10.0);
    h.move_to(below);
    let now = run_frames(&mut h, 1.0, 0.5);
    let scrolled = get(&h, t).scroll_offset();
    assert!(scrolled > 5.0 * TABLE_ROW, "scrolled {scrolled}");
    let middle = h.at(t, 60.0, TABLE_HEADER + 5.0 * TABLE_ROW);
    h.move_to(middle);
    run_frames(&mut h, now, 0.5);
    assert_eq!(get(&h, t).scroll_offset(), scrolled);
    h.up(middle);
    let rows = get(&h, t).displayed_rows();
    let moved_to = rows.iter().position(|&r| r == 1).unwrap();
    assert!(
        moved_to > 10,
        "dropped among the rows scrolled into view: {moved_to}"
    );
    let first_shown = (scrolled / TABLE_ROW).floor() as usize;
    assert!((first_shown..first_shown + 11).contains(&moved_to));
}

#[test]
fn dragging_rows_to_the_top_scrolls_back_and_stops_at_the_first_row() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(columns(), Generated::new(100))
            .width(400.0)
            .height(TEN_ROWS)
            .reorderable(true),
    );
    h.tree
        .update::<TableView, _>(t, |table, cx| table.scroll_to_row(cx, 30));
    h.prepare();
    assert!(get(&h, t).scroll_offset() > 15.0 * TABLE_ROW);
    h.tree.set_now(5.0);
    h.click(row(&h, t, 5));
    let from = row(&h, t, 5);
    h.down(from);
    h.move_to(Point::new(from.x, from.y - 20.0));
    h.move_to(h.at(t, 60.0, TABLE_HEADER + 4.0));
    run_frames(&mut h, 5.0, 3.0);
    assert_eq!(get(&h, t).scroll_offset(), 0.0);
    let top = h.at(t, 60.0, TABLE_HEADER + 4.0);
    h.up(top);
    let moved = get(&h, t).selected_rows()[0];
    assert_eq!(
        get(&h, t).displayed_rows()[0],
        moved,
        "dropped before the first row"
    );
}

#[test]
fn selecting_cells_by_dragging_past_the_bottom_extends_the_range_as_rows_scroll_in() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(columns(), Generated::new(100))
            .width(400.0)
            .height(TEN_ROWS)
            .cell_selection(true),
    );
    h.tree.set_now(1.0);
    let start = row(&h, t, 0);
    h.down(start);
    h.move_to(h.at(t, 60.0, TEN_ROWS + 30.0));
    run_frames(&mut h, 1.0, 0.5);
    let range = get(&h, t).selected_cells().expect("a range");
    assert!(range.rows.len() > 12, "{range:?}");
    h.up(h.at(t, 60.0, TEN_ROWS + 30.0));
    let before = get(&h, t).scroll_offset();
    let inside = row(&h, t, 3);
    h.down(inside);
    h.move_to(row(&h, t, 6));
    run_frames(&mut h, 2.0, 0.5);
    h.up(row(&h, t, 6));
    assert_eq!(get(&h, t).scroll_offset(), before);
}

/// Compose stacks listed as on a page: a group row per stack with its own Up and
/// Restart buttons, its containers with the ID under the name; the stopped stack is
/// set apart by a dashed line and its last container has no line.
struct Compose;

/// Name, ID and whether the row is a stack.
const COMPOSE: [(&str, &str, bool); 5] = [
    ("shop", "", true),
    ("shop-web-1", "3f9a1c2b", false),
    ("shop-db-1", "a71e0c55", false),
    ("blog", "", true),
    ("blog-app-1", "0c2d9e41", false),
];
const COMPOSE_ROW: f32 = 36.0;

impl TableData for Compose {
    fn row_count(&self) -> usize {
        COMPOSE.len()
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        Cow::Borrowed(if column == 0 { COMPOSE[row].0 } else { "" })
    }

    fn is_group_row(&self, row: usize) -> bool {
        COMPOSE[row].2
    }

    fn cell_detail(&self, row: usize, column: usize) -> Option<Cow<'_, str>> {
        let id = COMPOSE[row].1;
        (column == 0 && !id.is_empty()).then_some(Cow::Borrowed(id))
    }

    fn group_row_buttons(&self, _row: usize) -> Vec<CellButton> {
        vec![
            CellButton::new("up").label("Up"),
            CellButton::new("restart").label("Restart"),
        ]
    }

    fn row_separator(&self, row: usize) -> RowSeparator {
        match row {
            3 => RowSeparator::Dashed,
            4 => RowSeparator::None,
            _ => RowSeparator::Line,
        }
    }
}

fn compose(h: &mut Harness, configure: impl FnOnce(TableView) -> TableView) -> WidgetId {
    let columns = vec![
        TableColumn::new("Name", 120.0),
        TableColumn::new("State", 100.0),
    ];
    let table = TableView::new(columns, Compose)
        .width(400.0)
        .height(TABLE_HEADER + 5.0 * COMPOSE_ROW)
        .row_height(COMPOSE_ROW)
        .floats_group_rows(false);
    h.add(configure(table))
}

#[test]
fn a_table_without_its_frame_reaches_its_edges_on_its_own_background() {
    let page = Color::hex(0x202428);
    let mut h = Harness::new();
    let t = compose(&mut h, |t| t.bordered(false).background(page));
    let (bounds, color, border) = quads(&h)[0];
    assert_eq!((bounds.width, color, border.width), (400.0, page, 0.0));
    assert!(quads(&h).iter().filter(|(_, c, _)| *c == page).count() >= 2);
    let texts = drawn_texts(&h, t);
    let name = texts.iter().find(|(s, ..)| s == "shop-web-1").unwrap().1;
    assert_eq!(name.x, 8.0);

    let mut h = Harness::new();
    let t = compose(&mut h, |t| t);
    assert_eq!(quads(&h)[0].2.width, 1.0);
    let texts = drawn_texts(&h, t);
    let name = texts.iter().find(|(s, ..)| s == "shop-web-1").unwrap().1;
    assert_eq!(name.x, 1.0 + 8.0);
}

#[test]
fn row_separators_draw_a_line_dashed_or_none_under_each_row() {
    let lines = |h: &Harness| -> Vec<(Rect, Color, noa_gpui_scene::Border)> {
        quads(h)
            .into_iter()
            .filter(|(r, ..)| r.height == 1.0 && r.y > TABLE_HEADER)
            .collect()
    };
    let mut h = Harness::new();
    compose(&mut h, |t| t);
    assert!(lines(&h).is_empty());

    let mut h = Harness::new();
    compose(&mut h, |t| t.row_separators(true));
    let lines = lines(&h);
    let bottom = |pos: usize| TABLE_HEADER + (pos + 1) as f32 * COMPOSE_ROW - 1.0;
    for pos in 0..3 {
        let at = lines.iter().filter(|(r, ..)| r.y == bottom(pos)).count();
        assert_eq!(at, 1, "row {pos}");
    }
    let dashes: Vec<_> = lines.iter().filter(|(r, ..)| r.y == bottom(3)).collect();
    assert!(dashes.len() > 10 && dashes.iter().all(|(r, ..)| r.width <= 4.0));
    assert!(lines.iter().all(|(r, ..)| r.y != bottom(4)));
}

#[test]
fn a_cell_can_show_a_smaller_second_line_under_its_text() {
    let mut h = Harness::new();
    let t = compose(&mut h, |t| t);
    let texts = drawn_texts(&h, t);
    let find = |s: &str| texts.iter().find(|(t, ..)| t == s).unwrap().1;
    let (name, id) = (find("shop-web-1"), find("3f9a1c2b"));
    assert_eq!(id.x, name.x);
    assert!(id.y >= name.bottom() && id.height < name.height);
    let top = TABLE_HEADER + COMPOSE_ROW;
    let middle = top + COMPOSE_ROW / 2.0;
    assert!((((name.y + id.bottom()) / 2.0) - middle).abs() < 1.0);
    let stack = find("shop");
    assert!(((stack.y + stack.height / 2.0) - (TABLE_HEADER + COMPOSE_ROW / 2.0)).abs() < 0.5);
}

#[test]
fn a_group_row_reports_clicks_and_carries_buttons_free_of_the_columns() {
    let mut h = Harness::new();
    let t = compose(&mut h, |t| t);
    let texts = drawn_texts(&h, t);
    let label = |s: &str, nth: usize| {
        let r = texts.iter().filter(|(t, ..)| t == s).nth(nth).unwrap().1;
        (r.x + r.width / 2.0, r.y + r.height / 2.0)
    };
    // Both group rows carry the buttons, at the trailing edge past the two columns
    // (which end at 1 + 220).
    let restart = texts.iter().filter(|(s, ..)| s == "Restart").count();
    assert_eq!(restart, 2);
    let up = texts.iter().find(|(s, ..)| s == "Up").unwrap().1;
    assert!(up.x > 221.0, "{up:?}");

    h.click(h.at(t, 60.0, TABLE_HEADER + 3.5 * COMPOSE_ROW));
    h.click(h.at(t, 200.0, TABLE_HEADER + 0.5 * COMPOSE_ROW));
    let groups: Vec<usize> = values::<GroupRowClicked>(&h.messages())
        .into_iter()
        .map(|GroupRowClicked(row)| row)
        .collect();
    assert_eq!(groups, [3, 0]);
    assert_eq!(get(&h, t).selected_count(), 0);

    let (restart, up) = (label("Restart", 1), label("Up", 0));
    h.click(h.at(t, restart.0, restart.1));
    h.click(h.at(t, up.0, up.1));
    let messages = h.messages();
    assert!(values::<GroupRowClicked>(&messages).is_empty());
    let actions: Vec<(usize, String)> = values::<GroupActionClicked>(&messages)
        .into_iter()
        .map(|m| (m.row, m.action.into_owned()))
        .collect();
    assert_eq!(actions, [(3, "restart".into()), (0, "up".into())]);

    h.down(h.at(t, 60.0, TABLE_HEADER + 0.5 * COMPOSE_ROW));
    h.up(h.at(t, 60.0, TABLE_HEADER + 1.5 * COMPOSE_ROW));
    assert!(values::<GroupRowClicked>(&h.messages()).is_empty());
}

/// A stack listed as the Compose app shows it: a dot for its state and "2/3 running"
/// beside its name, a small Down button; under it its containers, a strong status tag
/// first and the name in the second column with its ID in monospace under it.
struct StackList;

/// Name, ID and whether the row is the stack.
const STACK_LIST: [(&str, &str, bool); 3] = [
    ("shop", "", true),
    ("shop-web-1", "3f9a1c2b", false),
    ("shop-db-1", "a71e0c55", false),
];
const RUNNING: Color = Color::hex(0x00a000);

impl TableData for StackList {
    fn row_count(&self) -> usize {
        STACK_LIST.len()
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        let (name, _, stack) = STACK_LIST[row];
        Cow::Borrowed(match (column, stack) {
            (0, true) => name,
            (0, false) => "running",
            (_, true) => "",
            (_, false) => name,
        })
    }

    fn is_group_row(&self, row: usize) -> bool {
        STACK_LIST[row].2
    }

    fn cell_look(&self, row: usize, column: usize) -> CellLook {
        if column == 0 && !STACK_LIST[row].2 {
            CellLook::StrongOutlinedBadge(Tone::Success)
        } else {
            CellLook::Text
        }
    }

    fn cell_detail(&self, row: usize, column: usize) -> Option<Cow<'_, str>> {
        let id = STACK_LIST[row].1;
        (column == 1 && !id.is_empty()).then_some(Cow::Borrowed(id))
    }

    fn cell_note(&self, row: usize, column: usize) -> Option<Cow<'_, str>> {
        (column == 0 && STACK_LIST[row].2).then_some(Cow::Borrowed("2/3 running"))
    }

    fn cell_dot(&self, row: usize, column: usize) -> Option<Tone> {
        (column == 0 && STACK_LIST[row].2).then_some(Tone::Success)
    }

    fn group_row_buttons(&self, _row: usize) -> Vec<CellButton> {
        vec![CellButton::new("down").label("Down").height(20.0)]
    }
}

fn stack_list(h: &mut Harness, configure: impl FnOnce(TableView) -> TableView) -> WidgetId {
    h.tree.set_theme(Theme::light().with_status_colors(
        RUNNING,
        Color::hex(0xc08000),
        Color::hex(0xc00000),
    ));
    let columns = vec![
        TableColumn::new("State", 100.0),
        TableColumn::new("Name", 160.0).detail_font_family("monospace"),
    ];
    let table = TableView::new(columns, StackList)
        .width(400.0)
        .height(TABLE_HEADER + 3.0 * COMPOSE_ROW)
        .row_height(COMPOSE_ROW)
        .floats_group_rows(false)
        .group_member_indent(36.0)
        .group_member_indent_column(1);
    h.add(configure(table))
}

fn text_color(h: &Harness, s: &str) -> Color {
    (h.tree.display_list().commands().iter())
        .find_map(|c| match c {
            DrawCommand::Text { layout, color, .. }
                if h.text.layouts().buffer(*layout).lines[0].text() == s =>
            {
                Some(*color)
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn a_cells_second_line_can_take_its_own_font() {
    let mut h = Harness::new();
    let t = stack_list(&mut h, |t| t);
    let texts = drawn_texts(&h, t);
    let named = |s: &str| texts.iter().find(|(t, ..)| t == s).unwrap().2;
    assert!(named("3f9a1c2b"), "the ID is monospaced");
    assert!(!named("shop-web-1"), "the name keeps the column's font");
}

#[test]
fn group_members_can_be_indented_in_another_column_than_the_first() {
    let mut h = Harness::new();
    let t = stack_list(&mut h, |t| t);
    let texts = drawn_texts(&h, t);
    let find = |s: &str| texts.iter().find(|(t, ..)| t == s).unwrap().1;
    // The name column starts at 1 + 100; its text after the padding and the indent,
    // with the ID under it lined up.
    assert_eq!(find("shop-web-1").x, 101.0 + 8.0 + 36.0);
    assert_eq!(find("3f9a1c2b").x, 101.0 + 8.0 + 36.0);
    let status = texts.iter().find(|(t, ..)| t == "running").unwrap().1;
    assert_eq!(status.x, 1.0 + 8.0 + 6.0);
}

#[test]
fn strong_outlined_badges_draw_their_frame_and_text_in_the_tone_itself() {
    let mut h = Harness::new();
    stack_list(&mut h, |t| t);
    let framed = (quads(&h).into_iter())
        .filter(|(_, fill, border)| border.width == 1.0 && border.color == RUNNING && fill.a < 1.0)
        .count();
    assert_eq!(framed, 2, "one tag per container");
    assert_eq!(text_color(&h, "running"), RUNNING);
}

#[test]
fn a_group_row_shows_a_dot_and_a_note_beside_its_heading() {
    for cells in [false, true] {
        let mut h = Harness::new();
        let t = stack_list(&mut h, |t| t.group_row_cells(cells));
        let texts = drawn_texts(&h, t);
        let find = |s: &str| texts.iter().find(|(t, ..)| t.starts_with(s)).unwrap();
        let (name, (note_text, note, _)) = (find("shop").1, find("2/3"));
        // The heading runs across the columns; as a cell, the note takes what room the
        // 100-wide column leaves and is shortened.
        assert_eq!(note_text.ends_with('…'), cells, "{note_text}");
        let note = *note;
        let origin = h.rect(t);
        let dot = (quads(&h).into_iter())
            .find(|(_, color, _)| *color == RUNNING)
            .map(|(r, ..)| r.translate(-origin.x, -origin.y))
            .unwrap();
        assert_eq!(
            (dot.x, dot.width, dot.height),
            (9.0, 8.0, 8.0),
            "cells: {cells}"
        );
        let middle = |r: Rect| r.y + r.height / 2.0;
        assert!((middle(dot) - middle(name)).abs() < 0.5);
        assert_eq!(name.x, dot.right() + 6.0);
        assert_eq!(note.x, name.right() + 6.0);
        assert!(note.height < name.height);
        assert!((middle(note) - middle(name)).abs() < 0.5);
        assert_eq!(text_color(&h, note_text), h.tree.theme().muted);
    }
}

#[test]
fn a_cell_button_can_be_lower_than_the_row() {
    let mut h = Harness::new();
    let t = stack_list(&mut h, |t| t);
    let down = drawn_texts(&h, t)
        .into_iter()
        .find(|(s, ..)| s == "Down")
        .unwrap()
        .1;
    let x = down.x + down.width / 2.0;
    // The button is 20 high in the middle of the 36-high row: 8 from its top.
    h.click(h.at(t, x, TABLE_HEADER + 4.0));
    let messages = h.messages();
    assert!(values::<GroupActionClicked>(&messages).is_empty());
    assert_eq!(values::<GroupRowClicked>(&messages), [GroupRowClicked(0)]);
    h.click(h.at(t, x, TABLE_HEADER + 10.0));
    let actions = values::<GroupActionClicked>(&h.messages());
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action, "down");
}

#[test]
fn the_cell_padding_follows_the_apps_spacing() {
    use noa_gpui_core::widgets::TABLE_CELL_PADDING;
    let mut h = Harness::new();
    let t = compose(&mut h, |t| {
        t.bordered(false).content_inset(16.0).cell_padding(12.0)
    });
    let texts = drawn_texts(&h, t);
    let find = |s: &str| texts.iter().find(|(t, ..)| t == s).unwrap().1;
    for s in ["Name", "shop-web-1", "3f9a1c2b", "shop"] {
        assert_eq!(find(s).x, 16.0 + 12.0, "{s}");
    }
    assert_eq!(find("State").x, 16.0 + 120.0 + 12.0);
    let restart = find("Restart");
    assert!(restart.right() <= 400.0 - 16.0 - 12.0, "{restart:?}");
    assert_eq!(TABLE_CELL_PADDING, 8.0);
}

#[test]
fn a_content_inset_moves_the_columns_in_from_the_sides_but_not_the_rows() {
    let mut h = Harness::new();
    let t = compose(&mut h, |t| {
        t.bordered(false)
            .content_inset(16.0)
            .highlights_hovered_row(true)
    });
    let texts = drawn_texts(&h, t);
    let find = |s: &str| texts.iter().find(|(t, ..)| t == s).unwrap().1;
    for s in ["Name", "shop-web-1", "3f9a1c2b", "shop"] {
        assert_eq!(find(s).x, 16.0 + 8.0, "{s}");
    }
    assert_eq!(find("State").x, 16.0 + 120.0 + 8.0);
    let restart = find("Restart");
    assert!(restart.right() <= 400.0 - 16.0 - 8.0, "{restart:?}");
    assert!(restart.right() > 400.0 - 16.0 - 8.0 - 20.0, "{restart:?}");
    h.click(h.at(t, 16.0 + 4.0, TABLE_HEADER + 1.5 * COMPOSE_ROW));
    assert_eq!(get(&h, t).selected_rows(), [1]);
    h.move_to(h.at(t, 200.0, TABLE_HEADER + 2.5 * COMPOSE_ROW));
    let origin = h.rect(t);
    let tint = (quads(&h).into_iter())
        .map(|(r, ..)| r.translate(-origin.x, -origin.y))
        .find(|r| r.y == TABLE_HEADER + 2.0 * COMPOSE_ROW && r.height == COMPOSE_ROW)
        .unwrap();
    assert_eq!((tint.x, tint.width), (0.0, 400.0));

    let mut h = Harness::new();
    let columns = vec![
        TableColumn::new("Name", 100.0).flex(1.0),
        TableColumn::new("Size", 100.0),
    ];
    let t = h.add(
        TableView::new(columns, Generated::new(3))
            .width(400.0)
            .bordered(false)
            .content_inset(16.0),
    );
    assert_eq!(get(&h, t).column_width(0), 400.0 - 32.0 - 100.0);
}

/// Rows of (name, kind), with a group row where the kind is empty.
struct Listing(Vec<(&'static str, &'static str)>);

impl TableData for Listing {
    fn row_count(&self) -> usize {
        self.0.len()
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        let (name, kind) = self.0[row];
        Cow::Borrowed(if column == 0 { name } else { kind })
    }

    fn is_group_row(&self, row: usize) -> bool {
        self.0[row].1.is_empty()
    }
}

fn file_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("Name", 160.0).sort(ColumnSort::Text),
        TableColumn::new("Kind", 120.0).sort(ColumnSort::Text),
    ]
}

fn listing() -> Listing {
    Listing(vec![
        ("Documents", ""),
        ("Report.pdf", "PDF"),
        ("Budget.xlsx", "Spreadsheet"),
        ("Notes.txt", "Text"),
        ("Pictures", ""),
        ("Beach.png", "Image"),
        ("Report cover.png", "Image"),
    ])
}

fn filter(h: &mut Harness, t: WidgetId, query: &str) {
    h.tree
        .update::<TableView, _>(t, |table, cx| table.set_filter_text(cx, query));
    h.prepare();
}

#[test]
fn filter_text_shows_rows_with_every_word_and_the_groups_over_them() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(file_columns(), listing())
            .width(400.0)
            .height(TEN_ROWS),
    );
    assert!(!get(&h, t).is_filtered());
    assert_eq!(get(&h, t).total_row_count(), 5);

    filter(&mut h, t, "REPORT");
    assert_eq!(get(&h, t).displayed_rows(), [0, 1, 4, 6]);
    assert_eq!(get(&h, t).matching_row_count(), 2);
    assert_eq!(
        get(&h, t).row_count(),
        4,
        "the two group rows stay over them"
    );
    assert!(get(&h, t).is_filtered());
    assert_eq!(get(&h, t).filter_text(), "REPORT");

    filter(&mut h, t, "report image");
    assert_eq!(get(&h, t).displayed_rows(), [4, 6], "the empty group goes");
    assert!(!get(&h, t).is_row_shown(1));
    assert!(get(&h, t).is_row_shown(6));

    filter(&mut h, t, "nothing like it");
    assert_eq!(get(&h, t).displayed_rows(), [] as [usize; 0]);
    assert_eq!(get(&h, t).matching_row_count(), 0);

    filter(&mut h, t, "  ");
    assert_eq!(get(&h, t).displayed_rows(), [0, 1, 2, 3, 4, 5, 6]);
    assert!(!get(&h, t).is_filtered());
}

#[test]
fn filtering_keeps_the_sort_unselects_hidden_rows_and_survives_new_data() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(columns(), Generated::new(30))
            .width(400.0)
            .height(TEN_ROWS),
    );
    // Sort by size, descending.
    h.click(h.at(t, 200.0, TABLE_HEADER / 2.0));
    h.click(h.at(t, 200.0, TABLE_HEADER / 2.0));
    h.tree
        .update::<TableView, _>(t, |table, cx| table.select_rows(cx, [1, 2, 4]));
    filter(&mut h, t, "green");
    let shown = get(&h, t).displayed_rows().to_vec();
    assert_eq!(shown.len(), 10);
    assert!(shown.iter().all(|r| r % 3 == 1), "{shown:?}");
    let sizes: Vec<usize> = shown.iter().map(|r| (r * 7919) % 1000).collect();
    assert!(
        sizes.windows(2).all(|w| w[0] >= w[1]),
        "still sorted: {sizes:?}"
    );
    assert_eq!(get(&h, t).selected_rows(), [1, 4], "row 2 is hidden");

    h.click(row(&h, t, 0));
    h.key_with(Key::Char('a'), CMD);
    assert_eq!(get(&h, t).selected_count(), 10);
    h.key_with(Key::Char('c'), CMD);
    let copied = h.tree.clipboard().read_text().unwrap_or_default();
    assert_eq!(copied.lines().count(), 10);
    assert!(copied.lines().all(|l| l.ends_with("Green")), "{copied}");

    h.tree
        .update::<TableView, _>(t, |table, cx| table.select_rows(cx, [0, 1]));
    assert_eq!(get(&h, t).selected_rows(), [1]);

    h.tree
        .update::<TableView, _>(t, |table, cx| table.set_data(cx, Generated::new(6)));
    h.prepare();
    assert_eq!(get(&h, t).displayed_rows().len(), 2);
    h.tree
        .update::<TableView, _>(t, |table, cx| table.clear_filter(cx));
    h.prepare();
    assert_eq!(get(&h, t).displayed_rows().len(), 6);
}

#[test]
fn a_condition_narrows_the_rows_with_the_text_and_columns_limit_the_search() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(file_columns(), listing())
            .width(400.0)
            .height(TEN_ROWS)
            .filter_columns([0]),
    );
    filter(&mut h, t, "image");
    assert_eq!(get(&h, t).matching_row_count(), 0);
    filter(&mut h, t, "re");
    assert_eq!(get(&h, t).displayed_rows(), [0, 1, 4, 6]);
    h.tree.update::<TableView, _>(t, |table, cx| {
        table.set_filter(cx, |data, row| data.cell(row, 1) == "Image")
    });
    h.prepare();
    assert_eq!(get(&h, t).displayed_rows(), [4, 6]);
    filter(&mut h, t, "");
    assert_eq!(
        get(&h, t).displayed_rows(),
        [4, 5, 6],
        "the condition alone"
    );
    assert!(get(&h, t).is_filtered());
    h.tree
        .update::<TableView, _>(t, |table, cx| table.clear_filter(cx));
    h.prepare();
    assert_eq!(get(&h, t).row_count(), 7);
}

#[test]
fn rows_dragged_while_filtered_leave_the_hidden_rows_in_their_places() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(columns(), Generated::new(9))
            .width(400.0)
            .height(TEN_ROWS)
            .reorderable(true),
    );
    // Rows 0, 3 and 6 are red.
    filter(&mut h, t, "red");
    assert_eq!(get(&h, t).displayed_rows(), [0, 3, 6]);
    h.click(row(&h, t, 0));
    let from = row(&h, t, 0);
    h.down(from);
    h.move_to(Point::new(from.x, from.y + 20.0));
    let to = h.at(t, 60.0, TABLE_HEADER + 3.0 * TABLE_ROW);
    h.move_to(to);
    h.up(to);
    assert_eq!(get(&h, t).displayed_rows(), [3, 6, 0]);
    assert_eq!(
        values::<RowsMoved>(&h.messages()),
        [RowsMoved {
            rows: vec![0],
            to: 2
        }]
    );
    filter(&mut h, t, "");
    assert_eq!(get(&h, t).displayed_rows(), [1, 2, 3, 4, 5, 6, 0, 7, 8]);
}

#[test]
fn the_letters_a_filter_matched_are_marked_in_the_cells() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(file_columns(), listing())
            .width(400.0)
            .height(TEN_ROWS),
    );
    let accent = h.tree.theme().accent.with_alpha(0.28);
    let marks = |h: &Harness| -> Vec<Rect> {
        quads(h)
            .into_iter()
            .filter(|(_, color, _)| *color == accent)
            .map(|(rect, _, _)| rect)
            .collect()
    };
    assert!(marks(&h).is_empty());
    filter(&mut h, t, "rep");
    // "Rep" in "Report.pdf" and in "Report cover.png".
    let found = marks(&h);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|r| r.width > 5.0 && r.width < 40.0));

    let t2 = h.add(
        TableView::new(file_columns(), listing())
            .width(400.0)
            .height(TEN_ROWS)
            .marks_filter_matches(false),
    );
    filter(&mut h, t2, "rep");
    assert_eq!(marks(&h).len(), 2, "only the first table marks them");
}

#[test]
fn a_rule_editors_matcher_filters_a_table_in_place() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(file_columns(), listing())
            .width(400.0)
            .height(TEN_ROWS),
    );
    let editor = RuleEditor::new(fields(), 500.0).rules_from([Rule {
        field: 1,
        operator: RuleOperator::Is,
        value: "image".into(),
    }]);
    let matcher = editor.matcher();
    assert_eq!(matcher.rules, editor.rules());
    h.tree
        .update::<TableView, _>(t, |table, cx| table.select_rows(cx, [5]));
    h.tree.update::<TableView, _>(t, |table, cx| {
        table.set_filter(cx, move |data, row| matcher.matches(|f| data.cell(row, f)))
    });
    h.prepare();
    assert_eq!(get(&h, t).displayed_rows(), [4, 5, 6]);
    assert_eq!(
        get(&h, t).selected_rows(),
        [5],
        "a shown row stays selected"
    );
}

#[test]
fn dragging_outline_items_past_the_bottom_scrolls_to_drop_them_further_down() {
    let mut h = Harness::new();
    let items = (0..40)
        .map(|i| OutlineItem::new(format!("Note {i}")))
        .collect();
    let o = h.add(
        OutlineView::new(items)
            .reorderable(true)
            .width(300.0)
            .height(10.0 * 22.0 + 2.0),
    );
    h.tree.set_now(1.0);
    h.click(outline_row(&h, o, 1, 120.0));
    let from = outline_row(&h, o, 1, 120.0);
    h.down(from);
    h.move_to(Point::new(from.x, from.y + 20.0));
    h.move_to(h.at(o, 120.0, 10.0 * 22.0 + 12.0));
    run_frames(&mut h, 1.0, 0.5);
    let first = outline(&h, o).visible_rows().start;
    assert!(first > 5, "scrolled to row {first}");
    // The top quarter of a row now on screen.
    let row = first + 4;
    let scrolled = outline(&h, o).scroll_offset();
    let to = in_row(&h, o, row, 3.0);
    let to = Point::new(to.x, to.y - scrolled);
    h.move_to(to);
    h.up(to);
    let titles: Vec<String> = outline(&h, o)
        .items()
        .iter()
        .map(|i| i.title().to_owned())
        .collect();
    let at = titles.iter().position(|t| t == "Note 1").unwrap();
    assert_eq!(at, row - 1, "before the row it was dropped on");
}

#[test]
fn a_table_exports_the_rows_it_shows_as_csv_with_a_header() {
    let mut h = Harness::new();
    let t = h.add(TableView::new(file_columns(), listing()).width(400.0));
    let csv = get(&h, t).export_text(TableText::Csv);
    assert_eq!(
        csv,
        "Name,Kind\nReport.pdf,PDF\nBudget.xlsx,Spreadsheet\nNotes.txt,Text\nBeach.png,Image\n\
         Report cover.png,Image"
    );
    filter(&mut h, t, "report");
    h.tree
        .update::<TableView, _>(t, |table, cx| table.sort_by(cx, 0, false));
    let text = get(&h, t).export_text(TableText::Tsv);
    assert_eq!(text, "Name\tKind\nReport.pdf\tPDF\nReport cover.png\tImage");
    let rows = TableText::Tsv.read(&text);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2], ["Report cover.png", "Image"]);
}

/// Name (always shown), Size and Kind, with the header's menu of columns.
fn chooser_table(h: &mut Harness) -> WidgetId {
    h.add(
        TableView::new(
            vec![
                TableColumn::new("Name", 160.0)
                    .sort(ColumnSort::Text)
                    .hideable(false),
                TableColumn::new("Size", 100.0).sort(ColumnSort::Number),
                TableColumn::new("Kind", 120.0),
            ],
            Generated::new(20),
        )
        .column_chooser(true)
        .width(400.0)
        .height(TEN_ROWS),
    )
}

fn right_click(h: &mut Harness, pos: Point) {
    let button = PointerButton::Secondary;
    h.tree
        .dispatch(Event::PointerDown { pos, button }, &mut h.text);
    h.tree
        .dispatch(Event::PointerUp { pos, button }, &mut h.text);
    h.prepare();
}

fn menu_items(h: &Harness) -> Vec<(String, bool, bool)> {
    let menu = h.open_menu().expect("a menu is open");
    h.tree
        .get::<Menu>(menu)
        .unwrap()
        .items()
        .iter()
        .map(|i| (i.label().to_owned(), i.is_checked(), i.is_enabled()))
        .collect()
}

#[test]
fn the_header_menu_hides_and_shows_columns() {
    let mut h = Harness::new();
    let t = chooser_table(&mut h);
    let p = row(&h, t, 2);
    right_click(&mut h, p);
    assert!(h.open_menu().is_none());
    let p = h.at(t, 200.0, TABLE_HEADER / 2.0);
    right_click(&mut h, p);
    assert_eq!(
        menu_items(&h),
        [("Size".into(), true, true), ("Kind".into(), true, true)]
    );
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert!(h.open_menu().is_none());
    assert_eq!(
        values::<ColumnVisibilityChanged>(&h.messages()),
        [ColumnVisibilityChanged {
            column: 1,
            visible: false
        }]
    );
    let view = get(&h, t);
    assert!(!view.is_column_visible(1));
    assert_eq!(view.visible_columns(), [0, 2]);
    assert_eq!(view.column_width(1), 0.0);
    // Kind moved into Size's place: a click on that header does not sort by Size.
    h.click(h.at(t, 1.0 + 160.0 + 20.0, TABLE_HEADER / 2.0));
    assert!(
        values::<SortChanged>(&h.messages()).is_empty(),
        "Kind does not sort"
    );
    assert_eq!(get(&h, t).sort_column(), None);

    let p = h.at(t, 30.0, TABLE_HEADER / 2.0);
    right_click(&mut h, p);
    assert_eq!(
        menu_items(&h),
        [("Size".into(), false, true), ("Kind".into(), true, true)]
    );
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(
        values::<ColumnVisibilityChanged>(&h.messages()),
        [ColumnVisibilityChanged {
            column: 1,
            visible: true
        }]
    );
    assert_eq!(get(&h, t).visible_columns(), [0, 1, 2]);
    assert_eq!(get(&h, t).column_width(1), 100.0);
}

#[test]
fn the_last_column_shown_cannot_be_hidden() {
    let mut h = Harness::new();
    let t = chooser_table(&mut h);
    let hidden = |h: &mut Harness, c: usize| {
        let mut changed = false;
        h.tree.update::<TableView, _>(t, |table, cx| {
            changed = table.set_column_visible(cx, c, false);
        });
        h.prepare();
        changed
    };
    assert!(hidden(&mut h, 1));
    assert!(!hidden(&mut h, 1), "already hidden");
    assert!(hidden(&mut h, 0));
    assert_eq!(get(&h, t).visible_columns(), [2]);
    assert!(!hidden(&mut h, 2), "Kind is the only column shown");
    let p = h.at(t, 30.0, TABLE_HEADER / 2.0);
    right_click(&mut h, p);
    assert_eq!(
        menu_items(&h),
        [("Size".into(), false, true), ("Kind".into(), true, false)]
    );
}

#[test]
fn a_table_without_the_chooser_has_no_header_menu() {
    let mut h = Harness::new();
    let t = table(&mut h, 5);
    let p = h.at(t, 30.0, TABLE_HEADER / 2.0);
    right_click(&mut h, p);
    assert!(h.open_menu().is_none());
    h.tree.update::<TableView, _>(t, |table, cx| {
        table.set_column_visible(cx, 2, false);
    });
    assert_eq!(get(&h, t).visible_columns(), [0, 1]);
}

#[test]
fn the_context_menu_key_opens_the_column_menu_of_a_focused_table() {
    let mut h = Harness::new();
    let t = chooser_table(&mut h);
    h.click(row(&h, t, 0));
    h.key(Key::ContextMenu);
    assert_eq!(menu_items(&h).len(), 2);
    h.key(Key::Escape);
    assert!(h.open_menu().is_none());
}

#[test]
fn hidden_columns_are_not_copied_exported_or_searched() {
    let mut h = Harness::new();
    let t = h.add(
        TableView::new(
            vec![
                TableColumn::new("Name", 160.0),
                TableColumn::new("Size", 100.0),
                TableColumn::new("Kind", 120.0).hidden(true),
            ],
            Generated::new(3),
        )
        .width(400.0),
    );
    assert_eq!(get(&h, t).visible_columns(), [0, 1]);
    assert!(!get(&h, t).columns()[2].is_visible());
    assert_eq!(
        get(&h, t).export_text(TableText::Csv),
        "Name,Size\nItem 0,0\nItem 1,919\nItem 2,838"
    );
    h.click(row(&h, t, 1));
    assert_eq!(get(&h, t).copy_text().as_deref(), Some("Item 1\t919"));
    // "Green" is only in the hidden Kind column of row 1.
    filter(&mut h, t, "green");
    assert_eq!(get(&h, t).matching_row_count(), 0);
    h.tree.update::<TableView, _>(t, |table, cx| {
        table.set_column_visible(cx, 2, true);
    });
    assert_eq!(get(&h, t).displayed_rows(), [1], "searched once shown");
}

const CMD_ALT: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: true,
    meta: true,
    command: true,
    command_is_meta: true,
};

#[test]
fn option_cmd_arrows_move_items_from_the_keyboard() {
    let mut h = Harness::new();
    let o = open_outline(&mut h);
    h.click(outline_row(&h, o, 3, 120.0)); // Dog
    h.messages();
    h.key_with(Key::ArrowUp, CMD_ALT);
    assert_eq!(
        outline_shape(&h, o)[1..4],
        ["  +Mammals", "    Dog", "    Cat"]
    );
    assert_eq!(
        values::<ItemsMoved>(&h.messages()),
        [ItemsMoved {
            items: vec![3],
            parent: Some(1),
            index: 0
        }]
    );
    assert_eq!(outline(&h, o).selected_items(), [2]);
    h.key_with(Key::ArrowUp, CMD_ALT);
    assert!(values::<ItemsMoved>(&h.messages()).is_empty());
    h.key_with(Key::ArrowDown, CMD_ALT);
    assert_eq!(outline_shape(&h, o)[2..4], ["    Cat", "    Dog"]);
    h.messages();
    h.click(outline_row(&h, o, 1, 120.0)); // Mammals
    h.click_with(outline_row(&h, o, 4, 120.0), CMD); // Birds
    h.key_with(Key::ArrowDown, CMD_ALT);
    assert!(
        values::<ItemsMoved>(&h.messages()).is_empty(),
        "already last"
    );
    h.key_with(Key::ArrowUp, CMD_ALT);
    assert!(
        values::<ItemsMoved>(&h.messages()).is_empty(),
        "already first"
    );

    let items = (0..10)
        .map(|i| CollectionItem::new(format!("Item {i}")))
        .collect();
    let c = h.add(
        CollectionView::new(items)
            .reorderable(true)
            .width(450.0)
            .height(300.0),
    );
    h.click(coll_item(&h, c, 1));
    h.key_with(Key::ArrowRight, CMD_ALT);
    assert_eq!(coll_titles(&h, c)[..3], ["Item 0", "Item 2", "Item 1"]);
    h.key_with(Key::ArrowDown, CMD_ALT);
    assert_eq!(coll_titles(&h, c)[6], "Item 1");
    assert_eq!(coll(&h, c).selected_items(), [6]);
    assert_eq!(
        values::<RowsMoved>(&h.messages()),
        [
            RowsMoved {
                rows: vec![1],
                to: 2
            },
            RowsMoved {
                rows: vec![2],
                to: 6
            }
        ]
    );
    h.key_with(Key::ArrowLeft, CMD_ALT);
    assert_eq!(coll(&h, c).selected_items(), [5]);
}

#[test]
fn a_filtered_outline_moves_items_past_the_rows_it_lists() {
    let mut h = Harness::new();
    let items = ["apple one", "banana", "apple two", "cherry"]
        .map(OutlineItem::new)
        .to_vec();
    let o = h.add(
        OutlineView::new(items)
            .reorderable(true)
            .width(300.0)
            .height(300.0),
    );
    h.tree
        .update::<OutlineView, _>(o, |v, cx| v.set_filter_text(cx, "apple"));
    h.prepare();
    h.tree
        .update::<OutlineView, _>(o, |v, cx| v.select_item(cx, Some(2)));
    h.tree.set_focus(Some(o));
    h.prepare();
    h.messages();
    let listed = |h: &Harness| {
        let v = outline(h, o);
        v.visible_items()
            .iter()
            .map(|&i| v.title(i).to_owned())
            .collect::<Vec<_>>()
    };
    h.key_with(Key::ArrowUp, CMD_ALT);
    assert_eq!(listed(&h), ["apple two", "apple one"]);
    assert_eq!(
        values::<ItemsMoved>(&h.messages()),
        [ItemsMoved {
            items: vec![2],
            parent: None,
            index: 0
        }]
    );
    h.key_with(Key::ArrowUp, CMD_ALT);
    assert!(values::<ItemsMoved>(&h.messages()).is_empty());
    h.key_with(Key::ArrowDown, CMD_ALT);
    assert_eq!(listed(&h), ["apple one", "apple two"]);
    h.messages();
    h.key_with(Key::ArrowDown, CMD_ALT);
    assert!(values::<ItemsMoved>(&h.messages()).is_empty());
    h.tree
        .update::<OutlineView, _>(o, |v, cx| v.set_filter_text(cx, ""));
    h.prepare();
    assert_eq!(listed(&h), ["apple one", "apple two", "banana", "cherry"]);
}

#[test]
fn a_collection_item_at_an_edge_stays_put_from_the_keyboard() {
    let mut h = Harness::new();
    let items = (0..10)
        .map(|i| CollectionItem::new(format!("Item {i}")))
        .collect();
    let c = h.add(
        CollectionView::new(items)
            .reorderable(true)
            .width(450.0)
            .height(300.0),
    );
    let before = coll_titles(&h, c);
    assert_eq!(coll(&h, c).columns(), 4);
    h.click(coll_item(&h, c, 2));
    h.messages();
    h.key_with(Key::ArrowUp, CMD_ALT);
    h.click(coll_item(&h, c, 0));
    h.key_with(Key::ArrowLeft, CMD_ALT);
    h.click(coll_item(&h, c, 8));
    h.key_with(Key::ArrowDown, CMD_ALT);
    h.click(coll_item(&h, c, 9));
    h.key_with(Key::ArrowRight, CMD_ALT);
    assert_eq!(coll_titles(&h, c), before);
    assert!(values::<RowsMoved>(&h.messages()).is_empty());
    h.click(coll_item(&h, c, 5));
    h.key_with(Key::ArrowDown, CMD_ALT);
    assert_eq!(coll_titles(&h, c)[9], "Item 5");
}

#[test]
fn a_drop_lands_where_the_pointer_is_after_the_wheel_scrolled_the_rows() {
    let mut h = Harness::new();
    let items = (0..40)
        .map(|i| OutlineItem::new(format!("Note {i}")))
        .collect();
    let o = h.add(
        OutlineView::new(items)
            .reorderable(true)
            .width(300.0)
            .height(10.0 * 22.0 + 2.0),
    );
    h.click(outline_row(&h, o, 1, 120.0));
    let from = outline_row(&h, o, 1, 120.0);
    // The top quarter of the fifth row on screen, away from the edges.
    let to = in_row(&h, o, 5, 3.0);
    h.down(from);
    h.move_to(Point::new(from.x, from.y + 6.0));
    h.move_to(to);
    h.wheel(to, Point::new(0.0, -220.0));
    let scrolled = outline(&h, o).scroll_offset();
    assert!(scrolled > 100.0, "scrolled {scrolled}");
    h.up(to);
    let titles: Vec<String> = outline(&h, o)
        .items()
        .iter()
        .map(|i| i.title().to_owned())
        .collect();
    let row = 5 + (scrolled / 22.0).round() as usize;
    assert_eq!(
        titles.iter().position(|t| t == "Note 1"),
        Some(row - 1),
        "before Note {row}, under the pointer"
    );

    // The same in a collection: 4 columns of 122-px rows, 300 px tall.
    let items = (0..40)
        .map(|i| CollectionItem::new(format!("Item {i}")))
        .collect();
    let c = h.add(
        CollectionView::new(items)
            .reorderable(true)
            .width(450.0)
            .height(300.0),
    );
    h.click(coll_item(&h, c, 0));
    let from = coll_item(&h, c, 0);
    let to = Point::new(coll_item(&h, c, 5).x - 30.0, coll_item(&h, c, 5).y);
    h.down(from);
    h.move_to(Point::new(from.x + 10.0, from.y));
    h.move_to(to);
    h.wheel(to, Point::new(0.0, -244.0));
    h.up(to);
    // Two rows of four scrolled by: before item 13 (now 12, with item 0 taken out).
    assert_eq!(coll_titles(&h, c)[12], "Item 0");
}

#[test]
fn a_drop_on_a_pinned_sidebar_heading_goes_to_the_start_of_its_section() {
    let mut h = Harness::new();
    let items = vec![
        OutlineItem::new("Favorites")
            .children((0..20).map(|i| OutlineItem::new(format!("Place {i}")))),
        OutlineItem::new("Tags").child(OutlineItem::new("Red")),
    ];
    // Ids: 0 Favorites, 1..=20 Places, 21 Tags, 22 Red.
    let s = h.add(
        OutlineView::new(items)
            .sidebar()
            .reorderable(true)
            .width(220.0)
            .height(200.0),
    );
    h.wheel(h.at(s, 60.0, 100.0), Point::new(0.0, -140.0));
    assert_eq!(outline(&h, s).floating_heading_item(), Some(0));
    let first = (outline(&h, s).scroll_offset() / 28.0).ceil() as usize;
    let place = outline(&h, s).visible_items()[first + 2];
    let title = outline(&h, s).title(place).to_owned();
    let from = h.at(
        s,
        60.0,
        (first + 2) as f32 * 28.0 - outline(&h, s).scroll_offset() + 14.0,
    );
    h.click(from);
    assert_eq!(outline(&h, s).selected_items(), [place]);
    let heading = h.at(s, 60.0, 14.0);
    h.down(from);
    h.move_to(Point::new(from.x, from.y - 10.0));
    h.move_to(heading);
    h.up(heading);
    let items = outline(&h, s).items();
    assert_eq!(items[0].child_items()[0].title(), title);
}

#[test]
fn an_item_the_filter_hides_is_not_renamed() {
    let mut h = Harness::new();
    let o = h.add(OutlineView::new(tree_items()).renamable(true).width(300.0));
    filter_outline(&mut h, o, "owl");
    h.tree
        .update::<OutlineView, _>(o, |v, cx| v.rename_item(cx, 7)); // Fern
    assert_eq!(outline(&h, o).renaming(), None);
    h.tree
        .update::<OutlineView, _>(o, |v, cx| v.rename_item(cx, 5)); // Owl
    assert_eq!(outline(&h, o).renaming(), Some(5));
}

fn sales() -> Vec<Vec<String>> {
    [
        ("East", "Tokyo", "1,200"),
        ("West", "Osaka", "800"),
        ("East", "Sendai", "400"),
        ("North", "Sapporo", "n/a"),
        ("West", "Kobe", "300"),
    ]
    .iter()
    .map(|(r, c, s)| vec![(*r).to_owned(), (*c).to_owned(), (*s).to_owned()])
    .collect()
}

fn sales_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("Region", 120.0),
        TableColumn::new("City", 120.0),
        TableColumn::new("Sales", 90.0).sort(ColumnSort::Number),
    ]
}

#[test]
fn grouped_data_puts_a_group_row_with_aggregates_before_each_group() {
    let grouping = Grouping::by(0)
        .aggregate(2, Aggregate::Sum)
        .aggregate(1, Aggregate::Count);
    let data = grouping.apply(sales());
    let rows: Vec<Vec<String>> = (0..data.row_count())
        .map(|r| (0..3).map(|c| data.cell(r, c).into_owned()).collect())
        .collect();
    // Groups in the order their first row comes; group rows show the key in the first
    // and grouping columns and the aggregates in theirs ("n/a" is not a number).
    assert_eq!(
        rows,
        [
            ["▾ East", "2", "1,600"],
            ["East", "Tokyo", "1,200"],
            ["East", "Sendai", "400"],
            ["▾ West", "2", "1,100"],
            ["West", "Osaka", "800"],
            ["West", "Kobe", "300"],
            ["▾ North", "1", ""],
            ["North", "Sapporo", "n/a"],
        ]
    );
    let groups: Vec<usize> = (0..data.row_count())
        .filter(|&r| data.is_group_row(r))
        .collect();
    assert_eq!(groups, [0, 3, 6]);
    assert_eq!(data.group_count(), 3);
    assert_eq!(data.group_key(3), Some("West"));
    assert_eq!(data.source_row(4), Some(1));
    assert_eq!(data.source_row(3), None);
    assert_eq!(data.cell_note(0, 0).as_deref(), Some("2 rows"));
    assert_eq!(data.cell_note(6, 0).as_deref(), Some("1 row"));
    assert_eq!(data.cell_note(0, 2), None);

    let averages = Grouping::by(0)
        .aggregate(2, Aggregate::Average)
        .apply(sales());
    assert_eq!(averages.cell(0, 2), "800");
    let extremes = Grouping::by(0).aggregate(2, Aggregate::Max).apply(sales());
    assert_eq!(extremes.cell(3, 2), "800");
}

#[test]
fn clicking_a_group_row_folds_and_unfolds_it_in_a_table() {
    let mut h = Harness::new();
    let mut grouping = Grouping::by(0).aggregate(2, Aggregate::Sum);
    let t = h.add(
        TableView::new(sales_columns(), grouping.apply(sales()))
            .group_row_cells(true)
            .height(TEN_ROWS),
    );
    h.prepare();
    assert_eq!(get(&h, t).displayed_rows().len(), 8);
    let y = |pos: usize| TABLE_HEADER + (pos as f32 + 0.5) * TABLE_ROW;
    h.messages();
    h.click(h.at(t, 40.0, y(3)));
    let clicked = values::<GroupRowClicked>(&h.messages());
    assert_eq!(clicked, [GroupRowClicked(3)]);

    let source = sales();
    assert_eq!(
        grouping.toggle_at(&source, clicked[0].0).as_deref(),
        Some("West")
    );
    assert!(grouping.is_collapsed("West"));
    let folded = grouping.apply(sales());
    h.tree
        .update::<TableView, _>(t, |table, cx| table.set_data(cx, folded));
    h.prepare();
    let table = get(&h, t);
    assert_eq!(table.displayed_rows().len(), 6);
    // Folded groups still count their rows and sum them.
    assert_eq!(grouping.apply(sales()).cell(3, 0), "▸ West");
    assert_eq!(grouping.apply(sales()).cell(3, 2), "1,100");
    assert_eq!(grouping.toggle_at(&source, 0).as_deref(), Some("East"));
    // With East and West folded, the rows are East, West, North and Sapporo.
    assert_eq!(grouping.toggle_at(&source, 3), None, "not a group row");
}

#[test]
fn grouped_rows_sort_within_their_groups_and_edits_reach_the_source() {
    let mut h = Harness::new();
    let grouping = Grouping::by(0).aggregate(2, Aggregate::Sum);
    let t = h.add(
        TableView::new(
            vec![
                TableColumn::new("Region", 120.0),
                TableColumn::new("City", 120.0).editable(true),
                TableColumn::new("Sales", 90.0).sort(ColumnSort::Number),
            ],
            grouping.apply(sales()),
        )
        .group_row_cells(true)
        .height(TEN_ROWS),
    );
    h.prepare();
    h.tree
        .update::<TableView, _>(t, |table, cx| table.sort_by(cx, 2, true));
    h.prepare();
    // East's 400 comes before its 1,200; group rows stay at the head of their group.
    assert_eq!(get(&h, t).displayed_rows(), [0, 2, 1, 3, 5, 4, 6, 7]);

    let mut data = grouping.apply(sales());
    assert!(data.set_cell(1, 1, "Yokohama"));
    assert_eq!(data.source()[0][1], "Yokohama");
    assert!(!data.set_cell(0, 1, "x"), "group rows take no edits");
}

#[test]
fn edits_to_grouped_rows_update_the_aggregates_but_not_the_grouping_column() {
    let grouping = Grouping::by(0).aggregate(2, Aggregate::Sum);
    let mut data = grouping.apply(sales());
    assert_eq!(data.cell(0, 2), "1,600");
    assert!(data.set_cell(1, 2, "2,000"));
    assert_eq!(data.cell(1, 2), "2,000");
    assert_eq!(data.cell(0, 2), "2,400", "East's sum follows the edit");
    assert_eq!(data.cell(3, 2), "1,100", "other groups are unchanged");
    // Moving a row to another group is the app's: it re-applies the grouping.
    assert!(!data.set_cell(1, 0, "West"));
    assert_eq!(data.source()[0][0], "East");
}

#[test]
fn folded_groups_stay_visible_under_a_filter_so_they_can_be_unfolded() {
    let mut h = Harness::new();
    let mut grouping = Grouping::by(0).aggregate(2, Aggregate::Sum);
    let t = h.add(
        TableView::new(sales_columns(), grouping.apply(sales()))
            .group_row_cells(true)
            .height(TEN_ROWS),
    );
    h.prepare();
    h.tree
        .update::<TableView, _>(t, |table, cx| table.set_filter_text(cx, "Tokyo"));
    h.prepare();
    // East and its matching row; the other groups have no match.
    assert_eq!(get(&h, t).displayed_rows(), [0, 1]);

    grouping.toggle("East");
    let folded = grouping.apply(sales());
    h.tree.update::<TableView, _>(t, |table, cx| {
        table.set_data(cx, folded);
        table.set_filter_text(cx, "Tokyo");
    });
    h.prepare();
    // The folded East is still there to click; West and North keep their rows, which
    // do not match, and so are hidden as before.
    assert_eq!(get(&h, t).displayed_rows(), [0]);
    assert!(get(&h, t).is_group_row(0));
}

fn remote_page(rows: std::ops::Range<usize>) -> Vec<Vec<String>> {
    rows.map(|r| vec![format!("Item {r}"), format!("{}", 1000 - r)])
        .collect()
}

fn remote_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("Name", 200.0).sort(ColumnSort::Text),
        TableColumn::new("Score", 100.0).sort(ColumnSort::Number),
    ]
}

#[test]
fn remote_rows_ask_only_for_the_pages_on_screen_and_show_them_when_they_arrive() {
    let mut h = Harness::new();
    let rows = RemoteRows::new(100_000, 8);
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let counter = calls.clone();
    rows.on_request(move || counter.set(counter.get() + 1));
    let t = h.add(
        TableView::new(remote_columns(), rows.data())
            .sorts_rows(false)
            .height(TEN_ROWS),
    );
    h.prepare();
    let requests = rows.take_requests();
    let pages: Vec<usize> = requests.iter().map(|r| r.page).collect();
    // Ten rows on screen: the first two pages of eight, nothing more.
    assert_eq!(pages, [0, 1]);
    assert_eq!(requests[1].rows, 8..16);
    assert_eq!(calls.get(), 1, "woken once for the batch");
    assert!(!rows.has_requests());
    let data = rows.data();
    assert_eq!(data.cell_look(0, 0), CellLook::Loading);
    assert_eq!(get(&h, t).row_count(), 100_000);

    // Drawing again asks for nothing new while the pages are on their way.
    h.tree
        .update::<TableView, _>(t, |table, cx| table.data_changed(cx));
    h.prepare();
    assert!(rows.take_requests().is_empty());

    for request in &requests {
        assert!(rows.fulfill_request(request, remote_page(request.rows.clone())));
    }
    h.tree
        .update::<TableView, _>(t, |table, cx| table.data_changed(cx));
    h.prepare();
    assert_eq!(data.cell(9, 0), "Item 9");
    assert_eq!(data.cell_look(9, 0), CellLook::Text);
    assert_eq!(rows.loaded_pages(), 2);

    // Scrolling far down asks for the pages there.
    h.tree
        .update::<TableView, _>(t, |table, cx| table.scroll_to_row(cx, 50_000));
    h.prepare();
    let pages: Vec<usize> = rows.take_requests().iter().map(|r| r.page).collect();
    assert!(pages.contains(&(50_000 / 8)), "{pages:?}");
    assert!(pages.len() <= 3, "{pages:?}");
}

#[test]
fn a_reset_drops_the_pages_and_refuses_pages_fetched_before_it() {
    let mut h = Harness::new();
    let rows = RemoteRows::new(30, 10);
    let t = h.add(TableView::new(remote_columns(), rows.data()).height(TEN_ROWS));
    h.prepare();
    let old = rows.take_requests();
    assert_eq!(old.len(), 1);

    // The source's order changed (another sort): fewer rows, every page again.
    rows.reset(12);
    h.tree
        .update::<TableView, _>(t, |table, cx| table.data_changed(cx));
    h.prepare();
    assert_eq!(get(&h, t).row_count(), 12);
    assert!(!rows.fulfill_request(&old[0], remote_page(0..10)), "stale");
    assert_eq!(rows.loaded_pages(), 0);
    let first = rows.take_requests();
    assert_eq!(first.len(), 1, "the ten rows on screen are page 0");
    h.tree
        .update::<TableView, _>(t, |table, cx| table.scroll_to_row(cx, 11));
    h.prepare();
    let last = rows.take_requests();
    assert_eq!(last.len(), 1);
    assert_eq!(last[0].rows, 10..12, "clipped to the row count");
    assert!(rows.fulfill_request(&last[0], remote_page(10..15)));
    assert_eq!(rows.data().cell(11, 0), "Item 11");
    let new = first;

    // A failed page is asked for again the next time it is drawn.
    rows.fail(new[0].page);
    h.tree
        .update::<TableView, _>(t, |table, cx| table.data_changed(cx));
    h.prepare();
    let again: Vec<usize> = rows.take_requests().iter().map(|r| r.page).collect();
    assert_eq!(again, [0]);
}

#[test]
fn without_sorting_rows_a_header_click_only_reports_the_sort() {
    let mut h = Harness::new();
    let rows = RemoteRows::new(1_000, 10);
    let t = h.add(
        TableView::new(remote_columns(), rows.data())
            .sorts_rows(false)
            .height(TEN_ROWS),
    );
    h.prepare();
    for request in rows.take_requests() {
        rows.fulfill(request.page, remote_page(request.rows));
    }
    h.tree
        .update::<TableView, _>(t, |table, cx| table.data_changed(cx));
    h.prepare();
    h.messages();
    // Scores fall as rows go down, so sorting here would put row 9 first.
    h.click(h.at(t, 250.0, TABLE_HEADER / 2.0));
    assert_eq!(
        values::<SortChanged>(&h.messages()),
        [SortChanged {
            column: 1,
            ascending: true
        }]
    );
    let table = get(&h, t);
    assert_eq!(table.sort_column(), Some((1, true)));
    assert_eq!(
        &table.displayed_rows()[..3],
        [0, 1, 2],
        "rows stay in order"
    );
    h.prepare();
    // Sorting would have read every row; nothing beyond the screen is asked for.
    assert!(rows.take_requests().len() <= 1);
}

#[test]
fn remote_rows_keep_a_bounded_number_of_pages() {
    let rows = RemoteRows::new(1_000, 10).cached_pages(3);
    for page in 0..5 {
        let range = page * 10..page * 10 + 10;
        rows.fulfill(page, remote_page(range));
    }
    assert_eq!(rows.loaded_pages(), 3);
    assert!(!rows.is_loaded(0) && !rows.is_loaded(15));
    assert!(rows.is_loaded(20) && rows.is_loaded(49));
}
