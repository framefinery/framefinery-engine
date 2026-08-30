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
- VVC predictive encoding helpers were separated from the main encoder path.
- VVC CTU mode selection was separated from CTU orchestration helpers.
- VVC CCLM prediction was separated from the remaining prediction kernels.
- AV2 lossy subsampled scoring was separated from its materialization path.
- AV2 transform-block entropy writers were separated from coefficient/context
  helpers.
- The optional `vvc-stats` feature was made independently compilable instead of
  implicitly depending on benchmark-only internals.

All of these remain included in their original parent module scope, so the
split does not create an alternate coding path or change name resolution.

## Remaining hotspots

The next refactor should be selected from this inventory after inspecting the
current call graph rather than by line count alone:

| Area | Approximate size | Current concern |
| --- | ---: | --- |
| VVC CABAC CTU generation | 2,900 lines | tightly coupled partition traversal, neighbour state, and syntax emission |
| VVC residual prediction | 2,200 lines after CCLM split | shared angular/reference-edge machinery still has several large helpers |
| AV2 lossless subsampled residuals | 2,365 lines | materialization and score paths need further comparison before extraction |
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
