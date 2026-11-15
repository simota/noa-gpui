use noa_gpui_core::style::{FlexDirection, Rect as LayoutRect, Size as LayoutSize, length};
use noa_gpui_core::widgets::{
    Button, ButtonClicked, ButtonStyle, CheckState, Checkbox, CheckboxChanged, ComboButton,
    Container, ControlSize, Disclosure, Label, MenuChosen, MenuItem, PopUpButton, Pressable,
    PullDownButton, RadioGroup, SegmentClicked, SegmentMode, SegmentStyle, SegmentedControl,
    SegmentsChanged, SelectionChanged, Switch, Toggled,
};
use noa_gpui_core::{Message, Role, Shortcut, Style, WidgetId};

use super::kit::{Kit, gap, row_style};
use super::{Demo, DemoCx, SectionSpec};

pub const SECTIONS: [SectionSpec; 9] = [
    SectionSpec {
        title: "Button",
        description: "Push buttons in the macOS bezel styles, with text, an icon or both. Space \
                      presses the focused button; Return presses the default button and Escape \
                      the cancel button wherever focus is. A selected button is filled and \
                      framed; a Pressable is one button over whatever it holds. Emits \
                      ButtonClicked.",
        build: buttons,
    },
    SectionSpec {
        title: "Checkbox",
        description: "On, off and mixed. Clicking the box or its label, Space and assistive \
                      technology toggle it; a mixed box turns on. Emits CheckboxChanged.",
        build: checkboxes,
    },
    SectionSpec {
        title: "Radio group",
        description: "Exactly one option is selected. Arrow keys move the selection, wrapping \
                      at the ends. Emits SelectionChanged.",
        build: radios,
    },
    SectionSpec {
        title: "Switch",
        description: "An on / off switch whose knob slides over 150 ms. Click or Space. Emits \
                      Toggled.",
        build: switches,
    },
    SectionSpec {
        title: "Disclosure",
        description: "A triangle (with a label) or a button that shows and hides content. \
                      Click, Space, or Right / Left. Emits Toggled with the expanded state.",
        build: disclosures,
    },
    SectionSpec {
        title: "Pop-up button",
        description: "Shows the chosen option and opens a menu with it checked. Up / Down change \
                      the option without opening the menu; in the menu, Up / Down and Return \
                      choose, Escape or a click outside cancels. Emits SelectionChanged.",
        build: pop_ups,
    },
    SectionSpec {
        title: "Pull-down & combo",
        description: "A pull-down button opens a menu of actions under a fixed title. A combo \
                      button splits a primary action from a menu of related ones. Emit \
                      MenuChosen (and ButtonClicked for the combo's main part).",
        build: pull_downs,
    },
    SectionSpec {
        title: "Segmented control",
        description: "Select one, select any, or momentary (button-like) segments. Left / Right \
                      move between segments, Space activates.",
        build: segmented,
    },
    SectionSpec {
        title: "Control size",
        description: "Every control takes control_size(ControlSize::Mini | Small | Regular | \
                      Large), scaling its metrics and text from the regular size.",
        build: control_sizes,
    },
];

struct EventLog {
    label: WidgetId,
    sources: Vec<(WidgetId, &'static str)>,
}

impl EventLog {
    fn new(kit: &mut Kit, parent: WidgetId) -> Self {
        let label = kit.tree.insert(
            Some(parent),
            Label::new("Last event: —").color(Role::Muted).wrap(true),
        );
        Self {
            label,
            sources: Vec::new(),
        }
    }

    fn watch(&mut self, id: WidgetId, name: &'static str) -> WidgetId {
        self.sources.push((id, name));
        id
    }
}

impl Demo for EventLog {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        let Some(&(_, name)) = self.sources.iter().find(|(id, _)| *id == message.source) else {
            return false;
        };
        let what = if message.downcast_ref::<ButtonClicked>().is_some() {
            "ButtonClicked".to_owned()
        } else if let Some(CheckboxChanged(state)) = message.downcast_ref() {
            format!("CheckboxChanged({state:?})")
        } else if let Some(SelectionChanged(i)) = message.downcast_ref() {
            format!("SelectionChanged({i})")
        } else if let Some(Toggled(on)) = message.downcast_ref() {
            format!("Toggled({on})")
        } else if let Some(MenuChosen(i)) = message.downcast_ref() {
            format!("MenuChosen({i})")
        } else if let Some(SegmentsChanged(states)) = message.downcast_ref() {
            format!("SegmentsChanged({states:?})")
        } else if let Some(SegmentClicked(i)) = message.downcast_ref() {
            format!("SegmentClicked({i})")
        } else {
            return false;
        };
        let text = format!("Last event: {name} → {what}");
        cx.tree
            .update::<Label, _>(self.label, |l, cx| l.set_text(cx, text));
        true
    }
}

