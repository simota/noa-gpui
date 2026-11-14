use std::path::PathBuf;

use ui_core::style::{FlexDirection, Size as LayoutSize, length, percent};
use ui_core::widgets::{
    CellEdited, CellRange, CellSelectionChanged, Chart, ChartKind, ChartPointActivated,
    ChartSeries, ChartSeriesToggled, ColorWell, ColumnSort, Container, DescriptionList,
    FileDialogRequested, FileDropZone, FilesDropped, FindAction, FindOptions, Label, LogView,
    NumberInput, PopUpButton, PropertyChanged, PropertyGrid, PropertyValue, RowActivated,
    ScrollView, Switch, TABLE_HEADER, TABLE_ROW, TableColumn, TableView, ValueChanged,
};
use ui_core::{DragData, Event, Key, Message, Modifiers, PointerButton, Style, UiTree, WidgetId};
use ui_scene::{Color, DrawCommand, Point, Rect, Shape, Size};
use ui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(800.0, 600.0);

#[test]
fn log_search_highlights_follow_the_shaped_bidi_selection() {
    let mut h = Harness::new();
    let line = "abc אבג def";
    let id = h.add(LogView::new().lines([line]));
    let status = h
        .tree
        .update::<LogView, _>(id, |view, cx| {
            view.find(
                cx,
                &FindAction::Search {
                    query: "אב".into(),
                    options: FindOptions::default(),
                },
            )
        })
        .unwrap();
    assert_eq!(status.total, 1);
    h.prepare();
    let (text_rect, layout) = h
        .tree
        .display_list()
        .commands()
        .iter()
        .find_map(|command| {
            if let DrawCommand::Text { rect, layout, .. } = command {
                Some((*rect, *layout))
            } else {
                None
            }
        })
        .expect("the log line is drawn");
    let start = line.find("אב").unwrap();
    let expected: Vec<_> = h
        .text
        .selection_rects(layout, start, start + "אב".len())
        .into_iter()
        .map(|rect| rect.translate(text_rect.x, text_rect.y))
        .collect();
    assert!(!expected.is_empty());
    let actual: Vec<_> = h
        .tree
        .display_list()
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Quad { rect, color, .. }
                if color.b < 0.3 && color.r > 0.9 && color.g > 0.7 =>
            {
                Some(*rect)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        actual, expected,
        "search paints the selected glyphs, including RTL runs"
    );
}

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
        }
    }

    fn add(&mut self, widget: impl ui_core::Widget) -> WidgetId {
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

    fn dispatch(&mut self, event: Event) {
        self.tree.dispatch(event, &mut self.text);
        self.prepare();
    }

    fn modifiers(&mut self, modifiers: Modifiers) {
        self.dispatch(Event::ModifiersChanged(modifiers));
    }

    fn down(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.dispatch(Event::PointerDown { pos, button });
    }

    fn up(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.dispatch(Event::PointerUp { pos, button });
    }

    fn click(&mut self, pos: Point) {
        self.down(pos);
        self.up(pos);
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.modifiers(modifiers);
        self.dispatch(Event::KeyDown { key, modifiers });
        self.modifiers(Modifiers::default());
    }

    fn type_text(&mut self, text: &str) {
        self.dispatch(Event::Text(text.into()));
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

const SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: false,
    alt: false,
    meta: false,
    command: false,
};
const CMD: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
    meta: true,
    command: true,
};

fn fixed_row(height: f32) -> Container {
    Container::new(Style {
        size: LayoutSize {
            width: percent(1.0),
            height: length(height),
        },
        flex_shrink: 0.0,
        ..Default::default()
    })
}

/// A 100-pixel-tall view over a column of `rows` 40-pixel rows; returns the view and
/// the column.
fn log_view(h: &mut Harness, rows: usize, follows: bool) -> (WidgetId, WidgetId) {
    let view = h.add(
        ScrollView::new(Style {
            size: LayoutSize {
                width: length(200.0),
                height: length(100.0),
            },
            ..Default::default()
        })
        .follows_end(follows),
    );
    let column = h.tree.insert(
        Some(view),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            size: LayoutSize {
                width: percent(1.0),
                height: ui_core::style::auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }),
    );
    for _ in 0..rows {
        h.tree.insert(Some(column), fixed_row(40.0));
    }
    h.prepare();
    (view, column)
}

fn scroll_y(h: &Harness, view: WidgetId) -> f32 {
    h.tree.get::<ScrollView>(view).unwrap().scroll_position().y
}

#[test]
fn a_view_that_follows_its_end_stays_at_the_bottom_as_content_is_added() {
    let mut h = Harness::new();
    let (view, column) = log_view(&mut h, 5, true);
    // 200 pixels of content in a 100-pixel view.
    assert_eq!(scroll_y(&h, view), 100.0);

    for _ in 0..3 {
        h.tree.insert(Some(column), fixed_row(40.0));
    }
    h.prepare();
    assert_eq!(scroll_y(&h, view), 220.0);
    let last = *h.tree.children(column).last().unwrap();
    assert_eq!(h.rect(last).bottom(), h.rect(view).bottom());
    assert!(h.tree.get::<ScrollView>(view).unwrap().is_following_end());
}

#[test]
fn scrolling_up_lets_go_of_the_end_and_scrolling_back_down_follows_again() {
    let mut h = Harness::new();
    let (view, column) = log_view(&mut h, 5, true);
    let inside = h.at(view, 50.0, 50.0);
    h.wheel(inside, 60.0);
    assert_eq!(scroll_y(&h, view), 40.0);
    assert!(!h.tree.get::<ScrollView>(view).unwrap().is_following_end());

    h.tree.insert(Some(column), fixed_row(40.0));
    h.prepare();
    assert_eq!(scroll_y(&h, view), 40.0);

    h.wheel(inside, -100.0);
    assert_eq!(scroll_y(&h, view), 140.0);
    assert!(h.tree.get::<ScrollView>(view).unwrap().is_following_end());
    h.tree.insert(Some(column), fixed_row(40.0));
    h.prepare();
    assert_eq!(scroll_y(&h, view), 180.0);
}

#[test]
fn scroll_to_end_jumps_down_and_follows_again() {
    let mut h = Harness::new();
    let (view, column) = log_view(&mut h, 5, true);
    h.tree
        .update::<ScrollView, _>(view, |v, cx| v.set_offset(cx, 0.0));
    h.prepare();
    assert!(!h.tree.get::<ScrollView>(view).unwrap().is_following_end());
    h.tree.insert(Some(column), fixed_row(40.0));
    h.prepare();
    assert_eq!(scroll_y(&h, view), 0.0);

    h.tree
        .update::<ScrollView, _>(view, |v, cx| v.scroll_to_end(cx));
    h.prepare();
    assert_eq!(scroll_y(&h, view), 140.0);
    h.tree.insert(Some(column), fixed_row(40.0));
    h.prepare();
    assert_eq!(scroll_y(&h, view), 180.0);
}

#[test]
fn a_plain_view_stays_where_it_is_when_content_grows() {
    let mut h = Harness::new();
    let (view, column) = log_view(&mut h, 5, false);
    assert_eq!(scroll_y(&h, view), 0.0);
    h.tree.insert(Some(column), fixed_row(40.0));
    h.prepare();
    assert_eq!(scroll_y(&h, view), 0.0);
    assert!(!h.tree.get::<ScrollView>(view).unwrap().is_following_end());
}

fn files(names: &[&str]) -> DragData {
    DragData::Files(names.iter().map(PathBuf::from).collect())
}

#[test]
fn a_drop_zone_takes_dropped_files_of_its_types() {
    let mut h = Harness::new();
    let zone = h.add(FileDropZone::new().extensions([".PNG", "jpg"]).width(300.0));
    let pos = h.at(zone, 150.0, 50.0);

    h.dispatch(Event::DragOver {
        pos,
        data: files(&["/tmp/a.png", "/tmp/b.JPG"]),
    });
    assert!(h.tree.get::<FileDropZone>(zone).unwrap().is_active());
    h.dispatch(Event::Drop {
        pos,
        data: files(&["/tmp/a.png", "/tmp/b.JPG"]),
    });
    assert!(!h.tree.get::<FileDropZone>(zone).unwrap().is_active());
    let dropped = values::<FilesDropped>(&h.messages());
    assert_eq!(
        dropped,
        [FilesDropped(vec![
            PathBuf::from("/tmp/a.png"),
            PathBuf::from("/tmp/b.JPG")
        ])]
    );

    h.dispatch(Event::DragOver {
        pos,
        data: files(&["/tmp/notes.txt"]),
    });
    assert!(!h.tree.get::<FileDropZone>(zone).unwrap().is_active());
    h.dispatch(Event::Drop {
        pos,
        data: files(&["/tmp/notes.txt"]),
    });
    assert!(values::<FilesDropped>(&h.messages()).is_empty());
}

#[test]
fn a_single_file_drop_zone_refuses_several_files() {
    let zone = FileDropZone::new().multiple(false);
    assert!(zone.accepts(&[PathBuf::from("a")]));
    assert!(!zone.accepts(&[PathBuf::from("a"), PathBuf::from("b")]));
    assert!(!zone.accepts(&[]));
}

#[test]
fn clicking_or_activating_a_drop_zone_asks_for_the_open_panel() {
    let mut h = Harness::new();
    let zone = h.add(FileDropZone::new().extensions(["csv"]).multiple(false));
    h.click(h.at(zone, 20.0, 20.0));
    let expected = FileDialogRequested {
        multiple: false,
        extensions: vec!["csv".into()],
    };
    assert_eq!(
        values::<FileDialogRequested>(&h.messages()),
        std::slice::from_ref(&expected)
    );

    h.tree.set_focus(Some(zone));
    h.prepare();
    h.key(Key::Space);
    h.key(Key::Enter);
    assert_eq!(
        values::<FileDialogRequested>(&h.messages()),
        [expected.clone(), expected]
    );
}

