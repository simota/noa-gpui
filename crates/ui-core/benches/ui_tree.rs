//! Measures `UiTree::prepare` per frame for a scrolling list of many rows, without a
//! GPU, with and without one visible row's text changing every frame.
//!
//! ```sh
//! cargo bench -p noa-gpui-core --bench ui_tree            # 10,000 rows
//! BENCH_ROWS=50000 cargo bench -p noa-gpui-core --bench ui_tree
//! ```

use std::time::{Duration, Instant};

use noa_gpui_core::style::{
    AlignItems, FlexDirection, Rect as LayoutRect, Size as LayoutSize, length, percent,
};
use noa_gpui_core::widgets::{Container, Image, Label, ScrollView};
use noa_gpui_core::{Event, Style, UiTree, UpdateStats, WidgetId};
use noa_gpui_scene::{Color, ImageId, Point, Size};
use noa_gpui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(1280.0, 800.0);
const FRAMES: usize = 600;
const ROW_HEIGHT: f32 = 36.0;

fn build(rows: usize, text: &mut TextStore) -> (UiTree, Vec<WidgetId>) {
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
        }),
    );
    let scroll = tree.insert(
        Some(root),
        ScrollView::new(Style {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.0,
            gap: LayoutSize {
                width: length(0.0),
                height: length(4.0),
            },
            padding: LayoutRect::length(8.0),
            ..Default::default()
        }),
    );
    let mut values = Vec::with_capacity(rows);
    for i in 0..rows {
        let row = tree.insert(
            Some(scroll),
            Container::new(Style {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::CENTER,
                gap: LayoutSize {
                    width: length(10.0),
                    height: length(0.0),
                },
                padding: LayoutRect::length(6.0),
                size: LayoutSize {
                    width: percent(1.0),
                    height: length(ROW_HEIGHT),
                },
                flex_shrink: 0.0,
                ..Default::default()
            })
            .background(Color::hex(0xeef1f8))
            .radius(6.0),
        );
        tree.insert(Some(row), Image::new(ImageId(0), Size::new(24.0, 24.0)));
        tree.insert(Some(row), Label::new(format!("Row {i}")));
        values.push(tree.insert(Some(row), Label::new(format!("{}", i * 7 % 1000))));
    }
    let started = Instant::now();
    tree.prepare(VIEWPORT, text).expect("first frame");
    let first = started.elapsed();
    println!(
        "build: {} widgets, first frame {:.1} ms ({:?})",
        tree.len(),
        first.as_secs_f64() * 1000.0,
        tree.stats()
    );
    (tree, values)
}

struct Run {
    times: Vec<Duration>,
    stats: Vec<UpdateStats>,
}

fn run(
    name: &str,
    tree: &mut UiTree,
    text: &mut TextStore,
    mut frame: impl FnMut(usize, &mut UiTree, &mut TextStore),
) {
    let mut run = Run {
        times: Vec::new(),
        stats: Vec::new(),
    };
    for i in 0..FRAMES {
        let started = Instant::now();
        frame(i, tree, text);
        let produced = tree.prepare(VIEWPORT, text).is_some();
        run.times.push(started.elapsed());
        assert!(produced, "every benchmark frame changes something");
        run.stats.push(tree.stats());
    }
    report(name, &run);
}

fn report(name: &str, run: &Run) {
    let mut ms: Vec<f64> = run.times.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    ms.sort_by(f64::total_cmp);
    let at = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
    let avg = |f: fn(&UpdateStats) -> usize| {
        run.stats.iter().map(f).sum::<usize>() as f64 / run.stats.len() as f64
    };
    println!(
        "{name:<14} p50 {:.3} / p95 {:.3} / p99 {:.3} / max {:.3} ms | per frame: layout {:.0}%, measured {:.1}, layouts read {:.1}, repainted {:.1}, commands {:.0}, culled {:.0}",
        at(0.50),
        at(0.95),
        at(0.99),
        ms[ms.len() - 1],
        100.0 * run.stats.iter().filter(|s| s.layout).count() as f64 / run.stats.len() as f64,
        avg(|s| s.measured),
        avg(|s| s.layouts_read),
        avg(|s| s.repainted),
        avg(|s| s.commands),
        avg(|s| s.culled),
    );
}

fn main() {
    let rows: usize = std::env::var("BENCH_ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    let mut text = TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: false,
    });
    let (mut tree, values) = build(rows, &mut text);
    let pos = Point::new(400.0, 400.0);
    let wheel = |tree: &mut UiTree, text: &mut TextStore| {
        tree.dispatch(
            Event::Wheel {
                pos,
                delta: Point::new(0.0, -7.0),
            },
            text,
        );
    };

    run("scroll", &mut tree, &mut text, |_, tree, text| {
        wheel(tree, text)
    });
    run("scroll+update", &mut tree, &mut text, |i, tree, text| {
        wheel(tree, text);
        let offset = FRAMES as f32 * 7.0 + (i + 1) as f32 * 7.0;
        let row = ((offset / (ROW_HEIGHT + 4.0)) as usize + 2).min(values.len() - 1);
        tree.update::<Label, _>(values[row], |label, cx| {
            label.set_text(cx, format!("updated {i}"))
        });
    });
}
