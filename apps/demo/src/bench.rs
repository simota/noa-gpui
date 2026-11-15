use noa_gpui_core::{UiTree, WidgetId};
use noa_gpui_platform::FrameReport;
use noa_gpui_scene::Point;

use crate::app::Config;
use crate::timing::FrameTimer;

#[derive(Default)]
pub struct Meter {
    config: Config,
    timer: FrameTimer,
    frame: u64,
    status: String,
    finished: bool,
}

impl Meter {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn bench(&self) -> bool {
        self.config.bench_frames.is_some()
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    /// The benchmark has rendered all its frames, so the app should quit.
    pub fn finished(&self) -> bool {
        self.finished
    }

    /// `widgets` is the widget count of a tree screen.
    pub fn presented(&mut self, report: &FrameReport<'_>, widgets: Option<usize>) {
        self.timer.record(report.cpu);
        self.frame += 1;
        self.update_status(report, widgets);
        if let Some(n) = self.config.bench_frames
            && self.timer.frames() >= n as usize
            && !self.finished
        {
            self.finished = true;
            self.print_report(report);
        }
    }

    fn update_status(&mut self, report: &FrameReport<'_>, widgets: Option<usize>) {
        if self.frame % 30 != 1 && !self.status.is_empty() {
            return;
        }
        let FrameReport {
            backend,
            viewport,
            stats,
            ..
        } = report;
        let mut status = match widgets {
            Some(widgets) => format!("{backend} · {widgets} widgets"),
            None => format!(
                "{} · {}×{} @{:.1}x · {} quads / {} images / {} texts in {} batches",
                backend,
                viewport.width,
                viewport.height,
                viewport.scale_factor,
                stats.quads,
                stats.images,
                stats.texts,
                stats.batches
            ),
        };
        if self.bench() && self.timer.frames() > 1 {
            let recent = self.timer.recent(60);
            status += &format!(
                " · {:.0} fps · cpu p95 {:.2} ms",
                recent.fps(),
                recent.cpu.p95
            );
        }
        self.status = status;
    }

    fn print_report(&self, report: &FrameReport<'_>) {
        let FrameReport {
            backend,
            viewport,
            stats,
            ..
        } = report;
        let report = format!(
            "bench result\nbackend:        {}\nscene:          {:?}\ntarget:         {}x{}\ndraw items:     {} quads, {} images, {} texts ({} culled)\nbatches:        {}\n{}",
            backend,
            self.config.scene,
            viewport.width,
            viewport.height,
            stats.quads,
            stats.images,
            stats.texts,
            stats.culled,
            stats.batches,
            self.timer.summary()
        );
        #[cfg(target_arch = "wasm32")]
        log::info!("{report}");
        #[cfg(not(target_arch = "wasm32"))]
        println!("{report}");
    }
}

/// Window-space center of `id`, for synthesizing pointer and wheel events.
pub fn center_of(tree: &UiTree, id: WidgetId) -> Point {
    let size = tree.layout(id).map(|l| l.rect).unwrap_or_default();
    let origin = tree.window_origin(id).unwrap_or_default();
    Point::new(origin.x + size.width / 2.0, origin.y + size.height / 2.0)
}
