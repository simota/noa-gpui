use noa_gpui_core::widgets::{
    Button, ButtonClicked, CollectionItem, CollectionView, ComboBox, DatePicker, DatePickerStyle,
    DateRangeChanged, FIND_SHORTCUT, FindBar, FormField, InlineEditableLabel, ItemRenamed,
    LabelEdited, LabelPosition, NumberInput, OutlineItem, OutlineView, RangeChanged,
    ReadingChanged, SecureTextField, ShortcutChanged, ShortcutRecorder, Slider, TextChanged,
    TextField, TextView, TokenField, TokensChanged, ValueChanged, drive_find,
};
use noa_gpui_core::{DateTime, Message, Role, Shortcut, ShortcutPressed, WidgetId, kana};
use noa_gpui_scene::Color;

use super::kit::{Kit, row_style};
use super::{Demo, DemoCx, SectionSpec};
use noa_gpui_core::widgets::{Container, Label};

pub const SECTIONS: [SectionSpec; 9] = [
    SectionSpec {
        title: "Number input",
        description: "A number typed into a field, read when committed (Return, Tab, \
                      leaving) and clamped to its range; text that is not a number goes \
                      back. The stepper inside and Up / Down step it (Shift: ten steps). \
                      Drag across the field before editing it to scrub the value. Emits \
                      ValueChanged.",
        build: numbers,
    },
    SectionSpec {
        title: "Shortcut recorder",
        description: "Click (or press Space) and type a shortcut: a key with ⌘ or ⌥ (and \
                      Shift) is recorded and shown as keycaps. Escape cancels, Backspace or \
                      the clear button removes it. Emits ShortcutChanged.",
        build: shortcuts,
    },
    SectionSpec {
        title: "Form field",
        description: "A control with its label, a red mark when required, help text and an \
                      error message that takes the help's place, spaced alike in every form. \
                      Labels go above the control or beside it in an aligned column.",
        build: forms,
    },
    SectionSpec {
        title: "Inline editing",
        description: "InlineEditableLabel: double-click or Return to rename in place; Return \
                      or leaving keeps the name, Escape cancels. Outline and collection views \
                      rename their items the same way with renamable(true). Emits LabelEdited \
                      and ItemRenamed.",
        build: inline,
    },
    SectionSpec {
        title: "Find bar",
        description: "Find and replace in a text view, like NSTextFinder: matches are \
                      highlighted as you type, Return / ⇧Return (or ⌘G / ⇧⌘G) walk them, \
                      with toggles for case, whole words and regular expressions, and a \
                      replace row. ⌘F shows the bar here, Escape hides it.",
        build: find,
    },
    SectionSpec {
        title: "Ranges",
        description: "A slider with two knobs picking a range (Tab moves to the other knob), \
                      and a calendar picking a range of days: drag across them, Shift-click \
                      or Shift+arrows to extend. Emit RangeChanged and DateRangeChanged.",
        build: ranges,
    },
    SectionSpec {
        title: "Field affixes",
        description: "Fixed text before or after a text field's input, such as a scheme or \
                      a unit. It is not part of the text, and clicking it puts the caret at \
                      that end.",
        build: affixes,
    },
    SectionSpec {
        title: "Completions",
        description: "Token fields and combo boxes list completions while typing, best \
                      matches first with the matched letters marked. Candidates can come \
                      from a fixed list or be fetched for each query; with multiple \
                      selection, ⌘Return or the check boxes pick several.",
        build: completions,
    },
    SectionSpec {
        title: "Furigana",
        description: "A name field captures the reading typed into the IME before each \
                      conversion and fills the furigana field in katakana as you type \
                      (type a name with a Japanese input method). The code field \
                      normalizes widths: full-width letters and digits become half-width, \
                      half-width katakana become full-width. Emits ReadingChanged.",
        build: furigana,
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

    fn describe(message: &Message) -> Option<String> {
        if let Some(ValueChanged(v)) = message.downcast_ref() {
            Some(format!("ValueChanged({v})"))
        } else if let Some(ShortcutChanged(s)) = message.downcast_ref() {
            Some(match s {
                Some(s) => format!("ShortcutChanged({})", s.label(true)),
                None => "ShortcutChanged(None)".into(),
            })
        } else if let Some(LabelEdited(t)) = message.downcast_ref() {
            Some(format!("LabelEdited({t:?})"))
        } else if let Some(ItemRenamed { item, name }) = message.downcast_ref() {
            Some(format!("ItemRenamed {{ item: {item}, name: {name:?} }}"))
        } else if let Some(RangeChanged { low, high }) = message.downcast_ref() {
            Some(format!("RangeChanged({low:.0} – {high:.0})"))
        } else if let Some(DateRangeChanged { start, end }) = message.downcast_ref() {
            Some(format!(
                "DateRangeChanged({}-{:02}-{:02} – {}-{:02}-{:02})",
                start.year, start.month, start.day, end.year, end.month, end.day
            ))
        } else if let Some(TokensChanged(t)) = message.downcast_ref() {
            Some(format!("TokensChanged({t:?})"))
        } else if let Some(TextChanged(t)) = message.downcast_ref() {
            Some(format!("TextChanged({t:?})"))
        } else {
            None
        }
    }
}

impl Demo for Echo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        let Some(&(_, label)) = self.items.iter().find(|(s, _)| *s == message.source) else {
            return false;
        };
        let Some(text) = Self::describe(message) else {
            return false;
        };
        cx.tree
            .update::<Label, _>(label, |l, cx| l.set_text(cx, text));
        true
    }
}

