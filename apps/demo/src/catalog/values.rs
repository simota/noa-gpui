use ui_core::style::{AlignItems, FlexDirection};
use ui_core::widgets::{
    Button, ButtonClicked, ColorChanged, ColorPicker, ColorWell, Container, DateChanged,
    DateElements, DateFormat, DatePicker, DatePickerStyle, Label, PathControl, PathItemClicked,
    PathStyle, Slider, Stepper, TextChanged, TextField, ValueChanged, hex_string,
};
use ui_core::{DateTime, Message, Role, Style, WidgetId};
use ui_scene::Color;

use super::kit::{Kit, gap, row_style};
use super::{Demo, DemoCx, SectionSpec};

pub const SECTIONS: [SectionSpec; 5] = [
    SectionSpec {
        title: "Slider",
        description: "Horizontal, vertical and circular sliders, with tick marks the value can \
                      be restricted to. Press to jump, then drag; arrows step, Page Up / Down \
                      move a tenth, Home / End go to the ends. Emits ValueChanged.",
        build: sliders,
    },
    SectionSpec {
        title: "Stepper",
        description: "Up and down arrows that change a number by an increment; hold an arrow \
                      to repeat. Paired with a text field it makes a number field. Emits \
                      ValueChanged.",
        build: steppers,
    },
    SectionSpec {
        title: "Color well",
        description: "A swatch that opens a color picker in a popover; changes apply at once. \
                      The picker also works on its own: drag the field and hue bar, click a \
                      swatch, or use the arrows and Page Up / Down. Emits ColorChanged.",
        build: colors,
    },
    SectionSpec {
        title: "Date picker",
        description: "Text fields (click a field or use Left / Right, then Up / Down or type \
                      digits), with an optional stepper or calendar popover, or a graphical \
                      calendar and clock. Values stay inside the range. Emits DateChanged.",
        build: dates,
    },
    SectionSpec {
        title: "Path control",
        description: "The components of a path, each clickable, or a pop-up listing them. \
                      Here choosing a component shortens the path to it. Emits \
                      PathItemClicked.",
        build: paths,
    },
];

const TODAY: DateTime = DateTime {
    year: 2026,
    month: 11,
    day: 12,
    hour: 14,
    minute: 30,
};

fn describe(message: &Message) -> Option<String> {
    if let Some(ValueChanged(v)) = message.downcast_ref() {
        Some(format!("{v:.1}"))
    } else if let Some(ColorChanged(c)) = message.downcast_ref() {
        Some(hex_string(*c))
    } else if let Some(DateChanged(d)) = message.downcast_ref() {
        Some(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}",
            d.year, d.month, d.day, d.hour, d.minute
        ))
    } else if let Some(PathItemClicked(i)) = message.downcast_ref() {
        Some(format!("PathItemClicked({i})"))
    } else {
        None
    }
}

#[derive(Default)]
struct Readouts {
    items: Vec<(WidgetId, WidgetId, &'static str)>,
}

impl Readouts {
    fn watch(
        &mut self,
        kit: &mut Kit,
        parent: WidgetId,
        source: WidgetId,
        name: &'static str,
        initial: &str,
    ) {
        let label = kit.tree.insert(
            Some(parent),
            Label::new(format!("{name}: {initial}")).color(Role::Muted),
        );
        self.items.push((source, label, name));
    }
}

impl Demo for Readouts {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        let Some(&(_, label, name)) = self.items.iter().find(|(s, _, _)| *s == message.source)
        else {
            return false;
        };
        let Some(value) = describe(message) else {
            return false;
        };
        cx.tree
            .update::<Label, _>(label, |l, cx| l.set_text(cx, format!("{name}: {value}")));
        true
    }
}

fn row(kit: &mut Kit, parent: WidgetId) -> WidgetId {
    kit.tree.insert(
        Some(parent),
        Container::new(Style {
            align_items: AlignItems::CENTER,
            ..row_style(12.0)
        }),
    )
}

