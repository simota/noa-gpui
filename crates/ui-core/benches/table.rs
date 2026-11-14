//! A 100,000-row `TableView`, without a GPU. Measures one frame of input dispatch and
//! `UiTree::prepare` for scrolling, clicks, range and keyboard selection, select all,
//! and column resizing. Sorting and filtering reread every row and are reported once
//! each. Fails when any frame takes longer than 16 ms (one frame at 60 Hz).
//!
//! ```sh
//! cargo bench -p ui-core --bench table
//! BENCH_ROWS=1000000 cargo bench -p ui-core --bench table
//! ```

use std::borrow::Cow;
use std::time::{Duration, Instant};

use ui_core::style::{FlexDirection, Size as LayoutSize, percent};
use ui_core::widgets::{ColumnAlign, ColumnSort, Container, TableColumn, TableData, TableView};
use ui_core::{Event, Key, Modifiers, PointerButton, Style, UiTree, WidgetId};
use ui_scene::{Point, Size};
use ui_text::{FontOptions, TextStore};

const VIEWPORT: Size = Size::new(1280.0, 800.0);
const FRAMES: usize = 300;
const BUDGET: Duration = Duration::from_millis(16);

/// Rows computed on demand, like a database cursor.
struct Rows(usize);

impl TableData for Rows {
    fn row_count(&self) -> usize {
        self.0
    }

    fn cell(&self, row: usize, column: usize) -> Cow<'_, str> {
        Cow::Owned(match column {
            0 => format!("File {row:06}.txt"),
            1 => format!("{}", (row * 7919) % 100_000),
            2 => ["Document", "Image", "Movie", "Folder"][row % 4].to_owned(),
            _ => format!("2026-{:02}-{:02}", row % 12 + 1, row % 28 + 1),
        })
    }
}

fn build(rows: usize, text: &mut TextStore) -> (UiTree, WidgetId) {
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
    let columns = vec![
        TableColumn::new("Name", 360.0).sort(ColumnSort::Text),
        TableColumn::new("Size", 160.0)
            .sort(ColumnSort::Number)
            .align(ColumnAlign::Trailing),
        TableColumn::new("Kind", 200.0).sort(ColumnSort::Text),
        TableColumn::new("Modified", 240.0),
    ];
    let table = tree.insert(
        Some(root),
        TableView::new(columns, Rows(rows)).height(VIEWPORT.height),
    );
    let started = Instant::now();
    tree.prepare(VIEWPORT, text).expect("first frame");
    println!(
        "build: {rows} rows, first frame {:.1} ms ({:?})",
        started.elapsed().as_secs_f64() * 1000.0,
        tree.stats()
    );
    tree.set_focus(Some(table));
    (tree, table)
}

struct Run {
    name: &'static str,
    times: Vec<Duration>,
}

fn run(
    name: &'static str,
    tree: &mut UiTree,
    text: &mut TextStore,
    mut frame: impl FnMut(usize, &mut UiTree, &mut TextStore),
) -> Run {
    let mut times = Vec::with_capacity(FRAMES);
    for i in 0..FRAMES {
        let started = Instant::now();
        frame(i, tree, text);
        let produced = tree.prepare(VIEWPORT, text).is_some();
        times.push(started.elapsed());
        assert!(produced, "{name}: every frame changes something");
    }
    let run = Run { name, times };
    report(&run);
    run
}

fn report(run: &Run) {
    let mut ms: Vec<f64> = run.times.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    ms.sort_by(f64::total_cmp);
    let at = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
    println!(
        "{:<12} p50 {:.3} / p95 {:.3} / p99 {:.3} / max {:.3} ms",
        run.name,
        at(0.50),
        at(0.95),
        at(0.99),
        ms[ms.len() - 1],
    );
}

fn click(tree: &mut UiTree, text: &mut TextStore, pos: Point, modifiers: Modifiers) {
    let button = PointerButton::Primary;
    tree.dispatch(Event::ModifiersChanged(modifiers), text);
    tree.dispatch(Event::PointerDown { pos, button }, text);
    tree.dispatch(Event::PointerUp { pos, button }, text);
    tree.dispatch(Event::ModifiersChanged(Modifiers::default()), text);
}

