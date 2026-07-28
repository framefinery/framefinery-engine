# AV2 Module Layout

The AV2 implementation is still an imported experimental encoder model, but
the files are grouped by responsibility so individual codec areas are easier to
audit:

- `bitstream/`: bit-level syntax, entropy writer, and optional bit accounting.
- `tile/`: tile entropy payload construction, block layout, partition syntax,
  residual coding, TXB coding, and local coding contexts.
- `palette/`: screen-content palette modes, palette building, scoring, and
  prediction helpers.
- `prediction/`: intra-prediction kernels shared by palette and tile coding.
- `inter/`: intra-block-copy and motion-search helpers.
- `image/`: planar image layout helpers used by codec internals.
- `mode/`: local mode-decision helpers.
- `benchmarks/`: optional benchmark-only internals.

The AV2 root also has focused chunks that are included by `mod.rs` to keep the
current imported-code namespace stable while making the file boundaries easier
to navigate:

- `api.rs`: public request/option/metrics types.
- `format.rs`: AV2 stream format, profile, and quantization state.
- `layout.rs`: visible geometry and tile layout derivation.
- `frame_mode.rs`: 4:4:4 MVP frame-mode selection.
- `encode.rs`: public encode loop and per-frame bitstream assembly.
- `predictive.rs`: predictive/inter-frame tile-mode selection.
- `headers/`: sequence headers, frame headers, and frame payload assembly.
- `obu.rs`: OBU packing and tile-group byte layout.
- `trace.rs`: JSONL syntax/entropy trace output.
- `reconstruction.rs`: black-frame reconstruction and request validation.
- `tests.rs`: codec-level AV2 unit tests.

`mod.rs` now acts as the AV2 module map.