fn row(kit: &mut Kit, parent: WidgetId) -> WidgetId {
    kit.tree
        .insert(Some(parent), Container::new(row_style(8.0)))
}

fn column(kit: &mut Kit, parent: WidgetId, gap_y: f32) -> WidgetId {
    kit.tree.insert(
        Some(parent),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            align_items: noa_gpui_core::style::AlignItems::START,
            gap: gap(0.0, gap_y),
            ..Default::default()
        }),
    )
}

fn buttons(kit: &mut Kit) -> Box<dyn Demo> {
    let styles = kit.card("ButtonStyle: Push, Gradient, Recessed, Inline, RoundedRect, Textured");
    let mut log_target = Vec::new();
    let r = row(kit, styles);
    for (name, style) in [
        ("Push", ButtonStyle::Push),
        ("Recessed", ButtonStyle::Recessed),
        ("Inline", ButtonStyle::Inline),
    ] {
        log_target.push((
            kit.tree.insert(Some(r), Button::new(name).style(style)),
            name,
        ));
    }
    let r = row(kit, styles);
    for (name, style) in [
        ("Rounded", ButtonStyle::RoundedRect),
        ("Textured", ButtonStyle::Textured),
        ("+", ButtonStyle::Gradient),
        ("−", ButtonStyle::Gradient),
    ] {
        log_target.push((
            kit.tree.insert(Some(r), Button::new(name).style(style)),
            name,
        ));
    }

    let details =
        kit.card(".detail(\"12\") · .detail_font_family(\"monospace\") · .selected(true)");
    let r = row(kit, details);
    for (name, count) in [("Containers", "12"), ("Images", "48"), ("Volumes", "3")] {
        log_target.push((
            kit.tree.insert(
                Some(r),
                Button::new(name)
                    .style(ButtonStyle::Recessed)
                    .detail(count)
                    .detail_font_family("monospace")
                    .selected(name == "Containers"),
            ),
            name,
        ));
    }

    let pressable = kit.card(
        "Pressable::new(style) · .label(…) · .tooltip(…) — its children only show; the whole box \
         is the button",
    );
    let engine = kit.tree.insert(
        Some(pressable),
        Pressable::new(Style {
            gap: gap(8.0, 0.0),
            padding: LayoutRect {
                left: length(8.0),
                right: length(8.0),
                top: length(4.0),
                bottom: length(4.0),
            },
            align_items: noa_gpui_core::style::AlignItems::CENTER,
            ..Default::default()
        })
        .label("Engine: Docker Desktop, running")
        .tooltip("Choose the engine"),
    );
    kit.tree
        .insert(Some(engine), Label::new("●").color(Role::Accent));
    kit.tree.insert(Some(engine), Label::new("Docker Desktop"));
    kit.tree.insert(
        Some(engine),
        Label::new("unix:///var/run/docker.sock")
            .color(Role::Muted)
            .font_family("monospace"),
    );
    kit.tree
        .insert(Some(engine), Label::new("▾").color(Role::Muted));
    log_target.push((engine, "engine"));

    let icons = kit.card(".icon(id) · Button::icon_only(id) · Circular · Button::help()");
    let r = row(kit, icons);
    log_target.push((
        kit.tree
            .insert(Some(r), Button::new("Favorite").icon(kit.assets.icons[0])),
        "Favorite",
    ));
    let ring = kit
        .tree
        .insert(Some(r), Button::icon_only(kit.assets.icons[1]));
    kit.tree.set_accessible_label(ring, "Ring");
    log_target.push((ring, "icon only"));
    let gradient = kit.tree.insert(
        Some(r),
        Button::icon_only(kit.assets.icons[3]).style(ButtonStyle::Circular),
    );
    kit.tree.set_accessible_label(gradient, "Gradient");
    log_target.push((gradient, "circular"));
    log_target.push((kit.tree.insert(Some(r), Button::help()), "help"));

    let dialog = kit.card(
        "default_button (Return) · cancel_button (Escape) · shortcut(Cmd/Ctrl+D) — try the keys",
    );
    let r = row(kit, dialog);
    log_target.push((
        kit.tree.insert(
            Some(r),
            Button::new("Don't Save").shortcut(Shortcut::command('d')),
        ),
        "Don't Save",
    ));
    log_target.push((
        kit.tree
            .insert(Some(r), Button::new("Cancel").cancel_button(true)),
        "Cancel",
    ));
    log_target.push((
        kit.tree
            .insert(Some(r), Button::new("Save").default_button(true)),
        "Save",
    ));
    let disabled = kit.tree.insert(Some(dialog), Button::new("Disabled"));
    kit.tree.set_enabled(disabled, false);
    let mut log = EventLog::new(kit, dialog);
    for (id, name) in log_target {
        log.watch(id, name);
    }
    Box::new(log)
}

