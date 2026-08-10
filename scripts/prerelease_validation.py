#!/usr/bin/env python3
"""Run the full local pre-release validation checkpoint."""

from __future__ import annotations

import argparse
import os
import re
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUT_DIR = REPO_ROOT / "verification" / "generated" / "pre_release_validation"
DEFAULT_SET_DIR = REPO_ROOT / "verification" / "test_vector_sets"
DEFAULT_VECTOR_DIR = REPO_ROOT / "verification" / "generated" / "test_vectors"
DEFAULT_ENCODED_DIR = REPO_ROOT / "verification" / "generated" / "encoded"
DEFAULT_LOG_DIR = REPO_ROOT / "verification" / "generated" / "validation_logs"
GEOMETRY_SWEEP_SETS = (
    "screenshot-sweep-444",
    "screenshot-sweep-444-10bit",
    "screenshot-sweep-420-10bit-canary",
)
CODECS = ("av2", "vvc")
MODES = ("lossless", "lossy")
REGRESSION_SETS = (
    ("unusual-geometry-smoke", True),
    ("regression", False),
    ("multictu-regression", False),
)


@dataclass(frozen=True)
class RunRecord:
    name: str
    command: list[str]
    log: Path
    status: str
    seconds: float


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ff", type=Path, default=REPO_ROOT / "ff")
    parser.add_argument("--aomctc-root", default=os.environ.get("AOMCTC_ROOT", ""))
    parser.add_argument("--set-dir", type=Path, default=DEFAULT_SET_DIR)
    parser.add_argument("--vector-dir", type=Path, default=DEFAULT_VECTOR_DIR)
    parser.add_argument("--encoded-dir", type=Path, default=DEFAULT_ENCODED_DIR)
    parser.add_argument("--validation-log-dir", type=Path, default=DEFAULT_LOG_DIR)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--run-name", default=time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()))
    parser.add_argument("--reference-mode", choices=("auto", "required", "off"), default="required")
    parser.add_argument("--av2-lossy-qp", type=int, default=24)
    parser.add_argument("--vvc-lossy-qp", type=int, default=19)
    parser.add_argument("--vvc-fast-search", default="lossless-speed")
    parser.add_argument("--six-vector-set", default="release-six-vectors-full")
    parser.add_argument("--six-vector-baseline-json", type=Path)
    parser.add_argument(
        "--report-only",
        action="store_true",
        help="regenerate the combined Markdown report from existing logs and matrix reports",
    )
    args = parser.parse_args()

    if not args.ff.exists():
        print(f"error: missing CLI binary: {args.ff}; run make build first", file=sys.stderr)
        return 2
    if not args.aomctc_root:
        print(
            "error: AOMCTC_ROOT is required; pass AOMCTC_ROOT=/path/to/aomctc",
            file=sys.stderr,
        )
        return 2
    aomctc_root = Path(args.aomctc_root).expanduser()
    if not aomctc_root.exists():
        print(f"error: AOMCTC_ROOT does not exist: {aomctc_root}", file=sys.stderr)
        return 2

    run_dir = (args.out_dir / args.run_name).resolve()
    run_log_dir = run_dir / "logs"
    matrix_out_dir = run_dir / "encode_matrix"
    run_log_dir.mkdir(parents=True, exist_ok=True)
    matrix_out_dir.mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    env["AOMCTC_ROOT"] = str(aomctc_root)

    if args.report_only:
        records = existing_records(args, run_log_dir, matrix_out_dir)
        report = write_report(args, run_dir, records, matrix_out_dir)
        failed = [record for record in records if record.status != "PASS"]
        if failed:
            print(f"\nFAIL: {len(failed)} existing validation artifact(s) are missing or failed")
            print(f"wrote {relpath(report)}")
            return 1
        print("\nOK: pre-release validation report regenerated")
        print(f"wrote {relpath(report)}")
        return 0

    records: list[RunRecord] = []
    try:
        for command_name, command in validation_commands(args):
            records.append(run_logged(command_name, command, run_log_dir, env))
        matrix_name = f"{args.run_name}-six-vectors-full"
        matrix_command = six_vector_command(args, matrix_out_dir, matrix_name)
        records.append(run_logged("six-vector-matrix", matrix_command, run_log_dir, env))
    except subprocess.CalledProcessError as err:
        records.append(
            RunRecord(
                name=err.cmd_name,  # type: ignore[attr-defined]
                command=err.cmd if isinstance(err.cmd, list) else [str(err.cmd)],
                log=err.log_path,  # type: ignore[attr-defined]
                status="FAIL",
                seconds=err.seconds,  # type: ignore[attr-defined]
            )
        )
        write_report(args, run_dir, records, matrix_out_dir)
        print(f"\nFAIL: {err.cmd_name}; see {relpath(err.log_path)}", file=sys.stderr)  # type: ignore[attr-defined]
        return err.returncode or 1

    report = write_report(args, run_dir, records, matrix_out_dir)
    print(f"\nOK: full pre-release validation completed")
    print(f"wrote {relpath(report)}")
    return 0


