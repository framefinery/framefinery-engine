# FrameFinery Engine

FrameFinery Engine is a safe Rust media pipeline toolkit. The project is built around
composable stages:

```text
input -> decode -> filter -> encode -> output
```

The initial focus is experimental video encoding and validation infrastructure,
with AV2 and VVC as the first planned codec families. The repository is a
software-only sibling of the FrameFinery Engine hardware project: FrameFinery Engine remains the
RTL, synthesis, and hardware-validation workspace, while this repository is free
to optimize for software APIs, usability, codec quality, and safe Rust
performance.

This repository is in bootstrap state. It currently provides project structure,
shared media primitives, a CLI, and imported experimental AV2/VVC software
models from the FrameFinery Engine hardware workspace.

## Goals

- Provide safe Rust media pipeline components.
- Keep codec implementations modular and independently selectable at build time.
- Validate generated bitstreams and reconstructions with strict, reproducible
  tests.
- Support commercial and non-commercial use under a permissive license.
- Grow from codec and validation foundations into a broader media toolkit
  without forcing premature abstractions.

## Quick Start

Requirements:

- Rust toolchain with Cargo.
- `make`.

Check the local toolchain:

```sh
make check-tools
```

Build and test:

```sh
make build
make test
```

`make build` creates a release binary at:

```sh
./ff
```

For a debug build, use:

```sh
make debug
```

Run the CLI:

```sh
make run ARGS="--help"
```

The crates.io package is intended to be `framefinery`. By default it includes
the public library facade, the `ff` CLI binary, AV2, VVC, and the current filter
catalog:

```sh
cargo install framefinery
ff --help
```

API documentation is published by docs.rs after crates.io publication:

```text
https://docs.rs/framefinery
```

