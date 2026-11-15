use noa_gpui_core::style::{
    AlignItems, Display, FlexDirection, GridPlacement, JustifyContent, Line, Rect as LayoutRect,
    Size as LayoutSize, auto, length, percent,
};
use noa_gpui_core::widgets::{
    Accordion, Alert, AlertResponse, AlertStyle, BoxResized, Button, ButtonClicked, CheckState,
    Checkbox, CheckboxChanged, Container, DisclosureGroup, GridCell, GridPlacementMode, GridTrack,
    GridView, GroupBox, Image, Label, Material, NewTabRequested, PanesChanged, PopUpButton,
    ResizableBox, ScrollView, ScrollerStyle, Separator, SplitPane, SplitView, StackAlignment,
    StackDistribution, StackView, Switch, TabCloseRequested, TabMoved, TabView, TextArea,
    TextChanged, TextField, VisualEffectView, from_hsv,
};
use noa_gpui_core::{Message, Paint, Role, Style, WidgetId};
use noa_gpui_scene::{Color, Size};

use super::kit::{Kit, gap, row_style, swatch};
use super::{Demo, DemoCx, SectionSpec};

pub const SECTIONS: [SectionSpec; 10] = [
    SectionSpec {
        title: "Container",
        description: "A Flexbox / Grid box with an optional fill, border and corner radius. \
                      Rows can wrap onto more lines, and a box can keep its aspect ratio.",
        build: container,
    },
    SectionSpec {
        title: "Stack view",
        description: "Children in a row or column with fixed spacing, aligned across the axis \
                      and distributed along it. Hiding a child closes its gap.",
        build: stacks,
    },
    SectionSpec {
        title: "Grid view",
        description: "Rows and columns with CSS Grid: fixed, content-sized and flexible \
                      columns, and cells spanning several columns. The flexible column follows \
                      the window width.",
        build: grids,
    },
    SectionSpec {
        title: "Box & separator",
        description: "A titled group box, or a card (every specimen on this page is one), \
                      and horizontal and vertical hairlines.",
        build: boxes,
    },
    SectionSpec {
        title: "Split view",
        description: "Panes with draggable dividers that respect each pane's minimum. \
                      Double-click the sidebar's divider to collapse or restore it. Emits \
                      PanesChanged.",
        build: splits,
    },
    SectionSpec {
        title: "Tab view",
        description: "Pages switched by tabs; only the selected page is laid out, drawn and \
                      exposed. Click a tab, or focus the tabs and use Left / Right. Emits \
                      SelectionChanged. As document tabs, tabs open with +, close (asking \
                      to save changes) and are dragged into a new order.",
        build: tabs,
    },
    SectionSpec {
        title: "Disclosure group",
        description: "A titled section that shows or hides its content; the chevron turns \
                      over 150 ms. Emits Toggled. An accordion stacks several and can keep \
                      only one open.",
        build: disclosure_groups,
    },
    SectionSpec {
        title: "Resizable box",
        description: "A box resized by dragging its edges or the grip in its corner, within a \
                      minimum and maximum size. Emits BoxResized when the drag ends. Inner \
                      windows resize the same way.",
        build: resizable_boxes,
    },
    SectionSpec {
        title: "ScrollView",
        description: "A clipping viewport that scrolls vertically, horizontally or both. \
                      Overlay scroll bars appear while scrolling; legacy ones stay in a track. \
                      Drag a thumb or press a track; past an edge the content rubber-bands, \
                      and a flick keeps gliding. Scrolling re-composites cached commands \
                      without re-layout or repaint.",
        build: scroll_view,
    },
    SectionSpec {
        title: "Visual effect view",
        description: "A translucent material that blurs what is drawn behind it: sidebar, \
                      popover, HUD and title bar looks. Frames with a material are drawn \
                      offscreen and blurred at each material.",
        build: visual_effects,
    },
];

