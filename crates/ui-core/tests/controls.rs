use ui_core::style::{FlexDirection, Size as LayoutSize, percent};
use ui_core::widgets::{
    Button, ButtonClicked, ButtonStyle, CheckState, Checkbox, CheckboxChanged, ComboButton,
    Container, Disclosure, Menu, MenuChosen, MenuItem, PopUpButton, PullDownButton, RadioGroup,
    SegmentClicked, SegmentMode, SegmentedControl, SegmentsChanged, SelectionChanged, Switch,
    TextField, Toggled,
};
use ui_core::{
    Event, Key, Message, Modifiers, OverlayOptions, Placement, PointerButton, Shortcut, Style,
    UiTree, WidgetId,
};
use ui_scene::{DrawCommand, Point, Rect, Size};
use ui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(500.0, 400.0);

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
        self.tree.insert(Some(self.root), widget)
    }

    fn prepare(&mut self) {
        self.tree.prepare(VIEWPORT, &mut self.text);
    }

    fn center(&self, id: WidgetId) -> Point {
        let r = self.tree.window_rect(id).unwrap();
        Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    fn click_at(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.tree
            .dispatch(Event::PointerDown { pos, button }, &mut self.text);
        self.tree
            .dispatch(Event::PointerUp { pos, button }, &mut self.text);
    }

    fn click(&mut self, id: WidgetId) {
        self.prepare();
        let pos = self.center(id);
        self.click_at(pos);
    }

    fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }

    fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.tree
            .dispatch(Event::KeyDown { key, modifiers }, &mut self.text);
    }

    fn messages(&mut self) -> Vec<Message> {
        self.tree.take_messages()
    }

    fn open_menu(&self) -> Option<WidgetId> {
        self.tree
            .overlays()
            .find(|&o| self.tree.get::<Menu>(o).is_some())
    }
}

fn only<T: 'static + Clone>(messages: &[Message]) -> Vec<(WidgetId, T)> {
    messages
        .iter()
        .filter_map(|m| m.downcast_ref::<T>().map(|p| (m.source, p.clone())))
        .collect()
}

#[test]
fn return_and_escape_activate_the_default_and_cancel_buttons_from_anywhere() {
    let mut h = Harness::new();
    let field = h.add(TextField::new("", 200.0));
    let cancel = h.add(Button::new("Cancel").cancel_button(true));
    let ok = h.add(Button::new("OK").default_button(true));
    let save = h.add(Button::new("Save").shortcut(Shortcut::command('s')));
    h.prepare();
    h.tree.set_focus(Some(field));

    h.key(Key::Enter);
    assert_eq!(only::<ButtonClicked>(&h.messages()), [(ok, ButtonClicked)]);
    h.key(Key::Escape);
    assert_eq!(
        only::<ButtonClicked>(&h.messages()),
        [(cancel, ButtonClicked)]
    );
    h.key_with(
        Key::Char('s'),
        Modifiers {
            command: true,
            meta: true,
            ..Default::default()
        },
    );
    assert_eq!(
        only::<ButtonClicked>(&h.messages()),
        [(save, ButtonClicked)]
    );

    h.tree.set_focus(Some(cancel));
    h.key(Key::Space);
    assert_eq!(
        only::<ButtonClicked>(&h.messages()),
        [(cancel, ButtonClicked)]
    );
    h.key(Key::Enter);
    assert_eq!(only::<ButtonClicked>(&h.messages()), [(ok, ButtonClicked)]);

    h.tree.set_enabled(ok, false);
    h.key(Key::Enter);
    assert!(only::<ButtonClicked>(&h.messages()).is_empty());
}

#[test]
fn inside_a_modal_only_its_own_default_button_answers_return() {
    let mut h = Harness::new();
    h.add(Button::new("Behind").default_button(true));
    h.prepare();
    let alert = h.tree.open_overlay(
        Container::new(Style::default()),
        Placement::Center,
        OverlayOptions::MODAL,
    );
    let delete = h
        .tree
        .insert(Some(alert), Button::new("Delete").default_button(true));
    h.prepare();
    h.key(Key::Enter);
    assert_eq!(
        only::<ButtonClicked>(&h.messages()),
        [(delete, ButtonClicked)]
    );
}

