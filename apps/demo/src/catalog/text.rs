use ui_core::widgets::{
    Button, ButtonClicked, ComboBox, FindBar, Label, LogLine, LogView, SearchField,
    SearchSubmitted, SecureTextField, SelectionChanged, Terminal, TerminalInput, TextChanged,
    TextField, TextSubmitted, TextView, TokenField, TokensChanged, Truncation, drive_find,
};
use ui_core::{Message, Paint, Role, WidgetId};
use ui_scene::Color;
use ui_text::TextAlign;

use super::kit::Kit;
use super::{Demo, DemoCx, SectionSpec};

pub const SECTIONS: [SectionSpec; 9] = [
    SectionSpec {
        title: "Label",
        description: "Static text. Single-line by default; wrap(true) wraps at the available \
                      width and max_lines limits it. A single line that does not fit can be \
                      truncated at the end, middle or start; selectable labels can be \
                      selected and copied.",
        build: label,
    },
    SectionSpec {
        title: "TextField",
        description: "Single-line input with IME composition, grapheme-aware editing, \
                      selection, undo and the clipboard; long text scrolls sideways. Can be \
                      read-only or disabled. Emits TextChanged, and TextSubmitted on Return.",
        build: text_field,
    },
    SectionSpec {
        title: "Secure field",
        description: "Password input: a bullet per character, no copy or cut, and the IME \
                      kept off as with macOS secure input.",
        build: secure,
    },
    SectionSpec {
        title: "Search field",
        description: "A magnifying glass with recent searches, and a clear button. Typing \
                      emits TextChanged; Return emits SearchSubmitted and remembers it; \
                      Escape clears.",
        build: search,
    },
    SectionSpec {
        title: "Token field",
        description: "Return or a comma turns the text into a token. Backspace or Left at \
                      the start selects the last token; Backspace / Delete remove it. Emits \
                      TokensChanged.",
        build: tokens,
    },
    SectionSpec {
        title: "Combo box",
        description: "Free text with a list of suggestions: typing completes a matching item, \
                      the arrow or Down opens the list. Emits TextChanged and \
                      SelectionChanged.",
        build: combo,
    },
    SectionSpec {
        title: "Text view",
        description: "Multi-line plain-text editing with a fixed height: wraps, follows the \
                      caret and scrolls with the wheel.",
        build: text_view,
    },
    SectionSpec {
        title: "Log view",
        description: "Many monospaced lines that keep coming, laid out only while on screen. \
                      Stays at the bottom as lines arrive unless scrolled up; lines can have \
                      their own color and background. A FindBar searches it through \
                      drive_find; click, Shift+click or drag selects lines, ⌘C copies them.",
        build: log_view,
    },
    SectionSpec {
        title: "Terminal",
        description: "A VT100 / xterm screen for a program in a pseudo-terminal (on desktop, \
                      ui_platform::Pty): colors, cursor movement, the alternate screen of \
                      full-screen programs and a scrollback the wheel scrolls. Keys become the \
                      bytes a program reads (TerminalInput). Here a tiny line editor answers \
                      instead of a shell.",
        build: terminal,
    },
];

#[derive(Default)]
struct Echo {
    items: Vec<(WidgetId, WidgetId)>,
}

impl Echo {
    fn watch(&mut self, kit: &mut Kit, parent: WidgetId, source: WidgetId) {
        let label = kit
            .tree
            .insert(Some(parent), Label::new("—").color(Role::Muted).wrap(true));
        self.items.push((source, label));
    }
}

impl Demo for Echo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        let Some(&(_, label)) = self.items.iter().find(|(s, _)| *s == message.source) else {
            return false;
        };
        let text = if let Some(TextChanged(t)) = message.downcast_ref() {
            format!("TextChanged({t:?})")
        } else if let Some(TextSubmitted(t)) = message.downcast_ref() {
            format!("TextSubmitted({t:?})")
        } else if let Some(SearchSubmitted(t)) = message.downcast_ref() {
            format!("SearchSubmitted({t:?})")
        } else if let Some(TokensChanged(t)) = message.downcast_ref() {
            format!("TokensChanged({t:?})")
        } else if let Some(SelectionChanged(i)) = message.downcast_ref() {
            format!("SelectionChanged({i})")
        } else {
            return false;
        };
        cx.tree
            .update::<Label, _>(label, |l, cx| l.set_text(cx, text));
        true
    }
}

