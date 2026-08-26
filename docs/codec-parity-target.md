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
- VVC predictive partition/slice classification and edge geometry are now
  reference-clean for the mixed multi-CTU and 270x480 smoke cases, with exact
  reconstruction. The checkpoint is commit `fed05b4`.
- `make dead-code-audit` currently reaches the inventory but fails on the
  repository's existing collection of intentionally public validation and
  experimental helpers. Those APIs need an explicit audit policy or feature
  ownership cleanup; the gate must not be weakened with blanket suppression.
- After those correctness blockers, benchmark motion search and mode selection
  changes using a combined byte/PSNR/FPS report, with exact lossless checksums
  and required AVM/VTM decode validation on every retained change.

## Optimization probe log

The first post-checkpoint probe (2026-08-26) replaced repeated sorting of the
small luma RD candidate list with a stable in-place insertion pass. It was
rejected: the maintained VVC microbenchmark showed no statistically meaningful
change on most cases, while 10-bit lossy showed a noisy slowdown. The probe was
reverted, leaving candidate ordering and all coding paths unchanged. This is
intentional evidence that a micro-optimization was measured and not retained
without a representative gain.

The follow-up RD-cache probe (2026-08-26) removed redundant sorting from the
shared luma and chroma winner caches. The caches now scan only for the current
worst entry, preserving the candidate membership contract while avoiding
reordering work that no later stage observes. The six-vector lossy checkpoint
kept all 6 byte counts and PSNR values identical; the maintained microbenchmark
showed a 3--5% gain on the 10-bit lossy case, with the other cases within
measurement noise. Lossless cache behavior is unchanged because its cache limit
is zero. The mixed predictive 128x64 stream passed VTM decode with an exact
reconstruction match.
