use noa_gpui_core::style::{
    AlignItems, Dimension, FlexDirection, JustifyContent, Rect as LayoutRect, Size as LayoutSize,
    auto, length, percent,
};
use noa_gpui_core::widgets::{
    Button, ButtonClicked, Container, Image, Label, ScrollView, TextChanged, TextField,
};
use noa_gpui_core::{Event, Message, Role, Style, UiTree, WidgetId};
use noa_gpui_scene::{Color, Point, Size};
use noa_gpui_text::TextStore;

use crate::assets::Assets;
use crate::bench::{Meter, center_of};
use noa_gpui_platform::{FrameReport, Screen};

const HEADER_BG: Color = Color::hex(0x1e2230);
const ROW_HEIGHT: f32 = 36.0;
const ROW_GAP: f32 = 4.0;

struct Ids {
    status: WidgetId,
    increment: WidgetId,
    counter: WidgetId,
    randomize: WidgetId,
    jump: WidgetId,
    greeting: WidgetId,
    scroll: WidgetId,
    values: Vec<WidgetId>,
}

pub struct UiDemo {
    tree: UiTree,
    ids: Ids,
    clicks: u32,
    randomized: u32,
    meter: Meter,
}

impl UiDemo {
    pub fn new(rows: u32, assets: &Assets) -> Self {
        let mut tree = UiTree::new();
        let root = tree.insert(
            None,
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            })
            .background(Role::Background),
        );

        let header = tree.insert(
            Some(root),
            Container::new(Style {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::CENTER,
                justify_content: JustifyContent::SPACE_BETWEEN,
                padding: LayoutRect {
                    left: length(16.0),
                    right: length(16.0),
                    top: zero(),
                    bottom: zero(),
                },
                size: LayoutSize {
                    width: percent(1.0),
                    height: length(44.0),
                },
                flex_shrink: 0.0,
                ..Default::default()
            })
            .background(HEADER_BG),
        );
        tree.insert(
            Some(header),
            Label::new(format!("noa-gpui · UI tree · {rows} rows"))
                .font_size(17.0)
                .weight(700)
                .color(Color::WHITE),
        );
        let status = tree.insert(
            Some(header),
            Label::new("").font_size(12.0).color(Color::hex(0xb8c0d8)),
        );

        let body = tree.insert(
            Some(root),
            Container::new(Style {
                flex_direction: FlexDirection::Row,
                // `flex: 1 1 0; min-height: 0`: fill the window below the header without
                // sizing from the content (the whole list).
                flex_grow: 1.0,
                flex_basis: length(0.0),
                min_size: LayoutSize {
                    width: auto(),
                    height: length(0.0),
                },
                ..Default::default()
            }),
        );

        let sidebar = tree.insert(
            Some(body),
            Container::new(Style {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::START,
                gap: LayoutSize {
                    width: zero(),
                    height: length(10.0),
                },
                padding: LayoutRect::length(16.0),
                size: LayoutSize {
                    width: length(280.0),
                    height: Dimension::auto(),
                },
                flex_shrink: 0.0,
                ..Default::default()
            })
            .background(Role::Surface)
            .border(1.0, Role::Separator),
        );
        let section = |tree: &mut UiTree, title: &str| {
            tree.insert(
                Some(sidebar),
                Label::new(title)
                    .font_size(12.0)
                    .weight(700)
                    .color(Role::Muted),
            );
        };
        section(&mut tree, "BUTTON · click, Tab + Enter/Space");
        let increment = tree.insert(Some(sidebar), Button::new("Increment"));
        let counter = tree.insert(Some(sidebar), Label::new("Clicked 0 times"));
        section(&mut tree, "LIST");
        let randomize = tree.insert(Some(sidebar), Button::new("Change visible values"));
        let jump = tree.insert(Some(sidebar), Button::new("Jump to the middle"));
        section(&mut tree, "TEXT FIELD");
        tree.insert(Some(sidebar), TextField::new("Type your name", 248.0));
        let greeting = tree.insert(Some(sidebar), Label::new("Hello!").wrap(true));
        tree.insert(
            Some(sidebar),
            Label::new(
                "Scrolling only re-composites cached commands. Editing a row re-lays-out and \
                 repaints that row alone; offscreen rows are culled.",
            )
            .font_size(12.0)
            .color(Role::Muted)
            .wrap(true),
        );

        let scroll = tree.insert(
            Some(body),
            ScrollView::new(Style {
                flex_direction: FlexDirection::Column,
                // `flex: 1 1 0; min-width: 0`: the viewport's width comes from the
                // window, so Taffy never sizes it from the (long) content.
                flex_grow: 1.0,
                flex_basis: length(0.0),
                min_size: LayoutSize {
                    width: length(0.0),
                    height: auto(),
                },
                gap: LayoutSize {
                    width: zero(),
                    height: length(ROW_GAP),
                },
                padding: LayoutRect::length(8.0),
                ..Default::default()
            }),
        );
        let mut values = Vec::with_capacity(rows as usize);
        for i in 0..rows {
            let row = tree.insert(
                Some(scroll),
                Container::new(Style {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::CENTER,
                    gap: LayoutSize {
                        width: length(10.0),
                        height: zero(),
                    },
                    padding: LayoutRect {
                        left: length(8.0),
                        right: length(12.0),
                        top: zero(),
                        bottom: zero(),
                    },
                    size: LayoutSize {
                        width: percent(1.0),
                        height: length(ROW_HEIGHT),
                    },
                    flex_shrink: 0.0,
                    ..Default::default()
                })
                .background(if i % 2 == 0 {
                    Role::Surface
                } else {
                    Role::SurfaceAlt
                })
                .radius(6.0),
            );
            tree.insert(
                Some(row),
                Image::new(assets.icons[(i % 4) as usize], Size::new(22.0, 22.0)),
            );
            tree.insert(Some(row), Label::new(format!("Row {i}")));
            tree.insert(
                Some(row),
                Container::new(Style {
                    flex_grow: 1.0,
                    ..Default::default()
                }),
            );
            values.push(tree.insert(Some(row), Label::new(value_text(i, 0)).color(Role::Muted)));
        }

        Self {
            tree,
            ids: Ids {
                status,
                increment,
                counter,
                randomize,
                jump,
                greeting,
                scroll,
                values,
            },
            clicks: 0,
            randomized: 0,
            meter: Meter::default(),
        }
    }

    pub fn metered(mut self, meter: Meter) -> Self {
        self.meter = meter;
        self
    }

    fn clicked(&mut self, source: WidgetId) {
        if source == self.ids.increment {
            self.clicks += 1;
            let label = format!(
                "Clicked {} time{}",
                self.clicks,
                if self.clicks == 1 { "" } else { "s" }
            );
            self.tree
                .update::<Label, _>(self.ids.counter, |l, cx| l.set_text(cx, label));
        } else if source == self.ids.randomize {
            self.randomized += 1;
            for row in self.visible_rows() {
                let value = value_text(row as u32, self.randomized);
                self.tree
                    .update::<Label, _>(self.ids.values[row], |l, cx| l.set_text(cx, value));
            }
        } else if source == self.ids.jump {
            let middle = self.ids.values.len() as f32 / 2.0 * (ROW_HEIGHT + ROW_GAP);
            self.tree
                .update::<ScrollView, _>(self.ids.scroll, |s, cx| s.set_offset(cx, middle));
        }
    }

    fn visible_rows(&self) -> std::ops::Range<usize> {
        let offset = self
            .tree
            .get::<ScrollView>(self.ids.scroll)
            .map_or(0.0, ScrollView::offset);
        let height = self
            .tree
            .layout(self.ids.scroll)
            .map_or(0.0, |l| l.rect.height);
        let first = (offset / (ROW_HEIGHT + ROW_GAP)) as usize;
        let last = ((offset + height) / (ROW_HEIGHT + ROW_GAP)).ceil() as usize;
        first.min(self.ids.values.len())..last.min(self.ids.values.len())
    }
}

