use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use noa_gpui_core::ViewState;

/// The entry [`crate::App::autosave_view_state`] keeps the main window's geometry
/// under (see [`MainWindowState`]).
pub const MAIN_WINDOW_KEY: &str = "noa.main-window";

static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

/// A file holding a [`ViewState`] in its text form ([`ViewState::to_text`]).
#[derive(Debug, Clone)]
pub struct ViewStateFile {
    path: PathBuf,
}

impl ViewStateFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The saved state; empty when there is no file yet (the first run). Lines that
    /// cannot be read are skipped ([`ViewState::parse`]).
    pub fn load(&self) -> io::Result<ViewState> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => Ok(ViewState::parse(&text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(ViewState::new()),
            Err(e) => Err(e),
        }
    }

    /// Write `state`, creating the file's folder if needed. The text goes to a file
    /// beside it first, which then replaces the old one. Each save exclusively creates
    /// its own temporary file, so concurrent saves cannot share a partial write and
    /// pre-existing temporary files or symlinks are left alone. A write or replacement
    /// error leaves the destination unchanged. This does not guarantee durability
    /// across power loss.
    pub fn save(&self, state: &ViewState) -> io::Result<()> {
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let mut collisions = 0;
        let (temp, mut file) = loop {
            let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
            // Use a short sibling name even if the destination's name is already
            // close to the filesystem's limit. A previous process may have left a
            // file with this name: create_new never opens or truncates it.
            let temp = self.path.with_file_name(format!(
                ".noa-view-state-{}-{sequence}.tmp",
                std::process::id()
            ));
            if temp == self.path {
                continue;
            }
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
            {
                Ok(file) => break (temp, file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists && collisions < 64 => {
                    collisions += 1;
                }
                Err(error) => return Err(error),
            }
        };
        let result = (|| {
            file.write_all(state.to_text().as_bytes())?;
            drop(file);
            std::fs::rename(&temp, &self.path)
        })();
        result.inspect_err(|_| {
            let _ = std::fs::remove_file(&temp);
        })
    }
}

/// The main window's size, position and whether it was maximized, as kept under
/// [`MAIN_WINDOW_KEY`]: `size:1280,800;position:120,64;maximized:false`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MainWindowState {
    /// Logical size of the content area.
    pub size: (f64, f64),
    /// Physical position of the window's outer top-left corner, where the system
    /// reports it (not on Wayland).
    pub position: Option<(i32, i32)>,
    pub maximized: bool,
}

impl MainWindowState {
    /// The smallest size restored: anything smaller is taken as a mistake.
    pub const MIN_SIZE: (f64, f64) = (200.0, 150.0);

    pub fn to_value(&self) -> String {
        let mut out = format!("size:{},{}", self.size.0.round(), self.size.1.round());
        if let Some((x, y)) = self.position {
            out.push_str(&format!(";position:{x},{y}"));
        }
        out.push_str(&format!(";maximized:{}", self.maximized));
        out
    }

