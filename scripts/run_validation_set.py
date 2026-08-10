#!/usr/bin/env python3
"""Run a generated vector set through the FrameFinery CLI encoder."""

from __future__ import annotations

import argparse
import hashlib
import shlex
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

import generate_test_vectors


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_VECTOR_DIR = REPO_ROOT / "verification" / "generated" / "test_vectors"
DEFAULT_ENCODED_DIR = REPO_ROOT / "verification" / "generated" / "encoded"
DEFAULT_RECON_DIR = REPO_ROOT / "verification" / "generated" / "recon"
DEFAULT_LOG_DIR = REPO_ROOT / "verification" / "generated" / "validation_logs"
REFERENCE_TOOLS = REPO_ROOT / "scripts" / "reference_tools.py"


@dataclass(frozen=True)
class ValidationResult:
    vector_name: str
    output: Path
    recon: Path
    reference_recon: Path | None
    log: Path
    status: str
    reason: str
    bytes_written: int | None
    sha256: str
    recon_sha256: str
    reference_sha256: str


@dataclass(frozen=True)
class FileCase:
    vector: generate_test_vectors.TestVector
    path: Path
    cleanup_path: Path | None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("set", nargs="?", default="smoke", help="test vector set name")
    parser.add_argument("--codec", required=True, help="codec name accepted by ff encode")
    parser.add_argument("--ff", type=Path, default=REPO_ROOT / "ff")
    parser.add_argument("--set-dir", type=Path, default=generate_test_vectors.DEFAULT_SET_DIR)
    parser.add_argument("--vector-dir", type=Path, default=DEFAULT_VECTOR_DIR)
    parser.add_argument("--encoded-dir", type=Path, default=DEFAULT_ENCODED_DIR)
    parser.add_argument("--recon-dir", type=Path, default=DEFAULT_RECON_DIR)
    parser.add_argument("--log-dir", type=Path, default=DEFAULT_LOG_DIR)
    parser.add_argument("--limit", type=int, default=0, help="run only the first N vectors")
    parser.add_argument(
        "--reference-mode",
        choices=("auto", "required", "off"),
        default="auto",
        help="decode and compare with reference tools when available",
    )
    parser.add_argument(
        "--setting",
        action="append",
        default=[],
        help="extra --set key[=value]; qp=<1..255> treats manifest lossless rows as lossy",
    )
    parser.add_argument(
        "--force-lossy",
        action="store_true",
        help="do not pass --set lossless even when the manifest row is lossless",
    )
    parser.add_argument(
        "--force-lossless",
        action="store_true",
        help="pass --set lossless and enable source comparison for every row without qp",
    )
    parser.add_argument(
        "--source-filters",
        action="store_true",
        help="run manifest patterns directly through --filter pattern=... without input files",
    )
    parser.add_argument(
        "--direct-source-files",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="feed source_file rows directly instead of materializing raw clips",
    )
    parser.add_argument(
        "--frames",
        type=parse_positive_int,
        default=0,
        help="override each vector's manifest frame count",
    )
    parser.add_argument(
        "--cleanup-recon",
        action="store_true",
        help="delete successful internal/reference reconstruction artifacts after validation",
    )
    parser.add_argument(
        "--cleanup-output",
        action="store_true",
        help="delete successful encoded bitstreams after validation metrics/checksums are collected",
    )
    parser.add_argument(
        "--cleanup-vectors",
        action="store_true",
        help="delete generated raw source vectors after each case; direct source_file inputs are never removed",
    )
    parser.add_argument("--stop-on-fail", action="store_true")
    args = parser.parse_args()
    args.qp_setting = qp_setting(args.setting)
    args.frames = args.frames or None
    if args.force_lossy and args.force_lossless:
        parser.error("--force-lossy and --force-lossless are mutually exclusive")
    if args.force_lossless and args.qp_setting is not None:
        parser.error("--force-lossless cannot be combined with qp=<1..255>")
    if args.qp_setting is not None and args.codec.lower() not in ("av2", "vvc"):
        parser.error("qp is currently supported for AV2 and VVC validation only")

    if not args.ff.exists():
        print(f"error: missing CLI binary: {args.ff}; run 'make build' first", file=sys.stderr)
        return 2
    args.ff = args.ff.resolve()

    vector_set = load_vector_set(args.set, args.set_dir)
    if args.source_filters:
        cases = [
            FileCase(override_vector_frames(vector, args.frames), Path(), None)
            for vector in vector_set.vectors
        ]
    else:
        cases = file_cases(vector_set, args)
    skipped_by_codec = [
        case.vector for case in cases if not vector_enabled_for_codec(case.vector, args.codec)
    ]
    cases = [
        case for case in cases if vector_enabled_for_codec(case.vector, args.codec)
    ]
    if args.limit:
        cases = cases[: args.limit]
    if skipped_by_codec:
        print(
            f"Skipped {len(skipped_by_codec)} vector(s) not enabled for codec {args.codec}",
            flush=True,
        )

    results: list[ValidationResult] = []
    for index, case in enumerate(cases, start=1):
        vector = case.vector
        vector_path = case.path
        name = vector.filename if args.source_filters else vector.name
        print(f"[{index:03d}/{len(cases):03d}] {name}", flush=True)
        try:
            if args.source_filters:
                result = run_source_case(vector, args)
            else:
                if case.cleanup_path is not None:
                    materialize_vector(vector_set, vector, args.vector_dir)
                result = run_file_case(vector, vector_path, args)
        finally:
            cleanup_vector_artifact(args, case.cleanup_path)
        results.append(result)
        size = "n/a" if result.bytes_written is None else str(result.bytes_written)
        print(f"  {result.status}: {result.reason} ({size} byte(s))", flush=True)
        if result.status != "PASS" and args.stop_on_fail:
            break

    print()
    print(f"FrameFinery media validation set: {args.set} ({args.codec})")
    print("| # | vector | result | bytes | sha256 | recon_sha256 | reference_sha256 | reason | log |")
    print("|---:|---|---|---:|---|---|---|---|---|")
    for index, result in enumerate(results, start=1):
        print(
            f"| {index} | {result.vector_name} | {result.status} | "
            f"{result.bytes_written if result.bytes_written is not None else 'n/a'} | "
            f"{result.sha256} | {result.recon_sha256} | {result.reference_sha256} | "
            f"{markdown_escape(result.reason)} | {relpath(result.log)} |"
        )

    failed = [result for result in results if result.status != "PASS"]
    if failed:
        print(f"\nFAIL: {len(failed)} of {len(results)} validation case(s) failed", file=sys.stderr)
        return 1
    print(f"\nOK: {len(results)} validation case(s) passed")
    return 0


