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

Maintainability is also a release criterion for this goal. In addition to the
runtime measurements, retained changes must be accountable through:

- `make api-docs-strict` and focused rustdoc updates for every changed public
  Rust or WASM API;
- `make dead-code-audit` findings assigned to an intentional public/feature
  boundary or removed, never silenced with a blanket lint allowance;
- `make dependency-audit` and the existing Clippy/feature-matrix checks for
  dependency and configuration drift;
- a short note in this document for each rejected experiment, including its
  hypothesis, measured result, and reason for rejection;
- an explicit review that lossy, lossless, RGB, and subsampled inputs still
  use the same coding path, with feature gates applied at the deepest legal
  mode-selection point.

Profiling is evidence for where to investigate, not permission to trade away
clarity. Any optimization whose benefit cannot be reproduced by the recorded
benchmark or whose control flow makes the shared path harder to audit should
be rejected or deferred.

## Optimization accountability protocol

The means of achieving this target are part of the target. Every optimization
experiment follows the same small record, kept in this document or in a linked
generated report under `verification/generated/`:

1. State the suspected bottleneck or regression and the expected byte, PSNR,
   and FPS trade-off before changing the encoder.
2. Capture a reproducible baseline with the same vectors, frame count, codec
   settings, build profile, and reference-validation mode. Keep generated
   outputs outside version control unless they are intentional fixtures.
3. Use the narrowest useful accountability tools: formatter, compiler and
   tests, Clippy, feature-matrix checks, dead-code and dependency audits,
   rustdoc/API checks, and a profiler or stage-level timing report. A tool that
   is unavailable or has a known repository failure must be recorded as such,
   never presented as a passing gate.
4. Review the diff for duplicated lossy/lossless/RGB paths, accidental profile
   or geometry restrictions, unchecked indexing, stale comments, and public
   API documentation. Keep mode and profile gates at the deepest legal
   selection point while preserving one shared reconstruction and syntax path.
5. Require internal reconstruction checks and the relevant reference decoder
   before accepting any bitstream-affecting change. For lossy work, report
   bytes, PSNR, and FPS deltas together; a single improved metric is not
   sufficient evidence of success.
6. Record the disposition: retained in a focused commit, rejected and reverted
   with measurements, or deferred with a concrete follow-up. This prevents
   expedient local fixes from becoming undocumented regressions that must be
   rediscovered later.

The release-facing checklist is therefore `make release-check`, the relevant
strict validation-set commands with `VALIDATION_REFERENCE_MODE=required`,
`make api-docs-strict`, `make clippy-perf`, `make feature-matrix`,
`make dead-code-audit`, `make dependency-audit`, and `git diff --check`, plus a
recorded representative profile. These checks are accountability tools, not
reasons to weaken the implementation or to add broad lint suppressions.

The accountability rerun on 2026-08-27 passed `make api-docs-strict`,
`make clippy-perf`, and `git diff --check`. `make dependency-audit` failed at
the explicit setup check because `cargo-audit` is not installed; no dependency
audit result is claimed until that tool is available. The existing strict
dead-code inventory remains a known follow-up recorded below.

## Current blockers and next experiments

- AV2 lossless 1920-wide regular inter is still gated because mixed multi-column
  tile modes can desynchronize AVM entropy decoding. A safe uniform-mode probe
  was reference-clean but byte-neutral on the six vectors, so it was not kept as
  an optimization.
- VVC predictive partition/slice classification and edge geometry remain
  reference-clean for the previously covered mixed multi-CTU and 270x480
  smoke cases, with exact reconstruction. The multi-frame GOP-30 P-slice
  failure described in the probe log was resolved in the current working
  change; the unusual-geometry and multi-CTU reference sets now pass, while
  the complete predictive validation matrix is still pending.
- `make dead-code-audit` currently reaches the inventory but fails on the
  repository's existing collection of intentionally public validation and
  experimental helpers. Those APIs need an explicit audit policy or feature
  ownership cleanup; the gate must not be weakened with blanket suppression.
- After those correctness blockers, benchmark motion search and mode selection
  changes using a combined byte/PSNR/FPS report, with exact lossless checksums
  and required AVM/VTM decode validation on every retained change.

The first implementation audit for this goal (2026-08-26) found a more urgent
VVC screen-content gap than a micro-optimization: the production encoder
collects SCC opportunity counters, but the production quantizer still fills
every `luma_tu_scc_decisions` entry with `RegularIntra`. The IBC/palette CU
emitter is currently a test/scaffold path rather than a production candidate
inside the shared CTU traversal. This is now a tracked integration item. Any
fix must carry the decision through the existing quantize/reconstruct/CABAC
path at block level, preserve the profile gates, and pass required VTM decode
validation before its rate benefit is accepted.

The first SCC integration probe (2026-08-27) was rejected. It wired exact
8x8 IBC decisions into the shared quantize/reconstruct/CABAC data flow and
selected 52,024 blocks on the Wayland frame, reducing the first-frame output
from 352,005 to 175,752 bytes. However, the existing residual RGB partition
contract did not signal the required IBC/single-tree state: the first version
produced VTM's invalid-block-vector error, and the follow-up single-tree
version failed the encoder's own lossless reconstruction check. All source
changes were reverted. The measured result is retained as evidence, and a
future SCC integration must first unify the RGB tree/reconstruction contract
with the syntax state before enabling the rate-saving mode.

A follow-up SCC probe (2026-08-27) corrected the copy direction for overlapping
2-D IBC regions, using reverse row order when the destination is below the
reference. This did not address the failure: the same Wayland lossless RGB
case still differed from the source reconstruction, despite compiling and
selecting the SCC candidates. The copy-order change and all associated source
changes were reverted. The next implementation should compare the complete
single-tree RGB quantization and reconstruction contracts against the
existing dual-tree path before attempting another mode-selection shortcut.

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

The VVC luma RD shortlist probe (2026-08-26) increased the `lossless-speed`
4:2:0 luma shortlist from two to three candidates. On the four-vector,
20-frame lossy probe it produced identical bytes and PSNR for every vector,
while measured FPS decreased from 8.14/10.27/5.74/4.80 to
8.02/10.11/5.50/4.70. The additional candidate therefore showed no coding
benefit and a speed cost; the source change was reverted. The 4:4:4 one-winner
screen-content policy was not changed. This rejected probe preserves the
accountability record without adding an unproven policy branch.

The AV2 lossy chroma-context hoist probe (2026-08-26) moved the immutable
predictor context construction outside the per-mode scoring loops. It preserved
all four byte counts and PSNR values in the 20-frame probe, but measured FPS
decreased from 8.79/14.15/14.48/18.05 to 8.27/13.48/14.12/18.01. The source
change was reverted because it did not demonstrate a stable speed gain; the
existing loop structure remains explicit for auditability.

