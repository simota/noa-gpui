use noa_gpui_core::style::{
    FlexDirection, Rect as LayoutRect, Size as LayoutSize, length, percent,
};
use noa_gpui_core::widgets::{
    AnchorNav, AnchorSelected, Button, ButtonClicked, CommandChosen, CommandPalette, Container,
    Kbd, Label, OUTLINE_RAIL_WIDTH, OutlineItem, OutlineView, PaletteCommand, PanesChanged,
    PathControl, PathItem, PathItemClicked, PathSeparator, RecentCommands, ScrollView, SplitPane,
    SplitView,
};
use noa_gpui_core::{Message, Role, Shortcut, Style, UiTree, WidgetId};

use super::icons::Icons;
use super::kit::{DIVIDER, Kit, gap, row_style};
use super::{Demo, DemoCx, SectionSpec};

pub const SECTIONS: [SectionSpec; 4] = [
    SectionSpec {
        title: "Command palette",
        description: "A search over the app's commands, opened with ⌘K or ⇧⌘P (Ctrl+K and \
                      Ctrl+Shift+P elsewhere) — try it anywhere in the catalog to jump to a \
                      section or change the appearance. Typing filters by fuzzy match and \
                      marks the matched letters; Up / Down and Return choose, Escape or a click \
                      outside closes. Shortcuts show as keycaps. The commands chosen lately \
                      come first the next time it opens (RecentCommands).",
        build: palettes,
    },
    SectionSpec {
        title: "Breadcrumbs",
        description: "PathControl for any hierarchy, not only file paths: components with \
                      icons, separated by chevrons or slashes. When the row is too narrow, the \
                      components after the first collapse into “…”, whose menu lists them; \
                      choosing a component here shortens the trail to it.",
        build: breadcrumbs,
    },
    SectionSpec {
        title: "Anchor navigation",
        description: "Links to the sections of a scrolling page that highlight the one being \
                      read as the page scrolls (a scroll spy). The catalog's own sidebar is \
                      one; so is the list beside this small page. Click a link to scroll its \
                      section to the top.",
        build: anchors,
    },
    SectionSpec {
        title: "Navigation rail",
        description: "A split view pane that collapses to a rail of icons instead of hiding, \
                      with an animation, and an outline sidebar that turns into the rail: \
                      headings become separators, badges dots, and titles move to tooltips. \
                      Double-click the divider or use the button.",
        build: rail,
    },
];

fn readout(kit: &mut Kit, parent: WidgetId, text: &str) -> WidgetId {
    kit.tree
        .insert(Some(parent), Label::new(text).color(Role::Muted))
}

fn set_text(tree: &mut UiTree, label: WidgetId, text: String) {
    tree.update::<Label, _>(label, |l, cx| l.set_text(cx, text));
}

const EDITOR_COMMANDS: [(&str, &str, Option<Shortcut>); 8] = [
    ("New File", "File", Some(Shortcut::command('n'))),
    (
        "Open Recent…",
        "File",
        Some(Shortcut::command('o').with_shift()),
    ),
    ("Save All", "File", Some(Shortcut::command('s').with_alt())),
    ("Toggle Sidebar", "View", Some(Shortcut::command('b'))),
    ("Toggle Terminal", "View", None),
    ("Go to Line…", "Go", Some(Shortcut::command('l'))),
    (
        "Go to Symbol in File…",
        "Go",
        Some(Shortcut::command('o').with_shift().with_alt()),
    ),
    ("Rename Symbol", "Refactor", None),
];

struct PaletteDemo {
    open: WidgetId,
    palette: Option<WidgetId>,
    label: WidgetId,
    /// Recently chosen commands, listed first next time.
    recent: RecentCommands,
}

impl Demo for PaletteDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.open && message.downcast_ref::<ButtonClicked>().is_some() {
            let commands = EDITOR_COMMANDS
                .iter()
                .map(|&(title, group, shortcut)| {
                    let command = PaletteCommand::new(title).detail(group);
                    match shortcut {
                        Some(s) => command.shortcut(s),
                        None => command,
                    }
                })
                .collect();
            let palette = CommandPalette::new(commands)
                .recent(&self.recent)
                .placeholder("Search editor commands…")
                .top(cx.content_top + 12.0);
            self.palette = Some(palette.open(cx.tree));
            return true;
        }
        if Some(message.source) == self.palette
            && let Some(&CommandChosen(i)) = message.downcast_ref()
        {
            let (title, group, _) = EDITOR_COMMANDS[i];
            self.recent
                .note_command(&PaletteCommand::new(title).detail(group));
            let text = format!("CommandChosen({i}) — {}", EDITOR_COMMANDS[i].0);
            set_text(cx.tree, self.label, text);
            return true;
        }
        false
    }
}

