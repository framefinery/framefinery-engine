#!/usr/bin/env python3
"""Generate the release performance table for FrameFinery encoders."""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
import time
import tomllib
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
BENCHMARK_MATRIX = REPO_ROOT / "scripts" / "benchmark_encode_matrix.py"
DEFAULT_SET = "release-aomctc"
DEFAULT_OUT_DIR = REPO_ROOT / "verification" / "generated" / "release_performance"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("set", nargs="?", default=DEFAULT_SET, help="test vector set name")
    parser.add_argument("--ff", type=Path, default=REPO_ROOT / "ff")
    parser.add_argument("--set-dir", type=Path, default=REPO_ROOT / "verification" / "test_vector_sets")
    parser.add_argument("--vector-dir", type=Path, default=REPO_ROOT / "verification" / "generated" / "test_vectors")
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--run-name", default="", help="report label; defaults to version timestamp")
    parser.add_argument("--codec", action="append", choices=("av2", "vvc"), default=[])
    parser.add_argument("--mode", action="append", choices=("lossless", "lossy"), default=[])
    parser.add_argument("--frames", type=parse_positive_int, default=50)
    parser.add_argument(
        "--full-stream",
        action="store_true",
        help="use manifest frame counts instead of the default 50-frame release table",
    )
    parser.add_argument("--limit", type=parse_positive_int, default=0)
    parser.add_argument("--av2-lossy-qp", type=parse_qp, default=24)
    parser.add_argument("--vvc-lossy-qp", type=parse_qp, default=19)
    parser.add_argument(
        "--vvc-fast-search",
        choices=("off", "conservative", "moderate", "aggressive", "lossless-speed"),
        default="lossless-speed",
    )
    parser.add_argument("--av2-gop", type=parse_gop, default=-1)
    parser.add_argument("--vvc-gop", type=parse_gop, default=-1)
    parser.add_argument("--keep-bitstreams", action="store_true")
    parser.add_argument("--write-recon", action="store_true")
    parser.add_argument("--cleanup-recon", action="store_true")
    parser.add_argument(
        "--df-path",
        action="append",
        type=Path,
        default=[],
        help="path to include in before/after disk-usage snapshots",
    )
    args = parser.parse_args()

    print_disk_usage("before", disk_paths(args))
    command = benchmark_command(args)
    print("$ " + " ".join(shell_word(part) for part in command), flush=True)
    process = subprocess.run(command, cwd=REPO_ROOT)
    print_disk_usage("after", disk_paths(args))
    return process.returncode


def benchmark_command(args: argparse.Namespace) -> list[str]:
    run_name = args.run_name or f"v{workspace_version()}-{time.strftime('%Y%m%d-%H%M%S')}"
    command = [
        sys.executable,
        str(BENCHMARK_MATRIX),
        args.set,
        "--ff",
        str(args.ff),
        "--set-dir",
        str(args.set_dir),
        "--vector-dir",
        str(args.vector_dir),
        "--out-dir",
        str(args.out_dir),
        "--run-name",
        run_name,
        "--av2-lossy-qp",
        str(args.av2_lossy_qp),
        "--vvc-lossy-qp",
        str(args.vvc_lossy_qp),
        "--vvc-fast-search",
        args.vvc_fast_search,
        "--direct-source-files",
    ]
    if not args.full_stream:
        command.extend(["--frames", str(args.frames)])
    for codec in args.codec:
        command.extend(["--codec", codec])
    for mode in args.mode:
        command.extend(["--mode", mode])
    if args.limit:
        command.extend(["--limit", str(args.limit)])
    command.extend(["--av2-gop", str(args.av2_gop)])
    command.extend(["--vvc-gop", str(args.vvc_gop)])
    if not args.keep_bitstreams:
        command.append("--cleanup-output")
    if args.write_recon:
        command.append("--write-recon")
    if args.cleanup_recon:
        command.append("--cleanup-recon")
    return command


def workspace_version() -> str:
    manifest = tomllib.loads((REPO_ROOT / "Cargo.toml").read_text())
    return manifest["workspace"]["package"]["version"]


def disk_paths(args: argparse.Namespace) -> list[Path]:
    paths = [REPO_ROOT, Path("/media/gabriel/storage")]
    paths.extend(args.df_path)
    unique = []
    seen = set()
    for path in paths:
        key = str(path)
        if key not in seen:
            unique.append(path)
            seen.add(key)
    return unique


def print_disk_usage(label: str, paths: list[Path]) -> None:
    print(f"Disk usage {label}:")
    for path in paths:
        try:
            usage = shutil.disk_usage(path)
        except FileNotFoundError:
            print(f"  {path}: missing")
            continue
        total = format_bytes(usage.total)
        used = format_bytes(usage.used)
        free = format_bytes(usage.free)
        print(f"  {path}: used {used} / {total}, free {free}")
    print(flush=True)


def format_bytes(value: int) -> str:
    size = float(value)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB"):
        if size < 1024.0 or unit == "TiB":
            return f"{size:.1f} {unit}"
        size /= 1024.0
    return f"{size:.1f} TiB"


def parse_positive_int(value: str) -> int:
    try:
        parsed = int(value, 10)
    except ValueError as err:
        raise argparse.ArgumentTypeError(f"expected a positive integer, got '{value}'") from err
    if parsed <= 0:
        raise argparse.ArgumentTypeError(f"expected a positive integer, got '{value}'")
    return parsed


def parse_qp(value: str) -> int:
    parsed = parse_positive_int(value)
    if parsed > 255:
        raise argparse.ArgumentTypeError(f"QP expects an integer from 1 through 255, got '{value}'")
    return parsed


def parse_gop(value: str) -> int:
    try:
        parsed = int(value, 10)
    except ValueError as err:
        raise argparse.ArgumentTypeError(
            f"GOP expects an integer from -1 through 65535, got '{value}'"
        ) from err
    if not (-1 <= parsed <= 65535):
        raise argparse.ArgumentTypeError(
            f"GOP expects an integer from -1 through 65535, got '{value}'"
        )
    return parsed


def shell_word(value: str) -> str:
    if value and all(ch.isalnum() or ch in "/._:-=+" for ch in value):
        return value
    return "'" + value.replace("'", "'\"'\"'") + "'"


if __name__ == "__main__":
    raise SystemExit(main())
