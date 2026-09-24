//! Reproducible scan/classification timings; optional path uses real metadata.
use disktree_core::{
    classify::classify,
    scan::{ScanOptions, scan},
    tree::{Node, NodeKind},
};
use std::hint::black_box;
use std::time::Instant;

fn main() {
    let path = std::env::args_os().nth(1);
    if let Some(path) = path {
        for round in 0..6 {
            let start = Instant::now();
            let tree =
                scan(std::path::Path::new(&path), ScanOptions::default())
                    .unwrap();
            println!(
                "scan {round}: {:.3} ms; {} files; {} bytes",
                start.elapsed().as_secs_f64() * 1000.0,
                tree.files,
                tree.bytes
            );
            black_box(tree);
        }
    } else {
        let mut root = Node::directory("fixture");
        for d in 0..1000 {
            let mut dir = Node::directory(format!("project-{d}"));
            dir.children
                .push(Node::entry("Cargo.toml", NodeKind::File, 100));
            dir.children.push(Node::directory("target"));
            for f in 0..200 {
                dir.children.push(Node::entry(
                    format!("file-{f}.rs"),
                    NodeKind::File,
                    4096,
                ));
            }
            root.children.push(dir);
        }
        for round in 0..8 {
            let start = Instant::now();
            classify(black_box(&mut root));
            println!(
                "classify {round}: {:.3} ms",
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
}
