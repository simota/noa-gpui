use ui_core::{Event, Message, UiTree};
use ui_platform::{FrameReport, Screen};
use ui_scene::{Border, Color, DisplayList, Point, Rect, Size, TextLayoutId};
use ui_text::{TextStore, TextStyle};

use crate::assets::Assets;
use crate::bench::Meter;

const HEADER: f32 = 44.0;
const BACKGROUND: Color = Color::hex(0xf4f5f8);
const HEADER_BG: Color = Color::hex(0x1e2230);
const INK: Color = Color::hex(0x1e2230);
const MUTED: Color = Color::hex(0x5c6375);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneKind {
    Showcase,
    /// A grid of `count` widgets (quad, image and text each), all visible at once.
    Stress {
        count: u32,
    },
    /// The widget tree demo with a list of `count` rows.
    Ui {
        count: u32,
    },
    Catalog,
    MiniOs,
}

/// A text layout that is reshaped only when its string or style changes.
struct Label {
    id: Option<TextLayoutId>,
    text: String,
    style: Option<TextStyle>,
    max_width: Option<f32>,
}

impl Label {
    fn new() -> Self {
        Self {
            id: None,
            text: String::new(),
            style: None,
            max_width: None,
        }
    }

    fn get(
        &mut self,
        store: &mut TextStore,
        text: &str,
        style: TextStyle,
        max_width: Option<f32>,
    ) -> (TextLayoutId, Size) {
        let unchanged =
            self.text == text && self.style == Some(style) && self.max_width == max_width;
        let id = match self.id {
            Some(id) if unchanged => id,
            old => {
                if let Some(old) = old {
                    store.remove(old);
                }
                let id = store.create(text, style, max_width);
                self.id = Some(id);
                self.text = text.to_owned();
                self.style = Some(style);
                self.max_width = max_width;
                id
            }
        };
        (id, store.size(id))
    }
}

pub struct Scene {
    kind: SceneKind,
    assets: Assets,
    /// Empty: a scene draws its display list itself.
    tree: UiTree,
    list: DisplayList,
    meter: Meter,
    title: Label,
    status: Label,
    labels: Vec<Label>,
}

impl Scene {
    pub fn new(kind: SceneKind, assets: Assets, meter: Meter) -> Self {
        Self {
            kind,
            assets,
            tree: UiTree::new(),
            list: DisplayList::new(),
            meter,
            title: Label::new(),
            status: Label::new(),
            labels: Vec::new(),
        }
    }

    /// `size` is in logical pixels. `frame` drives animation; `status` is shown in the
    /// header.
    fn build(
        &mut self,
        list: &mut DisplayList,
        text: &mut TextStore,
        size: Size,
        frame: u64,
        status: &str,
    ) {
        list.clear();
        let title = match self.kind {
            SceneKind::Showcase => "noa-gpui · Phase 1 renderer".to_owned(),
            SceneKind::Stress { count } => format!("noa-gpui · {count} widgets"),
            SceneKind::Ui { .. } | SceneKind::Catalog | SceneKind::MiniOs => {
                unreachable!("widget-tree scenes are built by a Screen")
            }
        };
        self.header(list, text, size, &title, status);
        match self.kind {
            SceneKind::Showcase => self.showcase(list, text, size, frame),
            SceneKind::Stress { count } => self.stress(list, text, size, frame, count),
            SceneKind::Ui { .. } | SceneKind::Catalog | SceneKind::MiniOs => {
                unreachable!("checked above")
            }
        }
    }

    fn header(
        &mut self,
        list: &mut DisplayList,
        text: &mut TextStore,
        size: Size,
        title: &str,
        status: &str,
    ) {
        list.fill_rect(Rect::new(0.0, 0.0, size.width, HEADER), HEADER_BG);
        let (id, s) = self
            .title
            .get(text, title, TextStyle::new(17.0).weight(700), None);
        list.text(
            Rect::new(16.0, (HEADER - s.height) / 2.0, s.width, s.height),
            id,
            Color::WHITE,
        );

        let (id, s) = self.status.get(text, status, TextStyle::new(12.0), None);
        let x = (size.width - s.width - 16.0).max(16.0 + 260.0);
        list.text(
            Rect::new(x, (HEADER - s.height) / 2.0, s.width, s.height),
            id,
            Color::hex(0xb8c0d8),
        );
    }

    fn label(&mut self, index: usize) -> &mut Label {
        while self.labels.len() <= index {
            self.labels.push(Label::new());
        }
        &mut self.labels[index]
    }

