use std::borrow::Cow;

use ui_core::style::{FlexDirection, Size as LayoutSize, length, percent};
use ui_core::widgets::{
    ComboBox, Container, InlineEditableLabel, Label, ResizableBox, ResizeEdges, SearchField,
    SplitPane, SplitView, TABLE_HEADER, TableColumn, TableData, TableView, TextArea, TextField,
    WindowManager,
};
use ui_core::{
    Cursor, DragData, Event, EventCx, Key, Modifiers, PaintCx, PointerButton, Style, UiTree,
    Widget, WidgetId,
};
use ui_scene::{Point, Rect, Size};
use ui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(800.0, 600.0);

struct Harness {
    tree: UiTree,
    text: TextStore,
    root: WidgetId,
}

impl Harness {
    fn new() -> Self {
        Self::with_root(Style {
            flex_direction: FlexDirection::Column,
            size: LayoutSize {
                width: percent(1.0),
                height: percent(1.0),
            },
            ..Default::default()
        })
    }

    fn with_root(style: Style) -> Self {
        let mut tree = UiTree::new();
        let root = tree.insert(None, Container::new(style));
        let mut h = Self {
            tree,
            text: TextStore::new(FontOptions {
                locale: "en-US".into(),
                load_system_fonts: false,
            }),
            root,
        };
        h.prepare();
        h
    }

    fn prepare(&mut self) {
        self.tree.prepare(VIEWPORT, &mut self.text);
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

    fn rect(&self, id: WidgetId) -> Rect {
        self.tree.window_rect(id).unwrap()
    }

    fn dispatch(&mut self, event: Event) {
        self.tree.dispatch(event, &mut self.text);
        self.prepare();
    }

    fn cursor_at(&mut self, pos: Point) -> Cursor {
        self.dispatch(Event::PointerMove { pos });
        self.tree.cursor()
    }

    fn down(&mut self, pos: Point) {
        self.dispatch(Event::PointerDown {
            pos,
            button: PointerButton::Primary,
        });
    }

    fn up(&mut self, pos: Point) {
        self.dispatch(Event::PointerUp {
            pos,
            button: PointerButton::Primary,
        });
    }
}

fn center(r: Rect) -> Point {
    Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
}

struct Link;

impl Widget for Link {
    fn style(&self) -> Style {
        Style {
            size: LayoutSize {
                width: length(120.0),
                height: length(40.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }
    }

    fn paint(&mut self, _cx: &mut PaintCx) {}

    fn cursor(&self, _local: Point, _size: Size) -> Option<Cursor> {
        Some(Cursor::Pointer)
    }
}

#[test]
fn the_pointer_is_an_arrow_until_it_is_over_text_and_after_it_leaves() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("Name", 200.0));
    let spare = h.add(Link);
    assert_eq!(h.tree.cursor(), Cursor::Default, "no pointer yet");
    let over = center(h.rect(field));
    assert_eq!(h.cursor_at(over), Cursor::Text);
    assert_eq!(h.cursor_at(Point::new(700.0, 550.0)), Cursor::Default);
    assert_eq!(h.cursor_at(center(h.rect(spare))), Cursor::Pointer);
    h.cursor_at(over);
    h.dispatch(Event::PointerLeave);
    assert_eq!(h.tree.cursor(), Cursor::Default);
}

#[test]
fn the_innermost_widget_that_answers_wins_and_others_fall_back_to_their_parent() {
    let mut h = Harness::new();
    let link = h.add(Link);
    let label = h.add_to(link, Label::new("Docs"));
    assert_eq!(h.cursor_at(center(h.rect(label))), Cursor::Pointer);
    let selectable = h.add_to(link, Label::new("Copy me").selectable(true));
    assert_eq!(h.cursor_at(center(h.rect(selectable))), Cursor::Text);
}

#[test]
fn disabled_widgets_show_the_arrow() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("Name", 200.0));
    let over = center(h.rect(field));
    assert_eq!(h.cursor_at(over), Cursor::Text);
    h.tree.set_enabled(field, false);
    h.prepare();
    assert_eq!(h.cursor_at(over), Cursor::Default);
}

