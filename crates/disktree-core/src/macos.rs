//! macOS volume discovery and Finder's recoverable trash, without shell
//! interpolation or a dependency on Linux desktop utilities.

use std::ffi::OsString;
use std::io;
use std::mem::{MaybeUninit, size_of};
use std::os::unix::ffi::OsStringExt as _;
use std::path::{Path, PathBuf};

use objc2::rc::autoreleasepool;
use objc2_foundation::{NSFileManager, NSURL};

use crate::space::Mount;

const DATA: &str = "/System/Volumes/Data";

fn native_path(bytes: &[std::ffi::c_char]) -> PathBuf {
    let bytes = bytes
        .iter()
        .take_while(|&&byte| byte != 0)
        .map(|&byte| byte.cast_unsigned())
        .collect();
    PathBuf::from(OsString::from_vec(bytes))
}

pub fn volume_root(path: &Path) -> Option<PathBuf> {
    let stat = rustix::fs::statfs(path).ok()?;
    Some(native_path(&stat.f_mntonname))
}

pub fn device(path: &Path) -> Option<String> {
    let stat = rustix::fs::statfs(path).ok()?;
    Some(
        native_path(&stat.f_mntfromname)
            .to_string_lossy()
            .into_owned(),
    )
}

/// Read cached mount records without spawning a process or querying remote
/// volumes. Extra capacity handles mounts appearing between the two calls.
#[allow(
    unsafe_code,
    reason = "getfsstat initializes only the returned prefix of a sized, aligned allocation"
)]
fn mounts() -> Option<Vec<Mount>> {
    // SAFETY: A null buffer with length zero asks only for the mount count.
    let count =
        unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    let mut capacity = usize::try_from(count).ok()?.checked_add(8)?;
    for _ in 0..3 {
        let mut records = vec![MaybeUninit::<libc::statfs>::uninit(); capacity];
        let bytes =
            i32::try_from(capacity.checked_mul(size_of::<libc::statfs>())?)
                .ok()?;
        // SAFETY: The allocation has capacity aligned statfs slots and the
        // supplied byte count is its exact size. No uninitialized slot is read.
        let count = unsafe {
            libc::getfsstat(
                records.as_mut_ptr().cast(),
                bytes,
                libc::MNT_NOWAIT,
            )
        };
        let count = usize::try_from(count).ok()?;
        if count >= capacity {
            capacity = capacity.checked_mul(2)?;
            continue;
        }
        let mut mounts = Vec::with_capacity(count);
        for record in records.into_iter().take(count) {
            // SAFETY: getfsstat succeeded and filled this returned-prefix slot.
            let record = unsafe { record.assume_init() };
            mounts.push(Mount {
                source: native_path(&record.f_mntfromname)
                    .to_string_lossy()
                    .into_owned(),
                point: native_path(&record.f_mntonname),
                fstype: native_path(&record.f_fstypename)
                    .to_string_lossy()
                    .into_owned(),
                options: String::new(),
            });
        }
        let links =
            std::fs::read_to_string("/usr/share/firmlinks").unwrap_or_default();
        add_firmlink_aliases(&mut mounts, &links);
        return (!mounts.is_empty()).then_some(mounts);
    }
    None
}

#[cfg(test)]
fn parse_mounts(table: &str) -> Vec<Mount> {
    table
        .lines()
        .filter_map(|line| {
            let (location, flags) = line.rsplit_once(" (")?;
            let (source, point) = location.split_once(" on ")?;
            let options = flags.strip_suffix(')')?;
            let fstype = options.split(',').next()?.trim();
            Some(Mount {
                source: source.into(),
                point: point.into(),
                fstype: fstype.into(),
                options: options.into(),
            })
        })
        .collect()
}

/// A mount below /Users is also reachable below Data/Users (and vice
/// versa). Exclude both spellings before opening a directory, including
/// mounts such as `CoreSimulator` images and network shares inside a home.
fn add_firmlink_aliases(mounts: &mut Vec<Mount>, links: &str) {
    let mut aliases = Vec::new();
    for line in links.lines() {
        let Some((visible, relative)) = line.split_once('\t') else {
            continue;
        };
        let physical = Path::new(DATA).join(relative);
        for mount in mounts.iter() {
            let alias = if let Ok(tail) = mount.point.strip_prefix(&physical) {
                Some(Path::new(visible).join(tail))
            } else if let Ok(tail) = mount.point.strip_prefix(visible) {
                Some(physical.join(tail))
            } else {
                None
            };
            if let Some(point) = alias {
                aliases.push(Mount {
                    point,
                    ..mount.clone()
                });
            }
        }
    }
    mounts.extend(aliases);
}