#[test]
fn a_drop_zone_goes_dark_when_the_dragged_files_stop_matching() {
    let mut h = Harness::new();
    let zone = h.add(FileDropZone::new().extensions(["png"]).width(300.0));
    let pos = h.at(zone, 150.0, 50.0);
    // The OS reports the files of one drag one after the other.
    let data = files(&["/tmp/a.png"]);
    h.dispatch(Event::DragOver { pos, data });
    assert!(h.tree.get::<FileDropZone>(zone).unwrap().is_active());
    let data = files(&["/tmp/a.png", "/tmp/notes.txt"]);
    h.dispatch(Event::DragOver { pos, data });
    assert!(!h.tree.get::<FileDropZone>(zone).unwrap().is_active());
}

#[test]
fn a_drop_zone_disabled_during_the_drag_takes_nothing() {
    let mut h = Harness::new();
    let zone = h.add(FileDropZone::new().width(300.0));
    let pos = h.at(zone, 150.0, 50.0);
    let data = files(&["/tmp/a.png"]);
    h.dispatch(Event::DragOver { pos, data });
    assert!(h.tree.get::<FileDropZone>(zone).unwrap().is_active());
    h.tree.set_enabled(zone, false);
    let data = files(&["/tmp/a.png"]);
    h.dispatch(Event::Drop { pos, data });
    assert!(values::<FilesDropped>(&h.messages()).is_empty());
    assert!(!h.tree.get::<FileDropZone>(zone).unwrap().is_active());
}

#[test]
fn a_drop_zone_compares_extensions_without_case_beyond_ascii() {
    let zone = FileDropZone::new().extensions(["Ödf"]);
    assert!(zone.accepts(&[PathBuf::from("/tmp/a.ÖDF")]));
    assert!(zone.accepts(&[PathBuf::from("/tmp/a.ödf")]));
    assert!(!zone.accepts(&[PathBuf::from("/tmp/a.odf")]));
}

fn label_text(h: &Harness, id: WidgetId) -> String {
    h.tree.get::<Label>(id).unwrap().text().to_owned()
}

#[test]
fn a_description_list_lines_values_up_beside_their_labels() {
    let mut h = Harness::new();
    let box_ = h.add(fixed_row(300.0));
    let mut parts = DescriptionList::new()
        .item("Kind", "PNG image")
        .item("Size", "2.4 MB on disk")
        .items([("Where", "Pictures")])
        .insert(&mut h.tree, box_);
    h.prepare();
    assert_eq!(parts.labels.len(), 3);
    assert_eq!(label_text(&h, parts.labels[1]), "Size");
    assert_eq!(label_text(&h, parts.values[1]), "2.4 MB on disk");

    let x = h.rect(parts.values[0]).x;
    for (&label, &value) in parts.labels.iter().zip(&parts.values) {
        assert_eq!(h.rect(value).x, x);
        assert!(h.rect(label).right() < x);
        assert_eq!(h.rect(value).y, h.rect(label).y);
    }
    assert!(h.rect(parts.values[1]).y > h.rect(parts.values[0]).y);

    parts.set_value(&mut h.tree, 0, "JPEG image");
    parts.push(&mut h.tree, "Age", "Today");
    h.prepare();
    assert_eq!(label_text(&h, parts.values[0]), "JPEG image");
    assert_eq!(h.rect(parts.values[3]).x, x);
    assert!(h.rect(parts.values[3]).y > h.rect(parts.values[2]).y);
}

#[test]
fn description_lists_can_rule_off_items_and_set_the_values_font() {
    let mut h = Harness::new();
    let box_ = h.add(Container::new(Style {
        size: LayoutSize {
            width: length(400.0),
            height: ui_core::style::auto(),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }));
    let mut parts = DescriptionList::new()
        .separators(true)
        .row_spacing(8.0)
        .value_font_family("monospace")
        .value_font_size(12.0)
        .item("Target", "unix:///var/run/docker.sock")
        .item("Engine", "Docker 27.1")
        .insert(&mut h.tree, box_);
    parts.push(&mut h.tree, "Compose", "v2.29");
    h.prepare();

    assert_eq!(parts.separators.len(), 2);
    let list = h.rect(parts.root);
    for (i, &line) in parts.separators.iter().enumerate() {
        let r = h.rect(line);
        assert_eq!((r.x, r.width, r.height), (list.x, list.width, 1.0));
        let above = h
            .rect(parts.values[i])
            .bottom()
            .max(h.rect(parts.labels[i]).bottom());
        assert_eq!(r.y, above + 8.0);
        assert_eq!(h.rect(parts.labels[i + 1]).y, r.bottom() + 8.0);
    }
    let font = |id: WidgetId| {
        let r = h.rect(id);
        (h.tree.display_list().commands().iter())
            .find_map(|c| match c {
                DrawCommand::Text { rect, layout, .. }
                    if r.contains(Point::new(rect.x, rect.y)) =>
                {
                    let buffer = h.text.layouts().buffer(*layout);
                    let family = buffer.lines[0].attrs_list().defaults().family;
                    let named = matches!(family, ui_text::cosmic_text::Family::Name(_));
                    Some((buffer.metrics().font_size, named))
                }
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(font(parts.values[2]), (12.0, true));
    assert_eq!(font(parts.labels[2]), (14.0, false));
}

#[test]
fn description_lists_can_size_and_color_their_labels() {
    let mut h = Harness::new();
    let box_ = h.add(fixed_row(300.0));
    let gray = Color::hex(0x808080);
    let mut parts = DescriptionList::new()
        .label_font_size(12.0)
        .label_color(gray)
        .item("Target", "unix:///var/run/docker.sock")
        .insert(&mut h.tree, box_);
    parts.push(&mut h.tree, "Engine", "Docker 27.1");
    h.prepare();
    let text = |id: WidgetId| {
        let r = h.rect(id);
        (h.tree.display_list().commands().iter())
            .find_map(|c| match c {
                DrawCommand::Text {
                    rect,
                    layout,
                    color,
                    ..
                } if r.contains(Point::new(rect.x, rect.y)) => {
                    Some((h.text.layouts().buffer(*layout).metrics().font_size, *color))
                }
                _ => None,
            })
            .unwrap()
    };
    for &label in &parts.labels {
        assert_eq!(text(label), (12.0, gray));
    }
    assert_eq!(text(parts.values[1]).0, 14.0);
}

#[test]
fn a_fixed_label_width_lines_lists_up() {
    let mut h = Harness::new();
    let box_ = h.add(Container::new(Style {
        flex_direction: FlexDirection::Column,
        size: LayoutSize {
            width: length(400.0),
            height: length(200.0),
        },
        ..Default::default()
    }));
    let a = DescriptionList::new()
        .label_width(120.0)
        .item("A", "1")
        .insert(&mut h.tree, box_);
    let b = DescriptionList::new()
        .label_width(120.0)
        .item("A much longer label", "2")
        .insert(&mut h.tree, box_);
    h.prepare();
    assert_eq!(h.rect(a.values[0]).x, h.rect(b.values[0]).x);
}

#[test]
fn a_property_grid_puts_a_control_of_each_type_beside_its_name() {
    let mut h = Harness::new();
    let box_ = h.add(fixed_row(300.0));
    let mut grid = PropertyGrid::new(&mut h.tree, box_, Some(100.0));
    grid.add_section(&mut h.tree, "Layout");
    let width = grid.add_number(&mut h.tree, "Width", 0.0, 1000.0, 320.0);
    let fill = grid.add_color(&mut h.tree, "Fill", Color::hex(0x3366ff));
    let visible = grid.add_bool(&mut h.tree, "Visible", true);
    let blend = grid.add_choice(&mut h.tree, "Blend", ["Normal", "Multiply", "Screen"], 1);
    h.prepare();

    assert_eq!(grid.len(), 4);
    assert_eq!(grid.index_of("Fill"), Some(fill));
    let editor = |i| grid.editor(i).unwrap();
    assert!(h.tree.get::<NumberInput>(editor(width)).is_some());
    assert!(h.tree.get::<ColorWell>(editor(fill)).is_some());
    assert!(h.tree.get::<Switch>(editor(visible)).is_some());
    assert_eq!(
        h.tree.get::<PopUpButton>(editor(blend)).unwrap().selected(),
        1
    );
    assert_eq!(grid.value(blend), Some(&PropertyValue::Choice(1)));

    let x = h.rect(editor(width)).x;
    for i in 0..grid.len() {
        let label = h.rect(grid.label(i).unwrap());
        let control = h.rect(editor(i));
        assert_eq!(control.x, x);
        assert!(label.right() <= x);
        let mid = |r: Rect| r.y + r.height / 2.0;
        assert!((mid(label) - mid(control)).abs() <= 1.0);
    }
}

#[test]
fn a_property_grid_reports_what_the_user_changed() {
    let mut h = Harness::new();
    let box_ = h.add(fixed_row(300.0));
    let mut grid = PropertyGrid::new(&mut h.tree, box_, None);
    let width = grid.add_number(&mut h.tree, "Width", 0.0, 1000.0, 320.0);
    let visible = grid.add_bool(&mut h.tree, "Visible", true);
    h.prepare();

    let switch = grid.editor(visible).unwrap();
    h.click(h.at(switch, 5.0, 5.0));
    let changes: Vec<PropertyChanged> =
        h.messages().iter().filter_map(|m| grid.handle(m)).collect();
    assert_eq!(
        changes,
        [PropertyChanged {
            index: visible,
            value: PropertyValue::Bool(false)
        }]
    );
    assert_eq!(grid.value(visible), Some(&PropertyValue::Bool(false)));

    let input = grid.editor(width).unwrap();
    h.tree.set_focus(Some(input));
    h.prepare();
    h.key(Key::ArrowUp);
    let messages = h.messages();
    assert!(!values::<ValueChanged>(&messages).is_empty());
    let changes: Vec<PropertyChanged> = messages.iter().filter_map(|m| grid.handle(m)).collect();
    assert_eq!(
        changes,
        [PropertyChanged {
            index: width,
            value: PropertyValue::Number(321.0)
        }]
    );
}

#[test]
fn setting_a_property_updates_its_control_and_ignores_other_types() {
    let mut h = Harness::new();
    let box_ = h.add(fixed_row(300.0));
    let mut grid = PropertyGrid::new(&mut h.tree, box_, None);
    let width = grid.add_number(&mut h.tree, "Width", 0.0, 100.0, 50.0);
    let blend = grid.add_choice(&mut h.tree, "Blend", ["Normal", "Multiply"], 0);
    h.prepare();

    assert!(grid.set_value(&mut h.tree, width, PropertyValue::Number(500.0)));
    assert_eq!(grid.value(width), Some(&PropertyValue::Number(100.0)));
    let input = grid.editor(width).unwrap();
    assert_eq!(h.tree.get::<NumberInput>(input).unwrap().value(), 100.0);

    assert!(grid.set_value(&mut h.tree, blend, PropertyValue::Choice(1)));
    let popup = grid.editor(blend).unwrap();
    assert_eq!(h.tree.get::<PopUpButton>(popup).unwrap().selected(), 1);

    assert!(!grid.set_value(&mut h.tree, blend, PropertyValue::Bool(true)));
    assert!(!grid.set_value(&mut h.tree, 9, PropertyValue::Bool(true)));
    assert!(!grid.set_value(&mut h.tree, blend, PropertyValue::Choice(9)));
    assert_eq!(grid.value(blend), Some(&PropertyValue::Choice(1)));
    assert!(h.messages().iter().all(|m| grid.handle(m).is_none()));
}

/// A table this tall shows exactly ten rows.
const TEN_ROWS: f32 = TABLE_HEADER + 10.0 * TABLE_ROW + 1.0;

fn grid_rows(n: usize) -> Vec<Vec<String>> {
    (0..n)
        .map(|r| (0..4).map(|c| format!("r{r}c{c}")).collect())
        .collect()
}

/// Four 100-pixel columns, the middle two editable, in a 302-pixel-wide view (so the
/// columns scroll sideways).
fn data_grid(h: &mut Harness, rows: usize, frozen: usize) -> WidgetId {
    let columns = (0..4)
        .map(|c| TableColumn::new(format!("C{c}"), 100.0).editable(c == 1 || c == 2))
        .collect();
    h.add(
        TableView::new(columns, grid_rows(rows))
            .cell_selection(true)
            .frozen_columns(frozen)
            .width(302.0)
            .height(TEN_ROWS),
    )
}

fn table(h: &Harness, id: WidgetId) -> &TableView {
    h.tree.get::<TableView>(id).unwrap()
}

/// The middle of the cell at displayed position `pos` and `column`, unscrolled.
fn cell(h: &Harness, id: WidgetId, pos: usize, column: usize) -> Point {
    h.at(
        id,
        1.0 + column as f32 * 100.0 + 50.0,
        TABLE_HEADER + pos as f32 * TABLE_ROW + TABLE_ROW / 2.0,
    )
}

#[test]
fn clicking_a_cell_focuses_it_and_arrows_move_the_focus() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 30, 0);
    h.click(cell(&h, id, 2, 1));
    assert_eq!(table(&h, id).focused_cell(), Some((2, 1)));
    assert!(table(&h, id).selected_rows().is_empty());
    let changes = values::<CellSelectionChanged>(&h.messages());
    assert_eq!(
        changes,
        [CellSelectionChanged {
            focus: (2, 1),
            range: CellRange {
                rows: 2..3,
                columns: 1..2
            }
        }]
    );

    h.key(Key::ArrowRight);
    h.key(Key::ArrowDown);
    assert_eq!(table(&h, id).focused_cell(), Some((3, 2)));
    h.key(Key::Home);
    assert_eq!(table(&h, id).focused_cell(), Some((3, 0)));
    h.key(Key::End);
    assert_eq!(table(&h, id).focused_cell(), Some((3, 3)));
    h.key(Key::ArrowRight);
    assert_eq!(table(&h, id).focused_cell(), Some((3, 3)));
    assert_eq!(table(&h, id).selected_cells().unwrap().len(), 1);
    h.key_with(Key::End, CMD);
    assert_eq!(table(&h, id).focused_cell(), Some((29, 3)));
    assert!(table(&h, id).visible_rows().contains(&29));
}

#[test]
fn shift_and_dragging_select_a_rectangle_of_cells() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 30, 0);
    h.click(cell(&h, id, 1, 0));
    h.key_with(Key::ArrowDown, SHIFT);
    h.key_with(Key::ArrowRight, SHIFT);
    let range = table(&h, id).selected_cells().unwrap();
    assert_eq!(
        range,
        CellRange {
            rows: 1..3,
            columns: 0..2
        }
    );
    assert!(table(&h, id).is_cell_selected(2, 1));
    assert!(!table(&h, id).is_cell_selected(3, 1));
    assert_eq!(table(&h, id).focused_cell(), Some((2, 1)));

    h.modifiers(SHIFT);
    h.click(cell(&h, id, 4, 2));
    h.modifiers(Modifiers::default());
    assert_eq!(
        table(&h, id).selected_cells().unwrap(),
        CellRange {
            rows: 1..5,
            columns: 0..3
        }
    );

    h.down(cell(&h, id, 5, 2));
    h.dispatch(Event::PointerMove {
        pos: cell(&h, id, 3, 1),
    });
    h.up(cell(&h, id, 3, 1));
    assert_eq!(
        table(&h, id).selected_cells().unwrap(),
        CellRange {
            rows: 3..6,
            columns: 1..3
        }
    );

    h.key_with(Key::Char('a'), CMD);
    assert_eq!(table(&h, id).selected_cells().unwrap().len(), 30 * 4);
}

