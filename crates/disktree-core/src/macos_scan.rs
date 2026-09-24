//! Darwin's bulk directory metadata path. APFS can vend a batch directly
//! from its catalog, avoiding a path allocation and `lstat` for each file.
//!
//! Unsupported filesystems fall back before yielding any entries. Missing
//! per-entry attributes use ordinary metadata; partial directory errors are
//! reported, never restarted in a way that could count entries twice.

use std::cell::RefCell;
use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd as _;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

// sys/attr.h: libc does not expose ATTR_CMN_ERROR yet.
const ATTR_CMN_ERROR: u32 = 0x2000_0000;
const COMMON: u32 = libc::ATTR_CMN_NAME
    | libc::ATTR_CMN_DEVID
    | libc::ATTR_CMN_OBJTYPE
    | libc::ATTR_CMN_MODTIME
    | libc::ATTR_CMN_FILEID;
const FILE: u32 = libc::ATTR_FILE_LINKCOUNT
    | libc::ATTR_FILE_ALLOCSIZE
    | libc::ATTR_FILE_DATALENGTH;

// One buffer per active scanner worker, reused across directories. Records
// require eight-byte alignment; their individual fields use four-byte packing.
#[repr(align(8))]
struct Buffer([u8; 64 * 1024]);
thread_local! {
    static BUFFER: RefCell<Box<Buffer>> =
        RefCell::new(Box::new(Buffer([0; 64 * 1024])));
}

#[derive(Debug)]
pub struct Entry<'a> {
    pub name: &'a OsStr,
    pub metadata: Option<Metadata>,
}

#[derive(Debug)]
pub struct Metadata {
    /// Darwin vnode types: VREG = 1, VDIR = 2, VLNK = 5.
    pub kind: u32,
    pub device: u64,
    pub inode: u64,
    pub links: u32,
    pub modified: i64,
    pub allocated: u64,
    pub apparent: u64,
}

/// Returns false only if bulk enumeration is unsupported, before visiting
/// anything. Cancellation is checked between both entries and system calls.
pub fn visit(
    path: &Path,
    cancelled: impl Fn() -> bool,
    mut visitor: impl FnMut(Entry<'_>),
) -> io::Result<bool> {
    let directory = File::open(path)?;
    BUFFER.with_borrow_mut(|buffer| {
        let mut first = true;
        loop {
            if cancelled() {
                return Ok(true);
            }
            let count = match read_batch(&directory, buffer) {
                Ok(count) => count,
                Err(error)
                    if first
                        && matches!(
                            error.raw_os_error(),
                            Some(libc::ENOTSUP | libc::ENOSYS | libc::EINVAL)
                        ) =>
                {
                    return Ok(false);
                }
                Err(error) => return Err(error),
            };
            first = false;
            if count == 0 {
                return Ok(true);
            }
            let mut remaining = buffer.0.as_slice();
            for _ in 0..count {
                if cancelled() {
                    return Ok(true);
                }
                let length = usize::try_from(u32_at(remaining, 0)?)
                    .map_err(|_| invalid_record())?;
                if length < 72 || length % 8 != 0 {
                    return Err(invalid_record());
                }
                let record =
                    remaining.get(..length).ok_or_else(invalid_record)?;
                visitor(parse(record)?);
                remaining = &remaining[length..];
            }
        }
    })
}

#[allow(
    unsafe_code,
    reason = "bounded Darwin syscall with a live directory fd and aligned, writable buffer"
)]
fn read_batch(directory: &File, buffer: &mut Buffer) -> io::Result<usize> {
    let mut attributes = libc::attrlist {
        bitmapcount: 5,
        reserved: 0,
        commonattr: COMMON | ATTR_CMN_ERROR | libc::ATTR_CMN_RETURNED_ATTRS,
        volattr: 0,
        dirattr: 0,
        fileattr: FILE,
        forkattr: 0,
    };
    // SAFETY: File owns the open fd; both pointers are valid for the entire
    // call. The kernel receives the exact writable buffer length. No pointers
    // into it escape the visitor or overlap the next call.
    let count = unsafe {
        libc::getattrlistbulk(
            directory.as_raw_fd(),
            (&raw mut attributes).cast(),
            buffer.0.as_mut_ptr().cast(),
            buffer.0.len(),
            u64::from(libc::FSOPT_PACK_INVAL_ATTRS),
        )
    };
    usize::try_from(count).map_err(|_| io::Error::last_os_error())
}

fn invalid_record() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid bulk metadata record")
}

fn bytes_at<const N: usize>(
    record: &[u8],
    offset: usize,
) -> io::Result<[u8; N]> {
    record
        .get(offset..offset + N)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(invalid_record)
}

fn u32_at(record: &[u8], offset: usize) -> io::Result<u32> {
    bytes_at(record, offset).map(u32::from_ne_bytes)
}

fn u64_at(record: &[u8], offset: usize) -> io::Result<u64> {
    bytes_at(record, offset).map(u64::from_ne_bytes)
}

