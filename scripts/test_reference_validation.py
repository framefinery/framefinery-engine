"""A decoder exit status alone must not establish stream conformance."""
from argparse import Namespace
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import run_validation_set
import generate_test_vectors


class ReferenceValidationTests(unittest.TestCase):
    def check_decoder(self, returncode, diagnostic, reference=b"pixels"):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            internal, decoded = root / "internal.yuv", root / "decoded.yuv"
            internal.write_bytes(b"pixels")
            decoded.write_bytes(reference)
            with patch("run_validation_set.subprocess.run", return_value=
                       subprocess.CompletedProcess([], returncode, stdout=diagnostic)):
                return run_validation_set.validate_reference_decode(
                    Namespace(codec="vvc", reference_mode="required"), root / "stream.vvc",
                    internal, decoded, root / "decode.log", Namespace(fmt="yuv420p8"))

    def test_unknown_level_fails_despite_matching_pixels(self):
        self.assertEqual(self.check_decoder(0, "Warning: Level set to none, invalid or unknown value\n")[0], "FAIL")

    def test_unknown_profile_fails_despite_matching_pixels(self):
        self.assertEqual(self.check_decoder(0, "Warning: Profile set to none or unknown value\n")[0], "FAIL")

    def test_missing_required_decoder_cannot_skip(self):
        self.assertEqual(self.check_decoder(2, "no decoder found")[0], "FAIL")

    def test_decoder_failure_cannot_pass_with_matching_pixels(self):
        self.assertEqual(self.check_decoder(1, "decoder failed")[0], "FAIL")

    def test_mismatched_pixels_cannot_pass(self):
        self.assertEqual(self.check_decoder(0, "", b"different")[0], "FAIL")

    def test_success_requires_matching_pixels_and_valid_diagnostics(self):
        self.assertEqual(self.check_decoder(0, "POC 0 I-SLICE\n")[0], "PASS")

    def vector(self, fmt, pattern, frames=1):
        return generate_test_vectors.TestVector(
            name="oracle", width=1, height=1, frames=frames, fmt=fmt, pattern=pattern,
            fps="30", source_path=None, source=None, crop_x=None, crop_y=None,
            lossless=True, codecs=None, filters=())

    def test_filter_depth_scaling_does_not_change_file_fixtures(self):
        vector = self.vector("yuv444p10le", "checker")
        self.assertEqual(run_validation_set.source_filter_pattern_bytes(vector),
                         b"\x42\x03\xc2\x02\x41\x01")  # 834, 706, 321
        self.assertEqual(generate_test_vectors.generate_yuv(vector, {}),
                         b"\x40\x03\xc0\x02\x40\x01")  # Existing zero-padded file fixture.

    def test_filter_rgb_order_and_temporal_phase(self):
        gbr = self.vector("gbrp8", "color_blocks", 2)
        rgb = self.vector("rgb24", "color_blocks", 2)
        self.assertEqual(run_validation_set.source_filter_pattern_bytes(gbr),
                         bytes([128, 128, 32, 96, 176, 80]))
        self.assertEqual(run_validation_set.source_filter_pattern_bytes(rgb),
                         bytes([32, 128, 128, 80, 96, 176]))
        self.assertEqual(generate_test_vectors.generate_yuv(gbr, {}),
                         bytes([32, 128, 128, 80, 96, 176]))


if __name__ == "__main__":
    unittest.main()