#[test]
fn checkboxes_toggle_by_click_and_space_and_mixed_turns_on() {
    let mut h = Harness::new();
    let check = h.add(Checkbox::new("Show hidden files"));
    h.click(check);
    assert_eq!(
        only::<CheckboxChanged>(&h.messages()),
        [(check, CheckboxChanged(CheckState::On))]
    );
    h.tree.set_focus(Some(check));
    h.key(Key::Space);
    assert_eq!(
        h.tree.get::<Checkbox>(check).unwrap().state(),
        CheckState::Off
    );
    h.tree
        .update::<Checkbox, _>(check, |c, cx| c.set_state(cx, CheckState::Mixed));
    h.click(check);
    assert_eq!(
        h.tree.get::<Checkbox>(check).unwrap().state(),
        CheckState::On
    );
}

#[test]
fn radio_groups_select_one_option_by_click_or_arrow_keys() {
    let mut h = Harness::new();
    let radio = h.add(RadioGroup::new(["Small", "Medium", "Large"]));
    h.prepare();
    let rect = h.tree.window_rect(radio).unwrap();
    h.click_at(Point::new(rect.x + 8.0, rect.bottom() - 6.0));
    assert_eq!(
        only::<SelectionChanged>(&h.messages()),
        [(radio, SelectionChanged(2))]
    );
    assert_eq!(h.tree.focused(), Some(radio));
    h.key(Key::ArrowDown);
    assert_eq!(
        h.tree.get::<RadioGroup>(radio).unwrap().selected(),
        0,
        "wraps"
    );
    h.key(Key::ArrowUp);
    assert_eq!(h.tree.get::<RadioGroup>(radio).unwrap().selected(), 2);
    assert_eq!(only::<SelectionChanged>(&h.messages()).len(), 2);
}

#[test]
fn switches_toggle_and_slide_their_knob() {
    let mut h = Harness::new();
    let switch = h.add(Switch::new(false));
    h.tree.set_now(1.0);
    h.click(switch);
    assert_eq!(only::<Toggled>(&h.messages()), [(switch, Toggled(true))]);
    assert!(h.tree.wants_animation_frame());
    h.tree.animate(1.05, &mut h.text);
    let mid = h.tree.get::<Switch>(switch).unwrap().knob_position();
    assert!(mid > 0.0 && mid < 1.0, "{mid}");
    h.tree.animate(2.0, &mut h.text);
    assert_eq!(h.tree.get::<Switch>(switch).unwrap().knob_position(), 1.0);

    h.tree.set_reduce_motion(true);
    h.tree.set_now(3.0);
    h.click(switch);
    assert_eq!(only::<Toggled>(&h.messages()), [(switch, Toggled(false))]);
    h.tree.animate(3.01, &mut h.text);
    assert_eq!(h.tree.get::<Switch>(switch).unwrap().knob_position(), 0.0);
    assert!(!h.tree.wants_animation_frame());
    assert!(!h.tree.wants_animation_frame());
}

#[test]
fn disclosures_toggle_by_click_space_and_arrows() {
    let mut h = Harness::new();
    let triangle = h.add(Disclosure::triangle("Advanced"));
    let button = h.add(Disclosure::button());
    h.click(triangle);
    h.click(button);
    assert_eq!(
        only::<Toggled>(&h.messages()),
        [(triangle, Toggled(true)), (button, Toggled(true))]
    );
    h.tree.set_focus(Some(triangle));
    h.key(Key::ArrowLeft);
    assert!(!h.tree.get::<Disclosure>(triangle).unwrap().is_expanded());
    h.key(Key::ArrowRight);
    assert!(h.tree.get::<Disclosure>(triangle).unwrap().is_expanded());
}

#[test]
fn pop_up_buttons_stay_as_wide_as_their_widest_option_without_reshaping_it() {
    let mut h = Harness::new();
    let mut options: Vec<String> = (0..40).map(|i| format!("Option {i}")).collect();
    options[7] = "A much, much longer option".into();
    let popup = h.add(PopUpButton::new(options));
    h.prepare();
    let width = h.tree.window_rect(popup).unwrap().width;
    let revision = h.text.layouts().changes();
    for index in [3, 7, 20] {
        h.tree
            .update::<PopUpButton, _>(popup, |p, cx| p.set_selected(cx, index));
        h.prepare();
        assert_eq!(h.tree.window_rect(popup).unwrap().width, width, "{index}");
    }
    // Only the shown option is shaped again: a layout made and one dropped per change.
    let changes = h.text.layouts().changes() - revision;
    assert!(changes <= 2 * 3, "{changes} layouts made or dropped");
}