impl Screen for UiDemo {
    fn tree(&self) -> &UiTree {
        &self.tree
    }

    fn tree_mut(&mut self) -> &mut UiTree {
        &mut self.tree
    }

    fn before_frame(&mut self, text: &mut TextStore) {
        if self.meter.bench() {
            self.bench_step(self.meter.frame(), text);
        }
        let status = self.meter.status();
        self.tree
            .update::<Label, _>(self.ids.status, |l, cx| l.set_text(cx, status));
    }

    fn frame_presented(&mut self, report: &FrameReport<'_>) {
        let widgets = self.widget_count();
        self.meter.presented(report, Some(widgets));
    }

    fn continuous(&self) -> bool {
        self.meter.bench()
    }

    fn wants_close(&self) -> bool {
        self.meter.finished()
    }

    fn handle(&mut self, message: Message, _text: &mut TextStore) {
        if message.downcast_ref::<ButtonClicked>().is_some() {
            self.clicked(message.source);
        } else if let Some(TextChanged(name)) = message.downcast_ref::<TextChanged>() {
            let greeting = if name.is_empty() {
                "Hello!".to_owned()
            } else {
                format!("Hello, {name}!")
            };
            self.tree
                .update::<Label, _>(self.ids.greeting, |l, cx| l.set_text(cx, greeting));
        }
    }
}

impl UiDemo {
    fn bench_step(&mut self, frame: u64, text: &mut TextStore) {
        let pos = center_of(&self.tree, self.ids.scroll);
        self.dispatch(
            Event::Wheel {
                pos,
                delta: Point::new(0.0, -7.0),
            },
            text,
        );
        let rows = self.visible_rows();
        if !rows.is_empty() {
            let row = rows.start + (frame as usize % rows.len());
            let value = value_text(row as u32, frame as u32 + 1);
            self.tree
                .update::<Label, _>(self.ids.values[row], |l, cx| l.set_text(cx, value));
        }
    }
}

fn zero() -> noa_gpui_core::style::LengthPercentage {
    length(0.0)
}

fn value_text(row: u32, seed: u32) -> String {
    let v = (row.wrapping_mul(2_654_435_761) ^ seed.wrapping_mul(40_503)) % 10_000;
    format!("{}.{:02}", v / 100, v % 100)
}
