use noa_gpui_core::style::{AlignItems, FlexDirection, Size as LayoutSize, percent};
use noa_gpui_core::tree::{HOVER_CARD_CLOSE_DELAY, HOVER_CARD_DELAY};
use noa_gpui_core::widgets::{
    Badge, BannerAction, BannerClosed, Button, ButtonClicked, Container, EmptyState, HoverCard,
    Kbd, Label, NotificationBanner, SHIMMER_PERIOD, STATUS_BAR_HEIGHT, Skeleton, StatusBar,
    TOAST_DURATION, TOAST_WIDTH, Tag, TagRemoved, Toast, ToastAction, ToastClosed, Tone,
};
use noa_gpui_core::{
    Event, Key, Message, Modifiers, PointerButton, Shortcut, Style, UiTree, WidgetId, access_id,
    accesskit,
};
use noa_gpui_scene::{Point, Size};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(600.0, 400.0);

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
                align_items: AlignItems::START,
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
        };
        h.prepare();
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

    fn frame(&mut self, now: f64) {
        self.tree.animate(now, &mut self.text);
        self.prepare();
    }

    fn dispatch(&mut self, event: Event) {
        self.tree.dispatch(event, &mut self.text);
        self.prepare();
    }

    fn at(&self, id: WidgetId, x: f32, y: f32) -> Point {
        let r = self.tree.window_rect(id).unwrap();
        Point::new(r.x + x, r.y + y)
    }

    fn near_end(&self, id: WidgetId, dx: f32, dy: f32) -> Point {
        let r = self.tree.window_rect(id).unwrap();
        Point::new(r.right() - dx, r.y + dy)
    }

    fn click(&mut self, pos: Point) {
        let button = PointerButton::Primary;
        self.dispatch(Event::PointerDown { pos, button });
        self.dispatch(Event::PointerUp { pos, button });
    }

    fn key(&mut self, key: Key) {
        self.dispatch(Event::KeyDown {
            key,
            modifiers: Modifiers::default(),
        });
    }

    fn messages(&mut self) -> Vec<Message> {
        self.tree.take_messages()
    }

    fn role(&self, id: WidgetId) -> accesskit::Role {
        let update = self.tree.accessibility_update(1.0);
        update
            .nodes
            .iter()
            .find(|(n, _)| *n == access_id(id))
            .expect("in the accessibility tree")
            .1
            .role()
    }
}

fn has<T: 'static>(messages: &[Message], source: WidgetId) -> bool {
    messages
        .iter()
        .any(|m| m.source == source && m.downcast_ref::<T>().is_some())
}

#[test]
fn badges_cap_their_count_and_hide_at_zero() {
    let mut h = Harness::new();
    let badge = h.add(Badge::count(7));
    assert_eq!(
        h.tree.get::<Badge>(badge).unwrap().label().as_deref(),
        Some("7")
    );
    let small = h.tree.window_rect(badge).unwrap();
    assert!(small.width >= small.height, "at least round: {small:?}");

    h.tree
        .update::<Badge, _>(badge, |b, cx| b.set_count(cx, 1234));
    h.prepare();
    assert_eq!(
        h.tree.get::<Badge>(badge).unwrap().label().as_deref(),
        Some("99+")
    );
    assert!(h.tree.window_rect(badge).unwrap().width > small.width);

    h.tree.update::<Badge, _>(badge, |b, cx| b.set_count(cx, 0));
    h.prepare();
    assert!(!h.tree.get::<Badge>(badge).unwrap().is_shown());
    assert_eq!(h.tree.window_rect(badge).unwrap().width, 0.0);

    let dot = h.add(Badge::dot().tone(Tone::Success));
    assert_eq!(h.tree.get::<Badge>(dot).unwrap().label(), None);
    assert_eq!(h.tree.window_rect(dot).unwrap().width, 8.0);
    let capped = h.add(Badge::count(5).max(3));
    assert_eq!(
        h.tree.get::<Badge>(capped).unwrap().label().as_deref(),
        Some("3+")
    );
}

