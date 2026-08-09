# FrameFinery Engine Agent Guide

This file is for AI coding agents working in this repository. It captures the
initial project intent and boundaries agreed while splitting this work out of
the FrameFinery Engine hardware repository.

The scope of this file is the whole repository.

## Project Identity

FrameFinery Engine is the software-only media pipeline sibling of the original
FrameFinery Engine hardware project. The hardware repository remains the RTL research
and verification workspace.
This repository is intended to grow as a safe Rust media toolkit around the
pipeline model:

```text
input -> decode -> filter -> encode -> output
```

The initial implementation focus is experimental video encoding and validation,
starting from AV2 and VVC concepts already explored in FrameFinery Engine. The long-term
scope may include additional codecs, stream tools, filters, metrics, validation
adapters, and media-pipeline utilities.

This project should not be treated as the RTL hardware model. It is allowed to
optimize for software usability, safe Rust APIs, performance, and codec quality.

## Relationship To FrameFinery Engine

- `framefinery`: hardware/software co-design, RTL, synthesis, hardware-golden
  Rust models, and strict SW/RTL/reference validation.
- `framefinery-engine`: software-only safe Rust media pipeline and codec toolkit.

This repository may reuse ideas and carefully imported code from FrameFinery Engine,
but it should not inherit hardware-specific constraints unless they are useful
for validation or interoperability.

Avoid trying to keep this repository as a full mirror of FrameFinery Engine. Treat
FrameFinery Engine as a source of tested codec syntax/reconstruction ideas and
validation practices, not as a repo to merge wholesale.

## Licensing Intent

The intended license model is dual licensing:

- open-source distribution under the GNU Affero General Public License version
  3 or later (`AGPL-3.0-or-later`);
- commercial/private licensing under separate written agreement from the
  applicable copyright holder(s).

Project intent:

- open-source users may use, study, modify, and share the project under the
  open license;
- companies and individuals that need proprietary integration, private-source
  derivative distribution, hosted service deployment, or other terms different
  from AGPL-3.0-or-later should use a separate commercial license;
- paid support, custom development, integration, and optimization work should
  be allowed for the maintainer and for third parties under the applicable
  license terms.

Codec patent obligations are separate from source-code copyright licensing.
Documentation should make clear that users are responsible for evaluating codec
patent or deployment obligations for their use case and jurisdiction.

## Safety Posture

The implementation should use safe Rust.

Guidelines:

- Performance work should use safe Rust, algorithmic improvements,
  optimizer-friendly data layout, and compiler-supported optimizations.
- Prove optimized safe implementations are bit-exact against simple scalar
  implementations when replacing correctness-critical kernels.
- Prefer checked, saturating, or explicitly wrapping arithmetic where overflow
  behavior matters.
- Validate frame dimensions and buffer lengths before allocation or indexing.

The intended public claim is not that bugs are impossible; it is that the
default implementation avoids memory-unsafe Rust and uses validation to catch
codec correctness errors.

## Architecture Direction

Make the pipeline concept first-class. Useful abstractions may include:

- sources that produce packets or frames;
- decoders that convert packets into frames;
- filters that transform frames;
- encoders that convert frames into packets/bitstreams;
- sinks that write packets or streams;
- metrics stages for bitrate, PSNR, checksums, and validation.

Keep codec internals independent at first. Do not force AV2 and VVC into a
premature shared abstraction for entropy, block trees, prediction, transform, or
mode decisions. Share stable, boring infrastructure first:

- frame and plane buffers;
- pixel formats and color metadata;
- raw YUV/PNG I/O helpers;
- byte/bitstream output helpers;
- metrics and checksums;
- reference-decoder validation adapters;
- command-line plumbing and benchmarks.

## Profiles And Builds

If profile-specific behavior is needed, prefer build-time selection for
independent products rather than runtime mode flags. The user-facing software
encoder should not include hardware-model-only decision logic unless there is a
specific reason.

Codec and filter availability may also be selected at build time. Prefer
separate Cargo features or crates for optional codecs and filters so binaries
can be built with only the media stages they need.

Avoid scattering profile checks through codec syntax and reconstruction code.
If profiles are introduced, resolve them at construction or crate feature
boundaries and keep shared syntax/reconstruction code profile-neutral.

Potential future products:

- a user-facing safe software encoder build;
- optional experimental builds;
- optional hardware-compatibility import tools, kept separate from normal user
  binaries.

## Validation Principles

Validation should remain strict and reproducible:

- reference decoders validate generated bitstreams when available;
- lossless paths must reconstruct exactly;
- lossy paths should report PSNR and bitrate;
- bitstream sizes, checksums, and metrics should be recorded for regressions;
- test vectors should be deterministic and regenerable;
- do not weaken validation criteria to make incomplete work appear correct.

FrameFinery Engine hardware validation can inspire the workflow, but this repository is
free to add software-specific benchmarks and quality tests.

## Development Boundaries

Avoid early feature creep into a general-purpose media suite. The broad vision
is a media pipeline toolkit, but the early milestones should stay narrow:

1. establish a clean safe Rust project structure;
2. import or reimplement minimal frame/pixel/bitstream primitives;
3. bring up one codec path with reference-decoder validation;
4. add metrics and reproducible tests;
5. expand codecs and filters incrementally.

When adding features, keep APIs small and practical. Prefer code that can be
validated now over abstractions for codecs or container formats that do not
exist yet.

