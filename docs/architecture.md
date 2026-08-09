# Architecture Notes

FrameFinery Engine is organized around a media pipeline:

```text
input -> decode -> filter -> encode -> output
```

The shared crate, `framefinery-api`, intentionally contains only stable
infrastructure:

- frame metadata and owned frame buffers;
- packet metadata and owned packet buffers;
- shared error types;
- source, decoder, filter, encoder, and sink traits;
- reusable source/filter implementations and their public filter manifest.

Codec internals should remain independent until common APIs are proven by real
implementations. AV2 and VVC may share frame buffers, metrics, validation
adapters, and byte/bitstream helpers, but should not be forced into one entropy
or block-tree abstraction early.

Local experimental AV2/VVC software encoders live in `framefinery-codecs`.
Those modules are allowed to keep codec-specific internal structures while they
evolve behind the software-facing API.
The user-facing package is `framefinery`; it provides the public facade crate
and the `ff` binary. Its default feature set enables AV2, VVC, and the current
product filter catalog so `cargo install framefinery` produces the normal CLI
build. `all-filters` remains available as a compatibility alias for the full
compiled filter catalog.

The first public video API contract is documented in [`api-v0.md`](api-v0.md).
That contract is deliberately codec-neutral: callers select encoders by codec
ID through a registry instead of constructing codec-named public encoder types.
Codec-specific helpers may be shared opportunistically inside
`framefinery-codecs`, but those helpers are not public API and do not define
what future codecs must use.

Long-running encodes should enter codecs through the source-driven API:
`RawVideoFrameSource` fills one raw frame buffer per pull, and the selected
codec consumes those frames until source EOF or an optional caller-provided
frame limit. Total frame counts are a CLI/source concern for bounded tests,
synthetic filters, and progress reporting; they are not an encoder
construction requirement. `VideoEncoderSession` remains useful for owned
`Frame` flows after filters and for future incremental implementations. Until
AV2/VVC are rewritten as truly incremental sessions, their session path is a
compatibility bridge and should not be the default for large files.

Optional codecs and filters should be selected at build time using Cargo
features or separate crates. The Makefile default enables the normal product
feature set so `./ff` is usable after `make build` without compiling
analysis-only instrumentation; override `CARGO_FEATURES` for narrower binaries.
Runtime pipeline construction can still choose which compiled stages to connect.
Filter features are individually toggleable. The normal product build enables
the executable `pattern`, `identity`, `crop`, and `scale` filters by default.
Future discovery scaffolds should stay out of that default until they have
implementations and validation. Instrumentation features should stay behind
explicit Makefile switches such as `AV2_SB_BITS=1` or `AV2_LOSSY_STATS=1`.

## CLI Contract

The `ff` CLI should remain easy to use for common work while staying explicit
enough for reproducible validation.

Initial command families:

- `ff --help [<codecs|filters [filter]|pixfmt|settings [setting]|presets>]`
  prints the general CLI help, a focused help topic, or one filter/setting spec
  contract.
- `ff codecs` lists known codec stages and the Cargo feature that compiles each
  one into the binary.
- `ff filters` lists known filter stages, their primary spec form, and the
  Cargo feature that compiles each one into the binary.
- `ff encode` is the path for one raw or Y4M input, optional input metadata,
  zero or more filters, one encoder, and one output:
  `ff encode input.yuv --video 1920x1080:yuv444p --filter identity --encode av2:output.obu --set lossless`.
  The encode endpoint must name a codec, using `--encode codec:path`.
  Input-only options belong after the input path; output-only options belong
  after `--encode codec:path`.

The first transform filters are `identity`, `crop`, and `scale`. `identity` is
the no-op pipeline check, `crop` updates geometry by selecting a rectangular
region, and `scale` performs deterministic nearest-neighbor resizing. Mutating
filters must report their output frame metadata before encoder construction so
the CLI and future frontends can configure encoders with post-filter geometry.

