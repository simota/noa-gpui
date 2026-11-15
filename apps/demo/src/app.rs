use noa_gpui_platform::{App, AppOptions, Screen, Setup};

use crate::assets::Assets;
use crate::bench::Meter;
use crate::catalog::Catalog;
use crate::mini_os::MiniOs;
use crate::scene::{Scene, SceneKind};
use crate::ui_demo::UiDemo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub scene: SceneKind,
    /// Render this many frames continuously, report timings, then stop.
    pub bench_frames: Option<u32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scene: SceneKind::Ui { count: 1000 },
            bench_frames: None,
        }
    }
}

impl Config {
    /// Parse `key=value` pairs: `scene=ui|catalog|showcase|stress`, `count=N`, `bench=N`.
    pub fn parse<'a>(pairs: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
        let mut config = Config::default();
        let mut scene = "ui";
        let mut count = None;
        for pair in pairs {
            let pair = pair.trim_start_matches('-');
            if pair.is_empty() {
                continue;
            }
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let number = || {
                value
                    .parse::<u32>()
                    .map_err(|_| format!("{key} expects a number, got {value:?}"))
            };
            match key {
                "scene" => match value {
                    "ui" | "catalog" | "minios" | "showcase" | "stress" => scene = value,
                    other => return Err(format!("unknown scene {other:?}")),
                },
                "count" => count = Some(number()?),
                "bench" => config.bench_frames = Some(number()?),
                // Consumed by the web loader.
                "backend" => {}
                other => return Err(format!("unknown option {other:?}")),
            }
        }
        config.scene = match scene {
            "catalog" => SceneKind::Catalog,
            "minios" => SceneKind::MiniOs,
            "showcase" => SceneKind::Showcase,
            "stress" => SceneKind::Stress {
                count: count.unwrap_or(1500),
            },
            _ => SceneKind::Ui {
                count: count.unwrap_or(1000),
            },
        };
        Ok(config)
    }
}

fn options() -> AppOptions {
    AppOptions {
        version: Some(env!("CARGO_PKG_VERSION").into()),
        title: "noa-gpui demo".into(),
        ..AppOptions::new("noa-gpui")
    }
}

fn screen(config: Config, setup: &mut Setup<'_>) -> Box<dyn Screen> {
    screen_with(
        config,
        Assets::load(setup.renderer, setup.device, setup.queue),
    )
}

fn screen_with(config: Config, assets: Assets) -> Box<dyn Screen> {
    let meter = Meter::new(config);
    match config.scene {
        SceneKind::Ui { count } => Box::new(UiDemo::new(count, &assets).metered(meter)),
        SceneKind::Catalog => Box::new(Catalog::new(&assets).metered(meter)),
        SceneKind::MiniOs => Box::new(MiniOs::new().metered(meter)),
        kind => Box::new(Scene::new(kind, assets, meter)),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn app(config: Config) -> App {
    App::new(options(), move |setup| screen(config, setup))
}

/// Draws the screen for `config` without a window, at the main window's size and
/// `scale`, and writes it to `path` as a PNG. Uses the GPU, or the CPU when there is no
/// GPU adapter.
#[cfg(feature = "native")]
///
/// With `stylesheet`, the screen is styled with the Noa Style sheet in that file first.
pub fn snapshot(
    config: Config,
    path: &std::path::Path,
    scale: f32,
    stylesheet: Option<&std::path::Path>,
    replay: Option<&std::path::Path>,
) -> Result<(), String> {
    let sheet = stylesheet.map(load_stylesheet).transpose()?;
    let recording = replay
        .map(|path| {
            let source = std::fs::read_to_string(path)
                .map_err(|e| format!("read {}: {e}", path.display()))?;
            noa_gpui_platform::noa_gpui_core::recording::Recording::parse(&source)
                .map_err(|e| format!("{}: {e}", path.display()))
        })
        .transpose()?;
    let mut headless = noa_gpui_platform::Headless::new_or_software();
    let mut screen = if headless.is_software() {
        screen_with(
            config,
            Assets::generate(|w, h, rgba| headless.create_image(w, h, rgba)),
        )
    } else {
        screen(config, &mut headless.setup())
    };
    if let Some(sheet) = sheet {
        screen.tree_mut().set_stylesheet(sheet);
    }
    let mut text = noa_gpui_platform::noa_gpui_text::TextStore::new(Default::default());
    let (width, height) = options().size;
    if let Some(recording) = &recording {
        let size = noa_gpui_platform::noa_gpui_scene::Size::new(width as f32, height as f32);
        noa_gpui_platform::replay(screen.as_mut(), recording, &mut text, size);
    }
    let image = headless
        .render_screen(
            screen.as_mut(),
            &mut text,
            (width as u32, height as u32),
            scale,
        )
        .map_err(|e| e.to_string())?;
    image
        .save_png(path)
        .map_err(|e| format!("write {}: {e}", path.display()))
}

#[cfg(feature = "native")]
pub fn load_stylesheet(
    path: &std::path::Path,
) -> Result<noa_gpui_platform::noa_gpui_core::StyleSheet, String> {
    let source =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    noa_gpui_platform::noa_gpui_core::StyleSheet::parse(&source)
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(target_arch = "wasm32")]
pub fn app_with_gpu(
    config: Config,
    instance: noa_gpui_platform::wgpu::Instance,
    ctx: noa_gpui_platform::GpuContext,
    canvas: web_sys::HtmlCanvasElement,
) -> App {
    App::with_gpu(options(), instance, ctx, canvas, move |setup| {
        screen(config, setup)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scenes() {
        let parse = |args: &[&str]| Config::parse(args.iter().copied()).map(|c| c.scene);
        assert_eq!(parse(&[]), Ok(SceneKind::Ui { count: 1000 }));
        assert_eq!(parse(&["--scene=catalog"]), Ok(SceneKind::Catalog));
        assert_eq!(parse(&["--scene=minios"]), Ok(SceneKind::MiniOs));
        assert_eq!(
            parse(&["scene=stress", "count=20"]),
            Ok(SceneKind::Stress { count: 20 })
        );
        assert!(parse(&["scene=nope"]).is_err());
    }

    #[cfg(feature = "native")]
    #[test]
    fn the_example_stylesheet_parses() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("styles/example.noa.css");
        load_stylesheet(&path).unwrap();
        let missing = load_stylesheet(std::path::Path::new("no/such/file.noa.css"));
        assert!(missing.unwrap_err().contains("no/such/file.noa.css"));
    }
}
