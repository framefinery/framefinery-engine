#!/usr/bin/env python3
"""Unit tests for encode-matrix reporting helpers."""

from __future__ import annotations

import contextlib
import io
import unittest

import benchmark_encode_matrix


class BenchmarkEncodeMatrixTests(unittest.TestCase):
    def test_tradeoff_gate_fails_scored_regressions(self) -> None:
        rows = [
            {
                "codec": "vvc",
                "mode": "lossy",
                "tradeoff_score": -1.0,
                "tradeoff_status": "regress",
                "baseline_byte_ratio": 1.25,
                "baseline_fps_ratio": 1.20,
                "delta_psnr_all_mean": 0.0,
            }
        ]

        self.assertEqual(benchmark_encode_matrix.tradeoff_gate_status(rows, False), 0)
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            self.assertEqual(benchmark_encode_matrix.tradeoff_gate_status(rows, True), 1)
        self.assertIn("tradeoff gate failed", stderr.getvalue())

    def test_markdown_report_includes_vvc_lossy_qp(self) -> None:
        markdown = benchmark_encode_matrix.markdown_report(
            {
                "run_name": "probe",
                "set": "smoke",
                "av2_gop": -1,
                "vvc_gop": -1,
                "av2_lossy_qp": 24,
                "vvc_lossy_qp": 19,
                "vvc_fast_search": "lossless-speed",
                "results": [],
            },
            skipped=0,
        )

        self.assertIn("- AV2 lossy QP: `24`", markdown)
        self.assertIn("- VVC lossy QP: `19`", markdown)


if __name__ == "__main__":
    unittest.main()