#[test]
fn return_edits_the_focused_cell_in_place_and_stores_the_text() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    h.click(cell(&h, id, 1, 1));
    h.messages();
    h.key(Key::Enter);
    assert_eq!(table(&h, id).editing_cell(), Some((1, 1)));
    assert_eq!(table(&h, id).edit_text(), Some("r1c1"));

    h.type_text("new");
    assert_eq!(table(&h, id).edit_text(), Some("new"));
    h.key(Key::ArrowLeft);
    assert_eq!(table(&h, id).focused_cell(), Some((1, 1)));
    h.key(Key::Enter);
    assert_eq!(table(&h, id).editing_cell(), None);
    assert_eq!(
        values::<CellEdited>(&h.messages()),
        [CellEdited {
            row: 1,
            column: 1,
            text: "new".into(),
            stored: true
        }]
    );
    h.key(Key::Enter);
    assert_eq!(table(&h, id).edit_text(), Some("new"));
    h.type_text("other");
    h.key(Key::Escape);
    assert_eq!(table(&h, id).editing_cell(), None);
    assert!(values::<CellEdited>(&h.messages()).is_empty());
    h.key(Key::Enter);
    assert_eq!(table(&h, id).edit_text(), Some("new"));
}

#[test]
fn typing_starts_editing_and_read_only_cells_activate_their_row() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    h.click(cell(&h, id, 0, 2));
    h.type_text("4");
    h.type_text("2");
    assert_eq!(table(&h, id).edit_text(), Some("42"));
    h.click(cell(&h, id, 3, 0));
    assert_eq!(table(&h, id).editing_cell(), None);
    assert_eq!(table(&h, id).focused_cell(), Some((3, 0)));
    let messages = h.messages();
    assert_eq!(values::<CellEdited>(&messages)[0].text, "42");

    h.type_text("x");
    assert_eq!(table(&h, id).editing_cell(), None);
    h.key(Key::Enter);
    assert_eq!(values::<RowActivated>(&h.messages()), [RowActivated(3)]);

    h.click(cell(&h, id, 2, 1));
    h.click(cell(&h, id, 2, 1));
    assert_eq!(table(&h, id).editing_cell(), Some((2, 1)));
}

#[test]
fn frozen_columns_stay_put_while_the_others_scroll_under_them() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 1);
    assert_eq!(table(&h, id).frozen_column_count(), 1);
    // Scroll fully right: 400 wide content in a 300 wide view.
    h.wheel(cell(&h, id, 0, 1), 0.0);
    h.modifiers(SHIFT);
    h.wheel(cell(&h, id, 0, 1), -200.0);
    h.modifiers(Modifiers::default());

    // The frozen column is still under its old place; column 1 has scrolled away under
    // it, so the second visible column is now column 2 (at 101..201 → 201..301 - 100).
    h.click(cell(&h, id, 0, 0));
    assert_eq!(table(&h, id).focused_cell(), Some((0, 0)));
    h.click(cell(&h, id, 0, 1));
    assert_eq!(table(&h, id).focused_cell(), Some((0, 2)));
    h.click(cell(&h, id, 0, 2));
    assert_eq!(table(&h, id).focused_cell(), Some((0, 3)));

    h.key(Key::ArrowLeft);
    h.key(Key::ArrowLeft);
    assert_eq!(table(&h, id).focused_cell(), Some((0, 1)));
    h.click(cell(&h, id, 0, 1));
    assert_eq!(table(&h, id).focused_cell(), Some((0, 1)));
}

#[test]
fn trailing_frozen_columns_stay_at_the_right_edge_while_the_others_scroll_under_them() {
    let mut h = Harness::new();
    let columns = (0..4)
        .map(|c| TableColumn::new(format!("C{c}"), 100.0))
        .collect();
    // 400 wide content in a 300 wide view.
    let id = h.add(
        TableView::new(columns, grid_rows(5))
            .cell_selection(true)
            .frozen_trailing_columns(1)
            .width(302.0)
            .height(TEN_ROWS),
    );
    assert_eq!(table(&h, id).frozen_trailing_column_count(), 1);
    // Unscrolled, the last column covers the view's right third, over column 2.
    h.click(cell(&h, id, 0, 2));
    assert_eq!(table(&h, id).focused_cell(), Some((0, 3)));
    h.click(cell(&h, id, 0, 1));
    assert_eq!(table(&h, id).focused_cell(), Some((0, 1)));

    h.key(Key::ArrowRight);
    assert_eq!(table(&h, id).focused_cell(), Some((0, 2)));
    h.click(cell(&h, id, 0, 1));
    assert_eq!(table(&h, id).focused_cell(), Some((0, 2)));
    h.click(cell(&h, id, 0, 2));
    assert_eq!(table(&h, id).focused_cell(), Some((0, 3)));
    h.click(cell(&h, id, 0, 0));
    assert_eq!(table(&h, id).focused_cell(), Some((0, 1)));

    let mut h = Harness::new();
    let columns = (0..4)
        .map(|c| TableColumn::new(format!("C{c}"), 100.0))
        .collect();
    let id = h.add(
        TableView::new(columns, grid_rows(5))
            .cell_selection(true)
            .frozen_trailing_columns(1)
            .width(502.0)
            .height(TEN_ROWS),
    );
    h.click(cell(&h, id, 0, 3));
    assert_eq!(table(&h, id).focused_cell(), Some((0, 3)));
    h.click(cell(&h, id, 0, 4));
    assert_eq!(table(&h, id).focused_cell(), Some((0, 3)));
}