fn row(kit: &mut Kit, parent: WidgetId) -> WidgetId {
    kit.tree
        .insert(Some(parent), Container::new(row_style(8.0)))
}

fn numbers(kit: &mut Kit) -> Box<dyn Demo> {
    let mut echo = Echo::default();
    let card = kit.card("NumberInput::new(0, 100, 42)");
    let input = kit
        .tree
        .insert(Some(card), NumberInput::new(0.0, 100.0, 42.0));
    kit.tree.set_accessible_label(input, "Quantity");
    echo.watch(kit, card, input);

    let card = kit.card(".step(0.5).decimals(1).unit(\"px\")");
    let input = kit.tree.insert(
        Some(card),
        NumberInput::new(0.0, 64.0, 12.0)
            .step(0.5)
            .decimals(1)
            .unit("px")
            .width(120.0),
    );
    kit.tree.set_accessible_label(input, "Font size");
    echo.watch(kit, card, input);

    let card = kit.card("Scrubbing: drag left or right");
    let line = row(kit, card);
    kit.tree
        .insert(Some(line), Label::new("Opacity").color(Role::Muted));
    let input = kit.tree.insert(
        Some(line),
        NumberInput::new(0.0, 100.0, 80.0)
            .unit("%")
            .stepper(false)
            .width(80.0),
    );
    kit.tree.set_accessible_label(input, "Opacity");
    echo.watch(kit, card, input);
    Box::new(echo)
}

fn shortcuts(kit: &mut Kit) -> Box<dyn Demo> {
    let mut echo = Echo::default();
    let card = kit.card("ShortcutRecorder::new(Some(⇧⌘K))");
    let recorder = kit.tree.insert(
        Some(card),
        ShortcutRecorder::new(Some(Shortcut::command('k').with_shift())),
    );
    echo.watch(kit, card, recorder);
    let card = kit.card("ShortcutRecorder::new(None)");
    let recorder = kit.tree.insert(Some(card), ShortcutRecorder::new(None));
    echo.watch(kit, card, recorder);
    Box::new(echo)
}

struct FormDemo {
    submit: WidgetId,
    email_form: WidgetId,
    email: WidgetId,
    password_form: WidgetId,
    password: WidgetId,
    result: WidgetId,
}

