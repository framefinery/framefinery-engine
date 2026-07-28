# VVC Module Layout

The VVC encoder is organized around the current all-intra software path while
keeping codec-specific internals independent from AV2.

- `mod.rs` is the public module surface and includes small root-level chunks.
- `api.rs` contains public encode request, option, artifact, progress, and
  callback types.
- `geometry.rs` contains picture geometry, CTU-region, sample, and quantized
  CTU data structures.
- `format.rs` contains picture-format and syntax-tool configuration.
- `mode_decision.rs` contains residual-mode, intra-mode, and TU decision policy.
- `sampling.rs` converts input frame bytes into the internal sampled-frame
  representation.
- `reconstruction.rs` owns reconstructed frame storage and availability maps.
- `ctu.rs` contains CTU traversal and local CTU leaf-size selection.
- `ctu_params.rs` maps quantized CTUs into CABAC partition parameters.
- `cabac_dump.rs` contains VVC CABAC vector dump formatting.
- `stats.rs` contains optional `vvc-stats` instrumentation sinks and counters.
- `encode.rs` contains the stream-level encode loop.
- `bitstream/` contains Annex-B/NAL and bit-level RBSP writing helpers.
- `headers/` contains VPS/SPS/PPS/slice-header syntax generation.
- `cabac/` contains CABAC context models, writer, and CTU body emission.
- `residual/` contains residual prediction, transform, quantization,
  reconstruction, syntax, and local residual tests.
- `residual/quant/` contains the mode-search and quantization pipeline split
  into CTU traversal, RD caches, luma/chroma mode refinement, luma/chroma
  residual finalization, transform-skip helpers, trace formatting, and tests.
- `palette/` contains the 4:4:4 screen-content palette path split into types,
  reconstruction, syntax, binarization, CU emission, slice emission, and dumps.
- `inter/` contains current intra-block-copy helpers.
- `benchmarks/` contains benchmark-only internal entry points.
- `tests/` contains the VVC root module test suite.
