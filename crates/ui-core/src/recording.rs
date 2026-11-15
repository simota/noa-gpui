//! Recording input and playing it back, for tests made from a real session.
//!
//! [`UiTree::start_recording`] keeps every input event the tree is given (pointer,
//! wheel, keys, text, IME composition and reconversion, and modifiers) with the
//! tree's clock. A
//! [`Recording`] is saved as text, one step per line, so it can be read, edited and
//! checked in next to a test:
//!
//! ```text
//! 0.000 move 120 34
//! 0.016 down primary 120 34
//! 0.081 up primary 120 34
//! 0.502 key Char('s') cmd
//! 0.900 text "山田"
//! 1.000 compose "やまだ" 9 9
//! 1.200 ime-select 0 6
//! ```
//!
//! [`Recording::replay`] gives the steps to a tree again, setting its clock to each
//! step's time, so double clicks and other timing come out the same. Events the tree
//! makes for itself (focus changes, timers, animation frames) and actions from
//! assistive technology are not recorded; they follow from the input.

use std::fmt::Write as _;

use noa_gpui_scene::Point;
use noa_gpui_text::TextStore;

use crate::event::{Event, Key, Modifiers, PointerButton};
use crate::tree::UiTree;

/// One input event and the tree's clock (seconds) when it was dispatched.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub at: f64,
    pub event: Event,
}

impl Step {
    /// `None` for events that are not input (see the module documentation).
    pub fn new(at: f64, event: &Event) -> Option<Self> {
        let recorded = matches!(
            event,
            Event::PointerMove { .. }
                | Event::PointerDown { .. }
                | Event::PointerUp { .. }
                | Event::PointerLeave
                | Event::Wheel { .. }
                | Event::KeyDown { .. }
                | Event::Text(_)
                | Event::Composition { .. }
                | Event::ImeSelect(_)
                | Event::ModifiersChanged(_)
        );
        recorded.then(|| Self {
            at,
            event: event.clone(),
        })
    }
}

/// Input steps in the order they were dispatched.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recording {
    pub steps: Vec<Step>,
}

/// A line of a saved recording that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// From 1.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

impl Recording {
    /// The saved form: one step per line (see the module documentation). Steps that
    /// are not input (built by hand) are left out.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for line in self.steps.iter().filter_map(step_line) {
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    /// Read the saved form. Empty lines and lines starting with `#` are skipped.
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut steps = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let step = parse_step(line).map_err(|message| ParseError {
                line: index + 1,
                message,
            })?;
            steps.push(step);
        }
        Ok(Self { steps })
    }

    /// Dispatch every step to `tree` at its time. Messages the widgets emit stay in
    /// the tree for the caller to take (see [`UiTree::take_messages`]).
    pub fn replay(&self, tree: &mut UiTree, text: &mut TextStore) {
        for step in &self.steps {
            tree.set_now(step.at);
            tree.dispatch(step.event.clone(), text);
        }
    }
}

/// One step as a line, without the line break; `None` when its event is not input.
pub fn step_line(step: &Step) -> Option<String> {
    let mut line = format!("{:.3} ", step.at);
    let point = |line: &mut String, p: Point| {
        let _ = write!(line, "{} {}", number(p.x), number(p.y));
    };
    match &step.event {
        Event::PointerMove { pos } => {
            line.push_str("move ");
            point(&mut line, *pos);
        }
        Event::PointerDown { pos, button } | Event::PointerUp { pos, button } => {
            let kind = if matches!(step.event, Event::PointerDown { .. }) {
                "down"
            } else {
                "up"
            };
            let _ = write!(line, "{kind} {} ", button_name(*button));
            point(&mut line, *pos);
        }
        Event::PointerLeave => line.push_str("leave"),
        Event::Wheel { pos, delta } => {
            line.push_str("wheel ");
            point(&mut line, *pos);
            line.push(' ');
            point(&mut line, *delta);
        }
        Event::KeyDown { key, modifiers } => {
            let _ = write!(line, "key {key:?} {}", modifier_names(*modifiers));
        }
        Event::Text(input) => {
            let _ = write!(line, "text {}", quote(input));
        }
        Event::Composition { text, cursor } => {
            let _ = write!(line, "compose {}", quote(text));
            match cursor {
                Some((start, end)) => {
                    let _ = write!(line, " {start} {end}");
                }
                None => line.push_str(" -"),
            }
        }
        Event::ImeSelect(range) => {
            let _ = write!(line, "ime-select {} {}", range.start, range.end);
        }
        Event::ModifiersChanged(modifiers) => {
            let _ = write!(line, "modifiers {}", modifier_names(*modifiers));
        }
        _ => return None,
    }
    Some(line)
}

/// Whole numbers without a fraction, others as they are.
fn number(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn button_name(button: PointerButton) -> &'static str {
    match button {
        PointerButton::Primary => "primary",
        PointerButton::Secondary => "secondary",
        PointerButton::Middle => "middle",
    }
}