fn container(kit: &mut Kit) -> Box<dyn Demo> {
    let radii = kit.card(".radius(0 | 6 | 16 | 24)");
    let row = kit
        .tree
        .insert(Some(radii), Container::new(row_style(10.0)));
    for radius in [0.0, 6.0, 16.0, 24.0] {
        kit.tree.insert(
            Some(row),
            swatch(48.0, 48.0).background(Role::Accent).radius(radius),
        );
    }
    let borders = kit.card(".border(width, color)");
    let row = kit
        .tree
        .insert(Some(borders), Container::new(row_style(10.0)));
    for (width, color) in [
        (1.0, Role::Outline),
        (2.0, Role::Accent),
        (4.0, Role::FocusRing),
    ] {
        kit.tree.insert(
            Some(row),
            swatch(48.0, 48.0)
                .background(Role::Surface)
                .border(width, color)
                .radius(8.0),
        );
    }
    let fills = kit.card(".background(color.with_alpha(…))");
    let row = kit
        .tree
        .insert(Some(fills), Container::new(row_style(10.0)));
    for alpha in [1.0, 0.6, 0.3, 0.1] {
        kit.tree.insert(
            Some(row),
            swatch(48.0, 48.0)
                .background(Color::hex(0xd6455d).with_alpha(alpha))
                .radius(8.0),
        );
    }
    let flex = kit.card("Flexbox: grow 1 / 2 / 1");
    let row = kit.tree.insert(
        Some(flex),
        Container::new(Style {
            size: LayoutSize {
                width: length(240.0),
                height: length(32.0),
            },
            align_items: AlignItems::STRETCH,
            ..row_style(6.0)
        }),
    );
    for grow in [1.0, 2.0, 1.0] {
        kit.tree.insert(
            Some(row),
            Container::new(Style {
                flex_grow: grow,
                flex_basis: length(0.0),
                ..Default::default()
            })
            .background(Color::hex(0x8fa8f8))
            .radius(4.0),
        );
    }
    let wrap = kit.card("Container::wrapping_row(column_gap, row_gap)");
    let row = kit.tree.insert(
        Some(wrap),
        Container::wrapping_row(6.0, 6.0)
            .border(1.0, Role::Separator)
            .radius(6.0),
    );
    for (i, word) in [
        "Rust", "GPU", "Layout", "Flexbox", "Grid", "Text", "Taffy", "wgpu", "Themes",
    ]
    .into_iter()
    .enumerate()
    {
        let tag = kit.tree.insert(
            Some(row),
            Container::new(Style {
                padding: LayoutRect {
                    left: length(8.0),
                    right: length(8.0),
                    top: length(3.0),
                    bottom: length(3.0),
                },
                ..Default::default()
            })
            .background(from_hsv(i as f32 * 40.0, 0.25, 0.95))
            .radius(10.0),
        );
        kit.tree
            .insert(Some(tag), Label::new(word).color(Color::hex(0x1d1d1f)));
    }
    let ratio = kit.card(".aspect_ratio(16 / 9 | 1 | 3 / 4) with only the width set");
    let row = kit.tree.insert(
        Some(ratio),
        Container::new(Style {
            align_items: AlignItems::END,
            ..row_style(10.0)
        }),
    );
    for (ratio, name) in [(16.0 / 9.0, "16:9"), (1.0, "1:1"), (3.0 / 4.0, "3:4")] {
        let frame = kit.tree.insert(
            Some(row),
            Container::new(Style {
                size: LayoutSize {
                    width: length(72.0),
                    height: auto(),
                },
                justify_content: JustifyContent::CENTER,
                align_items: AlignItems::CENTER,
                ..Default::default()
            })
            .aspect_ratio(ratio)
            .background(Role::Accent)
            .radius(6.0),
        );
        kit.tree
            .insert(Some(frame), Label::new(name).color(Role::OnAccent));
    }
    Box::new(())
}

