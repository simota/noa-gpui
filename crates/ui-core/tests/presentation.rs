use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, percent};
use noa_gpui_core::widgets::{
    Alert, AlertResponse, AlertStyle, Button, ButtonClicked, ColorPanel, Container,
    ContextMenuArea, FontChoice, FontPanel, Label, MenuItem, PANEL_TITLE_BAR, POPOVER_ARROW, Panel,
    PanelClosed, PanelStyle, PopUpButton, Popover, Pressable, Sheet, SheetClosed, TableView,
    TextField, Tooltip,
};
use noa_gpui_core::{
    Edge, Event, Key, Message, Modifiers, OverlayDismissed, PointerButton, Shortcut,
    ShortcutPressed, Style, UiTree, WidgetId, access_id, accesskit,
};
use noa_gpui_scene::{Color, DrawCommand, Point, Rect, Size};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(900.0, 600.0);
const FRAME: f64 = 1.0 / 60.0;

struct Harness {
    tree: UiTree,
    text: TextStore,
    root: WidgetId,
    now: f64,
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
        let mut h = Self {
            tree,
            text: TextStore::new(FontOptions {
                locale: "en-US".into(),
                load_system_fonts: false,
            }),
            root,
            now: 10.0,
        };
        h.tree.set_now(h.now);
        h
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

    fn center(&self, id: WidgetId) -> Point {
        let r = self.rect(id);
        Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    fn dispatch(&mut self, event: Event) {
        self.tree.dispatch(event, &mut self.text);
        self.prepare();
    }

    fn click(&mut self, pos: Point) {
        self.dispatch(Event::PointerDown {
            pos,
            button: PointerButton::Primary,
        });
        self.dispatch(Event::PointerUp {
            pos,
            button: PointerButton::Primary,
        });
    }

    fn hover(&mut self, pos: Point) {
        self.dispatch(Event::PointerMove { pos });
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.dispatch(Event::KeyDown { key, modifiers });
        self.dispatch(Event::ModifiersChanged(Modifiers::default()));
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }

    fn run(&mut self, seconds: f64) {
        let end = self.now + seconds;
        while self.now < end {
            self.now += FRAME;
            self.tree.set_now(self.now);
            self.tree.animate(self.now, &mut self.text);
            self.prepare();
        }
    }

    fn messages(&mut self) -> Vec<Message> {
        self.tree.take_messages()
    }

    fn overlays(&self) -> Vec<WidgetId> {
        self.tree.overlays().collect()
    }
}

fn find<T: Copy + 'static>(messages: &[Message]) -> Option<(WidgetId, T)> {
    messages
        .iter()
        .find_map(|m| m.downcast_ref::<T>().map(|t| (m.source, *t)))
}

fn command() -> Modifiers {
    Modifiers {
        command: true,
        ..Default::default()
    }
}

fn min_quad_alpha(tree: &UiTree) -> f32 {
    tree.display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Quad { color, .. } if color.a > 0.0 => Some(color.a),
            _ => None,
        })
        .fold(1.0, f32::min)
}

#[test]
fn an_alert_is_centered_takes_focus_and_return_answers_the_default_button() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("Name", 200.0));
    h.tree.set_focus(Some(field));
    let alert = Alert::new("Delete “Report”?")
        .informative_text("This cannot be undone.")
        .style(AlertStyle::Critical)
        .button("Delete")
        .button("Cancel")
        .open(&mut h.tree);
    h.prepare();
    assert_eq!(h.tree.focused(), Some(alert));
    let r = h.rect(alert);
    assert_eq!(r.width, 260.0);
    assert!(
        (r.x - (VIEWPORT.width - r.width) / 2.0).abs() < 0.5,
        "centered"
    );
    assert!((r.y - (VIEWPORT.height - r.height) / 3.0).abs() < 0.5);

    h.run(FRAME);
    assert!(h.tree.get::<Alert>(alert).unwrap().focused_control() == 0);
    h.key(Key::Enter);
    let response = find::<AlertResponse>(&h.messages()).expect("answered");
    assert_eq!(
        response,
        (
            alert,
            AlertResponse {
                button: 0,
                suppressed: false
            }
        )
    );
    assert!(h.tree.contains(alert));
    h.run(0.5);
    assert!(!h.tree.contains(alert));
    assert_eq!(h.tree.focused(), Some(field));
}