fn palettes(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("CommandPalette::new(commands).recent(&recent).open(tree)");
    let open = kit
        .tree
        .insert(Some(card), Button::new("Open editor commands"));
    let label = readout(kit, card, "CommandChosen(…)");
    let card = kit.card("COMMAND_PALETTE_SHORTCUTS");
    for (text, shortcut) in [
        ("Open the catalog's palette", Shortcut::command('k')),
        ("…or, as in VS Code", Shortcut::command('p').with_shift()),
    ] {
        let row = kit.tree.insert(Some(card), Container::new(row_style(10.0)));
        kit.tree.insert(
            Some(row),
            Label::new(text).color(Role::Muted).font_size(13.0),
        );
        kit.tree.insert(Some(row), Kbd::shortcut(shortcut));
    }
    Box::new(PaletteDemo {
        open,
        palette: None,
        label,
        recent: RecentCommands::default(),
    })
}

const TRAIL: [&str; 7] = [
    "Home",
    "Projects",
    "noa-gpui",
    "crates",
    "noa-gpui-core",
    "widgets",
    "path_control.rs",
];

fn trail(icons: &Icons, len: usize) -> Vec<PathItem> {
    TRAIL[..len]
        .iter()
        .enumerate()
        .map(|(i, &title)| match i {
            0 => PathItem::new(title).icon(icons.house.clone()),
            _ if i + 1 < TRAIL.len() => PathItem::new(title).icon(icons.folder.clone()),
            _ => PathItem::new(title),
        })
        .collect()
}

struct BreadcrumbDemo {
    icons: Icons,
    paths: Vec<WidgetId>,
    reset: WidgetId,
    label: WidgetId,
}

impl Demo for BreadcrumbDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.reset && message.downcast_ref::<ButtonClicked>().is_some() {
            for &id in &self.paths {
                let items = trail(&self.icons, TRAIL.len());
                cx.tree
                    .update::<PathControl, _>(id, |p, cx| p.set_items(cx, items));
            }
            return true;
        }
        if self.paths.contains(&message.source)
            && let Some(&PathItemClicked(i)) = message.downcast_ref()
        {
            let items = trail(&self.icons, i + 1);
            cx.tree
                .update::<PathControl, _>(message.source, |p, cx| p.set_items(cx, items));
            set_text(cx.tree, self.label, format!("PathItemClicked({i})"));
            return true;
        }
        false
    }
}

fn breadcrumbs(kit: &mut Kit) -> Box<dyn Demo> {
    let icons = Icons::load();
    let card = kit.wide_card(
        "PathControl::new([PathItem::new(\"Home\").icon(house), …]).separator(Slash)",
        560.0,
    );
    let full = kit.tree.insert(
        Some(card),
        PathControl::new(trail(&icons, TRAIL.len())).separator(PathSeparator::Slash),
    );
    kit.tree.set_accessible_label(full, "Breadcrumbs");
    let card = kit.card(".width(280.0) — the middle collapses into “…”");
    let narrow = kit.tree.insert(
        Some(card),
        PathControl::new(trail(&icons, TRAIL.len())).width(280.0),
    );
    kit.tree.set_accessible_label(narrow, "Breadcrumbs, narrow");
    let reset = kit.tree.insert(Some(card), Button::new("Reset trails"));
    let label = readout(kit, card, "PathItemClicked(…)");
    Box::new(BreadcrumbDemo {
        icons,
        paths: vec![full, narrow],
        reset,
        label,
    })
}

const CHAPTERS: [(&str, Role); 4] = [
    ("Overview", Role::SurfaceAlt),
    ("Install", Role::Surface),
    ("Configure", Role::SurfaceAlt),
    ("Deploy", Role::Surface),
];

struct AnchorDemo {
    nav: WidgetId,
}

impl Demo for AnchorDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.nav {
            return false;
        }
        if let Some(&AnchorSelected(i)) = message.downcast_ref() {
            AnchorNav::scroll_to(cx.tree, self.nav, i);
        }
        true
    }

    fn after_input(&mut self, tree: &mut UiTree) {
        AnchorNav::sync(tree, self.nav);
    }
}

