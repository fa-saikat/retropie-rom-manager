//! Asset loading: the Lucide icons the UI uses (embedded, via GPUI Kit) and
//! the per-system icons, which are plain SVG files on disk.
//!
//! System icons are *not* embedded — they live in an `assets/` folder next
//! to the app (or the installed `/usr/share/...` copy) so people can swap
//! them. Every lookup is a simple existence check and returns `None` when
//! nothing is found, so callers fall back to a generic gamepad icon.
//!
//! Expected layout:
//! ```text
//! assets/
//!   icons/
//!     psx.svg              <- emulator-specific, checked first
//!     device-gamepad.svg   <- generic fallback (SystemDef::icon), e.g. Tabler icons
//!   logos/
//!     snes.svg             <- the system's own logo, by system id (full colour)
//!   systems/
//!     snes.jpg             <- product photo for the "Add emulator" picker
//! ```

use gpui_kit::{AssetSource, Result, SharedString};
use std::borrow::Cow;
use std::path::PathBuf;

// GPUI Kit's default bundle covers ~100 Lucide icons; these are the extra
// ones this app uses. Unlisted names fall through to the default bundle.
gpui_kit::assets::icon_assets!(ExtraIcons, [
    Trash, LayoutGrid, List, ImageDown, ImageOff, Gamepad2, Upload, X, VideoOff, Files, Download,
    FolderInput, TriangleAlert, ArrowRight, Play, Folder, File, HardDrive, House, ArrowUp,
    ChevronRight, AppWindow, Maximize,
]);

/// Tabler icons the UI needs, embedded so a missing `assets/` folder can't
/// blank a button. Use with `Icon::empty().path(QUIT_ICON)`.
pub const QUIT_ICON: &str = "icons/door-exit.svg";

/// The bundled fonts (see `assets/fonts/README.md`), embedded so the UI
/// looks the same on every machine. Registered once at startup.
pub fn fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Bold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Poppins-Regular.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Poppins-Medium.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Poppins-SemiBold.ttf")),
        Cow::Borrowed(include_bytes!("../assets/fonts/Poppins-Bold.ttf")),
    ]
}

/// The app's asset source: our extra icons first, then GPUI Kit's defaults.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == QUIT_ICON {
            return Ok(Some(Cow::Borrowed(include_bytes!("../assets/icons/door-exit.svg"))));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

/// Root of the on-disk asset tree. Resolution order:
/// 1. `assets/` next to the running executable (installed/release layout)
/// 2. `assets/` in the current working directory (`cargo run` from repo root)
/// 3. the Debian package's `/usr/share/retropie-rom-manager/assets`
fn assets_root() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("assets");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }

    let cwd_candidate = PathBuf::from("assets");
    if cwd_candidate.is_dir() {
        return Some(cwd_candidate);
    }

    let system_candidate = PathBuf::from("/usr/share/retropie-rom-manager/assets");
    if system_candidate.is_dir() {
        return Some(system_candidate);
    }

    None
}

/// Icon for a system.
///
/// Looks for an emulator-specific icon first (`icons/<system_id>.svg`,
/// e.g. `icons/psx.svg`), then falls back to the generic icon named on
/// `SystemDef::icon` (`icons/<icon>.svg` — Tabler icon names, since that's
/// what `SystemDef::icon` stores). Only SVGs: they're drawn as single-colour
/// masks so they can sit white-on-accent in the UI.
pub fn system_icon(system_id: &str, generic_icon: &str) -> Option<PathBuf> {
    let root = assets_root()?;
    [system_id, generic_icon]
        .iter()
        .map(|stem| root.join("icons").join(format!("{stem}.svg")))
        .find(|candidate| candidate.is_file())
}

/// The system's own logo (`logos/<system_id>.svg`), drawn in full colour on
/// a light plate in the library header, its empty state and the "Add
/// emulator" picker. `None` for systems without one, so callers fall back
/// to the coloured tile. See `assets/logos/CREDITS.md` for sources.
pub fn system_logo(system: &crate::systems::SystemDef) -> Option<PathBuf> {
    let path = assets_root()?.join("logos").join(format!("{}.svg", system.id));
    path.is_file().then_some(path)
}

/// Product photo for a system (`systems/<photo>.jpg`), shown in the
/// "Add emulator" picker. See `assets/systems/CREDITS.md` for sources.
pub fn system_photo(system: &crate::systems::SystemDef) -> Option<PathBuf> {
    let path = assets_root()?.join("systems").join(format!("{}.jpg", system.photo));
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::AppAssets;
    use crate::systems::SYSTEMS;
    use gpui_kit::SvgRenderer;
    use std::path::Path;
    use std::sync::Arc;

    fn logos_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/logos")
    }

    /// Every logo is named after a real system id, so none is silently unused.
    #[test]
    fn logos_belong_to_known_systems() {
        for entry in std::fs::read_dir(logos_dir()).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if let Some(id) = name.strip_suffix(".svg") {
                assert!(SYSTEMS.iter().any(|s| s.id == id), "{name} matches no system id");
            }
        }
    }

    /// Every system has a logo, apart from the ones with no free logo to use.
    #[test]
    fn systems_have_logos() {
        const WITHOUT_LOGO: &[&str] = &["fba"];
        for system in SYSTEMS.iter().filter(|s| !WITHOUT_LOGO.contains(&s.id)) {
            assert!(logos_dir().join(format!("{}.svg", system.id)).is_file(), "{} has no logo", system.id);
        }
    }

    /// GPUI renders image SVGs at twice their declared size. The logos are
    /// normalised to declare 800-1000px wide: sharp on a 4K header plate,
    /// without rasterising a 7680px-wide bitmap.
    #[test]
    fn logos_render_at_a_sensible_size() {
        let renderer = SvgRenderer::new(Arc::new(AppAssets));
        for entry in std::fs::read_dir(logos_dir()).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "svg") {
                continue;
            }
            let image = renderer
                .render_single_frame(&std::fs::read(&path).unwrap(), 1.0)
                .unwrap_or_else(|e| panic!("{} doesn't render: {e}", path.display()));
            let width = image.size(0).width.0;
            assert!((1600..=2000).contains(&width), "{} renders {width}px wide", path.display());
        }
    }
}