#[test]
fn replacing_the_data_clears_the_focused_cell_and_the_edit() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    h.click(cell(&h, id, 1, 1));
    h.key(Key::Enter);
    h.tree
        .update::<TableView, _>(id, |t, cx| t.set_data(cx, grid_rows(3)));
    h.prepare();
    assert_eq!(table(&h, id).focused_cell(), None);
    assert_eq!(table(&h, id).editing_cell(), None);
    h.tree
        .update::<TableView, _>(id, |t, cx| t.select_cell(cx, 2, 3));
    assert_eq!(table(&h, id).focused_cell(), Some((2, 3)));
    let edited = h
        .tree
        .update::<TableView, _>(id, |t, cx| t.edit_cell(cx, 2, 0))
        .unwrap();
    assert!(!edited);
    let edited = h
        .tree
        .update::<TableView, _>(id, |t, cx| t.edit_cell(cx, 2, 2))
        .unwrap();
    assert!(edited);
    assert_eq!(table(&h, id).edit_text(), Some("r2c2"));
}

#[test]
fn a_press_where_a_scrolled_away_editor_was_commits_it_and_focuses_the_cell() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 30, 0);
    h.click(cell(&h, id, 0, 1));
    h.key(Key::Enter);
    h.type_text("x");
    h.wheel(cell(&h, id, 0, 1), -10.0 * TABLE_ROW);
    assert_eq!(table(&h, id).visible_rows().start, 10);
    h.messages();
    // The start of the cell's text, where the editor was drawn.
    h.click(h.at(id, 1.0 + 100.0 + 12.0, TABLE_HEADER + TABLE_ROW / 2.0));
    assert_eq!(table(&h, id).editing_cell(), None);
    assert_eq!(table(&h, id).focused_cell(), Some((10, 1)));
    assert_eq!(
        values::<CellEdited>(&h.messages()),
        [CellEdited {
            row: 0,
            column: 1,
            text: "x".into(),
            stored: true
        }]
    );
}

#[test]
fn a_press_on_a_frozen_cell_over_a_hidden_editor_commits_it() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 1);
    h.click(cell(&h, id, 0, 1));
    h.key(Key::Enter);
    h.modifiers(SHIFT);
    h.wheel(cell(&h, id, 0, 1), -50.0);
    h.modifiers(Modifiers::default());
    assert_eq!(table(&h, id).editing_cell(), Some((0, 1)));
    // Over the frozen column, where the editor's text starts beneath it.
    h.click(h.at(id, 70.0, TABLE_HEADER + TABLE_ROW / 2.0));
    assert_eq!(table(&h, id).editing_cell(), None);
    assert_eq!(table(&h, id).focused_cell(), Some((0, 0)));
}

#[test]
fn sorting_keeps_the_selected_cells_to_the_focused_one() {
    let mut h = Harness::new();
    let columns = vec![
        TableColumn::new("Name", 100.0).sort(ColumnSort::Text),
        TableColumn::new("Value", 100.0),
    ];
    let rows: Vec<Vec<String>> = ["b", "z", "a", "y", "c"]
        .iter()
        .map(|name| vec![name.to_string(), "1".into()])
        .collect();
    let id = h.add(
        TableView::new(columns, rows)
            .cell_selection(true)
            .width(202.0)
            .height(TEN_ROWS),
    );
    h.click(cell(&h, id, 1, 0));
    h.key_with(Key::ArrowDown, SHIFT);
    assert_eq!(table(&h, id).selected_cells().unwrap().len(), 2);
    h.messages();
    // Sorted by name, "z" (row 1) is last and "a" (row 2) first: the range between
    // them would cover every row.
    h.click(h.at(id, 50.0, TABLE_HEADER / 2.0));
    assert_eq!(table(&h, id).focused_cell(), Some((2, 0)));
    let range = table(&h, id).selected_cells().unwrap();
    assert_eq!(
        range,
        CellRange {
            rows: 0..1,
            columns: 0..1
        }
    );
    assert!(!table(&h, id).is_cell_selected(0, 0));
    assert_eq!(
        values::<CellSelectionChanged>(&h.messages()),
        [CellSelectionChanged {
            focus: (2, 0),
            range
        }]
    );
}

#[test]
fn selecting_all_cells_again_reports_nothing() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    h.click(cell(&h, id, 1, 1));
    h.messages();
    h.key_with(Key::Char('a'), CMD);
    assert_eq!(values::<CellSelectionChanged>(&h.messages()).len(), 1);
    h.key_with(Key::Char('a'), CMD);
    assert!(values::<CellSelectionChanged>(&h.messages()).is_empty());
}

fn edits(messages: &[Message]) -> Vec<(usize, usize, String)> {
    values::<CellEdited>(messages)
        .into_iter()
        .map(|e| {
            assert!(e.stored);
            (e.row, e.column, e.text)
        })
        .collect()
}

fn cell_text(h: &mut Harness, id: WidgetId, row: usize, column: usize) -> String {
    h.tree
        .update::<TableView, _>(id, |t, cx| t.select_cell(cx, row, column));
    h.prepare();
    table(h, id).copy_text().unwrap()
}

#[test]
fn cmd_c_copies_the_selected_cells_as_tab_separated_text() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    h.click(cell(&h, id, 1, 0));
    h.key_with(Key::ArrowDown, SHIFT);
    h.key_with(Key::ArrowRight, SHIFT);
    h.key_with(Key::Char('c'), CMD);
    assert_eq!(
        h.tree.clipboard().read_text().as_deref(),
        Some("r1c0\tr1c1\nr2c0\tr2c1")
    );
    assert!(edits(&h.messages()).is_empty());
}

#[test]
fn copying_and_pasting_empty_final_cells_preserves_the_rows_and_undo() {
    for source_rows in [1, 2] {
        let mut h = Harness::new();
        let id = data_grid(&mut h, 6, 0);
        let mut rows = grid_rows(6);
        rows[source_rows - 1][1].clear();
        h.tree
            .update::<TableView, _>(id, |t, cx| t.set_data(cx, rows));
        h.prepare();
        h.click(cell(&h, id, 0, 1));
        if source_rows == 2 {
            h.key_with(Key::ArrowDown, SHIFT);
        }
        h.key_with(Key::Char('c'), CMD);
        assert!(edits(&h.messages()).is_empty());

        h.click(cell(&h, id, 3, 1));
        h.messages();
        h.key_with(Key::Char('v'), CMD);
        let pasted = if source_rows == 1 {
            vec![(3, 1, String::new())]
        } else {
            vec![(3, 1, "r0c1".into()), (4, 1, String::new())]
        };
        assert_eq!(edits(&h.messages()), pasted);
        assert_eq!(
            table(&h, id).selected_cells().unwrap(),
            CellRange {
                rows: 3..3 + source_rows,
                columns: 1..2,
            }
        );

        h.key_with(Key::Char('z'), CMD);
        let restored: Vec<_> = (3..3 + source_rows)
            .rev()
            .map(|row| (row, 1, format!("r{row}c1")))
            .collect();
        assert_eq!(edits(&h.messages()), restored);
        assert!(!table(&h, id).can_undo());
        h.key_with(Key::Char('z'), SHIFT_CMD);
        assert_eq!(edits(&h.messages()), pasted);
    }
}

#[test]
fn cmd_v_pastes_a_block_from_the_focused_cell_into_editable_columns() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    h.click(cell(&h, id, 1, 1));
    h.messages();
    // Three columns wide from column 1: column 3 is read-only and keeps its text.
    h.tree
        .clipboard()
        .write_text("a\tb\tc\r\nd\t\"e\tf\"\tg\r\n");
    h.key_with(Key::Char('v'), CMD);
    let messages = h.messages();
    assert_eq!(
        edits(&messages),
        [
            (1, 1, "a".into()),
            (1, 2, "b".into()),
            (2, 1, "d".into()),
            (2, 2, "e\tf".into()),
        ]
    );
    assert_eq!(
        table(&h, id).selected_cells().unwrap(),
        CellRange {
            rows: 1..3,
            columns: 1..4
        }
    );
    assert_eq!(values::<CellSelectionChanged>(&messages).len(), 1);
    assert_eq!(
        table(&h, id).copy_text().as_deref(),
        Some("a\tb\tr1c3\nd\t\"e\tf\"\tr2c3")
    );
    assert!(!table(&h, id).is_cell_selected(3, 1));

    h.click(cell(&h, id, 4, 1));
    h.messages();
    h.key_with(Key::Char('v'), CMD);
    assert_eq!(
        edits(&h.messages()),
        [(4, 1, "a".into()), (4, 2, "b".into())]
    );

    h.key_with(Key::Char('v'), CMD);
    assert!(edits(&h.messages()).is_empty());
}

#[test]
fn one_pasted_value_fills_the_selected_range() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    h.click(cell(&h, id, 0, 0));
    h.key_with(Key::ArrowDown, SHIFT);
    h.key_with(Key::ArrowRight, SHIFT);
    h.key_with(Key::ArrowRight, SHIFT);
    h.messages();
    h.tree.clipboard().write_text("x\n");
    h.key_with(Key::Char('v'), CMD);
    assert_eq!(
        edits(&h.messages()),
        [
            (0, 1, "x".into()),
            (0, 2, "x".into()),
            (1, 1, "x".into()),
            (1, 2, "x".into()),
        ]
    );
    assert_eq!(table(&h, id).selected_cells().unwrap().len(), 6);
}