#[test]
fn pop_up_buttons_choose_from_a_menu_with_pointer_or_keyboard() {
    let mut h = Harness::new();
    let popup = h.add(PopUpButton::new(["Red", "Green", "Blue"]).selected_index(1));
    h.click(popup);
    let menu = h.open_menu().expect("a menu opened");
    assert_eq!(h.tree.focused(), Some(menu), "the menu takes the keyboard");
    assert_eq!(h.tree.get::<Menu>(menu).unwrap().highlighted(), Some(1));
    assert!(h.tree.get::<PopUpButton>(popup).unwrap().is_open());
    h.prepare();
    let menu_rect = h.tree.window_rect(menu).unwrap();
    let button_rect = h.tree.window_rect(popup).unwrap();
    assert!(
        menu_rect.width >= button_rect.width,
        "at least as wide as the button"
    );

    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert!(h.open_menu().is_none());
    assert_eq!(h.tree.focused(), Some(popup), "focus returns to the button");
    assert_eq!(
        only::<SelectionChanged>(&h.messages()),
        [(popup, SelectionChanged(2))]
    );

    h.click(popup);
    h.prepare();
    let menu = h.open_menu().unwrap();
    let rect = h.tree.window_rect(menu).unwrap();
    let first_row = Point::new(rect.x + 20.0, rect.y + 5.0 + 12.0);
    h.tree
        .dispatch(Event::PointerMove { pos: first_row }, &mut h.text);
    h.tree.dispatch(
        Event::PointerUp {
            pos: first_row,
            button: PointerButton::Primary,
        },
        &mut h.text,
    );
    assert_eq!(h.tree.get::<PopUpButton>(popup).unwrap().selected(), 0);

    h.messages();
    h.click(popup);
    h.key(Key::Escape);
    assert!(h.open_menu().is_none());
    assert!(!h.tree.get::<PopUpButton>(popup).unwrap().is_open());
    assert!(only::<SelectionChanged>(&h.messages()).is_empty());

    h.tree.set_focus(Some(popup));
    h.key(Key::ArrowDown);
    assert_eq!(
        only::<SelectionChanged>(&h.messages()),
        [(popup, SelectionChanged(1))]
    );
    assert!(h.open_menu().is_none());
}

#[test]
fn pull_down_and_combo_buttons_report_the_chosen_action() {
    let mut h = Harness::new();
    let items = || {
        vec![
            MenuItem::new("Duplicate"),
            MenuItem::separator(),
            MenuItem::new("Disabled").enabled(false),
            MenuItem::new("Delete"),
        ]
    };
    let pull = h.add(PullDownButton::new("Actions", items()));
    let combo = h.add(ComboButton::new("Share", items()));
    h.prepare();

    h.click(pull);
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(only::<MenuChosen>(&h.messages()), [(pull, MenuChosen(3))]);

    let rect = h.tree.window_rect(combo).unwrap();
    h.click_at(Point::new(rect.x + 10.0, rect.y + rect.height / 2.0));
    assert_eq!(
        only::<ButtonClicked>(&h.messages()),
        [(combo, ButtonClicked)]
    );
    assert!(h.open_menu().is_none());
    h.click_at(Point::new(rect.right() - 6.0, rect.y + rect.height / 2.0));
    assert!(h.open_menu().is_some());
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    assert_eq!(only::<MenuChosen>(&h.messages()), [(combo, MenuChosen(0))]);
}