#[test]
fn text_inputs_show_the_i_beam_only_over_their_text() {
    let mut h = Harness::new();
    let area = h.add(TextArea::new("Notes"));
    assert_eq!(h.cursor_at(center(h.rect(area))), Cursor::Text);
    let combo = h.add(ComboBox::new(["One", "Two"], 200.0));
    let r = h.rect(combo);
    assert_eq!(
        h.cursor_at(Point::new(r.x + 30.0, r.y + r.height / 2.0)),
        Cursor::Text
    );
    assert_eq!(
        h.cursor_at(Point::new(r.right() - 4.0, r.y + r.height / 2.0)),
        Cursor::Default
    );
    let search = h.add(SearchField::new("Search", 200.0));
    assert_eq!(h.cursor_at(center(h.rect(search))), Cursor::Text);
}

#[test]
fn an_editable_label_shows_the_i_beam_only_while_it_is_edited() {
    let mut h = Harness::new();
    let label = h.add(InlineEditableLabel::new("Untitled"));
    let over = center(h.rect(label));
    assert_eq!(h.cursor_at(over), Cursor::Default);
    h.tree.set_focus(Some(label));
    h.dispatch(Event::KeyDown {
        key: Key::Enter,
        modifiers: Modifiers::default(),
    });
    assert!(
        h.tree
            .get::<InlineEditableLabel>(label)
            .unwrap()
            .is_editing()
    );
    assert_eq!(h.cursor_at(over), Cursor::Text);
}

#[test]
fn split_view_dividers_show_a_resize_pointer_for_the_whole_drag() {
    let mut h = Harness::with_root(Style {
        size: LayoutSize {
            width: length(600.0),
            height: length(400.0),
        },
        ..Default::default()
    });
    let split = h.add(
        SplitView::horizontal()
            .pane(SplitPane::fixed(150.0).min(100.0))
            .pane(SplitPane::flexible().min(120.0)),
    );
    for _ in 0..2 {
        h.add_to(split, Container::new(Style::default()));
    }
    let divider = Point::new(150.0 + 3.0, 200.0);
    assert_eq!(h.cursor_at(divider), Cursor::ColResize);
    assert_eq!(h.cursor_at(Point::new(60.0, 200.0)), Cursor::Default);
    // Dragging faster than the divider follows (here beyond the pane's limit) keeps the
    // resize pointer, because the split view captures the pointer.
    h.down(divider);
    assert_eq!(h.cursor_at(Point::new(40.0, 200.0)), Cursor::ColResize);
    h.up(Point::new(40.0, 200.0));
    assert_eq!(h.cursor_at(Point::new(40.0, 200.0)), Cursor::Default);

    let mut h = Harness::with_root(Style {
        size: LayoutSize {
            width: length(600.0),
            height: length(400.0),
        },
        ..Default::default()
    });
    let stacked = h.add(
        SplitView::vertical()
            .pane(SplitPane::fixed(100.0))
            .pane(SplitPane::flexible()),
    );
    for _ in 0..2 {
        h.add_to(stacked, Container::new(Style::default()));
    }
    assert_eq!(h.cursor_at(Point::new(300.0, 103.0)), Cursor::RowResize);
}

#[test]
fn resizable_boxes_show_the_pointer_for_the_edge_or_corner_under_it() {
    let mut h = Harness::new();
    let resizable = h.add(ResizableBox::new(Size::new(200.0, 100.0)).edges(ResizeEdges::ALL));
    let r = h.rect(resizable);
    assert_eq!(h.cursor_at(center(r)), Cursor::Default);
    let right = Point::new(r.right() - 1.0, r.y + 50.0);
    assert_eq!(h.cursor_at(right), Cursor::EwResize);
    assert_eq!(
        h.cursor_at(Point::new(r.x + 100.0, r.bottom() - 1.0)),
        Cursor::NsResize
    );
    assert_eq!(
        h.cursor_at(Point::new(r.right() - 1.0, r.bottom() - 1.0)),
        Cursor::NwseResize
    );
    assert_eq!(
        h.cursor_at(Point::new(r.right() - 1.0, r.y + 1.0)),
        Cursor::NeswResize
    );
    h.down(right);
    assert_eq!(
        h.cursor_at(Point::new(r.right() + 80.0, 400.0)),
        Cursor::EwResize
    );
    h.up(Point::new(r.right() + 80.0, 400.0));
}