#[test]
fn an_alerts_text_is_centered_line_by_line() {
    let mut h = Harness::new();
    let informative = "The file was changed by another application since you opened it. \
                       Keep your version?";
    Alert::new("Save changes?")
        .informative_text(informative)
        .button("Keep")
        .open(&mut h.tree);
    h.prepare();
    h.run(0.5);
    // The wrapped informative text: its last, shorter line starts right of the others.
    let starts: Vec<f32> = h
        .tree
        .display_list()
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text { layout, .. } => Some(*layout),
            _ => None,
        })
        .map(|layout| h.text.layouts().buffer(layout))
        .find(|buffer| buffer.lines.iter().any(|l| l.text() == informative))
        .expect("the informative text is drawn")
        .layout_runs()
        .map(|run| run.glyphs.first().map_or(0.0, |g| g.x))
        .collect();
    assert!(starts.len() >= 2, "it wraps: {starts:?}");
    let widest_start = starts.iter().copied().fold(f32::MAX, f32::min);
    assert!(widest_start < 1.0, "{starts:?}");
    assert!(starts.last().unwrap() - widest_start > 4.0, "{starts:?}");
}

#[test]
fn escape_and_command_period_answer_the_cancel_button() {
    for (key, modifiers) in [
        (Key::Escape, Modifiers::default()),
        (Key::Char('.'), command()),
    ] {
        let mut h = Harness::new();
        let alert = Alert::new("Save changes?")
            .button("Save")
            .button("Cancel")
            .button("Don’t Save")
            .open(&mut h.tree);
        h.prepare();
        h.key_with(key, modifiers);
        let (_, response) = find::<AlertResponse>(&h.messages()).expect("answered");
        assert_eq!(response.button, 1, "{key:?}");
        h.run(0.5);
        assert!(!h.tree.contains(alert));
    }
    let mut h = Harness::new();
    let alert = Alert::new("Done").open(&mut h.tree);
    h.prepare();
    h.key(Key::Escape);
    assert!(find::<AlertResponse>(&h.messages()).is_none());
    assert!(h.tree.contains(alert));
    assert_eq!(h.tree.get::<Alert>(alert).unwrap().buttons(), ["OK"]);
}

#[test]
fn two_short_buttons_sit_side_by_side_with_the_default_on_the_right() {
    let mut h = Harness::new();
    let alert = Alert::new("Quit?")
        .button("Quit")
        .button("Cancel")
        .open(&mut h.tree);
    h.prepare();
    let r = h.rect(alert);
    let bottom = Point::new(r.x + r.width * 0.75, r.bottom() - 18.0 - 14.0);
    h.click(bottom);
    let (_, response) = find::<AlertResponse>(&h.messages()).unwrap();
    assert_eq!(response.button, 0, "the right half is the default");

    // Three buttons are stacked, default on top: the bottom one is the last.
    let mut h = Harness::new();
    let alert = Alert::new("Save?")
        .button("Save")
        .button("Cancel")
        .button("Don’t Save")
        .open(&mut h.tree);
    h.prepare();
    let r = h.rect(alert);
    h.click(Point::new(r.x + 30.0, r.bottom() - 18.0 - 14.0));
    let (_, response) = find::<AlertResponse>(&h.messages()).unwrap();
    assert_eq!(response.button, 2);
}

#[test]
fn the_suppression_checkbox_is_reported_and_tab_space_reach_every_control() {
    let mut h = Harness::new();
    let alert = Alert::new("Empty the Trash?")
        .button("Empty")
        .button("Cancel")
        .suppression("Do not ask again")
        .open(&mut h.tree);
    h.prepare();
    // Tab: Empty → Cancel → checkbox; Space toggles it, Tab wraps to Empty.
    h.key(Key::Tab);
    h.key(Key::Tab);
    assert_eq!(h.tree.get::<Alert>(alert).unwrap().focused_control(), 2);
    h.key(Key::Space);
    assert!(h.tree.get::<Alert>(alert).unwrap().is_suppressed());
    assert_eq!(h.tree.focused(), Some(alert), "Tab stays in the alert");
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    h.key_with(Key::Tab, shift);
    h.key(Key::Space);
    let (_, response) = find::<AlertResponse>(&h.messages()).unwrap();
    assert_eq!(
        response,
        AlertResponse {
            button: 1,
            suppressed: true
        }
    );
}