#[test]
fn segmented_controls_select_one_select_any_or_act_momentarily() {
    let mut h = Harness::new();
    let one = h.add(SegmentedControl::new(
        ["Day", "Week", "Month"],
        SegmentMode::SelectOne,
    ));
    let any = h.add(SegmentedControl::new(
        ["B", "I", "U"],
        SegmentMode::SelectAny,
    ));
    let momentary = h.add(SegmentedControl::new(["<", ">"], SegmentMode::Momentary));
    h.prepare();
    let at = |h: &Harness, id: WidgetId, fraction: f32| {
        let r = h.tree.window_rect(id).unwrap();
        Point::new(r.x + r.width * fraction, r.y + r.height / 2.0)
    };

    let p = at(&h, one, 0.5);
    h.click_at(p);
    assert_eq!(
        only::<SelectionChanged>(&h.messages()),
        [(one, SelectionChanged(1))]
    );
    assert_eq!(
        h.tree.get::<SegmentedControl>(one).unwrap().selected(),
        [false, true, false]
    );
    h.key(Key::ArrowRight);
    h.key(Key::Space);
    assert_eq!(
        only::<SelectionChanged>(&h.messages()),
        [(one, SelectionChanged(2))]
    );

    let (b, u) = (at(&h, any, 0.15), at(&h, any, 0.85));
    h.click_at(b);
    h.click_at(u);
    let changes = only::<SegmentsChanged>(&h.messages());
    assert_eq!(
        changes.last().unwrap().1,
        SegmentsChanged(vec![true, false, true])
    );

    let next = at(&h, momentary, 0.75);
    h.click_at(next);
    assert_eq!(
        only::<SegmentClicked>(&h.messages()),
        [(momentary, SegmentClicked(1))]
    );
    assert_eq!(
        h.tree
            .get::<SegmentedControl>(momentary)
            .unwrap()
            .selected(),
        [false, false]
    );
}

#[test]
fn segmented_controls_change_their_selection_from_the_app() {
    let mut h = Harness::new();
    let one = h.add(SegmentedControl::new(
        ["A", "B", "C"],
        SegmentMode::SelectOne,
    ));
    let any = h.add(SegmentedControl::new(
        ["B", "I", "U"],
        SegmentMode::SelectAny,
    ));
    h.prepare();
    let set = |h: &mut Harness, id, indices: &[usize]| {
        h.tree
            .update::<SegmentedControl, _>(id, |s, cx| s.set_selected(cx, indices));
    };
    set(&mut h, one, &[2]);
    set(&mut h, any, &[0, 2]);
    set(&mut h, one, &[]);
    assert_eq!(
        h.tree.get::<SegmentedControl>(one).unwrap().selected(),
        [false, false, true]
    );
    assert_eq!(
        h.tree.get::<SegmentedControl>(any).unwrap().selected(),
        [true, false, true]
    );
    assert!(
        h.messages().is_empty(),
        "no message for a change by the app"
    );
    h.tree.set_focus(Some(one));
    h.key(Key::ArrowLeft);
    h.key(Key::Space);
    assert_eq!(
        only::<SelectionChanged>(&h.messages()),
        [(one, SelectionChanged(1))]
    );
}

#[test]
fn a_segmented_control_created_hidden_paints_once_shown() {
    let mut h = Harness::new();
    let tabs = h.add(SegmentedControl::new(
        ["A", "B", "C"],
        SegmentMode::SelectOne,
    ));
    h.tree.set_hidden(tabs, true);
    h.prepare();
    h.tree.set_hidden(tabs, false);
    h.prepare();
    let r = h.tree.window_rect(tabs).unwrap();
    assert!(r.width > 0.0 && r.height > 0.0);
    h.click_at(Point::new(r.x + r.width * 0.9, r.y + r.height / 2.0));
    assert_eq!(
        only::<SelectionChanged>(&h.messages()),
        [(tabs, SelectionChanged(2))]
    );
}

#[test]
fn assistive_technology_can_operate_the_controls() {
    use ui_core::accesskit::{Action, ActionRequest, Role, Toggled as AxToggled, TreeId};
    let mut h = Harness::new();
    let check = h.add(Checkbox::new("Wrap lines").checked(true));
    let switch = h.add(Switch::new(false));
    let popup = h.add(PopUpButton::new(["A", "B"]));
    h.prepare();
    let update = h.tree.accessibility_update(1.0);
    let node = |id| {
        update
            .nodes
            .iter()
            .find(|(n, _)| *n == ui_core::access_id(id))
            .map(|(_, node)| node.clone())
            .unwrap()
    };
    assert_eq!(node(check).role(), Role::CheckBox);
    assert_eq!(node(check).toggled(), Some(AxToggled::True));
    assert_eq!(node(switch).role(), Role::Switch);
    assert_eq!(node(popup).value(), Some("A"));

    let click = |h: &mut Harness, id| {
        h.tree.accessibility_action(
            &ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: ui_core::access_id(id),
                data: None,
            },
            &mut h.text,
        );
    };
    click(&mut h, check);
    click(&mut h, switch);
    assert_eq!(
        h.tree.get::<Checkbox>(check).unwrap().state(),
        CheckState::Off
    );
    assert!(h.tree.get::<Switch>(switch).unwrap().is_on());
    click(&mut h, popup);
    assert!(h.open_menu().is_some(), "Click opens the pop-up's menu");
}

