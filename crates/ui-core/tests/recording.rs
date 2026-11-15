use noa_gpui_core::recording::{ParseError, Recording, Step};
use noa_gpui_core::style::{FlexDirection, Size as LayoutSize, percent};
use noa_gpui_core::widgets::{Button, ButtonClicked, Container, TextChanged, TextField};
use noa_gpui_core::{Event, Key, Message, Modifiers, PointerButton, Style, UiTree, WidgetId};
use noa_gpui_scene::{Point, Size};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(600.0, 400.0);

struct Form {
    tree: UiTree,
    text: TextStore,
    field: WidgetId,
    button: WidgetId,
}

impl Form {
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
        let field = tree.insert(Some(root), TextField::new("", 200.0));
        let button = tree.insert(Some(root), Button::new("Save"));
        let mut text = TextStore::new(FontOptions {
            locale: "en-US".into(),
            ..Default::default()
        });
        tree.prepare(VIEWPORT, &mut text);
        Self {
            tree,
            text,
            field,
            button,
        }
    }

    fn at(&self, id: WidgetId, x: f32, y: f32) -> Point {
        let r = self.tree.window_rect(id).unwrap();
        Point::new(r.x + x, r.y + y)
    }

    fn send(&mut self, at: f64, event: Event) {
        self.tree.set_now(at);
        self.tree.dispatch(event, &mut self.text);
        self.tree.prepare(VIEWPORT, &mut self.text);
    }

    fn click(&mut self, at: f64, pos: Point) {
        let button = PointerButton::Primary;
        self.send(at, Event::PointerDown { pos, button });
        self.send(at + 0.05, Event::PointerUp { pos, button });
    }

    fn field_text(&self) -> String {
        self.tree
            .get::<TextField>(self.field)
            .unwrap()
            .text()
            .to_owned()
    }

    fn clicks(messages: &[Message]) -> usize {
        messages
            .iter()
            .filter(|m| m.downcast_ref::<ButtonClicked>().is_some())
            .count()
    }
}

/// A session: type into the field, double-click the first word to select it, type
/// over it, compose with the IME, and press the button.
fn session(form: &mut Form) {
    let field = form.at(form.field, 20.0, 10.0);
    form.click(0.0, field);
    form.send(0.2, Event::Text("hello world".into()));
    let word = form.at(form.field, 14.0, 10.0);
    form.click(0.5, word);
    form.click(0.6, word);
    form.send(0.9, Event::Text("bye".into()));
    for (at, composition) in [(1.0, "にほん"), (1.1, "日本"), (1.2, "")] {
        form.send(
            at,
            Event::Composition {
                text: composition.into(),
                cursor: Some((composition.len(), composition.len())),
            },
        );
    }
    form.send(1.3, Event::Text("日本".into()));
    form.send(
        1.4,
        Event::KeyDown {
            key: Key::Char('a'),
            modifiers: Modifiers {
                command: true,
                ..Default::default()
            },
        },
    );
    let button = form.at(form.button, 10.0, 8.0);
    form.send(1.6, Event::PointerMove { pos: button });
    form.click(1.7, button);
}

#[test]
fn a_recorded_session_replays_to_the_same_state_after_saving() {
    let mut original = Form::new();
    original.tree.start_recording();
    session(&mut original);
    let recording = original.tree.stop_recording();
    let messages = original.tree.take_messages();
    assert_eq!(original.field_text(), "bye日本 world");
    assert_eq!(Form::clicks(&messages), 1);

    let saved = recording.to_text();
    let loaded = Recording::parse(&saved).expect("parse");
    assert_eq!(loaded, recording);

    let mut replayed = Form::new();
    loaded.replay(&mut replayed.tree, &mut replayed.text);
    let replayed_messages = replayed.tree.take_messages();
    assert_eq!(replayed.field_text(), original.field_text());
    assert_eq!(
        replayed
            .tree
            .get::<TextField>(replayed.field)
            .unwrap()
            .selection(),
        original
            .tree
            .get::<TextField>(original.field)
            .unwrap()
            .selection()
    );
    assert_eq!(Form::clicks(&replayed_messages), 1);
    let texts = |messages: &[Message]| -> Vec<String> {
        messages
            .iter()
            .filter_map(|m| m.downcast_ref::<TextChanged>().map(|t| t.0.clone()))
            .collect()
    };
    assert_eq!(texts(&replayed_messages), texts(&messages));
}

