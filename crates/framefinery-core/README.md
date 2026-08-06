# framefinery-core

`framefinery-core` contains the reusable media primitives behind FrameFinery.
It is intentionally codec-agnostic: codec crates and applications can share
frame buffers, pixel-format handling, pipeline traits, and small reusable
filters without depending on AV2, VVC, or the `ff` command-line frontend.

## API Surface

The crate is organized around a simple media pipeline:

```text
Source -> Filter -> Encoder -> Sink
```

Important public types:

- `FrameInfo`, `Frame`, `FrameRef`, `PixelFormat`, `SampleBitDepth`, and
  `ChromaSampling` describe raw frames and validated buffer sizes.
- `CodecId`, `VideoEncoderConfig`, `RawVideoFrameSource`,
  `VideoEncoderSession`, and `EncodedVideoChunk` define the molten v0 video
  encoder API contract.
- `DecodedPictureBuffer`, `DpbEntry`, and `PictureId` provide codec-neutral
  reference-picture storage helpers without imposing codec reference-list
  policy.
- `Source`, `Filter`, `Encoder`, and `Sink` are small traits for pipeline
  stages.
- `run_frame_filter_pipeline` and `run_frame_encode_pipeline` connect stages
  and report frame/packet counts.
- `IdentityFilter` is the no-op transform filter.
- `PatternSource` and `PatternKind` generate deterministic raw-video test
  frames.
- `FILTERS` and `filter_manifest` expose the reusable filter catalog used by
  the `ff` CLI.
- `FilterStageSpec`, `FilterPipelineSpec`, `parse_filter_pipeline_specs`,
  `generate_source_filter_stream`, and `build_filter_transform` let frontends
  pass filter specs through the core registry without knowing concrete filter
  implementation types.
- `FilterSpecManifest` describes each filter's accepted spec forms,
  parameters, examples, and notes.

## Filters

Filter manifests live in this crate so library users and the CLI discover the
same compiled stages:

```rust
use framefinery_core::{filter_manifest, FilterStageKind};

let filter = filter_manifest("identity").expect("identity filter");
assert_eq!(filter.stage, FilterStageKind::Transform);
assert_eq!(filter.spec.forms[0].syntax, "identity");
```

Filter spec metadata is structured so applications can render their own help
pages without copying CLI strings:

```rust
use framefinery_core::filter_spec_manifest;

let spec = filter_spec_manifest("pattern").expect("pattern spec");
assert_eq!(spec.forms[0].syntax, "pattern=<name>");
```

`PatternSource` is modeled as a source filter because it creates frames instead
of transforming an input stream:

```rust
use framefinery_core::{FrameInfo, PatternKind, PatternSource, PixelFormat, Source};

let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8)?;
let mut source = PatternSource::new(info, PatternKind::Checker, 1)?;
let frame = source.pull()?.expect("one generated frame");
assert_eq!(frame.info(), info);
# Ok::<(), framefinery_core::MediaError>(())
```

Current pattern output supports `yuv420p8` and `yuv444p8`. The accepted pattern
names are `black`, `checker`, `gradient`, and `color_blocks`; `blocks` is
accepted as a short alias for `color_blocks`.

`crop` and `scale` are currently manifest scaffolds. They are kept out of the
default product filter set and must be compiled explicitly with `all-filters`,
`filter-crop`, or `filter-scale` until executable implementations and
validation coverage exist.

## Features

The default `product-filters` feature enables the executable filter catalog for
direct `framefinery-core` users:

- `filter-pattern`
- `filter-identity`

Discovery scaffolds are available as explicit opt-ins:

- `all-filters`
- `filter-crop`
- `filter-scale`

Applications that want a smaller compiled surface can disable default features
and enable individual filter implementations. Applications that want the
scaffold manifests as well can use `all-filters`:

```toml
framefinery-core = {
  version = "0.0.2",
  default-features = false,
  features = ["filter-identity"]
}
```

Disabled filters are not listed in `FILTERS`. Concrete implementation types and
helpers, such as `IdentityFilter` and `PatternSource`, are compiled only when
their matching `filter-*` feature is enabled.

## v0 Video API

The first encoder API contract is documented in
[`../../docs/api-v0.md`](../../docs/api-v0.md). It is intentionally molten: the
contract is useful enough for the CLI, Rust API, and future WASM work to share a
direction, but it can still change before `0.1.0`.

Long-stream adapters should prefer `RawVideoFrameSource`: the caller fills one
frame buffer on demand and the encoder consumes frames without requiring the
whole raw stream to be resident in memory or requiring a total frame count up
front. `VideoEncoderConfig::frame_limit` is an optional caller/source bound,
not an encoder requirement. `Frame` remains the owned post-filter and
reconstruction type; `FrameRef` is the borrowed validated view.

## Errors

Public fallible APIs return `framefinery_core::Result<T>`, an alias for
`Result<T, MediaError>`. Buffer sizes and frame dimensions are validated before
frame construction or pattern generation.

Encoder-facing API boundaries use structured `MediaError` variants for common
failures such as unsupported codecs, unsupported pixel formats, unknown or
invalid settings, short raw-frame reads, encode-after-flush, and frame-limit
violations. Experimental codec internals may still surface string messages
until the failure mode is stable enough to expose directly.