const MODIFIERS: [&str; 5] = ["shift", "ctrl", "alt", "meta", "cmd"];

fn modifier_names(m: Modifiers) -> String {
    let names: Vec<&str> = [m.shift, m.ctrl, m.alt, m.meta, m.command]
        .into_iter()
        .zip(MODIFIERS)
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
    if names.is_empty() {
        "-".into()
    } else {
        names.join("+")
    }
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn parse_step(line: &str) -> Result<Step, String> {
    let (at, rest) = line.split_once(' ').ok_or("no event after the time")?;
    let at: f64 = at.parse().map_err(|_| format!("not a time: {at}"))?;
    let (kind, args) = rest.split_once(' ').unwrap_or((rest, ""));
    let words: Vec<&str> = args.split_whitespace().collect();
    let float = |i: usize| -> Result<f32, String> {
        let word = words.get(i).ok_or("missing number")?;
        word.parse().map_err(|_| format!("not a number: {word}"))
    };
    let point = |i: usize| -> Result<Point, String> { Ok(Point::new(float(i)?, float(i + 1)?)) };
    let event = match kind {
        "move" => Event::PointerMove { pos: point(0)? },
        "down" | "up" => {
            let button = match words.first().copied() {
                Some("primary") => PointerButton::Primary,
                Some("secondary") => PointerButton::Secondary,
                Some("middle") => PointerButton::Middle,
                other => return Err(format!("not a button: {other:?}")),
            };
            let pos = point(1)?;
            if kind == "down" {
                Event::PointerDown { pos, button }
            } else {
                Event::PointerUp { pos, button }
            }
        }
        "leave" => Event::PointerLeave,
        "wheel" => Event::Wheel {
            pos: point(0)?,
            delta: point(2)?,
        },
        "key" => {
            // The key comes first and may hold a quoted space (`Char(' ')`).
            let (key, modifiers) = args.rsplit_once(' ').ok_or("missing modifiers")?;
            Event::KeyDown {
                key: parse_key(key)?,
                modifiers: parse_modifiers(modifiers)?,
            }
        }
        "text" => Event::Text(unquote(args)?.0),
        "compose" => {
            let (text, rest) = unquote(args)?;
            let rest: Vec<&str> = rest.split_whitespace().collect();
            let cursor = match rest.as_slice() {
                ["-"] => None,
                [start, end] => Some((
                    start
                        .parse()
                        .map_err(|_| format!("not an index: {start}"))?,
                    end.parse().map_err(|_| format!("not an index: {end}"))?,
                )),
                _ => return Err("expected a cursor range or -".into()),
            };
            Event::Composition { text, cursor }
        }
        "ime-select" => {
            let index = |i: usize| -> Result<usize, String> {
                let word = words.get(i).copied().unwrap_or_default();
                word.parse().map_err(|_| format!("not an index: {word}"))
            };
            Event::ImeSelect(index(0)?..index(1)?)
        }
        "modifiers" => Event::ModifiersChanged(parse_modifiers(args)?),
        other => return Err(format!("unknown event: {other}")),
    };
    Ok(Step { at, event })
}

fn parse_key(word: &str) -> Result<Key, String> {
    if let Some(inner) = word
        .strip_prefix("Char('")
        .and_then(|w| w.strip_suffix("')"))
    {
        let unescaped = match inner {
            "\\'" => "'".to_owned(),
            "\\\\" => "\\".to_owned(),
            _ => inner.to_owned(),
        };
        let mut chars = unescaped.chars();
        return match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(Key::Char(c)),
            _ => Err(format!("not a key: {word}")),
        };
    }
    Ok(match word {
        "Tab" => Key::Tab,
        "Enter" => Key::Enter,
        "Space" => Key::Space,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "F10" => Key::F10,
        "ContextMenu" => Key::ContextMenu,
        "Other" => Key::Other,
        _ => return Err(format!("not a key: {word}")),
    })
}

fn parse_modifiers(word: &str) -> Result<Modifiers, String> {
    let mut m = Modifiers::default();
    if word.trim() == "-" {
        return Ok(m);
    }
    for name in word.trim().split('+') {
        let flag = match name {
            "shift" => &mut m.shift,
            "ctrl" => &mut m.ctrl,
            "alt" => &mut m.alt,
            "meta" => &mut m.meta,
            "cmd" => &mut m.command,
            _ => return Err(format!("not a modifier: {name}")),
        };
        *flag = true;
    }
    Ok(m)
}

/// The quoted string at the start of `args` and what follows it.
fn unquote(args: &str) -> Result<(String, &str), String> {
    let mut chars = args.char_indices();
    if chars.next().map(|(_, c)| c) != Some('"') {
        return Err("expected a quoted string".into());
    }
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &args[i + 1..])),
            '\\' => match chars.next().map(|(_, c)| c) {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some(c @ ('"' | '\\')) => out.push(c),
                other => return Err(format!("unknown escape: \\{other:?}")),
            },
            _ => out.push(c),
        }
    }
    Err("unterminated string".into())
}