fn scroll_view(kit: &mut Kit) -> Box<dyn Demo> {
    let list = kit.card("ScrollView with 40 rows, 200 px tall");
    let inner = kit.tree.insert(
        Some(list),
        ScrollView::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 2.0),
            padding: LayoutRect::length(4.0),
            size: LayoutSize {
                width: length(260.0),
                height: length(200.0),
            },
            ..Default::default()
        }),
    );
    for i in 0..40 {
        let row = kit.tree.insert(
            Some(inner),
            Container::new(Style {
                align_items: AlignItems::CENTER,
                padding: LayoutRect {
                    left: length(10.0),
                    right: length(10.0),
                    top: length(6.0),
                    bottom: length(6.0),
                },
                flex_shrink: 0.0,
                ..row_style(8.0)
            })
            .background(if i % 2 == 0 {
                Role::SurfaceAlt
            } else {
                Role::Surface
            })
            .radius(4.0),
        );
        kit.tree.insert(
            Some(row),
            Image::new(kit.assets.icons[i % 4], Size::new(16.0, 16.0)),
        );
        kit.tree
            .insert(Some(row), Label::new(format!("Item {}", i + 1)));
    }
    let both = kit.card(".horizontal(true) · ScrollerStyle::Legacy — 2D content");
    let inner = kit.tree.insert(
        Some(both),
        ScrollView::new(Style {
            size: LayoutSize {
                width: length(260.0),
                height: length(180.0),
            },
            ..Default::default()
        })
        .horizontal(true)
        .scroller_style(ScrollerStyle::Legacy),
    );
    let tiles = kit.tree.insert(
        Some(inner),
        GridView::new((0..8).map(|_| GridTrack::Fixed(60.0)))
            .row_spacing(6.0)
            .column_spacing(6.0),
    );
    for i in 0..48 {
        let hue = (i * 37 % 360) as f32;
        let tile = kit.tree.insert(
            Some(tiles),
            swatch(60.0, 40.0)
                .background(from_hsv(hue, 0.45, 0.9))
                .radius(6.0),
        );
        kit.tree.insert(
            Some(tile),
            Label::new(format!("{}", i + 1)).color(Color::hex(0x1e2230)),
        );
    }
    let stiff = kit.card(".elastic(false).momentum(false)");
    let inner = kit.tree.insert(
        Some(stiff),
        ScrollView::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 4.0),
            size: LayoutSize {
                width: length(260.0),
                height: length(120.0),
            },
            ..Default::default()
        })
        .elastic(false)
        .momentum(false),
    );
    for i in 0..20 {
        kit.tree.insert(
            Some(inner),
            Label::new(format!("No bounce, no glide · {}", i + 1)),
        );
    }
    Box::new(())
}

fn stacks(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("StackView::horizontal(8) · .alignment(Leading | Center | Trailing)");
    for alignment in [
        StackAlignment::Leading,
        StackAlignment::Center,
        StackAlignment::Trailing,
    ] {
        let stack = kit.tree.insert(
            Some(card),
            StackView::horizontal(8.0)
                .alignment(alignment)
                .edge_insets(4.0),
        );
        for h in [16.0, 32.0, 24.0] {
            kit.tree.insert(
                Some(stack),
                swatch(40.0, h).background(Role::Accent).radius(4.0),
            );
        }
    }
    let card = kit.card(".distribution(FillEqually | EqualSpacing | EqualCentering)");
    for distribution in [
        StackDistribution::FillEqually,
        StackDistribution::EqualSpacing,
        StackDistribution::EqualCentering,
    ] {
        let stack = kit.tree.insert(
            Some(card),
            StackView::horizontal(6.0)
                .distribution(distribution)
                .alignment(StackAlignment::Fill)
                .fill_parent(true),
        );
        for _ in 0..3 {
            // Equal shares need children without a width of their own.
            let child = if distribution == StackDistribution::FillEqually {
                Container::new(Style {
                    size: LayoutSize {
                        width: noa_gpui_core::style::auto(),
                        height: length(20.0),
                    },
                    ..Default::default()
                })
            } else {
                swatch(36.0, 20.0)
            };
            kit.tree
                .insert(Some(stack), child.background(Role::Accent).radius(4.0));
        }
    }
    let card = kit.card("UiTree::set_hidden — the others close the gap");
    let stack = kit.tree.insert(Some(card), StackView::horizontal(8.0));
    let items: Vec<WidgetId> = ["A", "B", "C", "D"]
        .into_iter()
        .map(|name| {
            // Centered like the aspect-ratio frames above, not pinned to a corner.
            let item = kit.tree.insert(
                Some(stack),
                Container::new(Style {
                    justify_content: JustifyContent::CENTER,
                    align_items: AlignItems::CENTER,
                    size: LayoutSize {
                        width: length(40.0),
                        height: length(32.0),
                    },
                    flex_shrink: 0.0,
                    ..Default::default()
                })
                .background(Role::Accent)
                .radius(6.0),
            );
            kit.tree
                .insert(Some(item), Label::new(name).color(Role::OnAccent));
            item
        })
        .collect();
    let toggle = kit.tree.insert(Some(card), Button::new("Hide B"));
    Box::new(HideDemo {
        toggle,
        target: items[1],
        hidden: false,
    })
}