The CLI guide lives in
[`docs/cli.md`](https://github.com/framefinery/framefinery-engine/blob/main/docs/cli.md).

Generate a standalone Rust module/code browser:

```sh
make code-browser
```

The generated HTML is written under `verification/generated/code_browser/`.
To overlay gated wall-time profiling data on the same browser, run:

```sh
make profile-hotspots HOTSPOT_CODECS="av2 vvc" HOTSPOT_VISUALIZE=1
```

That workflow builds only the requested compile-gated stats features and writes
the matrix, wall-time summary, raw JSONL traces, and heatmap browser under
`verification/generated/profiling/hotspots/`.

The installed command name is intended to be short:

```sh
./ff --help
./ff --help codecs
./ff --help filters
./ff --help pixfmt
./ff --help settings
./ff --help presets
```

Run the default local quality gate:

```sh
make release-check
```

Release candidates should also run the local AOM CTC release manifest and save
a performance table for version-to-version comparison:

```sh
make validate-release-aomctc
make release-performance-table
```

Those targets read the local A5/B2 Y4M files under
`/media/gabriel/storage/YUV/aomctc` directly, avoid decompression, and clean
encoded/reconstruction artifacts after metrics are collected.

Generate and run the current software encode fixtures:

```sh
make test-vector-sets
make validate-set CODEC=av2 VALIDATION_SET=smoke
make validate-set CODEC=vvc VALIDATION_SET=smoke
make validate-set CODEC=av2 VALIDATION_SET=smoke VALIDATION_SOURCE_FILTERS=1
make validate-set CODEC=av2 VALIDATION_SET=pipeline-smoke
```

Reference decoders are optional but recommended for strict bitstream checks.
Declared reference toolchains can be listed and built with:

```sh
make reference-list
make reference-setup
make reference-setup REFERENCE_CODEC=av2
```

Validation uses `VALIDATION_REFERENCE_MODE=auto` by default: if a declared
reference decoder is already built or configured, the runner decodes the
FrameFinery Engine bitstream and compares that reconstruction against the encoder's
internal reconstruction. Use `VALIDATION_REFERENCE_MODE=required` to fail when
the reference decoder is missing, or `VALIDATION_REFERENCE_MODE=off` to skip
external decoding.

## Build-Time Composition

Codec and filter availability is selected at build time. The published
`framefinery` package enables the normal product feature set by default, so
`cargo install framefinery` and `make build` include every codec and filter stage
currently intended for the developer CLI without compiling analysis-only
instrumentation.

Override `CARGO_FEATURES` to build a smaller or more specialized binary:

```sh
make build CARGO_FEATURES=all
make build CARGO_FEATURES="av2 filter-pattern filter-identity"
make build CARGO_FEATURES=
```

`CARGO_FEATURES=all` means all normal product features. The user-facing `av2`
and `vvc` features, plus the compatibility `codec-av2` and `codec-vvc`
features, enable the imported experimental software models. The
`filter-pattern` feature enables input-free generated pattern sources for
fixtures. `filter-identity` enables the no-op transform filter used to exercise
the executable frame pipeline. Other filter features are discovery placeholders
for now. Analysis-only features, such as AV2 superblock bit accounting and codec
wall-time profiling, are enabled through dedicated Makefile switches like
`AV2_SB_BITS=1`, `AV2_STATS=1`, and `VVC_STATS=1` so the normal product build is
not slowed by instrumentation.

## CLI Shape

The CLI entry point is `ff`. The initial interface is centered on stage
discovery and a single encode action:

```sh
ff --help
ff --help codecs
ff --help filters
ff --help pixfmt
ff --help settings
ff --help presets
ff codecs
ff filters
ff encode input.yuv --video 640x360:yuv444p \
  --encode av2:output.obu --set lossless
ff encode input.y4m --encode av2:output.obu --set lossless
ff encode input.y4m --encode av2:output.obu --set qp=24
ff encode --filter pattern=checker --video 64x64:yuv444p \
  --frames 1 --encode av2:pattern.obu
ff encode input_640x360_30_1f_yuv444p8.yuv \
  --filter identity --encode av2:output.obu
ff encode input_640x360_30_1f_yuv444p8.yuv \
  --encode av2:output.obu --recon output_recon.yuv
ff encode input_640x360_30_1f_yuv444p8.yuv \
  --encode av2:output.obu --set qp=24 --psnr
```

The commands validate command-line structure and report stage availability.
When built with `av2`/`codec-av2` or `vvc`/`codec-vvc`, `ff encode` can encode
raw YUV inputs and Y4M inputs through the imported software model for that codec. Y4M
files are demuxed by the shared input reader before frames reach AV2 or VVC.

Current option placement and inference rules:

- Input options, such as `--video`, `--fps`, and `--frames`, belong after the
  input path and override metadata inferred from filenames or Y4M headers.
- If `--frames` and filename frame-count metadata are both omitted for a file
  input, `ff encode` processes whole frames until the raw input file or Y4M
  stream reaches EOF.
- If `--frames` is larger than the number of complete frames in a file,
  `ff encode` stops at EOF instead of failing.
- Source filters require explicit `--frames` because they do not have a file
  EOF.
- Output/encoder options, such as `--recon output.yuv`, `--psnr`,
  `--set lossless`, `--set qp=<1..255>`, `--preset`, and repeated
  `--set key[=value]`, belong after `--encode codec:output`.
- `--recon <path>` writes the encoder's internal reconstructed raw stream for
  debugging and reference validation. `--psnr` calculates per-frame PSNR from
  that same internal reconstruction without writing the raw reconstruction
  stream.
- Bare `--set` keys imply `true`. `--set qp=<1..255>` requests lossy AV2 or VVC
  quantization and is mutually exclusive with `--set lossless`; lower values
  preserve more detail.

Global accepted settings are listed by `ff codecs`; codec-specific settings are
listed with the codec that owns them. The current codec-specific
`--set predictive` mode is experimental. AV2 starts a multi-picture stream and
uses show-existing-frame, zero-MV tiles, and motion-residual tiles where the
current subset can encode them; otherwise it falls back to the existing
key-frame path. VVC accepts the same predictive setting so temporal coding tools
can be developed and benchmarked behind the same CLI shape.

The positional input is optional when the first filter is a source. The initial
source filter is `pattern=<name>`, with `black`, `checker`, `gradient`, and
`color_blocks` patterns. Source filters require explicit `--video` metadata
because there is no filename to infer dimensions or pixel format from. The
`identity` transform filter is executable for file inputs and source-filter
inputs; `crop` and `scale` remain listed as future stage scaffolds and are
rejected until their frame transforms are implemented.

Current filter capability:

| Filter | Kind | Status |
|---|---|---|
| `pattern=<name>` | source | executable generated input |
| `identity` | transform | executable no-op frame pass-through |
| `crop` | transform | scaffold; rejected until implemented |
| `scale` | transform | scaffold; rejected until implemented |

Raw video metadata uses a compact `WxH:pixfmt` spelling when it cannot be
inferred from the input filename or Y4M header, or when it needs to be
overridden. File names imply metadata with
`*_<WxH>[_<fps>][_<frames>f][_<pixfmt>].yuv`, for example
`input_640x360_30_1f_yuv444p8.yuv`. Y4M headers provide width, height, frame
rate, and planar YUV pixel format. Short 8-bit aliases such as `yuv444p` and
`yuv420p` are accepted and normalized to `yuv444p8` and `yuv420p8` internally.
Planar YUV and gray input formats accept checked numeric bit depths from 8
through 16, for example `yuv420p9le`, `yuv444p12le`, and `gray16le`.
If a `.yuv` filename has dimensions but no pixel-format token, the CLI assumes
`yuv420p8`. Encode endpoints must name the codec and output path together, such
as `--encode av2:output.obu`.

The raw input CLI/API contract is documented in
[`docs/raw-input-formats.md`](docs/raw-input-formats.md).

## Repository Layout

```text
crates/
  framefinery-cli/   Published as package `framefinery`; library facade plus `ff`.
  framefinery-core/  Shared frame, packet, error, and pipeline primitives.
  framefinery-codecs/  Imported experimental AV2/VVC software models.
docs/                     Architecture and validation notes.
tests/                    Future shared integration tests and fixtures.
tools/                    Future development and validation helper scripts.
```

## Current Limitations

- Compressed input decode is not implemented yet; `ff encode` accepts raw YUV
  and Y4M inputs.
- AV2 and VVC encoders are experimental software models, not production codec
  implementations.
- `identity` is the only executable transform filter. `crop` and `scale` are
  feature-gated discovery scaffolds.
- Reference decoders are optional local tools. Use
  `VALIDATION_REFERENCE_MODE=required` when a release or compatibility claim
  depends on external decode validation.

## Safety Posture

FrameFinery Engine should use safe Rust. Performance work should start with safe
Rust, better algorithms, optimizer-friendly data layout, and compiler-supported
optimizations. Optimized implementations that replace correctness-critical
kernels should be proven bit-exact against simple scalar implementations.

## Validation Direction

Validation should remain strict and reproducible:

- lossless paths must reconstruct exactly;
- lossy paths should report PSNR and bitrate;
- reference decoders should validate generated bitstreams when available;
- checksums and bitstream sizes should be recorded for regressions;
- generated test vectors should be deterministic.

The first batch fixtures live under `verification/test_vector_sets/`. They are
generated on demand into `verification/generated/test_vectors/` and encoded by
`scripts/run_validation_set.py`, which records per-vector logs, output sizes,
and SHA-256 checksums under `verification/generated/`.

## License

FrameFinery Engine is licensed under the Apache License, Version 2.0.

The project is open for commercial and non-commercial use. Companies and
individuals may build public or proprietary extensions, products, and services
on top of it under the terms of the Apache-2.0 license.

Codec patent rights are separate from source-code copyright licensing. Users are
responsible for evaluating any codec patent or deployment obligations that apply
to their use case and jurisdiction.