#[test]
fn alerts_are_exposed_as_modal_dialogs_with_their_buttons() {
    let mut h = Harness::new();
    let alert = Alert::new("Replace “a.txt”?")
        .informative_text("A file with that name exists.")
        .button("Replace")
        .button("Cancel")
        .suppression("Always")
        .open(&mut h.tree);
    h.prepare();
    let update = h.tree.accessibility_update(1.0);
    let node = |id| update.nodes.iter().find(|(n, _)| *n == id).map(|(_, n)| n);
    let dialog = node(access_id(alert)).unwrap();
    assert_eq!(dialog.role(), accesskit::Role::AlertDialog);
    assert!(dialog.is_modal());
    assert_eq!(dialog.label(), Some("Replace “a.txt”?"));
    let roles: Vec<_> = dialog
        .children()
        .iter()
        .map(|&c| node(c).unwrap().role())
        .collect();
    assert_eq!(
        roles,
        [
            accesskit::Role::Button,
            accesskit::Role::Button,
            accesskit::Role::CheckBox
        ]
    );
    h.tree.accessibility_action(
        &accesskit::ActionRequest {
            action: accesskit::Action::Click,
            target_tree: accesskit::TreeId::ROOT,
            target_node: dialog.children()[1],
            data: None,
        },
        &mut h.text,
    );
    let (_, response) = find::<AlertResponse>(&h.messages()).unwrap();
    assert_eq!(response.button, 1);
}

#[test]
fn an_alert_fades_in_and_an_alert_sheet_slides_down() {
    let mut h = Harness::new();
    h.add(Container::new(Style::default()).background(Color::WHITE));
    let alert = Alert::new("Hello").open(&mut h.tree);
    h.prepare();
    // The first frame starts the fade at 0 (nothing drawn); the next is part way.
    h.run(FRAME * 2.0);
    let early = min_quad_alpha(&h.tree);
    assert!(early < 0.5, "faded at first: {early}");
    h.run(0.5);
    let bg = h
        .tree
        .display_list()
        .commands()
        .iter()
        .any(|c| matches!(c, DrawCommand::Quad { color, .. } if color.a == 1.0));
    assert!(bg, "opaque once shown");
    h.tree.close_overlay(alert, &mut h.text);

    let sheet = Alert::new("Hello").open_as_sheet(&mut h.tree, 40.0);
    h.prepare();
    assert_eq!(h.rect(sheet).y, 40.0, "hangs from the top edge given");
    h.run(FRAME * 2.0);
    // The alert's own panel: the opaque 260-wide rounded rectangle.
    let panel_top = |tree: &UiTree| {
        tree.display_list()
            .commands()
            .iter()
            .find_map(|c| match c {
                DrawCommand::Quad { rect, color, .. } if color.a == 1.0 && rect.width == 260.0 => {
                    Some(rect.y)
                }
                _ => None,
            })
            .expect("the alert's panel")
    };
    let sliding = panel_top(&h.tree);
    h.run(0.5);
    let shown = panel_top(&h.tree);
    assert!(sliding < shown - 5.0, "starts above its edge: {sliding}");
    assert_eq!(shown, 40.0);
}

struct Background {
    button: WidgetId,
    field: WidgetId,
    default_button: WidgetId,
    context: WidgetId,
}

fn background(h: &mut Harness) -> Background {
    let button = h.add(Button::new("Behind"));
    let field = h.add(TextField::new("Field", 200.0));
    let default_button = h.add(Button::new("Default").default_button(true));
    let context = h.add(ContextMenuArea::new(
        Style {
            size: LayoutSize {
                width: noa_gpui_core::style::length(100.0),
                height: noa_gpui_core::style::length(40.0),
            },
            ..Default::default()
        },
        vec![MenuItem::new("Copy")],
    ));
    h.tree.add_shortcut(Shortcut::command('s'), button);
    Background {
        button,
        field,
        default_button,
        context,
    }
}