The AV2 lossy full-search sampling probe (2026-08-26) raised the full-search
threshold from 64 to 256 transform blocks. It produced identical bytes and
PSNR on all four 20-frame probe vectors, while measured FPS changed from
8.79/14.15/14.48/18.05 to 8.57/13.54/13.94/17.89. The source change was
reverted: the representative leaves did not benefit from the extra search and
the broader threshold only added measured work.

The AV2 lossy directional-delta refinement (2026-08-26) retained a bounded
quality search around the best screened luma direction for subsampled input.
It evaluates only the existing syntax-supported ±3 delta angles, uses the
same predictor and residual path as the selected base modes, and stores the
two temporary scores on the stack. On the repeated 20-frame probe,
`probe_gradient_420` improved from 903,793 bytes / 44.226 dB to 772,819 bytes
/ 45.058 dB; the other three vectors were byte- and PSNR-identical. Across
the four-vector aggregate this was 2,041,252 vs 2,172,226 bytes (-6.0%),
56.162 vs 55.954 dB (+0.208 dB), and approximately 11.70 vs 12.95 FPS
(-9.6%). The tradeoff is retained as a compression/quality improvement, with
the speed cost explicitly recorded for a future adaptive-search pass. The
required AVM smoke (3/3) and broader regression (7/7) reference validations
matched internal reconstruction exactly. The full six-vector release profile
could not be run in this workspace because its mandatory `AOMCTC_ROOT` source
directory is unavailable.

The AV2 lossy directional-admission probe (2026-08-26) widened the shared
4:2:0 luma directional score margin from 256 to 512 units per transform block.
It selected the same modes, producing identical bytes and PSNR on all four
20-frame vectors; measured FPS changed from 8.79/14.15/14.48/18.05 to
8.50/13.52/14.28/17.85. The wider margin was reverted because it exercised no
additional coding decision and did not improve speed or output quality.

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

The AV2 fused reconstruction-metrics cleanup (2026-08-26) removed a second
16-sample temporary and combined reconstructed SSE and variance-loss
calculation in one shared helper for transform, regular-DCT, and spatial
lossy candidates. The four-vector 20-frame output remained exactly
2,041,252 bytes with mean PSNR 56.162 dB; a longer 50-frame run remained
byte- and PSNR-stable at 5,104,423 bytes and 11.95 aggregate FPS. The
observed 20-frame speed change was neutral (11.71 versus 11.70 FPS), so no
speed gain is claimed, but the cleanup reduces repeated memory traffic without
changing clipping, reconstruction arithmetic, candidate identity, or syntax.
Required AVM validation passed for all three lossy and all three lossless
smoke vectors, and the full codec suite remained at 356 tests.

The VVC shared residual-SSE cleanup (2026-08-26) moved the common
reconstructed-residual SSE loop into the residual-sample module and reused it
from transformed luma and chroma scoring. Component-specific transform and
transform-skip decisions remain unchanged, avoiding duplicated correctness
logic without introducing a new coding path. The four-vector, 20-frame lossy
probe remained exactly 4,275,283 bytes with identical PSNR; the observed
aggregate speed was 6.67 FPS versus the 6.47 FPS baseline, recorded as a
promising but not isolated speed claim. Required VTM validation passed for all
three lossy and all three lossless smoke vectors, and the full codec suite
passed all 356 tests.

The VVC proxy-admission probe (2026-08-26) added a bounded lossy
`lossless-speed` gate for luma RD refinement: candidates whose cheap mode
proxy exceeded twice the best shortlist score were not fully rescored. The
best proxy candidate is always retained, and lossless and other speed policies
remain unchanged. On the four-vector, 20-frame probe, output stayed exactly
4,275,283 bytes with identical PSNR; luma RD candidate scoring calls fell from
144,000 to 89,127 (−38.1%) and measured luma RD-scoring time fell from 869.6
ms to 832.0 ms (−4.3%). Aggregate FPS moved from 6.47 to 6.53, recorded as
corroborating rather than standalone evidence. Required VTM validation passed
for all three lossy and all three lossless smoke vectors, and all 356 codec
tests passed.

The follow-up VVC chroma proxy-admission probes (2026-08-26) tested the same
idea against the three-entry lossy `lossless-speed` chroma shortlist. The 2×
threshold cut cached chroma scoring calls by about 40% and scoring time by
9.8%, but increased the four-vector output by about 0.30% with no meaningful
PSNR gain. A 4× threshold reduced the rate penalty to about 0.21%, but still
changed the compression frontier and did not provide isolated end-to-end
evidence beyond timing noise. Both variants were reverted; chroma candidate
selection remains complete until a content-adaptive gate can protect rate as
well as speed.

The format-aware VVC chroma admission probe (2026-08-26) retained that gate
only for lossy `lossless-speed` 4:2:0. The 4:2:2 and 4:4:4 paths remain fully
searched after the preceding rate regression. On the four-vector, 20-frame
probe, the 4:2:0 rows improved or preserved rate by 2,229 bytes, while both
4:4:4 rows remained byte-identical; PSNR was unchanged. Chroma cached scoring
calls fell from 607,770 to 389,465 (−36.0%), measured chroma scoring time fell
from 1,851.5 ms to 1,677.2 ms (−9.4%), and aggregate FPS moved from 6.47 to
6.71. Required VTM validation passed for all three lossy and all three
lossless smoke vectors. This is retained as a format-aware policy, not a
general chroma shortcut.

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


The AV2 large-transform exact-residual probe (2026-08-26) extended the same
shared zero-residual shortcut to the regular 8x8 and 4x8 chroma transforms.
All six lossy byte counts and PSNR values remained identical. With the same
`qp=24` and predictive settings, total wall time fell from 4.472 s to 4.292 s
(1.342 to 1.398 aggregate FPS), and measured `lossy_tile_payload` time fell
from 4271.647 ms to 4093.910 ms. Required AVM smoke validation passed for all
three lossy vectors. The shortcut changes no syntax or mode decision and
keeps the existing 4x4/8x8 coding paths shared.

The AV2 DPCM exact-residual probe (2026-08-26) was rejected. It bypassed
quantized, refined, and transformed DPCM candidate construction when the DPCM
predictor was exact, while preserving the existing zero-coefficient syntax.
The six-vector A/B outputs were identical, but wall time changed only from
4.276 s to 4.263 s and `lossy_tile_payload` from 4081.453 ms to 4069.022 ms,
which is within run-to-run noise. The implementation was reverted and no
performance claim is attached to it.

The VVC residual-selector zero-block probe (2026-08-26) was also rejected.
Although it preserved all six lossy byte counts and PSNR values, moving the
zero-residual test above the selector's transform-skip and MTS decisions made
the controlled run slightly slower: 8.074 s versus the 8.015 s baseline. The
lower-level quantizer shortcuts already remove the expensive work, so the
additional selector scan was reverted.