#[test]
fn delete_clears_and_cmd_x_cuts_the_selected_editable_cells() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    h.click(cell(&h, id, 0, 0));
    h.key_with(Key::ArrowRight, SHIFT);
    h.messages();
    h.key(Key::Delete);
    assert_eq!(edits(&h.messages()), [(0, 1, String::new())]);
    assert_eq!(cell_text(&mut h, id, 0, 0), "r0c0");

    h.click(cell(&h, id, 2, 1));
    h.key_with(Key::ArrowRight, SHIFT);
    h.messages();
    h.key_with(Key::Char('x'), CMD);
    assert_eq!(
        h.tree.clipboard().read_text().as_deref(),
        Some("r2c1\tr2c2")
    );
    assert_eq!(
        edits(&h.messages()),
        [(2, 1, String::new()), (2, 2, String::new())]
    );
    h.key_with(Key::Char('v'), CMD);
    assert_eq!(cell_text(&mut h, id, 2, 1), "r2c1");
    assert_eq!(cell_text(&mut h, id, 2, 2), "r2c2");
    h.key(Key::Enter);
    h.messages();
    h.key(Key::Backspace);
    assert!(edits(&h.messages()).is_empty());
    assert_eq!(table(&h, id).edit_text(), Some(""));
}

fn set_visible(h: &mut Harness, id: WidgetId, column: usize, visible: bool) {
    h.tree.update::<TableView, _>(id, |t, cx| {
        t.set_column_visible(cx, column, visible);
    });
    h.prepare();
}

#[test]
fn the_keyboard_passes_over_hidden_columns_of_a_data_grid() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    h.click(cell(&h, id, 1, 1));
    set_visible(&mut h, id, 1, false);
    assert_eq!(table(&h, id).focused_cell(), Some((1, 2)));
    h.key(Key::ArrowLeft);
    assert_eq!(table(&h, id).focused_cell(), Some((1, 0)), "past C1");
    h.key(Key::ArrowRight);
    assert_eq!(table(&h, id).focused_cell(), Some((1, 2)));
    set_visible(&mut h, id, 3, false);
    h.key(Key::End);
    assert_eq!(table(&h, id).focused_cell(), Some((1, 2)), "C3 is hidden");
    h.key(Key::ArrowRight);
    assert_eq!(table(&h, id).focused_cell(), Some((1, 2)));
    h.key_with(Key::Char('a'), CMD);
    let range = table(&h, id).selected_cells().unwrap();
    assert_eq!(range.columns, 0..3);
    let copied = table(&h, id).copy_text().unwrap();
    assert!(copied.starts_with("r0c0\tr0c2\n"), "{copied}");
}

#[test]
fn the_first_key_in_a_data_grid_skips_a_hidden_first_column() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    set_visible(&mut h, id, 0, false);
    h.tree.set_focus(Some(id));
    h.key(Key::ArrowDown);
    let (_, column) = table(&h, id).focused_cell().unwrap();
    assert_eq!(column, 1, "Down starts in the first column shown");
}

#[test]
fn pasting_into_a_data_grid_fills_only_the_columns_shown() {
    let mut h = Harness::new();
    let columns = (0..4)
        .map(|c| TableColumn::new(format!("C{c}"), 100.0).editable(true))
        .collect();
    let id = h.add(
        TableView::new(columns, grid_rows(3))
            .cell_selection(true)
            .width(402.0)
            .height(TEN_ROWS),
    );
    set_visible(&mut h, id, 1, false);
    h.click(cell(&h, id, 0, 0));
    h.messages();
    h.tree.clipboard().write_text("a\tb\n");
    h.key_with(Key::Char('v'), CMD);
    assert_eq!(
        edits(&h.messages()),
        [(0, 0, "a".into()), (0, 2, "b".into())]
    );
    assert_eq!(
        table(&h, id).selected_cells().unwrap(),
        CellRange {
            rows: 0..1,
            columns: 0..3
        }
    );
    h.key(Key::Delete);
    assert_eq!(
        edits(&h.messages()),
        [(0, 0, String::new()), (0, 2, String::new())]
    );
    let mut edited = true;
    h.tree.update::<TableView, _>(id, |t, cx| {
        edited = t.edit_cell(cx, 1, 1);
    });
    assert!(!edited);
}

const SHIFT_CMD: Modifiers = Modifiers { shift: true, ..CMD };

#[test]
fn cmd_z_takes_back_a_paste_as_a_whole_and_shift_cmd_z_does_it_again() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    assert!(!table(&h, id).can_undo());
    h.click(cell(&h, id, 1, 1));
    h.tree.clipboard().write_text("a\tb\nc\td\n");
    h.key_with(Key::Char('v'), CMD);
    assert!(table(&h, id).can_undo());
    h.click(cell(&h, id, 4, 0));
    h.messages();

    h.key_with(Key::Char('z'), CMD);
    let messages = h.messages();
    assert_eq!(
        edits(&messages),
        [
            (2, 2, "r2c2".into()),
            (2, 1, "r2c1".into()),
            (1, 2, "r1c2".into()),
            (1, 1, "r1c1".into()),
        ]
    );
    assert_eq!(
        table(&h, id).selected_cells().unwrap(),
        CellRange {
            rows: 1..3,
            columns: 1..3
        }
    );
    assert_eq!(values::<CellSelectionChanged>(&messages).len(), 1);
    assert_eq!(
        table(&h, id).copy_text().as_deref(),
        Some("r1c1\tr1c2\nr2c1\tr2c2")
    );
    assert!(!table(&h, id).can_undo());
    assert!(table(&h, id).can_redo());
    h.key_with(Key::Char('z'), CMD);
    assert!(h.messages().is_empty());

    h.key_with(Key::Char('z'), SHIFT_CMD);
    assert_eq!(
        edits(&h.messages()),
        [
            (1, 1, "a".into()),
            (1, 2, "b".into()),
            (2, 1, "c".into()),
            (2, 2, "d".into()),
        ]
    );
    assert_eq!(table(&h, id).copy_text().as_deref(), Some("a\tb\nc\td"));
    assert!(table(&h, id).can_undo());
    assert!(!table(&h, id).can_redo());
}

#[test]
fn each_edit_cut_and_clear_is_undone_on_its_own_and_a_new_change_drops_the_redo() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    h.click(cell(&h, id, 0, 1));
    h.key(Key::Enter);
    h.type_text("one");
    h.key(Key::Enter);
    h.key(Key::Enter);
    h.type_text("two");
    h.key(Key::Enter);
    h.click(cell(&h, id, 3, 1));
    h.key_with(Key::ArrowRight, SHIFT);
    h.key_with(Key::Char('x'), CMD);
    h.click(cell(&h, id, 4, 2));
    h.key(Key::Delete);
    h.messages();

    h.key_with(Key::Char('z'), CMD);
    assert_eq!(edits(&h.messages()), [(4, 2, "r4c2".into())]);
    h.key_with(Key::Char('z'), CMD);
    assert_eq!(
        edits(&h.messages()),
        [(3, 2, "r3c2".into()), (3, 1, "r3c1".into())]
    );
    h.key_with(Key::Char('z'), CMD);
    assert_eq!(edits(&h.messages()), [(0, 1, "one".into())]);
    assert_eq!(table(&h, id).focused_cell(), Some((0, 1)));
    h.key_with(Key::Char('z'), CMD);
    assert_eq!(edits(&h.messages()), [(0, 1, "r0c1".into())]);

    h.key_with(Key::Char('z'), SHIFT_CMD);
    assert_eq!(edits(&h.messages()), [(0, 1, "one".into())]);
    assert!(table(&h, id).can_redo());
    h.click(cell(&h, id, 2, 2));
    h.key(Key::Delete);
    assert!(!table(&h, id).can_redo());
    h.messages();
    h.key_with(Key::Char('z'), SHIFT_CMD);
    assert!(edits(&h.messages()).is_empty());
}

#[test]
fn ctrl_y_redoes_off_apple_platforms() {
    const CTRL: Modifiers = Modifiers {
        shift: false,
        ctrl: true,
        alt: false,
        meta: false,
        command: true,
    };
    let mut h = Harness::new();
    let id = data_grid(&mut h, 3, 0);
    h.click(cell(&h, id, 0, 1));
    h.key(Key::Delete);
    h.key_with(Key::Char('z'), CTRL);
    assert_eq!(cell_text(&mut h, id, 0, 1), "r0c1");
    h.messages();
    h.key_with(Key::Char('y'), CTRL);
    assert_eq!(edits(&h.messages()), [(0, 1, String::new())]);
}

#[test]
fn while_a_cell_is_edited_cmd_z_undoes_its_text_not_the_grid() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 3, 0);
    h.click(cell(&h, id, 0, 1));
    h.key(Key::Delete);
    h.click(cell(&h, id, 1, 1));
    h.key(Key::Enter);
    h.type_text("x");
    h.messages();
    h.key_with(Key::Char('z'), CMD);
    assert!(edits(&h.messages()).is_empty());
    assert_eq!(table(&h, id).edit_text(), Some("r1c1"));
    assert!(table(&h, id).can_undo());
}

