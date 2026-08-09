# Validation Notes

FrameFinery Engine should keep validation strict and reproducible.

Expected validation layers:

- unit tests for frame, packet, syntax, and reconstruction primitives;
- integration tests using deterministic generated vectors;
- reference-decoder checks for generated bitstreams when a reference is
  available;
- checksum comparison for lossless paths;
- PSNR and bitrate reporting for lossy paths;
- benchmark and throughput reporting for performance-sensitive stages.

Do not weaken pass criteria to hide incomplete codec support. Unsupported
syntax or geometry should fail visibly until implemented. Manifest `codecs`
gates may keep future vectors generateable while excluding them from a codec's
validation run, but an enabled row is expected to pass.

## Batch Fixtures

Portable generated-vector manifests live under:

```text
verification/test_vector_sets/
```

Use these targets for software-only CLI regression batches:

```sh
make test-vector-sets
make test-vectors TEST_VECTOR_SET=smoke
make validate-set CODEC=av2 VALIDATION_SET=smoke
make validate-set CODEC=vvc VALIDATION_SET=smoke
make validate-set CODEC=av2 VALIDATION_SET=smoke VALIDATION_SOURCE_FILTERS=1
make validate-set CODEC=av2 VALIDATION_SET=pipeline-smoke
make validate-set CODEC=av2 VALIDATION_SET=smoke VALIDATION_SETTINGS=lossless
make validate-release-aomctc
make release-performance-table
make regression
```

`scripts/generate_test_vectors.py` writes deterministic raw YUV inputs under
`verification/generated/test_vectors/`. `scripts/run_validation_set.py` encodes
each generated vector through `./ff encode`, writes the encoder's internal
reconstruction through `--recon`, checks that non-empty bitstream and
reconstruction outputs were produced, and prints a markdown summary with output
size, SHA-256 checksums, reason, and log path.
This validation path intentionally keeps using `--recon` because lossless and
reference-decoder checks compare reconstruction bytes. Encode-matrix benchmark
runs use `./ff encode --psnr` by default instead, so PSNR is computed directly
from the in-memory encoder reconstruction without materializing large raw
reconstruction streams. Set `ENCODE_MATRIX_WRITE_RECON=1` only when the matrix
also needs raw reconstruction files and `recon_sha256` entries.
For long source-file manifests, set `VALIDATION_DIRECT_SOURCE_FILES=1` or use
the release AOM CTC targets. This feeds Y4M/raw source rows directly into
`./ff` instead of writing large raw copies under `verification/generated/`.
Set `VALIDATION_CLEANUP_RECON=1` and `VALIDATION_CLEANUP_OUTPUT=1` to remove
successful reconstruction and encoded bitstream artifacts after checksums and
metrics have been collected. Failure artifacts are left in place for debugging.

Manifest `format` values use the same raw input names accepted by the CLI.
Planar YUV and gray bit depths from 8 through 16 are described in
[`raw-input-formats.md`](raw-input-formats.md), including exact higher-depth
codec support where available and the 8-bit fallback used by codec paths that
do not yet encode a higher depth natively.

Rows may set `lossless=true`. Validation passes that request to `ff encode`
with `--set lossless` and compares the encoder's internal reconstruction bytes
against the generated source bytes before optional reference-decoder checks.
When `VALIDATION_REFERENCE_MODE` is `auto` or `required` and a reference decoder
is used, the reference reconstruction must also match the internal
reconstruction. A lossless stream should only be enabled for a codec when both
checks are expected to pass.
Rows may also set `filters=<spec>|<spec>` to append executable transform
filters to the `ff encode` command. The committed `pipeline-smoke` set covers
`identity`, `crop`, and `scale`. Lossless source-byte comparison is only applied
when all filters are source-preserving; mutating filters still require
non-empty encoded output and internal reconstruction, plus optional reference
decoder agreement.
For `rgb24` lossless vectors, FrameFinery Engine writes packed RGB reconstruction bytes
while reference raw decoder output may be planar identity GBR. The validation
runner normalizes that reference output back to packed `rgb24` before comparing
checksums. For `gbrp8` vectors, the source and internal reconstruction are
already planar GBR, so validation compares the bytes directly.

