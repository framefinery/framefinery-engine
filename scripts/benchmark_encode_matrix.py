#!/usr/bin/env python3
"""Benchmark FrameFinery encode speed over codec/mode/vector matrices."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import shlex
import subprocess
import sys
import time
from dataclasses import replace
from fractions import Fraction
from pathlib import Path
from typing import Any

import generate_test_vectors


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SET = "local-aomctc-b2-scc-1080p-lossless-50f"
DEFAULT_VECTOR_DIR = REPO_ROOT / "verification" / "generated" / "test_vectors"
DEFAULT_OUT_DIR = REPO_ROOT / "verification" / "generated" / "encode_matrix"
PSNR_ALL_RE = re.compile(r"\bpsnr_all=(inf|[-+]?[0-9]*\.?[0-9]+)")
CLI_SOURCE_FILTER_PATTERNS = frozenset(("black", "checker", "gradient", "color_blocks"))
TRADEOFF_FPS_LOG2_WEIGHT = 10.0
TRADEOFF_BYTES_LOG2_WEIGHT = 4.0
TRADEOFF_PSNR_DB_WEIGHT = 8.0
TRADEOFF_ACCEPT_SCORE = 2.0
TRADEOFF_MINOR_PSNR_LOSS_DB = 0.30
TRADEOFF_HARD_PSNR_LOSS_DB = 1.00
TRADEOFF_MINOR_BYTE_REGRESSION_RATIO = 1.05
TRADEOFF_HARD_BYTE_REGRESSION_RATIO = 1.20
TRADEOFF_MIN_FPS_RATIO_FOR_ACCEPT = 1.10
TRADEOFF_HARD_FPS_REGRESSION_RATIO = 0.90


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("set", nargs="?", default=DEFAULT_SET, help="test vector set name")
    parser.add_argument("--ff", type=Path, default=REPO_ROOT / "ff")
    parser.add_argument("--set-dir", type=Path, default=generate_test_vectors.DEFAULT_SET_DIR)
    parser.add_argument("--vector-dir", type=Path, default=DEFAULT_VECTOR_DIR)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--run-name", default="", help="label for output files/directories")
    parser.add_argument("--codec", action="append", choices=("av2", "vvc"), default=[])
    parser.add_argument("--mode", action="append", choices=("lossless", "lossy"), default=[])
    parser.add_argument("--limit", type=int, default=0, help="run only the first N enabled rows")
    parser.add_argument(
        "--frames",
        type=parse_positive_int,
        default=0,
        help="override each vector's frame count, e.g. --frames 1 for I-frame checks",
    )
    parser.add_argument("--av2-lossy-qp", type=parse_qp, default=24)
    parser.add_argument("--vvc-lossy-qp", type=parse_qp, default=24)
    parser.add_argument(
        "--vvc-fast-search",
        choices=("off", "conservative", "moderate", "aggressive", "lossless-speed"),
        default="lossless-speed",
        help="optional VVC mode-search pruning level passed as --set fast-search=<level>",
    )
    parser.add_argument("--av2-gop", type=parse_gop, default=-1)
    parser.add_argument("--vvc-gop", type=parse_gop, default=-1)
    parser.add_argument(
        "--direct-source-files",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="feed source_file rows directly instead of materializing raw clips",
    )
    parser.add_argument(
        "--cleanup-recon",
        action="store_true",
        help="delete each reconstruction artifact after checksums are collected; only used with --write-recon",
    )
    parser.add_argument(
        "--cleanup-output",
        action="store_true",
        help="delete each encoded bitstream after size and checksum are collected",
    )
    parser.add_argument(
        "--cleanup-vectors",
        action="store_true",
        help="delete generated raw source vectors before exit; direct source_file inputs are never removed",
    )
    parser.add_argument(
        "--write-recon",
        action="store_true",
        help="write reconstruction artifacts and include recon_sha256 in the JSON report",
    )
    parser.add_argument(
        "--baseline-json",
        type=Path,
        help="optional previous JSON report to include byte/fps deltas",
    )
    parser.add_argument(
        "--av2-stats-dir",
        type=Path,
        help="optional directory for per-case FRAMEFINERY_AV2_STATS JSONL files",
    )
    parser.add_argument(
        "--vvc-stats-dir",
        type=Path,
        help="optional directory for per-case FRAMEFINERY_VVC_STATS JSONL files",
    )
    parser.add_argument(
        "--rerender-json",
        type=Path,
        help="load an existing JSON report and re-emit JSON/Markdown without encoding",
    )
    args = parser.parse_args()

    if args.rerender_json is not None:
        return rerender_report(args)

    if not args.ff.exists():
        print(f"error: missing CLI binary: {args.ff}; run 'make build' first", file=sys.stderr)
        return 2
    args.ff = args.ff.resolve()
    args.frames = args.frames or None
    codecs = args.codec or ["av2", "vvc"]
    modes = args.mode or ["lossless", "lossy"]
    run_name = args.run_name or time.strftime("%Y%m%d-%H%M%S")
    run_dir = (args.out_dir / run_name).resolve()
    log_dir = run_dir / "logs"
    run_dir.mkdir(parents=True, exist_ok=True)
    log_dir.mkdir(parents=True, exist_ok=True)

    vector_set = load_vector_set(args.set, args.set_dir)
    baseline = load_baseline(args.baseline_json)
    results: list[dict[str, Any]] = []
    skipped = 0
    cases = [
        (codec, mode, vector)
        for codec in codecs
        for mode in modes
        for vector in vector_set.vectors
        if vector_enabled_for_codec(vector, codec) and mode_supported(vector, codec, mode)
    ]
    if args.limit:
        cases = cases[: args.limit]
    total_cases = len(cases)

    generated_source_cache: dict[tuple[Any, ...], Path] = {}
    try:
        for codec, mode, vector in cases:
            case_index = len(results) + 1
            print(
                f"[{case_index:03d}/{total_cases:03d}] {codec} {mode} {vector.name}",
                flush=True,
            )
            result = run_case(
                vector_set,
                vector,
                codec,
                mode,
                run_dir,
                log_dir,
                args,
                generated_source_cache,
            )
            apply_baseline_delta(result, baseline)
            results.append(result)
            delta = delta_label(result)
            print(
                "  bytes={bytes} bitrate_kbps={bitrate} fps={fps:.2f} psnr={psnr}{delta}".format(
                    bytes=result["bytes"],
                    bitrate=format_optional_one_decimal(result.get("bitrate_kbps")),
                    fps=result["fps"],
                    psnr=format_optional_float(result.get("psnr_all_mean")),
                    delta=delta,
                ),
                flush=True,
            )
    finally:
        cleanup_cached_vector_artifacts(args, generated_source_cache)
    skipped = count_skipped(vector_set, codecs, modes, args.limit)
    apply_av2_parity_gaps(results)

    report = {
        "set": args.set,
        "run_name": run_name,
        "ff": str(args.ff),
        "av2_gop": args.av2_gop,
        "vvc_gop": args.vvc_gop,
        "av2_lossy_qp": args.av2_lossy_qp,
        "vvc_lossy_qp": args.vvc_lossy_qp,
        "vvc_fast_search": args.vvc_fast_search,
        "cleanup_recon": args.cleanup_recon,
        "cleanup_output": args.cleanup_output,
        "cleanup_vectors": args.cleanup_vectors,
        "write_recon": args.write_recon,
        "skipped": skipped,
        "results": results,
    }
    write_report_files(report, args.out_dir, skipped)
    if skipped:
        print(f"skipped {skipped} unsupported codec/vector/mode combination(s)")
    return 0


def rerender_report(args: argparse.Namespace) -> int:
    report = json.loads(args.rerender_json.read_text())
    report["run_name"] = args.run_name or report.get("run_name") or args.rerender_json.stem
    report.setdefault("set", DEFAULT_SET)
    report.setdefault("av2_gop", args.av2_gop)
    report.setdefault("vvc_gop", args.vvc_gop)
    report.setdefault("av2_lossy_qp", args.av2_lossy_qp)
    report.setdefault("vvc_lossy_qp", args.vvc_lossy_qp)
    report.setdefault("vvc_fast_search", args.vvc_fast_search)
    report.setdefault("cleanup_recon", False)
    report.setdefault("cleanup_output", False)
    report.setdefault("cleanup_vectors", False)
    report.setdefault("write_recon", True)
    results = report.get("results", [])
    baseline = load_baseline(args.baseline_json)
    for row in results:
        clear_derived_metrics(row)
        apply_baseline_delta(row, baseline)
    apply_av2_parity_gaps(results)
    skipped = int(report.get("skipped", 0))
    write_report_files(report, args.out_dir, skipped)
    if skipped:
        print(f"skipped {skipped} unsupported codec/vector/mode combination(s)")
    return 0


def clear_derived_metrics(row: dict[str, Any]) -> None:
    for key in list(row):
        if key.startswith(("baseline_", "delta_", "tradeoff_", "av2_")):
            del row[key]


def write_report_files(report: dict[str, Any], out_dir: Path, skipped: int) -> None:
    json_path = out_dir / f"{report['run_name']}.json"
    md_path = out_dir / f"{report['run_name']}.md"
    json_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    md_path.write_text(markdown_report(report, skipped) + "\n")
    print()
    print(f"wrote {relpath(json_path)}")
    print(f"wrote {relpath(md_path)}")


def load_vector_set(set_name: str, set_dir: Path) -> generate_test_vectors.TestVectorSet:
    sets = generate_test_vectors.vector_sets(set_dir)
    if set_name not in sets:
        choices = ", ".join(sorted(sets)) or "<none>"
        raise SystemExit(f"unknown test vector set '{set_name}'; choices: {choices}")
    return sets[set_name]


def vector_enabled_for_codec(vector: generate_test_vectors.TestVector, codec: str) -> bool:
    return vector.codecs is None or codec.lower() in vector.codecs


def mode_supported(vector: generate_test_vectors.TestVector, codec: str, mode: str) -> bool:
    if codec == "vvc" and mode == "lossy" and vector.fmt == "yuv422p10le":
        return True
    return True


def count_skipped(
    vector_set: generate_test_vectors.TestVectorSet,
    codecs: list[str],
    modes: list[str],
    limit: int,
) -> int:
    if limit:
        return 0
    skipped = 0
    for codec in codecs:
        for mode in modes:
            for vector in vector_set.vectors:
                if not vector_enabled_for_codec(vector, codec):
                    skipped += 1
                elif not mode_supported(vector, codec, mode):
                    skipped += 1
    return skipped


def run_case(
    vector_set: generate_test_vectors.TestVectorSet,
    vector: generate_test_vectors.TestVector,
    codec: str,
    mode: str,
    run_dir: Path,
    log_dir: Path,
    args: argparse.Namespace,
    generated_source_cache: dict[tuple[Any, ...], Path],
) -> dict[str, Any]:
    if args.frames is not None and args.frames != vector.frames:
        vector = replace(vector, frames=args.frames)
    use_source_filter = should_use_source_filter(vector)
    source_path: Path | None = None
    if not use_source_filter:
        source_path = source_path_for_vector(
            vector_set,
            vector,
            args,
            generated_source_cache,
        )
    case_dir = run_dir / codec / mode
    case_dir.mkdir(parents=True, exist_ok=True)
    stem = Path(vector.filename).stem
    output = case_dir / f"{stem}.{codec_extension(codec)}"
    recon = case_dir / f"{stem}_recon.{raw_extension(vector)}"
    log = log_dir / f"{codec}_{mode}_{stem}.log"
    av2_stats_path = codec_stats_path_for_case(codec, mode, stem, args, "av2")
    vvc_stats_path = codec_stats_path_for_case(codec, mode, stem, args, "vvc")
    output.unlink(missing_ok=True)
    recon.unlink(missing_ok=True)
    if av2_stats_path is not None:
        av2_stats_path.unlink(missing_ok=True)
    if vvc_stats_path is not None:
        vvc_stats_path.unlink(missing_ok=True)

    command = [str(args.ff), "encode"]
    if use_source_filter:
        command.extend(["--filter", f"pattern={vector.pattern}"])
    else:
        assert source_path is not None
        command.append(str(source_path))
    command.extend(
        [
            "--video",
            f"{vector.width}x{vector.height}:{vector.fmt}",
            "--frames",
            str(vector.frames),
        ]
    )
    if vector.fps is not None:
        command.extend(["--fps", vector.fps])
    append_vector_filters(command, vector)
    command.extend(["--encode", f"{codec}:{output}", "--psnr"])
    if args.write_recon:
        command.extend(["--recon", str(recon)])
    settings: list[str] = []
    if mode == "lossless":
        settings.append("lossless")
    if codec == "av2":
        settings.append(f"gop={args.av2_gop}")
    if codec == "vvc":
        settings.append(f"gop={args.vvc_gop}")
    if codec == "vvc" and args.vvc_fast_search != "off":
        settings.append(f"fast-search={args.vvc_fast_search}")
    for setting in settings:
        command.extend(["--set", setting])
    if codec == "av2" and mode == "lossy":
        command.extend(["--set", f"qp={args.av2_lossy_qp}"])
    if codec == "vvc" and mode == "lossy":
        command.extend(["--set", f"qp={args.vvc_lossy_qp}"])

    env = None
    if av2_stats_path is not None:
        env = os.environ.copy()
        env["FRAMEFINERY_AV2_STATS"] = str(av2_stats_path)
    if vvc_stats_path is not None:
        env = env or os.environ.copy()
        env["FRAMEFINERY_VVC_STATS"] = str(vvc_stats_path)

    start = time.perf_counter()
    process = subprocess.run(
        command,
        cwd=REPO_ROOT,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    elapsed = time.perf_counter() - start
    log.write_text(f"$ {shlex.join(command)}\n\n{process.stdout}")
    if process.returncode != 0:
        print(process.stdout, file=sys.stderr, end="")
        raise SystemExit(
            f"encode failed for {codec} {mode} {vector.filename}; see {relpath(log)}"
        )
    require_non_empty(output, "bitstream", vector.filename, log)
    if args.write_recon:
        require_non_empty(recon, "reconstruction", vector.filename, log)
    psnr_all_mean = mean_psnr_all(process.stdout)
    if psnr_all_mean is None:
        raise SystemExit(
            f"encoder did not emit PSNR for {codec} {mode} {vector.filename}; see {relpath(log)}"
        )

    fps = vector.frames / elapsed if elapsed > 0.0 else math.inf
    bitstream_size = output.stat().st_size
    bitstream_sha256 = sha256_file(output)
    recon_sha256 = sha256_file(recon) if args.write_recon else "n/a"
    source_fps = source_fps_value(vector.fps)
    duration_seconds = vector.frames / source_fps if source_fps is not None else None
    bitrate_kbps = (
        (bitstream_size * 8.0 / duration_seconds) / 1000.0
        if duration_seconds is not None and duration_seconds > 0.0
        else None
    )
    result = {
        "codec": codec,
        "mode": mode_label(codec, mode, args),
        "mode_key": mode,
        "vector": vector.name,
        "filename": vector.filename,
        "format": vector.fmt,
        "width": vector.width,
        "height": vector.height,
        "frames": vector.frames,
        "bytes": bitstream_size,
        "bitrate_kbps": bitrate_kbps,
        "source_fps": source_fps,
        "duration_seconds": duration_seconds,
        "seconds": elapsed,
        "fps": fps,
        "psnr_all_mean": psnr_all_mean,
        "bitstream_sha256": bitstream_sha256,
        "recon_sha256": recon_sha256,
        "log": str(relpath(log)),
    }
    if av2_stats_path is not None:
        require_non_empty(av2_stats_path, "AV2 stats", vector.filename, log)
        result["av2_stats"] = str(relpath(av2_stats_path))
    if vvc_stats_path is not None:
        require_non_empty(vvc_stats_path, "VVC stats", vector.filename, log)
        result["vvc_stats"] = str(relpath(vvc_stats_path))
    cleanup_success_artifacts(args, output, recon)
    return result


def codec_stats_path_for_case(
    codec: str,
    mode: str,
    stem: str,
    args: argparse.Namespace,
    stats_codec: str,
) -> Path | None:
    if codec != stats_codec:
        return None
    stats_dir = getattr(args, f"{stats_codec}_stats_dir")
    if stats_dir is None:
        return None
    stats_dir = stats_dir.resolve()
    stats_dir.mkdir(parents=True, exist_ok=True)
    return stats_dir / f"{stats_codec}_{mode}_{stem}.jsonl"


def source_path_for_vector(
    vector_set: generate_test_vectors.TestVectorSet,
    vector: generate_test_vectors.TestVector,
    args: argparse.Namespace,
    generated_source_cache: dict[tuple[Any, ...], Path],
) -> Path:
    if args.direct_source_files and vector.pattern == "source_file" and vector.source_path:
        return source_file_path(vector)
    cache_key = source_cache_key(vector)
    if cache_key in generated_source_cache and generated_source_cache[cache_key].exists():
        return generated_source_cache[cache_key]
    path = args.vector_dir / vector.filename
    generated_source_cache[cache_key] = path
    try:
        generate_test_vectors.write_vector_file(vector, vector_set.sources, path)
    except BaseException:
        cleanup_vector_artifact(args, path)
        generated_source_cache.pop(cache_key, None)
        raise
    return path


def source_cache_key(vector: generate_test_vectors.TestVector) -> tuple[Any, ...]:
    return (
        vector.filename,
        vector.width,
        vector.height,
        vector.frames,
        vector.fmt,
        vector.fps,
        vector.pattern,
        vector.source_path,
        tuple(vector.filters),
    )


def should_use_source_filter(vector: generate_test_vectors.TestVector) -> bool:
    return vector.source_path is None and vector.pattern in CLI_SOURCE_FILTER_PATTERNS


def append_vector_filters(command: list[str], vector: generate_test_vectors.TestVector) -> None:
    for filter_spec in vector.filters:
        command.extend(["--filter", filter_spec])


def source_file_path(vector: generate_test_vectors.TestVector) -> Path:
    assert vector.source_path is not None
    return generate_test_vectors.resolve_manifest_path(vector.source_path, vector.name)


def cleanup_recon_artifact(args: argparse.Namespace, recon: Path) -> None:
    if args.write_recon and args.cleanup_recon:
        recon.unlink(missing_ok=True)


def cleanup_success_artifacts(args: argparse.Namespace, output: Path, recon: Path) -> None:
    if args.cleanup_output:
        output.unlink(missing_ok=True)
    cleanup_recon_artifact(args, recon)


def cleanup_vector_artifact(args: argparse.Namespace, path: Path | None) -> None:
    if args.cleanup_vectors and path is not None:
        path.unlink(missing_ok=True)


def cleanup_cached_vector_artifacts(
    args: argparse.Namespace,
    generated_source_cache: dict[tuple[Any, ...], Path],
) -> None:
    for path in generated_source_cache.values():
        cleanup_vector_artifact(args, path)


def mean_psnr_all(output: str) -> float | None:
    values = []
    for match in PSNR_ALL_RE.finditer(output):
        value = match.group(1)
        if value == "inf":
            values.append(math.inf)
        else:
            values.append(float(value))
    if not values:
        return None
    if any(math.isinf(value) for value in values):
        finite_values = [value for value in values if math.isfinite(value)]
        if finite_values:
            return sum(finite_values) / len(finite_values)
        return math.inf
    return sum(values) / len(values)


def load_baseline(path: Path | None) -> dict[tuple[str, str, str], dict[str, Any]]:
    if path is None:
        return {}
    report = json.loads(path.read_text())
    return {
        (row["codec"], row["mode_key"], row["filename"]): row
        for row in report.get("results", [])
    }


def apply_baseline_delta(
    result: dict[str, Any],
    baseline: dict[tuple[str, str, str], dict[str, Any]],
) -> None:
    previous = baseline.get((result["codec"], result["mode_key"], result["filename"]))
    if previous is None:
        return
    result["baseline_bytes"] = previous["bytes"]
    result["baseline_fps"] = previous["fps"]
    result["delta_bytes"] = result["bytes"] - previous["bytes"]
    result["delta_fps"] = result["fps"] - previous["fps"]
    if previous["bytes"] > 0:
        byte_ratio = result["bytes"] / previous["bytes"]
        result["baseline_byte_ratio"] = byte_ratio
        result["delta_bytes_pct"] = (byte_ratio - 1.0) * 100.0
    if previous["fps"] > 0.0:
        fps_ratio = result["fps"] / previous["fps"]
        result["baseline_fps_ratio"] = fps_ratio
        result["delta_fps_pct"] = (fps_ratio - 1.0) * 100.0
    previous_psnr = previous.get("psnr_all_mean")
    current_psnr = result.get("psnr_all_mean")
    if previous_psnr is not None and current_psnr is not None:
        if math.isfinite(previous_psnr) and math.isfinite(current_psnr):
            result["delta_psnr_all_mean"] = current_psnr - previous_psnr
    apply_tradeoff_scale(result)


def apply_tradeoff_scale(result: dict[str, Any]) -> None:
    score = 0.0
    scored = False

    fps_ratio = result.get("baseline_fps_ratio")
    if finite_positive(fps_ratio):
        score += TRADEOFF_FPS_LOG2_WEIGHT * math.log2(fps_ratio)
        scored = True

    byte_ratio = result.get("baseline_byte_ratio")
    if finite_positive(byte_ratio):
        score += TRADEOFF_BYTES_LOG2_WEIGHT * math.log2(1.0 / byte_ratio)
        scored = True

    psnr_delta = result.get("delta_psnr_all_mean")
    if finite_number(psnr_delta):
        score += TRADEOFF_PSNR_DB_WEIGHT * psnr_delta
        scored = True

    if not scored:
        return
    result["tradeoff_score"] = score
    result["tradeoff_status"] = classify_tradeoff_result(result)


def classify_tradeoff_result(result: dict[str, Any]) -> str:
    score = result.get("tradeoff_score", 0.0)
    fps_ratio = result.get("baseline_fps_ratio")
    byte_ratio = result.get("baseline_byte_ratio")
    psnr_delta = result.get("delta_psnr_all_mean")

    if finite_positive(fps_ratio) and fps_ratio < TRADEOFF_HARD_FPS_REGRESSION_RATIO:
        return "regress"
    if finite_positive(byte_ratio) and byte_ratio > TRADEOFF_HARD_BYTE_REGRESSION_RATIO:
        return "regress"
    if finite_number(psnr_delta) and psnr_delta < -TRADEOFF_HARD_PSNR_LOSS_DB:
        return "regress"

    watched = False
    if finite_number(psnr_delta) and psnr_delta < -TRADEOFF_MINOR_PSNR_LOSS_DB:
        watched = True
    if finite_positive(byte_ratio) and byte_ratio > TRADEOFF_MINOR_BYTE_REGRESSION_RATIO:
        watched = True

    if score >= TRADEOFF_ACCEPT_SCORE and (
        not finite_positive(fps_ratio) or fps_ratio >= TRADEOFF_MIN_FPS_RATIO_FOR_ACCEPT
    ):
        return "watch" if watched else "accept"
    if score >= 0.0:
        return "watch"
    return "regress"


def apply_av2_parity_gaps(results: list[dict[str, Any]]) -> None:
    av2_rows = {
        (row["mode_key"], row["filename"]): row
        for row in results
        if row["codec"] == "av2"
    }
    for row in results:
        if row["codec"] != "vvc":
            continue
        av2 = av2_rows.get((row["mode_key"], row["filename"]))
        if av2 is None:
            continue
        if av2["bytes"] > 0:
            byte_ratio = row["bytes"] / av2["bytes"]
            row["av2_byte_ratio"] = byte_ratio
            row["av2_byte_delta_pct"] = (byte_ratio - 1.0) * 100.0
        if av2["fps"] > 0.0:
            row["av2_fps_ratio"] = row["fps"] / av2["fps"]
        av2_psnr = av2.get("psnr_all_mean")
        row_psnr = row.get("psnr_all_mean")
        if av2_psnr is not None and row_psnr is not None:
            if math.isfinite(av2_psnr) and math.isfinite(row_psnr):
                row["av2_psnr_delta"] = row_psnr - av2_psnr


def markdown_report(report: dict[str, Any], skipped: int) -> str:
    lines = [
        f"# Encode Matrix: {report['run_name']}",
        "",
        f"- Set: `{report['set']}`",
        f"- AV2 GOP: `{report.get('av2_gop', legacy_gop(report.get('av2_predictive', True)))}`",
        f"- VVC GOP: `{report.get('vvc_gop', legacy_gop(report.get('vvc_predictive', True)))}`",
        f"- AV2 lossy QP: `{report['av2_lossy_qp']}`",
        f"- VVC fast search: `{report.get('vvc_fast_search', 'off')}`",
        f"- Write recon: `{report.get('write_recon', True)}`",
        f"- Cleanup recon: `{report.get('cleanup_recon', False)}`",
        f"- Cleanup output: `{report.get('cleanup_output', False)}`",
        f"- Cleanup vectors: `{report.get('cleanup_vectors', False)}`",
        f"- Skipped combinations: `{skipped}`",
        "",
    ]
    lines.extend(tradeoff_scale_rows(report["results"]))
    lines.extend(av2_vvc_aggregate_rows(report["results"]))
    lines.extend(
        [
            "",
            "| Codec | Mode | Vector | Format | Frames | Bytes | Bitrate kb/s | FPS | PSNR mean | Delta bytes | Delta FPS | Delta PSNR | Tradeoff | AV2 FPS | AV2 bytes | AV2 PSNR | Log |",
            "|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|---:|---|",
        ]
    )
    for row in report["results"]:
        lines.append(
            "| {codec} | {mode} | {vector} | {format} | {frames} | {bytes} | {bitrate} | {fps:.2f} | "
            "{psnr} | {delta_bytes} | {delta_fps} | {delta_psnr} | {tradeoff} | "
            "{av2_fps} | {av2_bytes} | {av2_psnr} | {log} |".format(
                codec=row["codec"],
                mode=row["mode"],
                vector=row["vector"],
                format=row["format"],
                frames=row["frames"],
                bytes=row["bytes"],
                bitrate=format_optional_one_decimal(row.get("bitrate_kbps")),
                fps=row["fps"],
                psnr=format_optional_float(row.get("psnr_all_mean")),
                delta_bytes=format_optional_int(row.get("delta_bytes")),
                delta_fps=format_optional_delta_float(row.get("delta_fps")),
                delta_psnr=format_optional_delta_float(row.get("delta_psnr_all_mean")),
                tradeoff=format_tradeoff(row),
                av2_fps=format_optional_ratio(row.get("av2_fps_ratio")),
                av2_bytes=format_optional_percent(row.get("av2_byte_delta_pct")),
                av2_psnr=format_optional_delta_float(row.get("av2_psnr_delta")),
                log=row["log"],
            )
        )
    lines.extend(total_rows(report["results"]))
    return "\n".join(lines)


def av2_vvc_aggregate_rows(results: list[dict[str, Any]]) -> list[str]:
    lines = [
        "",
        "## AV2/VVC Aggregate",
        "",
        "| Mode | AV2 bytes | VVC bytes | VVC bytes vs AV2 | AV2 FPS | VVC FPS | VVC FPS / AV2 | AV2 PSNR | VVC PSNR | VVC PSNR - AV2 |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    rows = 0
    for mode_key in ("lossless", "lossy"):
        av2 = aggregate_codec_mode(results, "av2", mode_key)
        vvc = aggregate_codec_mode(results, "vvc", mode_key)
        if av2 is None or vvc is None:
            continue
        rows += 1
        psnr_delta = None
        if finite_number(av2["psnr"]) and finite_number(vvc["psnr"]):
            psnr_delta = vvc["psnr"] - av2["psnr"]
        lines.append(
            "| {mode} | {av2_bytes} | {vvc_bytes} | {byte_delta} | "
            "{av2_fps:.2f} | {vvc_fps:.2f} | {fps_ratio} | "
            "{av2_psnr} | {vvc_psnr} | {psnr_delta} |".format(
                mode=mode_key,
                av2_bytes=int(av2["bytes"]),
                vvc_bytes=int(vvc["bytes"]),
                byte_delta=format_optional_percent(
                    (vvc["bytes"] / av2["bytes"] - 1.0) * 100.0
                    if av2["bytes"] > 0.0
                    else None
                ),
                av2_fps=av2["fps"],
                vvc_fps=vvc["fps"],
                fps_ratio=format_optional_ratio(
                    vvc["fps"] / av2["fps"] if av2["fps"] > 0.0 else None
                ),
                av2_psnr=format_optional_float(av2["psnr"]),
                vvc_psnr=format_optional_float(vvc["psnr"]),
                psnr_delta=format_optional_delta_float(psnr_delta),
            )
        )
    return lines if rows else []


def aggregate_codec_mode(
    results: list[dict[str, Any]], codec: str, mode_key: str
) -> dict[str, float] | None:
    rows = [
        row
        for row in results
        if row["codec"] == codec and row["mode_key"] == mode_key
    ]
    if not rows:
        return None
    frames = sum(row["frames"] for row in rows)
    seconds = sum(row["seconds"] for row in rows)
    return {
        "bytes": float(sum(row["bytes"] for row in rows)),
        "fps": frames / seconds if seconds > 0.0 else math.inf,
        "psnr": aggregate_psnr(row.get("psnr_all_mean") for row in rows),
    }


def aggregate_psnr(values: Any) -> float | None:
    values = [value for value in values if value is not None]
    if not values:
        return None
    finite = [value for value in values if math.isfinite(value)]
    if finite:
        return sum(finite) / len(finite)
    if all(math.isinf(value) for value in values):
        return math.inf
    return None


def total_rows(results: list[dict[str, Any]]) -> list[str]:
    totals: dict[tuple[str, str], dict[str, float]] = {}
    for row in results:
        key = (row["codec"], row["mode"])
        total = totals.setdefault(
            key,
            {"frames": 0.0, "bytes": 0.0, "seconds": 0.0, "duration_seconds": 0.0},
        )
        total["frames"] += row["frames"]
        total["bytes"] += row["bytes"]
        total["seconds"] += row["seconds"]
        if row.get("duration_seconds") is not None:
            total["duration_seconds"] += row["duration_seconds"]
    if not totals:
        return []
    lines = [
        "",
        "## Totals",
        "",
        "| Codec | Mode | Frames | Bytes | Bitrate kb/s | FPS |",
        "|---|---|---:|---:|---:|---:|",
    ]
    for (codec, mode), total in sorted(totals.items()):
        fps = total["frames"] / total["seconds"] if total["seconds"] > 0.0 else math.inf
        bitrate = (
            (total["bytes"] * 8.0 / total["duration_seconds"]) / 1000.0
            if total["duration_seconds"] > 0.0
            else None
        )
        lines.append(
            f"| {codec} | {mode} | {int(total['frames'])} | {int(total['bytes'])} | "
            f"{format_optional_one_decimal(bitrate)} | {fps:.2f} |"
        )
    return lines


def tradeoff_scale_rows(results: list[dict[str, Any]]) -> list[str]:
    scored = [row for row in results if "tradeoff_score" in row]
    if not scored:
        return []
    lines = [
        "## Tradeoff Scale",
        "",
        "Higher is better. Score = 10*log2(FPS ratio) + 4*log2(baseline bytes / current bytes) + 8*PSNR dB delta.",
        "Status is `accept` for clear aggregate wins, `watch` for useful wins with local bitrate/PSNR concerns, and `regress` for changes that need more work.",
        "",
        "| Codec | Mode | Rows | Average score | Accept | Watch | Regress |",
        "|---|---|---:|---:|---:|---:|---:|",
    ]
    totals: dict[tuple[str, str], dict[str, float]] = {}
    for row in scored:
        key = (row["codec"], row["mode"])
        total = totals.setdefault(
            key,
            {
                "rows": 0.0,
                "score": 0.0,
                "accept": 0.0,
                "watch": 0.0,
                "regress": 0.0,
            },
        )
        total["rows"] += 1.0
        total["score"] += row["tradeoff_score"]
        status = row.get("tradeoff_status")
        if status in {"accept", "watch", "regress"}:
            total[status] += 1.0
    for (codec, mode), total in sorted(totals.items()):
        rows = int(total["rows"])
        average = total["score"] / total["rows"] if total["rows"] else 0.0
        lines.append(
            f"| {codec} | {mode} | {rows} | {average:+.1f} | "
            f"{int(total['accept'])} | {int(total['watch'])} | {int(total['regress'])} |"
        )
    return lines


def mode_label(codec: str, mode: str, args: argparse.Namespace) -> str:
    if codec == "av2":
        gop = f"+gop={args.av2_gop}"
        if mode == "lossy":
            return f"qp={args.av2_lossy_qp}{gop}"
        return f"lossless{gop}"
    if codec == "vvc" and args.vvc_fast_search != "off":
        gop = f"+gop={args.vvc_gop}"
        if mode == "lossy":
            return f"qp={args.vvc_lossy_qp}{gop}+fast={args.vvc_fast_search}"
        return f"lossless{gop}+fast={args.vvc_fast_search}"
    if codec == "vvc" and mode == "lossy":
        return f"qp={args.vvc_lossy_qp}+gop={args.vvc_gop}"
    return mode


def legacy_gop(predictive: bool) -> int:
    return -1 if predictive else 0


def codec_extension(codec: str) -> str:
    return {"av2": "obu", "vvc": "vvc"}.get(codec, codec)


def raw_extension(vector: generate_test_vectors.TestVector) -> str:
    return "rgb" if vector.fmt in {"gbrp8", "rgb24"} else "yuv"


def source_fps_value(value: str | None) -> float | None:
    if value is None:
        return None
    try:
        return float(Fraction(value))
    except ValueError:
        return None


def require_non_empty(path: Path, label: str, vector_name: str, log: Path) -> None:
    if not path.exists() or path.stat().st_size == 0:
        raise SystemExit(f"{label} missing or empty for {vector_name}; see {relpath(log)}")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        while chunk := file.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


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


def parse_gop(value: str) -> int:
    try:
        gop = int(value, 10)
    except ValueError as err:
        raise argparse.ArgumentTypeError(
            f"GOP expects an integer from -1 through 65535, got '{value}'"
        ) from err
    if not (-1 <= gop <= 65535):
        raise argparse.ArgumentTypeError(
            f"GOP expects an integer from -1 through 65535, got '{value}'"
        )
    return gop


def parse_positive_int(value: str) -> int:
    try:
        parsed = int(value, 10)
    except ValueError as err:
        raise argparse.ArgumentTypeError(f"expected a positive integer, got '{value}'") from err
    if parsed <= 0:
        raise argparse.ArgumentTypeError(f"expected a positive integer, got '{value}'")
    return parsed


def delta_label(result: dict[str, Any]) -> str:
    parts = []
    if "delta_bytes" in result:
        parts.append(f"bytes_delta={result['delta_bytes']:+d}")
    if "delta_fps" in result:
        parts.append(f"fps_delta={result['delta_fps']:+.2f}")
    if "tradeoff_score" in result:
        parts.append(
            "score={score:+.1f}:{status}".format(
                score=result["tradeoff_score"],
                status=result.get("tradeoff_status", "n/a"),
            )
        )
    return " " + " ".join(parts) if parts else ""


def format_optional_float(value: Any) -> str:
    if value is None:
        return "n/a"
    if isinstance(value, float) and math.isinf(value):
        return "inf"
    return f"{value:.3f}"


def format_optional_one_decimal(value: Any) -> str:
    if value is None:
        return "n/a"
    if isinstance(value, float) and math.isinf(value):
        return "inf"
    return f"{value:.1f}"


def format_optional_delta_float(value: Any) -> str:
    if value is None:
        return "n/a"
    return f"{value:+.2f}"


def format_optional_int(value: Any) -> str:
    if value is None:
        return "n/a"
    return f"{value:+d}"


def format_optional_ratio(value: Any) -> str:
    if not finite_number(value):
        return "n/a"
    return f"{value:.2f}x"


def format_optional_percent(value: Any) -> str:
    if not finite_number(value):
        return "n/a"
    return f"{value:+.1f}%"


def format_tradeoff(row: dict[str, Any]) -> str:
    score = row.get("tradeoff_score")
    if not finite_number(score):
        return "n/a"
    return "{score:+.1f} {status}".format(
        score=score,
        status=row.get("tradeoff_status", "n/a"),
    )


def finite_positive(value: Any) -> bool:
    return finite_number(value) and value > 0.0


def finite_number(value: Any) -> bool:
    return isinstance(value, (float, int)) and math.isfinite(value)


def relpath(path: Path) -> Path:
    try:
        return path.resolve().relative_to(REPO_ROOT)
    except ValueError:
        return path


if __name__ == "__main__":
    raise SystemExit(main())
