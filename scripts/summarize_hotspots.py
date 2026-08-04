#!/usr/bin/env python3
"""Summarize codec hotspot runs and emit a code-browser profile overlay."""

from __future__ import annotations

import argparse
import json
import math
from collections import defaultdict
from pathlib import Path
from typing import Any, Iterable


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_RUN_DIR = REPO_ROOT / "verification" / "generated" / "profiling" / "hotspots" / "latest"

STAGE_MAP: dict[str, dict[str, tuple[str, str | None]]] = {
    "av2": {
        "read_frame": ("framefinery_codecs::picture", "read_input_frame"),
        "rgb24_to_planar_gbr": ("framefinery_codecs::av2::image::rgb", "rgb24_to_planar_gbr"),
        "planar_gbr_to_rgb24": ("framefinery_codecs::av2::image::rgb", "planar_gbr_to_rgb24"),
        "lossless_ibc_search": ("framefinery_codecs::av2::inter::ibc", None),
        "lossless_palette_build": ("framefinery_codecs::av2::palette", "build_luma_palette_lossless"),
        "lossless_headers": ("framefinery_codecs::av2::headers", None),
        "lossless_predictive_headers": ("framefinery_codecs::av2::headers", None),
        "lossless_tile_payload": ("framefinery_codecs::av2::tile", None),
        "lossless_inter_tiles": ("framefinery_codecs::av2::inter", None),
        "lossless_show_existing_frame": ("framefinery_codecs::av2::encode", None),
        "lossless_entropy_pack": ("framefinery_codecs::av2::obu", "append_obu"),
        "lossy_headers": ("framefinery_codecs::av2::headers", None),
        "lossy_predictive_headers": ("framefinery_codecs::av2::headers", None),
        "lossy_tile_payload": ("framefinery_codecs::av2::tile", None),
        "lossy_zero_mv_inter_tiles": ("framefinery_codecs::av2::inter", None),
        "lossy_show_existing_frame": ("framefinery_codecs::av2::encode", None),
        "lossy_entropy_pack": ("framefinery_codecs::av2::obu", "append_obu"),
        "mvp_444_mode_decision": ("framefinery_codecs::av2::frame_mode", None),
        "mvp_444_bitstream": ("framefinery_codecs::av2::obu", None),
        "mvp_444_reconstruction": ("framefinery_codecs::av2::reconstruction", None),
        "bitstream_write": ("framefinery_codecs::av2::encode", None),
        "write_reconstruction": ("framefinery_codecs::av2::encode", None),
        "frame_metrics": ("framefinery_codecs::av2::encode", None),
    },
    "vvc": {
        "read_frame": ("framefinery_codecs::picture", "read_input_frame"),
        "sample_frame": ("framefinery_codecs::vvc::sampling", None),
        "ctu_quantize": ("framefinery_codecs::vvc::residual::quant", None),
        "frame_entropy_write": ("framefinery_codecs::vvc::cabac", None),
        "frame_recon_finalize": ("framefinery_codecs::vvc::reconstruction", None),
        "write_reconstruction": ("framefinery_codecs::vvc::encode", None),
        "frame_metrics": ("framefinery_codecs::vvc::encode", None),
        "frame_entropy_build_nanos": ("framefinery_codecs::vvc::cabac", None),
        "frame_annexb_write_nanos": ("framefinery_codecs::vvc::bitstream::nal", "write_annex_b"),
        "luma_mode_search_nanos": ("framefinery_codecs::vvc::residual::quant::luma_mode", None),
        "luma_rd_refinement_nanos": (
            "framefinery_codecs::vvc::residual::quant::luma_prediction",
            None,
        ),
        "luma_mrl_nanos": ("framefinery_codecs::vvc::residual::quant::luma_mode", None),
        "luma_bdpcm_nanos": ("framefinery_codecs::vvc::residual::quant::luma_mode", None),
        "luma_finalize_nanos": ("framefinery_codecs::vvc::residual::quant::luma_mode", None),
        "luma_dc_prediction_nanos": (
            "framefinery_codecs::vvc::residual::quant::luma_prediction",
            None,
        ),
        "luma_planar_prediction_nanos": (
            "framefinery_codecs::vvc::residual::quant::luma_prediction",
            None,
        ),
        "luma_directional_prediction_nanos": (
            "framefinery_codecs::vvc::residual::quant::directional",
            None,
        ),
        "luma_mrl_prediction_nanos": (
            "framefinery_codecs::vvc::residual::quant::luma_prediction",
            None,
        ),
        "luma_bdpcm_prediction_nanos": (
            "framefinery_codecs::vvc::residual::quant::luma_prediction",
            None,
        ),
        "luma_residual_build_nanos": (
            "framefinery_codecs::vvc::residual::quant::luma_residual",
            None,
        ),
        "luma_mode_score_nanos": ("framefinery_codecs::vvc::residual::quant::luma_mode", None),
        "luma_rd_prediction_nanos": (
            "framefinery_codecs::vvc::residual::quant::luma_prediction",
            None,
        ),
        "luma_rd_residual_build_nanos": (
            "framefinery_codecs::vvc::residual::quant::luma_residual",
            None,
        ),
        "luma_rd_scoring_nanos": (
            "framefinery_codecs::vvc::residual::quant::luma_prediction",
            None,
        ),
        "luma_transform_skip_candidate_nanos": (
            "framefinery_codecs::vvc::residual::quant::transform_skip",
            None,
        ),
        "luma_transformed_quant_nanos": ("framefinery_codecs::vvc::residual::quant", None),
        "luma_residual_recon_nanos": ("framefinery_codecs::vvc::residual::recon", None),
        "luma_fill_nanos": ("framefinery_codecs::vvc::residual::quant::luma_residual", None),
        "chroma_mode_search_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_mode",
            None,
        ),
        "chroma_rd_refinement_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_residual",
            None,
        ),
        "chroma_bdpcm_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_mode",
            None,
        ),
        "chroma_finalize_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_mode",
            None,
        ),
        "chroma_derived_prediction_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_mode",
            None,
        ),
        "chroma_explicit_prediction_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_mode",
            None,
        ),
        "chroma_cclm_prediction_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_mode",
            None,
        ),
        "chroma_bdpcm_prediction_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_mode",
            None,
        ),
        "chroma_residual_build_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_residual",
            None,
        ),
        "chroma_mode_score_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_mode",
            None,
        ),
        "chroma_rd_prediction_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_residual",
            None,
        ),
        "chroma_rd_residual_build_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_residual",
            None,
        ),
        "chroma_rd_scoring_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_residual",
            None,
        ),
        "chroma_transform_skip_candidate_nanos": (
            "framefinery_codecs::vvc::residual::quant::transform_skip",
            None,
        ),
        "chroma_transformed_quant_nanos": ("framefinery_codecs::vvc::residual::quant", None),
        "chroma_residual_recon_nanos": ("framefinery_codecs::vvc::residual::recon", None),
        "chroma_fill_nanos": (
            "framefinery_codecs::vvc::residual::quant::chroma_residual",
            None,
        ),
    },
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run_dir", type=Path, nargs="?", default=DEFAULT_RUN_DIR)
    parser.add_argument("--encode-matrix-json", type=Path)
    parser.add_argument("--out-json", type=Path)
    parser.add_argument("--out-md", type=Path)
    parser.add_argument("--profile-json", type=Path)
    parser.add_argument("--codec", action="append", choices=("av2", "vvc"), default=[])
    parser.add_argument("--top", type=int, default=18)
    args = parser.parse_args()

    run_dir = args.run_dir.resolve()
    matrix_path = (args.encode_matrix_json or default_matrix_path(run_dir)).resolve()
    if not matrix_path.exists():
        raise SystemExit(f"missing encode matrix JSON: {relpath(matrix_path)}")

    matrix = json.loads(matrix_path.read_text(encoding="utf-8"))
    codecs = set(args.codec or ["av2", "vvc"])
    rows = [row for row in matrix.get("results", []) if row.get("codec") in codecs]
    labels = stats_labels(rows)
    stats_files = list(discover_stats_files(run_dir, labels, codecs))
    if not stats_files:
        raise SystemExit(f"no AV2/VVC stats JSONL files found under {relpath(run_dir)}")

    records = [
        (labels.get(path.resolve(), fallback_label(path)), record)
        for path in stats_files
        for record in read_stats(path, codecs)
    ]
    summary = build_summary(run_dir, matrix_path, rows, stats_files, records, args.top)

    out_json = (args.out_json or run_dir / "hotspots_summary.json").resolve()
    out_md = (args.out_md or run_dir / "hotspots_summary.md").resolve()
    profile_json = (args.profile_json or run_dir / "hotspots_profile.json").resolve()
    out_json.parent.mkdir(parents=True, exist_ok=True)
    out_md.parent.mkdir(parents=True, exist_ok=True)
    profile_json.parent.mkdir(parents=True, exist_ok=True)
    out_json.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    out_md.write_text(markdown_summary(summary) + "\n", encoding="utf-8")
    profile_json.write_text(
        json.dumps(summary["profile"], indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    print(f"wrote {relpath(out_json)}")
    print(f"wrote {relpath(out_md)}")
    print(f"wrote {relpath(profile_json)}")
    return 0


def default_matrix_path(run_dir: Path) -> Path:
    run_name = run_dir.name
    return run_dir / "encode_matrix" / f"{run_name}.json"


def stats_labels(rows: list[dict[str, Any]]) -> dict[Path, dict[str, str]]:
    labels = {}
    for row in rows:
        codec = str(row.get("codec") or "")
        stats = row.get(f"{codec}_stats")
        if not stats:
            continue
        path = Path(stats)
        if not path.is_absolute():
            path = REPO_ROOT / path
        labels[path.resolve()] = {
            "codec": codec,
            "mode": str(row.get("mode_key") or row.get("mode") or ""),
            "vector": str(row.get("vector") or row.get("filename") or path.stem),
            "format": str(row.get("format") or ""),
        }
    return labels


def discover_stats_files(
    run_dir: Path, labels: dict[Path, dict[str, str]], codecs: set[str]
) -> Iterable[Path]:
    seen: set[Path] = set()
    for path in labels:
        if path.exists():
            seen.add(path)
            yield path
    for path in sorted((run_dir / "stats").glob("*.jsonl")):
        resolved = path.resolve()
        if resolved in seen:
            continue
        if path.name.split("_", 1)[0] in codecs:
            seen.add(resolved)
            yield path


def fallback_label(path: Path) -> dict[str, str]:
    name = path.stem
    codec = name.split("_", 1)[0] if "_" in name else "unknown"
    mode = "unknown"
    for candidate in ("lossless", "lossy"):
        if f"_{candidate}_" in name:
            mode = candidate
            break
    return {"codec": codec, "mode": mode, "vector": name, "format": ""}


def read_stats(path: Path, codecs: set[str]) -> Iterable[dict[str, Any]]:
    with path.open("r", encoding="utf-8") as source:
        for line_number, line in enumerate(source, start=1):
            stripped = line.strip()
            if not stripped:
                continue
            try:
                record = json.loads(stripped)
            except json.JSONDecodeError as exc:
                raise SystemExit(f"{relpath(path)}:{line_number}: invalid JSON: {exc}") from exc
            codec = codec_for_record(record)
            if codec in codecs:
                yield record


def codec_for_record(record: dict[str, Any]) -> str:
    kind = str(record.get("kind") or "")
    if kind == "framefinery.av2.stats.v1":
        return "av2"
    if kind == "framefinery.vvc.stats.v1":
        return "vvc"
    return ""


def build_summary(
    run_dir: Path,
    matrix_path: Path,
    rows: list[dict[str, Any]],
    stats_files: list[Path],
    records: list[tuple[dict[str, str], dict[str, Any]]],
    top: int,
) -> dict[str, Any]:
    profile_events = profile_events_from_records(records)
    profile = build_profile(profile_events)
    return {
        "kind": "framefinery.hotspots.summary.v1",
        "run_dir": str(relpath(run_dir)),
        "encode_matrix_json": str(relpath(matrix_path)),
        "stats_files": [str(relpath(path)) for path in stats_files],
        "matrix_totals": matrix_totals(rows),
        "top_stages": top_stages(records, top),
        "top_modules": top_profile_modules(profile, top),
        "top_items": top_profile_items(profile, top),
        "profile": profile,
    }


def matrix_totals(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    totals: dict[tuple[str, str], dict[str, float]] = {}
    for row in rows:
        codec = str(row.get("codec") or "unknown")
        mode = str(row.get("mode_key") or row.get("mode") or "unknown")
        total = totals.setdefault(
            (codec, mode),
            {
                "rows": 0.0,
                "frames": 0.0,
                "bytes": 0.0,
                "seconds": 0.0,
                "psnr_sum": 0.0,
                "psnr_count": 0.0,
            },
        )
        total["rows"] += 1.0
        total["frames"] += float(row.get("frames") or 0)
        total["bytes"] += float(row.get("bytes") or 0)
        total["seconds"] += float(row.get("seconds") or 0.0)
        psnr = row.get("psnr_all_mean")
        if isinstance(psnr, (int, float)) and math.isfinite(psnr):
            total["psnr_sum"] += float(psnr)
            total["psnr_count"] += 1.0
    rows_out = []
    for (codec, mode), total in sorted(totals.items()):
        seconds = total["seconds"]
        rows_out.append(
            {
                "codec": codec,
                "mode": mode,
                "rows": int(total["rows"]),
                "frames": int(total["frames"]),
                "bytes": int(total["bytes"]),
                "seconds": seconds,
                "fps": total["frames"] / seconds if seconds > 0.0 else math.inf,
                "psnr_mean": total["psnr_sum"] / total["psnr_count"]
                if total["psnr_count"]
                else None,
            }
        )
    return rows_out


def top_stages(
    records: list[tuple[dict[str, str], dict[str, Any]]], top: int
) -> list[dict[str, Any]]:
    totals: dict[tuple[str, str, str], int] = defaultdict(int)
    for label, record in records:
        codec = codec_for_record(record) or label.get("codec", "unknown")
        mode = label["mode"]
        for stage in timed_entries(record):
            key = (codec, mode, stage["name"])
            totals[key] += stage["ns"]
    total_nanos = sum(totals.values())
    return [
        {
            "codec": codec,
            "mode": mode,
            "stage": stage,
            "nanos": nanos,
            "share": nanos / total_nanos if total_nanos else 0.0,
        }
        for (codec, mode, stage), nanos in sorted(
            totals.items(), key=lambda item: item[1], reverse=True
        )[:top]
    ]


def timed_entries(record: dict[str, Any]) -> Iterable[dict[str, Any]]:
    for stage in record.get("stages", []):
        yield {
            "name": str(stage.get("name") or ""),
            "ns": int(stage.get("ns") or 0),
            "source": "stage",
        }
    for counter in record.get("counters", []):
        name = str(counter.get("name") or "")
        if name.endswith("_nanos"):
            yield {
                "name": name,
                "ns": int(counter.get("value") or 0),
                "source": "counter",
            }


def profile_events_from_records(
    records: list[tuple[dict[str, str], dict[str, Any]]]
) -> list[dict[str, Any]]:
    events = []
    for label, record in records:
        codec = codec_for_record(record)
        for entry in timed_entries(record):
            module, item = STAGE_MAP.get(codec, {}).get(
                entry["name"], (f"framefinery_codecs::{codec}", None)
            )
            if entry["ns"] <= 0:
                continue
            events.append(
                {
                    "codec": codec,
                    "mode": label.get("mode", ""),
                    "vector": label.get("vector", ""),
                    "stage": entry["name"],
                    "source": entry["source"],
                    "module": module,
                    "item": item,
                    "nanos": entry["ns"],
                }
            )
    return events


def build_profile(events: list[dict[str, Any]]) -> dict[str, Any]:
    modules: dict[str, dict[str, Any]] = defaultdict(
        lambda: {"direct_ns": 0, "inclusive_ns": 0, "stages": defaultdict(int)}
    )
    items: dict[str, dict[str, Any]] = defaultdict(
        lambda: {"module": "", "item": "", "nanos": 0, "stages": defaultdict(int)}
    )
    total_ns = 0
    for event in events:
        module = event["module"]
        nanos = int(event["nanos"])
        total_ns += nanos
        modules[module]["direct_ns"] += nanos
        modules[module]["stages"][event["stage"]] += nanos
        for ancestor in module_ancestors(module):
            modules[ancestor]["inclusive_ns"] += nanos
        if event["item"]:
            key = f"{module}::{event['item']}"
            items[key]["module"] = module
            items[key]["item"] = event["item"]
            items[key]["nanos"] += nanos
            items[key]["stages"][event["stage"]] += nanos

    module_out = {}
    for module, data in modules.items():
        module_out[module] = {
            "direct_ns": data["direct_ns"],
            "inclusive_ns": data["inclusive_ns"],
            "share": data["inclusive_ns"] / total_ns if total_ns else 0.0,
            "stages": sorted_stage_map(data["stages"]),
        }
    item_out = {}
    for key, data in items.items():
        item_out[key] = {
            "module": data["module"],
            "item": data["item"],
            "nanos": data["nanos"],
            "share": data["nanos"] / total_ns if total_ns else 0.0,
            "stages": sorted_stage_map(data["stages"]),
        }
    return {
        "kind": "framefinery.hotspots.profile.v1",
        "metric": "wall_time_nanos",
        "total_ns": total_ns,
        "modules": module_out,
        "items": item_out,
        "events": sorted(events, key=lambda event: event["nanos"], reverse=True)[:100],
        "note": "Stage and counter buckets are wall-time measurements; nested buckets may be inclusive.",
    }


def module_ancestors(module: str) -> Iterable[str]:
    parts = module.split("::")
    for end in range(1, len(parts) + 1):
        yield "::".join(parts[:end])


def sorted_stage_map(stages: dict[str, int]) -> list[dict[str, Any]]:
    return [
        {"name": name, "nanos": nanos}
        for name, nanos in sorted(stages.items(), key=lambda item: item[1], reverse=True)
    ]


def top_profile_modules(profile: dict[str, Any], top: int) -> list[dict[str, Any]]:
    return [
        {
            "module": module,
            "inclusive_ns": data["inclusive_ns"],
            "direct_ns": data["direct_ns"],
            "share": data["share"],
        }
        for module, data in sorted(
            profile["modules"].items(),
            key=lambda item: item[1]["inclusive_ns"],
            reverse=True,
        )[:top]
    ]


def top_profile_items(profile: dict[str, Any], top: int) -> list[dict[str, Any]]:
    return [
        {"item": key, "nanos": data["nanos"], "share": data["share"]}
        for key, data in sorted(
            profile["items"].items(), key=lambda item: item[1]["nanos"], reverse=True
        )[:top]
    ]


def markdown_summary(summary: dict[str, Any]) -> str:
    lines = [
        f"# Hotspot Summary: {Path(summary['run_dir']).name}",
        "",
        f"- Encode matrix: `{summary['encode_matrix_json']}`",
        f"- Stats files: `{len(summary['stats_files'])}`",
        f"- Profile metric: `{summary['profile']['metric']}`",
        "",
        "## Matrix Totals",
        "",
        "| Codec | Mode | Rows | Frames | Bytes | Seconds | FPS | PSNR mean |",
        "|---|---|---:|---:|---:|---:|---:|---:|",
    ]
    for row in summary["matrix_totals"]:
        lines.append(
            f"| {row['codec']} | {row['mode']} | {row['rows']} | {row['frames']} | {row['bytes']} | "
            f"{row['seconds']:.3f} | {row['fps']:.3f} | {format_optional_float(row['psnr_mean'])} |"
        )
    lines.extend(
        [
            "",
            "## Top Timed Buckets",
            "",
            "| Codec | Mode | Stage | Time ms | Share |",
            "|---|---|---|---:|---:|",
        ]
    )
    for row in summary["top_stages"]:
        lines.append(
            f"| {row['codec']} | {row['mode']} | `{row['stage']}` | "
            f"{row['nanos'] / 1_000_000.0:.3f} | {row['share'] * 100.0:.2f}% |"
        )
    lines.extend(
        [
            "",
            "## Top Visualized Modules",
            "",
            "| Module | Inclusive ms | Direct ms | Share |",
            "|---|---:|---:|---:|",
        ]
    )
    for row in summary["top_modules"]:
        lines.append(
            f"| `{row['module']}` | {row['inclusive_ns'] / 1_000_000.0:.3f} | "
            f"{row['direct_ns'] / 1_000_000.0:.3f} | {row['share'] * 100.0:.2f}% |"
        )
    lines.extend(
        [
            "",
            "## Top Visualized Items",
            "",
            "| Item | Time ms | Share |",
            "|---|---:|---:|",
        ]
    )
    for row in summary["top_items"]:
        lines.append(
            f"| `{row['item']}` | {row['nanos'] / 1_000_000.0:.3f} | {row['share'] * 100.0:.2f}% |"
        )
    lines.extend(["", f"> {summary['profile']['note']}"])
    return "\n".join(lines)


def format_optional_float(value: float | None) -> str:
    if value is None:
        return "n/a"
    if math.isinf(value):
        return "inf"
    return f"{value:.3f}"


def relpath(path: Path) -> Path:
    try:
        return path.resolve().relative_to(REPO_ROOT)
    except ValueError:
        return path


if __name__ == "__main__":
    raise SystemExit(main())
