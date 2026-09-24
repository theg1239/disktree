# Performance and validation

Measured on an Apple M4 Mac running macOS 27.0 (26A5388g), with APFS,
Rust 1.97.1, and release builds. The comparison is against upstream commit
`59096e4`, using the same benchmark executable source in both checkouts.

| Measurement | Upstream | macOS port | Change |
| --- | ---: | ---: | ---: |
| Scan, 101,000 files | 115.12 ms | 89.95 ms | 22% less time |
| Classify, 203,001 nodes | 7.31 ms | 2.02 ms | 3.6× faster |
| Scan process peak RSS | 35.64 MiB | 31.09 MiB | 13% lower |

These are warm medians, not cold disk or whole-volume promises. Six process
pairs alternate order. Each process's first sample is excluded; there are
30 scan timings, 42 classification timings, and six RSS readings per version.
The scan fixture contains 1,000 project directories, each with 100 small
files, a Cargo.toml, and an empty target directory. Both versions reported
101,000 files and 413,696,000 allocated bytes. Background desktop activity
introduces variance; raw summary ranges are in `benchmark-results.json`.
No system cache purge or privileged tuning was used.

## Changes that remove work

- Darwin `getattrlistbulk` fetches metadata in 64 KiB batches. Complete file
  and unfollowed-link records need no individual `lstat` or full-path
  allocation. Aligned buffers are reused per worker.
- Four reusable filesystem workers reduce contention and leave CPU time for
  the UI. `DISKTREE_SCAN_THREADS=1..64` overrides this for storage-specific
  experiments. Linux retains its existing Rayon policy.
- Native `getfsstat(MNT_NOWAIT)` reads cached mount records, replacing a
  subprocess. Firmlink aliases preserve foreign-volume exclusions.
- Progress atomics are flushed every 128 entries. Error strings are retained
  only for the first 50 failures. The identity set stores only possible
  hardlinks, unless following symlinks requires tracking all identities.
- Classification inspects manifest names once per directory. Rendering borrows
  the cached layout, skips offscreen tiles, and partially selects the largest
  150 labels. Metric changes avoid copying an exclusively owned tree.
- Footer label, count, size/time, and zoom fields reserve their widths;
  tabular figures prevent glyph-width changes within counters.

## Reproduce

Build the example in this checkout and in an upstream checkout after copying
`crates/disktree-core/examples/bench.rs` into the corresponding directory:

```sh
MACOSX_DEPLOYMENT_TARGET=12.0 cargo build --release -p disktree-core --example bench
python3 benchmarks/fixture.py /tmp/disktree-scan-fixture
python3 benchmarks/compare.py /path/to/upstream/target/release/examples/bench \
  target/release/examples/bench /tmp/disktree-scan-fixture
```

The fixture script requires a new directory and never overwrites an existing
one. The comparison rejects mismatched file counts or byte totals.

## Correctness and limits

`cargo xtask lint` passes. All 48 application and 88 core tests pass;
two intentionally ignored integration tests remain opt-in. The Finder Trash
round-trip test was also run explicitly and passed.

Tests compare native metadata with `lstat` across multiple batches, Unicode
names, directories, sparse files, resource forks, hardlinks, and symlinks.
They cover cancellation, invalid records, hidden files, depth limits,
followed-link cycles, volume/removal guards, and footer geometry during
scanning and completion. Native unsupported filesystems fall back before
emitting entries; missing per-entry attributes use ordinary metadata. A
partial read failure is reported instead of restarting and double-counting.

Allocated sizes are filesystem-reported allocation, not exclusive ownership
of APFS extents: clones, snapshots, compression, and cloud placeholders can
make actual reclaimed space differ. Permission failures remain visible; the
scanner does not request root access or silently omit protected folders.