fn poke(h: &mut Harness, bg: &Background) -> Vec<String> {
    let mut through = Vec::new();
    h.messages();
    let menus = h.overlays().len();
    h.click(h.center(bg.button));
    h.dispatch(Event::PointerDown {
        pos: h.center(bg.context),
        button: PointerButton::Secondary,
    });
    if h.overlays().len() > menus {
        through.push("context menu".into());
    }
    h.tree.set_focus(Some(bg.field));
    h.dispatch(Event::Text("x".into()));
    if h.tree.get::<TextField>(bg.field).unwrap().text() == "x" {
        through.push("text".into());
    }
    h.key_with(Key::Char('s'), command());
    h.key(Key::Enter);
    for m in h.messages() {
        if m.downcast_ref::<ButtonClicked>().is_some() {
            through.push(if m.source == bg.default_button {
                "default button".into()
            } else {
                "click".into()
            });
        }
        if m.downcast_ref::<ShortcutPressed>().is_some() {
            through.push("shortcut".into());
        }
    }
    through
}

#[test]
fn a_modal_alert_blocks_clicks_keys_text_shortcuts_and_context_menus_below_it() {
    let mut h = Harness::new();
    let bg = background(&mut h);
    let alert = Alert::new("Wait").button("OK").open(&mut h.tree);
    h.prepare();
    assert_eq!(poke(&mut h, &bg), Vec::<String>::new());
    h.run(0.5);
    assert!(!h.tree.contains(alert), "Return answered the alert");

    let got = poke(&mut h, &bg);
    for expected in ["click", "context menu", "text", "shortcut"] {
        assert!(got.iter().any(|g| g == expected), "{expected} in {got:?}");
    }
}

#[test]
fn a_sheet_blocks_input_below_but_its_own_controls_work() {
    let mut h = Harness::new();
    let bg = background(&mut h);
    let sheet = Sheet::new(320.0).open(&mut h.tree, 0.0);
    let name = h.tree.insert(Some(sheet), TextField::new("Name", 200.0));
    let cancel = h
        .tree
        .insert(Some(sheet), Button::new("Cancel").cancel_button(true));
    h.prepare();
    h.run(0.5);
    assert_eq!(poke(&mut h, &bg), Vec::<String>::new());
    h.click(h.center(name));
    h.dispatch(Event::Text("Report".into()));
    assert_eq!(h.tree.get::<TextField>(name).unwrap().text(), "Report");
    h.messages();
    h.key(Key::Escape);
    assert_eq!(
        find::<ButtonClicked>(&h.messages()).map(|m| m.0),
        Some(cancel)
    );
    Sheet::close(&mut h.tree, sheet);
    h.prepare();
    assert!(h.tree.get::<Sheet>(sheet).unwrap().is_closing());
    h.run(0.5);
    assert_eq!(find::<SheetClosed>(&h.messages()).map(|m| m.0), Some(sheet));
    assert!(!h.tree.contains(sheet));
    assert!(poke(&mut h, &bg).iter().any(|g| g == "click"));
}

#[test]
fn a_sheet_hangs_from_its_edge_and_slides_its_content_down() {
    let mut h = Harness::new();
    let sheet = Sheet::new(300.0).open(&mut h.tree, 52.0);
    let label = h.tree.insert(Some(sheet), Label::new("Export as"));
    h.prepare();
    let r = h.rect(sheet);
    assert_eq!(
        (r.x, r.y, r.width),
        ((VIEWPORT.width - 300.0) / 2.0, 52.0, 300.0)
    );
    h.run(FRAME * 2.0);
    let sliding = h.rect(label).y;
    h.run(0.5);
    let shown = h.rect(label).y;
    assert!(
        sliding < shown - 5.0,
        "content slides down: {sliding} → {shown}"
    );
    assert_eq!(shown, 52.0 + 20.0);
}

#[test]
fn with_reduced_motion_a_sheet_and_an_alert_show_and_hide_on_the_next_frame() {
    let mut h = Harness::new();
    h.tree.set_reduce_motion(true);
    assert!(h.tree.reduces_motion());
    let sheet = Sheet::new(300.0).open(&mut h.tree, 52.0);
    let label = h.tree.insert(Some(sheet), Label::new("Export as"));
    h.prepare();
    h.run(FRAME);
    assert_eq!(h.rect(label).y, 52.0 + 20.0, "in place on the first frame");
    assert!(!h.tree.wants_animation_frame(), "no frames after that");
    h.tree.close_overlay(sheet, &mut h.text);

    h.add(Container::new(Style::default()).background(Color::WHITE));
    let alert = Alert::new("Hello").open(&mut h.tree);
    h.prepare();
    h.run(FRAME);
    assert!(!h.tree.wants_animation_frame());
    assert!(h.overlays().contains(&alert));
    h.key(Key::Enter);
    h.run(FRAME);
    assert!(!h.overlays().contains(&alert), "gone after one frame");
}