#[test]
fn undo_and_redo_for_an_edit_menu_return_the_changes_and_new_data_forgets_them() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 3, 0);
    h.click(cell(&h, id, 2, 2));
    h.key(Key::Delete);
    h.click(cell(&h, id, 0, 0));
    h.click(cell(&h, id, 0, 1));
    h.key(Key::Enter);
    h.messages();
    let undone = h
        .tree
        .update::<TableView, _>(id, |t, cx| t.undo(cx))
        .unwrap();
    assert_eq!(
        undone,
        [CellEdited {
            row: 2,
            column: 2,
            text: "r2c2".into(),
            stored: true
        }]
    );
    h.prepare();
    assert!(edits(&h.messages()).is_empty());
    assert_eq!(table(&h, id).editing_cell(), None);
    assert_eq!(table(&h, id).focused_cell(), Some((2, 2)));
    let redone = h
        .tree
        .update::<TableView, _>(id, |t, cx| t.redo(cx))
        .unwrap();
    assert_eq!(redone[0].text, "");
    assert!(table(&h, id).can_undo());

    h.tree
        .update::<TableView, _>(id, |t, cx| t.set_data(cx, grid_rows(3)));
    assert!(!table(&h, id).can_undo());
    assert!(!table(&h, id).can_redo());
    let undone = h
        .tree
        .update::<TableView, _>(id, |t, cx| t.undo(cx))
        .unwrap();
    assert!(undone.is_empty());
}

/// Rows the app keeps itself: the table does not store edits.
#[derive(Clone)]
struct AppRows(Vec<Vec<String>>);

impl ui_core::widgets::TableData for AppRows {
    fn row_count(&self) -> usize {
        self.0.len()
    }
    fn cell(&self, row: usize, column: usize) -> std::borrow::Cow<'_, str> {
        self.0[row][column].as_str().into()
    }
}

fn apply_edits(h: &mut Harness, id: WidgetId, rows: &mut AppRows) -> Vec<(usize, usize, String)> {
    let edits: Vec<CellEdited> = values::<CellEdited>(&h.messages());
    for e in &edits {
        assert!(!e.stored);
        rows.0[e.row][e.column] = e.text.clone();
    }
    let data = rows.clone();
    h.tree
        .update::<TableView, _>(id, |t, cx| t.update_data(cx, data));
    h.prepare();
    edits
        .into_iter()
        .map(|e| (e.row, e.column, e.text))
        .collect()
}

#[test]
fn an_app_that_keeps_its_own_rows_can_undo_the_edits_it_applied() {
    let mut h = Harness::new();
    let columns = (0..4)
        .map(|c| TableColumn::new(format!("C{c}"), 100.0).editable(c == 1 || c == 2))
        .collect();
    let mut rows = AppRows(grid_rows(3));
    let id = h.add(
        TableView::new(columns, rows.clone())
            .cell_selection(true)
            .width(302.0)
            .height(TEN_ROWS),
    );
    h.click(cell(&h, id, 1, 1));
    h.key_with(Key::ArrowRight, SHIFT);
    h.key(Key::Delete);
    assert_eq!(
        apply_edits(&mut h, id, &mut rows),
        [(1, 1, String::new()), (1, 2, String::new())]
    );
    assert_eq!(table(&h, id).copy_text().as_deref(), Some("\t"));
    assert_eq!(table(&h, id).focused_cell(), Some((1, 2)));
    assert!(table(&h, id).can_undo());

    h.key_with(Key::Char('z'), CMD);
    assert_eq!(
        apply_edits(&mut h, id, &mut rows),
        [(1, 2, "r1c2".into()), (1, 1, "r1c1".into())]
    );
    assert_eq!(table(&h, id).copy_text().as_deref(), Some("r1c1\tr1c2"));
    h.key_with(Key::Char('z'), SHIFT_CMD);
    assert_eq!(apply_edits(&mut h, id, &mut rows).len(), 2);
    assert_eq!(rows.0[1][1], "");

    rows.0.push(grid_rows(4).pop().unwrap());
    let data = rows.clone();
    h.tree
        .update::<TableView, _>(id, |t, cx| t.update_data(cx, data));
    h.prepare();
    assert!(!table(&h, id).can_undo());
    assert_eq!(table(&h, id).focused_cell(), None);
}

#[test]
fn cmd_z_works_while_the_filter_hides_every_row() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 3, 0);
    h.click(cell(&h, id, 0, 1));
    h.key(Key::Delete);
    h.tree
        .update::<TableView, _>(id, |t, cx| t.set_filter_text(cx, "nothing matches"));
    h.prepare();
    assert!(table(&h, id).displayed_rows().is_empty());
    h.messages();
    h.key_with(Key::Char('z'), CMD);
    assert_eq!(edits(&h.messages()), [(0, 1, "r0c1".into())]);
    assert!(table(&h, id).can_redo());
}

#[test]
fn undo_from_a_menu_repaints_the_cancelled_editor_away_with_nothing_to_undo() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 3, 0);
    h.click(cell(&h, id, 0, 1));
    h.key(Key::Enter);
    h.type_text("typed");
    h.prepare();
    let editing = h.tree.display_list().commands().len();
    let undone = h
        .tree
        .update::<TableView, _>(id, |t, cx| t.undo(cx))
        .unwrap();
    assert!(undone.is_empty());
    assert_eq!(table(&h, id).editing_cell(), None);
    h.prepare();
    assert_ne!(h.tree.display_list().commands().len(), editing);
}

#[test]
fn cmd_d_fills_down_and_cmd_r_fills_right_into_editable_cells() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    // Rows 1-3, columns 0-2: row 1 goes down; column 0 is read-only.
    h.click(cell(&h, id, 1, 0));
    h.key_with(Key::ArrowDown, SHIFT);
    h.key_with(Key::ArrowDown, SHIFT);
    h.key_with(Key::ArrowRight, SHIFT);
    h.key_with(Key::ArrowRight, SHIFT);
    h.messages();
    h.key_with(Key::Char('d'), CMD);
    assert_eq!(
        edits(&h.messages()),
        [
            (2, 1, "r1c1".into()),
            (3, 1, "r1c1".into()),
            (2, 2, "r1c2".into()),
            (3, 2, "r1c2".into()),
        ]
    );
    assert_eq!(table(&h, id).selected_cells().unwrap().len(), 9);
    h.key_with(Key::Char('z'), CMD);
    assert_eq!(edits(&h.messages()).len(), 4);
    assert_eq!(cell_text(&mut h, id, 3, 2), "r3c2");

    h.click(cell(&h, id, 4, 1));
    h.messages();
    h.key_with(Key::Char('d'), CMD);
    assert_eq!(edits(&h.messages()), [(4, 1, "r3c1".into())]);
    h.click(cell(&h, id, 0, 1));
    h.messages();
    h.key_with(Key::Char('d'), CMD);
    assert!(edits(&h.messages()).is_empty());

    // Right, from the first column of the range (row 0, column 1 is focused).
    h.key_with(Key::ArrowRight, SHIFT);
    h.key_with(Key::ArrowRight, SHIFT);
    h.messages();
    h.key_with(Key::Char('r'), CMD);
    assert_eq!(edits(&h.messages()), [(0, 2, "r0c1".into())]);
    // One column: filled from the column to the left. (The columns have scrolled
    // sideways, so the cell is focused by its place in the data.)
    h.tree
        .update::<TableView, _>(id, |t, cx| t.select_cell(cx, 2, 1));
    h.prepare();
    h.messages();
    h.key_with(Key::Char('r'), CMD);
    assert_eq!(edits(&h.messages()), [(2, 1, "r2c0".into())]);
}

#[test]
fn the_first_arrow_key_focuses_the_first_cell() {
    for key in [Key::ArrowRight, Key::ArrowLeft, Key::ArrowDown] {
        let mut h = Harness::new();
        let id = data_grid(&mut h, 5, 0);
        h.tree.set_focus(Some(id));
        h.key(key);
        assert_eq!(table(&h, id).focused_cell(), Some((0, 0)), "{key:?}");
    }
}

#[test]
fn selecting_a_cell_right_after_replacing_the_data_scrolls_to_it() {
    let mut h = Harness::new();
    let id = data_grid(&mut h, 5, 0);
    // In one update: the table has not been laid out with the new rows yet.
    h.tree.update::<TableView, _>(id, |t, cx| {
        t.set_data(cx, grid_rows(1000));
        t.select_cell(cx, 900, 3);
    });
    h.prepare();
    assert!(table(&h, id).visible_rows().contains(&900));
    h.click(cell(&h, id, 0, 2));
    assert_eq!(table(&h, id).focused_cell().unwrap().1, 3);
}

#[test]
fn selecting_before_the_first_layout_scrolls_once_the_table_is_laid_out() {
    let mut h = Harness::new();
    let columns = (0..4)
        .map(|c| TableColumn::new(format!("C{c}"), 100.0))
        .collect();
    let id = h.tree.insert(
        Some(h.root),
        TableView::new(columns, grid_rows(1000))
            .cell_selection(true)
            .width(302.0)
            .height(TEN_ROWS),
    );
    h.tree
        .update::<TableView, _>(id, |t, cx| t.select_cell(cx, 900, 0));
    h.prepare();
    assert!(table(&h, id).visible_rows().contains(&900));

    let columns = vec![TableColumn::new("C0", 100.0)];
    let id = h.tree.insert(
        Some(h.root),
        TableView::new(columns, grid_rows(1000)).height(TEN_ROWS),
    );
    h.tree
        .update::<TableView, _>(id, |t, cx| t.select_rows(cx, [900]));
    h.prepare();
    assert!(table(&h, id).visible_rows().contains(&900));
}

#[test]
fn a_view_does_not_stay_past_the_end_when_its_content_shrinks() {
    for follows in [true, false] {
        let mut h = Harness::new();
        let (view, column) = log_view(&mut h, 5, follows);
        h.tree
            .update::<ScrollView, _>(view, |v, cx| v.scroll_to_end(cx));
        h.prepare();
        assert_eq!(scroll_y(&h, view), 100.0);
        // Three of the five rows go: the two left fit in the view.
        let gone = h.tree.children(column)[2..].to_vec();
        for id in gone {
            h.tree.remove(id, &mut h.text);
        }
        h.prepare();
        assert_eq!(scroll_y(&h, view), 0.0, "follows_end: {follows}");
        let first = h.tree.children(column)[0];
        assert_eq!(h.rect(first).y, h.rect(view).y);
    }
}