    /// Read a value written by [`MainWindowState::to_value`]; `None` without a usable
    /// size.
    pub fn parse(value: &str) -> Option<Self> {
        let field = |key: &str| {
            value.split(';').find_map(|f| {
                let (k, v) = f.split_once(':')?;
                (k.trim() == key).then_some(v.trim())
            })
        };
        let pair = |s: &str| -> Option<(f64, f64)> {
            let (a, b) = s.split_once(',')?;
            let (a, b) = (a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?);
            (a.is_finite() && b.is_finite()).then_some((a, b))
        };
        let size = pair(field("size")?)?;
        if size.0 < Self::MIN_SIZE.0 || size.1 < Self::MIN_SIZE.1 {
            return None;
        }
        let position = field("position").and_then(|p| {
            let (x, y) = p.split_once(',')?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        });
        Some(Self {
            size,
            position,
            maximized: field("maximized") == Some("true"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "noa-view-state-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_missing_file_is_an_empty_state_and_saving_creates_it() {
        let dir = temp_dir("save");
        let file = ViewStateFile::new(dir.join("nested/state.txt"));
        assert!(file.load().unwrap().is_empty());
        let mut state = ViewState::new();
        state.set("files", "columns:3;widths:0=200");
        state.set(MAIN_WINDOW_KEY, "size:900,700;maximized:false");
        file.save(&state).unwrap();
        assert_eq!(file.load().unwrap(), state);
        state.remove("files");
        file.save(&state).unwrap();
        assert_eq!(file.load().unwrap(), state);
        let names: Vec<_> = std::fs::read_dir(dir.join("nested"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["state.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_folder_in_the_way_is_an_error() {
        let dir = temp_dir("folder");
        std::fs::create_dir_all(dir.join("state.txt")).unwrap();
        let file = ViewStateFile::new(dir.join("state.txt"));
        assert!(file.load().is_err());
        assert!(file.save(&ViewState::new()).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn saving_leaves_an_existing_temporary_file_untouched() {
        let dir = temp_dir("existing-temp");
        std::fs::create_dir_all(&dir).unwrap();
        let sidecar = dir.join("state.txt.tmp");
        std::fs::write(&sidecar, "another file").unwrap();
        let file = ViewStateFile::new(dir.join("state.txt"));
        let state = ViewState::from_iter([("panel".into(), "position:20,40".into())]);
        file.save(&state).unwrap();
        assert_eq!(file.load().unwrap(), state);
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), "another file");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn saving_does_not_follow_a_temporary_file_symlink() {
        let dir = temp_dir("symlink");
        std::fs::create_dir_all(&dir).unwrap();
        let other = dir.join("other.txt");
        let sidecar = dir.join("state.txt.tmp");
        std::fs::write(&other, "another file").unwrap();
        std::os::unix::fs::symlink(&other, &sidecar).unwrap();
        let file = ViewStateFile::new(dir.join("state.txt"));
        let state = ViewState::from_iter([("panel".into(), "position:20,40".into())]);
        file.save(&state).unwrap();
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "another file");
        assert!(
            std::fs::symlink_metadata(&sidecar)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(file.load().unwrap(), state);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_replacement_cleans_up_only_its_own_temporary_file() {
        let dir = temp_dir("failed-replacement");
        let destination = dir.join("state.txt");
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(destination.join("keep"), "previous content").unwrap();
        let sidecar = dir.join("state.txt.tmp");
        std::fs::write(&sidecar, "another file").unwrap();
        let file = ViewStateFile::new(&destination);
        assert!(file.save(&ViewState::new()).is_err());
        assert_eq!(
            std::fs::read_to_string(destination.join("keep")).unwrap(),
            "previous content"
        );
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), "another file");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn concurrent_saves_each_replace_the_file_with_one_complete_state() {
        let dir = temp_dir("concurrent");
        let file = ViewStateFile::new(dir.join("state.txt"));
        let states: Vec<_> = (0..4)
            .map(|writer| {
                ViewState::from_iter([
                    ("writer".into(), writer.to_string()),
                    ("payload".into(), format!("writer-{writer}-").repeat(8192)),
                ])
            })
            .collect();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(states.len()));
        let writers: Vec<_> = states
            .iter()
            .cloned()
            .map(|state| {
                let file = file.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || -> io::Result<()> {
                    barrier.wait();
                    for _ in 0..16 {
                        file.save(&state)?;
                    }
                    Ok(())
                })
            })
            .collect();
        let results: Vec<_> = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect();
        for result in results {
            result.unwrap();
        }
        assert!(states.contains(&file.load().unwrap()));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn main_window_values_round_trip_and_reject_nonsense() {
        let state = MainWindowState {
            size: (1024.0, 700.0),
            position: Some((-1200, 40)),
            maximized: true,
        };
        assert_eq!(
            state.to_value(),
            "size:1024,700;position:-1200,40;maximized:true"
        );
        assert_eq!(MainWindowState::parse(&state.to_value()), Some(state));
        let no_position = MainWindowState::parse("size:800,600").unwrap();
        assert_eq!(no_position.position, None);
        assert!(!no_position.maximized);
        assert_eq!(MainWindowState::parse("size:10,10"), None);
        assert_eq!(MainWindowState::parse("size:inf,600"), None);
        assert_eq!(MainWindowState::parse("position:1,2"), None);
    }
}