fn column(kit: &mut Kit, parent: WidgetId) -> WidgetId {
    kit.tree.insert(
        Some(parent),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::START,
            gap: gap(0.0, 8.0),
            ..Default::default()
        }),
    )
}

fn sliders(kit: &mut Kit) -> Box<dyn Demo> {
    let mut readouts = Readouts::default();
    let card = kit.card("Slider::new(0, 100, 40) · .length(220)");
    let volume = kit
        .tree
        .insert(Some(card), Slider::new(0.0, 100.0, 40.0).length(220.0));
    kit.tree.set_accessible_label(volume, "Volume");
    readouts.watch(kit, card, volume, "Value", "40.0");
    let off = kit
        .tree
        .insert(Some(card), Slider::new(0.0, 1.0, 0.7).length(220.0));
    kit.tree.set_accessible_label(off, "Opacity");
    kit.tree.set_enabled(off, false);

    let card = kit.card(".tick_marks(11) · .snap_to_ticks(true)");
    let snapped = kit.tree.insert(
        Some(card),
        Slider::new(0.0, 10.0, 3.0)
            .tick_marks(11)
            .snap_to_ticks(true)
            .length(220.0),
    );
    kit.tree.set_accessible_label(snapped, "Snapped");
    readouts.watch(kit, card, snapped, "Snapped", "3.0");
    let ticks = kit.tree.insert(
        Some(card),
        Slider::new(0.0, 1.0, 0.5).tick_marks(5).length(220.0),
    );
    kit.tree.set_accessible_label(ticks, "Free");
    readouts.watch(kit, card, ticks, "Free", "0.5");

    let card = kit.card(".vertical(true) · .circular()");
    let r = row(kit, card);
    let vertical = kit.tree.insert(
        Some(r),
        Slider::new(0.0, 100.0, 60.0).vertical(true).length(110.0),
    );
    kit.tree.set_accessible_label(vertical, "Vertical");
    let vertical_ticks = kit.tree.insert(
        Some(r),
        Slider::new(0.0, 100.0, 25.0)
            .vertical(true)
            .tick_marks(5)
            .snap_to_ticks(true)
            .length(110.0),
    );
    kit.tree
        .set_accessible_label(vertical_ticks, "Vertical (ticks)");
    let dial = kit
        .tree
        .insert(Some(r), Slider::new(0.0, 360.0, 90.0).circular());
    kit.tree.set_accessible_label(dial, "Angle");
    let ticked_dial = kit.tree.insert(
        Some(r),
        Slider::new(0.0, 360.0, 180.0)
            .circular()
            .tick_marks(9)
            .snap_to_ticks(true),
    );
    kit.tree.set_accessible_label(ticked_dial, "Angle (ticks)");
    readouts.watch(kit, card, vertical, "Vertical", "60.0");
    readouts.watch(kit, card, vertical_ticks, "Vertical (ticks)", "25.0");
    readouts.watch(kit, card, dial, "Angle", "90.0");
    readouts.watch(kit, card, ticked_dial, "Angle (ticks)", "180.0");
    Box::new(readouts)
}

struct NumberField {
    field: WidgetId,
    stepper: WidgetId,
    readouts: Readouts,
}

impl Demo for NumberField {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.stepper
            && let Some(ValueChanged(v)) = message.downcast_ref()
        {
            let text = format!("{v}");
            cx.tree
                .update::<TextField, _>(self.field, |f, cx| f.set_text(cx, text));
        }
        if message.source == self.field
            && let Some(TextChanged(text)) = message.downcast_ref()
            && let Ok(v) = text.trim().parse::<f64>()
        {
            cx.tree
                .update::<Stepper, _>(self.stepper, |s, cx| s.set_value(cx, v));
        }
        self.readouts.handle(cx, message)
    }
}

