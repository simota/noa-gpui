//! 20 windows of 500 widgets each on a `Desktop`, without a GPU. Measures one frame of
//! dragging a window, resizing one, and bringing the back window to the front. Fails
//! when any frame takes longer than 16 ms (one frame at 60 Hz).
//!
//! ```sh
//! cargo bench -p ui-core --bench desktop
//! BENCH_WINDOWS=50 BENCH_WIDGETS=1000 cargo bench -p ui-core --bench desktop
//! ```

use std::time::{Duration, Instant};

use ui_core::style::{FlexDirection, FlexWrap, Size as LayoutSize, length, percent};
use ui_core::widgets::{Container, Label, WINDOW_TITLE_BAR, WindowManager};
use ui_core::{Event, Key, Modifiers, PointerButton, Style, UiTree, WidgetId};
use ui_scene::{Point, Rect, Size};
use ui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(1600.0, 1000.0);
const FRAMES: usize = 300;
const BUDGET: Duration = Duration::from_millis(16);

fn env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

struct Bench {
    tree: UiTree,
    text: TextStore,
    manager: WindowManager,
    windows: Vec<WidgetId>,
}

impl Bench {
    fn new(windows: usize, widgets: usize) -> Self {
        let mut text = TextStore::new(FontOptions {
            locale: "en-US".into(),
            load_system_fonts: false,
        });
        let mut tree = UiTree::new();
        let root = tree.insert(
            None,
            Container::new(Style {
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            }),
        );
        let mut manager = WindowManager::new(
            &mut tree,
            root,
            Style {
                size: LayoutSize {
                    width: percent(1.0),
                    height: percent(1.0),
                },
                ..Default::default()
            },
        );
        let ids = (0..windows)
            .map(|w| {
                let frame = Rect::new(
                    20.0 + (w % 10) as f32 * 110.0,
                    20.0 + (w / 10) as f32 * 180.0 + (w % 10) as f32 * 12.0,
                    420.0,
                    320.0,
                );
                let window = manager.open(&mut tree, &format!("Window {w}"), Some(frame));
                let content = tree.insert(
                    Some(window),
                    Container::new(Style {
                        flex_direction: FlexDirection::Row,
                        flex_wrap: FlexWrap::Wrap,
                        gap: LayoutSize {
                            width: length(4.0),
                            height: length(2.0),
                        },
                        ..Default::default()
                    }),
                );
                // Each cell is a box with a label: two widgets.
                for i in 0..widgets / 2 {
                    let cell = tree.insert(Some(content), Container::new(Style::default()));
                    tree.insert(Some(cell), Label::new(format!("{w}.{i}")).font_size(11.0));
                }
                window
            })
            .collect();
        tree.prepare(VIEWPORT, &mut text);
        Self {
            tree,
            text,
            manager,
            windows: ids,
        }
    }

    fn dispatch(&mut self, event: Event) {
        self.tree.dispatch(event, &mut self.text);
        for message in self.tree.take_messages() {
            self.manager
                .handle(&mut self.tree, &mut self.text, &message);
        }
    }

    fn frame(&mut self, f: impl FnOnce(&mut Self)) -> Duration {
        let started = Instant::now();
        f(self);
        self.tree.prepare(VIEWPORT, &mut self.text);
        started.elapsed()
    }

    fn rect(&self, window: WidgetId) -> Rect {
        self.tree.window_rect(window).expect("laid out")
    }
}

fn report(name: &str, mut times: Vec<Duration>) -> Duration {
    times.sort();
    let at = |q: f64| times[((times.len() - 1) as f64 * q) as usize].as_secs_f64() * 1e3;
    let max = times[times.len() - 1];
    println!(
        "{name:<10} p50 {:>7.3} ms   p95 {:>7.3} ms   p99 {:>7.3} ms   max {:>7.3} ms",
        at(0.5),
        at(0.95),
        at(0.99),
        max.as_secs_f64() * 1e3,
    );
    max
}

fn main() {
    let (windows, widgets) = (env("BENCH_WINDOWS", 20), env("BENCH_WIDGETS", 500));
    let mut b = Bench::new(windows, widgets);
    println!(
        "{windows} windows x {widgets} widgets ({} widgets in the tree)",
        b.tree.len()
    );
    let button = PointerButton::Primary;
    let top = *b.windows.last().expect("windows");
    let mut worst = Duration::ZERO;

    let r = b.rect(top);
    let grab = Point::new(r.x + r.width / 2.0, r.y + WINDOW_TITLE_BAR / 2.0);
    b.dispatch(Event::PointerDown { pos: grab, button });
    let times = (0..FRAMES)
        .map(|i| {
            let t = i as f32 * 0.05;
            let pos = Point::new(grab.x + t.cos() * 300.0 - 300.0, grab.y + t.sin() * 200.0);
            b.frame(|b| b.dispatch(Event::PointerMove { pos }))
        })
        .collect();
    b.dispatch(Event::PointerUp { pos: grab, button });
    worst = worst.max(report("drag", times));

    let r = b.rect(top);
    let corner = Point::new(r.right() - 2.0, r.bottom() - 2.0);
    b.dispatch(Event::PointerDown {
        pos: corner,
        button,
    });
    let times = (0..FRAMES)
        .map(|i| {
            let t = i as f32 * 0.05;
            let pos = Point::new(corner.x + t.sin() * 150.0, corner.y + t.cos() * 100.0);
            b.frame(|b| b.dispatch(Event::PointerMove { pos }))
        })
        .collect();
    b.dispatch(Event::PointerUp {
        pos: corner,
        button,
    });
    worst = worst.max(report("resize", times));

    let modifiers = Modifiers {
        meta: true,
        command: true,
        ..Default::default()
    };
    let times = (0..FRAMES)
        .map(|_| {
            let back = b.manager.windows()[0];
            let time = b.frame(|b| {
                b.dispatch(Event::KeyDown {
                    key: Key::Char('`'),
                    modifiers,
                })
            });
            assert_eq!(
                b.manager.active(),
                Some(back),
                "the back window came forward"
            );
            time
        })
        .collect();
    worst = worst.max(report("activate", times));

    if worst > BUDGET {
        eprintln!(
            "FAILED: a frame took {:.3} ms (budget {} ms)",
            worst.as_secs_f64() * 1e3,
            BUDGET.as_millis()
        );
        std::process::exit(1);
    }
}
