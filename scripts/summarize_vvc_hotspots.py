#!/usr/bin/env python3
"""Summarize VVC hotspot profile runs."""

from __future__ import annotations

import argparse
import json
import math
from collections import defaultdict
from pathlib import Path
from typing import Any, Iterable


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_RUN_DIR = REPO_ROOT / "verification" / "generated" / "profiling" / "vvc_hotspots" / "latest"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run_dir", type=Path, nargs="?", default=DEFAULT_RUN_DIR)
    parser.add_argument("--encode-matrix-json", type=Path)
    parser.add_argument("--out-json", type=Path)
    parser.add_argument("--out-md", type=Path)
    parser.add_argument("--top", type=int, default=16)
    args = parser.parse_args()

    run_dir = args.run_dir.resolve()
    matrix_path = (args.encode_matrix_json or default_matrix_path(run_dir)).resolve()
    if not matrix_path.exists():
        raise SystemExit(f"missing encode matrix JSON: {relpath(matrix_path)}")

    matrix = json.loads(matrix_path.read_text(encoding="utf-8"))
    rows = [row for row in matrix.get("results", []) if row.get("codec") == "vvc"]
    labels = stats_labels(rows)
    stats_files = list(discover_stats_files(run_dir, labels))
    if not stats_files:
        raise SystemExit(f"no VVC stats JSONL files found under {relpath(run_dir)}")

    records = [
        (labels.get(path.resolve(), fallback_label(path)), record)
        for path in stats_files
        for record in read_vvc_stats(path)
    ]
    summary = build_summary(run_dir, matrix_path, rows, stats_files, records, args.top)

    out_json = (args.out_json or run_dir / "vvc_hotspots_summary.json").resolve()
    out_md = (args.out_md or run_dir / "vvc_hotspots_summary.md").resolve()
    out_json.parent.mkdir(parents=True, exist_ok=True)
    out_md.parent.mkdir(parents=True, exist_ok=True)
    out_json.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    out_md.write_text(markdown_summary(summary) + "\n", encoding="utf-8")

    print(f"wrote {relpath(out_json)}")
    print(f"wrote {relpath(out_md)}")
    return 0


def default_matrix_path(run_dir: Path) -> Path:
    run_name = run_dir.name
    return run_dir / "encode_matrix" / f"{run_name}.json"


def stats_labels(rows: list[dict[str, Any]]) -> dict[Path, dict[str, str]]:
    labels = {}
    for row in rows:
        stats = row.get("vvc_stats")
        if not stats:
            continue
        path = Path(stats)
        if not path.is_absolute():
            path = REPO_ROOT / path
        labels[path.resolve()] = {
            "mode": str(row.get("mode_key") or row.get("mode") or ""),
            "vector": str(row.get("vector") or row.get("filename") or path.stem),
            "format": str(row.get("format") or ""),
        }
    return labels


def discover_stats_files(run_dir: Path, labels: dict[Path, dict[str, str]]) -> Iterable[Path]:
    seen: set[Path] = set()
    for path in labels:
        if path.exists():
            seen.add(path)
            yield path
    for path in sorted((run_dir / "stats").glob("*.jsonl")):
        resolved = path.resolve()
        if resolved not in seen:
            seen.add(resolved)
            yield resolved


def fallback_label(path: Path) -> dict[str, str]:
    name = path.stem
    for prefix in ("vvc_lossless_", "vvc_lossy_"):
        if name.startswith(prefix):
            return {"mode": prefix.removeprefix("vvc_").removesuffix("_"), "vector": name[len(prefix) :], "format": ""}
    return {"mode": "unknown", "vector": name, "format": ""}


def read_vvc_stats(path: Path) -> Iterable[dict[str, Any]]:
    with path.open("r", encoding="utf-8") as source:
        for line_number, line in enumerate(source, start=1):
            stripped = line.strip()
            if not stripped:
                continue
            try:
                record = json.loads(stripped)
            except json.JSONDecodeError as exc:
                raise SystemExit(f"{relpath(path)}:{line_number}: invalid JSON: {exc}") from exc
            if record.get("kind") == "framefinery.vvc.stats.v1":
                yield record


def build_summary(
    run_dir: Path,
    matrix_path: Path,
    rows: list[dict[str, Any]],
    stats_files: list[Path],
    records: list[tuple[dict[str, str], dict[str, Any]]],
    top: int,
) -> dict[str, Any]:
    return {
        "run_dir": str(relpath(run_dir)),
        "encode_matrix_json": str(relpath(matrix_path)),
        "stats_files": [str(relpath(path)) for path in stats_files],
        "matrix_totals": matrix_totals(rows),
        "top_stages": top_stages(records, top),
        "top_timed_counters": top_timed_counters(records, top),
        "candidate_pressure": candidate_pressure(records),
    }


