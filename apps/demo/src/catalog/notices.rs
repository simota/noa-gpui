use noa_gpui_core::style::{
    AlignItems, FlexDirection, Position, Size as LayoutSize, auto, length, percent,
};
use noa_gpui_core::widgets::{
    Badge, Button, ButtonClicked, Container, EmptyState, HoverCard, Image, Kbd, Label,
    NotificationBanner, Skeleton, StatusBar, Switch, Tag, TagRemoved, TagStyle, Toast, ToastAction,
    Toggled, Tone,
};
use noa_gpui_core::{Key, Message, Role, Shortcut, Style, WidgetId, WindowCorner};
use noa_gpui_scene::Size;

use super::kit::{DIVIDER, Kit, gap, row_style};
use super::{Demo, DemoCx, SectionSpec};

pub const SECTIONS: [SectionSpec; 7] = [
    SectionSpec {
        title: "Toast",
        description: "Short notices stacked in a corner of the window that hide themselves \
                      after a few seconds, or stay until closed. One can carry an action such \
                      as Undo. While the pointer rests on a toast it stays; waiting draws no \
                      frames, so the app idles at 0 FPS.",
        build: toasts,
    },
    SectionSpec {
        title: "Notification banner",
        description: "A notice set into the layout, as wide as its container: information, \
                      success, a warning or an error, with an optional action and an × that \
                      removes it. Focused, Return runs the action and Escape closes it.",
        build: banners,
    },
    SectionSpec {
        title: "Badge and tag",
        description: "Badges put a count, short text or a dot on the corner of an icon; counts \
                      above 99 show as 99+, and zero hides the badge. Tags name a status or \
                      category in the color of its tone; removable ones take focus and go away \
                      with their × or Backspace.",
        build: badges,
    },
    SectionSpec {
        title: "Keycaps",
        description: "Keys drawn as keycaps, one per key, for shortcuts in help text and \
                      command palettes. A shortcut is written as in the platform's menus: \
                      glyphs on a Mac, names elsewhere.",
        build: keycaps,
    },
    SectionSpec {
        title: "Skeleton",
        description: "Gray placeholders in the shape of content that is still loading, with a \
                      shimmer passing over them (circles pulse). Like the progress indicators \
                      they draw frames only while animating and on screen.",
        build: skeletons,
    },
    SectionSpec {
        title: "Hover card",
        description: "Rest the pointer on the name: after half a second a card shows below it. \
                      It stays while the pointer is on the name or the card, so its button can \
                      be clicked, and hides shortly after the pointer leaves both, on a click \
                      elsewhere, on scrolling and on Escape.",
        build: hover_card,
    },
    SectionSpec {
        title: "Empty state and status bar",
        description: "What a view shows when it has nothing to show — a title, an explanation \
                      and the action that fills it — and a strip along the bottom of a window \
                      with items at both ends. Both are arrangements of labels, buttons and \
                      containers.",
        build: empty_and_status,
    },
];

struct ToastDemo {
    plain: WidgetId,
    undo: WidgetId,
    persistent: WidgetId,
    top: WidgetId,
    count: u32,
}

impl Demo for ToastDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.downcast_ref::<ToastAction>().is_some() {
            Toast::new("Moved back").show(cx.tree);
            return true;
        }
        if message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        self.count += 1;
        let toast = if message.source == self.plain {
            Toast::new(format!("Message {} sent", self.count))
        } else if message.source == self.undo {
            Toast::new("3 files moved to the Trash").action("Undo")
        } else if message.source == self.persistent {
            Toast::new("Export finished. This one stays until you close it.").persistent()
        } else if message.source == self.top {
            Toast::new("Up here instead").duration(2.5)
        } else {
            return false;
        };
        if message.source == self.top {
            toast.show_at(cx.tree, WindowCorner::TopRight);
        } else {
            toast.show(cx.tree);
        }
        true
    }
}