impl Demo for FormDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.submit || message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        let email = cx
            .tree
            .get::<TextField>(self.email)
            .map(|f| f.text().to_owned())
            .unwrap_or_default();
        let password = cx
            .tree
            .get::<SecureTextField>(self.password)
            .map(|f| f.text().len())
            .unwrap_or_default();
        let email_error = if email.is_empty() {
            Some("Enter an email address.".to_owned())
        } else if !email.contains('@') {
            Some("An email address has an @.".to_owned())
        } else {
            None
        };
        let password_error = (password < 8).then(|| "Use at least 8 characters.".to_owned());
        let ok = email_error.is_none() && password_error.is_none();
        cx.tree
            .update::<FormField, _>(self.email_form, |f, cx| f.set_error(cx, email_error));
        cx.tree
            .update::<FormField, _>(self.password_form, |f, cx| f.set_error(cx, password_error));
        let text = if ok {
            "Signed up."
        } else {
            "Check the fields marked in red."
        };
        cx.tree
            .update::<Label, _>(self.result, |l, cx| l.set_text(cx, text));
        true
    }
}

fn forms(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("FormField::new(\"Email\").required(true).help(\"…\")");
    let email_form = kit.tree.insert(
        Some(card),
        FormField::new("Email")
            .required(true)
            .help("We never share it."),
    );
    let email = kit
        .tree
        .insert(Some(email_form), TextField::new("name@example.com", 240.0));
    let password_form = kit.tree.insert(
        Some(card),
        FormField::new("Password")
            .required(true)
            .help("At least 8 characters."),
    );
    let password = kit
        .tree
        .insert(Some(password_form), SecureTextField::new("", 240.0));
    let submit = kit.tree.insert(Some(card), Button::new("Sign Up"));
    let result = kit
        .tree
        .insert(Some(card), Label::new("—").color(Role::Muted));

    let card = kit.card(".label_position(LabelPosition::Leading(80))");
    for (label, control) in [("Name", "Ada Lovelace"), ("Company", "")] {
        let form = kit.tree.insert(
            Some(card),
            FormField::new(label).label_position(LabelPosition::Leading(80.0)),
        );
        let field = TextField::new(control, 150.0);
        kit.tree.insert(Some(form), field);
    }
    let form = kit.tree.insert(
        Some(card),
        FormField::new("Port")
            .label_position(LabelPosition::Leading(80.0))
            .help("1024 to 65535"),
    );
    kit.tree.insert(
        Some(form),
        NumberInput::new(1024.0, 65535.0, 8080.0).width(110.0),
    );
    Box::new(FormDemo {
        submit,
        email_form,
        email,
        password_form,
        password,
        result,
    })
}

fn inline(kit: &mut Kit) -> Box<dyn Demo> {
    let mut echo = Echo::default();
    let card = kit.card("InlineEditableLabel::new(\"…\")");
    let title = kit.tree.insert(
        Some(card),
        InlineEditableLabel::new("Quarterly Report")
            .font_size(17.0)
            .weight(600)
            .max_width(260.0),
    );
    echo.watch(kit, card, title);

    let card = kit.card("OutlineView::new(…).renamable(true)");
    let outline = kit.tree.insert(
        Some(card),
        OutlineView::new(vec![
            OutlineItem::new("Projects")
                .expanded(true)
                .children([OutlineItem::new("Website"), OutlineItem::new("Mobile App")]),
            OutlineItem::new("Archive"),
        ])
        .renamable(true)
        .width(260.0)
        .height(110.0),
    );
    kit.tree.set_accessible_label(outline, "Projects");
    echo.watch(kit, card, outline);

    let card = kit.card("CollectionView::new(…).renamable(true)");
    let colors = [0x4f8ef7, 0xf7b84f, 0x5cc98a];
    let items = ["Ocean", "Sunset", "Forest"]
        .into_iter()
        .zip(colors)
        .map(|(title, color)| CollectionItem::new(title).color(Color::hex(color)))
        .collect();
    let collection = kit.tree.insert(
        Some(card),
        CollectionView::new(items)
            .renamable(true)
            .item_size(70.0, 92.0)
            .width(260.0)
            .height(120.0),
    );
    kit.tree.set_accessible_label(collection, "Themes");
    echo.watch(kit, card, collection);
    Box::new(echo)
}