def validation_commands(args: argparse.Namespace) -> list[tuple[str, list[str]]]:
    commands: list[tuple[str, list[str]]] = []
    for set_name in GEOMETRY_SWEEP_SETS:
        for codec in CODECS:
            for mode in MODES:
                commands.append(
                    (
                        f"geometry-{set_name}-{codec}-{mode}",
                        validation_command(args, set_name, codec, mode),
                    )
                )
    for set_name, source_filters in REGRESSION_SETS:
        for codec in CODECS:
            for mode in MODES:
                commands.append(
                    (
                        f"{set_name}-{codec}-{mode}",
                        validation_command(args, set_name, codec, mode, source_filters=source_filters),
                    )
                )
    for codec in CODECS:
        for mode in MODES:
            commands.append(
                (
                    f"release-aomctc-{codec}-{mode}",
                    validation_command(
                        args,
                        "release-aomctc",
                        codec,
                        mode,
                        direct_source_files=True,
                    ),
                )
            )
    return commands


def validation_command(
    args: argparse.Namespace,
    set_name: str,
    codec: str,
    mode: str,
    *,
    source_filters: bool = False,
    direct_source_files: bool = False,
) -> list[str]:
    command = [
        sys.executable,
        "scripts/run_validation_set.py",
        "--ff",
        str(args.ff.resolve()),
        "--codec",
        codec,
        set_name,
        "--set-dir",
        str(args.set_dir),
        "--vector-dir",
        str(args.vector_dir),
        "--encoded-dir",
        str(args.encoded_dir),
        "--log-dir",
        str(args.validation_log_dir),
        "--reference-mode",
        args.reference_mode,
        "--setting",
        "gop=-1",
        "--cleanup-recon",
        "--cleanup-output",
        "--cleanup-vectors",
        "--stop-on-fail",
    ]
    if source_filters:
        command.append("--source-filters")
    if direct_source_files:
        command.append("--direct-source-files")
    if codec == "vvc" and args.vvc_fast_search != "off":
        command.extend(["--setting", f"fast-search={args.vvc_fast_search}"])
    if mode == "lossy":
        command.append("--force-lossy")
        qp = args.av2_lossy_qp if codec == "av2" else args.vvc_lossy_qp
        command.extend(["--setting", f"qp={qp}"])
    else:
        command.append("--force-lossless")
    return command


def six_vector_command(args: argparse.Namespace, out_dir: Path, matrix_name: str) -> list[str]:
    command = [
        sys.executable,
        "scripts/benchmark_encode_matrix.py",
        args.six_vector_set,
        "--ff",
        str(args.ff.resolve()),
        "--set-dir",
        str(args.set_dir),
        "--vector-dir",
        str(args.vector_dir),
        "--out-dir",
        str(out_dir),
        "--run-name",
        matrix_name,
        "--av2-lossy-qp",
        str(args.av2_lossy_qp),
        "--vvc-lossy-qp",
        str(args.vvc_lossy_qp),
        "--vvc-fast-search",
        args.vvc_fast_search,
        "--av2-gop",
        "-1",
        "--vvc-gop",
        "-1",
        "--direct-source-files",
        "--cleanup-output",
        "--cleanup-vectors",
    ]
    if args.six_vector_baseline_json is not None:
        command.extend(["--baseline-json", str(args.six_vector_baseline_json)])
    return command


def run_logged(name: str, command: list[str], log_dir: Path, env: dict[str, str]) -> RunRecord:
    log_path = log_dir / f"{safe_name(name)}.log"
    print(f"\n== {name} ==")
    print(shlex.join(command), flush=True)
    start = time.perf_counter()
    with log_path.open("w") as log:
        log.write(f"$ {shlex.join(command)}\n\n")
        process = subprocess.Popen(
            command,
            cwd=REPO_ROOT,
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            bufsize=1,
        )
        assert process.stdout is not None
        for line in process.stdout:
            print(line, end="")
            log.write(line)
        returncode = process.wait()
    seconds = time.perf_counter() - start
    if returncode != 0:
        err = subprocess.CalledProcessError(returncode, command)
        err.cmd_name = name  # type: ignore[attr-defined]
        err.log_path = log_path  # type: ignore[attr-defined]
        err.seconds = seconds  # type: ignore[attr-defined]
        raise err
    return RunRecord(name=name, command=command, log=log_path, status="PASS", seconds=seconds)