The VVC lossless transform-skip probe (2026-08-26) applied the same exact-zero
early exit to ordinary and BDPCM luma/chroma transform-skip finalization. The
six-vector A/B run kept all 5,092,336 bytes and exact reconstruction results
identical; wall time fell from 5.225 s to 5.177 s (1.148 to 1.159 aggregate
FPS), and measured `ctu_quantize` time fell from 3668.599 ms to 3585.331 ms.
Required VTM validation passed for all three lossless smoke vectors, and the
new direct helper test covers ordinary and BDPCM blocks in both luma and
chroma. This is a small throughput improvement with no mode-selection or
syntax-policy change.

The lossy VVC audit checkpoint `goal-audit-20260826` profiled one frame of the
six-vector set with `qp=19`, predictive GOP settings, and lossless-speed search.
The run measured 2,225,429 bytes, 8.259 seconds, 0.726 aggregate FPS, and mean
PSNR 54.130. The largest codec-local module was the shared chroma mode path at
22.35% inclusive time, followed by chroma residual scoring at 13.39% and luma
mode selection at 8.64%. The largest stage counters were chroma mode search
(1,778.748 ms), chroma RD scoring (1,255.336 ms), and chroma BDPCM selection
(832.439 ms). No mode was disabled and no speculative shortcut was retained
from this audit. The next experiment must first establish a behavior-preserving
BDPCM candidate refactor, then measure any optimization against this checkpoint.

Accountability for the parity goal includes `cargo fmt`, workspace checks and
tests, `clippy-perf`, the feature matrix, `git diff --check`, and the existing
dead-code audit. The dead-code audit is intentionally still allowed to report
the repository's known public/experimental inventory failures; those findings
must be resolved by ownership or feature-policy changes rather than hidden by
blanket lint suppression. Generated profiles remain under
`verification/generated/profiling/hotspots/` and are not release inputs unless
their evidence is summarized here.

The VVC BDPCM candidate refactor checkpoint `goal-bdpcm-refactor-20260826`
removed duplicated chroma prediction, residual construction, transform-skip
finalization, and scoring code shared by the direct and regular BDPCM loops.
The policy loops remain separate, including the direct residual-safety gate and
the regular best-candidate update, so no legal mode or syntax decision was
removed. The six-vector lossy output remained 2,225,429 bytes with mean PSNR
54.130; the observed 8.172-second run versus the 8.259-second audit run is
within normal measurement variation and is not claimed as a speed gain.
Required VTM validation passed for all three lossy and all three lossless smoke
vectors with matching reconstruction checksums. This establishes the shared
helper as the maintainable base for a later measured BDPCM optimization.

The follow-up VVC direct-BDPCM safety probe `goal-bdpcm-safety-20260826`
performed the existing raw-SSE safety test immediately after candidate
prediction and residual construction, before transform-skip finalization and
RD scoring. This is safe because the old selection condition required that
same safety predicate as well as the scored candidate comparison. On the
screen vector, 6,062 of 217,665 direct candidates were therefore rejected
without quantized scoring. The six-vector lossy result remained 2,225,429
bytes with mean PSNR 54.130; the observed total fell from 8.172 s to 8.092 s
(0.734 to 0.741 aggregate FPS). Required VTM validation passed in both lossy
and lossless smoke modes, and the focused codec suite remained at 351 tests.

The AV2 transform-zero probe `goal-av2-transform-20260826` was rejected after
measurement. It added an exact-residual early return to the shared 4x4
transform candidate, preserving the transform candidate kind and zero syntax.
The six-vector lossy output stayed at 2,701,924 bytes with mean PSNR 51.146,
but the controlled run measured 4.266 s versus the 4.243 s fresh audit run.
Because the result was within run-to-run variation and slightly slower, the
change was reverted. This keeps the AV2 hot path free of shortcuts that are not
supported by representative evidence.

The follow-up AV2 detailed audit `goal-av2-detailed-audit-runtime-20260826`
enabled the existing runtime statistics on the six-vector lossy set. In the
1920x1080 samples, every luma and chroma transform block used the regular-DCT
candidate family; selected spatial, refined-spatial, transform, and FSC
candidates were zero in the recorded regions. The selected luma modes were
content-dependent DC, horizontal, vertical, directional, Paeth, and smooth
intra modes, while chroma selected DC, horizontal, vertical, and Paeth. The
audit therefore does not justify removing legal residual candidates or
replacing mode search with a separate fast path. It does identify regular-DCT
candidate construction and scoring as the next AV2-specific investigation
area, with any pruning required to preserve the shared candidate path and
reference-decoder validation. The run reproduced 2,701,924 bytes and mean
PSNR 51.146; its instrumented runtime is not a speed baseline because the
per-region statistics deliberately add logging overhead.

The accountability tool inventory was also checked for this goal. The
repository-provided gates are `cargo fmt`, workspace check/test, `make
clippy-perf`, `make feature-matrix`, `make dead-code-audit`,
`make dependency-audit`, `git diff --check`, and required AVM/VTM decode
validation. `make dependency-audit` is an explicit gate backed by
`cargo-audit`; it fails clearly when that tool is not installed rather than
silently skipping dependency review. No substitute lint or blanket suppression
is being claimed as equivalent. `perf` is present but cannot access hardware
performance counters under the current kernel policy, so the retained timing
evidence uses the repository's reproducible hotspot profiler instead.

On 2026-08-26, the accountability rerun passed `make clippy-perf` and
`make feature-matrix` after the AV2 TX_4X8 investigation. The strict
`make dead-code-audit` gate still reports the repository's known intentional
public/experimental inventory (183 errors under `-F dead_code`); it remains a
visibility audit rather than a release pass, and no blanket allowance was
added. The failing 4:2:2 reference-reconstruction case is therefore tracked
as an unresolved codec defect, not accepted as an optimization result.

The AV2 DC-only inverse-transform probe (2026-08-26) added a specialized
integer reconstruction for the already-selected regular-DCT DC-only candidate.
An exhaustive sampled test across 8-, 10-, and 12-bit ranges matched the full
inverse transform, including clipping and rounding. The six-vector output
remained 2,701,924 bytes with mean PSNR 51.146, but the controlled profile was
4.274 s versus the 4.243 s baseline. The production shortcut and its test were
reverted because the representative workload did not demonstrate a gain.

The AV2 source-variance temporary-elision probe (2026-08-26) computed the
existing variance formula directly from the 8/10/12-bit source samples instead
of first copying them into an integer array. It preserved the exact arithmetic
and all six output byte counts and PSNR values, but measured 4.301 s versus the
4.243 s baseline. The change was reverted; the next experiment should address
larger reuse across repeated mode analyses rather than this small temporary.

The AV2 tail-candidate score-gate probe (2026-08-26) was rejected. It moved
the existing extra-SSE bound ahead of coefficient-rate scoring for tail-pruned
regular-DCT candidates, without changing candidate order or tie behavior. The
six-vector output remained 2,701,924 bytes with mean PSNR 51.146, but the
controlled profile measured 4.319 s versus the 4.243 s baseline. The probe was
reverted; the small tail-candidate population does not justify retaining the
additional branch in the hot path.