#[test]
fn an_attached_badge_sits_over_its_parents_top_right_corner() {
    let mut h = Harness::new();
    let icon = h.add(Container::new(Style {
        position: noa_gpui_core::style::Position::Relative,
        size: LayoutSize {
            width: noa_gpui_core::style::length(32.0),
            height: noa_gpui_core::style::length(32.0),
        },
        ..Default::default()
    }));
    let badge = h.tree.insert(Some(icon), Badge::count(3).attached());
    h.prepare();
    let icon_rect = h.tree.window_rect(icon).unwrap();
    let badge_rect = h.tree.window_rect(badge).unwrap();
    assert_eq!(badge_rect.right(), icon_rect.right() + 6.0);
    assert_eq!(badge_rect.y, icon_rect.y - 6.0);
}

#[test]
fn removable_tags_emit_removed_from_the_cross_or_the_keyboard() {
    let mut h = Harness::new();
    let plain = h.add(Tag::new("Draft"));
    let tag = h.add(Tag::new("bug").tone(Tone::Critical).removable(true));
    assert!(
        h.tree.window_rect(tag).unwrap().width > h.tree.window_rect(plain).unwrap().width,
        "room for the ×"
    );

    h.click(h.at(tag, 8.0, 10.0));
    assert!(!has::<TagRemoved>(&h.messages(), tag));
    h.click(h.near_end(tag, 10.0, 10.0));
    assert!(has::<TagRemoved>(&h.messages(), tag));

    h.tree.set_focus(Some(tag));
    h.key(Key::Backspace);
    assert!(has::<TagRemoved>(&h.messages(), tag));
    h.tree.set_focus(Some(plain));
    assert_ne!(h.tree.focused(), Some(plain));
}

#[test]
fn outlined_tags_are_framed_badges_outside_a_table() {
    use noa_gpui_core::widgets::TagStyle;
    use noa_gpui_scene::DrawCommand;
    let mut h = Harness::new();
    let pill = h.add(Tag::new("running").tone(Tone::Success));
    let outlined = h.add(
        Tag::new("running")
            .tone(Tone::Success)
            .style(TagStyle::Outlined),
    );
    let strong = h.add(
        Tag::new("exited")
            .tone(Tone::Critical)
            .style(TagStyle::StrongOutlined)
            .font_family("monospace"),
    );
    h.prepare();
    let drawn = |id: WidgetId| {
        let bounds = h.tree.window_rect(id).unwrap();
        let commands = h.tree.display_list().commands();
        let frame = commands
            .iter()
            .find_map(|c| match c {
                DrawCommand::Quad {
                    rect,
                    radius,
                    border,
                    ..
                } if *rect == bounds => Some((radius[0], *border)),
                _ => None,
            })
            .unwrap();
        let (ink, monospaced) = commands
            .iter()
            .find_map(|c| match c {
                DrawCommand::Text {
                    rect,
                    color,
                    layout,
                } if bounds.contains(Point::new(rect.x, rect.y)) => {
                    let line = &h.text.layouts().buffer(*layout).lines[0];
                    let family = line.attrs_list().defaults().family;
                    // "monospace" resolves to a font's name.
                    let named = matches!(family, noa_gpui_text::cosmic_text::Family::Name(_));
                    Some((*color, named))
                }
                _ => None,
            })
            .unwrap();
        (frame, ink, monospaced, bounds.height)
    };
    let ((pill_radius, _), _, _, height) = drawn(pill);
    assert_eq!(pill_radius, height / 2.0);
    let ((radius, soft), soft_ink, soft_mono, _) = drawn(outlined);
    assert_eq!(radius, 4.0, "slightly rounded, not a pill");
    let ((_, frame), ink, mono, _) = drawn(strong);
    assert_eq!(frame.width, 1.0);
    assert_eq!(frame.color, ink, "frame and text in the tone's own color");
    assert!(soft.color.a < frame.color.a && soft_ink != soft.color);
    assert!(mono && !soft_mono);
}

#[test]
fn keycaps_show_one_cap_per_key() {
    let mut h = Harness::new();
    let shortcut = Shortcut::command('p').with_shift();
    let mac = h.add(Kbd::shortcut_with(shortcut, true));
    let pc = h.add(Kbd::shortcut_with(shortcut, false));
    assert_eq!(h.tree.get::<Kbd>(mac).unwrap().keys(), ["⇧", "⌘", "P"]);
    assert_eq!(
        h.tree.get::<Kbd>(pc).unwrap().keys(),
        ["Ctrl", "Shift", "P"]
    );
    assert!(
        h.tree.window_rect(pc).unwrap().width > h.tree.window_rect(mac).unwrap().width,
        "names are wider than glyphs"
    );
    let any = h.add(Kbd::new(["Esc"]));
    assert_eq!(h.tree.window_rect(any).unwrap().height, 20.0);
}