fn key(tree: &mut UiTree, text: &mut TextStore, key: Key, modifiers: Modifiers) {
    tree.dispatch(Event::KeyDown { key, modifiers }, text);
    tree.dispatch(Event::ModifiersChanged(Modifiers::default()), text);
}

fn main() {
    let rows: usize = std::env::var("BENCH_ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    let mut text = TextStore::new(FontOptions {
        locale: "en-US".into(),
        load_system_fonts: false,
    });
    let (mut tree, table) = build(rows, &mut text);
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let cmd = Modifiers {
        meta: true,
        command: true,
        ..Default::default()
    };
    let on_row = |n: usize| Point::new(200.0, 28.0 + n as f32 * 24.0 + 12.0);
    let mut runs = Vec::new();

    runs.push(run("scroll", &mut tree, &mut text, |_, tree, text| {
        tree.dispatch(
            Event::Wheel {
                pos: on_row(10),
                delta: Point::new(0.0, -37.0),
            },
            text,
        );
    }));
    runs.push(run("click", &mut tree, &mut text, |i, tree, text| {
        click(tree, text, on_row(i % 30), Modifiers::default());
    }));
    runs.push(run("range", &mut tree, &mut text, |i, tree, text| {
        match i % 3 {
            0 => key(tree, text, Key::PageDown, Modifiers::default()),
            1 => click(tree, text, on_row(20), shift),
            _ => click(tree, text, on_row(5), cmd),
        }
    }));
    runs.push(run("keys", &mut tree, &mut text, |i, tree, text| {
        match i % 4 {
            0 | 1 => key(tree, text, Key::ArrowDown, shift),
            2 => key(tree, text, Key::End, Modifiers::default()),
            _ => key(tree, text, Key::Home, Modifiers::default()),
        }
    }));
    runs.push(run("select all", &mut tree, &mut text, |i, tree, text| {
        if i % 2 == 0 {
            key(tree, text, Key::Char('a'), cmd);
        } else {
            click(tree, text, on_row(3), Modifiers::default());
        }
    }));
    let selected = tree.get::<TableView>(table).unwrap().selected_count();
    assert_eq!(selected, 1, "the last frame clicked one row");

    // Sorting rereads every row: reported, but not a per-frame interaction.
    let started = Instant::now();
    click(
        &mut tree,
        &mut text,
        Point::new(400.0, 14.0),
        Modifiers::default(),
    );
    tree.prepare(VIEWPORT, &mut text);
    println!(
        "sort by Size: {:.1} ms (once per header click, not per frame)",
        started.elapsed().as_secs_f64() * 1000.0
    );

    // Filtering also rereads every row, once per change of the search text.
    for query in ["image", "file 0042", ""] {
        let started = Instant::now();
        tree.update::<TableView, _>(table, |t, cx| t.set_filter_text(cx, query));
        tree.prepare(VIEWPORT, &mut text);
        let shown = tree.get::<TableView>(table).unwrap().row_count();
        println!(
            "filter {query:?}: {:.1} ms, {shown} rows shown (once per change of the text)",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }

    // Last, because it moves the other columns' headers.
    let edge = Point::new(tree.get::<TableView>(table).unwrap().column_width(0), 14.0);
    let button = PointerButton::Primary;
    tree.dispatch(Event::PointerDown { pos: edge, button }, &mut text);
    runs.push(run("resize", &mut tree, &mut text, |i, tree, text| {
        let pos = Point::new(44.0 + (i % 30) as f32 * 2.0, 14.0);
        tree.dispatch(Event::PointerMove { pos }, text);
    }));
    tree.dispatch(
        Event::PointerUp {
            pos: Point::new(60.0, 14.0),
            button,
        },
        &mut text,
    );
    let width = tree.get::<TableView>(table).unwrap().column_width(0);
    assert!(width < 200.0, "the drag resized the Name column ({width})");

    let slow: Vec<String> = runs
        .iter()
        .filter_map(|r| {
            let max = *r.times.iter().max().unwrap();
            (max > BUDGET).then(|| format!("{} ({:.2} ms)", r.name, max.as_secs_f64() * 1000.0))
        })
        .collect();
    if slow.is_empty() {
        println!("every frame within {} ms", BUDGET.as_millis());
    } else {
        eprintln!("over {} ms: {}", BUDGET.as_millis(), slow.join(", "));
        std::process::exit(1);
    }
}