The VVC direct chroma-BDPCM best-of-two probe (2026-08-26) evaluated every
safe horizontal/vertical fast-search candidate before returning instead of
stopping at the first candidate that beat the baseline. The six-vector lossy
run remained byte-identical at 2,225,429 bytes with mean PSNR 54.130 and
measured 8.015 s, with no change in selected-candidate counts. It was reverted
because the broader behavior had no demonstrated benefit on the representative
set.

The hotspot profiler now accepts `HOTSPOT_FRAMES`, `HOTSPOT_AV2_GOP`, and
`HOTSPOT_VVC_GOP`. Its quick-probe defaults remain one frame and infinite GOP,
while a predictive parity profile can explicitly use, for example,
`HOTSPOT_FRAMES=50 HOTSPOT_AV2_GOP=-1 HOTSPOT_VVC_GOP=-1`. This closes the
previous accountability gap where hotspot timing could not be requested for a
full predictive stream; generated profiles must record the selected frame and
GOP settings before being used as release evidence.

The AV2 intra-score reconstruction reuse probe (2026-08-26) consolidated
per-pixel clamping and Paeth predictor work across the two passes of the shared
intra scorer. It preserved all six byte counts and PSNR values, but increased
the controlled six-vector runtime from 4.243 s to 4.455 s. The temporary arrays
were removed; this confirms that larger-looking reuse is not automatically a
win when it adds storage traffic to 4x4 scoring.

The AV2 predictive zero-motion residual probe (2026-08-26) replaced the
lossless zero-motion tile payload with the shared lossy inter residual writer.
On the 50-frame predictive six-vector run it improved mean PSNR from 53.573 to
56.367 dB, but increased bytes by 6.88% and reduced aggregate FPS by 19.2%.
More importantly, required AVM validation exposed an existing 4:2:2 key-frame
reconstruction mismatch, so the probe was reverted rather than accepted on
incomplete compliance evidence. A controlled one-frame 4:2:2 run fails before
any predictive zero-motion tile is reached; the next AV2 work item is to
reconcile the TX_4X8 key-path arithmetic and syntax with AVM before re-running
this quality experiment. The temporary transform probe and generated outputs
were removed from the working tree.

The follow-up TX_4X8 audit narrowed the failure to the V plane of the second
vertical band: the 4:2:2 8-bit Y and U planes match AVM, while V differs only in
the affected 4x8 blocks. Standalone TX_4X8 blocks and the lossless 4:2:2 path
remain reference-clean. A rectangular left-context correction and a
high-bit-depth reciprocal DC-divider experiment were both run against the
focused strip sweep and rejected because neither restored the AVM
reconstruction. The source tree was restored after each probe; no speculative
workaround is part of the encoder. The focused sweep and the full workspace
tests must pass before this defect can be considered resolved.

A follow-up isolation probe then zeroed only the first V-plane TX_4X8 candidate
in the same 64x16 strip. The resulting stream decoded exactly through AVM,
including the unchanged second V band. This confirms that the failure is
triggered by the first V block's entropy coding or state evolution; it is not a
general predictor, geometry, or inverse-transform failure. The probe was
removed after the result was recorded, and the next fix must preserve the
shared coefficient writer while reconciling that state with AVM.

Disabling frame-level CDF adaptation for the focused stream did not remove the
mismatch, so the defect is not caused solely by adaptive CDF updates. That
experiment was reverted as well; the remaining audit target is the exact
TX_4X8 V-plane symbol/context mapping and its interaction with the first block
of the following vertical band.

The latest controlled probe retained all first-block V coefficients except its
DC high-range extension and then decoded exactly through AVM. This isolates the
next comparison to the V TX4X8 DC high-range condition/codeword and its state
transition; the shared Exp-Golomb helper itself was compared with AVM and left
unchanged after a candidate alteration produced a corrupt stream.
Simple -1/0/+1 DC high-range value-bias probes also failed to restore the
reference reconstruction, so the problem is not an obvious level offset.

The AVM accounting inspector was rebuilt on 2026-08-26 with accounting and
inspection enabled, while disabling the optional ML partition dependencies.
It still segfaults before producing output for both the native AV2 OBU and a
synthetic AV02 IVF wrapper around the same one-frame stream. This reproduces
the existing inspector limitation and is not evidence that the decoder accepts
or rejects the stream; ordinary AVM decoding remains the authority for this
investigation. The accounting attempt and its generated files stay in the
ignored agent scratch area rather than becoming a validation dependency.

The subsequent AV2 TX4X8 differential probe compared all eight V-plane blocks
from the focused 64x16 4:2:2 stream. The signed coefficient levels emitted by
the Rust writer and read by AVM matched block-for-block, and the decoder's
dequantized coefficients also matched the Rust candidate values. A width sweep
from 64 through 704 pixels reproduced the first mismatch at the rightmost
4x8 block of the second vertical band. This supersedes the earlier entropy-only
hypothesis: the remaining audit must compare block traversal, edge predictor
availability, and reconstructed samples at that boundary. The diagnostic
instrumentation was removed from both production and reference trees after
each probe; no workaround is retained without a focused reference test.

The AV2 DC predictor correction (`df61e02`, 2026-08-26) mirrors AVM's
reciprocal-divisor entries for small edge counts and routes both reconstruction
and mode-scoring DC averages through that helper. The focused 4:2:2 width
sweep (64, 128, 256, 384, 512, 640, and 704 pixels) now matches AVM exactly;
the one-frame required-reference release six-vector checks pass for all AV2
and VVC rows, and the full workspace test suite passes. The helper has a
direct regression test for the non-power-of-two 12-sample case that exposed
the original drift.

The local AOM CTC profiling manifests were also corrected to describe derived
4:2:2 and 4:4:4 rows as `source_y4m_convert` entries rooted at
`${AOMCTC_ROOT}`. This keeps cleanup safe: profiling can regenerate the
derived clips after removing them, instead of depending on stale generated
paths from a previous run.

The VVC residual-buffer write probe (2026-08-26) replaced the shared residual
extraction loops' `push` operations with indexed writes after one allocation.
The change preserved the scalar arithmetic and edge clamping, and required
reference validation passed, but the six-vector instrumented profile moved
from the recorded 1.141 to 1.111 aggregate lossless FPS and from 0.723 to
0.707 lossy FPS. Because the result was not a demonstrated improvement, the
probe was reverted. This is retained as an accountability record so a future
optimization does not circle back to the same unproductive change.

The follow-up VVC MTS DC-basis hoist probe (2026-08-26) moved the invariant
horizontal `k=0` transform lookup out of the sample loop. It preserved output
bytes, PSNR, and all MTS tests, but the six-vector instrumented profile moved
from 1.157 to 1.142 aggregate lossless FPS and from 0.726 to 0.722 lossy FPS.
The change was reverted because it did not produce a measurable gain on the
representative workload.

