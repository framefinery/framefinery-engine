# FrameFinery v0 API Contract

This document is the first public API contract for FrameFinery. It is molten:
the names, shapes, and boundaries are expected to move before `0.1.0`, but the
purpose of writing them down now is to make the codebase converge around one
integration model instead of letting the CLI, native library API, and future
WASM package drift apart.

The contract describes the API FrameFinery wants callers to build against. It
does not describe codec internals, helper ownership, or a promise that every
codec must share a given helper implementation.

## Design Principles

- The public surface is codec-neutral. Callers select codecs by ID, such as
  `av2` or `vvc`, rather than by constructing codec-named public classes.
- Codec-specific types may exist inside codec modules, but they are not the
  preferred integration API.
- CLI, native Rust, and future WASM frontends should all build the same encoder
  config and drive the same session/chunk concepts.
- Strings are accepted at the CLI edge, but library APIs should move toward
  typed settings, rate-control modes, frame metadata, and chunk metadata.
- Files, sockets, browser workers, and validation artifacts are adapters around
  the encoder API. The encoder core should not require them.
- Shared codec helpers are opportunistic internal utilities. They are not a
  public contract, not owned by one codec, and not a requirement for future
  codecs.

## Crate Roles

`framefinery-core` owns the common contract:

- `FrameInfo`
- `Frame`
- `FrameRef`
- `PixelFormat`
- `CodecId`
- `VideoEncoderConfig`
- `RawVideoFrameSource`
- `VideoEncoderSession`
- `EncodedVideoChunk`
- `FrameEncodeMetrics`
- `DecodedPictureBuffer`
- `FilterStageSpec`
- `FilterPipelineSpec`
- filter and encoder manifests

`framefinery-codecs` owns encoder implementations and the generic encoder
registry:

- `ENCODERS`
- `encoder("av2")`
- `encoder("vvc")`
- `create_encoder(config)`

Codec-specific modules remain implementation territory while the API cools.
Benchmark-only internals may be exposed under hidden feature-gated paths, but
normal applications should treat the registry as the codec boundary.

`framefinery` is the user-facing package and CLI facade. Its root Rust API
should prefer generic concepts from `framefinery-core` plus the generic encoder
registry from `framefinery-codecs`.

`framefinery-cli` maps command-line arguments and files onto the same generic
config and registry concepts. It should not contain AV2/VVC implementation
branches or filter implementation branches.

## Codec Identity

Codec identity is represented by `CodecId`:

```rust
use framefinery::{CodecId, FrameInfo, PixelFormat, VideoEncoderConfig};

let input = FrameInfo::new(1920, 1080, PixelFormat::Yuv420p8)?;
let config = VideoEncoderConfig::new(CodecId::new("vvc")?, input);
# Ok::<(), framefinery::MediaError>(())
```

Codec IDs are lowercase ASCII names with optional digits and hyphens. This keeps
Rust, CLI, JS, manifests, and future package metadata using the same stable
tokens.

## Video Encoder Config

`VideoEncoderConfig` is the standard config object:

```rust
use framefinery::{
    CodecId, FrameInfo, PixelFormat, ReconstructionMode, VideoEncoderConfig,
    VideoEncoderSetting, VideoRateControl,
};

let input = FrameInfo::new(1280, 720, PixelFormat::Yuv420p8)?;
let config = VideoEncoderConfig::new(CodecId::new("av2")?, input)
    .with_rate_control(VideoRateControl::constant_quantizer(24)?)
    .with_reconstruction(ReconstructionMode::MetricsOnly)
    .with_setting(VideoEncoderSetting::boolean("predictive", true)?);
# Ok::<(), framefinery::MediaError>(())
```

Current shared concepts:

- `codec`: selected `CodecId`;
- `input`: validated `FrameInfo`;
- `frame_rate`: optional rational frame rate;
- `frame_limit`: optional bounded frame count;
- `rate_control`: codec default, lossless, or constant quantizer;
- `reconstruction`: none, metrics only, or reconstructed frames;
- `settings`: codec extension settings as typed name/value pairs.

The CLI can still accept `--set key[=value]`, but it should translate toward
this config shape before invoking codecs. Codec manifests validate the selected
codec ID, input format, lossless support, extension setting names, duplicate
settings, and setting value types before implementation-specific parsing runs.

## Frames

`Frame` owns one complete raw frame. `FrameRef` borrows one complete raw frame.
Both are paired with `FrameInfo`, and both validate that the byte buffer length
matches the declared pixel format and geometry.

The owned form is useful for post-filter flows, tests, and reconstruction
returns. The borrowed form lets adapters and future WASM bindings expose frame
data without immediately copying it into another owned buffer.

## Source-Driven Encoding

Long streams should not be preloaded into memory. The streaming contract is a
raw-frame source callback:

```rust
use framefinery::{
    CodecId, FrameInfo, PixelFormat, RawVideoFrameSource, Result, VideoEncoderConfig,
};

fn fill_black(frame: &mut [u8]) -> Result<bool> {
    frame.fill(0);
    Ok(true)
}

let input = FrameInfo::new(640, 360, PixelFormat::Yuv420p8)?;
let config = VideoEncoderConfig::new(CodecId::new("av2")?, input).with_frame_limit(1);
let mut source = fill_black as fn(&mut [u8]) -> Result<bool>;
# let _ = (&config, &mut source as &mut dyn RawVideoFrameSource);
# Ok::<(), framefinery::MediaError>(())
```