#[test]
fn scrolling_up_slowly_lets_go_of_the_end() {
    let mut h = Harness::new();
    let (view, column) = log_view(&mut h, 5, true);
    let inside = h.at(view, 50.0, 50.0);
    // Steps of half a pixel, as a trackpad on a 2x display sends them.
    for _ in 0..20 {
        h.wheel(inside, 0.5);
    }
    assert_eq!(scroll_y(&h, view), 90.0);
    assert!(!h.tree.get::<ScrollView>(view).unwrap().is_following_end());
    h.tree.insert(Some(column), fixed_row(40.0));
    h.prepare();
    assert_eq!(scroll_y(&h, view), 90.0);
}

#[test]
fn scroll_to_end_reaches_content_added_in_the_same_update() {
    let mut h = Harness::new();
    let (view, column) = log_view(&mut h, 5, false);
    for _ in 0..3 {
        h.tree.insert(Some(column), fixed_row(40.0));
    }
    // Not laid out yet: the view still knows of five rows.
    h.tree
        .update::<ScrollView, _>(view, |v, cx| v.scroll_to_end(cx));
    h.prepare();
    assert_eq!(scroll_y(&h, view), 220.0);
    h.tree.insert(Some(column), fixed_row(40.0));
    h.prepare();
    assert_eq!(scroll_y(&h, view), 220.0);
}

#[test]
fn a_position_set_before_the_layout_is_not_taken_for_the_end() {
    let mut h = Harness::new();
    let view = h.tree.insert(
        Some(h.root),
        ScrollView::new(Style {
            size: LayoutSize {
                width: length(200.0),
                height: length(100.0),
            },
            ..Default::default()
        })
        .follows_end(true),
    );
    let column = h.tree.insert(
        Some(view),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            size: LayoutSize {
                width: percent(1.0),
                height: ui_core::style::auto(),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }),
    );
    for _ in 0..5 {
        h.tree.insert(Some(column), fixed_row(40.0));
    }
    h.tree
        .update::<ScrollView, _>(view, |v, cx| v.set_offset(cx, 40.0));
    h.prepare();
    assert_eq!(scroll_y(&h, view), 40.0);
    assert!(!h.tree.get::<ScrollView>(view).unwrap().is_following_end());
}

fn days(n: usize) -> Vec<String> {
    (1..=n).map(|d| format!("Day {d}")).collect()
}

/// A 400 by 200 line chart of two series over five days.
fn line_chart(h: &mut Harness) -> WidgetId {
    h.add(
        Chart::line(days(5))
            .series(ChartSeries::new(
                "Visits",
                vec![12.0, 30.0, 18.0, 44.0, 25.0],
            ))
            .series(ChartSeries::new(
                "Sign-ups",
                vec![2.0, 5.0, f64::NAN, 9.0, 4.0],
            ))
            .format(|v| format!("{v:.0}"))
            .width(400.0)
            .height(200.0),
    )
}

fn chart(h: &Harness, id: WidgetId) -> &Chart {
    h.tree.get::<Chart>(id).unwrap()
}

fn lines_drawn(h: &Harness) -> usize {
    h.tree
        .display_list()
        .commands()
        .iter()
        .filter(|c| {
            matches!(
                c,
                DrawCommand::Shape {
                    shape: Shape::Line { .. },
                    ..
                }
            )
        })
        .count()
}

#[test]
fn a_line_chart_draws_its_series_and_breaks_lines_at_missing_values() {
    let mut h = Harness::new();
    let id = line_chart(&mut h);
    // Four segments for the first series, two for the second (the gap breaks two).
    assert_eq!(lines_drawn(&h), 4 + 2);
    assert_eq!(chart(&h, id).ticks(), (0.0, 60.0, 20.0));
}

#[test]
fn hovering_a_chart_reads_the_nearest_category_and_leaving_clears_it() {
    let mut h = Harness::new();
    let id = line_chart(&mut h);
    h.dispatch(Event::PointerMove {
        pos: h.at(id, 396.0, 100.0),
    });
    assert_eq!(chart(&h, id).inspected(), Some(4));
    h.dispatch(Event::PointerMove {
        pos: h.at(id, 120.0, 100.0),
    });
    assert_eq!(chart(&h, id).inspected(), Some(1));
    assert_eq!(
        chart(&h, id).readout(),
        Some((
            "Day 2".into(),
            vec![
                ("Visits".into(), "30".into()),
                ("Sign-ups".into(), "5".into())
            ]
        ))
    );
    h.dispatch(Event::PointerLeave);
    assert_eq!(chart(&h, id).inspected(), None);
}

#[test]
fn clicking_or_return_on_a_chart_activates_the_category() {
    let mut h = Harness::new();
    let id = line_chart(&mut h);
    h.click(h.at(id, 396.0, 100.0));
    assert_eq!(
        values::<ChartPointActivated>(&h.messages()),
        [ChartPointActivated(4)]
    );
    assert_eq!(h.tree.focused(), Some(id));

    h.key(Key::ArrowLeft);
    h.key(Key::ArrowLeft);
    assert_eq!(chart(&h, id).inspected(), Some(2));
    assert_eq!(chart(&h, id).readout().unwrap().1[1].1, "—");
    h.key(Key::Home);
    h.key(Key::Enter);
    assert_eq!(
        values::<ChartPointActivated>(&h.messages()),
        [ChartPointActivated(0)]
    );
    h.key(Key::Escape);
    assert_eq!(chart(&h, id).inspected(), None);
}

#[test]
fn the_legend_hides_and_shows_a_series_and_the_axis_and_readout_follow() {
    let mut h = Harness::new();
    let id = line_chart(&mut h);
    assert!(chart(&h, id).is_series_visible(1));
    // The legend runs along the top, "Visits" first.
    h.click(h.at(id, 4.0, 9.0));
    let messages = h.messages();
    assert_eq!(
        values::<ChartSeriesToggled>(&messages),
        [ChartSeriesToggled {
            index: 0,
            visible: false
        }]
    );
    assert!(values::<ChartPointActivated>(&messages).is_empty());
    assert!(!chart(&h, id).is_series_visible(0));
    assert_eq!(lines_drawn(&h), 2);
    assert_eq!(chart(&h, id).ticks().1, 10.0);
    h.key(Key::Home);
    assert_eq!(
        chart(&h, id).readout(),
        Some(("Day 1".into(), vec![("Sign-ups".into(), "2".into())]))
    );

    h.key(Key::Char('1'));
    assert!(chart(&h, id).is_series_visible(0));
    h.key(Key::Char('2'));
    assert_eq!(
        values::<ChartSeriesToggled>(&h.messages()),
        [
            ChartSeriesToggled {
                index: 0,
                visible: true
            },
            ChartSeriesToggled {
                index: 1,
                visible: false
            }
        ]
    );
    assert_eq!(lines_drawn(&h), 4);
    h.key(Key::Char('3'));
    assert!(values::<ChartSeriesToggled>(&h.messages()).is_empty());

    h.tree
        .update::<Chart, _>(id, |c, cx| c.set_series_visible(cx, 1, true));
    h.tree.update::<Chart, _>(id, |c, cx| {
        c.set_series_visible(cx, 0, false);
        let series = c.series_list().to_vec();
        c.set_data(cx, days(5), series);
    });
    h.prepare();
    assert!(values::<ChartSeriesToggled>(&h.messages()).is_empty());
    assert!(!chart(&h, id).is_series_visible(0));
    assert!(chart(&h, id).is_series_visible(1));
}

fn chart_access(
    h: &Harness,
    id: WidgetId,
) -> (ui_core::accesskit::Node, Vec<ui_core::accesskit::Node>) {
    use ui_core::{access_id, access_item_id};
    let update = h.tree.accessibility_update(1.0);
    let node = |n| {
        update
            .nodes
            .iter()
            .find(|(i, _)| *i == n)
            .map(|(_, node)| node.clone())
    };
    let entries = (0..).map_while(|i| node(access_item_id(id, i))).collect();
    (node(access_id(id)).unwrap(), entries)
}

#[test]
fn a_screen_reader_reads_and_toggles_the_legend_entries_as_check_boxes() {
    use ui_core::access_item_id;
    use ui_core::accesskit::{Action, ActionRequest, Role, Toggled, TreeId};
    let mut h = Harness::new();
    let id = line_chart(&mut h);
    let (figure, entries) = chart_access(&h, id);
    assert_eq!(figure.children().len(), 2);
    let read = |n: &ui_core::accesskit::Node| (n.role(), n.label().map(str::to_owned), n.toggled());
    assert_eq!(
        entries.iter().map(read).collect::<Vec<_>>(),
        [
            (Role::CheckBox, Some("Visits".into()), Some(Toggled::True)),
            (Role::CheckBox, Some("Sign-ups".into()), Some(Toggled::True)),
        ]
    );
    let (chart_box, first, second) = (
        figure.bounds().unwrap(),
        entries[0].bounds().unwrap(),
        entries[1].bounds().unwrap(),
    );
    assert!(first.y0 == chart_box.y0 && first.x0 == chart_box.x0);
    assert!(second.x0 > first.x1 && second.x1 <= chart_box.x1);
    assert!(entries.iter().all(|n| n.supports_action(Action::Click)));

    h.tree.accessibility_action(
        &ActionRequest {
            action: Action::Click,
            target_tree: TreeId::ROOT,
            target_node: access_item_id(id, 1),
            data: None,
        },
        &mut h.text,
    );
    h.prepare();
    assert_eq!(
        values::<ChartSeriesToggled>(&h.messages()),
        [ChartSeriesToggled {
            index: 1,
            visible: false
        }]
    );
    assert_eq!(chart_access(&h, id).1[1].toggled(), Some(Toggled::False));

    let single = h.add(Chart::line(days(2)).series(ChartSeries::new("Only", vec![1.0, 2.0])));
    assert!(chart_access(&h, single).1.is_empty());
}