Raw video metadata should use the compact `WxH:pixfmt` form, for example
`--video 1920x1080:yuv444p`, when it cannot be inferred from the input
filename or Y4M header, or when it needs to be overridden. Explicit `--video`,
`--fps`, and `--frames` options take precedence over file metadata. File names
imply metadata with
`*_<WxH>[_<fps>][_<frames>f][_<pixfmt>].yuv`, for example
`clip_1920x1080_30_1f_yuv444p8.yuv`. If a `.yuv` filename has dimensions but
no pixel-format token, the CLI assumes `yuv420p8`. Y4M headers provide width,
height, frame rate, and planar YUV pixel format; when no explicit `--video` is
provided, Y4M header metadata takes precedence over filename metadata because
it describes the container payload. If a file input has no `--frames` value and
no filename frame-count metadata, the CLI infers the frame count from the raw
file size or by scanning Y4M frame markers and encodes whole frames until EOF.
If a user requests more frames than the file contains, the CLI clamps the
encode to the complete frames available instead of surfacing an EOF read error
from the codec model. Source filters must still provide `--frames` because
they generate frames rather than ending at a file EOF.

Raw planar YUV and gray inputs carry bit depth as checked numeric data rather
than as one enum variant per depth. The public API shape is documented in
[`raw-input-formats.md`](raw-input-formats.md): use constructors such as
`PixelFormat::yuv420(10)` and `PixelFormat::gray(16)`. The CLI currently uses
a shared frame-format converter for reversible packed RGB24 to planar GBR and
for higher-bit-depth inputs where the selected codec path only accepts the same
planar layout at 8-bit. The fallback converter does not change chroma sampling
or convert RGB to YUV. Codec paths that support an exact higher depth, such as
AV2 4:2:0/4:2:2/4:4:4 at 10 bits and VVC 4:2:0/4:2:2/4:4:4 through 12 bits,
receive the original raw format without conversion.
Lossless mode adds a stricter stream-exact requirement and never uses the
8-bit fallback converter. Current lossless validation is enabled for AV2
4:2:0/4:2:2/4:4:4 at 8/10 bits and VVC 4:2:0/4:2:2/4:4:4 at 8 through 12
bits. Planar `gbrp8` and legacy packed `rgb24` are validated as RGB-family
identity streams through the same shared repacking boundary; codec internals do
not convert RGB to YUV. AV2 12-bit inputs remain gated because the normal AVM
reference profiles validate 8-bit and 10-bit streams.

Prefer adding new stage-specific options behind repeated `--set key[=value]`
arguments until a setting is common enough to deserve a stable top-level flag.
Bare keys imply `true`, for example `--set lossless`. Shared settings such as
`lossless` are global and apply to any codec. `--set qp=<1..255>` is the
codec-specific lossy alternative to `--set lossless`; it currently drives AV2 and VVC
experimental planar residual quantizers and is rejected for codecs that do not
consume it.
`--recon <path>` remains the explicit raw reconstruction artifact option for
debugging and reference validation. `--psnr` is the explicit metrics option: it
computes per-frame PSNR from the encoder's internal reconstruction while the
frame is still in memory, so long benchmark streams can report quality without
writing reconstruction files.
Codec-specific setting catalogs carry controls such as the shared `gop`
temporal period and VVC's `fast-search` mode pruning and `profile` signalling.
Temporal prediction defaults to `gop=-1`, meaning one intra frame followed by
unbounded predictive frames; `gop=0` selects intra-only coding. VVC defaults to
`profile=auto`, which selects the lowest 4:4:4-capable profile for the input
bit depth so palette and related screen-content tools remain legal. Explicit
lower VVC profiles must gate unavailable tools before block-level mode
selection. Unknown options should still fail early instead of silently becoming
unused metadata.

AV2's QP path maps `--set qp=<1..255>` to a nonzero frame `base_qindex` and emits regular
transform-quantized 4x4 residuals for the current lossy intra path. The current
mapping treats `qp` as an encoder quality knob rather than the literal AV2
qindex; for example, `--set qp=24` signals `base_qindex=80`. Lossless mode remains
coded-lossless with `base_qindex=0`. Delta-q syntax is wired into the header
model but remains disabled until the encoder tracks and emits per-superblock
qindex adjustments.

VVC treats `--set qp=<1..255>` as a lossy quality request for the current residual path.
With `fast-search=lossless-speed`, the encoder may apply format-specific
signaled slice-QP offsets to keep the screen-content bitrate/PSNR tradeoff
closer across 8-bit 4:4:4 and high-depth inputs. These offsets currently spend
more bits on 4:4:4 and high-depth screen content to avoid the large PSNR loss
seen with one literal QP across all formats. Chroma QP is derived from the
signaled slice QP through the SPS chroma QP mapping table. Lossless VVC coding
keeps its QP-independent exact paths.
