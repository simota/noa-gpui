use noa_gpui_core::style::{AlignItems, FlexDirection};
use noa_gpui_core::widgets::{
    Button, ButtonClicked, Container, Image, ImageFrame, ImageScaling, Label, LevelIndicator,
    LevelStyle, ProgressBar, Slider, Spinner, ValueChanged,
};
use noa_gpui_core::{Message, Role, Style, WidgetId};
use noa_gpui_scene::Size;

use super::kit::{Kit, gap, row_style};
use super::{Demo, DemoCx, SectionSpec};

pub const SECTIONS: [SectionSpec; 5] = [
    SectionSpec {
        title: "Progress bar",
        description: "Determinate (the fraction done) or indeterminate (a sweeping highlight \
                      while busy). The busy animation draws frames only while it runs and is \
                      on screen: scrolled away or stopped, the app idles at 0 FPS.",
        build: progress_bars,
    },
    SectionSpec {
        title: "Spinner",
        description: "Indeterminate spokes that turn while busy, or a determinate ring. \
                      Like the bar, it animates only while running and visible.",
        build: spinners,
    },
    SectionSpec {
        title: "Level indicator",
        description: "Continuous or discrete capacity (green, then yellow and red at the \
                      warning and critical levels), a star rating and relevancy. Editable \
                      ones take clicks, drags and arrow keys and emit ValueChanged.",
        build: levels,
    },
    SectionSpec {
        title: "Image view",
        description: "A GPU texture in a fixed box: stretched, fitted, filled (cropped) or \
                      at its own size, with an optional photo, bezel or groove frame. These \
                      images are generated at startup, so the demo ships no image files.",
        build: image,
    },
    SectionSpec {
        title: "Vector icon",
        description: "Icons from SVG path data or small SVG files, as icon sets ship them: \
                      stroked outlines and filled glyphs, drawn sharp at any size in one \
                      color from the theme (dimmed when disabled). Buttons, toolbar items, \
                      sidebar rows and alerts take them as well as images. Each size is \
                      rasterized once and cached on the GPU.",
        build: vector_icons,
    },
];

fn column_style() -> Style {
    Style {
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::CENTER,
        gap: gap(0.0, 6.0),
        ..Default::default()
    }
}

struct ProgressDemo {
    toggle: WidgetId,
    busy: Vec<WidgetId>,
    running: bool,
    slider: WidgetId,
    determinate: Vec<WidgetId>,
}

impl Demo for ProgressDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        if message.source == self.toggle && message.downcast_ref::<ButtonClicked>().is_some() {
            self.running = !self.running;
            let running = self.running;
            for &id in &self.busy {
                cx.tree.update::<ProgressBar, _>(id, |b, cx| {
                    if running {
                        b.start_animation(cx)
                    } else {
                        b.stop_animation(cx)
                    }
                });
                cx.tree.update::<Spinner, _>(id, |s, cx| {
                    if running {
                        s.start_animation(cx)
                    } else {
                        s.stop_animation(cx)
                    }
                });
            }
            let label = if running { "Stop" } else { "Start" };
            cx.tree
                .update::<Button, _>(self.toggle, |b, cx| b.set_label(cx, label));
            return true;
        }
        if message.source == self.slider
            && let Some(&ValueChanged(v)) = message.downcast_ref()
        {
            for &id in &self.determinate {
                cx.tree
                    .update::<ProgressBar, _>(id, |b, cx| b.set_value(cx, v, 100.0));
                cx.tree
                    .update::<Spinner, _>(id, |s, cx| s.set_value(cx, v, 100.0));
            }
            return true;
        }
        false
    }
}

fn progress_controls(
    kit: &mut Kit,
    busy: Vec<WidgetId>,
    determinate: Vec<WidgetId>,
    parent: WidgetId,
) -> Box<dyn Demo> {
    let row = kit.tree.insert(
        Some(parent),
        Container::new(Style {
            align_items: AlignItems::CENTER,
            ..row_style(12.0)
        }),
    );
    let toggle = kit.tree.insert(Some(row), Button::new("Stop"));
    let slider = kit
        .tree
        .insert(Some(row), Slider::new(0.0, 100.0, 40.0).length(140.0));
    kit.tree.set_accessible_label(slider, "Progress");
    for &id in &busy {
        kit.tree
            .update::<ProgressBar, _>(id, |b, cx| b.start_animation(cx));
        kit.tree
            .update::<Spinner, _>(id, |s, cx| s.start_animation(cx));
    }
    Box::new(ProgressDemo {
        toggle,
        busy,
        running: true,
        slider,
        determinate,
    })
}

fn progress_bars(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("ProgressBar::new(240).value(40, 100) · .indeterminate()");
    let determinate = kit
        .tree
        .insert(Some(card), ProgressBar::new(240.0).value(40.0, 100.0));
    let busy = kit
        .tree
        .insert(Some(card), ProgressBar::new(240.0).indeterminate());
    let small = kit.tree.insert(
        Some(card),
        ProgressBar::new(240.0)
            .value(40.0, 100.0)
            .control_size(noa_gpui_core::widgets::ControlSize::Small),
    );
    kit.tree.insert(
        Some(card),
        Label::new("Start / Stop the busy bar; the slider sets the others.")
            .font_size(12.0)
            .color(Role::Muted)
            .wrap(true),
    );
    progress_controls(kit, vec![busy], vec![determinate, small], card)
}

