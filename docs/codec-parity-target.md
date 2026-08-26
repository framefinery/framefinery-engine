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
clippy-perf`, `make feature-matrix`, `make dead-code-audit`, `git diff --check`,
and required AVM/VTM decode validation. `cargo audit` is not installed in the
current environment, so dependency auditing remains an explicit follow-up
when that tool is available; no substitute lint or blanket suppression is
being claimed as equivalent. `perf` is present but cannot access hardware
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