#[test]
fn control_sizes_scale_every_control() {
    use ui_core::widgets::ControlSize;
    let mut h = Harness::new();
    let sizes = [
        ControlSize::Mini,
        ControlSize::Small,
        ControlSize::Regular,
        ControlSize::Large,
    ];
    let mut rows: Vec<Vec<WidgetId>> = Vec::new();
    for size in sizes {
        rows.push(vec![
            h.add(Button::new("Save").control_size(size)),
            h.add(Checkbox::new("Check").control_size(size)),
            h.add(RadioGroup::new(["A", "B"]).control_size(size)),
            h.add(Switch::new(true).control_size(size)),
            h.add(PopUpButton::new(["One", "Two"]).control_size(size)),
            h.add(PullDownButton::new("Do", vec![MenuItem::new("X")]).control_size(size)),
            h.add(ComboButton::new("Do", vec![MenuItem::new("X")]).control_size(size)),
            h.add(SegmentedControl::new(["L", "R"], SegmentMode::SelectOne).control_size(size)),
        ]);
    }
    h.prepare();
    for control in 0..rows[0].len() {
        let heights: Vec<f32> = rows
            .iter()
            .map(|row| h.tree.layout(row[control]).unwrap().rect.height)
            .collect();
        assert!(
            heights.windows(2).all(|w| w[0] < w[1]),
            "control {control}: heights {heights:?} grow from mini to large"
        );
    }
}

#[test]
fn radio_options_and_segments_are_separate_accessibility_items() {
    use ui_core::accesskit::{Action, ActionRequest, Role, Toggled as AxToggled, TreeId};
    use ui_core::{access_id, access_item_id};
    let mut h = Harness::new();
    let radio = h.add(RadioGroup::new(["Small", "Medium", "Large"]).selected_index(1));
    let tabs = h.add(SegmentedControl::new(
        ["Day", "Week"],
        SegmentMode::SelectOne,
    ));
    h.prepare();
    let update = h.tree.accessibility_update(1.0);
    let node = |id| {
        update
            .nodes
            .iter()
            .find(|(n, _)| *n == id)
            .map(|(_, node)| node.clone())
            .unwrap()
    };
    let group = node(access_id(radio));
    assert_eq!(
        group.children(),
        (0..3).map(|i| access_item_id(radio, i)).collect::<Vec<_>>()
    );
    let medium = node(access_item_id(radio, 1));
    assert_eq!(medium.role(), Role::RadioButton);
    assert_eq!(medium.label(), Some("Medium"));
    assert_eq!(medium.toggled(), Some(AxToggled::True));
    assert_eq!(
        node(access_item_id(radio, 0)).toggled(),
        Some(AxToggled::False)
    );
    // Item bounds are in window coordinates, inside the group.
    let (g, item) = (
        group.bounds().unwrap(),
        node(access_item_id(radio, 2)).bounds().unwrap(),
    );
    assert!(item.y0 >= g.y0 && item.y1 <= g.y1 && item.y0 > g.y0);
    let week = node(access_item_id(tabs, 1));
    assert_eq!(
        (week.role(), week.label(), week.is_selected()),
        (Role::Tab, Some("Week"), Some(false))
    );

    let click = |h: &mut Harness, target| {
        h.tree.accessibility_action(
            &ActionRequest {
                action: Action::Click,
                target_tree: TreeId::ROOT,
                target_node: target,
                data: None,
            },
            &mut h.text,
        );
    };
    click(&mut h, access_item_id(radio, 2));
    click(&mut h, access_item_id(tabs, 1));
    let selections = only::<SelectionChanged>(&h.messages());
    assert_eq!(
        selections,
        [(radio, SelectionChanged(2)), (tabs, SelectionChanged(1))]
    );
}