struct HideDemo {
    toggle: WidgetId,
    target: WidgetId,
    hidden: bool,
}

impl Demo for HideDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.toggle || message.downcast_ref::<ButtonClicked>().is_none() {
            return false;
        }
        self.hidden = !self.hidden;
        cx.tree.set_hidden(self.target, self.hidden);
        let label = if self.hidden { "Show B" } else { "Hide B" };
        cx.tree
            .update::<Button, _>(self.toggle, |b, cx| b.set_label(cx, label));
        true
    }
}

fn grids(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "GridView::new([Auto, Flexible(1)]) — a settings form",
        460.0,
    );
    let grid = kit.tree.insert(
        Some(card),
        GridView::new([GridTrack::Auto, GridTrack::Flexible(1.0)])
            .column_spacing(12.0)
            .row_spacing(10.0)
            .fill_width(true),
    );
    kit.tree.insert(Some(grid), Label::new("Name"));
    let name = kit
        .tree
        .insert(Some(grid), TextField::new("Full name", 220.0));
    kit.tree.set_accessible_label(name, "Name");
    kit.tree.insert(Some(grid), Label::new("Theme"));
    let theme = kit
        .tree
        .insert(Some(grid), PopUpButton::new(["System", "Light", "Dark"]));
    kit.tree.set_accessible_label(theme, "Theme");
    kit.tree.insert(Some(grid), Label::new("Sync"));
    let sync = kit.tree.insert(Some(grid), Switch::new(true));
    kit.tree.set_accessible_label(sync, "Sync");
    let span = kit.tree.insert(Some(grid), GridCell::new(2, 1));
    kit.tree.insert(
        Some(span),
        Checkbox::new("A cell spanning both columns").checked(true),
    );
    let card = kit.card("Fixed(60) × 3 · x_placement(Center)");
    let grid = kit.tree.insert(
        Some(card),
        GridView::new([
            GridTrack::Fixed(60.0),
            GridTrack::Fixed(60.0),
            GridTrack::Fixed(60.0),
        ])
        .x_placement(GridPlacementMode::Center),
    );
    for size in [20.0, 40.0, 60.0, 30.0, 50.0, 10.0] {
        kit.tree.insert(
            Some(grid),
            swatch(size, 24.0).background(Role::Accent).radius(4.0),
        );
    }
    Box::new(())
}