#[test]
fn a_toast_waits_on_a_timer_then_fades_out_and_removes_itself() {
    let mut h = Harness::new();
    h.tree.set_now(1.0);
    let toast = Toast::new("Message sent").show(&mut h.tree);
    h.prepare();
    let stack = h.tree.parent(toast).expect("in a stack");
    assert!(h.tree.overlays().any(|o| o == stack));
    let r = h.tree.window_rect(toast).unwrap();
    assert_eq!(r.width, TOAST_WIDTH);
    assert_eq!(r.right(), VIEWPORT.width - 16.0);
    assert_eq!(r.bottom(), VIEWPORT.height - 16.0);
    assert_eq!(h.role(toast), accesskit::Role::Alert);

    h.frame(1.0);
    h.frame(1.5);
    assert!(!h.tree.wants_animation_frame(), "waiting costs no frames");
    assert_eq!(h.tree.next_timer(), Some(1.0 + TOAST_DURATION));

    h.frame(1.0 + TOAST_DURATION);
    assert!(has::<ToastClosed>(&h.messages(), toast));
    assert!(h.tree.get::<Toast>(toast).unwrap().is_closing());
    for t in 1..=10 {
        h.frame(1.0 + TOAST_DURATION + f64::from(t) * 0.05);
    }
    assert!(!h.tree.contains(toast), "removed after fading out");
    assert!(h.tree.next_timer().is_none());
}

#[test]
fn toasts_stack_and_stay_while_the_pointer_rests_on_them() {
    let mut h = Harness::new();
    h.tree.set_now(0.0);
    let first = Toast::new("One").show(&mut h.tree);
    let second = Toast::new("Two").duration(2.0).show(&mut h.tree);
    h.prepare();
    assert_eq!(h.tree.parent(first), h.tree.parent(second), "one stack");
    let (a, b) = (
        h.tree.window_rect(first).unwrap(),
        h.tree.window_rect(second).unwrap(),
    );
    assert!(b.y > a.bottom(), "the newer one is nearer the corner");
    assert_eq!(h.tree.next_timer(), Some(2.0));

    h.frame(0.5);
    h.dispatch(Event::PointerMove {
        pos: h.at(second, 20.0, 10.0),
    });
    h.frame(2.0);
    assert!(
        !h.tree.get::<Toast>(second).unwrap().is_closing(),
        "kept while hovered"
    );
    h.dispatch(Event::PointerMove {
        pos: Point::new(10.0, 10.0),
    });
    h.frame(3.0);
    assert!(h.tree.get::<Toast>(second).unwrap().is_closing());
}

#[test]
fn a_toasts_action_and_cross_close_it() {
    let mut h = Harness::new();
    let undo = Toast::new("3 files moved")
        .action("Undo")
        .persistent()
        .show(&mut h.tree);
    h.prepare();
    assert!(h.tree.next_timer().is_none(), "persistent: no timer");
    // The action sits before the × at the end.
    let r = h.tree.window_rect(undo).unwrap();
    h.click(Point::new(
        r.right() - 14.0 - 20.0 - 6.0 - 15.0,
        r.y + r.height / 2.0,
    ));
    let messages = h.messages();
    assert!(has::<ToastAction>(&messages, undo));
    assert!(has::<ToastClosed>(&messages, undo));

    let other = Toast::new("Saved").show(&mut h.tree);
    h.prepare();
    let r = h.tree.window_rect(other).unwrap();
    h.click(Point::new(r.right() - 20.0, r.y + r.height / 2.0));
    let messages = h.messages();
    assert!(!has::<ToastAction>(&messages, other));
    assert!(has::<ToastClosed>(&messages, other));
}

