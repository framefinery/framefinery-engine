# FrameFinery v0 API Contract

This document is the first public API contract for FrameFinery. It is molten:
the names, shapes, and boundaries are expected to move before `0.1.0`, but the
purpose of writing them down now is to make the codebase converge around one
integration model instead of letting the CLI, native library API, and future
WASM package drift apart.

The `0.0.x` releases are not a stability promise. They are public checkpoints
for early adopters and validation users, and the Rust API may still make
breaking changes before `0.1.0`.

This is not the complete API reference. The reference is generated from Rust
`///` and `//!` documentation comments:

```sh
make api-docs
```

The generated facade entry point is `target/doc/framefinery/index.html`.
`make api-docs-strict` fails when exported API lacks Rustdoc documentation and
is part of the local release gate.

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
- `RawVideoFrameSourceReadAdapter`
- `VideoEncoderSession`
- `VideoEncoderBuilder`
- `EncodedVideoChunk`
- `FrameEncodeMetrics`
- `DecodedPictureBuffer`
- `FilterStageSpec`
- `FilterPipelineSpec`
- `FilterPipelineBuilder`
- `FilteredRawVideoFrameSource`
- `FrameSourceRawVideoAdapter`
- filter and encoder manifests

Decoders remain future architecture scope and are not exported as part of this
v0 release API.

`framefinery-codecs` owns encoder implementations and the generic encoder
registry:

- `ENCODERS`
- `find_encoder_manifest("av2")`
- `find_encoder_manifest("vvc")`
- `encoder("av2")`
- `encoder("vvc")`
- `create_encoder(config)`
- `encode_frame(config, frame)`
- `encode_source(&config, source, output, recon, metrics)`

Codec-specific modules remain implementation territory while the API cools.
Benchmark-only internals may be exposed under hidden feature-gated paths, but
normal applications should treat the registry as the codec boundary.

`framefinery` is the user-facing package and CLI facade. Its root Rust API
should prefer generic concepts from `framefinery-core` plus the generic encoder
registry from `framefinery-codecs`.

`framefinery-cli` maps command-line arguments and files onto the same generic
config and registry concepts. It should not contain AV2/VVC implementation
branches or filter implementation branches.

The facade also exposes the `ff` option inventory through
`cli_options()` and `cli_options_for_scope(...)`. The inventory describes the
same option names and aliases accepted by the parser, so external frontends can
render command help or build UI forms without copying CLI strings.

## Encoder Registry

`VideoEncoderConfig.codec` is the single codec selector for encoding. Callers
should not fetch one manifest and then drive it with a config for another codec.
The registry helpers enforce that boundary:

- `find_encoder_manifest(name)` returns discovery metadata for catalogs, help
  pages, capability checks, and setting validation UI.
- `encoder(name)` starts a fluent builder for a checked encoder session using
  one compiled codec.
- `create_encoder(config)` creates a frame-session encoder for the codec named
  by `config.codec`.
- `encode_frame(config, frame)` is the one-frame convenience path for callers
  that already own a `Frame`.
- `encode_source(&config, source, output, recon, frame_callback)` is the
  pull-based stream path for file, capture, and validation adapters. The frame
  callback is optional and reports timing, per-frame bytes, cumulative bytes,
  source/reconstruction samples, and PSNR when metric mode is selected.

`VideoEncoderManifest` is not the public object used to encode media. It
describes a compiled encoder and validates codec-neutral configuration, while
codec registration hooks remain implementation detail.

## Encoder Builder

Applications should prefer the fluent builder when they want a checked encoder
session without manually constructing every config field:

```rust
use framefinery::{encoder, FrameInfo, PixelFormat};

let input = FrameInfo::new(1280, 720, PixelFormat::Yuv420p8)?;
let mut encoder = encoder("vvc")?
    .input(input)
    .fps(30, 1)?
    .qp(24)?
    .metrics_only()
    .setting("predictive", true)?
    .setting("fast-search", "moderate")?
    .build()?;
# Ok::<(), framefinery::MediaError>(())
```

The builder is still codec-neutral: the codec is a string id, not a public
codec-specific encoder type. It validates the selected codec, input format,
rate-control mode, and extension settings before returning a session. The
lower-level `VideoEncoderConfig` remains available for adapters that need to
store, serialize, or mutate configuration before choosing how to drive an
encoder.

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

`VideoEncoderConfig` is the standard config object produced by the builder and
accepted by lower-level helpers. Manual construction is useful when callers
need to keep configs as data before creating an encoder:

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
- `frame_limit`: optional caller/source frame limit or progress hint;
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
    encode_source, encoder, FrameInfo, PixelFormat, RawVideoFrameSource, Result,
};