fn toasts(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Toast::new(…).action(\"Undo\").show(tree)");
    let row = kit.tree.insert(Some(card), Container::new(row_style(8.0)));
    let plain = kit.tree.insert(Some(row), Button::new("Show"));
    let undo = kit.tree.insert(Some(row), Button::new("With Undo"));
    let row = kit.tree.insert(Some(card), Container::new(row_style(8.0)));
    let persistent = kit.tree.insert(Some(row), Button::new("Persistent"));
    let top = kit.tree.insert(Some(row), Button::new("Top right"));
    Box::new(ToastDemo {
        plain,
        undo,
        persistent,
        top,
        count: 0,
    })
}

struct BannerDemo {
    column: WidgetId,
    reset: WidgetId,
}

fn add_banners(tree: &mut noa_gpui_core::UiTree, column: WidgetId) {
    let banners = [
        NotificationBanner::new("A new version is ready to install.").action("Restart"),
        NotificationBanner::new("Your changes were saved.").tone(Tone::Success),
        NotificationBanner::new("Your trial ends in 3 days. Upgrade to keep your projects.")
            .title("Trial ending")
            .tone(Tone::Warning)
            .action("Upgrade"),
        NotificationBanner::new("The connection to the server was lost.")
            .title("Offline")
            .tone(Tone::Critical),
    ];
    for banner in banners {
        tree.insert(Some(column), banner);
    }
}

impl Demo for BannerDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.reset || message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        for child in cx.tree.children(self.column).to_vec() {
            cx.tree.remove(child, cx.text);
        }
        add_banners(cx.tree, self.column);
        true
    }
}

fn banners(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "NotificationBanner::new(…).title(…).tone(Tone::Warning).action(…)",
        560.0,
    );
    let column = kit.tree.insert(
        Some(card),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 8.0),
            size: LayoutSize {
                width: percent(1.0),
                height: auto(),
            },
            ..Default::default()
        }),
    );
    add_banners(kit.tree, column);
    let reset = kit.tree.insert(Some(card), Button::new("Show all again"));
    Box::new(BannerDemo { column, reset })
}

struct BadgeDemo {
    mail: WidgetId,
    more: WidgetId,
    clear: WidgetId,
    count: u32,
    tags: WidgetId,
}

impl Demo for BadgeDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.downcast_ref::<TagRemoved>().is_some()
            && cx.tree.parent(message.source) == Some(self.tags)
        {
            cx.tree.remove(message.source, cx.text);
            return true;
        }
        if message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        if message.source == self.more {
            self.count = if self.count == 0 { 1 } else { self.count * 4 };
        } else if message.source == self.clear {
            self.count = 0;
        } else {
            return false;
        }
        let count = self.count;
        cx.tree
            .update::<Badge, _>(self.mail, |b, cx| b.set_count(cx, count));
        true
    }
}

/// A square standing for an icon, positioned so that badges attach to it.
fn icon(kit: &mut Kit, parent: WidgetId, index: usize) -> WidgetId {
    let frame = kit.tree.insert(
        Some(parent),
        Container::new(Style {
            position: Position::Relative,
            flex_shrink: 0.0,
            ..Default::default()
        }),
    );
    kit.tree.insert(
        Some(frame),
        Image::new(kit.assets.icons[index], Size::new(32.0, 32.0)),
    );
    frame
}