def load_vector_set(set_name: str, set_dir: Path) -> generate_test_vectors.TestVectorSet:
    sets = generate_test_vectors.vector_sets(set_dir)
    if set_name not in sets:
        choices = ", ".join(sorted(sets)) or "<none>"
        raise ValueError(f"unknown test vector set '{set_name}'; choices: {choices}")
    return sets[set_name]


def vector_enabled_for_codec(vector: generate_test_vectors.TestVector, codec: str) -> bool:
    return vector.codecs is None or codec.lower() in vector.codecs


def override_vector_frames(
    vector: generate_test_vectors.TestVector, frames: int | None
) -> generate_test_vectors.TestVector:
    if frames is None or frames == vector.frames:
        return vector
    return generate_test_vectors.TestVector(
        name=vector.name,
        width=vector.width,
        height=vector.height,
        frames=frames,
        fmt=vector.fmt,
        pattern=vector.pattern,
        fps=vector.fps,
        source_path=vector.source_path,
        source=vector.source,
        crop_x=vector.crop_x,
        crop_y=vector.crop_y,
        lossless=vector.lossless,
        codecs=vector.codecs,
        filters=vector.filters,
    )


def file_cases(
    vector_set: generate_test_vectors.TestVectorSet,
    args: argparse.Namespace,
) -> list[FileCase]:
    cases = []
    for original in vector_set.vectors:
        vector = override_vector_frames(original, args.frames)
        if args.direct_source_files and vector.pattern == "source_file" and vector.source_path:
            cases.append(FileCase(vector, source_file_path(vector), None))
        else:
            path = args.vector_dir / vector.filename
            cases.append(FileCase(vector, path, path))
    return cases


def source_file_path(vector: generate_test_vectors.TestVector) -> Path:
    assert vector.source_path is not None
    return generate_test_vectors.resolve_manifest_path(vector.source_path, vector.name)


def materialize_vector(
    vector_set: generate_test_vectors.TestVectorSet,
    vector: generate_test_vectors.TestVector,
    vector_dir: Path,
) -> Path:
    path = vector_dir / vector.filename
    generate_test_vectors.write_vector_file(vector, vector_set.sources, path)
    return path