#[test]
fn a_hidden_series_name_is_struck_through_not_dimmed() {
    let mut h = Harness::new();
    let id = line_chart(&mut h);
    let legend = |h: &Harness| {
        let top = h.rect(id).y + 18.0;
        let commands = h.tree.display_list().commands();
        let texts: Vec<(Rect, Color)> = (commands.iter())
            .filter_map(|c| match c {
                DrawCommand::Text { rect, color, .. } if rect.bottom() <= top => {
                    Some((*rect, *color))
                }
                _ => None,
            })
            .collect();
        let struck = texts
            .iter()
            .filter(|(text, _)| {
                commands.iter().any(|c| {
                    matches!(c, DrawCommand::Quad { rect, .. }
                        if rect.height == 1.0 && rect.x == text.x && rect.width == text.width
                            && rect.y > text.y && rect.bottom() < text.bottom())
                })
            })
            .count();
        (texts, struck)
    };
    let (shown, struck) = legend(&h);
    assert_eq!((shown.len(), struck), (2, 0));
    h.click(h.at(id, 4.0, 9.0));
    let (texts, struck) = legend(&h);
    assert_eq!(struck, 1);
    assert!(texts.iter().all(|(_, color)| *color == shown[1].1));
}

#[test]
fn a_legend_wider_than_the_chart_wraps_and_each_entry_can_be_clicked() {
    let mut h = Harness::new();
    let mut chart_widget = Chart::line(days(3)).width(200.0).height(200.0);
    for n in 1..=4 {
        chart_widget = chart_widget.series(ChartSeries::new(
            format!("Long series name {n}"),
            vec![1.0, 2.0, 3.0],
        ));
    }
    let id = h.add(chart_widget);
    let (figure, entries) = chart_access(&h, id);
    let chart_box = figure.bounds().unwrap();
    assert_eq!(entries.len(), 4);
    for entry in &entries {
        let b = entry.bounds().unwrap();
        assert!(
            b.x0 >= chart_box.x0 && b.x1 <= chart_box.x1,
            "{b:?} is outside {chart_box:?}"
        );
    }
    let last = entries[3].bounds().unwrap();
    assert!(last.y0 > entries[0].bounds().unwrap().y0);

    let (x, y) = (
        (last.x0 + 4.0 - chart_box.x0) as f32,
        ((last.y0 + last.y1) / 2.0 - chart_box.y0) as f32,
    );
    h.click(h.at(id, x, y));
    assert_eq!(
        values::<ChartSeriesToggled>(&h.messages()),
        [ChartSeriesToggled {
            index: 3,
            visible: false
        }]
    );
}

#[test]
fn with_every_series_hidden_the_chart_reads_no_values() {
    let mut h = Harness::new();
    let id = line_chart(&mut h);
    h.click(h.at(id, 200.0, 100.0));
    let texts = |h: &Harness| {
        let commands = h.tree.display_list().commands();
        let texts = commands
            .iter()
            .filter(|c| matches!(c, DrawCommand::Text { .. }));
        texts.count()
    };
    h.key(Key::Char('1'));
    h.key(Key::Char('2'));
    h.key(Key::Home);
    assert_eq!(chart(&h, id).inspected(), Some(0));
    let inspected = texts(&h);
    h.key(Key::Escape);
    assert_eq!(texts(&h), inspected);
    let description = chart_access(&h, id).0.description().unwrap().to_owned();
    assert_eq!(description, "Visits: hidden; Sign-ups: hidden");
    h.key(Key::Home);
    let description = chart_access(&h, id).0.description().unwrap().to_owned();
    assert_eq!(description, "Visits: hidden; Sign-ups: hidden");
}

#[test]
fn hidden_bar_series_give_their_room_to_the_others() {
    let mut h = Harness::new();
    let id = h.add(
        Chart::bar(days(2))
            .series(ChartSeries::new("A", vec![10.0, 20.0]))
            .series(ChartSeries::new("B", vec![5.0, 15.0]))
            .width(300.0)
            .height(150.0),
    );
    let widths = |h: &Harness| -> Vec<f32> {
        let plot = h.rect(id);
        h.tree
            .display_list()
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Quad { rect, .. }
                    if rect.y > plot.y + 20.0 && rect.height > 2.0 && rect.width < 200.0 =>
                {
                    Some(rect.width)
                }
                _ => None,
            })
            .collect()
    };
    let both = widths(&h);
    assert_eq!(both.len(), 4);
    h.tree
        .update::<Chart, _>(id, |c, cx| c.set_series_visible(cx, 1, false));
    h.prepare();
    let one = widths(&h);
    assert_eq!(one.len(), 2);
    assert!(one[0] > both[0] * 1.8, "{one:?} vs {both:?}");
}

#[test]
fn stacked_bars_add_up_and_fit_the_axis_to_the_totals() {
    let mut h = Harness::new();
    let id = h.add(
        Chart::bar(days(3))
            .series(ChartSeries::new("Disk", vec![30.0, 50.0, -10.0]))
            .series(ChartSeries::new("Network", vec![20.0, f64::NAN, -20.0]))
            .series(ChartSeries::new("CPU", vec![10.0, 40.0, 5.0]))
            .stacked(true)
            .width(300.0)
            .height(150.0),
    );
    assert!(chart(&h, id).is_stacked());
    assert_eq!(chart(&h, id).total(0), 60.0);
    assert_eq!(
        chart(&h, id).total(1),
        90.0,
        "a missing value counts as nothing"
    );
    // The axis reaches the tallest stack (90) and the deepest one (-30), not the
    // largest single value.
    let (lo, hi, _) = chart(&h, id).ticks();
    assert!(lo <= -30.0 && lo > -60.0, "{lo}");
    assert!((90.0..120.0).contains(&hi), "{hi}");
    let parts = |h: &Harness| -> Vec<Rect> {
        let plot = h.rect(id);
        let mut parts: Vec<Rect> = h
            .tree
            .display_list()
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Quad { rect, .. }
                    if rect.y > plot.y + 20.0 && rect.height > 2.0 && rect.width < 200.0 =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .collect();
        parts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
        parts
    };
    let drawn = parts(&h);
    assert_eq!(drawn.len(), 8, "{drawn:?}");
    let first: Vec<&Rect> = drawn.iter().filter(|r| r.x == drawn[0].x).collect();
    assert_eq!(first.len(), 3, "day 1 stacks its three parts");
    assert!(first.iter().all(|r| r.width == drawn[0].width));
    // Parts sit on one another: CPU on Network on Disk (screen y grows down).
    assert!(first[0].bottom() <= first[1].y + 1.0 && first[1].bottom() <= first[2].y + 1.0);
    h.dispatch(Event::PointerMove {
        pos: h.at(id, 250.0, 80.0),
    });
    let (label, values) = chart(&h, id).readout().unwrap();
    assert_eq!(label, "Day 3");
    assert_eq!(values.last().unwrap(), &("Total".into(), "-25".into()));
    h.dispatch(Event::PointerLeave);
    h.tree
        .update::<Chart, _>(id, |c, cx| c.set_series_visible(cx, 0, false));
    h.prepare();
    assert_eq!(chart(&h, id).total(1), 40.0);
    assert_eq!(parts(&h).len(), 5);
}

#[test]
fn pushing_onto_a_chart_with_a_capacity_drops_the_oldest_category() {
    let mut h = Harness::new();
    let id = h.add(
        Chart::line(Vec::new())
            .series(ChartSeries::new("CPU", Vec::new()))
            .series(ChartSeries::new("Memory", Vec::new()))
            .range(0.0, 100.0)
            .capacity(3)
            .width(300.0)
            .height(120.0),
    );
    for t in 0..5 {
        h.tree.update::<Chart, _>(id, |c, cx| {
            c.push(cx, format!("{t}s"), &[f64::from(t) * 10.0]);
        });
    }
    h.prepare();
    let c = chart(&h, id);
    assert_eq!(c.labels(), ["2s", "3s", "4s"]);
    assert_eq!(c.series_list()[0].values, [20.0, 30.0, 40.0]);
    assert!(c.series_list()[1].values.iter().all(|v| v.is_nan()));
    assert_eq!((c.ticks().0, c.ticks().1), (0.0, 100.0));
}

#[test]
fn bars_stand_on_zero_and_sparklines_ignore_the_pointer() {
    let mut h = Harness::new();
    let bars = h.add(
        Chart::new(ChartKind::Bar, days(3))
            .series(ChartSeries::new("Profit", vec![40.0, -15.0, 80.0]))
            .width(300.0)
            .height(150.0),
    );
    let (lo, hi, _) = chart(&h, bars).ticks();
    assert!(lo <= -15.0 && hi >= 80.0);
    let positive = h.add(
        Chart::bar(days(2))
            .series(ChartSeries::new("Sales", vec![40.0, 60.0]))
            .width(300.0)
            .height(150.0),
    );
    assert_eq!(chart(&h, positive).ticks().0, 0.0);

    let spark = h.add(
        Chart::line(days(4))
            .series(ChartSeries::new("Load", vec![1.0, 3.0, 2.0, 4.0]))
            .sparkline()
            .width(80.0)
            .height(20.0),
    );
    h.click(h.at(spark, 40.0, 10.0));
    assert_eq!(chart(&h, spark).inspected(), None);
    assert!(values::<ChartPointActivated>(&h.messages()).is_empty());
    assert_ne!(h.tree.focused(), Some(spark));
}

#[test]
fn a_line_through_values_outside_a_fixed_range_keeps_its_slope() {
    let mut h = Harness::new();
    let id = h.add(
        Chart::line(days(2))
            .series(ChartSeries::new("Delta", vec![-100.0, 100.0]))
            .range(0.0, 100.0)
            .width(400.0)
            .height(200.0),
    );
    let segment = h
        .tree
        .display_list()
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Shape {
                shape: Shape::Line { from, to, .. },
                ..
            } => Some((*from, *to)),
            _ => None,
        })
        .unwrap();
    let r = h.rect(id);
    let (from, to) = segment;
    // The days sit at the plot's two ends: the line enters at zero about halfway
    // across (not at the first day) and ends at the top at the second day.
    assert!(from.x > r.x + r.width * 0.4, "{segment:?} in {r:?}");
    assert!(from.x < r.x + r.width * 0.6, "{segment:?} in {r:?}");
    assert!(to.x > r.x + r.width * 0.9 && from.y > to.y);
}