fn boxes(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("GroupBox::new(title) · Separator::horizontal()");
    let group = kit
        .tree
        .insert(Some(card), GroupBox::new("Network").fill_width());
    kit.tree.insert(Some(group), Label::new("Wi-Fi: Connected"));
    kit.tree.insert(Some(group), Separator::horizontal());
    kit.tree.insert(
        Some(group),
        Label::new("Ethernet: Not connected").color(Role::Muted),
    );
    let card = kit.card("GroupBox::card(title) — GroupBoxStyle::Card");
    let inner = kit
        .tree
        .insert(Some(card), GroupBox::card("Storage").fill_width());
    kit.tree
        .insert(Some(inner), Label::new("82.4 GB of 256 GB used"));
    kit.tree.insert(
        Some(inner),
        Label::new("Last backup: today").color(Role::Muted),
    );
    let card = kit.card("Separator::vertical() between toolbar items");
    let row = kit.tree.insert(
        Some(card),
        Container::new(Style {
            align_items: AlignItems::STRETCH,
            ..row_style(10.0)
        }),
    );
    for (i, name) in ["Bold", "Italic", "Link"].into_iter().enumerate() {
        if i > 0 {
            kit.tree.insert(Some(row), Separator::vertical());
        }
        kit.tree.insert(Some(row), Button::new(name));
    }
    Box::new(())
}

struct SplitDemo {
    split: WidgetId,
    label: WidgetId,
}

impl Demo for SplitDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.split {
            return false;
        }
        let Some(PanesChanged(sizes)) = message.downcast_ref() else {
            return false;
        };
        let text = format!(
            "PanesChanged({})",
            sizes
                .iter()
                .map(|s| format!("{s:.0}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        cx.tree
            .update::<Label, _>(self.label, |l, cx| l.set_text(cx, text));
        true
    }
}

fn pane(kit: &mut Kit, parent: WidgetId, title: &str, role: Role) {
    let pane = kit.tree.insert(
        Some(parent),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            padding: LayoutRect::length(10.0),
            gap: gap(0.0, 4.0),
            ..Default::default()
        })
        .background(role)
        .clip(true),
    );
    kit.tree.insert(Some(pane), Label::new(title).weight(600));
    kit.tree.insert(
        Some(pane),
        Label::new("Drag the dividers.")
            .color(Role::Muted)
            .wrap(true),
    );
}

fn framed(kit: &mut Kit, parent: WidgetId, height: f32) -> WidgetId {
    kit.tree.insert(
        Some(parent),
        Container::new(Style {
            size: LayoutSize {
                width: percent(1.0),
                height: length(height),
            },
            ..Default::default()
        })
        .border(1.0, Role::Separator)
        .clip(true),
    )
}

fn splits(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "SplitView::horizontal() — sidebar fixed(120).min(80).collapsible, content flexible, \
         inspector fixed(110).min(80)",
        560.0,
    );
    let frame = framed(kit, card, 180.0);
    let split = kit.tree.insert(
        Some(frame),
        SplitView::horizontal()
            .pane(SplitPane::fixed(120.0).min(80.0).collapsible(true))
            .pane(SplitPane::flexible().min(120.0))
            .pane(SplitPane::fixed(110.0).min(80.0)),
    );
    kit.tree.set_autosave_name(split, "catalog.split");
    pane(kit, split, "Sidebar", Role::SurfaceAlt);
    pane(kit, split, "Content", Role::Surface);
    pane(kit, split, "Inspector", Role::SurfaceAlt);
    let label = kit
        .tree
        .insert(Some(card), Label::new("PanesChanged(…)").color(Role::Muted));
    let card = kit.card("SplitView::vertical()");
    let frame = framed(kit, card, 180.0);
    let vertical = kit.tree.insert(
        Some(frame),
        SplitView::vertical()
            .pane(SplitPane::fixed(70.0).min(40.0))
            .pane(SplitPane::flexible().min(40.0)),
    );
    kit.tree
        .set_autosave_name(vertical, "catalog.split.vertical");
    pane(kit, vertical, "Top", Role::SurfaceAlt);
    pane(kit, vertical, "Bottom", Role::Surface);
    Box::new(SplitDemo { split, label })
}

