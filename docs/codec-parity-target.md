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

The AV2 hotspot checkpoint `goal-av2-hotspot-20260826` measured both modes on
the six-vector set. Lossy tile payload accounted for 68.26% of measured time;
lossless tile payload accounted for 24.83%, while lossless IBC search and
palette construction accounted for 3.29% and 0.86%. The next AV2 probe should
therefore focus on the shared lossy tile mode/transform evaluation, with the
current lossless palette and IBC paths left intact until a representative gain
is demonstrated.

The AV2 DC-only residual probe (2026-08-26) added an exact early return after
regular DCT quantization when every AC level is zero. It retained the same
regular-DCT candidate and strict tie behavior, so the six-vector lossy run kept
all six byte counts and PSNR values unchanged. The instrumented six-vector
profile reduced lossy tile time from 4598 ms to 4437 ms (about 3.5%), and all
three lossy smoke streams matched the AVM reference reconstruction exactly.

The follow-up AV2 predictor-exact residual probe (2026-08-26) bypassed FDCT,
quantization, and IDCT when a 4x4 block's predictor already matches every
source sample. It retains the regular-DCT zero candidate and therefore keeps
the coding path and syntax choice unified. The six-vector lossy run again kept
all byte counts and PSNR values unchanged; the instrumented lossy tile time
fell from 4437 ms to 4221 ms (about 4.9%), and the AVM smoke reference checks
remained exact.

The VVC exact-residual probe (2026-08-26) added the corresponding early exit
to the shared transformed luma and chroma quantizers. A predictor-exact block
now returns the ordinary transformed zero block without DC search, coefficient
generation, or scratch-buffer mutation; lossless transform-skip code and all
other coding paths remain shared. The six-vector lossy A/B run kept all six
byte counts and PSNR values identical. With the same `qp=19`,
`gop=-1`, and `fast-search=lossless-speed` settings, total wall time fell
from 8.553 s to 8.152 s (0.702 to 0.736 aggregate FPS), while measured
`ctu_quantize` time fell from 7343.507 ms to 6978.623 ms. Required VTM smoke
validation passed for all three lossy and all three lossless vectors, and the
focused codec suite passed 350 tests. The generated A/B profiles are retained
under `verification/generated/profiling/hotspots/` for review.

The VVC lossless transform-skip probe (2026-08-26) applied the same exact-zero
early exit to ordinary and BDPCM luma/chroma transform-skip finalization. The
six-vector A/B run kept all 5,092,336 bytes and exact reconstruction results
identical; wall time fell from 5.225 s to 5.177 s (1.148 to 1.159 aggregate
FPS), and measured `ctu_quantize` time fell from 3668.599 ms to 3585.331 ms.
Required VTM validation passed for all three lossless smoke vectors, and the
new direct helper test covers ordinary and BDPCM blocks in both luma and
chroma. This is a small throughput improvement with no mode-selection or
syntax-policy change.