fn label(kit: &mut Kit) -> Box<dyn Demo> {
    let body = kit.card("Label::new(\"…\")");
    kit.tree
        .insert(Some(body), Label::new("The quick brown fox jumps"));
    let sizes = kit.card(".font_size(12 | 14 | 17 | 24)");
    for px in [12.0, 14.0, 17.0, 24.0] {
        kit.tree.insert(
            Some(sizes),
            Label::new(format!("{px} px text")).font_size(px),
        );
    }
    let weights = kit.card(".weight(400 | 600 | 700)");
    for (weight, name) in [
        (400, "Regular 400"),
        (600, "Semibold 600"),
        (700, "Bold 700"),
    ] {
        kit.tree
            .insert(Some(weights), Label::new(name).weight(weight));
    }
    let colors = kit.card(".color(Role::… | Color)");
    for (color, name) in [
        (Paint::from(Role::Text), "Role::Text"),
        (Role::Muted.into(), "Role::Muted"),
        (Role::Accent.into(), "Role::Accent"),
        (Color::hex(0xd6455d).into(), "Color #d6455d"),
    ] {
        kit.tree.insert(Some(colors), Label::new(name).color(color));
    }
    let wrapped = kit.card(".wrap(true)");
    kit.tree.insert(
        Some(wrapped),
        Label::new(
            "Wrapped text breaks at the card's width. Long paragraphs reflow when the window \
             is resized, and only the affected labels are measured again.",
        )
        .wrap(true),
    );
    let unicode = kit.card("Unicode");
    // The web build bundles only a Latin font. Native builds fall back to system
    // fonts for CJK.
    #[cfg(not(target_arch = "wasm32"))]
    kit.tree
        .insert(Some(unicode), Label::new("日本語のテキスト"));
    kit.tree
        .insert(Some(unicode), Label::new("Ünïcödé · Ελληνικά · Кириллица"));
    let truncated = kit.card(".truncation(Truncation::Tail | Middle | Head)");
    for mode in [Truncation::Tail, Truncation::Middle, Truncation::Head] {
        kit.tree.insert(
            Some(truncated),
            Label::new(format!(
                "{mode:?}: /Users/noa/Documents/Projects/noa-gpui/README.md"
            ))
            .truncation(mode),
        );
    }
    let aligned = kit.card(".align(TextAlign::Center | End)");
    for align in [TextAlign::Center, TextAlign::End] {
        kit.tree.insert(
            Some(aligned),
            Label::new(format!(
                "{align:?}-aligned:\nthe lines line up\nwith each other"
            ))
            .align(align),
        );
    }
    let lines = kit.card(".max_lines(2)");
    kit.tree.insert(
        Some(lines),
        Label::new(
            "Only two lines are shown. The rest of this long description is cut, and the \
             second line ends with an ellipsis.",
        )
        .max_lines(2),
    );
    let selectable = kit.card(".selectable(true) — drag, double-click, Cmd/Ctrl+C");
    kit.tree.insert(
        Some(selectable),
        Label::new("Order #2026-1112-0042 · Tracking 1Z999AA10123456784")
            .selectable(true)
            .wrap(true),
    );
    Box::new(())
}

fn text_field(kit: &mut Kit) -> Box<dyn Demo> {
    let mut echo = Echo::default();
    let empty = kit.card("TextField::new(placeholder, 240) — Return emits TextSubmitted");
    let field = kit
        .tree
        .insert(Some(empty), TextField::new("Type here (IME works)", 240.0));
    kit.tree.set_accessible_label(field, "Sample text");
    echo.watch(kit, empty, field);
    let filled = kit.card(".set_text(\"…\") · narrow (120 px) scrolls sideways");
    let prefilled = kit.tree.insert(Some(filled), TextField::new("Name", 240.0));
    kit.tree.set_accessible_label(prefilled, "Name");
    kit.tree
        .update::<TextField, _>(prefilled, |f, cx| f.set_text(cx, "Ada Lovelace"));
    let narrow = kit.tree.insert(Some(filled), TextField::new("Code", 120.0));
    kit.tree.set_accessible_label(narrow, "Code");
    kit.tree.update::<TextField, _>(narrow, |f, cx| {
        f.set_text(cx, "A value much longer than the field")
    });
    let states = kit.card(".read_only(true) · set_enabled(false)");
    let read_only = kit
        .tree
        .insert(Some(states), TextField::new("", 240.0).read_only(true));
    kit.tree.set_accessible_label(read_only, "Read-only text");
    kit.tree.update::<TextField, _>(read_only, |f, cx| {
        f.set_text(cx, "Read-only: select and copy")
    });
    let disabled = kit.tree.insert(Some(states), TextField::new("", 240.0));
    kit.tree.set_accessible_label(disabled, "Disabled text");
    kit.tree
        .update::<TextField, _>(disabled, |f, cx| f.set_text(cx, "Disabled"));
    kit.tree.set_enabled(disabled, false);
    Box::new(echo)
}

