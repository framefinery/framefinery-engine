#!/usr/bin/env python3
"""Unit tests for encode tradeoff scoring."""

from __future__ import annotations

import math
import unittest

import encode_tradeoff


class EncodeTradeoffTests(unittest.TestCase):
    def test_speed_win_with_neutral_rate_and_quality_accepts(self) -> None:
        result = encode_tradeoff.project_metric_tradeoff(
            baseline_bytes=1000,
            current_bytes=1000,
            baseline_psnr=50.0,
            current_psnr=50.0,
            baseline_fps=10.0,
            current_fps=12.0,
        )

        self.assertIsNotNone(result)
        assert result is not None
        self.assertEqual(result["tradeoff_status"], "accept")
        self.assertGreaterEqual(result["tradeoff_score"], 2.0)

    def test_large_byte_regression_hard_fails_even_with_speedup(self) -> None:
        result = encode_tradeoff.project_metric_tradeoff(
            baseline_bytes=1000,
            current_bytes=1300,
            baseline_psnr=50.0,
            current_psnr=50.4,
            baseline_fps=10.0,
            current_fps=20.0,
        )

        self.assertIsNotNone(result)
        assert result is not None
        self.assertEqual(result["tradeoff_status"], "regress")
        self.assertTrue(result["tradeoff_hard_regression"])

    def test_large_psnr_loss_hard_fails_even_with_speedup(self) -> None:
        result = encode_tradeoff.project_metric_tradeoff(
            baseline_bytes=1000,
            current_bytes=800,
            baseline_psnr=50.0,
            current_psnr=48.8,
            baseline_fps=10.0,
            current_fps=20.0,
        )

        self.assertIsNotNone(result)
        assert result is not None
        self.assertEqual(result["tradeoff_status"], "regress")
        self.assertTrue(result["tradeoff_hard_regression"])

    def test_minor_quality_or_byte_concern_downgrades_to_watch(self) -> None:
        result = encode_tradeoff.project_metric_tradeoff(
            baseline_bytes=1000,
            current_bytes=1060,
            baseline_psnr=50.0,
            current_psnr=49.8,
            baseline_fps=10.0,
            current_fps=20.0,
        )

        self.assertIsNotNone(result)
        assert result is not None
        self.assertEqual(result["tradeoff_status"], "watch")
        self.assertFalse(result["tradeoff_hard_regression"])

    def test_missing_metrics_do_not_create_score(self) -> None:
        result = encode_tradeoff.project_metric_tradeoff(
            baseline_bytes=0,
            current_bytes=1000,
            baseline_psnr=math.inf,
            current_psnr=math.inf,
            baseline_fps=0.0,
            current_fps=10.0,
        )

        self.assertIsNone(result)

    def test_aggregate_accepts_clear_clean_probe_win(self) -> None:
        rows = [
            encode_tradeoff.project_metric_tradeoff(
                baseline_bytes=1000,
                current_bytes=1000,
                baseline_psnr=50.0,
                current_psnr=50.0,
                baseline_fps=10.0,
                current_fps=12.0,
            ),
            encode_tradeoff.project_metric_tradeoff(
                baseline_bytes=2000,
                current_bytes=1900,
                baseline_psnr=49.0,
                current_psnr=49.1,
                baseline_fps=8.0,
                current_fps=10.0,
            ),
        ]

        summary = encode_tradeoff.aggregate_tradeoff_summary(
            [row for row in rows if row is not None]
        )

        self.assertIsNotNone(summary)
        assert summary is not None
        self.assertEqual(summary["tradeoff_status"], "accept")
        self.assertEqual(summary["hard_regressions"], 0)

    def test_aggregate_hard_regression_fails_even_with_positive_average(self) -> None:
        rows = [
            encode_tradeoff.project_metric_tradeoff(
                baseline_bytes=1000,
                current_bytes=1000,
                baseline_psnr=50.0,
                current_psnr=50.0,
                baseline_fps=10.0,
                current_fps=40.0,
            ),
            encode_tradeoff.project_metric_tradeoff(
                baseline_bytes=1000,
                current_bytes=1300,
                baseline_psnr=50.0,
                current_psnr=50.4,
                baseline_fps=10.0,
                current_fps=20.0,
            ),
        ]

        summary = encode_tradeoff.aggregate_tradeoff_summary(
            [row for row in rows if row is not None]
        )

        self.assertIsNotNone(summary)
        assert summary is not None
        self.assertEqual(summary["tradeoff_status"], "regress")
        self.assertEqual(summary["hard_regressions"], 1)


if __name__ == "__main__":
    unittest.main()
