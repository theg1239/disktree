"""Alternate upstream/port processes and report warm medians and macOS RSS."""
from pathlib import Path
import argparse
import json
import re
import statistics
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("upstream", type=Path)
parser.add_argument("macos", type=Path)
parser.add_argument("fixture", type=Path)
args = parser.parse_args()
binaries = {"upstream": args.upstream.resolve(), "macos": args.macos.resolve()}
results = {name: {"scan_ms": [], "classify_ms": [], "rss_bytes": []} for name in binaries}
counts = set()
for iteration in range(6):
    order = list(binaries) if iteration % 2 == 0 else list(reversed(binaries))
    for name in order:
        for mode in ["scan", "classify"]:
            command = ["/usr/bin/time", "-l", str(binaries[name])]
            if mode == "scan":
                command.append(str(args.fixture.resolve()))
            completed = subprocess.run(command, text=True, capture_output=True, check=True)
            timings = [float(value) for value in re.findall(r": ([\d.]+) ms", completed.stdout)]
            if len(timings) < 2:
                raise RuntimeError("Missing benchmark samples")
            # Exclude process startup, thread-pool setup and its first scan.
            results[name][f"{mode}_ms"] += timings[1:]
            if mode == "scan":
                counts.update(re.findall(r"; (\d+) files; (\d+) bytes", completed.stdout))
                rss = re.search(r"(\d+)\s+maximum resident set size", completed.stderr)
                results[name]["rss_bytes"].append(int(rss[1]))
if len(counts) != 1:
    raise RuntimeError(f"Scan totals differ: {counts}")
summary = {
    name: {
        key: {"median": statistics.median(values), "min": min(values),
              "max": max(values), "samples": len(values)}
        for key, values in metrics.items()
    }
    for name, metrics in results.items()
}
print(json.dumps(summary, indent=2))
