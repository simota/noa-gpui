use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use ui_core::{StyleError, StyleSheet};

use crate::wake::Waker;

/// Why a stylesheet file could not be used. The previous sheet stays in effect.
#[derive(Debug)]
pub enum StyleSheetLoadError {
    /// The file could not be read (it is missing, say, while an editor replaces it).
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    Parse {
        path: PathBuf,
        error: StyleError,
    },
}

impl fmt::Display for StyleSheetLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Parse { path, error } => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for StyleSheetLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { error, .. } => Some(error),
            Self::Parse { error, .. } => Some(error),
        }
    }
}

pub type StyleSheetLoad = Result<StyleSheet, StyleSheetLoadError>;

/// Loads a stylesheet from a file and reloads it whenever the file changes, checking
/// on a background thread every [`StyleSheetWatcher::DEFAULT_INTERVAL`].
///
/// A change is read once the file has stayed the same for one interval, so a file
/// caught half written is not loaded. After each load the [`Waker`] is woken. The
/// thread stops when the watcher is dropped.
#[derive(Debug)]
pub struct StyleSheetWatcher {
    path: PathBuf,
    shared: Arc<Shared>,
}

#[derive(Debug, Default)]
struct Shared {
    /// The newest load not yet taken.
    latest: Mutex<Option<StyleSheetLoad>>,
    stop: AtomicBool,
}

/// What the file looked like at the last check: its modification time and length, or
/// `None` when it could not be read.
type Signature = Option<(Option<SystemTime>, u64)>;

fn signature(path: &Path) -> Signature {
    std::fs::metadata(path)
        .ok()
        .map(|m| (m.modified().ok(), m.len()))
}

/// Decides when a changed file is read: once it has looked the same for a whole
/// interval. An editor that saves by emptying the file and writing it again is
/// otherwise read in between, and the empty file is a valid sheet that drops every
/// style until the next check.
#[derive(Debug)]
struct Settle {
    /// What the file looked like when it was last read.
    read: Signature,
    /// A different look seen at the last check, not read yet.
    pending: Option<Signature>,
}

impl Settle {
    fn new(read: Signature) -> Self {
        Self {
            read,
            pending: None,
        }
    }

    /// Whether to read the file, which looks like `now`.
    fn check(&mut self, now: Signature) -> bool {
        if now == self.read {
            self.pending = None;
            return false;
        }
        let settled = self.pending == Some(now);
        self.pending = Some(now);
        settled
    }

    /// The file was read while it looked like `now`.
    fn was_read(&mut self, now: Signature) {
        self.read = now;
        self.pending = None;
    }
}

fn load(path: &Path) -> (Option<String>, StyleSheetLoad) {
    match std::fs::read_to_string(path) {
        Ok(source) => {
            let sheet = StyleSheet::parse(&source).map_err(|error| StyleSheetLoadError::Parse {
                path: path.to_owned(),
                error,
            });
            (Some(source), sheet)
        }
        Err(error) => (
            None,
            Err(StyleSheetLoadError::Read {
                path: path.to_owned(),
                error,
            }),
        ),
    }
}

impl StyleSheetWatcher {
    pub const DEFAULT_INTERVAL: Duration = Duration::from_millis(250);

    /// Read `path` now (the result is the first [`StyleSheetWatcher::take`]) and watch it.
    pub fn new(path: impl Into<PathBuf>, waker: Waker) -> Self {
        Self::with_interval(path, waker, Self::DEFAULT_INTERVAL)
    }