#[test]
fn banners_fill_their_container_and_remove_themselves_when_closed() {
    let mut h = Harness::new();
    let column = h.add(Container::new(Style {
        flex_direction: FlexDirection::Column,
        size: LayoutSize {
            width: noa_gpui_core::style::length(400.0),
            height: noa_gpui_core::style::auto(),
        },
        ..Default::default()
    }));
    let banner = h.tree.insert(
        Some(column),
        NotificationBanner::new("Your trial ends in 3 days.")
            .title("Trial")
            .tone(Tone::Warning)
            .action("Upgrade"),
    );
    h.prepare();
    let r = h.tree.window_rect(banner).unwrap();
    assert_eq!(r.width, 400.0);
    assert_eq!(h.role(banner), accesskit::Role::Alert);

    h.tree.set_focus(Some(banner));
    h.key(Key::Enter);
    assert!(has::<BannerAction>(&h.messages(), banner));
    h.key(Key::Escape);
    assert!(has::<BannerClosed>(&h.messages(), banner));
    assert!(!h.tree.contains(banner));
    assert!(h.tree.children(column).is_empty());
}

#[test]
fn a_long_banner_message_wraps_and_the_cross_closes_it() {
    let mut h = Harness::new();
    let column = h.add(Container::new(Style {
        flex_direction: FlexDirection::Column,
        size: LayoutSize {
            width: noa_gpui_core::style::length(300.0),
            height: noa_gpui_core::style::auto(),
        },
        ..Default::default()
    }));
    let short = h
        .tree
        .insert(Some(column), NotificationBanner::new("Offline"));
    let long = h.tree.insert(
        Some(column),
        NotificationBanner::new(
            "The connection to the server was lost. Changes are kept on this device \
             and sent when the connection comes back.",
        ),
    );
    h.prepare();
    assert!(h.tree.window_rect(long).unwrap().height > h.tree.window_rect(short).unwrap().height);
    assert_eq!(h.role(short), accesskit::Role::Status);
    h.click(h.near_end(short, 12.0 + 10.0, 12.0 + 12.0));
    assert!(has::<BannerClosed>(&h.messages(), short));
    assert!(!h.tree.contains(short));
}

#[test]
fn skeletons_shimmer_only_while_started() {
    let mut h = Harness::new();
    let line = h.add(Skeleton::line(160.0));
    let avatar = h.add(Skeleton::circle(32.0));
    assert_eq!(h.tree.window_rect(line).unwrap().width, 160.0);
    assert_eq!(h.tree.window_rect(avatar).unwrap().height, 32.0);
    assert!(!h.tree.wants_animation_frame());
    assert_eq!(h.tree.get::<Skeleton>(line).unwrap().phase(), None);

    h.tree.set_now(2.0);
    h.tree
        .update::<Skeleton, _>(line, |s, cx| s.start_animation(cx));
    h.frame(2.0 + SHIMMER_PERIOD / 4.0);
    let phase = h.tree.get::<Skeleton>(line).unwrap().phase().unwrap();
    assert!((phase - 0.25).abs() < 1e-4, "{phase}");
    assert!(h.tree.wants_animation_frame());
    h.tree
        .update::<Skeleton, _>(line, |s, cx| s.stop_animation(cx));
    h.frame(3.0);
    assert!(!h.tree.wants_animation_frame());

    h.tree.set_reduce_motion(true);
    h.tree.set_now(4.0);
    h.tree
        .update::<Skeleton, _>(line, |s, cx| s.start_animation(cx));
    h.frame(4.0 + SHIMMER_PERIOD / 4.0);
    assert_eq!(h.tree.get::<Skeleton>(line).unwrap().phase(), Some(0.0));
    assert!(!h.tree.wants_animation_frame());
}

fn hover_card(h: &mut Harness) -> (WidgetId, WidgetId, WidgetId) {
    let card = h.add(HoverCard::new());
    let name = h.tree.insert(Some(card), Label::new("@ada"));
    let content = h.tree.insert(
        Some(card),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            ..Default::default()
        }),
    );
    h.tree.insert(Some(content), Label::new("Ada Lovelace"));
    let follow = h.tree.insert(Some(content), Button::new("Follow"));
    h.prepare();
    (card, name, follow)
}