#[test]
fn only_input_is_recorded_and_taking_keeps_recording() {
    let mut form = Form::new();
    form.send(0.0, Event::Text("before".into()));
    form.tree.start_recording();
    form.send(0.1, Event::FocusChanged(true));
    form.send(0.2, Event::PointerLeave);
    assert_eq!(
        form.tree.take_recorded(),
        [Step {
            at: 0.2,
            event: Event::PointerLeave
        }]
    );
    form.send(0.3, Event::ModifiersChanged(Modifiers::default()));
    assert_eq!(form.tree.stop_recording().steps.len(), 1);
    form.send(0.4, Event::PointerLeave);
    assert!(form.tree.take_recorded().is_empty(), "stopped");
}

#[test]
fn every_input_event_survives_the_saved_form() {
    let shift_cmd = Modifiers {
        shift: true,
        command: true,
        ..Modifiers::on_apple()
    };
    let events = [
        Event::PointerMove {
            pos: Point::new(1.5, 2.0),
        },
        Event::PointerDown {
            pos: Point::new(3.0, 4.0),
            button: PointerButton::Secondary,
        },
        Event::PointerUp {
            pos: Point::new(3.0, 4.0),
            button: PointerButton::Middle,
        },
        Event::PointerLeave,
        Event::Wheel {
            pos: Point::new(10.0, 20.0),
            delta: Point::new(0.0, -12.5),
        },
        Event::KeyDown {
            key: Key::Char(' '),
            modifiers: shift_cmd,
        },
        Event::KeyDown {
            key: Key::Char('\''),
            modifiers: Modifiers::default(),
        },
        Event::KeyDown {
            key: Key::PageDown,
            modifiers: Modifiers::default(),
        },
        Event::KeyDown {
            key: Key::F5,
            modifiers: Modifiers::default(),
        },
        Event::KeyDown {
            key: Key::F12,
            modifiers: shift_cmd,
        },
        Event::Text("a \"quoted\" \\ line\nand tab\t".into()),
        Event::Composition {
            text: "にほん".into(),
            cursor: None,
        },
        Event::Composition {
            text: "日本".into(),
            cursor: Some((0, 3)),
        },
        Event::ModifiersChanged(shift_cmd),
    ];
    let recording = Recording {
        steps: events
            .iter()
            .enumerate()
            .map(|(i, event)| Step::new(i as f64 * 0.125, event).unwrap())
            .collect(),
        multi_click: None,
    };
    let saved = recording.to_text();
    assert_eq!(saved.lines().count(), events.len());
    assert_eq!(Recording::parse(&saved), Ok(recording.clone()));

    // A step built by hand with an event that is not input is left out when saved.
    let mut with_focus = recording.clone();
    with_focus.steps.push(Step {
        at: 9.0,
        event: Event::FocusChanged(true),
    });
    assert_eq!(with_focus.to_text(), saved);
}

#[test]
fn saved_recordings_skip_comments_and_report_bad_lines() {
    let text = "# a session\n\n0.000 move 1 2\n0.100 jump 3 4\n";
    assert_eq!(
        Recording::parse(text),
        Err(ParseError {
            line: 4,
            message: "unknown event: jump".into()
        })
    );
    let ok = Recording::parse("# only\n0.5 text \"x\"\n").unwrap();
    assert_eq!(ok.steps[0].event, Event::Text("x".into()));
}

/// A session recorded with a slower double click (the OS setting) replays the same
/// on a tree that has never seen that setting.
#[test]
fn a_recording_keeps_the_double_click_interval_it_was_made_with() {
    let mut form = Form::new();
    form.tree.set_multi_click(0.8, 4.0);
    form.tree.start_recording();
    let field = form.at(form.field, 100.0, 10.0);
    form.click(0.0, field);
    form.send(0.2, Event::Text("alpha beta".into()));
    // 0.6 s apart: a double click at 0.8 s, two single ones at the default 0.4 s.
    let word = form.at(form.field, 14.0, 10.0);
    form.click(1.0, word);
    form.click(1.6, word);
    let selected = |form: &Form| form.tree.get::<TextField>(form.field).unwrap().selection();
    assert_eq!(selected(&form), 0..5, "selected while recording");
    let saved = form.tree.stop_recording().to_text();

    let mut fresh = Form::new();
    Recording::parse(&saved)
        .unwrap()
        .replay(&mut fresh.tree, &mut fresh.text);
    fresh.tree.prepare(VIEWPORT, &mut fresh.text);
    assert_eq!(selected(&fresh), 0..5, "selected on replay");
}