fn steppers(kit: &mut Kit) -> Box<dyn Demo> {
    let mut readouts = Readouts::default();
    let card = kit.card("Stepper::new(0, 10, 5)");
    let plain = kit.tree.insert(Some(card), Stepper::new(0.0, 10.0, 5.0));
    kit.tree.set_accessible_label(plain, "Count");
    readouts.watch(kit, card, plain, "Value", "5.0");

    let card = kit.card("TextField + Stepper::new(0, 100, 12).increment(0.5)");
    let r = row(kit, card);
    kit.tree.insert(Some(r), Label::new("Quantity"));
    let field = kit.tree.insert(Some(r), TextField::new("0", 64.0));
    kit.tree.set_accessible_label(field, "Quantity");
    kit.tree
        .update::<TextField, _>(field, |f, cx| f.set_text(cx, "12"));
    let stepper = kit
        .tree
        .insert(Some(r), Stepper::new(0.0, 100.0, 12.0).increment(0.5));
    kit.tree.set_accessible_label(stepper, "Quantity");
    readouts.watch(kit, card, stepper, "Stepper", "12.0");

    let card = kit.card(".increment(5).wraps(true) · .autorepeat(false)");
    let r = row(kit, card);
    kit.tree.insert(Some(r), Label::new("Minutes"));
    let minutes = kit.tree.insert(
        Some(r),
        Stepper::new(0.0, 55.0, 30.0).increment(5.0).wraps(true),
    );
    kit.tree.set_accessible_label(minutes, "Minutes");
    let single = kit
        .tree
        .insert(Some(r), Stepper::new(0.0, 3.0, 0.0).autorepeat(false));
    kit.tree.set_accessible_label(single, "No repeat");
    readouts.watch(kit, card, minutes, "Minutes", "30.0");
    readouts.watch(kit, card, single, "No repeat", "0.0");
    Box::new(NumberField {
        field,
        stepper,
        readouts,
    })
}

fn colors(kit: &mut Kit) -> Box<dyn Demo> {
    let mut readouts = Readouts::default();
    let card = kit.card("ColorWell::new(color) — click to open the picker");
    let r = row(kit, card);
    let fill = kit
        .tree
        .insert(Some(r), ColorWell::new(Color::hex(0x007aff)));
    kit.tree.set_accessible_label(fill, "Fill");
    let stroke = kit
        .tree
        .insert(Some(r), ColorWell::new(Color::hex(0xff9500)));
    kit.tree.set_accessible_label(stroke, "Stroke");
    let off = kit
        .tree
        .insert(Some(r), ColorWell::new(Color::hex(0x34c759)));
    kit.tree.set_accessible_label(off, "Shadow");
    kit.tree.set_enabled(off, false);
    readouts.watch(kit, card, fill, "Fill", "#007AFF");
    readouts.watch(kit, card, stroke, "Stroke", "#FF9500");

    let card = kit.card("ColorPicker::new(color) on its own");
    let picker = kit
        .tree
        .insert(Some(card), ColorPicker::new(Color::hex(0xaf52de)));
    readouts.watch(kit, card, picker, "Color", "#AF52DE");
    Box::new(readouts)
}

