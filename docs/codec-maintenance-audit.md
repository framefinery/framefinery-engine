# Codec maintenance audit

This document records the current structural cleanup checkpoint for the codec
implementations. It is deliberately separate from bitrate comparisons: its
purpose is to track maintainability, feature accountability, and validation
invariants while the encoders continue to evolve.

## Invariants for every refactor

- Keep lossless and lossy coding on the same traversal and reconstruction
  pipeline. Differences belong in the deepest mode or candidate selection
  layer.
- Keep pixel-format and profile restrictions at the mode-selection boundary
  where possible. Do not duplicate syntax or reconstruction implementations
  for a format or profile.
- Preserve safe Rust and validate dimensions, plane lengths, and arithmetic
  behavior at public and allocation boundaries.
- Require reference-decoder reconstruction agreement for generated streams.
  Lossless streams must reconstruct exactly; lossy streams must retain their
  recorded quality measurements.
- Run formatting, the complete codec tests, feature-matrix checks, the
  dead-code audit, performance-focused Clippy, required-reference smoke
  validation, and the representative lossless/lossy metric matrix before each
  cleanup commit.

## Completed structural cleanups

The following changes were behavior-preserving and independently validated:

- VVC CABAC statistics data/sink code was separated from statistics analysis.
- VVC intra MPM and mode-syntax helpers were separated from CTU traversal and
  neighbour-state orchestration.
- VVC predictive encoding helpers were separated from the main encoder path.
- VVC CTU mode selection was separated from CTU orchestration helpers.
- VVC CCLM prediction was separated from the remaining prediction kernels.
- VVC intra top/left reference-edge collection was separated from prediction
  kernels while retaining shared availability handling.
- VVC reference-sample availability, fallback, and top-left access are now
  isolated as shared helpers for prediction and reference-edge collection.
- VVC reference filtering and DC prediction arithmetic now live in a dedicated
  helper module shared by the common luma/chroma prediction paths.
- VVC plane-availability storage and bounds-checked lookup now live with the
  shared reference-access helpers.
- VVC planar/DC and angular PDPC arithmetic now lives in a dedicated helper
  module used by the common prediction paths.
- VVC angular zero-reference and multi-reference-line construction now lives
  in a dedicated reference-builder module.
- VVC transform-skip residual placement and reconstruction now share one
  parameterized kernel across luma/chroma and identity/QP/table paths.
- VVC BDPCM transform-skip inverse traversal and reconstruction now share the
  same parameterized kernel across luma and chroma paths.
- VVC visible luma and chroma reconstruction now share one clipped
  plane-writing loop, with subsampling handled only at the caller boundary.
- VVC visible reconstruction wrappers and their shared clipped plane writer now
  live in a dedicated reconstruction module, separate from prediction kernels.
- VVC BDPCM prediction and residual adapters now live in a dedicated module,
  retaining one shared plane predictor for luma and chroma.
- VVC luma and chroma transform-skip SSE scoring now share one kernel,
  including the common BDPCM accumulation, clipped tail handling, and
  format-neutral residual accounting utilities.
- VVC luma and chroma transform-skip AC extraction now share one bounded
  level-placement and nonzero-tracking kernel.
- VVC finalized residual-block and RD-score value types now live in the shared
  quantization types module used by luma, chroma, and CTU selection.
- VVC finalized luma and chroma TU records now live beside the shared
  quantization records used by CTU orchestration and tracing.
- AV2 visible/coded geometry padding and cropping now lives in a dedicated
  geometry module, keeping format conversion separate from encode traversal.
- VVC scalar residual energy, sum, and signed rounding helpers now live in a
  dedicated transform-math module separate from transform selection.
- AV2 lossy 4:2:2 and 4:4:4 chroma residual writers now live in a dedicated
  chroma-residual module while retaining the shared lossy analysis path.
- VVC DCT/DST basis lookup and MTS normalization now live in a dedicated
  transform-basis module separate from quantization and dequantization.
- VVC transform dequantization parameters and DC-only reconstruction now live
  in a dedicated dequant module beside the transform-basis helpers.
- VVC inverse transform and dequantized-level reconstruction now live in a
  dedicated transform-inverse module separate from forward quantization.
- VVC inter-motion state, AMVP candidate selection, and HMVP retention now
  live in a dedicated inter-motion module separate from CABAC emission.
- VVC luma intra-mode syntax and BDPCM/MIP/MRL/ISP eligibility checks now live
  in a dedicated mode-syntax module on the shared CABAC generator.
