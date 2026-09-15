#!/usr/bin/env python3
"""Run portable codec coverage with required reference decoding, preserving failures."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import time

import reference_tools

ROOT = Path(__file__).resolve().parents[1]
SETS = ("ci-smoke", "regression", "multictu-regression", "unusual-geometry-smoke", "pipeline-smoke")
SOURCE_FILTER_SETS = {"ci-smoke", "unusual-geometry-smoke", "pipeline-smoke"}


def validation_commands(ff: Path, output: Path):
    """Keep the existing validation runner as the single reconstruction oracle."""
    for codec in ("av2", "vvc"):
        for mode in ("lossless", "lossy"):
            for gop in (0, -1, 3):
                for vector_set in SETS:
                    name = f"{codec}-{mode}-gop{gop}-{vector_set}"
                    directory = output / name
                    command = [
                        sys.executable, str(ROOT / "scripts/run_validation_set.py"),
                        vector_set, "--ff", str(ff), "--codec", codec,
                        "--reference-mode", "required", "--setting", f"gop={gop}",
                        "--vector-dir", str(output / "vectors"),
                        "--encoded-dir", str(directory / "encoded"),
                        "--recon-dir", str(directory / "recon"),
                        "--log-dir", str(directory / "logs"),
                        "--cleanup-output", "--cleanup-recon",
                    ]
                    if mode == "lossless":
                        command.append("--force-lossless")
                    else:
                        command += ["--force-lossy", "--setting", "qp=24"]
                    if vector_set in SOURCE_FILTER_SETS:
                        command.append("--source-filters")
                    yield name, command


def run_commands(commands, output: Path, env: dict[str, str]) -> int:
    """Run every group, record every failure, and fail the aggregate gate."""
    output.mkdir(parents=True, exist_ok=True)
    results = []
    for name, command in commands:
        log = output / f"{name}.log"
        print(f"CI: {name}", flush=True)
        with log.open("w") as stream:
            stream.write(f"$ {shlex.join(command)}\n\n")
            stream.flush()
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
        results.append({"name": name, "status": "PASS" if result.returncode == 0 else "FAIL",
                        "returncode": result.returncode, "log": str(log)})
        (output / "results.json").write_text(json.dumps(results, indent=2) + "\n")
        print(f"  {results[-1]['status']}: {log}", flush=True)
    return int(not results or any(row["returncode"] != 0 for row in results))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ff", type=Path, default=ROOT / "ff")
    parser.add_argument("--output", type=Path, default=ROOT / "verification/generated/ci")
    parser.add_argument("--cargo", default="cargo")
    args = parser.parse_args()
    # A rerun must not overwrite the first failure's diagnostics.
    output = args.output.resolve() / f"run-{time.time_ns()}"
    env = os.environ.copy()
    manifests = reference_tools.load_manifests(reference_tools.DEFAULT_MANIFEST_DIR)
    # No implicit download/build and no optional decoder: missing tools fail CI.
    for codec in ("av2", "vvc"):
        decoder = reference_tools.resolve_tool(manifests[codec], "decoder", no_build=True)
        env[f"FRAMEFINERY_{codec.upper()}_DECODER"] = decoder
    env["FRAMEFINERY_TEST_ARTIFACT_DIR"] = str(output / "sessions")
    commands = [("reference-sessions", [
        *shlex.split(args.cargo), "test", "--locked", "--release", "-p", "framefinery-codecs",
        "--features", "av2 vvc", "--test", "av2_reference_streaming",
        "--test", "vvc_reference_streaming", "--test", "vvc_reference_scc",
        "--test", "prediction_preservation", "--", "--include-ignored", "--test-threads=1",
    ])]
    commands.extend(validation_commands(args.ff.resolve(), output))
    status = run_commands(commands, output, env)
    if status == 0:
        # Logs/checksums remain; these are disposable inputs and decoded prefixes
        # owned by this successful run, never the persistent six-vector sources.
        for directory in (output / "sessions", output / "vectors"):
            if directory.exists():
                shutil.rmtree(directory)
    print(f"CI {'FAIL' if status else 'PASS'}: {output / 'results.json'}", flush=True)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
