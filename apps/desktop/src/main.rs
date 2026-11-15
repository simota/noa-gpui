//! ```text
//! noa-demo [--scene=ui|catalog|minios|showcase|stress] [--count=N] [--bench=FRAMES]
//!          [--snapshot=FILE.png] [--stylesheet=FILE.noa.css] [--view-state=FILE]
//!          [--record=FILE] [--replay=FILE]
//! ```
//!
//! `--snapshot` writes the first frame as a PNG without a window and exits. Without a
//! GPU adapter it draws on the CPU.
//!
//! `--record` writes the window's input to a file as it happens. `--replay` plays such
//! a file back before `--snapshot` takes its picture (a second after the last step,
//! so animations have finished), so a session can be turned into an image to compare.
//!
//! `--stylesheet` applies a Noa Style sheet and, in a window, reloads it on each save.
//!
//! `--view-state` stores view state arranged by hand in that file: the window frame,
//! column widths and sort orders, dividers, tabs, open groups, the scroll position and
//! the mini OS's windows. It is read at start and written when the demo ends.

use demo::Config;

const USAGE: &str = "usage: noa-demo [--scene=ui|catalog|minios|showcase|stress] [--count=N] \
                     [--bench=FRAMES] [--snapshot=FILE.png] [--stylesheet=FILE.noa.css] \
                     [--view-state=FILE] [--record=FILE] [--replay=FILE]";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let snapshot = args
        .iter()
        .find_map(|a| a.strip_prefix("--snapshot="))
        .map(std::path::PathBuf::from);
    let stylesheet = args
        .iter()
        .find_map(|a| a.strip_prefix("--stylesheet="))
        .map(std::path::PathBuf::from);
    let view_state = args
        .iter()
        .find_map(|a| a.strip_prefix("--view-state="))
        .map(std::path::PathBuf::from);
    let record = args
        .iter()
        .find_map(|a| a.strip_prefix("--record="))
        .map(std::path::PathBuf::from);
    let replay = args
        .iter()
        .find_map(|a| a.strip_prefix("--replay="))
        .map(std::path::PathBuf::from);
    let options = args.iter().filter(|a| {
        !a.starts_with("--snapshot=")
            && !a.starts_with("--stylesheet=")
            && !a.starts_with("--view-state=")
            && !a.starts_with("--record=")
            && !a.starts_with("--replay=")
    });
    if replay.is_some() && snapshot.is_none() {
        eprintln!("error: --replay needs --snapshot\n{USAGE}");
        std::process::exit(2);
    }
    let config = Config::parse(options.map(String::as_str)).unwrap_or_else(|e| {
        eprintln!("error: {e}\n{USAGE}");
        std::process::exit(2);
    });

    if let Some(path) = snapshot {
        if let Err(e) = demo::snapshot(config, &path, 2.0, stylesheet.as_deref(), replay.as_deref())
        {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        println!("wrote {}", path.display());
        return;
    }
    let mut app = demo::app(config);
    if let Some(path) = stylesheet {
        app = app.watch_stylesheet(path);
    }
    if let Some(path) = view_state {
        app = app.autosave_view_state(path);
    }
    if let Some(path) = record {
        app = app.record_input(path);
    }
    app.run().expect("run event loop");
}