fn checkboxes(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Checkbox::new(label) · .checked(true) · set_state(CheckState::Mixed)");
    let list = column(kit, card, 8.0);
    let all = kit.tree.insert(Some(list), Checkbox::new("All formats"));
    kit.tree
        .update::<Checkbox, _>(all, |c, cx| c.set_state(cx, CheckState::Mixed));
    let indented = kit.tree.insert(
        Some(list),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 8.0),
            padding: LayoutRect {
                left: length(22.0),
                right: length(0.0),
                top: length(0.0),
                bottom: length(0.0),
            },
            ..Default::default()
        }),
    );
    let png = kit
        .tree
        .insert(Some(indented), Checkbox::new("PNG").checked(true));
    let jpeg = kit.tree.insert(Some(indented), Checkbox::new("JPEG"));
    let off = kit
        .tree
        .insert(Some(list), Checkbox::new("Disabled option").checked(true));
    kit.tree.set_enabled(off, false);
    let mut log = EventLog::new(kit, card);
    log.watch(all, "All formats");
    log.watch(png, "PNG");
    log.watch(jpeg, "JPEG");
    Box::new(log)
}

fn radios(kit: &mut Kit) -> Box<dyn Demo> {
    let vertical = kit.card("RadioGroup::new([...])");
    let size = kit.tree.insert(
        Some(vertical),
        RadioGroup::new(["Small", "Medium", "Large"]).selected_index(1),
    );
    kit.tree.set_accessible_label(size, "Size");
    let mut log = EventLog::new(kit, vertical);
    log.watch(size, "size");
    let horizontal = kit.card(".horizontal(true)");
    let align = kit.tree.insert(
        Some(horizontal),
        RadioGroup::new(["Left", "Center", "Right"]).horizontal(true),
    );
    kit.tree.set_accessible_label(align, "Alignment");
    log.watch(align, "alignment");
    Box::new(log)
}

fn labeled(
    kit: &mut Kit,
    parent: WidgetId,
    text: &str,
    widget: impl noa_gpui_core::Widget,
) -> WidgetId {
    let r = kit.tree.insert(
        Some(parent),
        Container::new(Style {
            align_items: noa_gpui_core::style::AlignItems::CENTER,
            justify_content: noa_gpui_core::style::JustifyContent::SPACE_BETWEEN,
            size: LayoutSize {
                width: length(220.0),
                height: noa_gpui_core::style::auto(),
            },
            ..Default::default()
        }),
    );
    kit.tree.insert(Some(r), Label::new(text));
    kit.tree.insert(Some(r), widget)
}

fn switches(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Switch::new(on)");
    let list = column(kit, card, 10.0);
    let wifi = labeled(kit, list, "Wi-Fi", Switch::new(true));
    kit.tree.set_accessible_label(wifi, "Wi-Fi");
    let bluetooth = labeled(kit, list, "Bluetooth", Switch::new(false));
    kit.tree.set_accessible_label(bluetooth, "Bluetooth");
    let airdrop = labeled(kit, list, "AirDrop (disabled)", Switch::new(false));
    kit.tree.set_accessible_label(airdrop, "AirDrop");
    kit.tree.set_enabled(airdrop, false);
    let mut log = EventLog::new(kit, card);
    log.watch(wifi, "Wi-Fi");
    log.watch(bluetooth, "Bluetooth");
    Box::new(log)
}

struct DisclosureDemo {
    log: EventLog,
    triangle: WidgetId,
    details: WidgetId,
}

impl Demo for DisclosureDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.triangle
            && let Some(Toggled(expanded)) = message.downcast_ref()
        {
            let style = Style {
                display: if *expanded {
                    noa_gpui_core::style::Display::Flex
                } else {
                    noa_gpui_core::style::Display::None
                },
                flex_direction: FlexDirection::Column,
                gap: gap(0.0, 4.0),
                padding: LayoutRect {
                    left: length(18.0),
                    right: length(0.0),
                    top: length(0.0),
                    bottom: length(0.0),
                },
                ..Default::default()
            };
            cx.tree
                .update::<Container, _>(self.details, |c, cx| c.set_style(cx, style));
        }
        self.log.handle(cx, message)
    }
}

fn disclosures(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Disclosure::triangle(label) · Disclosure::button()");
    let triangle = kit
        .tree
        .insert(Some(card), Disclosure::triangle("Advanced options"));
    let details = kit.tree.insert(
        Some(card),
        Container::new(Style {
            display: noa_gpui_core::style::Display::None,
            ..Default::default()
        }),
    );
    kit.tree
        .insert(Some(details), Label::new("Proxy: none").color(Role::Muted));
    kit.tree.insert(
        Some(details),
        Label::new("Timeout: 30 s").color(Role::Muted),
    );
    let r = row(kit, card);
    let button = kit.tree.insert(Some(r), Disclosure::button());
    kit.tree.set_accessible_label(button, "Details");
    kit.tree
        .insert(Some(r), Label::new("Disclosure button").color(Role::Muted));
    let mut log = EventLog::new(kit, card);
    log.watch(triangle, "triangle");
    log.watch(button, "button");
    Box::new(DisclosureDemo {
        log,
        triangle,
        details,
    })
}