fn tabs(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "TabView::new([\"General\", \"Appearance\", \"About\"])",
        460.0,
    );
    let tabs = kit
        .tree
        .insert(Some(card), TabView::new(["General", "Appearance", "About"]));
    kit.tree.set_accessible_label(tabs, "Settings");
    kit.tree.set_autosave_name(tabs, "catalog.tabs");
    let page = |kit: &mut Kit| {
        kit.tree.insert(
            Some(tabs),
            StackView::vertical(8.0).alignment(StackAlignment::Leading),
        )
    };
    let general = page(kit);
    kit.tree
        .insert(Some(general), Checkbox::new("Open at login").checked(true));
    kit.tree
        .insert(Some(general), Checkbox::new("Show in menu bar"));
    let appearance = page(kit);
    let theme = kit.tree.insert(
        Some(appearance),
        PopUpButton::new(["System", "Light", "Dark"]),
    );
    kit.tree.set_accessible_label(theme, "Theme");
    kit.tree.insert(
        Some(appearance),
        Label::new("Accent colors follow the theme."),
    );
    let about = page(kit);
    kit.tree
        .insert(Some(about), Label::new("noa-gpui").weight(600));
    kit.tree.insert(
        Some(about),
        Label::new("A GPU-rendered UI toolkit.").color(Role::Muted),
    );

    let card = kit.wide_card(
        ".documents() · .new_tab_button(true) · TabView::add_tab / close_tab · set_modified",
        460.0,
    );
    let documents = kit.tree.insert(
        Some(card),
        TabView::new(Vec::<String>::new())
            .documents()
            .new_tab_button(true),
    );
    kit.tree.set_accessible_label(documents, "Documents");
    let mut demo = DocumentsDemo {
        tabs: documents,
        status: kit.tree.insert(
            Some(card),
            Label::new("Temporary documents — this demo does not save changes.")
                .color(Role::Muted)
                .wrap(true),
        ),
        opened: 0,
        closing: None,
    };
    for _ in 0..2 {
        demo.open(kit.tree);
    }
    Box::new(demo)
}

/// Documents are not persisted; the save prompt says so.
struct DocumentsDemo {
    tabs: WidgetId,
    status: WidgetId,
    opened: usize,
    /// The alert asking about the changes of the tab being closed, and that tab's page.
    closing: Option<(WidgetId, WidgetId)>,
}

impl DocumentsDemo {
    fn open(&mut self, tree: &mut noa_gpui_core::UiTree) {
        self.opened += 1;
        let title = format!("Untitled {}", self.opened);
        let page = TabView::add_tab(
            tree,
            self.tabs,
            title.clone(),
            TextArea::new("Write something…").min_lines(5),
        );
        if let Some(page) = page {
            tree.set_accessible_label(page, title);
        }
    }

    /// The tab currently showing `page`; tabs can move.
    fn tab_of(&self, tree: &noa_gpui_core::UiTree, page: WidgetId) -> Option<usize> {
        let count = tree.get::<TabView>(self.tabs)?.tab_count();
        (0..count).find(|&i| TabView::page(tree, self.tabs, i) == Some(page))
    }

    fn status(&self, cx: &mut DemoCx, text: String) {
        cx.tree
            .update::<Label, _>(self.status, |l, cx| l.set_text(cx, text));
    }

    fn close(&mut self, cx: &mut DemoCx, tab: usize) {
        let title = cx.tree.get::<TabView>(self.tabs).unwrap().titles()[tab].clone();
        TabView::close_tab(cx.tree, cx.text, self.tabs, tab);
        self.status(cx, format!("closed {title}"));
    }
}