fn parse(record: &[u8]) -> io::Result<Entry<'_>> {
    // PACK_INVAL_ATTRS reserves requested fields within an attribute group.
    // Darwin omits the entire file group for directories.
    // length(4), returned attribute masks(20), error(4), name reference(8),
    // device(4), vnode type(4), timespec(16), file id(8), link count(4),
    // allocated size(8), data length(8). Variable name data follows.
    let common = u32_at(record, 4)?;
    let file = u32_at(record, 16)?;
    if common & libc::ATTR_CMN_NAME == 0 {
        return Err(invalid_record());
    }
    let offset = i32::from_ne_bytes(bytes_at(record, 28)?);
    let start = 28_usize
        .checked_add_signed(offset as isize)
        .ok_or_else(invalid_record)?;
    let length = u32_at(record, 32)? as usize;
    let end = start.checked_add(length).ok_or_else(invalid_record)?;
    let name = record
        .get(start..end)
        .and_then(|name| name.strip_suffix(&[0]))
        .filter(|name| {
            !name.is_empty()
                && *name != b"."
                && *name != b".."
                && !name.contains(&0)
                && !name.contains(&b'/')
        })
        .ok_or_else(invalid_record)?;
    let kind = u32_at(record, 40)?;
    let metadata = if u32_at(record, 24)? == 0
        && common & COMMON == COMMON
        && (kind == 2 || file & FILE == FILE)
    {
        Some(Metadata {
            kind,
            // dev_t is signed on Darwin; match MetadataExt::dev's cast.
            device: i64::from(i32::from_ne_bytes(bytes_at(record, 36)?))
                .cast_unsigned(),
            modified: i64::from_ne_bytes(bytes_at(record, 44)?).max(0),
            inode: u64_at(record, 60)?,
            links: if kind == 2 { 0 } else { u32_at(record, 68)? },
            allocated: if kind == 2 { 0 } else { u64_at(record, 72)? },
            apparent: if kind == 2 { 0 } else { u64_at(record, 80)? },
        })
    } else {
        None
    };
    Ok(Entry {
        name: OsStr::from_bytes(name),
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::os::unix::fs::MetadataExt as _;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn bulk_batches_match_lstat_for_names_sizes_times_and_hardlinks() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        // More than one 64 KiB batch, with mixed vnode types and Unicode names.
        for index in 0..1600 {
            fs::write(
                root.join(format!("entry-{index:04}-long-name.bin")),
                b"data",
            )
            .unwrap();
        }
        fs::create_dir(root.join("directory")).unwrap();
        fs::write(root.join("unicode-文書-🌲"), b"bytes").unwrap();
        let sparse = File::create(root.join("sparse")).unwrap();
        sparse.set_len(16 * 1024 * 1024).unwrap();
        fs::hard_link(root.join("sparse"), root.join("hardlink")).unwrap();
        std::os::unix::fs::symlink("sparse", root.join("symlink")).unwrap();
        // Resource forks affect allocated bytes, but not the data fork's length.
        fs::write(root.join("forked"), b"data").unwrap();
        fs::write(root.join("forked/..namedfork/rsrc"), vec![1_u8; 8192])
            .unwrap();
        let expected: HashSet<_> = fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        let mut seen = HashSet::new();
        let mut fast_files = 0;
        assert!(
            visit(
                root,
                || false,
                |entry| {
                    assert!(
                        seen.insert(entry.name.to_os_string()),
                        "duplicate entry"
                    );
                    let meta =
                        fs::symlink_metadata(root.join(entry.name)).unwrap();
                    if meta.is_file() {
                        let bulk = entry
                            .metadata
                            .expect("APFS supplies regular-file metadata");
                        assert_eq!(bulk.kind, 1);
                        assert_eq!(bulk.apparent, meta.len());
                        assert_eq!(bulk.allocated, meta.blocks() * 512);
                        assert_eq!(bulk.device, meta.dev());
                        assert_eq!(bulk.inode, meta.ino());
                        assert_eq!(u64::from(bulk.links), meta.nlink());
                        assert_eq!(bulk.modified, meta.mtime().max(0));
                        fast_files += 1;
                    } else if meta.is_dir() {
                        assert_eq!(entry.metadata.unwrap().kind, 2);
                    }
                }
            )
            .unwrap()
        );
        assert_eq!(seen, expected);
        assert_eq!(fast_files, 1604);
    }

    #[test]
    fn cancellation_stops_in_the_middle_of_a_batch() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..30 {
            fs::write(temp.path().join(index.to_string()), []).unwrap();
        }
        let visited = AtomicUsize::new(0);
        assert!(
            visit(
                temp.path(),
                || visited.load(Ordering::Relaxed) == 7,
                |_| {
                    visited.fetch_add(1, Ordering::Relaxed);
                }
            )
            .unwrap()
        );
        assert_eq!(visited.load(Ordering::Relaxed), 7);
    }

    #[test]
    fn invalid_and_truncated_records_fail_without_panicking() {
        for length in 0..128 {
            assert!(parse(&vec![0; length]).is_err());
        }
        let mut record = vec![0; 96];
        record[4..8].copy_from_slice(&libc::ATTR_CMN_NAME.to_ne_bytes());
        record[28..32].copy_from_slice(&i32::MAX.to_ne_bytes());
        record[32..36].copy_from_slice(&u32::MAX.to_ne_bytes());
        assert!(parse(&record).is_err());
    }
}