def run_file_case(
    vector: generate_test_vectors.TestVector, vector_path: Path, args: argparse.Namespace
) -> ValidationResult:
    output, recon, reference_recon, log = case_paths(vector_path.stem, args)
    command = [
        str(args.ff),
        "encode",
        str(vector_path),
        "--video",
        f"{vector.width}x{vector.height}:{vector.fmt}",
        "--frames",
        str(vector.frames),
    ]
    if vector.fps is not None:
        command.extend(["--fps", vector.fps])
    append_vector_filters(command, vector)
    command.extend(
        [
            "--encode",
            f"{args.codec}:{output}",
            "--recon",
            str(recon),
        ]
    )
    if effective_lossless(vector, args):
        command.extend(["--set", "lossless"])
    return run_command(
        vector_path.name,
        output,
        recon,
        reference_recon,
        log,
        command,
        args,
        vector,
        lossless_source=lossless_source(vector, vector_path, args),
    )


def run_source_case(vector: generate_test_vectors.TestVector, args: argparse.Namespace) -> ValidationResult:
    stem = Path(vector.filename).stem
    output, recon, reference_recon, log = case_paths(stem, args)
    command = [
        str(args.ff),
        "encode",
        "--filter",
        f"pattern={vector.pattern}",
        "--video",
        f"{vector.width}x{vector.height}:{vector.fmt}",
        "--frames",
        str(vector.frames),
    ]
    if vector.fps is not None:
        command.extend(["--fps", vector.fps])
    append_vector_filters(command, vector)
    command.extend(["--encode", f"{args.codec}:{output}", "--recon", str(recon)])
    if effective_lossless(vector, args):
        command.extend(["--set", "lossless"])
    return run_command(
        vector.filename,
        output,
        recon,
        reference_recon,
        log,
        command,
        args,
        vector,
        lossless_source=(
            vector
            if effective_lossless(vector, args) and filters_preserve_source(vector)
            else None
        ),
    )


def effective_lossless(vector: generate_test_vectors.TestVector, args: argparse.Namespace) -> bool:
    return (
        (vector.lossless or args.force_lossless)
        and not args.force_lossy
        and args.qp_setting is None
    )


def append_vector_filters(command: list[str], vector: generate_test_vectors.TestVector) -> None:
    for filter_spec in vector.filters:
        command.extend(["--filter", filter_spec])


def filters_preserve_source(vector: generate_test_vectors.TestVector) -> bool:
    return all(
        filter_spec.split("=", 1)[0].split(":", 1)[0] == "identity"
        for filter_spec in vector.filters
    )


def lossless_source(
    vector: generate_test_vectors.TestVector, vector_path: Path, args: argparse.Namespace
) -> Path | generate_test_vectors.TestVector | None:
    if not effective_lossless(vector, args) or not filters_preserve_source(vector):
        return None
    if args.direct_source_files and vector.pattern == "source_file" and vector.source_path:
        return vector
    return vector_path


def parse_qp(value: str) -> int:
    try:
        qp = int(value, 10)
    except ValueError as err:
        raise argparse.ArgumentTypeError(
            f"QP expects an integer from 1 through 255, got '{value}'"
        ) from err
    if not (1 <= qp <= 255):
        raise argparse.ArgumentTypeError(
            f"QP expects an integer from 1 through 255, got '{value}'"
        )
    return qp


def parse_positive_int(value: str) -> int:
    try:
        parsed = int(value, 10)
    except ValueError as err:
        raise argparse.ArgumentTypeError(f"expected a positive integer, got '{value}'") from err
    if parsed <= 0:
        raise argparse.ArgumentTypeError(f"expected a positive integer, got '{value}'")
    return parsed


def qp_setting(settings: list[str]) -> int | None:
    for spec in settings:
        name, _, value = spec.partition("=")
        if name != "qp":
            continue
        if not value:
            raise SystemExit("error: --setting qp expects qp=<1..255>")
        try:
            return parse_qp(value)
        except argparse.ArgumentTypeError as err:
            raise SystemExit(f"error: {err}") from err
    return None


def case_paths(stem: str, args: argparse.Namespace) -> tuple[Path, Path, Path, Path]:
    output_dir = args.encoded_dir / args.codec / args.set
    recon_dir = args.recon_dir / args.codec / args.set
    log_dir = args.log_dir / args.codec
    output_dir.mkdir(parents=True, exist_ok=True)
    recon_dir.mkdir(parents=True, exist_ok=True)
    log_dir.mkdir(parents=True, exist_ok=True)

    output = output_dir / f"{stem}.{codec_extension(args.codec)}"
    recon = recon_dir / f"{stem}_internal.yuv"
    reference_recon = recon_dir / f"{stem}_reference.yuv"
    log = log_dir / f"{args.set}_{stem}.log"
    return output, recon, reference_recon, log