def write_report(
    args: argparse.Namespace,
    run_dir: Path,
    records: list[RunRecord],
    matrix_out_dir: Path,
) -> Path:
    report = run_dir / "pre-release-validation.md"
    lines = [
        f"# Pre-release Validation: {args.run_name}",
        "",
        f"- Reference mode: `{args.reference_mode}`",
        f"- AOM CTC root: `{args.aomctc_root}`",
        f"- Predictive GOP: `av2=-1`, `vvc=-1`",
        f"- AV2 lossy QP: `{args.av2_lossy_qp}`",
        f"- VVC lossy QP: `{args.vvc_lossy_qp}`",
        f"- VVC fast search: `{args.vvc_fast_search}`",
        "",
        "## Run Summary",
        "",
        "| Step | Status | Seconds | Log |",
        "|---|---|---:|---|",
    ]
    for record in records:
        lines.append(
            f"| {record.name} | {record.status} | {record.seconds:.1f} | {relpath(record.log)} |"
        )

    lines.extend(["", "## Validation Tables", ""])
    for record in records:
        if record.name == "six-vector-matrix":
            continue
        lines.extend(validation_table_section(record))

    lines.extend(["", "## Six-vector Encode Matrix", ""])
    matrix_reports = sorted(matrix_out_dir.glob("*.md"))
    if matrix_reports:
        matrix_report = matrix_reports[-1]
        lines.append(f"Source report: `{relpath(matrix_report)}`")
        lines.append("")
        lines.extend(matrix_report.read_text().splitlines())
    else:
        lines.append("No six-vector matrix report was produced.")

    report.write_text("\n".join(lines) + "\n")
    return report


def existing_records(
    args: argparse.Namespace,
    run_log_dir: Path,
    matrix_out_dir: Path,
) -> list[RunRecord]:
    previous_seconds = existing_report_seconds(args.out_dir / args.run_name / "pre-release-validation.md")
    records = [
        RunRecord(
            name=name,
            command=command,
            log=run_log_dir / f"{safe_name(name)}.log",
            status="PASS" if validation_log_passed(run_log_dir / f"{safe_name(name)}.log") else "FAIL",
            seconds=previous_seconds.get(name, 0.0),
        )
        for name, command in validation_commands(args)
    ]
    matrix_reports = sorted(matrix_out_dir.glob("*.md"))
    matrix_report = matrix_reports[-1] if matrix_reports else run_log_dir / "six-vector-matrix.log"
    records.append(
        RunRecord(
            name="six-vector-matrix",
            command=six_vector_command(args, matrix_out_dir, f"{args.run_name}-six-vectors-full"),
            log=matrix_report,
            status="PASS" if matrix_reports else "FAIL",
            seconds=previous_seconds.get("six-vector-matrix", 0.0),
        )
    )
    return records


def existing_report_seconds(report: Path) -> dict[str, float]:
    if not report.exists():
        return {}
    seconds_by_name: dict[str, float] = {}
    row_re = re.compile(r"^\| ([^|]+) \| [^|]+ \| ([0-9]+(?:\.[0-9]+)?) \|")
    for line in report.read_text(errors="replace").splitlines():
        match = row_re.match(line)
        if match is None:
            continue
        seconds_by_name[match.group(1).strip()] = float(match.group(2))
    return seconds_by_name


def validation_log_passed(log: Path) -> bool:
    if not log.exists():
        return False
    return any(line.startswith("OK:") for line in log.read_text(errors="replace").splitlines())


def validation_table_section(record: RunRecord) -> list[str]:
    if not record.log.exists():
        return [f"### {record.name}", "", f"Missing validation log `{relpath(record.log)}`.", ""]
    text = record.log.read_text()
    marker = "FrameFinery media validation set:"
    index = text.find(marker)
    if index < 0:
        return [f"### {record.name}", "", f"No validation table found in `{relpath(record.log)}`.", ""]
    table = text[index:].strip().splitlines()
    return [f"### {record.name}", "", *table, ""]


def safe_name(value: str) -> str:
    return "".join(ch if ch.isalnum() or ch in "._-" else "_" for ch in value)


def relpath(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


if __name__ == "__main__":
    raise SystemExit(main())