#[test]
fn a_button_shows_a_detail_after_its_label_in_its_own_smaller_muted_type() {
    let mut h = Harness::new();
    let plain = h.add(Button::new("Containers").style(ButtonStyle::Recessed));
    let counted = h.add(
        Button::new("Containers")
            .style(ButtonStyle::Recessed)
            .detail("12")
            .detail_font_family("monospace"),
    );
    h.prepare();
    let width = |id| h.tree.window_rect(id).unwrap().width;
    assert!(width(counted) > width(plain));

    let texts: Vec<(String, Rect, ui_scene::Color, bool)> = (h.tree.display_list().commands())
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Text {
                rect,
                layout,
                color,
            } => {
                let line = &h.text.layouts().buffer(*layout).lines[0];
                let named = matches!(
                    line.attrs_list().defaults().family,
                    ui_text::cosmic_text::Family::Name(_)
                );
                Some((line.text().to_owned(), *rect, *color, named))
            }
            _ => None,
        })
        .collect();
    let bounds = h.tree.window_rect(counted).unwrap();
    let (label, detail) = {
        let mut inside = texts
            .iter()
            .filter(|(_, r, ..)| bounds.contains(Point::new(r.x, r.y)));
        (inside.next().unwrap(), inside.next().unwrap())
    };
    assert_eq!(label.0, "Containers");
    assert_eq!(detail.0, "12");
    let theme = h.tree.theme().clone();
    assert_eq!(label.2, theme.text);
    assert_eq!(detail.2, theme.muted);
    assert!(detail.3 && !label.3, "the detail alone is monospaced");
    assert_eq!(detail.1.x, label.1.right() + 6.0);
    assert!(detail.1.height < label.1.height);
    let middle = |r: Rect| r.y + r.height / 2.0;
    assert!((middle(detail.1) - middle(label.1)).abs() < 0.5);

    let update = h.tree.accessibility_update(1.0);
    let node = &(update.nodes.iter())
        .find(|(n, _)| *n == ui_core::access_id(counted))
        .unwrap()
        .1;
    assert_eq!(node.label(), Some("Containers 12"));
}

#[test]
fn selected_buttons_are_filled_and_framed_in_any_style() {
    let mut h = Harness::new();
    let plain = h.add(Button::new("Images").style(ButtonStyle::Recessed));
    let current = h.add(
        Button::new("Containers")
            .style(ButtonStyle::Recessed)
            .selected(true),
    );
    h.prepare();
    let theme = h.tree.theme().clone();
    let bezel = |h: &Harness, id: WidgetId| {
        let bounds = h.tree.window_rect(id).unwrap();
        h.tree
            .display_list()
            .commands()
            .iter()
            .find_map(|c| match c {
                DrawCommand::Quad {
                    rect,
                    color,
                    border,
                    ..
                } if *rect == bounds && (color.a > 0.0 || border.width > 0.0) => {
                    Some((*color, *border))
                }
                _ => None,
            })
    };
    assert_eq!(bezel(&h, plain), None, "recessed: no bezel until hovered");
    let (fill, border) = bezel(&h, current).unwrap();
    assert_eq!(fill, theme.selection);
    assert_eq!((border.width, border.color), (1.0, theme.accent));

    h.tree
        .update::<Button, _>(current, |b, cx| b.set_selected(cx, false));
    h.tree
        .update::<Button, _>(plain, |b, cx| b.set_selected(cx, true));
    h.prepare();
    assert!(!h.tree.stats().layout);
    assert_eq!(bezel(&h, current), None);
    assert_eq!(bezel(&h, plain).unwrap().0, theme.selection);
    let update = h.tree.accessibility_update(1.0);
    let selected = |id| {
        update
            .nodes
            .iter()
            .find(|(n, _)| *n == ui_core::access_id(id))
            .unwrap()
            .1
            .is_selected()
    };
    assert_eq!((selected(plain), selected(current)), (Some(true), None));
}