    fn showcase(&mut self, list: &mut DisplayList, text: &mut TextStore, size: Size, frame: u64) {
        let mut next = 0;
        let mut put_text = |scene: &mut Self,
                            list: &mut DisplayList,
                            s: &str,
                            at: Point,
                            style: TextStyle,
                            max_width: Option<f32>,
                            color: Color|
         -> Size {
            let (id, sz) = scene.label(next).get(text, s, style, max_width);
            next += 1;
            list.text(Rect::from_origin_size(at, sz), id, color);
            sz
        };
        let heading = TextStyle::new(13.0).weight(700);

        let x0 = 24.0;
        let mut y = HEADER + 20.0;
        put_text(
            self,
            list,
            "QUADS · SDF corners + borders, instanced",
            Point::new(x0, y),
            heading,
            None,
            MUTED,
        );
        y += 24.0;
        let samples: [([f32; 4], u32, f32); 5] = [
            ([0.0; 4], 0x4f7cff, 0.0),
            ([8.0; 4], 0x22b07d, 2.0),
            ([24.0; 4], 0xff8a3d, 4.0),
            ([28.0, 0.0, 28.0, 0.0], 0xa259ff, 0.0),
            ([40.0; 4], 0xffffff, 3.0),
        ];
        for (i, (radius, color, border)) in samples.into_iter().enumerate() {
            list.push(ui_scene::DrawCommand::Quad {
                rect: Rect::new(x0 + i as f32 * 104.0, y, 92.0, 72.0),
                radius,
                color: Color::hex(color),
                border: Border {
                    width: border,
                    color: INK,
                },
            });
        }
        // Translucent overlap shows the blending order.
        list.rounded_rect(
            Rect::new(x0 + 60.0, y + 40.0, 120.0, 44.0),
            12.0,
            Color::hex(0x1e2230).with_alpha(0.55),
            Border::default(),
        );

        y += 108.0;
        put_text(
            self,
            list,
            "IMAGES · textured quads, batched per texture",
            Point::new(x0, y),
            heading,
            None,
            MUTED,
        );
        y += 24.0;
        for (i, icon) in self.assets.icons.into_iter().enumerate() {
            list.image(Rect::new(x0 + i as f32 * 60.0, y, 48.0, 48.0), icon);
        }
        list.image(Rect::new(x0 + 250.0, y, 256.0, 48.0), self.assets.banner);

        y += 76.0;
        put_text(
            self,
            list,
            "TEXT · cosmic-text shaping + glyphon atlas",
            Point::new(x0, y),
            heading,
            None,
            MUTED,
        );
        y += 24.0;
        for (i, px) in [12.0, 16.0, 24.0, 32.0].into_iter().enumerate() {
            let s = put_text(
                self,
                list,
                &format!("{px}px The quick brown fox"),
                Point::new(x0, y),
                TextStyle::new(px).weight(if i == 3 { 700 } else { 400 }),
                None,
                INK,
            );
            y += s.height + 4.0;
        }
        let paragraph = "Wrapped paragraph: the display list stays GPU-independent; text is referenced by layout id and shaped once, then drawn by glyphon inside the same render pass, interleaved with quads and images in paint order.";
        let s = put_text(
            self,
            list,
            paragraph,
            Point::new(x0, y + 4.0),
            TextStyle::new(14.0),
            Some(500.0),
            MUTED,
        );
        y += s.height + 12.0;
        if !cfg!(target_arch = "wasm32") {
            put_text(
                self,
                list,
                "日本語テキスト：システムフォントへのフォールバック",
                Point::new(x0, y),
                TextStyle::new(18.0),
                None,
                INK,
            );
        }

        let panel = Rect::new((size.width - 344.0).max(560.0), HEADER + 44.0, 320.0, 300.0);
        put_text(
            self,
            list,
            "CLIP · nested scissor rects",
            Point::new(panel.x, HEADER + 20.0),
            heading,
            None,
            MUTED,
        );
        list.rounded_rect(
            panel,
            10.0,
            Color::WHITE,
            Border {
                width: 1.0,
                color: Color::hex(0xd5d9e3),
            },
        );
        let inner = Rect::new(
            panel.x + 8.0,
            panel.y + 8.0,
            panel.width - 16.0,
            panel.height - 16.0,
        );
        list.push_clip(inner);
        let scroll = (frame % 240) as f32 * 0.5;
        for row in 0..14 {
            let r = Rect::new(
                inner.x,
                inner.y + row as f32 * 40.0 - scroll,
                inner.width,
                34.0,
            );
            let color = if row % 2 == 0 {
                Color::hex(0xeef1f8)
            } else {
                Color::hex(0xe3e8f5)
            };
            list.rounded_rect(r, 6.0, color, Border::default());
            list.image(
                Rect::new(r.x + 6.0, r.y + 5.0, 24.0, 24.0),
                self.assets.icons[row % 4],
            );
            put_text(
                self,
                list,
                &format!("Row {row}"),
                Point::new(r.x + 40.0, r.y + 8.0),
                TextStyle::new(14.0),
                None,
                INK,
            );
        }
        let nested = Rect::new(inner.x + 150.0, inner.y + 30.0, 120.0, 120.0);
        list.push_clip(nested);
        list.rounded_rect(
            Rect::new(nested.x - 40.0, nested.y - 40.0, 160.0, 160.0),
            80.0,
            Color::hex(0xff5c7a).with_alpha(0.8),
            Border::default(),
        );
        list.pop_clip();
        list.pop_clip();
    }