Additional encoder settings can be passed to validation with
`VALIDATION_SETTINGS="key key=value"`. This is intended for codec experiments
that are not part of a manifest row yet, such as testing a fixed temporal GOP:

```sh
make validate-set CODEC=av2 \
  VALIDATION_SET=local-aomctc-b2-scc-predictive-sweep-3f \
  VALIDATION_SETTINGS=gop=30 \
  VALIDATION_REFERENCE_MODE=required
```

Explicit lossy AV2 and VVC smoke checks can invoke
`./ff encode ... --set qp=N` directly. `--set qp=<1..255>` is mutually exclusive with
`--set lossless`; lossy checks should compare bitstream size, reconstruction
PSNR, and reference-decoder agreement with the encoder reconstruction rather
than source-byte equality.

## Release AOM CTC Set

The committed `release-aomctc` manifest covers the existing AOM CTC A5 270p
and B2 screen-content Y4M streams under:

```text
/media/gabriel/storage/YUV/aomctc
```

It intentionally ignores `b1_syn.zip`; release validation does not decompress
anything. Each row points at an existing Y4M file and declares the local
metadata up front so repository discovery does not require the media directory.
Rows with dimensions that are not multiples of 8 can cover both AV2 and VVC
when the visible geometry is legal for the input format. Both codecs pad
internally to their current coded-canvas granularity and signal the visible crop
to the reference decoder.

Run the release crash/regression pass with:

```sh
make validate-release-aomctc
```

The default release pass uses `RELEASE_AOMCTC_FRAMES=1` so every A5/B2 stream
is touched quickly for AV2 and VVC, lossy and lossless. It reads source files
directly, writes reconstruction only long enough to validate it, removes
successful bitstreams and reconstructions, and prints filesystem usage before
and after the run. Override `RELEASE_AOMCTC_FRAMES=50` or set it to `130` for a
longer local release candidate pass.

For version-to-version performance tracking, run:

```sh
make release-performance-table
```

This wraps `scripts/release_performance_table.py`, which uses the same
`release-aomctc` manifest, reports FPS, bytes, bitrate-compatible bitstream
size, and PSNR through the encode matrix markdown/JSON output, and removes
successful encoded bitstreams by default. It defaults to 50 frames per stream
to exercise inter prediction without materializing raw reconstructions.

`scripts/generate_predictive_sweep.py` creates that local ignored manifest and
384 local Y4M crops: six AOM CTC B2 screen-content variants, 64 geometries from
8x8 through 64x64, and three frames per crop. Each crop currently repeats one
randomly selected source frame three times so AV2 show-existing-frame and
reference-buffer syntax are exercised across bit depth and subsampling
variants. A future companion set should use consecutive frames once block-level
inter prediction is implemented.

The `high-depth-smoke` set uses deterministic lower-bit canary samples so
truncation of 10-bit or 12-bit input is visible as a validation failure. VVC
4:2:0, 4:2:2, and 4:4:4 canaries are expected to pass with reference decoding;
AV2 10-bit 4:2:0, 4:2:2, and 4:4:4 canaries are expected to pass with
reference decoding. AV2 12-bit canaries remain gated until a reference-valid
12-bit profile path is available.

Reference tools are declared by JSON manifests under:

```text
verification/reference_codecs/
```

List and build declared references with:

```sh
make reference-list
make reference-setup
make reference-setup REFERENCE_CODEC=vvc
```

Reference source and build trees are local artifacts under
`verification/references/` and are not committed. `make validate-set` uses
`VALIDATION_REFERENCE_MODE=auto` by default. In `auto` mode, a built or
environment-configured decoder is used to decode the FrameFinery Engine bitstream and
the decoded output must match the internal reconstruction checksum. Missing
reference tools are reported as a skip. Use `VALIDATION_REFERENCE_MODE=required`
to make missing reference tools a failure, or `VALIDATION_REFERENCE_MODE=off`
for encode-only validation.

Reference encoder compression comparisons are intentionally separate from
decode-side validation. `make compare-compression` uses AVM/VTM encoders by
default to produce codec-native size baselines. These default reference outputs
are cached under `verification/generated/compression_compare/<codec>/<set>/`
and reused when the input, encoder path, preset, thread settings, and extra
reference arguments match. Set `COMPRESSION_REFRESH_REFERENCE=1` only when a
cached reference result should be regenerated.