#[test]
fn a_popover_points_at_its_anchor_and_flips_when_there_is_no_room() {
    let mut h = Harness::new();
    let anchor = Rect::new(400.0, 100.0, 80.0, 24.0);
    let popover = Popover::new().open(&mut h.tree, anchor, Edge::Bottom);
    h.tree.insert(Some(popover), Label::new("Popover content"));
    h.prepare();
    let r = h.rect(popover);
    assert!(r.y >= anchor.bottom(), "below");
    assert!(
        ((r.x + r.width / 2.0) - (anchor.x + anchor.width / 2.0)).abs() < 0.5,
        "centered on the anchor"
    );
    assert_eq!(
        h.tree.get::<Popover>(popover).unwrap().edge(),
        Some(Edge::Bottom)
    );
    // The arrow is drawn at the top, pointing up at the anchor's middle.
    let arrow = h.tree.display_list().commands().iter().any(|c| {
        matches!(c, DrawCommand::Shape { shape: noa_gpui_scene::Shape::Triangle { rect, direction: noa_gpui_scene::Direction::Up }, .. }
            if (rect.y - r.y).abs() < 2.0 && (rect.x + rect.width / 2.0 - 440.0).abs() < 2.0)
    });
    assert!(arrow, "arrow at the top, under the anchor's middle");
    h.tree.close_overlay(popover, &mut h.text);

    let low = Rect::new(400.0, 560.0, 80.0, 24.0);
    let popover = Popover::new().open(&mut h.tree, low, Edge::Bottom);
    h.tree.insert(Some(popover), Label::new("Content"));
    h.prepare();
    assert_eq!(
        h.tree.get::<Popover>(popover).unwrap().edge(),
        Some(Edge::Top)
    );
    assert!(h.rect(popover).bottom() <= low.y);
    h.tree.close_overlay(popover, &mut h.text);
    let popover = Popover::new().open(&mut h.tree, anchor, Edge::Right);
    h.tree.insert(Some(popover), Label::new("Content"));
    h.prepare();
    let r = h.rect(popover);
    assert_eq!(r.x, anchor.right() + noa_gpui_core::overlay::ANCHOR_GAP);
    assert!(r.width > 2.0 * POPOVER_ARROW);
}

#[test]
fn a_click_outside_or_escape_closes_a_popover_and_the_click_still_lands() {
    let mut h = Harness::new();
    let button = h.add(Button::new("Elsewhere"));
    let anchor = Rect::new(400.0, 100.0, 80.0, 24.0);
    let popover = Popover::new().open(&mut h.tree, anchor, Edge::Bottom);
    h.tree.insert(Some(popover), Label::new("Content"));
    h.prepare();
    h.click(h.center(popover));
    assert!(h.tree.contains(popover));
    h.messages();
    h.click(h.center(button));
    let messages = h.messages();
    assert_eq!(
        find::<OverlayDismissed>(&messages).map(|m| m.0),
        Some(popover)
    );
    assert_eq!(find::<ButtonClicked>(&messages).map(|m| m.0), Some(button));
    assert!(!h.tree.contains(popover));

    let popover = Popover::new().open(&mut h.tree, anchor, Edge::Bottom);
    h.prepare();
    h.key(Key::Escape);
    assert!(!h.tree.contains(popover));
}

#[test]
fn a_popover_opened_from_a_pressable_box_closes_on_the_next_press_of_it() {
    let mut h = Harness::new();
    let disk = h.add(Pressable::new(Style {
        size: noa_gpui_core::style::Size {
            width: noa_gpui_core::style::length(120.0),
            height: noa_gpui_core::style::length(32.0),
        },
        ..Default::default()
    }));
    h.tree.insert(Some(disk), Label::new("12.4 GB"));
    h.prepare();
    h.click(h.center(disk));
    assert_eq!(
        find::<ButtonClicked>(&h.messages()).map(|m| m.0),
        Some(disk)
    );
    let popover = Popover::new().open_from(&mut h.tree, disk, Edge::Bottom);
    h.tree.insert(Some(popover), Label::new("Images 8.1 GB"));
    h.prepare();
    assert!(h.rect(popover).y >= h.rect(disk).bottom());
    assert_eq!(
        h.tree.get::<Popover>(popover).unwrap().edge(),
        Some(Edge::Bottom)
    );

    // Pressing the box again closes it, and the box does not click, so the app does
    // not open it again.
    h.click(h.center(disk));
    let messages = h.messages();
    assert_eq!(
        find::<OverlayDismissed>(&messages).map(|m| m.0),
        Some(popover)
    );
    assert!(find::<ButtonClicked>(&messages).is_none());
    assert!(!h.tree.contains(popover));
    h.click(h.center(disk));
    assert_eq!(
        find::<ButtonClicked>(&h.messages()).map(|m| m.0),
        Some(disk)
    );
}