fn pop_ups(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("PopUpButton::new([...]).selected_index(1)");
    let r = row(kit, card);
    kit.tree.insert(Some(r), Label::new("Sort by"));
    let sort = kit.tree.insert(
        Some(r),
        PopUpButton::new(["Name", "Date Modified", "Size", "Kind"]).selected_index(1),
    );
    kit.tree.set_accessible_label(sort, "Sort by");
    let mut log = EventLog::new(kit, card);
    log.watch(sort, "Sort by");
    Box::new(log)
}

fn pull_downs(kit: &mut Kit) -> Box<dyn Demo> {
    let items = || {
        vec![
            MenuItem::new("Duplicate").shortcut("⌘D"),
            MenuItem::new("Rename…"),
            MenuItem::separator(),
            MenuItem::new("Move to Trash").shortcut("⌘⌫"),
            MenuItem::new("Unavailable").enabled(false),
        ]
    };
    let card = kit.card("PullDownButton::new(title, items) · ComboButton::new(title, items)");
    let r = row(kit, card);
    let pull = kit
        .tree
        .insert(Some(r), PullDownButton::new("Actions", items()));
    let combo = kit.tree.insert(
        Some(r),
        ComboButton::new(
            "Share",
            vec![
                MenuItem::new("Mail"),
                MenuItem::new("Messages"),
                MenuItem::new("Copy Link"),
            ],
        ),
    );
    let mut log = EventLog::new(kit, card);
    log.watch(pull, "Actions");
    log.watch(combo, "Share");
    Box::new(log)
}

fn segmented(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit
        .card("SegmentMode::SelectOne · SelectAny · Momentary · .style(SegmentStyle::Underline)");
    let list = column(kit, card, 10.0);
    let view = kit.tree.insert(
        Some(list),
        SegmentedControl::new(["Day", "Week", "Month", "Year"], SegmentMode::SelectOne)
            .with_selected(&[1]),
    );
    kit.tree.set_accessible_label(view, "Calendar view");
    let style = kit.tree.insert(
        Some(list),
        SegmentedControl::new(["Bold", "Italic", "Underline"], SegmentMode::SelectAny)
            .with_selected(&[0]),
    );
    kit.tree.set_accessible_label(style, "Text style");
    let nav = kit.tree.insert(
        Some(list),
        SegmentedControl::new(["Back", "Forward"], SegmentMode::Momentary),
    );
    kit.tree.set_accessible_label(nav, "History");
    let tabs = kit.tree.insert(
        Some(list),
        SegmentedControl::new(["Logs", "Inspect", "Stats", "Exec"], SegmentMode::SelectOne)
            .style(SegmentStyle::Underline),
    );
    kit.tree.set_accessible_label(tabs, "Container details");
    let mut log = EventLog::new(kit, card);
    log.watch(tabs, "tabs");
    log.watch(view, "view");
    log.watch(style, "style");
    log.watch(nav, "navigation");
    Box::new(log)
}

fn control_sizes(kit: &mut Kit) -> Box<dyn Demo> {
    for (name, size, size_name) in [
        ("ControlSize::Mini", ControlSize::Mini, "mini"),
        ("ControlSize::Small", ControlSize::Small, "small"),
        ("ControlSize::Regular", ControlSize::Regular, "regular"),
        ("ControlSize::Large", ControlSize::Large, "large"),
    ] {
        let card = kit.card(name);
        let list = column(kit, card, 8.0);
        let r = row(kit, list);
        kit.tree
            .insert(Some(r), Button::new("Button").control_size(size));
        let choice = kit
            .tree
            .insert(Some(r), PopUpButton::new(["Pop-up"]).control_size(size));
        kit.tree
            .set_accessible_label(choice, format!("Choice, {size_name}"));
        let enabled = kit
            .tree
            .insert(Some(r), Switch::new(true).control_size(size));
        kit.tree
            .set_accessible_label(enabled, format!("Enabled, {size_name}"));
        let r = row(kit, list);
        kit.tree.insert(
            Some(r),
            Checkbox::new("Checkbox").checked(true).control_size(size),
        );
        let page = kit.tree.insert(
            Some(r),
            SegmentedControl::new(["One", "Two"], SegmentMode::SelectOne).control_size(size),
        );
        kit.tree
            .set_accessible_label(page, format!("Page, {size_name}"));
    }
    Box::new(())
}
