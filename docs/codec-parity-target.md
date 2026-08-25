# Codec parity target

This is the working target for the next encoder effort: substantially improve
FrameFinery AV2 and VVC on the six-vector screen-content set for both lossless
and lossy operation, while preserving exact reference-decoder compatibility
and unified coding paths. Private benchmark reports remain local generated
artifacts and are not documented in the repository.

## Technique audit direction

The encoder audit is focused on techniques that map onto the existing shared
paths: hierarchical and bounded motion search, zero-motion and skip early
exits, partition/mode pruning, palette and screen-content candidate pruning,
transform-size pruning, and rate-distortion mode selection. The local AVM and
VTM sources remain the authorities for syntax and reconstruction.

## Accountability gates

Every retained change must leave a reviewable record of its intent, affected
shared path, measurements, and validation result. The minimum local gates are:

- `cargo fmt --all -- --check`, `cargo check --workspace`, and the relevant
  `cargo test` filters;
- `make clippy-perf`, `make dead-code-audit`, and `make feature-matrix` for
  changes that touch shared or codec code;
- required reference-decoder validation for every changed bitstream path;
- before/after byte, quality, speed, and exact-reconstruction measurements;
- a focused commit only after the above results are recorded in the relevant
  optimization or validation note.

No shortcut should suppress a failing test, hide a dead helper, duplicate a
coding path, or replace a syntax fix with an unchecked index clamp.

## Current blockers and next experiments

- AV2 lossless 1920-wide regular inter is still gated because mixed multi-column
  tile modes can desynchronize AVM entropy decoding. A safe uniform-mode probe
  was reference-clean but byte-neutral on the six vectors, so it was not kept as
  an optimization.
- VVC can panic on the second lossy frame of the vertical A5 vector when the
  partition walker emits a split operation with no legal split alternatives.
  The fix must correct partition availability and syntax emission, not clamp a
  CABAC context index.
- After those correctness blockers, benchmark motion search and mode selection
  changes using a combined byte/PSNR/FPS report, with exact lossless checksums
  and required AVM/VTM decode validation on every retained change.