def run_command(
    vector_name: str,
    output: Path,
    recon: Path,
    reference_recon: Path,
    log: Path,
    command: list[str],
    args: argparse.Namespace,
    vector: generate_test_vectors.TestVector,
    lossless_source: Path | generate_test_vectors.TestVector | None = None,
) -> ValidationResult:
    output.unlink(missing_ok=True)
    recon.unlink(missing_ok=True)
    reference_recon.unlink(missing_ok=True)

    for setting in args.setting:
        command.extend(["--set", setting])

    process = subprocess.run(
        command,
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    log.write_text(f"$ {shlex.join(command)}\n\n{process.stdout}")
    for line in process.stdout.splitlines():
        if line.startswith("frame:"):
            print(f"  {line}", flush=True)

    if process.returncode != 0:
        return ValidationResult(
            vector_name=vector_name,
            output=output,
            recon=recon,
            reference_recon=None,
            log=log,
            status="FAIL",
            reason=extract_failure_reason(process.stdout),
            bytes_written=output.stat().st_size if output.exists() else None,
            sha256=sha256_file(output) if output.exists() else "n/a",
            recon_sha256=sha256_file(recon) if recon.exists() else "n/a",
            reference_sha256="n/a",
        )
    if not output.exists():
        return ValidationResult(
            vector_name=vector_name,
            output=output,
            recon=recon,
            reference_recon=None,
            log=log,
            status="FAIL",
            reason="encoder returned success but did not create output",
            bytes_written=None,
            sha256="n/a",
            recon_sha256=sha256_file(recon) if recon.exists() else "n/a",
            reference_sha256="n/a",
        )
    size = output.stat().st_size
    if size == 0:
        return ValidationResult(
            vector_name=vector_name,
            output=output,
            recon=recon,
            reference_recon=None,
            log=log,
            status="FAIL",
            reason="encoder returned success but output is empty",
            bytes_written=size,
            sha256=sha256_file(output),
            recon_sha256=sha256_file(recon) if recon.exists() else "n/a",
            reference_sha256="n/a",
        )
    if not recon.exists():
        return ValidationResult(
            vector_name=vector_name,
            output=output,
            recon=recon,
            reference_recon=None,
            log=log,
            status="FAIL",
            reason="encoder returned success but did not create internal reconstruction",
            bytes_written=size,
            sha256=sha256_file(output),
            recon_sha256="n/a",
            reference_sha256="n/a",
        )
    recon_sha = sha256_file(recon)
    lossless_status = validate_lossless_source(lossless_source, recon)
    if lossless_status is not None and lossless_status[0] == "FAIL":
        return ValidationResult(
            vector_name=vector_name,
            output=output,
            recon=recon,
            reference_recon=None,
            log=log,
            status="FAIL",
            reason=lossless_status[1],
            bytes_written=size,
            sha256=sha256_file(output),
            recon_sha256=recon_sha,
            reference_sha256="n/a",
        )
    reference_status = validate_reference_decode(args, output, recon, reference_recon, log, vector)
    if reference_status is not None and reference_status[0] == "FAIL":
        return ValidationResult(
            vector_name=vector_name,
            output=output,
            recon=recon,
            reference_recon=reference_recon if reference_recon.exists() else None,
            log=log,
            status="FAIL",
            reason=reference_status[1],
            bytes_written=size,
            sha256=sha256_file(output),
            recon_sha256=recon_sha,
            reference_sha256=sha256_file(reference_recon) if reference_recon.exists() else "n/a",
        )
    reference_sha = sha256_file(reference_recon) if reference_recon.exists() else "n/a"
    reason = "encoded output and internal reconstruction were produced"
    if lossless_status is not None:
        reason = lossless_status[1]
    if reference_status is not None:
        reason = (
            f"{reason}; {reference_status[1]}"
            if lossless_status is not None
            else reference_status[1]
        )
    reference_recon_result = reference_recon if reference_recon.exists() else None
    result = ValidationResult(
        vector_name=vector_name,
        output=output,
        recon=recon,
        reference_recon=reference_recon_result,
        log=log,
        status="PASS",
        reason=reason,
        bytes_written=size,
        sha256=sha256_file(output),
        recon_sha256=recon_sha,
        reference_sha256=reference_sha,
    )
    cleanup_success_artifacts(args, output, recon, reference_recon_result)
    return result


def validate_lossless_source(
    source: Path | generate_test_vectors.TestVector | None, recon: Path
) -> tuple[str, str] | None:
    if source is None:
        return None
    if isinstance(source, Path):
        return validate_lossless_file_prefix(source, recon, source.stat().st_size)
    if (
        source.pattern == "source_file"
        and source.source_path is not None
    ):
        if source.source_path.suffix.lower() == ".y4m":
            return validate_lossless_y4m_file_prefix(source, recon)
        source_path = source_file_path(source)
        expected_bytes = generate_test_vectors.raw_frame_len(source) * source.frames
        return validate_lossless_file_prefix(source_path, recon, expected_bytes)
    else:
        source_bytes = generate_test_vectors.generate_yuv(source, {})
    recon_bytes = recon.read_bytes()
    if source_bytes != recon_bytes:
        if len(source_bytes) != len(recon_bytes):
            return (
                "FAIL",
                f"lossless reconstruction length differs from source ({len(recon_bytes)} != {len(source_bytes)})",
            )
        return ("FAIL", "lossless reconstruction differs from source")
    return ("PASS", "lossless reconstruction matches source")


def validate_lossless_y4m_file_prefix(
    vector: generate_test_vectors.TestVector,
    recon: Path,
) -> tuple[str, str]:
    source = source_file_path(vector)
    if not source.exists():
        return ("FAIL", f"Y4M source file does not exist: {source}")
    frame_len = generate_test_vectors.raw_frame_len(vector)
    expected_bytes = frame_len * vector.frames
    recon_size = recon.stat().st_size
    if recon_size != expected_bytes:
        return (
            "FAIL",
            f"lossless reconstruction length differs from Y4M source ({recon_size} != {expected_bytes})",
        )

    chunk_size = 16 * 1024 * 1024
    offset = 0
    with source.open("rb") as source_file, recon.open("rb") as recon_file:
        header = source_file.readline()
        if not header.startswith(b"YUV4MPEG2 "):
            return ("FAIL", f"source is not a Y4M stream: {source}")
        for frame_index in range(vector.frames):
            frame_header = source_file.readline()
            if not frame_header:
                return (
                    "FAIL",
                    f"Y4M source is too short: missing frame {frame_index + 1}",
                )
            if not frame_header.startswith(b"FRAME"):
                return (
                    "FAIL",
                    f"Y4M source has invalid frame marker at frame {frame_index + 1}",
                )
            remaining = frame_len
            while remaining:
                read_len = min(chunk_size, remaining)
                source_chunk = source_file.read(read_len)
                recon_chunk = recon_file.read(read_len)
                if len(source_chunk) != read_len:
                    return (
                        "FAIL",
                        f"Y4M source is too short near frame {frame_index + 1}",
                    )
                if source_chunk != recon_chunk:
                    return (
                        "FAIL",
                        f"lossless reconstruction differs from Y4M source near byte {offset}",
                    )
                remaining -= read_len
                offset += read_len
    return ("PASS", "lossless reconstruction matches Y4M source")


def validate_lossless_file_prefix(
    source: Path,
    recon: Path,
    expected_bytes: int,
) -> tuple[str, str]:
    source_size = source.stat().st_size
    if source_size < expected_bytes:
        return (
            "FAIL",
            f"source is too short for lossless comparison ({source_size} < {expected_bytes})",
        )
    recon_size = recon.stat().st_size
    if recon_size != expected_bytes:
        return (
            "FAIL",
            f"lossless reconstruction length differs from source ({recon_size} != {expected_bytes})",
        )
    chunk_size = 16 * 1024 * 1024
    remaining = expected_bytes
    offset = 0
    with source.open("rb") as source_file, recon.open("rb") as recon_file:
        while remaining:
            read_len = min(chunk_size, remaining)
            source_chunk = source_file.read(read_len)
            recon_chunk = recon_file.read(read_len)
            if source_chunk != recon_chunk:
                return (
                    "FAIL",
                    f"lossless reconstruction differs from source near byte {offset}",
                )
            remaining -= read_len
            offset += read_len
    return ("PASS", "lossless reconstruction matches source")


def cleanup_recon_artifacts(args: argparse.Namespace, *paths: Path | None) -> None:
    if not args.cleanup_recon:
        return
    for path in paths:
        if path is not None:
            path.unlink(missing_ok=True)


def cleanup_success_artifacts(
    args: argparse.Namespace,
    output: Path,
    recon: Path,
    reference_recon: Path | None,
) -> None:
    if args.cleanup_output:
        output.unlink(missing_ok=True)
    cleanup_recon_artifacts(args, recon, reference_recon)


def cleanup_vector_artifact(args: argparse.Namespace, path: Path | None) -> None:
    if args.cleanup_vectors and path is not None:
        path.unlink(missing_ok=True)


def codec_extension(codec: str) -> str:
    return {"av2": "obu", "vvc": "vvc"}.get(codec, codec)


def validate_reference_decode(
    args: argparse.Namespace,
    bitstream: Path,
    internal_recon: Path,
    reference_recon: Path,
    log: Path,
    vector: generate_test_vectors.TestVector,
) -> tuple[str, str] | None:
    if args.reference_mode == "off":
        return None

    command = [
        sys.executable,
        str(REFERENCE_TOOLS),
        "decode",
        "--codec",
        args.codec,
        "--bitstream",
        str(bitstream),
        "--output",
        str(reference_recon),
        "--no-build",
    ]
    process = subprocess.run(
        command,
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    with log.open("a") as file:
        file.write("\n\n")
        file.write(f"$ {shlex.join(command)}\n\n{process.stdout}")

    if process.returncode != 0:
        reason = extract_failure_reason(process.stdout)
        if process.returncode == 2 and args.reference_mode == "auto":
            return ("SKIP", f"reference decode skipped: {reason}")
        return ("FAIL", f"reference decode failed: {reason}")

    if not reference_recon.exists():
        return ("FAIL", "reference decoder returned success but did not create reconstruction")
    if reference_recon.stat().st_size == 0:
        return ("FAIL", "reference decoder returned success but reconstruction is empty")

    normalized_status = normalize_reference_reconstruction(vector, reference_recon, log)
    if normalized_status is not None and normalized_status[0] == "FAIL":
        return normalized_status

    internal_sha = sha256_file(internal_recon)
    reference_sha = sha256_file(reference_recon)
    if internal_sha != reference_sha:
        return (
            "FAIL",
            "reference reconstruction checksum differs from internal reconstruction",
        )
    if normalized_status is not None:
        return (
            "PASS",
            "reference reconstruction matches internal reconstruction after planar GBR to packed rgb24 normalization",
        )
    return ("PASS", "reference reconstruction matches internal reconstruction")


def normalize_reference_reconstruction(
    vector: generate_test_vectors.TestVector, reference_recon: Path, log: Path
) -> tuple[str, str] | None:
    if vector.fmt != "rgb24":
        return None

    pixels = vector.width * vector.height
    frame_len = pixels * 3
    expected_size = frame_len * vector.frames
    actual_size = reference_recon.stat().st_size
    if actual_size != expected_size:
        return (
            "FAIL",
            f"reference rgb24 reconstruction length differs from expected packed RGB size ({actual_size} != {expected_size})",
        )

    tmp = reference_recon.with_name(f"{reference_recon.name}.tmp")
    with reference_recon.open("rb") as source, tmp.open("wb") as output:
        for _ in range(vector.frames):
            frame = source.read(frame_len)
            if len(frame) != frame_len:
                tmp.unlink(missing_ok=True)
                return ("FAIL", "reference rgb24 reconstruction ended mid-frame")
            g_plane = frame[:pixels]
            b_plane = frame[pixels : pixels * 2]
            r_plane = frame[pixels * 2 :]
            packed = bytearray(frame_len)
            packed[0::3] = r_plane
            packed[1::3] = g_plane
            packed[2::3] = b_plane
            output.write(packed)
    tmp.replace(reference_recon)
    with log.open("a") as file:
        file.write("\nNormalized reference reconstruction from planar GBR to packed rgb24.\n")
    return ("PASS", "reference reconstruction normalized from planar GBR to packed rgb24")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        while chunk := file.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def extract_failure_reason(output: str) -> str:
    markers = ("error:", "Error:", "panic", "FAIL:")
    for line in output.splitlines():
        stripped = line.strip()
        if any(marker in stripped for marker in markers):
            return stripped
    lines = [line.strip() for line in output.splitlines() if line.strip()]
    return lines[-1] if lines else "encoder command failed"


def markdown_escape(value: str) -> str:
    return value.replace("|", "\\|")


def relpath(path: Path) -> Path:
    try:
        return path.resolve().relative_to(REPO_ROOT)
    except ValueError:
        return path


if __name__ == "__main__":
    raise SystemExit(main())