const FIND_TEXT: &str = "The find bar searches this text view. Type in its field to \
                         highlight every match: find, Find, FIND and finder all match \
                         \"find\" until you turn on case or whole words. With .* on, \
                         try \\bf\\w+ or (\\w+)ing and replace with $1.";

struct FindDemo {
    bar: WidgetId,
    view: WidgetId,
    show: WidgetId,
}

impl Demo for FindDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        let shortcut =
            message.source == self.bar && message.downcast_ref::<ShortcutPressed>().is_some();
        let button =
            message.source == self.show && message.downcast_ref::<ButtonClicked>().is_some();
        if shortcut || button {
            cx.tree.set_hidden(self.bar, false);
            cx.tree.set_focus(Some(self.bar));
            return true;
        }
        drive_find(cx.tree, self.bar, self.view, message)
    }
}

fn find(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "FindBar::new() + TextView, connected with drive_find",
        560.0,
    );
    let bar = kit.tree.insert(Some(card), FindBar::new());
    let view = kit
        .tree
        .insert(Some(card), TextView::new(120.0).width(524.0));
    kit.tree.set_accessible_label(view, "Document");
    kit.tree
        .update::<TextView, _>(view, |v, cx| v.set_text(cx, FIND_TEXT));
    let show = kit
        .tree
        .insert(Some(card), Button::new("Show Find Bar (⌘F)"));
    kit.tree.add_shortcut(FIND_SHORTCUT, bar);
    Box::new(FindDemo { bar, view, show })
}

const TODAY: DateTime = DateTime {
    year: 2026,
    month: 11,
    day: 12,
    hour: 9,
    minute: 0,
};

fn ranges(kit: &mut Kit) -> Box<dyn Demo> {
    let mut echo = Echo::default();
    let card = kit.card("Slider::range(0, 100, 20, 70)");
    let slider = kit.tree.insert(
        Some(card),
        Slider::range(0.0, 100.0, 20.0, 70.0).length(240.0),
    );
    kit.tree.set_accessible_label(slider, "Price range");
    echo.watch(kit, card, slider);
    let card = kit.card("Slider::range(…).tick_marks(11).snap_to_ticks(true)");
    let slider = kit.tree.insert(
        Some(card),
        Slider::range(0.0, 10.0, 3.0, 6.0)
            .tick_marks(11)
            .snap_to_ticks(true)
            .length(240.0),
    );
    kit.tree.set_accessible_label(slider, "Size range");
    echo.watch(kit, card, slider);

    let card = kit.card("DatePicker::new(…).style(Graphical).range_selection(true)");
    let picker = kit.tree.insert(
        Some(card),
        DatePicker::new(TODAY)
            .style(DatePickerStyle::Graphical)
            .range_selection(true),
    );
    kit.tree.set_accessible_label(picker, "Date range");
    echo.watch(kit, card, picker);
    Box::new(echo)
}

fn affixes(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("TextField::new(…).prefix(\"https://\")");
    let website = kit.tree.insert(
        Some(card),
        TextField::new("example.com", 240.0).prefix("https://"),
    );
    kit.tree.set_accessible_label(website, "Website");
    let card = kit.card(".suffix(\"kg\")  ·  .prefix(\"$\").suffix(\"USD\")");
    let weight = kit
        .tree
        .insert(Some(card), TextField::new("0", 120.0).suffix("kg"));
    kit.tree.set_accessible_label(weight, "Weight");
    let price = kit.tree.insert(
        Some(card),
        TextField::new("0.00", 160.0).prefix("$").suffix("USD"),
    );
    kit.tree.set_accessible_label(price, "Price");
    let card = kit.card(".suffix(\"@example.com\")");
    let email = kit.tree.insert(
        Some(card),
        TextField::new("username", 240.0).suffix("@example.com"),
    );
    kit.tree.set_accessible_label(email, "Email");
    Box::new(())
}

