# WASM Integration Notes

These notes capture the current direction for making FrameFinery useful as a
direct browser encoder package, especially for webcam and screen-share capture
flows. This is intentionally local planning material for now and is not part of
the committed release documentation.

## Product Direction

The WASM target should not wrap the existing CLI shape. The stronger product is
an encoder-native browser API:

```text
browser frame source -> optional filters -> encoder -> packetizer/muxer -> sink
```

The first target should be a low-drama JavaScript package that accepts frames
from browser APIs and streams encoded bytes to a receiving server. The package
should feel closer to WebCodecs than to `ffmpeg.wasm`: frame objects or typed
arrays go in, encoded chunks or muxed segments come out.

The intended beachhead is experimental AV2/VVC screen-content encoding in the
browser, where FrameFinery can offer codec paths that mainstream browser
encoders may not expose.

## API Shape

The Rust library API should become the center. CLI and WASM should both be
frontends over the same encoder/session model.

Proposed native layering:

```text
framefinery-core
  Frame, PixelFormat, timestamps, settings, metrics
  VideoEncoderSession trait
  EncodedVideoChunk
  muxer/packetizer traits

framefinery-codecs
  Av2Encoder
  VvcEncoder
  codec builders/options
  no filesystem requirement in the normal encode path

framefinery-cli
  filesystem readers/writers
  raw/Y4M handling
  --recon dumps
  --psnr
  validation and compression experiments

framefinery-wasm
  wasm-bindgen wrapper
  worker-first JS/TS API
  VideoFrame/ImageData/Canvas/RGBA/I420 adapters
  chunk or segment callbacks
```

The core encoder API should be session-oriented:

```rust
let mut encoder = VvcEncoder::new(config)?;
let chunks = encoder.encode_frame(frame)?;
let tail = encoder.flush()?;
```

The browser-facing API can then be simple:

```ts
const stream = await FrameFinery.streamScreen({
  codec: "vvc",
  container: "elementary",
  url: "wss://example.com/upload",
});
```

Advanced users should be able to bypass source capture and push frames
directly:

```ts
const encoder = await FrameFineryEncoder.create({
  codec: "av2",
  width,
  height,
  pixelFormat: "rgba",
  lossless: true,
});

encoder.encodeFrame(videoFrameOrBuffer);
await encoder.flush();
encoder.close();
```

## Encoded Chunk Contract

The encoder should emit encoded access units/chunks before any muxing or
transport step:

```rust
pub struct EncodedVideoChunk {
    pub data: Vec<u8>,
    pub codec: CodecId,
    pub frame_type: FrameType,
    pub pts: Timestamp,
    pub dts: Option<Timestamp>,
    pub duration: Option<Timestamp>,
    pub config: Option<Vec<u8>>,
    pub metrics: Option<FrameEncodeMetrics>,
}
```

This keeps file output, browser streaming, and validation from becoming
separate codec paths.

## Packetizer And Muxer Stage

Add an optional post-encode stage. "Muxer" may be too file-oriented as the only
name, so use a contract broad enough to include live transport packaging:

```text
Source -> Filter -> Encoder -> Packetizer/Muxer -> Sink
```

Potential implementations:

- elementary stream packetizer for raw VVC/AV2 access units;
- length-prefixed chunk stream for custom servers;
- fragmented container muxer later;
- complete file muxer later;
- transport sink adapters for WebSocket, WebTransport, WebRTC data channels, or
  HTTP upload.

The first WASM milestone should prefer elementary chunk streaming to a custom
server. Fragmented MP4/WebM can wait until the encoded chunk contract is stable.

## CLI And Offline Requirements

The CLI and native builds remain important. Offline compression experiments need
filesystem features, validation artifacts, internal reconstruction dumps, and
instrumentation.

Keep those features, but separate them from the codec core:

- always available core API: encode frames, return chunks, optional in-memory
  reconstruction and metrics;
- feature-gated diagnostics: detailed traces, per-CTU stats, wall-time
  instrumentation;
- CLI-only filesystem adapters: write recon files, generated reports,
  reference validation outputs;
- WASM-safe diagnostics: return JSON or typed metrics to JavaScript instead of
  writing files or reading environment variables.

The encoder should not know about paths, sockets, or containers. It should emit
chunks and metadata. The frontend chooses the sink.

## WASM-Specific Requirements

Before this is viable as a direct web package:

- make `framefinery-core` and the intended codec subset compile for
  `wasm32-unknown-unknown`;
- gate filesystem, environment-variable, stderr, and thread assumptions in
  codec instrumentation;
- provide worker-first packaging so encoding does not block the UI thread;
- expose TypeScript definitions;
- support `VideoFrame`, `ImageData`, canvas/OffscreenCanvas, RGBA, and planar
  YUV input adapters;
- define lifecycle calls: load, configure, encode, flush, reset, close,
  terminate;
- define typed errors for unsupported format, unsupported setting, invalid
  dimensions, memory pressure, cancellation, and encoder failure;
- publish feature variants eventually, such as AV2-only, VVC-only, and
  diagnostics-enabled builds.

## Why Not Just ffmpeg.wasm

`ffmpeg.wasm` is strong because it already provides packaging, workers, a
virtual filesystem, command execution, progress, logging, and broad media
coverage. FrameFinery should not compete by copying that model.

The direct FrameFinery appeal should be:

- no virtual filesystem required for the primary API;
- frame-in/chunk-out encoder session;
- first-class AV2/VVC experimental paths;
- screen-content-focused presets and metrics;
- safe Rust core shared by native CLI, library users, and WASM.

That makes the browser package a direct encoder SDK instead of a CLI emulator.

## Immediate Cleanup Path

1. Define the public video encoder session trait in `framefinery-core`.
2. Define stable config, settings, chunk, reconstruction, and metrics types.
3. Expose AV2/VVC encoder structs in `framefinery-codecs`.
4. Rework the CLI to consume those encoder structs through the shared API.
5. Add packetizer/muxer traits after the encoded chunk contract is proven.
6. Add a small `framefinery-wasm` crate only after native API shape settles.
7. Build a minimal browser demo that captures screen frames and streams
   elementary chunks to a local receiving server.