#[test]
fn a_pressable_box_is_one_button_over_whatever_it_holds() {
    use ui_core::widgets::{Label, Pressable};
    let mut h = Harness::new();
    let row = Style {
        flex_direction: FlexDirection::Row,
        gap: LayoutSize {
            width: ui_core::style::length(6.0),
            height: ui_core::style::length(0.0),
        },
        padding: ui_core::style::Rect::length(4.0),
        ..Default::default()
    };
    let engine = h.add(
        Pressable::new(row)
            .label("Engine: Docker Desktop")
            .tooltip("Switch engine")
            .shortcut(Shortcut::command('e')),
    );
    let name = h.tree.insert(Some(engine), Label::new("Docker Desktop"));
    let address = h
        .tree
        .insert(Some(engine), Label::new("unix:///var/run/docker.sock"));
    h.prepare();

    for part in [name, address] {
        let pos = h.center(part);
        assert_eq!(h.tree.hit_test(pos), Some(engine));
        h.click_at(pos);
        assert_eq!(
            only::<ButtonClicked>(&h.messages()),
            [(engine, ButtonClicked)]
        );
    }
    let inside = h.center(name);
    let button = PointerButton::Primary;
    h.tree.dispatch(
        Event::PointerDown {
            pos: inside,
            button,
        },
        &mut h.text,
    );
    h.tree.dispatch(
        Event::PointerUp {
            pos: Point::new(490.0, 390.0),
            button,
        },
        &mut h.text,
    );
    assert!(only::<ButtonClicked>(&h.messages()).is_empty());

    h.tree.set_focus(Some(engine));
    h.key(Key::Space);
    assert_eq!(only::<ButtonClicked>(&h.messages()).len(), 1);
    h.tree.set_focus(None);
    h.key_with(
        Key::Char('e'),
        Modifiers {
            command: true,
            meta: true,
            ..Default::default()
        },
    );
    assert_eq!(only::<ButtonClicked>(&h.messages()).len(), 1);

    assert_eq!(
        h.tree.get::<Pressable>(engine).unwrap().label_text(),
        "Engine: Docker Desktop"
    );
    let update = h.tree.accessibility_update(1.0);
    let node = &(update.nodes.iter())
        .find(|(n, _)| *n == ui_core::access_id(engine))
        .unwrap()
        .1;
    assert_eq!(node.label(), Some("Engine: Docker Desktop"));
    assert_eq!(node.role(), accesskit::Role::Button);
    h.tree
        .update::<Pressable, _>(engine, |p, cx| p.set_selected(cx, true));
    h.prepare();
    let bounds = h.tree.window_rect(engine).unwrap();
    let theme = h.tree.theme().clone();
    assert!(h.tree.display_list().commands().iter().any(|c| matches!(
        c,
        DrawCommand::Quad { rect, color, border, .. }
            if *rect == bounds && *color == theme.selection && border.color == theme.accent
    )));
}

#[test]
fn underlined_segments_are_tabs_with_a_line_under_the_selected_one() {
    use ui_core::widgets::{SEGMENT_UNDERLINE, SegmentStyle};
    let mut h = Harness::new();
    let tabs = h.add(
        SegmentedControl::new(["Logs", "Inspect", "Stats"], SegmentMode::SelectOne)
            .style(SegmentStyle::Underline),
    );
    h.prepare();
    let theme = h.tree.theme().clone();
    let bounds = h.tree.window_rect(tabs).unwrap();
    let paint = |h: &Harness| {
        let commands = h.tree.display_list().commands();
        let lines: Vec<Rect> = commands
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Quad { rect, color, .. } if *color == theme.accent => Some(*rect),
                _ => None,
            })
            .collect();
        let inks: Vec<_> = commands
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { color, .. } => Some(*color),
                _ => None,
            })
            .collect();
        (lines, inks)
    };
    let (lines, inks) = paint(&h);
    assert_eq!(lines.len(), 1, "no bezel, one line: {lines:?}");
    assert_eq!(lines[0].height, SEGMENT_UNDERLINE);
    assert_eq!(lines[0].bottom(), bounds.bottom());
    assert_eq!((lines[0].x, inks[0]), (bounds.x, theme.text));
    assert_eq!(&inks[1..], [theme.muted, theme.muted]);

    let first = lines[0];
    let pos = Point::new(first.right() + 20.0, bounds.y + bounds.height / 2.0);
    h.click_at(pos);
    assert_eq!(
        only::<SelectionChanged>(&h.messages()),
        [(tabs, SelectionChanged(1))]
    );
    h.prepare();
    let (lines, inks) = paint(&h);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].x >= first.right());
    assert_eq!(inks, [theme.muted, theme.text, theme.muted]);
}