The VVC luma quantization audit then found an unconditional duplicate: the
legacy AC transform was computed even when fast mode, a nonzero MTS index, a
non-8x8 TU, or an AC-free block made that candidate ineligible. The legacy
candidate is now materialized only in the single shared branch that compares
it with the alternate transform. The six-vector profile kept identical bytes
and PSNR and improved aggregate FPS from 1.111 to 1.157 for lossless (+4.1%)
and from 0.707 to 0.726 for lossy (+2.7%) against the immediately preceding
instrumented run. Required VTM validation passed for all six vectors, and the
focused transform suite passed all 23 tests. The change is limited to
avoiding discarded work; no syntax, mode, profile, or reconstruction path was
forked.

The AV2 zero-quantized regular-DCT probe (2026-08-26) attempted to skip
dequantization and IDCT after the quantizer produced an all-zero coefficient
set. The repeated six-vector profiles produced 2,702,877 bytes and mean PSNR
51.146. The first run measured
1.421 aggregate lossy FPS and the repeat 1.402, which is not a reliable gain.
The candidate and its helper refactor were reverted. A restored-baseline
profile on the unchanged tree reproduced 2,702,877 bytes exactly, showing
that the earlier 2,701,924-byte comparison came from regenerated-vector state
rather than this code change. The probe remains rejected because it did not
produce a reliable speed gain; future zero-candidate work must compare
candidate state with a same-input baseline.

The follow-up AV2 zero-dequantization probe (2026-08-26) skipped only the
dequantization loop when every regular-DCT coefficient was already zero, while
retaining the existing IDCT, reconstruction, candidate identity, and syntax
selection. Both profiles reproduced the restored baseline exactly at
5,922,076 lossless bytes and 2,702,877 lossy bytes with mean lossy PSNR 51.146.
Runtime was within measurement noise but slightly slower: lossless measured
2.825 and 2.813 aggregate FPS versus the baseline's 2.892, and lossy measured
1.399 and 1.400 versus 1.414. The code was reverted and the probe is rejected;
the audit requirement is to retain equivalent output and demonstrate a stable
runtime gain before accepting this class of kernel shortcut.

The VVC motion-search duplicate-candidate probe (2026-08-26) skipped a second
SAD evaluation when a spatial predictor repeated the search's initial zero
vector. A five-frame predictive 4:2:0 probe preserved 771,733 bytes and mean
PSNR 51.839 exactly, but the unchanged baseline measured 3.318 s / 1.507 FPS
and the probe measured 3.321 s / 1.505 FPS in two runs. The guard was reverted:
its correctness is straightforward, but it adds no demonstrated throughput
benefit on the motion workload, so it is not retained as code-quality noise.

The VVC near-motion admission probe (2026-08-26) allowed small nonzero-SAD
motion candidates from the existing luma motion map to reach the shared
explicit-inter/RD path. On the five-frame predictive 4:2:0 probe it changed
771,733 bytes to 771,725 with unchanged 51.839 dB PSNR, but required VTM
decoding failed at the final CABAC terminating-bit check. The experiment was
reverted; no output-changing motion optimization is acceptable without
reference-decoder agreement.

The same required-reference check then failed for the restored exact-only
baseline: a one-frame GOP-30 stream decoded, while the otherwise identical
two-frame and five-frame streams failed when the first P slice was decoded.
Temporarily disabling explicit-inter candidate production did not remove the
failure, so the defect is broader than the rejected near-motion candidate. It
currently isolates to the multi-frame predictive VVC syntax/CABAC path and is
a release-blocking compliance issue. The next investigation must compare the
P-slice mode, root-CBF, residual, and terminating-bit sequence against VTM and
add a multi-frame required-reference regression before further motion-search
optimization.

A follow-up root-CBF context probe tested that hypothesis against the same
 mixed two-frame fixture. The reference `QtRootCbf` initialization is 12 for
 I-slices and 5 for P-slices, whereas the existing `CuCodedFlag` scaffold uses
 a different I-slice row. Introducing a separate root-CBF model and routing
 the explicit-inter/IBC no-residual decisions through it produced byte-for-
 byte identical output and did not change the VTM failure. The probe was
 reverted: the remaining divergence is not explained by that context alone,
 and no speculative CABAC context alias is retained.

A controlled lossy P-slice probe then disabled every explicit-inter candidate
while leaving the shared P-slice partition and residual writer active. The
resulting stream grew from 255 to 272 bytes and still failed VTM, so explicit
motion signaling is not sufficient to explain the defect. The production
guard was removed. The remaining audit scope is the shared lossy P-slice
intra residual/partition syntax and its CABAC context evolution; a future
optimization must not be accepted until this path decodes over multiple
frames.

To remove slice-type ambiguity, a forced-P control kept the second picture as
P while suppressing explicit-inter selection. VTM still rejected the stream,
and the P picture grew from 91 to 108 coded bytes only because the same
lossy residual path remained active; this confirms that a valid P-slice cannot
yet be claimed merely from payload-derived slice classification. The force
and suppression hooks were removed immediately after the probe. The required
regression must assert decoding of a genuinely multi-frame lossy P-slice,
not only a one-frame or all-skip stream.

The accountability baseline on 2026-08-26 passed `make clippy-perf`, every
`make feature-matrix` product configuration, and the focused nine-test
predictive VVC suite. A fresh trace-enabled VTM decoder was configured and
built under `verification/generated/agent_scratch`; it reproduced the
multi-frame P-slice failure and produced a CABAC trace for the next comparison.
The strict `make dead-code-audit` gate still fails on the known inventory of
intentional experimental helpers and instrumentation, so that result remains
visible as maintenance debt rather than being hidden with blanket lint
allowances. Temporary reference builds and traces remain outside version
control.

The VVC predictive compliance repair (2026-08-26) corrected three related
syntax issues in the shared CABAC path: inter 32/64-sample leaves now retain
their explicit no-split signaling, explicit inter residuals emit the required
`rqt_root_cbf` with its own initialized context, and inter transform-depth-zero
units infer luma CBF when both chroma CBFs are zero. The focused two-frame
128x64 fixture now decodes through the reference decoder, and its reference
reconstruction SHA-256 matches the internal reconstruction exactly
(`06e5228d75f9b7aa336e27407b789105b0ec271356d46af51beb7b2c8324483c`). The
VVC smoke set passed all three required-reference cases, `make clippy-perf`
passed, and the focused predictive suite passed all nine tests. This repair is
kept as a correctness checkpoint; it is not yet a claim that the complete
predictive validation matrix is release-clean.