#[test]
fn inner_windows_show_resize_pointers_on_their_edges() {
    let mut h = Harness::new();
    let mut manager = WindowManager::new(
        &mut h.tree,
        h.root,
        Style {
            size: LayoutSize {
                width: length(800.0),
                height: length(600.0),
            },
            ..Default::default()
        },
    );
    let window = manager.open(
        &mut h.tree,
        "Notes",
        Some(Rect::new(100.0, 80.0, 300.0, 200.0)),
    );
    h.prepare();
    let r = h.rect(window);
    assert_eq!(
        h.cursor_at(Point::new(r.x + 1.0, r.y + 100.0)),
        Cursor::EwResize
    );
    assert_eq!(
        h.cursor_at(Point::new(r.x + 1.0, r.bottom() - 1.0)),
        Cursor::NeswResize
    );
    assert_eq!(h.cursor_at(center(r)), Cursor::Default);
}

struct Rows;

impl TableData for Rows {
    fn row_count(&self) -> usize {
        3
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        format!("{row}:{column}").into()
    }
}

#[test]
fn table_column_edges_show_a_column_resize_pointer() {
    let mut h = Harness::new();
    let table = h.add(TableView::new(
        vec![
            TableColumn::new("Name", 120.0),
            TableColumn::new("Size", 80.0).resizable(false),
            TableColumn::new("Kind", 100.0),
        ],
        Rows,
    ));
    let r = h.rect(table);
    let header_y = r.y + TABLE_HEADER / 2.0;
    assert_eq!(
        h.cursor_at(Point::new(r.x + 120.0, header_y)),
        Cursor::ColResize
    );
    assert_eq!(
        h.cursor_at(Point::new(r.x + 200.0, header_y)),
        Cursor::Default
    );
    assert_eq!(
        h.cursor_at(Point::new(r.x + 60.0, header_y)),
        Cursor::Default
    );
    assert_eq!(
        h.cursor_at(Point::new(r.x + 120.0, r.y + TABLE_HEADER + 10.0)),
        Cursor::Default
    );
}

#[test]
fn resize_edges_pick_the_pointer_for_their_direction() {
    assert_eq!(ResizeEdges::NONE.cursor(), None);
    assert_eq!(ResizeEdges::BOTTOM_RIGHT.cursor(), Some(Cursor::NwseResize));
    assert_eq!(
        Cursor::for_edges(true, false, true, false),
        Some(Cursor::NwseResize)
    );
    assert_eq!(
        Cursor::for_edges(false, true, true, false),
        Some(Cursor::NeswResize)
    );
    assert_eq!(
        Cursor::for_edges(false, false, true, false),
        Some(Cursor::NsResize)
    );
}

struct Draggable {
    pressed: bool,
}

impl Widget for Draggable {
    fn style(&self) -> Style {
        Link.style()
    }

    fn paint(&mut self, _cx: &mut PaintCx) {}

    fn event(&mut self, cx: &mut EventCx, event: &Event) {
        match event {
            Event::PointerDown { .. } => {
                self.pressed = true;
                cx.capture_pointer();
                cx.set_handled();
            }
            Event::PointerMove { .. } if self.pressed && cx.has_capture() => {
                cx.start_drag(DragData::Text("row".into()));
                self.pressed = false;
            }
            _ => {}
        }
    }

    fn cursor(&self, _local: Point, _size: Size) -> Option<Cursor> {
        Some(Cursor::Grab)
    }
}

#[test]
fn a_drag_and_drop_shows_the_closed_hand_until_it_ends() {
    let mut h = Harness::new();
    let row = h.add(Draggable { pressed: false });
    let at = center(h.rect(row));
    assert_eq!(h.cursor_at(at), Cursor::Grab);
    h.down(at);
    assert_eq!(
        h.cursor_at(Point::new(at.x, at.y + 200.0)),
        Cursor::Grabbing
    );
    h.up(Point::new(at.x, at.y + 200.0));
    assert_eq!(h.cursor_at(at), Cursor::Grab);
}
