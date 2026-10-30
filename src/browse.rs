//! What the built-in file browser shows: folder listings filtered to a
//! system's extensions, the places to start from (home folders and USB
//! drives, mounted or not), and mounting a drive through udisks.
//!
//! No GPUI dependency, so it's unit-testable.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// Bytes, for files.
    pub size: u64,
    pub modified: Option<SystemTime>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Listing {
    /// Folders first, then files, each sorted by name.
    pub entries: Vec<Entry>,
    /// Files left out because they don't match the extensions.
    pub hidden: usize,
}

/// Whether `name` ends in one of `extensions` (each like `.gba`), ignoring case.
pub fn matches(name: &str, extensions: &[&str]) -> bool {
    let name = name.to_lowercase();
    extensions.iter().any(|ext| name.ends_with(&ext.to_lowercase()))
}

/// List `dir`. Dotfiles are skipped. Files that don't match `extensions`
/// are counted in `hidden` instead, unless `show_all` is set.
pub fn list(dir: &Path, extensions: &[&str], show_all: bool) -> std::io::Result<Listing> {
    let mut listing = Listing::default();
    for item in std::fs::read_dir(dir)? {
        let Ok(item) = item else { continue };
        let name = item.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        // Follow symlinks, so a linked folder opens like a folder.
        let Ok(meta) = std::fs::metadata(item.path()) else { continue };
        let is_dir = meta.is_dir();
        if !is_dir && !show_all && !matches(&name, extensions) {
            listing.hidden += 1;
            continue;
        }
        listing.entries.push(Entry {
            name,
            path: item.path(),
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
            modified: meta.modified().ok(),
        });
    }
    listing.entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(listing)
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaceKind {
    Folder(PathBuf),
    /// A removable drive. `mount` is where it's mounted, if it is.
    Drive { device: String, mount: Option<PathBuf> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub detail: String,
    pub kind: PlaceKind,
}

impl Place {
    /// The folder to open, once mounted.
    pub fn root(&self) -> Option<&Path> {
        match &self.kind {
            PlaceKind::Folder(path) => Some(path),
            PlaceKind::Drive { mount, .. } => mount.as_deref(),
        }
    }
}

/// Home and Downloads, then any removable drives.
pub fn places() -> Vec<Place> {
    let mut places = Vec::new();
    let home = dirs::home_dir();
    if let Some(home) = &home {
        places.push(Place { name: "Home".into(), detail: String::new(), kind: PlaceKind::Folder(home.clone()) });
    }
    if let Some(downloads) = dirs::download_dir().filter(|d| d.is_dir() && Some(d) != home.as_ref()) {
        places.push(Place { name: "Downloads".into(), detail: String::new(), kind: PlaceKind::Folder(downloads) });
    }
    let lsblk = Command::new("lsblk").args(["-P", "-b", "-o", "PATH,LABEL,SIZE,FSTYPE,MOUNTPOINT,HOTPLUG,TYPE"]).output();
    if let Some(out) = lsblk.ok().filter(|o| o.status.success()) {
        places.extend(drives_from_lsblk(&String::from_utf8_lossy(&out.stdout)));
    }
    places
}

/// Removable drives with a filesystem, from `lsblk -P -b -o
/// PATH,LABEL,SIZE,FSTYPE,MOUNTPOINT,HOTPLUG,TYPE`.
pub fn drives_from_lsblk(text: &str) -> Vec<Place> {
    text.lines()
        .filter_map(|line| {
            let field = |key: &str| lsblk_field(line, key).unwrap_or_default();
            let removable = field("HOTPLUG") == "1";
            let filesystem = !field("FSTYPE").is_empty();
            let kind = field("TYPE");
            if !removable || !filesystem || !(kind == "part" || kind == "disk") {
                return None;
            }
            let device = field("PATH");
            let label = field("LABEL");
            let name = if label.is_empty() { device.trim_start_matches("/dev/").to_string() } else { label };
            let size = field("SIZE").parse::<u64>().map(crate::library::format_size).unwrap_or_default();
            let mount = Some(field("MOUNTPOINT")).filter(|m| !m.is_empty()).map(PathBuf::from);
            let detail = if mount.is_some() { format!("USB drive · {size}") } else { "Not mounted".to_string() };
            Some(Place { name, detail, kind: PlaceKind::Drive { device, mount } })
        })
        .collect()
}

/// One `KEY="value"` from an `lsblk -P` line. lsblk escapes characters
/// such as spaces in values as `\xNN`.
fn lsblk_field(line: &str, key: &str) -> Option<String> {
    // Match the whole key: TYPE must not hit FSTYPE.
    let pattern = format!("{key}=\"");
    let at = line.match_indices(&pattern).find(|(i, _)| *i == 0 || line.as_bytes()[i - 1] == b' ')?.0;
    let rest = &line[at + pattern.len()..];
    let raw = &rest[..rest.find('"')?];
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'x') {
            chars.next();
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                out.push(byte as char);
                continue;
            }
        }
        out.push(c);
    }
    Some(out)
}