pub fn foreign_mounts(root: &Path) -> Option<Vec<PathBuf>> {
    let own = device(root)?;
    Some(
        mounts()?
            .into_iter()
            .filter(|mount| {
                mount.point != root && mount.point.starts_with(root)
            })
            .filter(|mount| mount.source != own)
            .map(|mount| mount.point)
            .collect(),
    )
}

pub fn mounts_below(root: &Path) -> Option<bool> {
    Some(
        mounts()?
            .iter()
            .any(|mount| mount.point != root && mount.point.starts_with(root)),
    )
}

/// Removal guards compare the same logical spelling for the APFS Data
/// volume and its firmlinks. Case folding prevents bypasses on the default
/// case-insensitive macOS filesystem; it is deliberately conservative on
/// case-sensitive volumes.
pub fn guard_path(path: &Path) -> PathBuf {
    let logical = path
        .strip_prefix(DATA)
        .map_or_else(|_| path.to_path_buf(), |tail| Path::new("/").join(tail));
    PathBuf::from(logical.to_string_lossy().to_lowercase())
}

pub fn trash(path: &Path) -> io::Result<PathBuf> {
    autoreleasepool(|_| {
        // Keep the final component intact: trashing a symlink must move the
        // link, not its target. Foundation accepts directories as file URLs.
        let url = NSURL::from_file_path(path)
            .ok_or_else(|| io::Error::other("invalid file path for Trash"))?;
        let mut resulting = None;
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
            .map_err(|error| io::Error::other(error.to_string()))?;
        resulting.and_then(|url| url.to_file_path()).ok_or_else(|| {
            io::Error::other("Trash did not return its location")
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_names_with_spaces_and_parentheses_are_preserved() {
        let mounts = parse_mounts(
            "/dev/disk1 on / (apfs, local)\n\
/dev/disk2 on /Volumes/My Disk (backup) (apfs, local, journaled)\n\
map auto_home on /System/Volumes/Data/home (autofs, automounted)\n",
        );
        assert_eq!(mounts.len(), 3);
        assert_eq!(mounts[1].point, Path::new("/Volumes/My Disk (backup)"));
        assert_eq!(mounts[2].source, "map auto_home");
    }

    #[test]
    fn mounted_disks_are_excluded_through_both_firmlink_paths() {
        let mut mounts = parse_mounts(
            "nas:/share on /Users/me/NAS (nfs, local)\n\
/dev/disk2 on /System/Volumes/Data/Library/Simulator (apfs, local)\n",
        );
        add_firmlink_aliases(&mut mounts, "/Users\tUsers\n/Library\tLibrary\n");
        assert!(
            mounts
                .iter()
                .any(|m| m.point
                    == Path::new("/System/Volumes/Data/Users/me/NAS"))
        );
        assert!(
            mounts
                .iter()
                .any(|m| m.point == Path::new("/Library/Simulator"))
        );
    }

    #[test]
    fn native_volume_discovery_handles_the_data_volume() {
        let temp = std::env::temp_dir();
        let root = volume_root(&temp).expect("mounted temp directory");
        assert!(root.is_absolute());
        assert_eq!(device(&temp), device(&root));
        assert!(foreign_mounts(&root).is_some());
    }

    #[test]
    fn data_aliases_have_the_same_guard_path() {
        assert_eq!(
            guard_path(Path::new("/System/Volumes/Data/Users/Me")),
            guard_path(Path::new("/Users/me"))
        );
    }

    #[test]
    #[ignore = "moves only its own fixture into Finder Trash, then restores it"]
    fn native_trash_round_trip_preserves_a_symlink_target() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("keep.txt");
        std::fs::write(&target, "keep").expect("write");
        let link = temp.path().join("disktree-trash-test-link");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        let trashed = trash(&link).expect("native trash");
        assert!(!link.exists());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
        std::fs::rename(&trashed, &link).expect("restore fixture");
        assert!(link.is_symlink());
    }
}