fn secure(kit: &mut Kit) -> Box<dyn Demo> {
    let mut echo = Echo::default();
    let card = kit.card("SecureTextField::new(placeholder, 240)");
    let field = kit
        .tree
        .insert(Some(card), SecureTextField::new("Password", 240.0));
    kit.tree.set_accessible_label(field, "Password");
    echo.watch(kit, card, field);
    Box::new(echo)
}

fn search(kit: &mut Kit) -> Box<dyn Demo> {
    let mut echo = Echo::default();
    let card = kit.card("SearchField::new(placeholder, 240) — press the magnifier for recents");
    let field = kit
        .tree
        .insert(Some(card), SearchField::new("Search", 240.0));
    kit.tree.set_accessible_label(field, "Search");
    echo.watch(kit, card, field);
    let square = kit.card(".capsule(false) — the corners of a TextField");
    let field = kit.tree.insert(
        Some(square),
        SearchField::new("Search", 240.0).capsule(false),
    );
    kit.tree
        .set_accessible_label(field, "Search, square corners");
    Box::new(echo)
}

fn tokens(kit: &mut Kit) -> Box<dyn Demo> {
    let mut echo = Echo::default();
    let card = kit.card("TokenField::new(placeholder, 264).with_tokens([...])");
    let field = kit.tree.insert(
        Some(card),
        TokenField::new("Add tags", 264.0).with_tokens(["rust", "gpu", "ui"]),
    );
    kit.tree.set_accessible_label(field, "Tags");
    echo.watch(kit, card, field);
    let empty = kit.card("Empty: the placeholder shows until the first token");
    let field = kit
        .tree
        .insert(Some(empty), TokenField::new("Recipients", 264.0));
    kit.tree.set_accessible_label(field, "Recipients");
    echo.watch(kit, empty, field);
    Box::new(echo)
}

fn combo(kit: &mut Kit) -> Box<dyn Demo> {
    let mut echo = Echo::default();
    let card = kit.card("ComboBox::new(items, 240) — try typing \"ap\"");
    let field = kit.tree.insert(
        Some(card),
        ComboBox::new(
            ["Apple", "Apricot", "Banana", "Blueberry", "Cherry", "Grape"],
            240.0,
        )
        .placeholder("Fruit"),
    );
    kit.tree.set_accessible_label(field, "Fruit");
    echo.watch(kit, card, field);
    Box::new(echo)
}

fn text_view(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card("TextView::new(160).width(380)", 420.0);
    let view = kit
        .tree
        .insert(Some(card), TextView::new(160.0).width(380.0));
    kit.tree.set_accessible_label(view, "Notes");
    kit.tree.update::<TextView, _>(view, |v, cx| {
        v.set_text(
            cx,
            "A text view edits plain text over many lines.\n\nIt wraps at its width and keeps \
             a fixed height, so longer text scrolls: type past the bottom and the view follows \
             the caret, or use the wheel.\n\nSelection, undo, the clipboard and IME composition \
             work as in the text field.\n\nLine 8\nLine 9\nLine 10\nLine 11\nLine 12",
        )
    });
    let read_only = kit.card(".read_only(true)");
    let view = kit.tree.insert(
        Some(read_only),
        TextView::new(80.0).width(240.0).read_only(true),
    );
    kit.tree.set_accessible_label(view, "Read-only notes");
    kit.tree.update::<TextView, _>(view, |v, cx| {
        v.set_text(
            cx,
            "Read-only text can be selected and copied but not changed.",
        )
    });
    Box::new(())
}

struct LogDemo {
    bar: WidgetId,
    log: WidgetId,
    add: WidgetId,
    copy: WidgetId,
    added: usize,
}