#[test]
fn a_popover_that_closes_on_click_closes_after_a_button_in_it_clicks() {
    let mut h = Harness::new();
    let anchor = Rect::new(400.0, 100.0, 80.0, 24.0);
    let popover = Popover::new()
        .closes_on_click(true)
        .open(&mut h.tree, anchor, Edge::Bottom);
    let label = h
        .tree
        .insert(Some(popover), Label::new("Restart the stack?"));
    let restart = h.tree.insert(Some(popover), Button::new("Restart"));
    h.prepare();
    h.click(h.center(label));
    assert!(h.tree.contains(popover));
    assert!(h.messages().is_empty());
    h.click(h.center(restart));
    let messages = h.messages();
    let order: Vec<_> = messages
        .iter()
        .map(|m| {
            (
                m.downcast_ref::<ButtonClicked>().is_some(),
                m.downcast_ref::<OverlayDismissed>().is_some(),
            )
        })
        .collect();
    assert_eq!(order, [(true, false), (false, true)]);
    assert_eq!(messages[0].source, restart);
    assert_eq!(messages[1].source, popover);
    assert!(!h.tree.contains(popover));

    let popover = Popover::new().open(&mut h.tree, anchor, Edge::Bottom);
    let restart = h.tree.insert(Some(popover), Button::new("Restart"));
    h.prepare();
    h.click(h.center(restart));
    assert_eq!(
        find::<ButtonClicked>(&h.messages()).map(|m| m.0),
        Some(restart)
    );
    assert!(h.tree.contains(popover));
}

#[test]
fn a_tooltip_shows_after_the_pointer_rests_and_hides_when_it_leaves() {
    let mut h = Harness::new();
    let save = h.add(Button::new("Save").tooltip("Save the document"));
    let plain = h.add(Button::new("Plain"));
    let over = h.center(save);
    h.hover(over);
    let due = h.tree.next_timer().expect("pending");
    assert!((due - (h.now + noa_gpui_core::tree::TOOLTIP_DELAY)).abs() < 1e-9);
    assert!(h.tree.tooltip_overlay().is_none());
    h.run(noa_gpui_core::tree::TOOLTIP_DELAY - 0.1);
    assert!(h.tree.tooltip_overlay().is_none(), "not yet");
    h.run(0.15);
    let tip = h.tree.tooltip_overlay().expect("shown");
    assert_eq!(
        h.tree.get::<Tooltip>(tip).unwrap().text(),
        "Save the document"
    );
    assert!(h.tree.next_timer().is_none());
    assert!(h.rect(tip).y > over.y);
    assert_eq!(
        h.tree.hit_test(h.center(tip)),
        h.tree.hit_test(h.center(tip))
    );
    assert_ne!(h.tree.hit_test(h.center(tip)), Some(tip));

    h.hover(Point::new(over.x + 3.0, over.y));
    assert_eq!(h.tree.tooltip_overlay(), Some(tip));
    h.hover(h.center(plain));
    assert!(h.tree.tooltip_overlay().is_none());
    assert!(!h.tree.contains(tip));
    assert!(h.tree.next_timer().is_none(), "Plain has no tooltip");
}

#[test]
fn right_after_one_tooltip_the_next_shows_at_once() {
    let mut h = Harness::new();
    let a = h.add(Button::new("A").tooltip("First"));
    let b = h.add(Button::new("B").tooltip("Second"));
    h.hover(h.center(a));
    h.run(1.0);
    assert!(h.tree.tooltip_overlay().is_some());
    h.hover(h.center(b));
    let tip = h.tree.tooltip_overlay().expect("no delay");
    assert_eq!(h.tree.get::<Tooltip>(tip).unwrap().text(), "Second");
    h.dispatch(Event::PointerLeave);
    h.run(1.0);
    h.hover(h.center(a));
    assert!(h.tree.tooltip_overlay().is_none());
    assert!(h.tree.next_timer().is_some());
}