`RawVideoFrameSource::read_frame` fills exactly one caller-provided frame buffer
and returns `Ok(false)` only at EOF. The current AV2/VVC source encoders require
`VideoEncoderConfig::frame_limit` because their bitstream headers still need the
frame count up front. File, Y4M, WebCodecs, screen-capture, and test-vector code
should be adapters that implement this callback shape.

## Encoder Sessions

The intended encoder shape is a frame-session API:

```rust
use framefinery::{create_encoder, Frame, Result, VideoEncodeOutput, VideoEncoderConfig};

fn drive_encoder(
    config: VideoEncoderConfig,
    frames: impl IntoIterator<Item = Frame>,
) -> Result<VideoEncodeOutput> {
    let mut encoder = create_encoder(config)?;
    let mut output = VideoEncodeOutput::default();
    for frame in frames {
        let step = encoder.encode_frame(frame)?;
        output.chunks.extend(step.chunks);
        output.reconstructions.extend(step.reconstructions);
        output.metrics.extend(step.metrics);
    }
    let tail = encoder.flush()?;
    output.chunks.extend(tail.chunks);
    output.reconstructions.extend(tail.reconstructions);
    output.metrics.extend(tail.metrics);
    Ok(output)
}
```

The session path is the natural API for post-filter frame ownership and future
incremental encoders. The current AV2/VVC session implementation is a
compatibility bridge over whole-stream codec internals, so long CLI streams use
source-driven encoding instead of accumulating all frames in a session buffer.
Session semantics are still defined: frames must match `config.input`, `flush`
is idempotent, and encoding after `flush` is an error.

## Encoded Chunks

Encoders should produce `EncodedVideoChunk` values before muxing or transport:

```rust
pub struct EncodedVideoChunk {
    pub codec: CodecId,
    pub kind: VideoChunkKind,
    pub data: Vec<u8>,
    pub frame_index: Option<usize>,
    pub pts: Option<Timestamp>,
    pub dts: Option<Timestamp>,
    pub duration: Option<Timestamp>,
    pub keyframe: bool,
}
```

Chunk kinds are:

- `Config`
- `Frame`
- `EndOfStream`
- `Stream`

`Stream` exists for compatibility with current whole-stream encoders. The long
term goal is frame/access-unit chunks suitable for CLI writing, WASM callbacks,
packetizers, muxers, and streaming sinks.

## Reconstruction And Metrics

Reconstruction is a first-class option because it supports validation,
compression experiments, PSNR reporting, and future WASM demos. `VideoEncodeOutput`
can return zero or more reconstructed frames and zero or more per-frame metric
records.

Current modes:

- `None`: emit only encoded chunks;
- `MetricsOnly`: compute quality metrics without returning reconstructed frames;
- `Frames`: return reconstructed `Frame` values to the caller.

Native CLI builds can map these to `--psnr` and `--recon`. WASM builds should
return metrics or frames to JavaScript instead of writing files.

## Reference Frames

Shared reference-frame storage is represented by `DecodedPictureBuffer`.
It stores `DpbEntry` values with a `PictureId`, display/order value, frame,
reference flag, and keyframe flag.

The helper intentionally does not define AV2, VVC, or future-codec reference
list policy. It provides validated storage, lookup, reference marking, and
simple oldest/non-reference eviction tools so codecs do not duplicate basic
buffer management.

## Filters

Filters are selected by generic stage specs:

```rust
use framefinery::parse_filter_pipeline_specs;

let filters = vec!["pattern=checker".to_string(), "identity".to_string()];
let pipeline = parse_filter_pipeline_specs(&filters, false)?;
assert_eq!(pipeline.source.unwrap().name, "pattern");
# Ok::<(), framefinery::MediaError>(())
```

The CLI should pass filter strings into `framefinery-core` and let the core
filter registry validate and build stages. The CLI should not know whether
`identity`, `pattern`, `crop`, or future filters have concrete Rust types.

## Muxing And Transport

Muxing is not yet part of the implemented API, but it should become an optional
post-encode stage:

```text
Source -> Filter -> Encoder -> Packetizer/Muxer -> Sink
```

The encoder emits chunks. Packetizers, muxers, filesystem writers, WebSocket
sinks, and browser callbacks consume chunks.

## Stability Tiers

- `Frame`, `FrameInfo`, `PixelFormat`: intended to stabilize early.
- `VideoEncoderConfig`, `VideoEncoderSession`, `EncodedVideoChunk`: v0 molten
  contract; expected to evolve before `0.1.0`.
- Codec manifests and settings: v0 molten, but should stay codec-neutral.
- `RawVideoFrameSource`, `FrameRef`, and `DecodedPictureBuffer`: early shared
  infrastructure; useful now, still allowed to change before `0.1.0`.
- Codec internals, entropy writers, prediction helpers, residual helpers, and
  trace helpers: private/experimental and not stable API.
- CLI command names and help pages: stabilizing toward `0.1.0`.

## Build Matrix

The default product build includes all codecs and filters. The project does not
currently prioritize a no-codec product build, but it does check codec-specific
builds so public CLI/API code does not accidentally depend on both codecs being
present:

```sh
make feature-matrix
```

Normal CI runs `make ci`. `make dead-code-audit` remains an explicit stale-helper
audit because hiding codec internals makes many experimental helpers private.

## Non-Goals For v0

- A promise of full AV2 or VVC decoder coverage.
- A promise that one codec's helper functions are common framework APIs.
- A browser playback compatibility profile.
- A complete muxing/container contract.
- A no-breakage guarantee before `0.1.0`.