fn log_line(i: usize) -> LogLine {
    match i % 7 {
        3 => LogLine::new(format!("{i:05} WARN  slow response from db (812 ms)"))
            .color(Color::hex(0xc27c0e)),
        6 => LogLine::new(format!("{i:05} ERROR connection refused: redis:6379"))
            .color(Color::hex(0xd70015))
            .background(Color::hex(0xd70015).with_alpha(0.08)),
        _ => LogLine::new(format!("{i:05} INFO  GET /api/items 200 4ms")),
    }
}

impl Demo for LogDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.downcast_ref::<ButtonClicked>().is_some() {
            if message.source == self.add {
                let from = self.added;
                self.added += 100;
                cx.tree.update::<LogView, _>(self.log, |l, cx| {
                    l.extend(cx, (from..from + 100).map(log_line))
                });
                return true;
            }
            if message.source == self.copy {
                let text = cx.tree.get::<LogView>(self.log).map(LogView::all_text);
                if let Some(text) = text {
                    cx.tree.clipboard().write_text(&text);
                }
                return true;
            }
        }
        drive_find(cx.tree, self.bar, self.log, message)
    }
}

fn log_view(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "LogView::new().max_lines(10_000) + FindBar, connected with drive_find",
        560.0,
    );
    let bar = kit.tree.insert(Some(card), FindBar::new());
    let added = 2_000;
    let log = kit.tree.insert(
        Some(card),
        LogView::new()
            .max_lines(10_000)
            .height(200.0)
            .width(524.0)
            .lines((0..added).map(log_line)),
    );
    kit.tree.set_accessible_label(log, "Server log");
    let add = kit.tree.insert(Some(card), Button::new("Add 100 lines"));
    let copy = kit.tree.insert(Some(card), Button::new("Copy all"));
    Box::new(LogDemo {
        bar,
        log,
        add,
        copy,
        added,
    })
}

const PROMPT: &[u8] = b"\x1b[1;32mdemo\x1b[0m:\x1b[1;34m~\x1b[0m$ ";

struct TerminalDemo {
    term: WidgetId,
    line: String,
}

impl TerminalDemo {
    fn reply(&mut self) -> Vec<u8> {
        let line = std::mem::take(&mut self.line);
        let mut out = b"\r\n".to_vec();
        match line.trim() {
            "" => {}
            "colors" => {
                for i in 0..16 {
                    out.extend(format!("\x1b[48;5;{i}m  ").bytes());
                }
                out.extend(b"\x1b[0m\r\n");
            }
            "clear" => out.extend(b"\x1b[H\x1b[2J"),
            text => out.extend(format!("you typed \x1b[1m{text}\x1b[0m\r\n").bytes()),
        }
        out.extend_from_slice(PROMPT);
        out
    }
}

impl Demo for TerminalDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.term {
            return false;
        }
        let Some(TerminalInput(bytes)) = message.downcast_ref() else {
            return true;
        };
        let mut echo = Vec::new();
        for &b in bytes {
            match b {
                b'\r' => echo.extend(self.reply()),
                0x7f if self.line.pop().is_some() => echo.extend(b"\x08 \x08"),
                3 => {
                    self.line.clear();
                    echo.extend(b"^C\r\n");
                    echo.extend_from_slice(PROMPT);
                }
                0x20..=0x7e => {
                    self.line.push(b as char);
                    echo.push(b);
                }
                _ => {}
            }
        }
        cx.tree
            .update::<Terminal, _>(self.term, |t, cx| t.feed(cx, &echo));
        true
    }
}

fn terminal(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "Terminal::new().rows(12) · feed(bytes) · TerminalInput",
        640.0,
    );
    let term = kit.tree.insert(Some(card), Terminal::new().rows(12));
    kit.tree.set_accessible_label(term, "Shell");
    let banner = "\x1b[1mnoa-gpui terminal\x1b[0m  \x1b[2m(type a line, `colors` or `clear`)\x1b[0m\r\n\
                  \x1b[31mred\x1b[0m \x1b[32mgreen\x1b[0m \x1b[33myellow\x1b[0m \
                  \x1b[34mblue\x1b[0m \x1b[38;2;255;128;0mtrue color\x1b[0m \x1b[7m inverse \x1b[0m \
                  \x1b[4munderline\x1b[0m 日本語\r\n";
    kit.tree.update::<Terminal, _>(term, |t, cx| {
        t.feed(cx, banner.as_bytes());
        t.feed(cx, PROMPT);
    });
    Box::new(TerminalDemo {
        term,
        line: String::new(),
    })
}