AV2 does not currently have a fast dav1d-like production encoder separate from
AVM. For faster lossy iteration, `COMPRESSION_REFERENCE_BACKEND=rav1e` uses
the rav1e AV1 encoder as an explicit AV1 size/time baseline. rav1e does not
currently implement lossless encoding, so lossless manifests should keep the
default `COMPRESSION_REFERENCE_BACKEND=reference` path when a lossless AV1
baseline is required. The rav1e result is not an AV2 reference result, and it
is written under a backend-specific subdirectory such as
`verification/generated/compression_compare/av2/<set>/rav1e/` so it does not
clobber cached AVM results. Build it with:

```sh
make reference-setup REFERENCE_CODEC=rav1e
make compare-compression CODEC=av2 COMPRESSION_SET=smoke COMPRESSION_REFERENCE_BACKEND=rav1e
```

For realtime AV1 production-ceiling checks, `COMPRESSION_REFERENCE_BACKEND=ffmpeg-libaom`
uses the system `ffmpeg` binary with `libaom-av1`. The
`COMPRESSION_REFERENCE_PRESET=realtime-screen` preset enables realtime,
low-latency, screen-content-oriented libaom settings. This baseline may be
lossy even when the FrameFinery Engine row has `lossless=true`; use it to compare
speed and size against a realistic AV1 screen-share profile, not as a
stream-exact reference. Set `COMPRESSION_REFERENCE_PRESET=lossless` when an
AV1 lossless libaom baseline is needed instead. ffmpeg/libaom outputs are
cached under a backend-specific directory such as
`verification/generated/compression_compare/av2/<set>/ffmpeg-libaom/`.

Use `COMPRESSION_REFERENCE_BACKEND=libaom` to run the local `aomenc` binary
directly instead of going through ffmpeg. This is useful when comparing
FrameFinery Engine against libaom's native command-line behavior and checking whether
the ffmpeg wrapper is contributing materially different tuning. Build the
direct backend with:

```sh
make reference-setup REFERENCE_CODEC=libaom
make compare-compression CODEC=av2 \
  COMPRESSION_SET=smoke \
  COMPRESSION_REFERENCE_BACKEND=libaom \
  COMPRESSION_REFERENCE_PRESET=realtime-screen
```

For AV2 superblock bit accounting, compile the encoder with the gated
`av2-sb-bit-profile` feature through `AV2_SB_BITS=1`, then set
`FRAMEFINERY_AV2_SB_BITS` to a JSONL output path. The normal build does not
include this code unless the feature is enabled.

```sh
make build AV2_SB_BITS=1
FRAMEFINERY_AV2_SB_BITS=verification/generated/profiling/av2_sb_bits.jsonl \
  ./ff encode input.yuv --video 1920x1080:yuv420p8 --frames 1 \
  --encode av2:verification/generated/profiling/framefinery_sb_bits.obu --set qp=24
```

For AV2 lossy mode and TXB choice summaries, compile the separate gated
`av2-lossy-stats` feature through `AV2_LOSSY_STATS=1`, then set
`FRAMEFINERY_AV2_LOSSY_STATS=1` for the run. This keeps the normal encoder free
of the statistics counters and environment checks.

```sh
make build AV2_LOSSY_STATS=1
FRAMEFINERY_AV2_LOSSY_STATS=1 \
  ./ff encode input.yuv --video 1920x1080:yuv420p8 --frames 1 \
  --encode av2:verification/generated/profiling/framefinery_lossy_stats.obu \
  --set qp=24 2> verification/generated/profiling/framefinery_lossy_stats.log
```

For comparable direct-libaom superblock deltas, build the patched libaom
instrumentation in its separate build directory and set
`FRAMEFINERY_LIBAOM_SB_BITS` for the run. The direct libaom output is a total
arithmetic-coder bit delta per superblock; FrameFinery Engine additionally splits its
symbol bits into syntax categories such as partition, mode, and residual.