def matrix_totals(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    totals: dict[str, dict[str, float]] = {}
    for row in rows:
        mode = str(row.get("mode_key") or row.get("mode") or "unknown")
        total = totals.setdefault(mode, {"rows": 0.0, "frames": 0.0, "bytes": 0.0, "seconds": 0.0, "psnr_sum": 0.0, "psnr_count": 0.0})
        total["rows"] += 1.0
        total["frames"] += float(row.get("frames") or 0)
        total["bytes"] += float(row.get("bytes") or 0)
        total["seconds"] += float(row.get("seconds") or 0.0)
        psnr = row.get("psnr_all_mean")
        if isinstance(psnr, (int, float)) and math.isfinite(psnr):
            total["psnr_sum"] += float(psnr)
            total["psnr_count"] += 1.0
    rows_out = []
    for mode, total in sorted(totals.items()):
        seconds = total["seconds"]
        rows_out.append(
            {
                "mode": mode,
                "rows": int(total["rows"]),
                "frames": int(total["frames"]),
                "bytes": int(total["bytes"]),
                "seconds": seconds,
                "fps": total["frames"] / seconds if seconds > 0.0 else math.inf,
                "psnr_mean": total["psnr_sum"] / total["psnr_count"] if total["psnr_count"] else None,
            }
        )
    return rows_out


def top_stages(
    records: list[tuple[dict[str, str], dict[str, Any]]],
    top: int,
) -> list[dict[str, Any]]:
    totals: dict[tuple[str, str], dict[str, int]] = defaultdict(lambda: {"nanos": 0, "count": 0})
    for label, record in records:
        mode = label["mode"]
        for stage in record.get("stages", []):
            key = (mode, str(stage.get("name") or ""))
            totals[key]["nanos"] += int(stage.get("ns") or 0)
            totals[key]["count"] += int(stage.get("count") or 0)
    return [
        {"mode": mode, "stage": stage, "nanos": data["nanos"], "count": data["count"]}
        for (mode, stage), data in sorted(totals.items(), key=lambda item: item[1]["nanos"], reverse=True)[:top]
    ]


def top_timed_counters(
    records: list[tuple[dict[str, str], dict[str, Any]]],
    top: int,
) -> list[dict[str, Any]]:
    totals: dict[tuple[str, str], int] = defaultdict(int)
    for label, record in records:
        mode = label["mode"]
        for counter in record.get("counters", []):
            name = str(counter.get("name") or "")
            if name.endswith("_nanos"):
                totals[(mode, name)] += int(counter.get("value") or 0)
    return [
        {"mode": mode, "counter": counter, "nanos": nanos}
        for (mode, counter), nanos in sorted(totals.items(), key=lambda item: item[1], reverse=True)[:top]
    ]


def candidate_pressure(records: list[tuple[dict[str, str], dict[str, Any]]]) -> list[dict[str, Any]]:
    wanted = {
        "luma_tu_count",
        "chroma_tu_count",
        "luma_candidate_count",
        "chroma_candidate_count",
        "luma_rd_refinement_attempts",
        "chroma_rd_refinement_attempts",
        "luma_rd_cached_candidates",
        "luma_rd_generated_candidates",
        "chroma_rd_cached_candidates",
        "chroma_rd_generated_candidates",
        "luma_residual_build_calls",
        "chroma_residual_build_calls",
        "luma_transform_skip_candidate_count",
        "chroma_transform_skip_candidate_count",
    }
    totals: dict[tuple[str, str], int] = defaultdict(int)
    frames: dict[str, set[tuple[str, int]]] = defaultdict(set)
    for label, record in records:
        mode = label["mode"]
        frames[mode].add((label["vector"], int(record.get("frame_index") or 0)))
        for counter in record.get("counters", []):
            name = str(counter.get("name") or "")
            if name in wanted:
                totals[(mode, name)] += int(counter.get("value") or 0)
    out = []
    for (mode, counter), value in sorted(totals.items()):
        frame_count = len(frames[mode])
        out.append(
            {
                "mode": mode,
                "counter": counter,
                "value": value,
                "per_frame": value / frame_count if frame_count else 0.0,
            }
        )
    return out


def markdown_summary(summary: dict[str, Any]) -> str:
    lines = [
        f"# VVC Hotspot Summary: {Path(summary['run_dir']).name}",
        "",
        f"- Encode matrix: `{summary['encode_matrix_json']}`",
        f"- Stats files: `{len(summary['stats_files'])}`",
        "",
        "## Matrix Totals",
        "",
        "| Mode | Rows | Frames | Bytes | Seconds | FPS | PSNR mean |",
        "|---|---:|---:|---:|---:|---:|---:|",
    ]
    for row in summary["matrix_totals"]:
        lines.append(
            f"| {row['mode']} | {row['rows']} | {row['frames']} | {row['bytes']} | "
            f"{row['seconds']:.3f} | {row['fps']:.3f} | {format_optional_float(row['psnr_mean'])} |"
        )
    lines.extend(
        [
            "",
            "## Top Stages",
            "",
            "| Mode | Stage | Count | Time ms | Avg us/call |",
            "|---|---|---:|---:|---:|",
        ]
    )
    for row in summary["top_stages"]:
        count = row["count"]
        avg = row["nanos"] / count / 1000.0 if count else 0.0
        lines.append(
            f"| {row['mode']} | `{row['stage']}` | {count} | {row['nanos'] / 1_000_000.0:.3f} | {avg:.3f} |"
        )
    lines.extend(
        [
            "",
            "## Top Timed Counters",
            "",
            "| Mode | Counter | Time ms |",
            "|---|---|---:|",
        ]
    )
    for row in summary["top_timed_counters"]:
        lines.append(f"| {row['mode']} | `{row['counter']}` | {row['nanos'] / 1_000_000.0:.3f} |")
    lines.extend(
        [
            "",
            "## Candidate Pressure",
            "",
            "| Mode | Counter | Total | Per frame |",
            "|---|---|---:|---:|",
        ]
    )
    for row in summary["candidate_pressure"]:
        lines.append(f"| {row['mode']} | `{row['counter']}` | {row['value']} | {row['per_frame']:.2f} |")
    return "\n".join(lines)


def format_optional_float(value: Any) -> str:
    if value is None:
        return "n/a"
    if isinstance(value, float) and math.isinf(value):
        return "inf"
    return f"{float(value):.3f}"


def relpath(path: Path) -> Path:
    try:
        return path.resolve().relative_to(REPO_ROOT)
    except ValueError:
        return path


if __name__ == "__main__":
    raise SystemExit(main())