const FRUITS: [&str; 12] = [
    "Apple",
    "Apricot",
    "Avocado",
    "Banana",
    "Blackberry",
    "Blueberry",
    "Cherry",
    "Grape",
    "Grapefruit",
    "Lemon",
    "Mango",
    "Pineapple",
];

const PEOPLE: [&str; 8] = [
    "Ada Lovelace",
    "Alan Turing",
    "Barbara Liskov",
    "Donald Knuth",
    "Edsger Dijkstra",
    "Grace Hopper",
    "Katherine Johnson",
    "Margaret Hamilton",
];

fn completions(kit: &mut Kit) -> Box<dyn Demo> {
    let mut echo = Echo::default();
    let card = kit.card("TokenField::new(…).completions(FRUITS)");
    let tokens = kit.tree.insert(
        Some(card),
        TokenField::new("Fruit", 260.0).completions(FRUITS),
    );
    kit.tree.set_accessible_label(tokens, "Fruits");
    echo.watch(kit, card, tokens);

    let card = kit.card(".completion_source(|q| search(q)).multiple_completions(true)");
    let tokens = kit.tree.insert(
        Some(card),
        TokenField::new("To:", 260.0)
            .completion_source(|query| {
                let query = query.to_lowercase();
                PEOPLE
                    .iter()
                    .filter(|name| {
                        name.to_lowercase()
                            .split(' ')
                            .any(|word| word.starts_with(&query))
                    })
                    .map(|name| {
                        let first = name.split(' ').next().unwrap_or(name).to_lowercase();
                        format!("{name} <{first}@example.com>")
                    })
                    .collect()
            })
            .multiple_completions(true),
    );
    kit.tree.set_accessible_label(tokens, "To");
    echo.watch(kit, card, tokens);

    let card = kit.card("ComboBox::new(FRUITS, …).completion_list(true).multiple(true)");
    let combo = kit.tree.insert(
        Some(card),
        ComboBox::new(FRUITS, 260.0)
            .completion_list(true)
            .multiple(true)
            .placeholder("Favorite fruits"),
    );
    kit.tree.set_accessible_label(combo, "Favorite fruits");
    echo.watch(kit, card, combo);
    Box::new(echo)
}

struct FuriganaDemo {
    name: WidgetId,
    furigana: WidgetId,
    code: WidgetId,
}

impl Demo for FuriganaDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.name
            && let Some(ReadingChanged(reading)) = message.downcast_ref()
        {
            let furigana = kana::to_katakana(reading);
            cx.tree
                .update::<TextField, _>(self.furigana, |f, cx| f.set_text(cx, furigana));
            return true;
        }
        if message.source == self.code
            && let Some(TextChanged(text)) = message.downcast_ref()
        {
            let normalized = kana::normalize_width(text);
            if &normalized != text {
                cx.tree
                    .update::<TextField, _>(self.code, |f, cx| f.set_text(cx, normalized));
            }
            return true;
        }
        false
    }
}

fn furigana(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("TextField::new(\"…\", 200.0).capture_reading(true)");
    let form = kit.tree.insert(
        Some(card),
        FormField::new("氏名").label_position(LabelPosition::Leading(80.0)),
    );
    let name = kit.tree.insert(
        Some(form),
        TextField::new("山田 太郎", 200.0).capture_reading(true),
    );
    let form = kit.tree.insert(
        Some(card),
        FormField::new("フリガナ").label_position(LabelPosition::Leading(80.0)),
    );
    let furigana = kit
        .tree
        .insert(Some(form), TextField::new("ヤマダ タロウ", 200.0));

    let card = kit.card("kana::normalize_width(text)");
    let form = kit.tree.insert(
        Some(card),
        FormField::new("Code")
            .label_position(LabelPosition::Leading(80.0))
            .help("Try ＡＢＣ１２３ or ｶﾞｯｺｳ"),
    );
    let code = kit.tree.insert(Some(form), TextField::new("", 200.0));
    Box::new(FuriganaDemo {
        name,
        furigana,
        code,
    })
}