fn badges(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Badge::count(3).attached() · ::dot() · ::text(\"New\")");
    let row = kit.tree.insert(Some(card), Container::new(row_style(22.0)));
    let first = icon(kit, row, 0);
    let mail = kit.tree.insert(Some(first), Badge::count(3).attached());
    let second = icon(kit, row, 1);
    kit.tree
        .insert(Some(second), Badge::dot().tone(Tone::Success).attached());
    let third = icon(kit, row, 2);
    kit.tree.insert(
        Some(third),
        Badge::text("New").tone(Tone::Accent).attached(),
    );
    let row = kit.tree.insert(Some(card), Container::new(row_style(8.0)));
    let more = kit.tree.insert(Some(row), Button::new("More"));
    let clear = kit.tree.insert(Some(row), Button::new("Clear"));

    let card = kit.card("Tag::new(…).tone(…) · .removable(true) · .style(TagStyle::Outlined)");
    let tones = kit.tree.insert(Some(card), Container::new(row_style(6.0)));
    for (text, tone) in [
        ("Draft", Tone::Neutral),
        ("In review", Tone::Accent),
        ("Shipped", Tone::Success),
    ] {
        kit.tree.insert(Some(tones), Tag::new(text).tone(tone));
    }
    let more_tones = kit.tree.insert(Some(card), Container::new(row_style(6.0)));
    kit.tree
        .insert(Some(more_tones), Tag::new("Blocked").tone(Tone::Warning));
    kit.tree
        .insert(Some(more_tones), Tag::new("Failed").tone(Tone::Critical));
    let outlined = kit.tree.insert(Some(card), Container::new(row_style(6.0)));
    kit.tree.insert(
        Some(outlined),
        Tag::new("running")
            .tone(Tone::Success)
            .style(TagStyle::Outlined)
            .font_family("monospace"),
    );
    kit.tree.insert(
        Some(outlined),
        Tag::new("exited (1)")
            .tone(Tone::Critical)
            .style(TagStyle::StrongOutlined)
            .font_family("monospace"),
    );
    let tags = kit.tree.insert(Some(card), Container::new(row_style(6.0)));
    for text in ["rust", "gpu", "ui"] {
        kit.tree.insert(
            Some(tags),
            Tag::new(text).tone(Tone::Accent).removable(true),
        );
    }
    Box::new(BadgeDemo {
        mail,
        more,
        clear,
        count: 3,
        tags,
    })
}

fn keycaps(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Kbd::shortcut(Shortcut::command('p').with_shift())");
    for (label, shortcut) in [
        ("Command palette", Shortcut::command('p').with_shift()),
        ("Save", Shortcut::command('s')),
        ("Close", Shortcut::key(Key::Escape)),
    ] {
        let row = kit.tree.insert(Some(card), Container::new(row_style(10.0)));
        kit.tree.insert(
            Some(row),
            Label::new(label).color(Role::Muted).font_size(13.0),
        );
        kit.tree.insert(Some(row), Kbd::shortcut(shortcut));
    }
    let card = kit.card("Kbd::shortcut_with(…, false) · Kbd::new([…])");
    let row = kit.tree.insert(Some(card), Container::new(row_style(10.0)));
    kit.tree.insert(
        Some(row),
        Kbd::shortcut_with(Shortcut::command('k').with_alt(), false),
    );
    kit.tree.insert(Some(card), Kbd::new(["Fn", "F12"]));
    Box::new(())
}

struct SkeletonDemo {
    switch: WidgetId,
    skeletons: Vec<WidgetId>,
}

impl Demo for SkeletonDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.switch {
            return false;
        }
        let Some(&Toggled(on)) = message.downcast_ref() else {
            return false;
        };
        for &id in &self.skeletons {
            cx.tree.update::<Skeleton, _>(id, |s, cx| {
                if on {
                    s.start_animation(cx)
                } else {
                    s.stop_animation(cx)
                }
            });
        }
        true
    }
}

fn skeletons(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Skeleton::circle(40) · ::line(180) · ::new(w, h)");
    let mut skeletons = Vec::new();
    let row = kit.tree.insert(
        Some(card),
        Container::new(Style {
            align_items: AlignItems::START,
            ..row_style(12.0)
        }),
    );
    skeletons.push(kit.tree.insert(Some(row), Skeleton::circle(40.0)));
    let lines = kit.tree.insert(
        Some(row),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 8.0),
            ..Default::default()
        }),
    );
    for width in [180.0, 140.0, 160.0] {
        skeletons.push(kit.tree.insert(Some(lines), Skeleton::line(width)));
    }
    skeletons.push(kit.tree.insert(Some(card), Skeleton::new(244.0, 72.0)));
    for &id in &skeletons {
        kit.tree
            .update::<Skeleton, _>(id, |s, cx| s.start_animation(cx));
    }
    let row = kit.tree.insert(Some(card), Container::new(row_style(8.0)));
    kit.tree.insert(
        Some(row),
        Label::new("Shimmer").color(Role::Muted).font_size(13.0),
    );
    let switch = kit.tree.insert(Some(row), Switch::new(true));
    kit.tree.set_accessible_label(switch, "Shimmer");
    Box::new(SkeletonDemo { switch, skeletons })
}