#[test]
fn a_click_hides_the_tooltip_until_the_pointer_moves_to_another_widget() {
    let mut h = Harness::new();
    let a = h.add(Button::new("A").tooltip("First"));
    h.hover(h.center(a));
    h.run(1.0);
    assert!(h.tree.tooltip_overlay().is_some());
    h.click(h.center(a));
    assert!(h.tree.tooltip_overlay().is_none());
    h.hover(Point::new(h.center(a).x + 2.0, h.center(a).y));
    h.run(1.0);
    assert!(
        h.tree.tooltip_overlay().is_none(),
        "not again on the same widget"
    );
    let b = h.add(Button::new("B").tooltip("Second"));
    h.hover(h.center(b));
    h.run(1.0);
    assert!(h.tree.tooltip_overlay().is_some());
    h.key(Key::Char('x'));
    assert!(h.tree.tooltip_overlay().is_none());
}

#[test]
fn tooltips_fade_in_and_describe_their_widget_to_assistive_technology() {
    let mut h = Harness::new();
    let save = h.add(Button::new("Save").tooltip("Save the document"));
    let update = h.tree.accessibility_update(1.0);
    let node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == access_id(save))
        .unwrap();
    assert_eq!(node.1.description(), Some("Save the document"));
    h.hover(h.center(save));
    h.run(noa_gpui_core::tree::TOOLTIP_DELAY + FRAME * 0.5);
    let tip = h.tree.tooltip_overlay().unwrap();
    let r = h.rect(tip);
    let alpha_of_tip = |tree: &UiTree| {
        tree.display_list().commands().iter().find_map(|c| match c {
            DrawCommand::Quad { rect, color, .. } if *rect == r => Some(color.a),
            _ => None,
        })
    };
    h.run(FRAME);
    let early = alpha_of_tip(&h.tree).unwrap_or(0.0);
    h.run(0.3);
    let late = alpha_of_tip(&h.tree).unwrap();
    assert!(early < late, "fades in: {early} → {late}");
}

#[test]
fn icon_only_toolbar_items_name_themselves_in_tooltips() {
    use noa_gpui_core::widgets::{Toolbar, ToolbarDisplay, ToolbarItem, ToolbarSymbol};
    let mut h = Harness::new();
    let toolbar = h.add(
        Toolbar::new(vec![
            ToolbarItem::button("Share").symbol(ToolbarSymbol::Share),
            ToolbarItem::button("Get Info").symbol(ToolbarSymbol::Info),
        ])
        .display(ToolbarDisplay::IconOnly),
    );
    h.prepare();
    let item = h
        .tree
        .get::<Toolbar>(toolbar)
        .unwrap()
        .item_rect(1)
        .unwrap();
    let origin = h.rect(toolbar);
    h.hover(Point::new(
        origin.x + item.x + item.width / 2.0,
        origin.y + item.y + item.height / 2.0,
    ));
    h.run(1.0);
    let tip = h.tree.tooltip_overlay().expect("shown");
    assert_eq!(h.tree.get::<Tooltip>(tip).unwrap().text(), "Get Info");
}