fn spinners(kit: &mut Kit) -> Box<dyn Demo> {
    let card = kit.card("Spinner::new() · .control_size(Small) · .determinate(40, 100)");
    let row = kit.tree.insert(
        Some(card),
        Container::new(Style {
            align_items: AlignItems::CENTER,
            ..row_style(16.0)
        }),
    );
    let regular = kit.tree.insert(Some(row), Spinner::new());
    let small = kit.tree.insert(
        Some(row),
        Spinner::new().control_size(noa_gpui_core::widgets::ControlSize::Small),
    );
    let ring = kit
        .tree
        .insert(Some(row), Spinner::new().determinate(40.0, 100.0));
    progress_controls(kit, vec![regular, small], vec![ring], card)
}

#[derive(Default)]
struct LevelDemo {
    items: Vec<(WidgetId, WidgetId, &'static str)>,
}

impl Demo for LevelDemo {
    fn handle(&mut self, cx: &mut DemoCx, message: &Message) -> bool {
        let Some(&(_, label, name)) = self.items.iter().find(|(s, _, _)| *s == message.source)
        else {
            return false;
        };
        let Some(&ValueChanged(v)) = message.downcast_ref() else {
            return false;
        };
        cx.tree
            .update::<Label, _>(label, |l, cx| l.set_text(cx, format!("{name}: {v:.1}")));
        true
    }
}

fn levels(kit: &mut Kit) -> Box<dyn Demo> {
    let mut demo = LevelDemo::default();
    let mut watch = |kit: &mut Kit, parent: WidgetId, level: LevelIndicator, name: &'static str| {
        let initial = level.value();
        let id = kit.tree.insert(Some(parent), level);
        kit.tree.set_accessible_label(id, name);
        let label = kit.tree.insert(
            Some(parent),
            Label::new(format!("{name}: {initial:.1}")).color(Role::Muted),
        );
        demo.items.push((id, label, name));
    };
    let card = kit.card("ContinuousCapacity · .warning(70).critical(90) — drag it");
    watch(
        kit,
        card,
        LevelIndicator::new(LevelStyle::ContinuousCapacity, 0.0, 100.0, 60.0)
            .warning(70.0)
            .critical(90.0)
            .width(240.0)
            .editable(true),
        "Disk",
    );
    let card = kit.card("DiscreteCapacity · .warning(3).critical(1) (lower is worse)");
    watch(
        kit,
        card,
        LevelIndicator::new(LevelStyle::DiscreteCapacity, 0.0, 10.0, 7.0)
            .warning(3.0)
            .critical(1.0)
            .width(240.0)
            .editable(true),
        "Battery",
    );
    let card = kit.card("Rating (0–5 stars, editable) · Relevancy");
    watch(
        kit,
        card,
        LevelIndicator::new(LevelStyle::Rating, 0.0, 5.0, 3.0).editable(true),
        "Rating",
    );
    kit.tree.insert(
        Some(card),
        LevelIndicator::new(LevelStyle::Relevancy, 0.0, 1.0, 0.65).width(120.0),
    );
    Box::new(demo)
}

fn image(kit: &mut Kit) -> Box<dyn Demo> {
    let icons = kit.card("Image::new(id, 16 | 24 | 32 | 48)");
    let row = kit
        .tree
        .insert(Some(icons), Container::new(row_style(12.0)));
    for (i, px) in [16.0, 24.0, 32.0, 48.0].into_iter().enumerate() {
        kit.tree.insert(
            Some(row),
            Image::new(kit.assets.icons[i], Size::new(px, px)),
        );
    }
    let set = kit.card("Alpha-blended icons");
    let row = kit.tree.insert(Some(set), Container::new(row_style(12.0)));
    for icon in kit.assets.icons {
        kit.tree
            .insert(Some(row), Image::new(icon, Size::new(32.0, 32.0)));
    }
    let banner = kit.card("256 × 64 texture");
    kit.tree.insert(
        Some(banner),
        Image::new(kit.assets.banner, Size::new(256.0, 64.0)),
    );
    let modes = kit.wide_card(
        "256 × 64 picture in 96 × 72 boxes: .scaling(Stretch | Fit | Fill | None)",
        460.0,
    );
    let row = kit
        .tree
        .insert(Some(modes), Container::new(row_style(12.0)));
    for scaling in [
        ImageScaling::Stretch,
        ImageScaling::Fit,
        ImageScaling::Fill,
        ImageScaling::None,
    ] {
        let column = kit.tree.insert(Some(row), Container::new(column_style()));
        kit.tree.insert(
            Some(column),
            Image::new(kit.assets.banner, Size::new(96.0, 72.0))
                .natural_size(Size::new(256.0, 64.0))
                .scaling(scaling)
                .frame(ImageFrame::Bezel),
        );
        kit.tree.insert(
            Some(column),
            Label::new(format!("{scaling:?}"))
                .font_size(12.0)
                .color(Role::Muted),
        );
    }
    let down = kit.card("A 32 × 32 icon: Fit enlarges it, FitDown does not");
    let row = kit.tree.insert(Some(down), Container::new(row_style(12.0)));
    for scaling in [ImageScaling::Fit, ImageScaling::FitDown] {
        kit.tree.insert(
            Some(row),
            Image::new(kit.assets.icons[2], Size::new(96.0, 96.0))
                .natural_size(Size::new(32.0, 32.0))
                .scaling(scaling)
                .frame(ImageFrame::Bezel),
        );
    }
    let frames = kit.wide_card(".frame(ImageFrame::Photo | Bezel | Groove)", 460.0);
    let row = kit
        .tree
        .insert(Some(frames), Container::new(row_style(16.0)));
    for frame in [ImageFrame::Photo, ImageFrame::Bezel, ImageFrame::Groove] {
        kit.tree.insert(
            Some(row),
            Image::new(kit.assets.icons[3], Size::new(96.0, 96.0))
                .natural_size(Size::new(32.0, 32.0))
                .scaling(ImageScaling::Fill)
                .frame(frame),
        );
    }
    Box::new(())
}