let input = FrameInfo::new(640, 360, PixelFormat::Yuv420p8)?;
let config = encoder("av2")?.input(input).into_config()?;
let mut emitted = false;
let mut source = |frame: &mut [u8]| -> Result<bool> {
    if emitted {
        return Ok(false);
    }
    frame.fill(0);
    emitted = true;
    Ok(true)
};
let mut bitstream = Vec::new();
let mut frames = 0usize;
let mut on_frame = |metrics: framefinery::VideoEncodeFrameMetrics<'_>| {
    frames = metrics.frame_idx + 1;
    eprintln!(
        "frame={} bytes={} total={} frame_ms={:.3}",
        frames,
        metrics.bitstream_bytes,
        metrics.total_bitstream_bytes,
        metrics.encode_elapsed.as_secs_f64() * 1000.0,
    );
};
encode_source(&config, &mut source, &mut bitstream, None, Some(&mut on_frame))?;
# let _ = &mut source as &mut dyn RawVideoFrameSource;
# Ok::<(), framefinery::MediaError>(())
```

`RawVideoFrameSource::read_frame` fills exactly one caller-provided frame buffer
and returns `Ok(false)` only at clean EOF before a frame. The encoder does not
need a total frame count: `VideoEncoderConfig::frame_limit` is an optional
upper bound for callers that want bounded file/test encodes or known progress.
File, Y4M, WebCodecs, screen-capture, and test-vector code should be adapters
that implement this callback shape.

The frame callback is called after each encoded frame. `bitstream_bytes` is the
frame payload size, `total_bitstream_bytes` is the encoded stream size observed
through that frame, and `encode_elapsed` is per-frame wall time after the source
frame has been read. `psnr` is populated when
`VideoEncoderConfig::reconstruction` is `MetricsOnly`; callers can also use the
borrowed `source` and `reconstruction` slices to compute custom metrics.

When a byte-reader bridge is still needed, `RawVideoFrameSourceReadAdapter`
adapts a raw-frame source to `std::io::Read` while buffering one frame at a
time. This is a compatibility adapter for current stream encoders and should
not be used to collect complete videos in memory.

## Encoder Sessions

The simplest one-frame encode helper is `encode_frame(config, frame)`. For
callers that already own filtered frames and want to feed a stream one frame at
a time, the lower-level session API is:

```rust
use framefinery::{
    create_encoder, encode_frame, Frame, Result, VideoEncodeOutput, VideoEncoderConfig,
};

fn drive_encoder(
    config: VideoEncoderConfig,
    frames: impl IntoIterator<Item = Frame>,
) -> Result<VideoEncodeOutput> {
    // For a single frame, prefer:
    // let output = encode_frame(config, frame)?;
    //
    // Sessions are useful when the caller naturally owns a sequence of frames.
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
- `MetricsOnly`: compute quality metrics, including callback PSNR, without
  returning reconstructed frames;
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
use framefinery::{FilterPipelineSpec, FrameInfo, PixelFormat};

let pipeline = FilterPipelineSpec::from_source_filter()
    .filter("pattern=checker")?
    .filter("identity")?
    .build()?;
assert_eq!(pipeline.source.unwrap().name, "pattern");
# Ok::<(), framefinery::MediaError>(())
```

The CLI should pass filter strings into `framefinery-core` and let the core
filter registry validate and build stages. The CLI should not know whether
`identity`, `pattern`, `crop`, or future filters have concrete Rust types.

Source filters can be built either as owned-frame sources or as raw-frame
callbacks. For encoder paths and future WASM capture pipelines, prefer the raw
form:

```rust
use framefinery::{FilterPipelineSpec, FrameInfo, PixelFormat};

let info = FrameInfo::new(3840, 2160, PixelFormat::Gbrp8)?;
let pipeline = FilterPipelineSpec::from_source_filter()
    .filter("pattern=color_blocks")?
    .build()?;
let mut source = pipeline
    .build_raw_video_source(info, 60)?
    .expect("source filter");
# let mut frame = vec![0; info.expected_len()];
# assert!(source.read_frame(&mut frame)?);
# Ok::<(), framefinery::MediaError>(())
```

Transform filters should be applied with `FilteredRawVideoFrameSource` when the
next stage is a source-driven encoder. That adapter keeps only the current input
frame and pending transformed frames, avoiding whole-stream buffering for long
files, generated patterns, or browser capture feeds.

## Muxing And Transport

Muxing is not yet part of the implemented API, but it should become an optional
post-encode stage:

```text
Source -> Filter -> Encoder -> Packetizer/Muxer -> Sink
```

The encoder emits chunks. Packetizers, muxers, filesystem writers, WebSocket
sinks, and browser callbacks consume chunks.

## Errors

Public fallible APIs return `Result<T, MediaError>`. Stable API boundaries
should prefer structured variants over string-only errors so callers can match
common failures:

- `UnsupportedCodec` for missing or mismatched encoder IDs;
- `UnsupportedPixelFormat` for codec/input-format rejection;
- `UnknownSetting`, `DuplicateSetting`, `InvalidSettingValue`, and
  `ConflictingSettings` for config validation;
- `ShortFrameRead` for EOF after a partial raw frame;
- `EncodeAfterFlush` and `FrameLimitExceeded` for session lifecycle errors.

Codec-internal experimental paths may still map implementation-specific
failures to string messages until those errors become stable API concepts.

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