#[test]
fn a_hover_card_shows_after_a_rest_and_its_buttons_work() {
    let mut h = Harness::new();
    let (card, name, follow) = hover_card(&mut h);
    assert!(h.tree.is_hidden(follow), "hidden until hovered");

    h.tree.set_now(1.0);
    h.dispatch(Event::PointerMove {
        pos: h.at(name, 5.0, 5.0),
    });
    assert_eq!(h.tree.next_timer(), Some(1.0 + HOVER_CARD_DELAY));
    h.frame(1.2);
    assert!(h.tree.hover_card_overlay().is_none(), "not yet");
    h.frame(1.0 + HOVER_CARD_DELAY);
    let panel = h.tree.hover_card_overlay().expect("shown");
    assert!(h.tree.contains(follow));
    assert!(!h.tree.is_hidden(follow));
    assert_eq!(h.tree.ancestors(follow).last(), Some(panel));
    let widget = h.tree.window_rect(card).unwrap();
    assert!(h.tree.window_rect(panel).unwrap().y >= widget.bottom());

    let on_button = h.at(follow, 5.0, 5.0);
    h.dispatch(Event::PointerMove { pos: on_button });
    h.frame(2.0 + HOVER_CARD_CLOSE_DELAY);
    assert_eq!(h.tree.hover_card_overlay(), Some(panel));
    h.click(on_button);
    assert!(has::<ButtonClicked>(&h.messages(), follow));
    assert_eq!(h.tree.hover_card_overlay(), Some(panel));

    h.tree.set_now(3.0);
    h.dispatch(Event::PointerMove {
        pos: Point::new(500.0, 350.0),
    });
    h.frame(3.1);
    assert!(h.tree.hover_card_overlay().is_some(), "crossing the gap");
    h.frame(3.0 + HOVER_CARD_CLOSE_DELAY);
    assert!(h.tree.hover_card_overlay().is_none());
    assert_eq!(
        h.tree.parent(follow).and_then(|c| h.tree.parent(c)),
        Some(card)
    );
    assert!(h.tree.is_hidden(follow));
    assert!(!h.tree.contains(panel));
}

#[test]
fn escape_and_presses_elsewhere_hide_a_hover_card() {
    let mut h = Harness::new();
    let (_, name, follow) = hover_card(&mut h);
    let show = |h: &mut Harness, now: f64| {
        h.tree.set_now(now);
        h.dispatch(Event::PointerMove {
            pos: Point::new(590.0, 390.0),
        });
        h.dispatch(Event::PointerMove {
            pos: h.at(name, 5.0, 5.0),
        });
        h.frame(now + HOVER_CARD_DELAY);
        assert!(h.tree.hover_card_overlay().is_some());
    };
    show(&mut h, 1.0);
    h.key(Key::Escape);
    assert!(h.tree.hover_card_overlay().is_none());
    assert!(h.tree.is_hidden(follow));

    show(&mut h, 5.0);
    h.click(Point::new(590.0, 390.0));
    assert!(h.tree.hover_card_overlay().is_none());
    assert!(h.tree.next_timer().is_none());
}

#[test]
fn an_empty_state_builds_a_centered_column_with_its_action() {
    let mut h = Harness::new();
    let parts = EmptyState::new("No documents")
        .message("Documents you create appear here.")
        .action("New Document")
        .insert(&mut h.tree, h.root);
    h.prepare();
    let action = parts.action.expect("button");
    assert!(parts.message.is_some());
    let root = h.tree.window_rect(parts.root).unwrap();
    let title = h.tree.window_rect(parts.title).unwrap();
    let center = root.x + root.width / 2.0;
    assert!(
        (title.x + title.width / 2.0 - center).abs() < 1.0,
        "centered"
    );
    assert!(h.tree.window_rect(action).unwrap().y > title.bottom());
    h.click(h.at(action, 10.0, 10.0));
    assert!(has::<ButtonClicked>(&h.messages(), action));
}

#[test]
fn a_status_bar_spans_the_width_with_items_at_both_ends() {
    let mut h = Harness::new();
    h.add(Container::new(Style {
        flex_grow: 1.0,
        ..Default::default()
    }));
    let parts = StatusBar::insert(&mut h.tree, h.root);
    let mode = h.tree.insert(Some(parts.leading), Label::new("INSERT"));
    let position = h
        .tree
        .insert(Some(parts.trailing), Label::new("Ln 1, Col 1"));
    h.prepare();
    let bar = h.tree.window_rect(parts.bar).unwrap();
    assert_eq!(bar.width, VIEWPORT.width);
    assert_eq!(bar.height, STATUS_BAR_HEIGHT);
    assert_eq!(bar.bottom(), VIEWPORT.height);
    assert!(h.tree.window_rect(mode).unwrap().x < 20.0);
    assert!(h.tree.window_rect(position).unwrap().right() > VIEWPORT.width - 20.0);
    assert_eq!(h.role(parts.bar), accesskit::Role::Status);
}