The follow-up reference sweep passed `unusual-geometry-smoke` (7/7) and
`multictu-regression` (4/4) for VVC with `VALIDATION_REFERENCE_MODE=required`.
These cases cover odd visible dimensions, 4:2:0/4:2:2/4:4:4, RGB, multiple
CTUs, multiple frames, and 10-bit input. Every case matched the encoder's
internal reconstruction, and the lossless multi-CTU cases also matched the
source exactly. The full release matrix and performance comparison remain
separate gates.

The paired chroma-residual loop probe (2026-08-26) consolidated Cb and Cr
residual construction into one helper while preserving residual values in a
direct equivalence test. Required-reference unusual-geometry (7/7) and
multi-CTU (4/4) validation remained clean, but two one-frame profiling runs
were inconclusive: lossless aggregate throughput was effectively unchanged
and lossy throughput varied more between runs than the observed change. The
probe was reverted. Future kernel changes must show a stable improvement over
repeated runs before being retained; correctness-only refactors should be
accepted separately from performance claims.

The chroma transform-skip zero-scan probe (2026-08-26) removed a full residual
block all-zero scan because the coefficient extraction pass already produces
the same zero block. The residual test suite passed, but Criterion showed
mode- and format-dependent results: some 4:2:0/8-bit and 4:4:4/10-bit cases
improved while other 4:4:4/10-bit and lossy cases regressed. The probe was
reverted. This remains a possible targeted optimization only if a future
policy can demonstrate a stable benefit for the complete supported matrix.

The AV2 palette first-occurrence probe (2026-08-26) replaced the per-sample
palette `contains` scan with a histogram count check, preserving insertion
order and palette contents in the unit suite. The 4:4:4 palette microbenchmark
instead regressed by about 3% at 64x64 and showed no improvement at 128x128,
so the probe was reverted. The existing linear scan remains until a measured
replacement improves both palette sizes without changing selection behavior.

The AV2 palette capacity short-circuit probe (2026-08-26) reordered the
existing condition so full palettes would skip membership scans. It preserved
the palette output, but the 64x64 microbenchmark improvement stayed within
noise and the 128x128 case showed no gain. The probe was reverted; a retained
palette optimization must improve the complete size and content mix.

The predictive-tree audit (2026-08-26) found that the encoder could quantize
an inter-slice CTU with the single-tree partition while constructing its CABAC
parameters as dual-tree. The resulting shared traversal consumed different
numbers of chroma transform leaves and could panic on longer 4:4:4/RGB
streams. Parameter construction now derives the tree choice from the same
inter-slice contract used by quantization. A 50-frame 2560x1440 RGB stream
no longer panics; the maintained required-reference smoke and unusual-geometry
sets remain clean. A separate later-frame VTM coefficient-range failure in
that desktop stream remains under investigation and is not hidden by this fix.

The follow-up P-slice state audit found a second boundary: reusing prior
chroma intra modes in lossy 4:4:4 predictive CTUs can produce a VTM
coefficient-range failure on a changed desktop frame. The shared chroma mode
selector now declines that reuse for lossy 4:4:4 while retaining the hint path
for subsampled formats. A fresh two-frame reproducer still fails on a later
changed P frame even with temporal hints and explicit inter decisions removed,
so the remaining defect is in the broader lossy predictive residual/partition
path. The all-intra control and maintained geometry gates remain
reference-decodable; this longer changed-P case remains a compliance blocker
for claiming fast predictive parity.

The predictive partition regression audit (2026-08-26) compared the current
stream against the last known-good pre-mixed-P revision using the exact
two-frame 4:4:4 crop and three-frame 4:2:0 multi-CTU fixtures. It found that
decision-to-CABAC conversion was passing the predictive-frame flag even when
the emitted fallback slice was an I slice, causing quantization and entropy
traversal to use different tree contracts. The shared handoff now uses the
actual mixed-P policy. Mixed-P eligibility is also limited to full-CTU 4:2:0
pictures until partial-edge syntax is fully reference-validated; this keeps
the experimental code unified without emitting unverified edge syntax.
Required VTM decoding now passes for both focused fixtures, including the
4:4:4 crop and 4:2:0 multi-CTU sequence. The partial-edge mixed-P case remains
a documented optimization/compliance follow-up, not a hidden fallback.
The AV2 hotspot follow-up (2026-08-26) successfully profiled the committed
local smoke set in lossy mode: the shared `lossy_tile_payload` stage consumed
84.92% of measured wall time across the three vectors. The six-vector profile
was intentionally not substituted with smoke data because its manifest
requires the caller-provided `AOMCTC_ROOT`; no AV2 optimization or performance
claim is retained from the undersized smoke sample.

The AV2 lossless hotspot audit (2026-08-26) profiled the same four-row,
20-frame local set with exact reconstruction. It measured 1,925,221 bytes,
0.528 seconds, and 37.855 aggregate FPS. The shared `lossless_tile_payload`
stage consumed 72.98% of wall time, local IBC search 14.54%, input reading
9.91%, and palette construction 1.94%. This establishes the lossless baseline
for future mode-search or tile-payload work; the IBC hash-index experiment
below was measured against this run and rejected.

The AV2 lossy mode audit then found that the shared proxy scorer calculated
BDPCM costs even though lossy AV2 syntax does not permit BDPCM signaling; the
lossless selector and residual implementation remain unchanged. Removing that
dead lossy-only scoring work kept the four-row, 20-frame probe at 545,482 bytes
and mean PSNR 55.957, while the measured runtime improved from the recorded
1.754 s / 11.404 FPS to 1.656 s / 12.074 FPS. The change is output-neutral,
keeps the lossy candidate set aligned with the legal syntax, and is retained
as a shared-path cleanup rather than a mode-specific implementation.

The AV2 lossless IBC hash-index probe (2026-08-26) replaced the per-tile
linear hash-bucket lookup with a `HashMap` while preserving each bucket's
candidate order. It was rejected after the representative four-row,
20-frame probe remained byte-exact but moved from 0.528 s / 37.855 FPS to
0.533 s / 37.505 FPS; measured IBC time increased from 73.938 ms to 75.976
ms. The small per-tile index is faster in its current vector form, so the
original implementation was restored.

The AV2 lossless fast-mode sampling probe (2026-08-26) tested a coarser 4x4
transform-block sample grid instead of the maintained 2x2 grid. With both
planes coarsened, the four-row, 20-frame probe reduced bytes from 1,925,221 to
1,902,875 but slowed from 0.528 s / 37.855 FPS to 0.571 s / 35.017 FPS. With
only luma coarsened, it produced 1,925,336 bytes and 0.557 s / 35.916 FPS.
Both variants were rejected and the 2x2 sampling policy was restored; the
extra downstream mode-selection cost outweighs the reduced sample count on
this workload.

The VVC adaptive motion-seed probe (2026-08-26) replaced the fixed 8-pixel
seed ring with the outer search-radius ring before unit-diamond refinement.
This followed the source audit's bounded/coarse-search direction, but the
five-frame 1920x1080 predictive screen-content A/B was negative: both versions
produced 355,622 bytes with identical per-frame PSNR, while the probe took
1.35 s versus 1.20 s for the fixed-8 baseline. The probe was reverted. The
predictive two-frame 4:2:0 fixture and the unusual-geometry and multi-CTU
required-reference sets remained clean during the experiment.

