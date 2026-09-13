#!/usr/bin/env python3
"""Rebuild the retained synthetic helper using the recorded dependency artifacts."""
import argparse
import json
import pathlib
import subprocess
import tempfile

parser = argparse.ArgumentParser()
parser.add_argument("env_file", type=pathlib.Path)
parser.add_argument("--target", type=pathlib.Path, help="Matching backend Cargo target directory")
args = parser.parse_args()
here = pathlib.Path(__file__).resolve().parent
repo = here.parents[4]
deps = (args.target or repo / "backend/target") / "debug/deps"
artifacts = json.loads((here / "artifact-inputs.json").read_text())
missing = [name for name in artifacts.values() if not (deps / name).is_file()]
if missing:
    raise SystemExit("Recorded dependency artifacts unavailable; build this revision with the recorded test-support features before repeating: " + ", ".join(missing))
with tempfile.TemporaryDirectory(prefix="crm-handoff-executable-") as build:
    executable = pathlib.Path(build) / "handoff"
    command = ["rustc", "--edition=2021", "-C", "debuginfo=line-tables-only", str(here / "main.rs"), "-o", str(executable), "-L", f"dependency={deps}"]
    for crate, artifact in artifacts.items():
        command.extend(["--extern", f"{crate}={deps / artifact}"])
    subprocess.run(command, check=True, cwd=repo)
    subprocess.run([str(executable), str(args.env_file.resolve()), str(repo)], check=True, cwd=repo)