    fn stress(
        &mut self,
        list: &mut DisplayList,
        text: &mut TextStore,
        size: Size,
        frame: u64,
        count: u32,
    ) {
        let area = Rect::new(
            4.0,
            HEADER + 4.0,
            size.width - 8.0,
            size.height - HEADER - 8.0,
        );
        if area.is_empty() {
            return;
        }
        let mut cols = ((count as f32 * area.width / area.height).sqrt().ceil() as u32).max(1);
        let cell = loop {
            let rows = count.div_ceil(cols);
            let cell = (area.width / cols as f32).min(area.height / rows as f32);
            let wider = cols + 1;
            let rows_w = count.div_ceil(wider);
            if (area.width / wider as f32).min(area.height / rows_w as f32) > cell {
                cols = wider;
            } else {
                break cell;
            }
        };
        let gap = (cell * 0.08).max(1.0);
        // Keep the widest label inside its cell: overlapping neighbours would make the
        // batcher split batches to preserve paint order.
        let digits = (count.max(1) as f32).log10().floor() + 1.0;
        let font = TextStyle::new(
            (cell * 0.3)
                .min((cell - 2.0 * gap) / (0.65 * digits))
                .clamp(3.0, 14.0),
        );
        let radius = cell * 0.18;
        let icon = cell * 0.36;

        for i in 0..count {
            let x = area.x + (i % cols) as f32 * cell;
            let y = area.y + (i / cols) as f32 * cell;
            let rect = Rect::new(x, y, cell - gap, cell - gap);
            let hue = (i as f32 * 7.0 + frame as f32 * 2.0) % 360.0;
            list.rounded_rect(
                rect,
                radius,
                hsl(hue, 0.65, 0.78),
                Border {
                    width: 1.0,
                    color: hsl(hue, 0.5, 0.45),
                },
            );
            list.image(
                Rect::new(x + gap, y + gap, icon, icon),
                self.assets.icons[(i % 4) as usize],
            );
            let (id, s) = self.label(i as usize).get(text, &i.to_string(), font, None);
            list.text(
                Rect::new(
                    x + (rect.width - s.width) / 2.0,
                    y + rect.height - s.height - gap * 0.5,
                    s.width,
                    s.height,
                ),
                id,
                INK,
            );
        }
    }
}

impl Screen for Scene {
    fn tree(&self) -> &UiTree {
        &self.tree
    }

    fn tree_mut(&mut self) -> &mut UiTree {
        &mut self.tree
    }

    fn handle(&mut self, _message: Message, _text: &mut TextStore) {}

    fn dispatch(&mut self, _event: Event, _text: &mut TextStore) {}

    fn needs_update(&self, _viewport: Size) -> bool {
        false
    }

    /// Every frame is drawn again because `frame` animates the scene.
    fn prepare(&mut self, viewport: Size, text: &mut TextStore) -> bool {
        let mut list = std::mem::take(&mut self.list);
        let meter = std::mem::take(&mut self.meter);
        self.build(&mut list, text, viewport, meter.frame(), meter.status());
        self.list = list;
        self.meter = meter;
        true
    }

    fn display_list(&self) -> &DisplayList {
        &self.list
    }

    fn background(&self) -> Color {
        BACKGROUND
    }

    fn frame_presented(&mut self, report: &FrameReport<'_>) {
        self.meter.presented(report, None);
    }

    fn continuous(&self) -> bool {
        self.meter.bench()
    }

    fn wants_close(&self) -> bool {
        self.meter.finished()
    }
}

fn hsl(h: f32, s: f32, l: f32) -> Color {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Color::rgba(r + m, g + m, b + m, 1.0)
}