impl Demo for DocumentsDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if let Some((alert, page)) = self.closing
            && message.source == alert
        {
            if let Some(AlertResponse { button, .. }) = message.downcast_ref() {
                self.closing = None;
                if let Some(tab) = self.tab_of(cx.tree, page) {
                    match button {
                        0 => self.close(cx, tab), // Simulate Save (no persistence in this demo)
                        1 => self.close(cx, tab), // Don't Save
                        _ => self.status(cx, "kept open".into()),
                    }
                }
            }
            return true;
        }
        if message.source == self.tabs {
            if message.downcast_ref::<NewTabRequested>().is_some() {
                self.open(cx.tree);
                self.status(cx, format!("opened Untitled {}", self.opened));
            } else if let Some(TabCloseRequested(tab)) = message.downcast_ref() {
                let view = cx.tree.get::<TabView>(self.tabs).unwrap();
                let title = view.titles()[*tab].clone();
                if view.is_modified(*tab) {
                    let page = TabView::page(cx.tree, self.tabs, *tab).unwrap();
                    let alert = Alert::new(format!("Close “{title}”?"))
                    .informative_text(
                        "This demo does not save documents. “Simulate Save” and “Don’t Save” \
                         both close the tab and discard its contents. Choose Cancel to keep editing.",
                    )
                    .style(AlertStyle::Warning)
                    .button("Simulate Save")
                    .button("Don’t Save")
                    .button("Cancel")
                    .open(cx.tree);
                    self.closing = Some((alert, page));
                } else {
                    self.close(cx, *tab);
                }
            } else if let Some(TabMoved { from, to }) = message.downcast_ref() {
                self.status(cx, format!("moved tab {from} to {to}"));
            }
            return true;
        }
        if message.downcast_ref::<TextChanged>().is_some()
            && let Some(tab) = self.tab_of(cx.tree, message.source)
        {
            cx.tree
                .update::<TabView, _>(self.tabs, |t, cx| t.set_modified(cx, tab, true));
            return true;
        }
        false
    }
}

fn disclosure_groups(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("DisclosureGroup::new(title) · .expanded(true)");
    let advanced = kit
        .tree
        .insert(Some(card), DisclosureGroup::new("Advanced"));
    kit.tree
        .set_autosave_name(advanced, "catalog.disclosure.advanced");
    kit.tree.insert(
        Some(advanced),
        Checkbox::new("Use hardware acceleration").checked(true),
    );
    kit.tree
        .insert(Some(advanced), Checkbox::new("Verbose logging"));
    let details = kit
        .tree
        .insert(Some(card), DisclosureGroup::new("Details").expanded(true));
    kit.tree
        .set_autosave_name(details, "catalog.disclosure.details");
    kit.tree.insert(
        Some(details),
        Label::new("Version 0.1.0").color(Role::Muted),
    );
    kit.tree.insert(
        Some(details),
        Label::new("Build 2026-11-12").color(Role::Muted),
    );

    let card = kit.card("Accordion::new(tree, parent, exclusive) · .add(group) · .handle(message)");
    let exclusive = kit.tree.insert(
        Some(card),
        Checkbox::new("One open at a time").checked(true),
    );
    let mut accordion = Accordion::new(kit.tree, card, true);
    for (title, lines, open) in [
        ("General", ["Name: noa-gpui", "Version: 0.1.0"], true),
        (
            "Sharing",
            ["File sharing: Off", "Screen sharing: Off"],
            false,
        ),
        (
            "Privacy",
            ["Location: While using", "Analytics: Off"],
            false,
        ),
    ] {
        let group = accordion.add(kit.tree, DisclosureGroup::new(title).expanded(open));
        for line in lines {
            kit.tree
                .insert(Some(group), Label::new(line).color(Role::Muted));
        }
    }
    Box::new(AccordionDemo {
        accordion,
        exclusive,
    })
}

struct AccordionDemo {
    accordion: Accordion,
    exclusive: WidgetId,
}

impl Demo for AccordionDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.exclusive
            && let Some(&CheckboxChanged(state)) = message.downcast_ref()
        {
            self.accordion
                .set_exclusive(cx.tree, state == CheckState::On);
            return true;
        }
        self.accordion.handle(cx.tree, message)
    }
}

