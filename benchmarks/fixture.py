"""Create a new 101,000-file scan fixture without overwriting existing data."""
from pathlib import Path
import argparse

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("path", type=Path)
args = parser.parse_args()
args.path.mkdir(parents=True, exist_ok=False)
for index in range(1000):
    project = args.path / f"project-{index}"
    project.mkdir()
    (project / "target").mkdir()
    (project / "Cargo.toml").write_bytes(b"[package]\n")
    for file_index in range(100):
        (project / f"file-{file_index}.rs").write_bytes(b"x" * 128)
print(f"Created 101,000 files and 2,001 directories in {args.path}")