/// Mount `device` through udisks and return where it was mounted. polkit
/// may ask the user for permission.
pub fn mount(device: &str) -> Result<PathBuf> {
    let out = Command::new("udisksctl")
        .args(["mount", "-b", device])
        .output()
        .map_err(|e| anyhow!("couldn't run udisksctl: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if out.status.success() {
        if let Some(path) = mount_point_from_udisksctl(&stdout) {
            return Ok(path);
        }
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let reason = stderr.lines().chain(stdout.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("udisksctl failed");
    Err(anyhow!("{}", reason.trim_start_matches("Error mounting ")))
}

/// The mount point in `udisksctl mount` output:
/// `Mounted /dev/sdb1 at /media/user/STICK` (older versions end in `.`).
pub fn mount_point_from_udisksctl(output: &str) -> Option<PathBuf> {
    let line = output.lines().find(|l| l.starts_with("Mounted "))?;
    let path = line.split_once(" at ")?.1.trim().trim_end_matches('.');
    Some(PathBuf::from(path))
}

/// `3 Sep 2026`, in UTC. Close enough for telling files apart.
pub fn format_date(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{day} {} {year}", MONTHS[(month - 1) as usize])
}

/// Days since 1970-01-01 to a calendar date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn lists_folders_first_and_hides_other_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("hacks")).unwrap();
        std::fs::create_dir(dir.path().join(".cache")).unwrap();
        for name in ["b.gba", "A.ZIP", "notes.txt", ".hidden.gba"] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let listing = list(dir.path(), &[".gba", ".zip"], false).unwrap();
        let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["hacks", "A.ZIP", "b.gba"]);
        assert_eq!(listing.hidden, 1);

        let all = list(dir.path(), &[".gba"], true).unwrap();
        assert_eq!(all.entries.len(), 4);
        assert_eq!(all.hidden, 0);
    }

    #[test]
    fn reads_removable_drives_from_lsblk() {
        let text = concat!(
            "PATH=\"/dev/sda\" LABEL=\"\" SIZE=\"2000398934016\" FSTYPE=\"\" MOUNTPOINT=\"\" HOTPLUG=\"0\" TYPE=\"disk\"\n",
            "PATH=\"/dev/sda1\" LABEL=\"HDD-1\" SIZE=\"2000398826496\" FSTYPE=\"ext4\" MOUNTPOINT=\"/mnt/hdda\" HOTPLUG=\"0\" TYPE=\"part\"\n",
            "PATH=\"/dev/sdb\" LABEL=\"\" SIZE=\"32017047552\" FSTYPE=\"\" MOUNTPOINT=\"\" HOTPLUG=\"1\" TYPE=\"disk\"\n",
            "PATH=\"/dev/sdb1\" LABEL=\"SANDISK\" SIZE=\"32016998400\" FSTYPE=\"vfat\" MOUNTPOINT=\"/media/me/SANDISK\" HOTPLUG=\"1\" TYPE=\"part\"\n",
            "PATH=\"/dev/sdc1\" LABEL=\"MY\\x20GAMES\" SIZE=\"16000000000\" FSTYPE=\"exfat\" MOUNTPOINT=\"\" HOTPLUG=\"1\" TYPE=\"part\"\n",
        );
        let drives = drives_from_lsblk(text);
        assert_eq!(drives.len(), 2);
        assert_eq!(drives[0].name, "SANDISK");
        assert_eq!(drives[0].root(), Some(Path::new("/media/me/SANDISK")));
        assert_eq!(drives[1].name, "MY GAMES");
        assert_eq!(drives[1].detail, "Not mounted");
        assert_eq!(drives[1].kind, PlaceKind::Drive { device: "/dev/sdc1".into(), mount: None });
    }

    #[test]
    fn reads_the_mount_point_from_udisksctl() {
        assert_eq!(
            mount_point_from_udisksctl("Mounted /dev/sdc1 at /media/me/MY GAMES\n"),
            Some(PathBuf::from("/media/me/MY GAMES"))
        );
        assert_eq!(mount_point_from_udisksctl("Mounted /dev/sdc1 at /media/me/STICK.\n"), Some(PathBuf::from("/media/me/STICK")));
        assert_eq!(mount_point_from_udisksctl("Error mounting /dev/sdc1: Not authorized"), None);
    }

    #[test]
    fn formats_dates() {
        assert_eq!(format_date(UNIX_EPOCH), "1 Jan 1970");
        assert_eq!(format_date(UNIX_EPOCH + Duration::from_secs(1_788_393_600)), "3 Sep 2026");
        assert_eq!(format_date(UNIX_EPOCH + Duration::from_secs(951_782_400)), "29 Feb 2000");
    }

    #[test]
    fn matches_ignore_case() {
        assert!(matches("Golden Sun.GBA", &[".gba"]));
        assert!(!matches("Golden Sun.gba.txt", &[".gba"]));
    }
}