- VVC luma post-residual LFNST and MTS syntax gates now live in a dedicated
  residual-tools module on the shared CABAC generator.
- VVC chroma leaf/tool legality, split contexts, and BT direction selection now
  live in a dedicated chroma mode-selection module on the shared generator.
- VVC chroma transform-leaf, BDPCM, and intra-prediction syntax now live in a
  dedicated chroma-leaf module on the shared generator.
- VVC chroma coefficient emission now lives in a dedicated residual-syntax
  module while retaining the common CABAC coefficient writer.
- VVC luma CBF, transform-unit dispatch, and stored-coefficient emission now
  live in a dedicated luma-residual module on the shared generator.
- AV2 lossless intra/inter residual coefficient writers now live in a dedicated
  lossless-residual module, separate from lossy residual selection.
- AV2 palette-specific luma residual emission now lives in a dedicated
  palette-residual module, separate from the general residual writer.
- AV2 lossy subsampled scoring was separated from its materialization path.
- AV2 lossless subsampled tile construction and plane-coordinate mapping now
  live in a dedicated state module, separate from prediction and scoring.
- AV2 source-backed lossless DPCM now uses the shared edge-predictor residual
  kernel instead of a parallel hand-written materialization loop.
- AV2 source-backed lossless predictor adapters now live in a dedicated module,
  separate from ordinary reconstruction and score-path logic.
- AV2 lossless DC, horizontal, vertical, and BDPCM proxy scoring now lives in a
  dedicated score module, separate from residual materialization.
- AV2 lossy transform candidate selection and RD gates were separated from
  residual coefficient writing.
- AV2 transform-block entropy writers were separated from coefficient/context
  helpers.
- AV2 inter-residual tracing was separated from residual coefficient writing,
  keeping diagnostics and feature-gated instrumentation out of the coding
  pipeline.
- AV2 regular transform-partition syntax writers now share a dedicated helper
  cluster for intra and inter paths.
- AV2 residual scalar rounding, quantization-step, and TXB end-of-block
  helpers now live in a shared residual-math module.
- AV2 lossless directional-angle and residual proxy scoring helpers are now
  isolated from the stateful lossless tile implementation.
- AV2 lossless and lossy DC prediction now share one edge-accumulation and
  rounding kernel, with only typed edge access supplied by each state path.
- AV2 horizontal, vertical, and above-left intra predictors now share common
  tile-edge fallback kernels across lossless and lossy state paths.
- AV2 coefficient-proxy scoring and high-range symbol-cost helpers now live in
  a dedicated subsystem shared by residual candidate selection paths.
- AV2 lossless and lossy horizontal, vertical, and above-left predictor
  selection now share common edge-policy kernels.
- The optional `vvc-stats` feature was made independently compilable instead of
  implicitly depending on benchmark-only internals.

All of these remain included in their original parent module scope, so the
split does not create an alternate coding path or change name resolution.

## Remaining hotspots

The next refactor should be selected from this inventory after inspecting the
current call graph rather than by line count alone:

| Area | Approximate size | Current concern |
| --- | ---: | --- |
| VVC CABAC CTU generation | 2,757 lines after mode-syntax split | tightly coupled partition traversal, neighbour state, and syntax emission |
| VVC residual prediction | 2,098 lines after reference-edge split | shared angular/reference machinery still has several large helpers |
| AV2 lossless subsampled residuals | 1,753 lines | materialization and score paths need further comparison before extraction |
| AV2 tile transform syntax helpers | 1,516 lines after writer split | coefficient contexts remain broad but syntax-sensitive |
| VVC residual quantization | 943-line orchestration plus mode-selection sibling | luma/chroma selection should be unified only where types and gates truly match |

For the next functional cleanup, prefer extracting a small shared helper with
bit-exact scalar tests over introducing a broad cross-codec abstraction. AV2
and VVC should continue sharing only stable infrastructure unless their syntax
contracts genuinely coincide.

## Validation records

The durable full-release baseline and subsequent metric records live under
`verification/generated/encode_matrix/`. Generated validation logs and
reference reconstructions are disposable; manifests, deterministic test
vectors, and recorded checkpoints are retained when intentionally needed for
regression analysis.

The current cleanup series is intentionally incremental. A future optimization
may change bytes, PSNR, or FPS, but it must report the delta against the last
recorded checkpoint and explain the heuristic or mode-selection change before
it is accepted.
