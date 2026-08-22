#!/usr/bin/env python3
"""Shared encode-matrix byte/quality/speed tradeoff scoring."""

from __future__ import annotations

import math
from typing import Any


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
TRADEOFF_ACCEPT_STATUS = "accept"
TRADEOFF_WATCH_STATUS = "watch"
TRADEOFF_REGRESS_STATUS = "regress"


def project_metric_tradeoff(
    *,
    baseline_bytes: int | float | None,
    current_bytes: int | float | None,
    baseline_psnr: float | None,
    current_psnr: float | None,
    baseline_fps: float | None,
    current_fps: float | None,
) -> dict[str, Any] | None:
    """Project byte, quality, and speed metrics into one probe result.

    This intentionally stays local to one comparable row: positive FPS deltas
    help, positive byte deltas hurt, and positive PSNR deltas help. Log-scaled
    FPS/byte ratios make percentage changes comparable across small and large
    vectors while PSNR remains in decibels. The status then applies hard gates
    so a large speedup cannot hide severe quality, bitrate, or speed
    regressions.
    """
    result: dict[str, Any] = {}

    if finite_positive(baseline_bytes) and finite_positive(current_bytes):
        byte_ratio = float(current_bytes) / float(baseline_bytes)
        result["baseline_byte_ratio"] = byte_ratio
        result["delta_bytes_pct"] = (byte_ratio - 1.0) * 100.0

    if finite_positive(baseline_fps) and finite_positive(current_fps):
        fps_ratio = float(current_fps) / float(baseline_fps)
        result["baseline_fps_ratio"] = fps_ratio
        result["delta_fps_pct"] = (fps_ratio - 1.0) * 100.0

    if finite_number(baseline_psnr) and finite_number(current_psnr):
        result["delta_psnr_all_mean"] = float(current_psnr) - float(baseline_psnr)

    score = projected_tradeoff_score(result)
    if score is None:
        return result or None
    result["tradeoff_score"] = score
    result["tradeoff_hard_regression"] = has_hard_tradeoff_regression(result)
    result["tradeoff_status"] = classify_tradeoff_result(result)
    return result


def projected_tradeoff_score(result: dict[str, Any]) -> float | None:
    """Return the weighted scalar score for already-computed metric deltas."""
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
        score += TRADEOFF_PSNR_DB_WEIGHT * float(psnr_delta)
        scored = True

    if not scored:
        return None
    return score


def classify_tradeoff_result(result: dict[str, Any]) -> str:
    """Classify a projected tradeoff as accept/watch/regress."""
    score = result.get("tradeoff_score", 0.0)
    fps_ratio = result.get("baseline_fps_ratio")
    byte_ratio = result.get("baseline_byte_ratio")
    psnr_delta = result.get("delta_psnr_all_mean")

    if has_hard_tradeoff_regression(result):
        return TRADEOFF_REGRESS_STATUS

    watched = False
    if finite_number(psnr_delta) and float(psnr_delta) < -TRADEOFF_MINOR_PSNR_LOSS_DB:
        watched = True
    if finite_positive(byte_ratio) and byte_ratio > TRADEOFF_MINOR_BYTE_REGRESSION_RATIO:
        watched = True

    if finite_number(score) and float(score) >= TRADEOFF_ACCEPT_SCORE and (
        not finite_positive(fps_ratio)
        or fps_ratio >= TRADEOFF_MIN_FPS_RATIO_FOR_ACCEPT
    ):
        return TRADEOFF_WATCH_STATUS if watched else TRADEOFF_ACCEPT_STATUS
    if finite_number(score) and float(score) >= 0.0:
        return TRADEOFF_WATCH_STATUS
    return TRADEOFF_REGRESS_STATUS


def has_hard_tradeoff_regression(result: dict[str, Any]) -> bool:
    """Return whether a row crossed a hard byte, quality, or speed guardrail."""
    fps_ratio = result.get("baseline_fps_ratio")
    byte_ratio = result.get("baseline_byte_ratio")
    psnr_delta = result.get("delta_psnr_all_mean")

    if finite_positive(fps_ratio) and fps_ratio < TRADEOFF_HARD_FPS_REGRESSION_RATIO:
        return True
    if finite_positive(byte_ratio) and byte_ratio > TRADEOFF_HARD_BYTE_REGRESSION_RATIO:
        return True
    if finite_number(psnr_delta) and float(psnr_delta) < -TRADEOFF_HARD_PSNR_LOSS_DB:
        return True
    return False


def aggregate_tradeoff_summary(rows: list[dict[str, Any]]) -> dict[str, Any] | None:
    """Summarize comparable rows into one probe-level tradeoff decision.

    The aggregate decision is intentionally stricter than an average score:
    hard byte/quality/speed regressions fail the probe even if another row is
    much faster. Pure timing noise can still be reported as `watch` when the
    average score is non-negative but row-level classifications are mixed.
    """
    scored = [row for row in rows if finite_number(row.get("tradeoff_score"))]
    if not scored:
        return None

    score_sum = sum(float(row["tradeoff_score"]) for row in scored)
    hard_regressions = sum(
        1
        for row in scored
        if bool(row.get("tradeoff_hard_regression"))
        or has_hard_tradeoff_regression(row)
    )
    status_counts = {
        TRADEOFF_ACCEPT_STATUS: 0,
        TRADEOFF_WATCH_STATUS: 0,
        TRADEOFF_REGRESS_STATUS: 0,
    }
    for row in scored:
        status = row.get("tradeoff_status")
        if status in status_counts:
            status_counts[status] += 1

    average_score = score_sum / len(scored)
    if hard_regressions:
        status = TRADEOFF_REGRESS_STATUS
    elif average_score >= TRADEOFF_ACCEPT_SCORE and not status_counts[TRADEOFF_REGRESS_STATUS]:
        status = TRADEOFF_ACCEPT_STATUS
    elif average_score >= 0.0:
        status = TRADEOFF_WATCH_STATUS
    else:
        status = TRADEOFF_REGRESS_STATUS

    return {
        "rows": len(scored),
        "average_score": average_score,
        "accept": status_counts[TRADEOFF_ACCEPT_STATUS],
        "watch": status_counts[TRADEOFF_WATCH_STATUS],
        "regress": status_counts[TRADEOFF_REGRESS_STATUS],
        "hard_regressions": hard_regressions,
        "tradeoff_status": status,
    }


def finite_positive(value: Any) -> bool:
    return finite_number(value) and float(value) > 0.0


def finite_number(value: Any) -> bool:
    return isinstance(value, int | float) and math.isfinite(float(value))