struct HoverCardDemo {
    follow: WidgetId,
    following: bool,
}

impl Demo for HoverCardDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.follow || message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        self.following = !self.following;
        let title = if self.following {
            "Following"
        } else {
            "Follow"
        };
        cx.tree
            .update::<Button, _>(self.follow, |b, cx| b.set_label(cx, title));
        true
    }
}

fn hover_card(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("HoverCard::new() — children: the name, then the card");
    let row = kit.tree.insert(Some(card), Container::new(row_style(4.0)));
    kit.tree
        .insert(Some(row), Label::new("Written by").color(Role::Muted));
    let hover = kit.tree.insert(Some(row), HoverCard::new());
    kit.tree.insert(
        Some(hover),
        Label::new("@ada").color(Role::Accent).weight(600),
    );
    let content = kit.tree.insert(
        Some(hover),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 6.0),
            size: LayoutSize {
                width: length(220.0),
                height: auto(),
            },
            ..Default::default()
        }),
    );
    let head = kit
        .tree
        .insert(Some(content), Container::new(row_style(10.0)));
    kit.tree.insert(
        Some(head),
        Image::new(kit.assets.icons[3], Size::new(36.0, 36.0)),
    );
    let names = kit.tree.insert(
        Some(head),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            ..Default::default()
        }),
    );
    kit.tree
        .insert(Some(names), Label::new("Ada Lovelace").weight(600));
    kit.tree.insert(
        Some(names),
        Label::new("@ada").color(Role::Muted).font_size(12.0),
    );
    kit.tree.insert(
        Some(content),
        Label::new("Writes about analytical engines and the programs they run.")
            .wrap(true)
            .font_size(13.0),
    );
    let follow = kit.tree.insert(Some(content), Button::new("Follow"));
    Box::new(HoverCardDemo {
        follow,
        following: false,
    })
}

struct EmptyDemo {
    action: Option<WidgetId>,
}

impl Demo for EmptyDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if Some(message.source) != self.action || message.downcast_ref::<ButtonClicked>().is_none()
        {
            return false;
        }
        Toast::new("A new document would open here").show(cx.tree);
        true
    }
}

fn frame(kit: &mut Kit, parent: WidgetId, height: f32) -> WidgetId {
    kit.tree.insert(
        Some(parent),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            size: LayoutSize {
                width: percent(1.0),
                height: length(height),
            },
            ..Default::default()
        })
        .background(Role::Background)
        .border(1.0, DIVIDER)
        .radius(6.0),
    )
}

fn empty_and_status(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "EmptyState::new(…).message(…).action(…).insert(tree, view)",
        420.0,
    );
    let view = frame(kit, card, 200.0);
    let parts = EmptyState::new("No documents")
        .message("Documents you create or open appear here.")
        .action("New Document")
        .insert(kit.tree, view);

    let card = kit.wide_card("StatusBar::insert(tree, window)", 420.0);
    let window = frame(kit, card, 120.0);
    kit.tree.insert(
        Some(window),
        Container::new(Style {
            flex_grow: 1.0,
            ..Default::default()
        }),
    );
    let bar = StatusBar::insert(kit.tree, window);
    kit.tree.insert(
        Some(bar.leading),
        Label::new("INSERT").font_size(11.0).weight(600),
    );
    kit.tree.insert(
        Some(bar.leading),
        Label::new("main.rs").font_size(11.0).color(Role::Muted),
    );
    kit.tree.insert(
        Some(bar.trailing),
        Label::new("Ln 12, Col 8")
            .font_size(11.0)
            .color(Role::Muted),
    );
    kit.tree
        .insert(Some(bar.trailing), Badge::count(2).tone(Tone::Warning));
    Box::new(EmptyDemo {
        action: parts.action,
    })
}