const EMBLEM: &str = r##"<svg viewBox="0 0 24 24">
  <circle cx="12" cy="12" r="9" fill="#30b050"/>
  <path d="M7.5 12.5l3 3 6-6.5" fill="none" stroke="#ffffff" stroke-width="2.2" stroke-linecap="round"/>
  <circle cx="12" cy="12" r="11" fill="none" stroke="currentColor" stroke-width="1.5"/>
</svg>"##;

fn vector_icons(kit: &mut Kit) -> Box<dyn Demo> {
    use super::icons::Icons;
    use noa_gpui_core::widgets::{SvgIcon, Toolbar, ToolbarDisplay, ToolbarItem};

    let icons = Icons::load();
    let card = kit.card("SvgIcon::new(VectorIcon::from_svg(..)?, 24)");
    let row = kit.tree.insert(Some(card), Container::new(row_style(14.0)));
    for icon in [
        &icons.house,
        &icons.bell,
        &icons.search,
        &icons.gear,
        &icons.heart,
        &icons.star,
        &icons.folder,
        &icons.ring,
    ] {
        kit.tree.insert(Some(row), SvgIcon::new(icon.clone(), 24.0));
    }
    let row = kit.tree.insert(
        Some(card),
        Container::new(Style {
            align_items: AlignItems::END,
            ..row_style(14.0)
        }),
    );
    for size in [12.0, 16.0, 24.0, 32.0, 48.0] {
        kit.tree
            .insert(Some(row), SvgIcon::new(icons.star.clone(), size));
    }

    let card = kit.card(".multicolor(true): the SVG's own colors, currentColor in .color(..)");
    let row = kit.tree.insert(Some(card), Container::new(row_style(14.0)));
    let emblem = noa_gpui_scene::VectorIcon::from_svg(EMBLEM).expect("the emblem is valid SVG");
    for size in [24.0, 32.0, 48.0] {
        kit.tree.insert(
            Some(row),
            SvgIcon::new(emblem.clone(), size)
                .multicolor(true)
                .color(Role::Muted),
        );
    }
    kit.tree.insert(Some(row), SvgIcon::new(emblem, 48.0));

    let card = kit.card(".color(Role::Accent) · Button::icon(..) · disabled");
    let row = kit.tree.insert(Some(card), Container::new(row_style(14.0)));
    kit.tree.insert(
        Some(row),
        SvgIcon::new(icons.heart.clone(), 28.0).color(noa_gpui_scene::Color::hex(0xff3b30)),
    );
    kit.tree.insert(
        Some(row),
        SvgIcon::new(icons.star.clone(), 28.0).color(Role::Accent),
    );
    kit.tree.insert(
        Some(row),
        SvgIcon::new(icons.bell.clone(), 28.0).color(Role::Muted),
    );
    let off = kit
        .tree
        .insert(Some(row), SvgIcon::new(icons.gear.clone(), 28.0));
    kit.tree.set_enabled(off, false);
    let row = kit.tree.insert(Some(card), Container::new(row_style(8.0)));
    kit.tree
        .insert(Some(row), Button::new("Favorite").icon(icons.star.clone()));
    let search = kit
        .tree
        .insert(Some(row), Button::icon_only(icons.search.clone()));
    kit.tree.set_accessible_label(search, "Search");

    let card = kit.wide_card("ToolbarItem::button(..).image(vector_icon)", 420.0);
    kit.tree.insert(
        Some(card),
        Toolbar::new(vec![
            ToolbarItem::button("Home").image(icons.house.clone()),
            ToolbarItem::button("Search").image(icons.search.clone()),
            ToolbarItem::flexible_space(),
            ToolbarItem::button("Alerts").image(icons.bell.clone()),
            ToolbarItem::button("Settings").image(icons.gear.clone()),
        ])
        .display(ToolbarDisplay::IconAndLabel),
    );
    Box::new(())
}