#[test]
fn a_panel_floats_without_blocking_moves_by_its_title_bar_and_closes() {
    let mut h = Harness::new();
    let button = h.add(Button::new("Behind"));
    let panel = Panel::new("Inspector").open(&mut h.tree, Point::new(500.0, 100.0));
    h.tree.insert(Some(panel), Label::new("Width"));
    h.prepare();
    h.run(0.3);
    assert_eq!(h.rect(panel).x, 500.0);
    h.click(h.center(button));
    assert_eq!(
        find::<ButtonClicked>(&h.messages()).map(|m| m.0),
        Some(button)
    );
    assert!(h.tree.contains(panel));

    let grab = Point::new(600.0, 100.0 + PANEL_TITLE_BAR / 2.0);
    h.dispatch(Event::PointerDown {
        pos: grab,
        button: PointerButton::Primary,
    });
    h.hover(Point::new(grab.x - 50.0, grab.y + 30.0));
    h.hover(Point::new(grab.x - 80.0, grab.y + 40.0));
    h.dispatch(Event::PointerUp {
        pos: Point::new(grab.x - 80.0, grab.y + 40.0),
        button: PointerButton::Primary,
    });
    assert_eq!(
        (h.rect(panel).x, h.rect(panel).y),
        (420.0, 140.0),
        "moved with the pointer"
    );
    let r = h.rect(panel);
    let grab = Point::new(r.x + 100.0, r.y + 5.0);
    h.dispatch(Event::PointerDown {
        pos: grab,
        button: PointerButton::Primary,
    });
    h.hover(Point::new(grab.x + 2000.0, grab.y - 2000.0));
    h.dispatch(Event::PointerUp {
        pos: grab,
        button: PointerButton::Primary,
    });
    let r = h.rect(panel);
    assert_eq!((r.right(), r.y), (VIEWPORT.width, 0.0));

    h.click(Point::new(r.x + 15.0, r.y + PANEL_TITLE_BAR / 2.0));
    assert_eq!(find::<PanelClosed>(&h.messages()).map(|m| m.0), Some(panel));
    h.run(0.3);
    assert!(!h.tree.contains(panel));
}

#[test]
fn a_hud_panel_draws_a_dark_translucent_backdrop() {
    let mut h = Harness::new();
    let panel = Panel::new("HUD")
        .style(PanelStyle::Hud)
        .open(&mut h.tree, Point::new(100.0, 100.0));
    h.prepare();
    let r = h.rect(panel);
    let backdrop = h.tree.display_list().commands().iter().any(|c| {
        matches!(c, DrawCommand::Backdrop { rect, tint, .. } if *rect == r && tint.r < 0.2 && tint.a < 1.0)
    });
    assert!(backdrop);
    assert_eq!(
        h.tree.get::<Panel>(panel).unwrap().panel_style(),
        PanelStyle::Hud
    );
}

#[test]
fn the_color_panel_reports_the_picked_color() {
    let mut h = Harness::new();
    let panel = ColorPanel::open(&mut h.tree, Color::hex(0xff0000), Point::new(100.0, 80.0));
    h.prepare();
    assert!(panel.is_open(&h.tree));
    let picker = h.tree.children(panel.panel())[0];
    let r = h.rect(picker);
    h.click(Point::new(r.x + 30.0, r.y + 30.0));
    let messages = h.messages();
    let color = messages
        .iter()
        .find_map(|m| panel.handle(m))
        .expect("picked");
    assert!(color.r > color.b);
    assert_ne!(color, Color::hex(0xff0000));
}

#[test]
fn the_font_panel_changes_family_typeface_and_size() {
    let mut h = Harness::new();
    let choice = FontChoice {
        family: "Inter".into(),
        size: 13.0,
        bold: false,
        italic: false,
    };
    let mut panel = FontPanel::open(&mut h.tree, &h.text, choice, Point::new(100.0, 60.0));
    h.prepare();
    let children = h.tree.children(panel.panel()).to_vec();
    let list = children[0];
    let table = h.tree.get::<TableView>(list).unwrap();
    assert_eq!(table.row_count(), 1);
    assert_eq!(table.selected_rows(), [0]);
    // Typeface: Regular → Bold Italic; size: 13 → 24 pt (three steps each).
    let controls = h.tree.children(children[1]).to_vec();
    assert_eq!(
        h.tree.get::<PopUpButton>(controls[1]).unwrap().selected(),
        4
    );
    let mut chosen = Vec::new();
    for control in controls {
        h.tree.set_focus(Some(control));
        for _ in 0..3 {
            h.key(Key::ArrowDown);
        }
        for m in h.messages() {
            if let Some(c) = panel.handle(&mut h.tree, &m) {
                chosen.push(c);
            }
        }
    }
    let last = chosen.last().expect("changed");
    assert!(last.bold && last.italic);
    assert_eq!(last.size, 24.0);
    assert_eq!(panel.choice(), last);
    let other = h.add(Button::new("x"));
    h.click(h.center(other));
    for m in h.messages() {
        assert!(panel.handle(&mut h.tree, &m).is_none());
    }
    // The preview shows the font (capped at 36 px): bold italic 24 is taller than 13.
    let preview = *children.last().unwrap();
    assert!(h.rect(preview).height > 26.0);
}
