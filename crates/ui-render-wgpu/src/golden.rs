//! Comparing rendered images with saved ones ("golden" images) in tests.
//!
//! ```no_run
//! # use noa_gpui_render_wgpu::{RgbaImage, Tolerance, check_golden};
//! # fn render() -> RgbaImage { unimplemented!() }
//! let image = render();
//! check_golden(&image, "tests/golden/form.png", Tolerance::default()).unwrap();
//! ```
//!
//! A missing golden image is written and the check fails once, so a new test is
//! looked at before it passes. With `NOA_GPUI_UPDATE_GOLDEN=1` in the environment,
//! every checked image is written as the new golden one. On a mismatch the rendered
//! image and a picture of the differences are written next to the golden one
//! (`form.actual.png`, `form.diff.png`).

use std::path::{Path, PathBuf};

use crate::offscreen::RgbaImage;

/// How far two images may differ and still match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tolerance {
    /// The most a channel of a pixel may differ by and the pixel still be the same.
    pub channel: u8,
    /// The most pixels that may differ by more than [`Tolerance::channel`].
    pub pixels: usize,
}

impl Default for Tolerance {
    /// Exact.
    fn default() -> Self {
        Self {
            channel: 0,
            pixels: 0,
        }
    }
}

/// How two images of the same size differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageDiff {
    /// Pixels with a channel that differs by more than the tolerance.
    pub differing: usize,
    /// The largest difference of any channel.
    pub max_delta: u8,
    /// The expected image dimmed, with the differing pixels in red.
    pub image: RgbaImage,
}

impl RgbaImage {
    /// Read a PNG file written by [`RgbaImage::to_png`]. Files from other encoders
    /// (compressed or filtered) are refused with the reason.
    pub fn from_png(png: &[u8]) -> Result<Self, String> {
        let (width, height, pixels) = crate::png::decode(png)?;
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn load_png(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_png(&bytes)
            .map_err(|reason| std::io::Error::new(std::io::ErrorKind::InvalidData, reason))
    }

    /// Compare with `expected`, counting pixels with a channel that differs by more
    /// than `channel`. `None` when the sizes differ.
    pub fn diff(&self, expected: &RgbaImage, channel: u8) -> Option<ImageDiff> {
        if (self.width, self.height) != (expected.width, expected.height) {
            return None;
        }
        let mut differing = 0;
        let mut max_delta = 0;
        let mut image = Vec::with_capacity(expected.pixels.len());
        for (actual, wanted) in self
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.pixels.as_chunks::<4>().0)
        {
            let delta = actual
                .iter()
                .zip(wanted)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            max_delta = max_delta.max(delta);
            if delta > channel {
                differing += 1;
                image.extend_from_slice(&[255, 0, 0, 255]);
            } else {
                // A light gray version of the expected pixel, for orientation.
                let luma =
                    (u32::from(wanted[0]) * 3 + u32::from(wanted[1]) * 6 + u32::from(wanted[2]))
                        / 10;
                let dim = (192 + luma / 4) as u8;
                image.extend_from_slice(&[dim, dim, dim, 255]);
            }
        }
        Some(ImageDiff {
            differing,
            max_delta,
            image: RgbaImage {
                width: expected.width,
                height: expected.height,
                pixels: image,
            },
        })
    }
}

/// Why an image did not match its golden one.
#[derive(Debug)]
pub enum GoldenError {
    /// There was no golden image; the rendered one was written as it, to be looked at.
    Created(PathBuf),
    /// The sizes differ. The rendered image was written to `actual`.
    Size {
        expected: (u32, u32),
        actual: (u32, u32),
        written: PathBuf,
    },
    /// Too many pixels differ. The rendered image and the differences were written.
    Mismatch {
        differing: usize,
        max_delta: u8,
        actual: PathBuf,
        diff: PathBuf,
    },
    Io(std::io::Error),
}

impl std::fmt::Display for GoldenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created(path) => write!(
                f,
                "no golden image: wrote {}; check it and run again",
                path.display()
            ),
            Self::Size {
                expected,
                actual,
                written,
            } => write!(
                f,
                "size {}x{} differs from the golden {}x{}; wrote {}",
                actual.0,
                actual.1,
                expected.0,
                expected.1,
                written.display()
            ),
            Self::Mismatch {
                differing,
                max_delta,
                actual,
                diff,
            } => write!(
                f,
                "{differing} pixels differ (largest channel difference {max_delta}); wrote {} and {}",
                actual.display(),
                diff.display()
            ),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for GoldenError {}

impl From<std::io::Error> for GoldenError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// The environment variable that makes [`check_golden`] write every image as the new
/// golden one.
pub const UPDATE_GOLDEN: &str = "NOA_GPUI_UPDATE_GOLDEN";