## Agent Workflow

- Read this file before making changes.
- Read `README.md`, `docs/architecture.md`, and `docs/validation.md` before
  changing code or project structure. Also read any focused docs relevant to
  the files being changed.
- Check `git status --short` before edits.
- Keep commits small and scoped.
- Do not copy large chunks from FrameFinery Engine without preserving attribution and
  checking license compatibility.
- Prefer `rg` for search.
- Use `cargo fmt`, `cargo test`, and targeted validation once a Rust crate
  exists.
- Keep generated artifacts out of version control unless they are intentionally
  committed fixtures.

## Current Build And CLI Contract

The main developer binary is `./ff`. `make build` should build a release binary
with all workspace codec/filter features by default, then copy it to the repo
root as `./ff`. Use `make debug` for debug artifacts.

The current primary command shape is:

```sh
./ff encode [<input.yuv>] [input-options] [--filter <spec>] \
  --encode <codec:output> [output-options]
```

Important current CLI behavior:

- `--encode` must name the codec and output path together, e.g.
  `--encode av2:out.obu`.
- `--recon <path>` writes the encoder's internal reconstructed raw frame
  stream and is used by validation.
- `--psnr` prints per-frame PSNR from the encoder's internal reconstruction
  without writing the raw reconstruction stream.
- Raw YUV dimensions, frame rate, frame count, and pixel format may be inferred
  from filenames such as `clip_640x360_30_10f_yuv444p8.yuv`.
- If a bare `.yuv` filename has dimensions but no pixel format suffix, the
  current default is `yuv420p8`.
- File inputs may omit `--frames`; encoding stops at EOF. Source filters do not
  have EOF and must specify a frame count.
- `--set lossless` is a global boolean setting. Keep new settings global unless
  there is a clear codec-specific reason.

Compressed input decode is not implemented yet. Avoid designing CLI flows that
pretend compressed decode exists until a real decoder stage is present.

## Reference Tools

External reference tools are declared by manifests under:

```text
verification/reference_codecs/
```

Local reference source and build trees live under `verification/references/`
and must remain uncommitted. Use:

```sh
make reference-list
make reference-setup
make reference-setup REFERENCE_CODEC=av2
make reference-setup REFERENCE_CODEC=vvc
```

The validation runner uses reference decoders only for decode-side validation:
FrameFinery Engine encodes the stream, the reference decoder decodes it, and the
reference reconstruction must match the internal reconstruction checksum. Do not
use AVM/VTM reference encoders as bitrate baselines in the normal validation
path.

`VALIDATION_REFERENCE_MODE` controls reference handling:

- `auto`: use a reference decoder if it is already available, otherwise report
  a skip.
- `required`: fail if the reference decoder is missing or reconstruction
  checksums differ.
- `off`: skip reference decoding.

Prefer `required` when validating a release or claiming reference compatibility.

Ad hoc decode/playback notes for generated WASM captures:

- WASM screen-capture streams are written under
  `verification/generated/wasm_screen_capture/`.
- To decode an AV2 `.obu` with the local reference decoder, use
  `python3 scripts/reference_tools.py decode --codec av2 --bitstream <in.obu>
  --output <out.yuv> --no-build`. This expects the AVM decoder at
  `verification/references/av2/avm/build/avmdec` if references have already
  been set up.
- The WASM screen-capture demo currently feeds browser frames as `gbrp8`.
  Reference-decoded raw output for these captures should normally be played as
  `gbrp`, not `yuv420p`. AVM `--i420` can fail for these RGB-family streams.
- The browser demo should center-crop the captured screen to the configured
  encode dimensions without scaling. Scaling browser screen captures into the
  encode size adds resampling noise that dominates screen-content experiments.
- The browser demo uses `/stream`, a dependency-free WebSocket receiver in
  `examples/wasm-screen-capture/server.py`. It sends the WASM ABI's
  per-`encode_frame` output (`ff_wasm_last_output_ptr/len`) as binary messages;
  the server writes a `.part` file as bytes arrive and renames it after the
  browser sends the final frame count.
- If no sidecar metadata exists, infer raw playback dimensions from decoded
  size: for `gbrp8`, bytes = frames * width * height * 3. The frame count is
  often in the filename, e.g. `30f`.
- Example playback shape:
  `ffplay -f rawvideo -pixel_format gbrp -video_size <WxH> -framerate <fps>
  -autoexit <decoded.yuv>`.

VVC/VTM debugging notes:

- The local VTM tree is expected at `verification/references/vvc/vtm/`.
- VTM profile capabilities are defined in
  `source/Lib/CommonLib/ProfileTierLevel.cpp`; decoder-side profile constraint
  checks are in `source/Lib/DecoderLib/VLCReader.cpp`.
- When comparing SPS/PPS syntax against VTM, check the matching parser
  conditionals before changing the writer. For example, VTM reads
  `sps_gpm_enabled_flag` only when the signalled max merge candidate count is
  at least 2, and picture-header RPL/QP fields only when the PPS says those
  fields are carried in the picture header.

## Useful Commands

Common local quality gates:

```sh
make check-tools
make release-check
make test
make build
```

Validation examples:

```sh
make test-vectors TEST_VECTOR_SET=smoke
make validate-set CODEC=av2 VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=auto
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=auto
make validate-set CODEC=av2 VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
```

For source-filter-generated vectors, add:

```sh
VALIDATION_SOURCE_FILTERS=1
```
