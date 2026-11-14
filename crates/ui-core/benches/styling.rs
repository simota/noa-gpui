//! Resolves a Noa Style sheet for a tree of many widgets, without a GPU. `apply`
//! alternates two sheets so every apply changes styles; `hover` moves the pointer to
//! another row each frame, and descendant selectors make the rows it left and entered
//! resolve their whole subtrees.
//!
//! ```sh
//! cargo bench -p ui-core --bench styling            # 2,000 rows (8,101 widgets)
//! BENCH_ROWS=10000 cargo bench -p ui-core --bench styling
//! ```

use std::time::{Duration, Instant};

use ui_core::style::{FlexDirection, Size as LayoutSize, length, percent};
use ui_core::widgets::{Button, Container, Label};
use ui_core::{Event, Style, StyleMeta, StyleSheet, UiTree};
use ui_scene::{Point, Size};
use ui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(1280.0, 800.0);
const FRAMES: usize = 300;
const APPLIES: usize = 20;
const ROWS_PER_GROUP: usize = 20;
const ROW_HEIGHT: f32 = 20.0;

const SHEET_A: &str = r#"
:root { --accent: #3366cc; }
Container { background-color: #ffffff; }
.group { border: 1px solid #dddddd; }
.row { background-color: #f6f6f6; }
.row:hover { background-color: #e8eefc; }
.row:nth-child(2n) { background-color: #f0f0f0; }
.group .row Label { color: #222222; }
.row > Button { border-radius: 4px; }
.row > Button:hover { background-color: var(--accent); }
Button:active { background-color: #224488; }
Button:focus { border: 2px solid var(--accent); }
Button:disabled { opacity: 0.5; }
Label.title { font-weight: 600; }
Label.value { color: #555555; }
.group:hover .value { color: #000000 !important; }
.row:not(.odd) Label { font-size: 13px; }
#first { background-color: #fff8e0; }
Container > Container > .row { border-radius: 2px; }
.group > .row:first-child { border: 1px solid #cccccc; }
.group > .row:last-child { border: 1px solid #bbbbbb; }
"#;

const SHEET_B: &str = r#"
:root { --accent: #cc3366; }
Container { background-color: #fafafa; }
.group { border: 1px solid #d0d0d0; }
.row { background-color: #f2f2f2; }
.row:hover { background-color: #fce8ee; }
.row:nth-child(2n) { background-color: #ececec; }
.group .row Label { color: #333333; }
.row > Button { border-radius: 6px; }
.row > Button:hover { background-color: var(--accent); }
Button:active { background-color: #882244; }
Button:focus { border: 2px solid var(--accent); }
Button:disabled { opacity: 0.4; }
Label.title { font-weight: 700; }
Label.value { color: #666666; }
.group:hover .value { color: #111111 !important; }
.row:not(.odd) Label { font-size: 12px; }
#first { background-color: #e0fff8; }
Container > Container > .row { border-radius: 3px; }
.group > .row:first-child { border: 1px solid #c0c0c0; }
.group > .row:last-child { border: 1px solid #b0b0b0; }
"#;

fn build(rows: usize) -> UiTree {
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
    let column = || Style {
        flex_direction: FlexDirection::Column,
        flex_shrink: 0.0,
        ..Default::default()
    };
    let mut group = root;
    for i in 0..rows {
        if i % ROWS_PER_GROUP == 0 {
            group = tree.insert(Some(root), Container::new(column()));
            tree.set_style_meta(group, StyleMeta::new().class("group"));
        }
        let row = tree.insert(
            Some(group),
            Container::new(Style {
                flex_direction: FlexDirection::Row,
                flex_shrink: 0.0,
                size: LayoutSize {
                    width: percent(1.0),
                    height: length(ROW_HEIGHT),
                },
                ..Default::default()
            }),
        );
        let mut meta = StyleMeta::new().class("row");
        if i % 2 == 1 {
            meta = meta.class("odd");
        }
        if i == 0 {
            meta = meta.id("first");
        }
        tree.set_style_meta(row, meta);
        let title = tree.insert(Some(row), Label::new(format!("Row {i}")));
        tree.set_style_meta(title, StyleMeta::new().class("title"));
        let value = tree.insert(Some(row), Label::new(format!("{}", i * 7 % 1000)));
        tree.set_style_meta(value, StyleMeta::new().class("value"));
        tree.insert(Some(row), Button::new("Open"));
    }
    tree
}

fn report(name: &str, mut times: Vec<Duration>) {
    times.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    let at = |q: f64| ms(times[((times.len() - 1) as f64 * q).round() as usize]);
    println!(
        "{name:<8} p50 {:.3} / p95 {:.3} / p99 {:.3} / max {:.3} ms",
        at(0.50),
        at(0.95),
        at(0.99),
        ms(times[times.len() - 1]),
    );
}

fn main() {
    let rows: usize = std::env::var("BENCH_ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2_000);
    let mut text = TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: false,
    });
    let mut tree = build(rows);
    let sheets = [
        StyleSheet::parse(SHEET_A).expect("sheet A parses"),
        StyleSheet::parse(SHEET_B).expect("sheet B parses"),
    ];
    tree.set_stylesheet(sheets[0].clone());
    tree.prepare(VIEWPORT, &mut text).expect("first frame");
    println!("{} widgets, {rows} rows", tree.len());

    let mut times = Vec::new();
    for i in 0..APPLIES {
        let sheet = sheets[(i + 1) % 2].clone();
        let started = Instant::now();
        tree.set_stylesheet(sheet);
        times.push(started.elapsed());
        tree.prepare(VIEWPORT, &mut text).expect("restyled frame");
    }
    report("apply", times);

    let visible = ((VIEWPORT.height / ROW_HEIGHT) as usize).min(rows);
    let mut times = Vec::new();
    for i in 0..FRAMES {
        // A label, so that rows and buttons alternate.
        let row = (i * 7) % visible;
        let pos = Point::new(5.0 + (i % 2) as f32 * 200.0, row as f32 * ROW_HEIGHT + 2.0);
        let started = Instant::now();
        tree.dispatch(Event::PointerMove { pos }, &mut text);
        tree.prepare(VIEWPORT, &mut text);
        times.push(started.elapsed());
    }
    report("hover", times);
}