/// Compare `image` with the golden image at `path` (see the module documentation).
pub fn check_golden(
    image: &RgbaImage,
    path: impl AsRef<Path>,
    tolerance: Tolerance,
) -> Result<(), GoldenError> {
    let update = std::env::var_os(UPDATE_GOLDEN).is_some_and(|v| v != "0" && !v.is_empty());
    check_golden_with(image, path.as_ref(), tolerance, update)
}

fn check_golden_with(
    image: &RgbaImage,
    path: &Path,
    tolerance: Tolerance,
    update: bool,
) -> Result<(), GoldenError> {
    let sibling = |suffix: &str| {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        path.with_file_name(format!("{stem}.{suffix}.png"))
    };
    let (actual_path, diff_path) = (sibling("actual"), sibling("diff"));
    if update || !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        image.save_png(path)?;
        remove_if_present(&actual_path)?;
        remove_if_present(&diff_path)?;
        return if update {
            Ok(())
        } else {
            Err(GoldenError::Created(path.to_owned()))
        };
    }
    let expected = RgbaImage::load_png(path)?;
    let Some(diff) = image.diff(&expected, tolerance.channel) else {
        image.save_png(&actual_path)?;
        return Err(GoldenError::Size {
            expected: (expected.width, expected.height),
            actual: (image.width, image.height),
            written: actual_path,
        });
    };
    if diff.differing > tolerance.pixels {
        image.save_png(&actual_path)?;
        diff.image.save_png(&diff_path)?;
        return Err(GoldenError::Mismatch {
            differing: diff.differing,
            max_delta: diff.max_delta,
            actual: actual_path,
            diff: diff_path,
        });
    }
    // Leftovers of an earlier failure would be mistaken for this run's.
    remove_if_present(&actual_path)?;
    remove_if_present(&diff_path)?;
    Ok(())
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, fill: [u8; 4]) -> RgbaImage {
        RgbaImage {
            width,
            height,
            pixels: fill.repeat((width * height) as usize),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("noa-gpui-golden-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn images_round_trip_through_png_and_compare_with_a_tolerance() {
        let mut a = image(3, 2, [10, 20, 30, 255]);
        assert_eq!(RgbaImage::from_png(&a.to_png()), Ok(a.clone()));
        let b = a.clone();
        a.pixels[0] = 13; // pixel (0, 0) off by 3
        a.pixels[4 * 5 + 2] = 90; // pixel (2, 1) off by 60
        let diff = a.diff(&b, 3).unwrap();
        assert_eq!((diff.differing, diff.max_delta), (1, 60));
        assert_eq!(diff.image.pixel(2, 1), [255, 0, 0, 255]);
        assert_ne!(diff.image.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(a.diff(&b, 2).unwrap().differing, 2);
        assert!(a.diff(&image(2, 3, [0; 4]), 0).is_none());
    }

    #[test]
    fn golden_images_are_created_matched_reported_and_updated() {
        let dir = temp_dir("check");
        let path = dir.join("nested").join("form.png");
        let red = image(4, 4, [200, 0, 0, 255]);
        let exact = Tolerance::default();

        let created = check_golden_with(&red, &path, exact, false);
        assert!(
            matches!(created, Err(GoldenError::Created(_))),
            "{created:?}"
        );
        check_golden_with(&red, &path, exact, false).expect("matches what was written");

        let mut changed = red.clone();
        changed.pixels[0] = 0;
        let mismatch = check_golden_with(&changed, &path, exact, false);
        assert!(
            matches!(
                mismatch,
                Err(GoldenError::Mismatch {
                    differing: 1,
                    max_delta: 200,
                    ..
                })
            ),
            "{mismatch:?}"
        );
        let actual = dir.join("nested").join("form.actual.png");
        let diff = dir.join("nested").join("form.diff.png");
        assert_eq!(RgbaImage::load_png(&actual).unwrap(), changed);
        assert!(diff.exists());

        let loose = Tolerance {
            channel: 0,
            pixels: 1,
        };
        check_golden_with(&changed, &path, loose, false).expect("within the tolerance");
        assert!(!actual.exists() && !diff.exists(), "leftovers removed");

        let small = image(2, 2, [0; 4]);
        assert!(matches!(
            check_golden_with(&small, &path, exact, false),
            Err(GoldenError::Size { .. })
        ));
        check_golden_with(&small, &path, exact, true).expect("updated");
        assert_eq!(RgbaImage::load_png(&path).unwrap(), small);
        let _ = std::fs::remove_dir_all(dir);
    }
}
