use ui_core::style::{Size as LayoutSize, length};
use ui_core::widgets::{
    Alert, AlertResponse, Button, ButtonClicked, Container, DESKTOP_DOCK, Desktop, InnerWindow,
    Sheet, SheetClosed, TextField, WINDOW_MIN_SIZE, WINDOW_TITLE_BAR, WindowEvent, WindowManager,
    WindowPlacement,
};
use ui_core::{
    Design, Event, Key, Message, Modifiers, PointerButton, Style, Theme, UiTree, WidgetId,
    access_id, accesskit,
};
use ui_scene::{Point, Rect, Size};
use ui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(1000.0, 700.0);
/// The desktop: 800 x 600 at the window's origin, so desktop and window coordinates agree.
const DESK: Size = Size::new(800.0, 600.0);
const AREA_H: f32 = 600.0 - DESKTOP_DOCK;

struct Harness {
    tree: UiTree,
    text: TextStore,
    manager: WindowManager,
    now: f64,
    events: Vec<WindowEvent>,
    messages: Vec<Message>,
}

fn store() -> TextStore {
    TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: false,
    })
}

fn desk_style() -> Style {
    Style {
        size: LayoutSize {
            width: length(DESK.width),
            height: length(DESK.height),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }
}

impl Harness {
    fn new() -> Self {
        let mut tree = UiTree::new();
        let root = tree.insert(None, Container::new(Style::default()));
        let manager = WindowManager::new(&mut tree, root, desk_style());
        let mut h = Self {
            tree,
            text: store(),
            manager,
            now: 100.0,
            events: Vec::new(),
            messages: Vec::new(),
        };
        h.tree.set_now(h.now);
        h.prepare();
        h
    }

    fn prepare(&mut self) {
        self.tree.prepare(VIEWPORT, &mut self.text);
    }

    fn pump(&mut self) {
        for message in self.tree.take_messages() {
            match self
                .manager
                .handle(&mut self.tree, &mut self.text, &message)
            {
                Some(event) => self.events.push(event),
                None => self.messages.push(message),
            }
        }
        self.prepare();
    }

    fn open(&mut self, title: &str, frame: Rect) -> WidgetId {
        let id = self.manager.open(&mut self.tree, title, Some(frame));
        self.prepare();
        id
    }

    fn add(&mut self, parent: WidgetId, widget: impl ui_core::Widget) -> WidgetId {
        let id = self.tree.insert(Some(parent), widget);
        self.prepare();
        id
    }

    fn dispatch(&mut self, event: Event) {
        self.tree.dispatch(event, &mut self.text);
        self.pump();
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

    fn click(&mut self, pos: Point) {
        self.down(pos);
        self.up(pos);
    }

    fn drag(&mut self, from: Point, to: Point) {
        self.down(from);
        self.dispatch(Event::PointerMove { pos: to });
        self.up(to);
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.dispatch(Event::KeyDown { key, modifiers });
        self.dispatch(Event::ModifiersChanged(Modifiers::default()));
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }

    /// Let time pass (so two clicks are not a double click) and run animations.
    fn run(&mut self, seconds: f64) {
        let end = self.now + seconds;
        while self.now < end {
            self.now += 1.0 / 60.0;
            self.tree.set_now(self.now);
            self.tree.animate(self.now, &mut self.text);
            self.pump();
        }
    }

    fn rect(&self, id: WidgetId) -> Rect {
        self.tree.window_rect(id).unwrap()
    }

    fn center(&self, id: WidgetId) -> Point {
        let r = self.rect(id);
        Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    fn title(&self, window: WidgetId) -> Point {
        let r = self.rect(window);
        Point::new(r.x + r.width / 2.0, r.y + WINDOW_TITLE_BAR / 2.0)
    }

    fn window(&self, id: WidgetId) -> &InnerWindow {
        self.tree.get::<InnerWindow>(id).unwrap()
    }

    fn clicked(&mut self) -> Vec<WidgetId> {
        std::mem::take(&mut self.messages)
            .iter()
            .filter(|m| m.downcast_ref::<ButtonClicked>().is_some())
            .map(|m| m.source)
            .collect()
    }
}

fn command() -> Modifiers {
    Modifiers {
        command: true,
        ..Default::default()
    }
}

#[test]
fn a_press_inside_a_window_brings_it_forward_and_still_reaches_its_button() {
    let mut h = Harness::new();
    let a = h.open("A", Rect::new(40.0, 40.0, 300.0, 200.0));
    let button = h.add(a, Button::new("In A"));
    let b = h.open("B", Rect::new(200.0, 120.0, 300.0, 200.0));
    assert_eq!(h.manager.active(), Some(b));
    assert_eq!(h.manager.windows(), [a, b]);
    assert!(h.window(b).is_active() && !h.window(a).is_active());
    assert_eq!(h.tree.hit_test(Point::new(250.0, 130.0)), Some(b));

    h.click(h.center(button));
    assert_eq!(h.clicked(), [button], "the button still gets the click");
    assert_eq!(h.events, [WindowEvent::Activated(a)]);
    assert_eq!(h.manager.windows(), [b, a]);
    assert_eq!(h.tree.children(h.manager.desktop()), [b, a]);
    assert!(h.window(a).is_active() && !h.window(b).is_active());
    assert_eq!(h.tree.hit_test(Point::new(250.0, 130.0)), Some(a));
    assert_eq!(h.tree.focused(), Some(button));
}

#[test]
fn bringing_a_window_forward_lays_nothing_out() {
    let mut h = Harness::new();
    let a = h.open("A", Rect::new(40.0, 40.0, 300.0, 200.0));
    h.open("B", Rect::new(200.0, 120.0, 300.0, 200.0));
    h.tree.bring_to_front(a);
    h.prepare();
    assert!(!h.tree.stats().layout);
    assert_eq!(h.tree.children(h.manager.desktop()).last(), Some(&a));
}

#[test]
fn dragging_the_title_bar_moves_the_window_and_keeps_it_reachable() {
    let mut h = Harness::new();
    let a = h.open("A", Rect::new(100.0, 100.0, 300.0, 200.0));
    let from = h.title(a);
    h.drag(from, Point::new(from.x + 130.0, from.y + 57.0));
    assert_eq!(h.rect(a), Rect::new(230.0, 157.0, 300.0, 200.0));
    h.run(1.0);
    let from = h.title(a);
    h.down(from);
    h.dispatch(Event::PointerMove {
        pos: Point::new(790.0, 560.0),
    });
    let r = h.rect(a);
    assert_eq!(r.y, AREA_H - WINDOW_TITLE_BAR);
    assert!(r.x <= DESK.width - 48.0);
    h.up(Point::new(790.0, 560.0));
    h.run(1.0);
    let from = h.title(a);
    h.drag(from, Point::new(from.x - 200.0, 40.0));
    assert!(h.rect(a).y >= 0.0);
}

#[test]
fn a_dragged_window_snaps_to_the_edges_of_the_desktop_and_of_other_windows() {
    let mut h = Harness::new();
    let other = h.open("Other", Rect::new(400.0, 300.0, 200.0, 150.0));
    let a = h.open("A", Rect::new(100.0, 100.0, 200.0, 150.0));
    // Its right edge comes within 8 px of the other's left edge: it snaps onto it.
    let from = h.title(a);
    h.drag(from, Point::new(from.x + 95.0, from.y + 60.0));
    assert_eq!(h.rect(a).right(), h.rect(other).x);
    assert_eq!(h.rect(a).y, 160.0, "no snap vertically: nothing is near");
    h.run(1.0);
    let from = h.title(a);
    h.drag(from, Point::new(from.x - 194.0, from.y));
    assert_eq!(h.rect(a).x, 0.0);
    h.run(1.0);
    let from = h.title(a);
    h.drag(from, Point::new(from.x + 20.0, from.y));
    assert_eq!(h.rect(a).x, 20.0);
}

#[test]
fn dragging_to_an_edge_of_the_desktop_offers_a_half_or_the_whole_of_it() {
    let mut h = Harness::new();
    let a = h.open("A", Rect::new(100.0, 100.0, 300.0, 200.0));
    let desktop = h.manager.desktop();
    let from = h.title(a);
    h.down(from);
    h.dispatch(Event::PointerMove {
        pos: Point::new(1.0, 200.0),
    });
    assert_eq!(
        h.tree.get::<Desktop>(desktop).unwrap().snap_preview(),
        Some(Rect::new(0.0, 0.0, 400.0, AREA_H)),
        "the outline shows the left half"
    );
    h.dispatch(Event::PointerMove {
        pos: Point::new(300.0, 200.0),
    });
    assert_eq!(h.tree.get::<Desktop>(desktop).unwrap().snap_preview(), None);
    h.dispatch(Event::PointerMove {
        pos: Point::new(799.5, 200.0),
    });
    h.up(Point::new(799.5, 200.0));
    assert_eq!(h.window(a).placement(), WindowPlacement::RightHalf);
    assert_eq!(h.rect(a), Rect::new(400.0, 0.0, 400.0, AREA_H));
    assert_eq!(h.tree.get::<Desktop>(desktop).unwrap().snap_preview(), None);

    h.run(1.0);
    let from = h.title(a);
    h.drag(from, Point::new(from.x - 100.0, from.y + 80.0));
    let r = h.rect(a);
    assert_eq!((r.width, r.height), (300.0, 200.0));
    assert!(r.x < from.x - 100.0 && from.x - 100.0 < r.right());

    h.run(1.0);
    let from = h.title(a);
    h.drag(from, Point::new(400.0, 1.0));
    assert_eq!(h.window(a).placement(), WindowPlacement::Maximized);
    assert_eq!(h.rect(a), Rect::new(0.0, 0.0, DESK.width, AREA_H));
}

#[test]
fn the_zoom_button_and_a_double_click_toggle_maximized() {
    let mut h = Harness::new();
    let frame = Rect::new(100.0, 100.0, 300.0, 200.0);
    let a = h.open("A", frame);
    let zoom = Point::new(frame.x + 10.0 + 2.0 * 19.0 + 6.0, frame.y + 13.0);
    h.click(zoom);
    assert_eq!(h.rect(a), Rect::new(0.0, 0.0, DESK.width, AREA_H));
    h.run(1.0);
    h.drag(Point::new(799.0, 300.0), Point::new(700.0, 300.0));
    assert_eq!(h.rect(a).width, DESK.width);
    h.run(1.0);
    let title = h.title(a);
    h.click(title);
    h.click(title);
    assert_eq!(h.rect(a), frame);
    h.run(1.0);
    h.click(h.title(a));
    assert_eq!(h.rect(a), frame);
}

#[test]
fn edges_and_corners_resize_down_to_the_minimum_size() {
    let mut h = Harness::new();
    let a = h.open("A", Rect::new(100.0, 100.0, 300.0, 200.0));
    h.drag(Point::new(398.0, 298.0), Point::new(448.0, 328.0));
    assert_eq!(h.rect(a), Rect::new(100.0, 100.0, 350.0, 230.0));
    h.drag(Point::new(102.0, 200.0), Point::new(62.0, 200.0));
    assert_eq!(h.rect(a), Rect::new(60.0, 100.0, 390.0, 230.0));
    h.drag(Point::new(250.0, 102.0), Point::new(250.0, 600.0));
    let r = h.rect(a);
    assert_eq!(r.height, WINDOW_MIN_SIZE.height);
    assert_eq!(r.bottom(), 330.0);
    h.drag(Point::new(448.0, 300.0), Point::new(0.0, 300.0));
    assert_eq!(h.rect(a).width, WINDOW_MIN_SIZE.width);
    let right = h.rect(a).right();
    h.drag(Point::new(right - 2.0, 300.0), Point::new(2000.0, 300.0));
    assert_eq!(h.rect(a).right(), DESK.width);
}

#[test]
fn minimizing_sends_a_window_to_the_dock_and_its_item_brings_it_back() {
    let mut h = Harness::new();
    let a = h.open("Alpha", Rect::new(40.0, 40.0, 300.0, 200.0));
    let b = h.open("Beta", Rect::new(200.0, 120.0, 300.0, 200.0));
    let desktop = h.manager.desktop();
    // Beta's minimize button (the second).
    h.click(Point::new(200.0 + 10.0 + 19.0 + 6.0, 120.0 + 13.0));
    assert!(h.manager.is_minimized(b));
    assert!(h.tree.is_hidden(b));
    assert_eq!(
        h.tree.get::<Desktop>(desktop).unwrap().dock_items(),
        ["Beta"]
    );
    assert_eq!(
        h.manager.active(),
        Some(a),
        "the next window becomes active"
    );
    assert!(h.window(a).is_active());
    assert_eq!(h.tree.focused(), Some(a));

    let item = h
        .tree
        .get::<Desktop>(desktop)
        .unwrap()
        .dock_item_rect(0)
        .unwrap();
    h.click(Point::new(
        item.x + item.width / 2.0,
        item.y + item.height / 2.0,
    ));
    assert!(!h.manager.is_minimized(b));
    assert!(!h.tree.is_hidden(b));
    assert_eq!(h.manager.active(), Some(b));
    assert!(
        h.tree
            .get::<Desktop>(desktop)
            .unwrap()
            .dock_items()
            .is_empty()
    );
    assert_eq!(h.rect(b), Rect::new(200.0, 120.0, 300.0, 200.0));
}

#[test]
fn the_close_button_and_the_keyboard_commands_work_on_the_active_window() {
    let mut h = Harness::new();
    let a = h.open("A", Rect::new(40.0, 40.0, 300.0, 200.0));
    let b = h.open("B", Rect::new(200.0, 120.0, 300.0, 200.0));
    let c = h.open("C", Rect::new(360.0, 200.0, 300.0, 200.0));
    h.key_with(Key::Char('`'), command());
    assert_eq!(h.manager.active(), Some(a));
    h.key_with(Key::Char('`'), command());
    assert_eq!(h.manager.active(), Some(b));
    assert_eq!(h.tree.focused(), Some(b));
    h.key_with(Key::Char('m'), command());
    assert!(h.manager.is_minimized(b));
    assert_eq!(h.manager.active(), Some(a));
    h.events.clear();
    h.key_with(Key::Char('w'), command());
    assert_eq!(h.events, [WindowEvent::Closed(a)]);
    assert!(!h.tree.contains(a));
    assert_eq!(h.manager.active(), Some(c));
    // The close button (the first).
    h.events.clear();
    h.click(Point::new(360.0 + 16.0, 200.0 + 13.0));
    assert_eq!(h.events, [WindowEvent::Closed(c)]);
    assert_eq!(h.manager.windows(), [b]);
    assert_eq!(h.manager.active(), None, "only a minimized window is left");
}

/// Ctrl+Tab leaves a window so the keyboard is never trapped in it (WCAG 2.1.2).
#[test]
fn ctrl_tab_leaves_the_window_that_tab_cycles_in() {
    let mut h = Harness::new();
    let root = h.tree.root().unwrap();
    let after = h.add(root, Button::new("After"));
    let a = h.open("A", Rect::new(40.0, 40.0, 300.0, 200.0));
    let a_field = h.add(a, TextField::new("A1", 120.0));
    let a_ok = h.add(a, Button::new("OK A"));
    let b = h.open("B", Rect::new(400.0, 40.0, 300.0, 200.0));
    h.add(b, TextField::new("B1", 120.0));
    h.tree.set_focus(Some(a_field));
    h.key(Key::Tab);
    h.key(Key::Tab);
    assert_eq!(h.tree.focused(), Some(a_field), "Tab stays in the window");
    let ctrl = Modifiers {
        ctrl: true,
        ..Default::default()
    };
    h.key_with(Key::Tab, ctrl);
    let next = h.tree.focused().unwrap();
    assert!(
        h.tree.ancestors(next).any(|w| w == b),
        "into the next window"
    );
    h.key_with(Key::Tab, ctrl);
    assert_eq!(h.tree.focused(), Some(after));
    h.tree.set_focus(Some(a_ok));
    h.key_with(
        Key::Tab,
        Modifiers {
            shift: true,
            ..ctrl
        },
    );
    assert_eq!(h.tree.focused(), Some(after), "backwards, round the end");
}

#[test]
fn tab_and_the_default_button_stay_inside_the_focused_window() {
    let mut h = Harness::new();
    let a = h.open("A", Rect::new(40.0, 40.0, 300.0, 200.0));
    let a_field = h.add(a, TextField::new("A1", 120.0));
    let a_ok = h.add(a, Button::new("OK A").default_button(true));
    let b = h.open("B", Rect::new(400.0, 40.0, 300.0, 200.0));
    let b_field = h.add(b, TextField::new("B1", 120.0));
    let b_ok = h.add(b, Button::new("OK B").default_button(true));
    h.tree.set_focus(Some(b_field));
    h.key(Key::Tab);
    assert_eq!(h.tree.focused(), Some(b_ok));
    h.key(Key::Tab);
    assert_eq!(h.tree.focused(), Some(b_field));
    h.key(Key::Enter);
    assert_eq!(h.clicked(), [b_ok]);
    h.click(h.center(a_field));
    h.key(Key::Enter);
    assert_eq!(h.clicked(), [a_ok]);
    let c = h.open("C", Rect::new(200.0, 300.0, 300.0, 200.0));
    let c_field = h.add(c, TextField::new("C1", 120.0));
    h.click(h.center(c_field));
    h.key(Key::Enter);
    assert_eq!(h.clicked(), []);
}

#[test]
fn a_sheet_blocks_its_own_window_only() {
    let mut h = Harness::new();
    let a = h.open("A", Rect::new(40.0, 40.0, 320.0, 260.0));
    let a_button = h.add(a, Button::new("In A"));
    let a_field = h.add(a, TextField::new("A", 120.0));
    h.add(
        a,
        Container::new(Style {
            size: LayoutSize {
                width: length(10.0),
                height: length(110.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }),
    );
    let a_low = h.add(a, Button::new("Low"));
    let b = h.open("B", Rect::new(420.0, 40.0, 300.0, 200.0));
    let b_button = h.add(b, Button::new("In B"));

    let sheet = h.manager.present_sheet(&mut h.tree, a, Sheet::new(220.0));
    let name = h.add(sheet, TextField::new("Name", 160.0));
    let cancel = h.add(sheet, Button::new("Cancel").cancel_button(true));
    h.run(0.5);
    assert!(h.manager.is_blocked(&h.tree, a) && !h.manager.is_blocked(&h.tree, b));
    let (s, w) = (h.rect(sheet), h.rect(a));
    assert_eq!(s.y, w.y + WINDOW_TITLE_BAR);
    assert!(((s.x + s.width / 2.0) - (w.x + w.width / 2.0)).abs() < 1.0);
    assert_eq!(h.manager.active(), Some(a));

    h.click(h.center(a_button));
    assert_eq!(h.clicked(), []);
    assert!(h.rect(a_low).y > s.bottom(), "not under the sheet");
    h.click(h.center(a_low));
    assert_eq!(h.clicked(), [], "content beside the sheet is blocked too");
    h.tree.set_focus(Some(a_field));
    h.dispatch(Event::Text("x".into()));
    assert_eq!(h.tree.get::<TextField>(a_field).unwrap().text(), "");
    h.click(h.center(name));
    h.dispatch(Event::Text("Report".into()));
    assert_eq!(h.tree.get::<TextField>(name).unwrap().text(), "Report");
    h.key(Key::Tab);
    assert_eq!(h.tree.focused(), Some(cancel));
    h.key(Key::Tab);
    assert_eq!(h.tree.focused(), Some(name));
    h.click(h.center(b_button));
    assert_eq!(h.clicked(), [b_button]);
    assert_eq!(h.manager.active(), Some(b));
    let from = Point::new(w.x + 150.0, w.y + 13.0);
    h.drag(from, Point::new(from.x, from.y + 30.0));
    assert_eq!(h.rect(a).y, 70.0);

    h.click(h.center(name));
    h.messages.clear();
    h.key(Key::Escape);
    assert_eq!(h.clicked(), [cancel]);
    Sheet::close(&mut h.tree, sheet);
    h.run(0.5);
    assert!(
        h.messages
            .iter()
            .any(|m| m.downcast_ref::<SheetClosed>().is_some())
    );
    assert!(!h.tree.contains(sheet));
    assert!(!h.manager.is_blocked(&h.tree, a));
    assert_eq!(h.tree.focused(), Some(a), "focus goes back to the window");
    h.click(h.center(a_button));
    assert_eq!(h.clicked(), [a_button]);
}

#[test]
fn an_alert_in_a_window_is_answered_from_the_keyboard_and_removes_itself() {
    let mut h = Harness::new();
    let a = h.open("A", Rect::new(40.0, 40.0, 340.0, 420.0));
    let a_ok = h.add(a, Button::new("OK").default_button(true));
    let alert = h.manager.present_alert(
        &mut h.tree,
        a,
        Alert::new("Discard changes?")
            .button("Discard")
            .button("Cancel"),
    );
    h.run(0.5);
    assert_eq!(h.tree.focused(), Some(alert));
    assert_eq!(h.rect(alert).y, h.rect(a).y + WINDOW_TITLE_BAR);
    h.key(Key::Escape);
    let response = h
        .messages
        .iter()
        .find_map(|m| m.downcast_ref::<AlertResponse>().copied())
        .expect("answered");
    assert_eq!(response.button, 1);
    assert_eq!(
        h.clicked(),
        [],
        "the window's own default button was not pressed"
    );
    h.run(0.5);
    assert!(!h.tree.contains(alert));
    assert_eq!(h.tree.focused(), Some(a));
    h.key(Key::Enter);
    assert_eq!(h.clicked(), [a_ok]);
}

#[test]
fn dropping_a_window_outside_the_desktop_reports_it_and_its_content_can_move_to_another_tree() {
    let mut h = Harness::new();
    let a = h.open("Notes", Rect::new(500.0, 100.0, 280.0, 200.0));
    let field = h.add(a, TextField::new("Note", 160.0));
    h.click(h.center(field));
    h.dispatch(Event::Text("kept".into()));
    h.run(1.0);
    let from = h.title(a);
    let outside = Point::new(DESK.width + 80.0, 150.0);
    h.events.clear();
    h.drag(from, outside);
    assert_eq!(
        h.events,
        [WindowEvent::TornOff {
            window: a,
            at: outside
        }]
    );
    assert!(h.tree.contains(a));

    let widgets = h.tree.len();
    let (title, content) = h.manager.take_window(&mut h.tree, &mut h.text, a).unwrap();
    assert_eq!(title, "Notes");
    assert!(!h.tree.contains(a) && !h.tree.contains(field));
    assert_eq!(h.tree.len(), widgets - 2);
    assert!(h.manager.windows().is_empty());

    let mut other = UiTree::new();
    let root = other.insert(None, Container::new(Style::default()));
    let mut manager = WindowManager::new(&mut other, root, desk_style());
    let window = manager.adopt(&mut other, &title, content);
    other.prepare(VIEWPORT, &mut h.text);
    let moved = other.children(window)[0];
    assert_eq!(other.get::<TextField>(moved).unwrap().text(), "kept");
    assert_eq!(other.get::<InnerWindow>(window).unwrap().title(), "Notes");
    other.set_focus(Some(moved));
    other.dispatch(Event::Text("!".into()), &mut h.text);
    assert_eq!(other.get::<TextField>(moved).unwrap().text(), "kept!");
}

#[test]
fn windows_and_dock_items_are_exposed_to_assistive_technology() {
    let mut h = Harness::new();
    let a = h.open("Alpha", Rect::new(40.0, 40.0, 300.0, 200.0));
    let b = h.open("Beta", Rect::new(200.0, 120.0, 300.0, 200.0));
    h.manager.minimize(&mut h.tree, b);
    h.prepare();
    let update = h.tree.accessibility_update(1.0);
    let node = |id| update.nodes.iter().find(|(n, _)| *n == id).map(|(_, n)| n);
    let window = node(access_id(a)).unwrap();
    assert_eq!(window.role(), accesskit::Role::Window);
    assert_eq!(window.label(), Some("Alpha"));
    let labels: Vec<_> = window
        .children()
        .iter()
        .filter_map(|&c| node(c).unwrap().label().map(str::to_owned))
        .collect();
    assert_eq!(labels, ["Close", "Minimize", "Zoom"]);
    assert!(node(access_id(b)).is_none(), "minimized windows are hidden");
    let desktop = node(access_id(h.manager.desktop())).unwrap();
    let dock: Vec<_> = desktop
        .children()
        .iter()
        .filter_map(|&c| node(c).unwrap().label().map(str::to_owned))
        .collect();
    assert!(dock.contains(&"Restore Beta".to_owned()));
    let item = *desktop
        .children()
        .iter()
        .find(|&&c| node(c).unwrap().label() == Some("Restore Beta"))
        .unwrap();
    h.tree.accessibility_action(
        &accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: item,
            data: None,
        },
        &mut h.text,
    );
    h.pump();
    assert_eq!(h.manager.active(), Some(b));
}

#[test]
fn windows_designs_put_the_caption_buttons_at_the_top_right() {
    let mut h = Harness::new();
    h.tree
        .set_theme(Theme::light().with_design(Design::Fluent2));
    let a = h.open("A", Rect::new(40.0, 40.0, 300.0, 200.0));
    let b = h.open("B", Rect::new(400.0, 40.0, 300.0, 200.0));
    // Where the macOS close button would be is now just the title bar.
    h.events.clear();
    h.click(Point::new(400.0 + 16.0, 40.0 + 13.0));
    assert!(h.events.is_empty());
    // Close is the rightmost caption button, minimize the leftmost of the three.
    h.click(Point::new(400.0 + 300.0 - 10.0, 40.0 + 13.0));
    assert_eq!(h.events, [WindowEvent::Closed(b)]);
    h.click(Point::new(40.0 + 300.0 - 3.0 * 34.0 + 10.0, 40.0 + 13.0));
    assert!(h.manager.is_minimized(a));
}

/// The quads of the last frame that are `size` large, as (fill, border color).
fn quads_of(h: &Harness, size: Size) -> Vec<(ui_scene::Color, ui_scene::Color)> {
    h.tree
        .display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            ui_scene::DrawCommand::Quad {
                rect,
                color,
                border,
                ..
            } if rect.width == size.width && rect.height == size.height => {
                Some((*color, border.color))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn noa_marks_the_front_window_with_her_orange() {
    use ui_core::{Appearance, Role};
    let mut h = Harness::new();
    let theme = Theme::new(Design::Noa, Appearance::Dark);
    let (accent, outline) = (theme.color(Role::Accent), theme.color(Role::Outline));
    h.tree.set_theme(theme);
    h.open("A", Rect::new(40.0, 40.0, 300.0, 200.0));
    h.open("B", Rect::new(400.0, 40.0, 300.0, 200.0));
    let frames: Vec<_> = quads_of(&h, Size::new(300.0, 200.0))
        .into_iter()
        .map(|(_, border)| border)
        .collect();
    assert_eq!(frames, [outline, accent]);
    let glass: Vec<_> = h
        .tree
        .display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            ui_scene::DrawCommand::Backdrop {
                rect, blur, tint, ..
            } if rect.width == 300.0 => Some((*blur > 0.0, *tint)),
            _ => None,
        })
        .collect();
    let base = h.tree.theme().background;
    assert_eq!(
        glass,
        [(true, base.with_alpha(ui_core::widget::NOA_BASE_OPACITY)); 2]
    );
    assert!(h.tree.theme().surface.a < 0.5, "a wash over the glass");
    let cursors: Vec<_> = quads_of(&h, Size::new(7.0, 13.0))
        .into_iter()
        .map(|(fill, _)| fill)
        .collect();
    assert_eq!(cursors, [outline, accent]);
    let buttons: Vec<_> = quads_of(&h, Size::new(12.0, 12.0))
        .into_iter()
        .map(|(fill, _)| fill)
        .collect();
    assert_eq!(buttons, [[outline; 3], [accent; 3]].concat());
    let glow = quads_of(&h, Size::new(306.0, 206.0));
    assert_eq!(
        glow,
        [(accent.with_alpha(0.07), ui_scene::Color::TRANSPARENT); 2]
    );

    h.tree.set_theme(Theme::new(Design::Noa, Appearance::Light));
    h.prepare();
    assert!(quads_of(&h, Size::new(306.0, 206.0)).is_empty());
    let shadow = quads_of(&h, Size::new(305.0, 205.0));
    assert_eq!(shadow.len(), 2);
    assert_eq!(shadow[0].0.to_rgba8()[..3], [0, 0, 0]);
}