fn dates(kit: &mut Kit) -> Box<dyn Demo> {
    let mut readouts = Readouts::default();
    let initial = "2026-11-12 14:30";
    let card = kit.card("DatePickerStyle::TextField · DateElements::DateAndTime");
    let list = column(kit, card);
    let both = kit.tree.insert(
        Some(list),
        DatePicker::new(TODAY).elements(DateElements::DateAndTime),
    );
    kit.tree.set_accessible_label(both, "Date and time");
    let time = kit.tree.insert(
        Some(list),
        DatePicker::new(TODAY).elements(DateElements::Time),
    );
    kit.tree.set_accessible_label(time, "Time");
    readouts.watch(kit, card, both, "Date and time", initial);
    readouts.watch(kit, card, time, "Time", initial);

    let card = kit.card("TextFieldAndStepper · .range(2026-01-01, 2026-12-31)");
    let ranged = kit.tree.insert(
        Some(card),
        DatePicker::new(TODAY)
            .style(DatePickerStyle::TextFieldAndStepper)
            .range(
                DateTime::date(2026, 1, 1),
                DateTime::new(2026, 12, 31, 23, 59),
            ),
    );
    kit.tree.set_accessible_label(ranged, "Due date");
    readouts.watch(kit, card, ranged, "Value", initial);

    let card = kit.card(".format(DateFormat::for_locale(…)) — your locale, ja-JP, de-DE, en-US");
    let own = ui_text::FontOptions::default().locale;
    for locale in [own.as_str(), "ja-JP", "de-DE", "en-US"] {
        let row = kit.tree.insert(Some(card), Container::new(row_style(12.0)));
        kit.tree
            .insert(Some(row), Label::new(locale).color(Role::Muted));
        let picker = kit.tree.insert(
            Some(row),
            DatePicker::new(TODAY)
                .elements(DateElements::DateAndTime)
                .format(DateFormat::for_locale(locale)),
        );
        kit.tree
            .set_accessible_label(picker, format!("Date and time, {locale}"));
    }

    let card = kit.card(".calendar_popover(true) — press the date");
    let popover = kit
        .tree
        .insert(Some(card), DatePicker::new(TODAY).calendar_popover(true));
    kit.tree.set_accessible_label(popover, "Start date");
    readouts.watch(kit, card, popover, "Value", initial);

    let card = kit.card("DatePickerStyle::Graphical");
    let calendar = kit.tree.insert(
        Some(card),
        DatePicker::new(TODAY).style(DatePickerStyle::Graphical),
    );
    kit.tree.set_accessible_label(calendar, "Date");
    readouts.watch(kit, card, calendar, "Value", initial);

    let card = kit.wide_card(
        "Graphical · DateAndTime — Tab moves from the calendar to the clock",
        420.0,
    );
    let clock = kit.tree.insert(
        Some(card),
        DatePicker::new(TODAY)
            .style(DatePickerStyle::Graphical)
            .elements(DateElements::DateAndTime),
    );
    kit.tree.set_accessible_label(clock, "Appointment");
    readouts.watch(kit, card, clock, "Value", initial);
    Box::new(readouts)
}

const PATH: [&str; 5] = ["Macintosh HD", "Users", "noa", "Documents", "Report.pdf"];

struct PathDemo {
    controls: Vec<WidgetId>,
    reset: WidgetId,
    readouts: Readouts,
}

impl Demo for PathDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.reset && message.downcast_ref::<ButtonClicked>().is_some() {
            for &id in &self.controls {
                cx.tree
                    .update::<PathControl, _>(id, |p, cx| p.set_items(cx, PATH));
            }
            return true;
        }
        if self.controls.contains(&message.source)
            && let Some(&PathItemClicked(i)) = message.downcast_ref()
        {
            cx.tree.update::<PathControl, _>(message.source, |p, cx| {
                p.set_items(cx, PATH[..=i].iter().copied())
            });
        }
        self.readouts.handle(cx, message)
    }
}

fn paths(kit: &mut Kit) -> Box<dyn Demo> {
    let mut readouts = Readouts::default();
    let card = kit.wide_card("PathControl::new(components)", 460.0);
    let standard = kit.tree.insert(Some(card), PathControl::new(PATH));
    kit.tree.set_accessible_label(standard, "Path");
    readouts.watch(kit, card, standard, "Last", "—");

    let card = kit.card(".style(PathStyle::PopUp)");
    let pop_up = kit
        .tree
        .insert(Some(card), PathControl::new(PATH).style(PathStyle::PopUp));
    kit.tree.set_accessible_label(pop_up, "Path, pop-up");
    readouts.watch(kit, card, pop_up, "Last", "—");
    let reset = kit.tree.insert(Some(card), Button::new("Reset paths"));
    Box::new(PathDemo {
        controls: vec![standard, pop_up],
        reset,
        readouts,
    })
}