fn resizable_boxes(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "ResizableBox::new(size).min_size(…).max_size(…) — drag the right or bottom edge, or the corner",
        420.0,
    );
    let resizable = kit.tree.insert(
        Some(card),
        ResizableBox::new(Size::new(240.0, 120.0))
            .min_size(Size::new(140.0, 72.0))
            .max_size(Size::new(380.0, 260.0)),
    );
    let inside = kit.tree.insert(
        Some(resizable),
        Container::new(Style {
            flex_direction: FlexDirection::Column,
            gap: gap(0.0, 4.0),
            padding: LayoutRect::length(12.0),
            ..Default::default()
        }),
    );
    kit.tree
        .insert(Some(inside), Label::new("Preview").weight(600));
    kit.tree.insert(
        Some(inside),
        Label::new("The content is laid out again at each size and clipped to the box.")
            .color(Role::Muted)
            .wrap(true),
    );
    let size = kit
        .tree
        .insert(Some(card), Label::new("240 × 120").color(Role::Muted));
    Box::new(ResizableDemo { resizable, size })
}

struct ResizableDemo {
    resizable: WidgetId,
    size: WidgetId,
}

impl Demo for ResizableDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source != self.resizable {
            return false;
        }
        let Some(BoxResized(size)) = message.downcast_ref() else {
            return false;
        };
        let text = format!("BoxResized({:.0} × {:.0})", size.width, size.height);
        cx.tree
            .update::<Label, _>(self.size, |l, cx| l.set_text(cx, text));
        true
    }
}

fn visual_effects(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.wide_card(
        "VisualEffectView::new(style, Material::Sidebar | Popover | Hud | Titlebar)",
        560.0,
    );
    // A busy background so the blur shows: the banner with stripes over it. The
    // layers share the one cell of a grid, so they stack.
    let stage = kit.tree.insert(
        Some(card),
        Container::new(Style {
            display: Display::Grid,
            size: LayoutSize {
                width: length(524.0),
                height: length(150.0),
            },
            ..Default::default()
        })
        .radius(8.0)
        .clip(true),
    );
    let image = kit
        .tree
        .insert(Some(stage), Container::new(layer(Style::default())));
    kit.tree.insert(
        Some(image),
        Image::new(kit.assets.banner, Size::new(524.0, 150.0)),
    );
    let stripes = kit.tree.insert(
        Some(stage),
        Container::new(layer(Style {
            flex_direction: FlexDirection::Row,
            gap: gap(14.0, 0.0),
            ..Default::default()
        })),
    );
    for _ in 0..19 {
        kit.tree.insert(
            Some(stripes),
            swatch(8.0, 150.0).background(Color::WHITE.with_alpha(0.6)),
        );
    }
    let row = kit.tree.insert(
        Some(stage),
        Container::new(layer(Style {
            flex_direction: FlexDirection::Row,
            padding: LayoutRect {
                left: length(12.0),
                right: length(12.0),
                top: length(20.0),
                bottom: length(20.0),
            },
            justify_content: JustifyContent::SPACE_BETWEEN,
            ..Default::default()
        })),
    );
    for (material, name) in [
        (Material::Sidebar, "Sidebar"),
        (Material::Popover, "Popover"),
        (Material::Hud, "HUD"),
        (Material::Titlebar, "Title bar"),
    ] {
        let glass = kit.tree.insert(
            Some(row),
            VisualEffectView::new(
                Style {
                    size: LayoutSize {
                        width: length(115.0),
                        height: percent(1.0),
                    },
                    padding: LayoutRect::length(10.0),
                    ..Default::default()
                },
                material,
            )
            .radius(10.0),
        );
        let ink = if material == Material::Hud {
            Paint::from(Color::WHITE)
        } else {
            Paint::from(Role::Text)
        };
        kit.tree
            .insert(Some(glass), Label::new(name).weight(600).color(ink));
    }
    Box::new(())
}

/// `style` placed in the first cell of a grid, where layers stack.
fn layer(style: Style) -> Style {
    let first = Line {
        start: noa_gpui_core::style::line(1),
        end: GridPlacement::Auto,
    };
    Style {
        grid_row: first.clone(),
        grid_column: first,
        ..style
    }
}