    /// [`StyleSheetWatcher::new`], checking the file every `interval`.
    pub fn with_interval(path: impl Into<PathBuf>, waker: Waker, interval: Duration) -> Self {
        let path = path.into();
        let mut settle = Settle::new(signature(&path));
        let (mut text, first) = load(&path);
        let shared = Arc::new(Shared {
            latest: Mutex::new(Some(first)),
            stop: AtomicBool::new(false),
        });
        let thread_shared = Arc::clone(&shared);
        let thread_path = path.clone();
        let spawned = std::thread::Builder::new()
            .name("noa-stylesheet-watcher".into())
            .spawn(move || {
                let shared = thread_shared;
                let path = thread_path;
                while !shared.stop.load(Ordering::Acquire) {
                    std::thread::sleep(interval);
                    let now = signature(&path);
                    if !settle.check(now) {
                        continue;
                    }
                    let (source, result) = load(&path);
                    // Written to while being read: read it again once it settles.
                    if signature(&path) != now {
                        continue;
                    }
                    settle.was_read(now);
                    if source.is_some() && source == text {
                        continue;
                    }
                    text = source;
                    *shared.latest.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
                    waker.wake();
                }
            });
        if let Err(e) = spawned {
            log::error!("cannot watch {}: {e}", path.display());
        }
        Self { path, shared }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The newest load since the last call, if the file was (re)loaded since then.
    /// Loads that were never taken are replaced by newer ones.
    pub fn take(&self) -> Option<StyleSheetLoad> {
        self.shared
            .latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
}

impl Drop for StyleSheetWatcher {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    const FAST: Duration = Duration::from_millis(10);

    fn temp_file(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("noa-stylesheet-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("app.noa.css")
    }

    fn next(watcher: &StyleSheetWatcher) -> StyleSheetLoad {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(load) = watcher.take() {
                return load;
            }
            assert!(Instant::now() < deadline, "no reload within 10 s");
            std::thread::sleep(FAST);
        }
    }

    /// Write so that the modification time or the length differs from the last write
    /// even on file systems with coarse timestamps.
    fn write(path: &Path, text: &str) {
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_change_is_read_once_the_file_stays_the_same_for_an_interval() {
        let look = |len: u64| Some((None, len));
        let mut settle = Settle::new(look(20));
        assert!(!settle.check(look(20)), "unchanged");
        assert!(!settle.check(look(0)), "emptied");
        assert!(!settle.check(look(42)), "written again");
        assert!(settle.check(look(42)), "the same for an interval");
        settle.was_read(look(42));
        assert!(!settle.check(look(42)));
        assert!(!settle.check(look(7)));
        assert!(!settle.check(look(42)));
        assert!(!settle.check(look(42)));
        assert!(!settle.check(None));
        assert!(settle.check(None));
    }

    #[test]
    fn loads_now_and_again_after_each_change() {
        let path = temp_file("reload");
        write(&path, ".a { padding: 4px; }");
        let watcher = StyleSheetWatcher::with_interval(&path, Waker::default(), FAST);
        assert!(next(&watcher).is_ok());
        assert!(watcher.take().is_none(), "one load per change");

        write(&path, ".a { padding: 4px; } .b { gap: 2px; }");
        assert!(next(&watcher).is_ok());

        write(&path, ".a { padding: 4px; colour: red; }");
        let error = next(&watcher).unwrap_err();
        assert!(matches!(error, StyleSheetLoadError::Parse { .. }));
        assert!(error.to_string().contains("app.noa.css"));
        write(&path, ".a { color: red; }");
        assert!(next(&watcher).is_ok());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_missing_file_is_an_error_until_it_appears() {
        let path = temp_file("missing");
        let _ = std::fs::remove_file(&path);
        let watcher = StyleSheetWatcher::with_interval(&path, Waker::default(), FAST);
        assert!(matches!(
            next(&watcher),
            Err(StyleSheetLoadError::Read { .. })
        ));
        write(&path, "Button { border-radius: 2px; }");
        assert!(next(&watcher).is_ok());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn rewriting_the_same_text_loads_nothing() {
        let path = temp_file("same");
        write(&path, ".a { gap: 1px; }");
        let watcher = StyleSheetWatcher::with_interval(&path, Waker::default(), FAST);
        assert!(next(&watcher).is_ok());
        std::thread::sleep(Duration::from_millis(20));
        write(&path, ".a { gap: 1px; }");
        std::thread::sleep(Duration::from_millis(200));
        assert!(watcher.take().is_none());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn wakes_the_event_loop_after_a_reload() {
        let path = temp_file("wake");
        write(&path, ".a { gap: 1px; }");
        let waker = Waker::default();
        let watcher = StyleSheetWatcher::with_interval(&path, waker.clone(), FAST);
        assert!(next(&watcher).is_ok());
        assert!(
            !waker.take(),
            "the first load is there before the loop runs"
        );
        write(&path, ".a { gap: 12px; }");
        assert!(next(&watcher).is_ok());
        assert!(waker.take());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