The accountability-gate rerun (2026-08-26) passed formatting, workspace check,
Clippy performance lints, the feature matrix, `git diff --check`, and all 356
codec tests. The strict dead-code audit still reports the known experimental
and public-helper inventory, so it remains an actionable ownership audit rather
than a pass. The new `make dependency-audit` target makes dependency review
explicit and reports a clear setup failure when `cargo-audit` is unavailable;
the tool is not installed in this environment. The maintained VVC microbench
also completed without a statistically meaningful change, so no speculative
VVC optimization was retained from that run.

The AV2 direct-coefficient-rate probe (2026-08-26) removed the temporary
4x4 level array from the shared regular-DCT proxy scorer and derived levels
directly during the scan. The five-frame 1920x1080 A/B produced identical
334,760-byte output and identical PSNR, while wall time varied from 0.84 s
to 0.86 s between runs. Since the difference was within run-to-run noise and
the compiler already eliminates the apparent temporary cost, the probe was
reverted. No candidate or coding-path change is justified by this result.

The representative VVC chroma-mode audit (2026-08-26) profiled the four-row,
20-frame local mode probe under lossy `qp=19`, predictive `gop=-1`, and
`fast-search=lossless-speed`. It measured 1,104,492 bytes, 3.092 seconds,
6.469 aggregate FPS, and mean PSNR 57.590. Chroma mode search consumed
754.195 ms (9.91% of measured wall time), including 250.842 ms of explicit
prediction and 384.312 ms of chroma RD scoring. All ordinary explicit
candidate families remained active where applicable; no candidate was
removed. This is the baseline for a controlled chroma candidate-order or
RD-bound experiment, with the complete candidate path retained until quality
and reference evidence justify a change.

The VVC one-winner chroma RD probe (2026-08-26) reduced lossy
`fast-search=lossless-speed` refinement from two cached winners to one. On the
same 20-frame local probe it improved aggregate FPS from 6.469 to 6.768
(4.6%), but increased bytes from 1,104,492 to 1,129,862 (2.3%) and reduced
mean PSNR from 57.590 to 57.395 dB. Required-reference validation passed for
the lossy smoke and unusual-geometry sets (3/3 and 7/7), but the measured
quality/size loss outweighs the modest speed gain. The policy was reverted;
the two-winner bound was retained as the maintainable fast-search default at
that checkpoint.

The follow-up VVC three-winner chroma RD probe (2026-08-26) expanded lossy
`fast-search=lossless-speed` refinement from two cached winners to three. On
the same 20-frame local probe it reduced bytes from 1,104,492 to 1,069,626
(3.15%), improved mean PSNR from 57.590 to 57.758 dB, and measured 6.461 FPS
versus 6.471 FPS. The speed change is within measurement noise while the
rate/quality improvement is material, so the three-winner policy was retained.
The full codec suite passed 356 tests and required VTM smoke validation passed
3/3 with exact reconstruction matches.

The VVC four-winner chroma RD probe (2026-08-26) expanded the same
`lossless-speed` shortlist once more. It produced byte- and PSNR-identical
output on all four 20-frame probe vectors while measured FPS declined, so the
three-winner policy was restored. This confirms that the retained three-winner
bound is the current quality/rate frontier for this fast-search policy.

The VVC chroma score-reuse probe (2026-08-26) removed duplicate reconstructed
residual scoring when a chroma candidate had already been scored by the shared
quantization selector. The candidate wrapper now reuses the existing per-plane
distortion scores and recomputes only the inexpensive coefficient syntax cost;
candidate ordering, tie behavior, residual mode selection, and all coding paths
remain unchanged. On the local six-vector, 50-frame lossy checkpoint, all six
byte counts and PSNR values were identical. Instrumented total time fell from
185.154 s to 177.847 s (1.620 to 1.687 aggregate FPS, +4.1%). The change is
retained as a measured shared-path cleanup, subject to strict VTM validation
and the normal maintainability gates.

The VVC compact-seed directional probe (2026-08-26) expanded the
`lossless-speed` source and spatial consensus seeds to their compact
neighborhoods. It produced byte- and PSNR-identical output on all four
20-frame vectors while reducing measured FPS, including 8.01 to 5.81 FPS on
the gradient case. The candidate-generation change was reverted; broader
directional neighborhoods need a stronger content-adaptive gate before they
are suitable for this speed policy.

The VVC transform-skip-first probe (2026-08-26) removed the lossy
`lossless-speed` shortcut that returns the transform-skip candidate before
scoring transformed residual coding. This kept the shared residual path and
legal syntax unchanged, but the controlled four-vector, 20-frame probe grew
from 4,275,283 to 4,277,988 bytes (+0.06%), moved the mean PSNR only within
the probe's small per-vector variation, and reduced aggregate FPS from 6.47
to 5.97 (-7.7%). The shortcut was restored; a broader transformed-residual
search is not justified without a stronger content-adaptive gate.

The AV2 double-tail candidate probe (2026-08-26) screened out construction of
the regular-DCT double-tail candidate. The four-vector, 20-frame lossy probe
changed only the gradient output (−304 bytes and +0.011 dB), left the other
three outputs unchanged, and measured 11.73 FPS versus the 11.70 FPS baseline,
which is within timing noise. The detailed audit also showed that this
candidate is selected only rarely. The candidate path was restored because
the small, content-specific result does not justify another hot-path gate.

The AV2 zero-motion payload-cache probe (2026-08-26) cached the shared lossy
predictive zero-motion tile payload by tile width and height within each frame.
The payload is generated without frame samples, and a direct regression test
confirms that changing the tile origin does not change its bytes, fields, or
symbol count. The local six-vector, 50-frame lossy checkpoint kept all six
byte counts and PSNR values identical while instrumented time fell from
67.012 s to 65.612 s (4.477 to 4.572 aggregate FPS, +2.1%). Required AVM
validation passed for lossy smoke (3/3) and unusual geometries (7/7). The
cache is local to the shared predictive tile path and does not alter tile
mode selection, entropy state, or lossless coding.

The refreshed VVC end-to-end profile (2026-08-26) measured the current shared
lossy path on the local six-vector checkpoint for 10 predictive frames per
row: 7,403,327 bytes, mean PSNR 54.211 dB, and 29.989 seconds (2.001
aggregate FPS). CTU quantization accounted for 33.46% of measured time;
chroma mode search, chroma RD scoring, and chroma RD refinement accounted for
6.70%, 6.29%, and 5.49%, respectively. Required VTM smoke validation passed
3/3 with exact reconstruction matches. This profile selects shared chroma
candidate evaluation as the next investigation target; no speculative mode
gate or alternate coding path was retained from the profile.

