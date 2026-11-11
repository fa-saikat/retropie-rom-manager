//! Shrunk copies of scraped art, so the GPU holds each image at about the
//! size it's shown rather than the size Skyscraper saved it.
//!
//! [`sized`] is called while rendering, so it never decodes on the UI thread:
//! - The first call for an image in a session picks the file to draw and
//!   remembers it. An image never switches files mid-session; a switch would
//!   make GPUI reload it and flash its loading state.
//! - If a copy is already cached it picks that. Otherwise it picks the
//!   original, and a background thread writes the copy for the next launch.
//! - A copy is only made when it's clearly smaller than the original, and it
//!   is never smaller than `min` in either dimension, so `ObjectFit::Cover`
//!   still only ever shrinks it. What's on screen doesn't change.
//!
//! Copies live in `~/.cache/retropie-rom-manager/covers`, named from the
//! original's path, size and modified time plus `min`, so a re-scraped cover
//! gets a new copy. Deleting the folder is always safe.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use anyhow::Context as _;
use image::imageops::FilterType;
use image::ImageFormat;

/// Below this much shrinking a copy isn't worth the disk space.
const WORTH_IT: f64 = 0.9;

/// Which file each (original, min) draws from this session.
static CHOSEN: OnceLock<Mutex<HashMap<(PathBuf, (u32, u32)), PathBuf>>> = OnceLock::new();
/// The background worker's queue.
static QUEUE: OnceLock<Sender<Job>> = OnceLock::new();

struct Job {
    source: PathBuf,
    copy: PathBuf,
    min: (u32, u32),
}

/// The file to draw for `original` in a slot that needs at least `min`
/// device pixels (width, height).
pub fn sized(original: &Path, min: (u32, u32)) -> PathBuf {
    // Size not known (see `scale::large_art`): keep the original.
    if min.0 == u32::MAX || min.1 == u32::MAX {
        return original.to_path_buf();
    }
    let key = (original.to_path_buf(), min);
    let mut chosen = CHOSEN.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if let Some(path) = chosen.get(&key) {
        return path.clone();
    }
    let pick = match cache_dir().and_then(|dir| copy_path(&dir, original, min)) {
        Some(copy) if copy.is_file() => copy,
        Some(copy) => {
            queue(Job { source: original.to_path_buf(), copy, min });
            original.to_path_buf()
        }
        None => original.to_path_buf(),
    };
    chosen.insert(key, pick.clone());
    pick
}

fn cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("retropie-rom-manager").join("covers"))
}

/// Where the copy of `original` at `min` goes, or `None` if the original
/// can't be read.
fn copy_path(dir: &Path, original: &Path, min: (u32, u32)) -> Option<PathBuf> {
    let meta = fs::metadata(original).ok()?;
    let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    let mut hash = Fnv::new();
    hash.write(original.as_os_str().as_encoded_bytes());
    hash.write(&meta.len().to_le_bytes());
    hash.write(&modified.to_le_bytes());
    hash.write(&min.0.to_le_bytes());
    hash.write(&min.1.to_le_bytes());
    Some(dir.join(format!("{:016x}.png", hash.0)))
}

fn queue(job: Job) {
    let sender = QUEUE.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        // One worker, so a long scroll doesn't take every core from the UI.
        let spawned = std::thread::Builder::new().name("cover-cache".into()).spawn(move || {
            for job in rx {
                if let Err(err) = shrink(&job.source, &job.copy, job.min) {
                    log::debug!("cover cache: skipped {}: {err:#}", job.source.display());
                }
            }
        });
        if let Err(err) = spawned {
            log::warn!("cover cache: no worker thread: {err}");
        }
        tx
    });
    // If the worker never started, the original keeps being drawn.
    let _ = sender.send(job);
}

/// Writes a copy of `source` that covers `min` to `copy`. Does nothing if
/// the source is already about that small.
fn shrink(source: &Path, copy: &Path, min: (u32, u32)) -> anyhow::Result<()> {
    let dimensions = image::image_dimensions(source).context("reading size")?;
    let Some((width, height)) = shrunk_size(dimensions, min) else {
        return Ok(());
    };
    let image = image::open(source).context("decoding")?;
    let small = image.resize_exact(width, height, FilterType::Lanczos3);
    let dir = copy.parent().context("cache path has no folder")?;
    fs::create_dir_all(dir).context("creating cache folder")?;
    // Write then rename, so a half-written file is never picked up.
    let partial = copy.with_extension("partial");
    small.save_with_format(&partial, ImageFormat::Png).context("saving")?;
    fs::rename(&partial, copy).context("saving")?;
    Ok(())
}

/// The smallest size with the source's shape that is at least `min` in both
/// dimensions, or `None` if that wouldn't shrink the source meaningfully.
fn shrunk_size((width, height): (u32, u32), (min_w, min_h): (u32, u32)) -> Option<(u32, u32)> {
    if width == 0 || height == 0 {
        return None;
    }
    let scale = (min_w as f64 / width as f64).max(min_h as f64 / height as f64);
    if scale >= WORTH_IT {
        return None;
    }
    let w = ((width as f64 * scale).ceil() as u32).clamp(min_w.min(width), width);
    let h = ((height as f64 * scale).ceil() as u32).clamp(min_h.min(height), height);
    Some((w, h))
}

/// FNV-1a: stable across Rust versions, unlike `DefaultHasher`, so cached
/// names survive an upgrade.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= *byte as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinks_to_cover_the_slot_without_going_under() {
        // A tall cover in a 512 square: width is the limiting side.
        assert_eq!(shrunk_size((1000, 1400), (512, 512)), Some((512, 717)));
        // A wide screenshot for a 1920x1080 backdrop: height limits.
        assert_eq!(shrunk_size((3840, 2160), (1920, 1080)), Some((1920, 1080)));
    }

    #[test]
    fn leaves_small_or_nearly_small_images_alone() {
        assert_eq!(shrunk_size((400, 560), (512, 512)), None);
        assert_eq!(shrunk_size((540, 700), (512, 512)), None);
        assert_eq!(shrunk_size((0, 0), (512, 512)), None);
    }

    #[test]
    fn copy_name_changes_with_the_slot_and_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let cover = dir.path().join("cover.png");
        fs::write(&cover, b"one").unwrap();
        let a = copy_path(dir.path(), &cover, (512, 512)).unwrap();
        assert_eq!(a, copy_path(dir.path(), &cover, (512, 512)).unwrap());
        assert_ne!(a, copy_path(dir.path(), &cover, (1024, 1024)).unwrap());
        fs::write(&cover, b"rescraped").unwrap();
        assert_ne!(a, copy_path(dir.path(), &cover, (512, 512)).unwrap());
        assert_eq!(copy_path(dir.path(), &dir.path().join("missing.png"), (512, 512)), None);
    }

    #[test]
    fn writes_a_smaller_copy_that_still_covers_the_slot() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("cover.png");
        image::RgbaImage::from_pixel(1000, 1400, image::Rgba([200, 40, 40, 255])).save(&source).unwrap();
        let copy = dir.path().join("cache").join("copy.png");
        shrink(&source, &copy, (512, 512)).unwrap();
        assert_eq!(image::image_dimensions(&copy).unwrap(), (512, 717));
        assert!(!copy.with_extension("partial").exists());
    }

    #[test]
    fn writes_nothing_for_an_image_already_small_enough() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("cover.png");
        image::RgbaImage::new(300, 400).save(&source).unwrap();
        let copy = dir.path().join("copy.png");
        shrink(&source, &copy, (512, 512)).unwrap();
        assert!(!copy.exists());
    }
}