fn anchors(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "AnchorNav::new(scroll).heading(…).anchor(title, section) · sync · scroll_to",
        460.0,
    );
    let row = kit.tree.insert(
        Some(card),
        Container::new(Style {
            gap: gap(12.0, 0.0),
            size: LayoutSize {
                width: percent(1.0),
                height: length(220.0),
            },
            ..Default::default()
        }),
    );
    let nav_box = kit.tree.insert(
        Some(row),
        Container::new(Style {
            size: LayoutSize {
                width: length(140.0),
                height: percent(1.0),
            },
            flex_shrink: 0.0,
            ..Default::default()
        }),
    );
    let scroll = kit.tree.insert(
        Some(row),
        ScrollView::new(Style {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            padding: LayoutRect::length(10.0),
            gap: gap(0.0, 10.0),
            ..Default::default()
        }),
    );
    let mut nav = AnchorNav::new(scroll).heading("GUIDE").scroll_margin(10.0);
    for (title, role) in CHAPTERS {
        let section = kit.tree.insert(
            Some(scroll),
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                padding: LayoutRect::length(10.0),
                gap: gap(0.0, 6.0),
                size: LayoutSize {
                    width: percent(1.0),
                    height: length(180.0),
                },
                flex_shrink: 0.0,
                ..Default::default()
            })
            .background(role)
            .border(1.0, DIVIDER),
        );
        kit.tree
            .insert(Some(section), Label::new(title).font_size(16.0).weight(700));
        kit.tree.insert(
            Some(section),
            Label::new("Scroll: the link of the section crossing the upper third lights up.")
                .color(Role::Muted)
                .wrap(true),
        );
        nav = nav.anchor(title, section);
    }
    let nav = kit.tree.insert(Some(nav_box), nav);
    kit.tree.set_accessible_label(nav, "Guide");
    Box::new(AnchorDemo { nav })
}

struct RailDemo {
    split: WidgetId,
    toggle: WidgetId,
    label: WidgetId,
}

impl Demo for RailDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.toggle && message.downcast_ref::<ButtonClicked>().is_some() {
            let collapsed = cx
                .tree
                .get::<SplitView>(self.split)
                .is_some_and(|s| s.is_collapsed(0));
            cx.tree.update::<SplitView, _>(self.split, |s, cx| {
                s.set_collapsed(cx, 0, !collapsed);
            });
            return true;
        }
        if message.source == self.split
            && let Some(PanesChanged(sizes)) = message.downcast_ref()
        {
            let sizes: Vec<String> = sizes.iter().map(|s| format!("{s:.0}")).collect();
            set_text(
                cx.tree,
                self.label,
                format!("PanesChanged({})", sizes.join(", ")),
            );
            return true;
        }
        false
    }
}

fn rail(kit: &mut Kit) -> Box<dyn Demo> {
    let icons = Icons::load();
    let card = kit.wide_card(
        "SplitPane::fixed(200.0).rail(OUTLINE_RAIL_WIDTH) + OutlineView::sidebar()",
        560.0,
    );
    let frame = kit.tree.insert(
        Some(card),
        Container::new(Style {
            size: LayoutSize {
                width: percent(1.0),
                height: length(230.0),
            },
            ..Default::default()
        })
        .border(1.0, Role::Separator)
        .clip(true),
    );
    let split = kit.tree.insert(
        Some(frame),
        SplitView::horizontal()
            .pane(SplitPane::fixed(200.0).min(150.0).rail(OUTLINE_RAIL_WIDTH))
            .pane(SplitPane::flexible().min(160.0)),
    );
    let items = vec![
        OutlineItem::new("Favorites").children([
            OutlineItem::new("Home").icon(icons.house.clone()),
            OutlineItem::new("Notifications")
                .icon(icons.bell.clone())
                .badge("3"),
            OutlineItem::new("Search").icon(icons.search.clone()),
        ]),
        OutlineItem::new("Library").children([
            OutlineItem::new("Starred")
                .icon(icons.star.clone())
                .badge("12"),
            OutlineItem::new("Projects").icon(icons.folder.clone()),
            OutlineItem::new("Settings").icon(icons.gear.clone()),
        ]),
    ];
    let sidebar = kit
        .tree
        .insert(Some(split), OutlineView::new(items).sidebar().height(230.0));
    kit.tree.set_accessible_label(sidebar, "Sidebar");
    kit.tree.set_autosave_name(split, "catalog.rail-split");
    kit.tree
        .update::<OutlineView, _>(sidebar, |o, cx| o.select_item(cx, Some(1)));
    let content = kit.tree.insert(
        Some(split),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            padding: LayoutRect::length(12.0),
            gap: gap(0.0, 8.0),
            ..Default::default()
        })
        .background(Role::Surface)
        .clip(true),
    );
    kit.tree
        .insert(Some(content), Label::new("Content").weight(600));
    let toggle = kit
        .tree
        .insert(Some(content), Button::new("Toggle the rail"));
    kit.tree.insert(
        Some(content),
        Label::new("At the rail, rest the pointer on an icon for its title.")
            .color(Role::Muted)
            .wrap(true),
    );
    let label = readout(kit, card, "PanesChanged(…)");
    Box::new(RailDemo {
        split,
        toggle,
        label,
    })
}