The VVC chroma-prediction cache probe (2026-08-26) extended the existing
shared RD shortlist cache with the Cb/Cr predictions it had already scored, so
cached winners could avoid a second predictor pass. It preserved all six
bitstreams and PSNR values, but the controlled six-vector, 10-frame-per-row
run increased from 29.989 s (2.001 FPS) to 30.178 s (1.988 FPS). The extra
sample copies outweighed the saved prediction work, so the source change was
reverted. The existing residual-only cache remains the simpler policy; a
future prediction-reuse attempt needs a workload with a higher cached-winner
hit rate or a lower-copy representation.

The VVC CCLM inner-luma cache probe (2026-08-26) reused the temporary luma
samples across CCLM candidates within a chroma node. The fast-search six-vector
profile selected no CCLM candidates, so the added cache bookkeeping had no
useful hit rate and produced no output change; the source was reverted. The
existing predictor already shares inner-luma preparation between Cb and Cr in
each CCLM prediction call. Future reuse should be considered only alongside a
measured CCLM-heavy workload and an explicit hit-rate counter.

The AV2 quantization-constant hoist (2026-08-27) computes the regular-DCT
dequantization pair once per transform block and reuses it for every
coefficient in the shared 4x4, 4x8, and 8x8 quantizers. This preserves the
existing integer operations and candidate paths while removing repeated table
lookups and divisions. The microbenchmarks improved 4x4 quantization by
15.1--16.6% for the tested sizes (40, 80, and 128 samples); the AV2 unit suite,
formatting, Clippy, and required AVM smoke validation passed 3/3. The change
is retained for a broader encode-matrix measurement before any further
quantization specialization is attempted. Follow-up required-reference
regression validation passed 7/7 for AV2 and 7/7 for VVC, covering multi-frame
4:4:4 cases as well as subsampled geometry; successful reconstruction and
encoded outputs were cleaned up after validation.

The current-settings lossy six-vector checkpoint (2026-08-27) ran all six
persistent local vectors for 50 frames with direct source feeding, AV2
`gop=-1`/QP 24, and VVC `gop=-1`/QP 19 with `fast-search=lossless-speed`.
AV2 produced 47,700,867 bytes at 4.54 aggregate FPS; VVC produced 44,261,243
bytes at 2.34 aggregate FPS. Per-vector PSNR is retained in the generated
reports `verification/generated/encode_matrix/goal-av2-hoist-lossy-20260827/`
and `verification/generated/encode_matrix/goal-vvc-parity-lossy-20260827/`.
These are current-settings checkpoints, not deltas against the older report,
whose source files and predictive-setting semantics differ.

The matching current-settings VVC lossless slice completed all six vectors at
50 frames with `gop=-1` and `fast-search=lossless-speed`: 123,257,981 bytes at
3.84 aggregate FPS. The four current checkpoint aggregates are therefore:

| Codec | Mode | Bytes | Aggregate FPS |
|---|---|---:|---:|
| AV2 | lossless | 265,419,341 | 3.32 |
| VVC | lossless | 123,257,981 | 3.84 |
| AV2 | lossy, QP 24 | 47,700,867 | 4.54 |
| VVC | lossy, QP 19 | 44,261,243 | 2.34 |

These figures are checkpoint measurements, not a claim of parity with an
external codec baseline; the per-vector reports retain the required settings
and source provenance for the next controlled comparison.

The AV2 derived-quant-constant follow-up (2026-08-27) also hoisted the
per-coefficient reciprocal and rounding values into the same per-block
parameter bundle. Criterion measured an additional 4.4--7.2% improvement over
the previous hoist for the 4x4 transform benchmark at 40, 80, and 128 samples.
The focused AV2 transform tests, Clippy, formatting, and required AVM smoke
validation passed 3/3. The change keeps the existing integer arithmetic and
shared transform path; the follow-up also consolidates the identical
4x4/4x8/8x8 dequantization loops into one const-generic helper. Broader
end-to-end gains remain to be measured.

The VVC chroma pair-residual probe (2026-08-27) replaced two independent Cb
and Cr coordinate walks in lossy chroma RD mode search with one shared walk
that writes both residual vectors. The helper preserves the existing edge
clamping and per-plane delta arithmetic, and is not used by the lossless mode
search. Criterion showed 2.3--5.1% lower time in the measured 8-bit/lossy
cases. The 8-bit lossless movement and the observed 10-bit lossless variation
are outside this helper's execution path and are not attributed to the probe.
Full codec tests (357), required VTM smoke (3/3), required VTM high-depth
lossless validation (6/6), Clippy, and formatting passed.

The VVC BDPCM chroma residual pass (2026-08-27) extended the same shared
Cb/Cr residual walk to BDPCM candidate construction. This removes a second
independent coordinate traversal without changing the candidate set, syntax,
reconstruction, or the unified lossless/lossy control flow. Criterion measured
4.0% lower time for the 8-bit 4:4:4 lossless microbenchmark and 4.4% lower time
for the 10-bit 4:4:4 lossless case; the other measured cases remained within
the benchmark noise threshold. Workspace tests, Clippy, formatting, and
required AV2/VVC smoke validation passed 3/3 for each codec. The external
comparison harness was also checked, but a fresh six-vector run was not counted
as a checkpoint because the local AOM CTC media root was not supplied and the
dependent rows could not be materialized.

The VVC zero-chroma-score probe (2026-08-27) tried to skip residual
quantization and reconstruction scoring after detecting an all-zero chroma
residual. It preserved the transformed-versus-transform-skip rate-cost tie
behavior and passed its focused test, but the full six-case VVC microbenchmark
showed no statistically meaningful end-to-end change in any lossless or lossy
case. The extra residual scan can offset the saved work, so the probe was
reverted and no new fast path was retained.

The VVC predictive chroma zero-detection cleanup (2026-08-27) now shares the
Cb/Cr coordinate walk when constructing residuals for inter candidates and
returns both zero flags from the same implementation. The ordinary pair
helper and the zero-tracking helper are const-generic views of one traversal;
no coding decision, syntax path, or reconstruction rule is duplicated. The
materialized residual equivalence test passed, required regression validation
passed 7/7 for both codecs, and required high-depth validation passed 6/6 for
VVC and 3/3 for AV2. This change is retained as a maintainability and
predictive-path cleanup; its end-to-end speed effect remains workload-specific
and is not claimed as a six-vector checkpoint gain.

The AV2 quantization follow-up benchmark (2026-08-27) was rerun as an
accountability check after the reciprocal hoist. Palette selection remained
slightly faster on the 64x64 case, while the transform round-trip cases at
qindex 40 and 80 stayed within noise and qindex 128 measured a 2.75% slowdown
against its stored Criterion baseline. No additional AV2 quantization change
was retained from this observation; the qindex-dependent result remains a
follow-up investigation item rather than a claimed regression until it is
reproduced with an end-to-end encode workload.