```sh
make reference-setup REFERENCE_CODEC=libaom LIBAOM_SB_BITS=1
FRAMEFINERY_LIBAOM_SB_BITS=verification/generated/profiling/libaom_sb_bits.jsonl \
  make compare-compression CODEC=av2 COMPRESSION_SET=smoke \
  COMPRESSION_REFERENCE_BACKEND=libaom \
  COMPRESSION_REFERENCE_PRESET=realtime-screen \
  LIBAOM_SB_BITS=1 COMPRESSION_REFRESH_REFERENCE=1
```

For comparable AVM superblock deltas, build the patched AVM instrumentation in
its separate build directory and set `FRAMEFINERY_AVM_SB_BITS` for the run. This
is useful for AV2-native tool guidance; AVM is not expected to be an fps
baseline.

```sh
make reference-setup REFERENCE_CODEC=av2 AVM_SB_BITS=1
FRAMEFINERY_AVM_SB_BITS=verification/generated/profiling/avm_sb_bits.jsonl \
  make compare-compression CODEC=av2 COMPRESSION_SET=smoke \
  COMPRESSION_REFERENCE_BACKEND=reference \
  COMPRESSION_REFERENCE_PRESET=fast \
  AVM_SB_BITS=1 COMPRESSION_REFRESH_REFERENCE=1
```

Summarize FrameFinery Engine, libaom, AVM, field-trace, and lossy-stats outputs with:

```sh
scripts/summarize_encoder_instrumentation.py --help
```

The comparative workflow and source-code audit pointers are documented in
[`av2-comparative-instrumentation.md`](av2-comparative-instrumentation.md).

For VVC frame-stage and per-CTU accounting, compile the gated `vvc-stats`
feature through `VVC_STATS=1`. `FRAMEFINERY_VVC_STATS` writes per-frame timing
and mode counts as JSONL, while `FRAMEFINERY_VVC_CTU_BITS` writes per-CTU CABAC
symbol estimates. The normal build does not compile these counters. Use
`FRAMEFINERY_VVC_CTU_BITS` selectively: CTU bit dumping is useful for syntax
analysis, but it adds work inside the CTU loop and should not be enabled for
clean hotspot timing.

```sh
make build VVC_STATS=1
FRAMEFINERY_VVC_STATS=verification/generated/profiling/vvc_stats.jsonl \
FRAMEFINERY_VVC_CTU_BITS=verification/generated/profiling/vvc_ctu_bits.jsonl \
  ./ff encode input.yuv --video 1920x1080:yuv420p8 --frames 1 \
  --encode vvc:verification/generated/profiling/framefinery_vvc.obu --set qp=24
```

For codec wall-time profiling, `make profile-hotspots` builds only the requested
gated stats features, runs first-frame encodes over the selected vectors, writes
one stats JSONL file per case, and emits an encode matrix plus hotspot summary.
Set `HOTSPOT_VISUALIZE=1` to generate a Rust code-browser heatmap from the same
wall-time profile:

```sh
make profile-hotspots HOTSPOT_CODECS=vvc
make profile-hotspots HOTSPOT_CODECS="av2 vvc" HOTSPOT_VISUALIZE=1
make profile-hotspots HOTSPOT_CODECS=vvc HOTSPOT_RUN=vvc-rd-audit
make summarize-hotspots HOTSPOT_CODECS=vvc HOTSPOT_RUN=vvc-rd-audit
```

The default output root is:

```text
verification/generated/profiling/hotspots/latest/
```

The hotspot summary ranks frame stages and timed counters by accumulated wall
time only. Candidate and mode-count counters may remain in the gated raw JSONL
for targeted analysis, but they are not part of the primary wall-time hotspot
view. Generated profiling outputs stay under `verification/generated/` and
should remain uncommitted.

For local source-file manifests backed by large Y4M inputs, set
`COMPRESSION_DIRECT_SOURCE_FILES=1` to feed the source path directly and use
the manifest `frames` value as the total-frame limiter instead of materializing
a frame-limited raw copy first:

```sh
make compare-compression CODEC=av2 \
  COMPRESSION_SET=local-aomctc-b2-scc-1080p-lossless-50f \
  COMPRESSION_REFERENCE_BACKEND=ffmpeg-libaom \
  COMPRESSION_REFERENCE_PRESET=realtime-screen \
  COMPRESSION_SETTINGS=gop=-1 \
  COMPRESSION_DIRECT_SOURCE_FILES=1
```

For AV2/VVC lossy QP comparisons, set `COMPRESSION_QP=<1..255>`. This forwards
`--set qp=<1..255>` to `./ff encode` and treats manifest `lossless=true`
rows as lossy FrameFinery Engine rows for that comparison run:

```sh
make compare-compression CODEC=av2 \
  COMPRESSION_SET=local-aomctc-b2-scc-1080p-lossless-50f \
  COMPRESSION_REFERENCE_BACKEND=ffmpeg-libaom \
  COMPRESSION_REFERENCE_PRESET=realtime-screen \
  COMPRESSION_QP=24 \
  COMPRESSION_DIRECT_SOURCE_FILES=1
```

For AV2 native reference comparisons, the Makefile defaults to
`COMPRESSION_REFERENCE_PRESET=fast`, which keeps `--cpu-used=9` and adds AVM
threading and low-latency speed options. Use
`COMPRESSION_REFERENCE_PRESET=default` to keep the legacy AVM argument set.
The fast preset can be tuned with:

```sh
make compare-compression CODEC=av2 COMPRESSION_REFERENCE_THREADS=8
make compare-compression CODEC=av2 COMPRESSION_AVM_TILE_COLUMNS=1
make compare-compression CODEC=av2 COMPRESSION_REFERENCE_PRESET=default
```

For broader market-style comparisons against production encoders and adjacent
codecs, keep the tool-specific runner under `external-drivers/` and call it
through `make benchmark-external-encoders`. The `external-drivers/` directory is
gitignored on purpose: local driver bundles may name installed tools, local
checkouts, command-line shapes, and tuning knobs without turning those choices
into committed project policy. The committed Make target only passes stable
FrameFinery paths, vector-set paths, report paths, and generic benchmark
controls.

```sh
make benchmark-external-driver-list
make benchmark-external-encoders
make benchmark-external-encoders \
  EXTERNAL_BENCHMARK_TARGET_PSNR=48:52 \
  EXTERNAL_BENCHMARK_AUTO_TUNE_PSNR=1
make benchmark-external-encoders EXTERNAL_BENCHMARK_MODE=lossless
make benchmark-external-encoders \
  EXTERNAL_BENCHMARK_DRIVERS="driver-id ..."
make benchmark-external-encoders \
  EXTERNAL_BENCHMARK_ARGS="--driver-owned-option value"
```

The ignored driver bundle owns the driver list, external tool inventory,
per-driver quality search, and the exact metric implementation. It should report
FPS, bytes/bitrate, and PSNR for lossy rows, and exactness, bytes/bitrate, and
FPS for lossless rows where a driver supports true lossless output. For lossy
comparisons, prefer setting `EXTERNAL_BENCHMARK_TARGET_PSNR=48:52` with
`EXTERNAL_BENCHMARK_AUTO_TUNE_PSNR=1`; if no attempted quality setting lands in
the target range, the local runner should keep the closest result and mark the
row as a target miss. Strict compatibility claims should still go through the
reference-decoder validation flow.

For vectors whose manifest pattern can be generated by the CLI, set
`VALIDATION_SOURCE_FILTERS=1`. The runner will skip input-file generation and
invoke the source filter directly, for example:

```sh
./ff encode --filter pattern=checker --video 16x24:yuv420p8 \
  --frames 1 --fps 30 --encode av2:out.obu
```

For local long-run memory smoke testing, prefer generated source filters and a
hard virtual-memory cap so a regression cannot exhaust the workstation:

```sh
(ulimit -v 262144; /usr/bin/time -v ./ff encode \
  --filter pattern=color_blocks \
  --video 3840x2160:gbrp8 \
  --fps 60 \
  --frames 3600 \
  --encode av2:/dev/null \
  --set qp=24)
```

`ulimit -v 262144` caps the process at 256 MiB. A healthy source-filter path
should stay bounded by a small number of frame buffers plus codec working state;
raise the cap only when the measured peak RSS justifies the larger working set.
