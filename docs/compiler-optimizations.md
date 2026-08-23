# Compiler And Rust Optimization Notes

This document records practical ways to make FrameFinery Engine faster while preserving
the same public behavior, bitstream validity, and reconstruction rules. Treat
every item here as something to measure, not as a blanket rule. Codec changes
must still pass strict validation; an optimization that changes reconstruction,
syntax validity, or lossy quality guardrails is a codec change, not a compiler
cleanup.

The local toolchain observed while writing this note was:

```text
rustc 1.95.0 (59807616e 2026-04-14)
cargo 1.95.0 (f2d3ce0bd 2026-03-21)
```

The repo already has important guardrails:

- `make build` builds the release CLI as `./ff`.
- `make release-check` runs the normal quality gate.
- `make validate-set` and `make compare-compression` provide codec-level
  validation and scoreboards.
- Analysis hooks such as `AV2_SB_BITS=1` and `AV2_LOSSY_STATS=1` are feature
  gated and should stay out of normal product builds.
- The workspace currently has `unsafe_code = "forbid"`, so unsafe SIMD and
  unchecked indexing are not available without an explicit policy change.

## Optimization Order

Use this order for most performance work:

1. Measure a representative workload.
2. Identify the hot function, loop, allocation, or memory path.
3. Add or update a focused benchmark or validation vector.
4. Refactor in safe Rust first.
5. Rebuild with controlled compiler flags.
6. Compare speed, bitstream size, PSNR where relevant, and reconstruction
   checksums.
7. Keep the change only if it improves the measured target without weakening
   validation.

For AV2 and VVC work, prefer workloads that match the current project goals:
small smoke vectors for correctness, high-depth vectors for bit-depth safety,
and local screen-content sets for realistic encoder pressure.

## Baseline Commands

Normal quality gate:

```sh
make release-check
```

Normal release build:

```sh
make build
```

Release build with all normal product features is already the Makefile default:

```sh
make build CARGO_FEATURES=all
```

Targeted validation:

```sh
make validate-set CODEC=av2 VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=auto
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=auto
make validate-set CODEC=av2 VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=auto
```

Use `VALIDATION_REFERENCE_MODE=required` when claiming reference compatibility.

## Cargo Profile Levers

Cargo profile settings live at the workspace root. Dependency profile settings
inside dependency manifests are ignored by Cargo, so workspace-level profile
experiments belong in the root `Cargo.toml`.

The default `release` profile already uses `opt-level = 3`,
`debug-assertions = false`, `overflow-checks = false`, `incremental = false`,
and `codegen-units = 16`. Important profile knobs:

- `opt-level = 3`: default release speed optimization.
- `lto = "thin"`: cross-crate optimization with lower cost than fat LTO.
- `lto = "fat"` or `lto = true`: stronger whole-program LTO, slower to link.
- `codegen-units = 1`: usually better final code, slower compilation.
- `panic = "abort"`: smaller binaries and simpler code paths for CLI products.
- `debug = "line-tables-only"` or `debug = 1`: useful for profiling symbols.
- `strip = "symbols"`: smaller distribution binary, not useful during profiling.
- `incremental = false`: recommended for release-style optimized builds.

The repository includes a conservative experimental ThinLTO profile:

```toml
[profile.optimized]
inherits = "release"
lto = "thin"
codegen-units = 1
panic = "abort"
debug = "line-tables-only"
strip = "none"
```

Build it with:

```sh
make build PROFILE=optimized
```

This writes `./ff-optimized`, leaving the normal `./ff` release baseline in
place for side-by-side measurements. Do not strip symbols until after profiling
and debugging are done.

## Direct rustc Flags

For one-off experiments, prefer `RUSTFLAGS` or `cargo rustc` before editing
profiles.

Local-machine benchmark build:

```sh
RUSTFLAGS="-Ctarget-cpu=native" make build
```

`target-cpu=native` lets rustc tune for the current CPU. Use it for local
benchmarks and deployment to a known machine class. Do not use it for generic
release artifacts unless the deployment CPU is controlled.

Thin LTO and one codegen unit without editing `Cargo.toml`:

```sh
RUSTFLAGS="-Clto=thin -Ccodegen-units=1" make build
```

Profiling-friendly release build:

```sh
RUSTFLAGS="-Cdebuginfo=1 -Cforce-frame-pointers=yes -Csymbol-mangling-version=v0" \
  make build
```

Frame pointers make native profilers easier to use. They can reduce peak
performance slightly, so do not measure final speed with frame pointers unless
the production build will also use them.

Useful discovery commands:

```sh
rustc -C help
rustc --print target-cpus
rustc --print target-features
```

## LTO Strategy

Try LTO only after a baseline profile exists.

Suggested sequence:

1. `make build`
2. `make build PROFILE=optimized`
3. `RUSTFLAGS="-Clto=fat -Ccodegen-units=1" make build`

Measure all three on the same workload. Thin LTO is often the best first
release setting because it exposes cross-crate optimization without the full
link-time cost of fat LTO. Fat LTO is worth testing for final release binaries,
but it is not automatically faster.

## Profile-Guided Optimization

PGO is the compiler equivalent of telling LLVM which paths are hot. The rustc
workflow is:

```text
instrumented build
-> run representative workloads
-> merge .profraw files into .profdata
-> rebuild using profile data
```

Install LLVM profiling tools:

```sh
rustup component add llvm-tools-preview
```

The repository includes an opt-in helper that builds an instrumented encoder,
runs an encode matrix over a named vector set, merges the raw profiles, then
builds `./ff-pgo` with `-Cprofile-use`:

```sh
make build-pgo PGO_SET=smoke PGO_FRAMES=1
```

Use `PGO_SET`, `PGO_CODECS`, `PGO_MODES`, and `PGO_FRAMES` to choose the
training workload. For local source-file manifests, set
`PGO_DIRECT_SOURCE_FILES=1`.

Use a mix of representative inputs for the instrumented run. For FrameFinery Engine,
that should include:

- lossless AV2 4:2:0, 4:2:2, 4:4:4;
- AV2 lossy QP runs;
- VVC smoke runs;
- high-bit-depth paths;
- local screen-content clips when optimizing screen-share behavior.

PGO can make code worse if the profile does not match production usage. Keep
the profile set versioned or documented when using PGO for release builds.

## LLVM Optimization Remarks

There is no perfect "warn if slow" switch, but LLVM can explain many successful
and missed optimizations.

Start with rustc optimization remarks:

```sh
RUSTFLAGS="-Cremark=all" cargo build --release -p framefinery \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale"
```

This is noisy. For vectorization, inspect one crate at a time:

```sh
make llvm-vector-remarks
```

The target uses a separate Cargo target directory and emits remarks for
`loop-vectorize` and `slp-vectorizer` by default. Override
`LLVM_REMARK_PASSES`, `LLVM_REMARK_CRATE`, or `LLVM_REMARK_FEATURES` for a
narrower pass or crate.

Useful pass filters:

- `loop-vectorize`: loop SIMD vectorizer.
- `slp-vectorizer`: straight-line scalar-to-vector packing.
- `inline`: inlining decisions.
- `unroll`: loop unrolling.

Read missed remarks as clues, not final truth. A loop may fail vectorization
because of bounds checks, unknown aliasing, calls, branches, type choices, or
because LLVM's cost model decided scalar code was better.

## Clippy And Lints

Clippy's `perf` group is the first warning layer to use:

```sh
make clippy-perf
```

The Makefile target uses the normal product feature set and suppresses broader
default Clippy groups so the output stays focused on performance lints. The raw
equivalent is:

```sh
cargo clippy --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale" \
  -- -A clippy::all -W clippy::perf
```

For CI-style cleanup:

```sh
cargo clippy --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale" \
  -- -D warnings -W clippy::perf
```

Potentially useful cherry-picked lints:

- `clippy::perf`: low-risk performance suggestions.
- `clippy::large_enum_variant`: catches oversized enum variants that cause
  copies and cache pressure.
- `clippy::large_stack_arrays`: catches large stack allocations.
- `clippy::needless_collect`: catches temporary collections.
- `clippy::redundant_clone`: catches avoidable clones.
- `clippy::unnecessary_to_owned`: catches avoidable allocation.
- `clippy::unwrap_used` and `clippy::expect_used`: useful in non-test hot code,
  but noisy across existing tests.

Do not enable `clippy::restriction` as a group. It intentionally contains
policy lints that can contradict each other and should be selected case by
case.

Rust lints can also be configured in `[workspace.lints.rust]`. The existing
`unsafe_code = "forbid"` setting should stay unless a specific optimized kernel
is approved as an explicit exception.

## Profiling Runtime Hotspots

Use the existing gprof helper for the current AV2 lossless first-frame case:

```sh
make profile-av2-i-lossless
```

For Linux `perf`, build with line debug info and frame pointers:

```sh
RUSTFLAGS="-Cdebuginfo=1 -Cforce-frame-pointers=yes -Csymbol-mangling-version=v0" \
  make build

perf record -F 99 --call-graph fp -- ./ff encode input_640x360_30_1f_yuv444p8.yuv \
  --encode av2:verification/generated/profiling/framefinery-profile.obu \
  --recon verification/generated/profiling/framefinery-profile.yuv
perf report
```

Use `--call-graph dwarf` instead of `fp` when frame pointers are unavailable,
but expect more overhead.

Good profiling targets:

- wall time per encoded frame;
- cycles in intra prediction, residual, transform, quantization, entropy, and
  tile payload writing;
- allocation counts and bytes;
- branch misses in mode decisions;
- cache misses in frame/plane access;
- time spent formatting traces or errors in hot paths.

## Benchmarking

Add focused benchmarks before doing risky refactors. `cargo bench` uses the
`bench` profile and supports custom benchmark harnesses. Stable Rust projects
commonly use Criterion for statistically stronger microbenchmarks.

Suggested benchmark groups:

- planar sample read/write and bit-depth conversion;
- AV2 4x4 transform, quantization, reconstruction;
- SAD/SATD or prediction-error kernels;
- palette color counting and palette selection;
- IntraBC or motion-search candidate scoring;
- entropy token emission and tile payload assembly;
- frame copy, plane split, and RGB/GBR conversion.

Current AV2 microbenchmarks cover palette selection over deterministic 4:4:4
screen-content frames and 4x4 transform/quant/dequant/inverse-transform
roundtrips. Current VVC microbenchmarks cover the shared residual CTU path over
deterministic 4:2:0 and 4:4:4 screen-content frames in lossless and lossy mode:

```sh
make bench-av2-micro
make bench-vvc-micro
```

The harness uses the `bench-internals` feature to expose numeric checksums for
the measured internals without adding those helpers to normal product builds.

Benchmark rules:

- Use fixed deterministic inputs.
- Report throughput in samples, pixels, blocks, or frames per second.
- Keep benchmark data small enough for microbenchmarks, then validate with full
  encode runs.
- Never optimize solely for synthetic data if it hurts real validation sets.

## Safe Rust Refactoring Patterns

Most worthwhile Rust speedups come from making invariants visible to LLVM.

### Validate Once, Iterate Simply

Move dimension and buffer length checks to construction or setup code. Hot
loops should operate on already validated slices and fixed spans.

Prefer:

```rust
let row_start = y * stride;
let row = &src[row_start..row_start + width];
let out = &mut dst[row_start..row_start + width];
for (d, s) in out.iter_mut().zip(row) {
    *d = *s;
}
```

over repeatedly checking computed indexes inside the inner loop.

### Use Row Slices And chunks_exact

For image kernels, row slices and `chunks_exact` often remove bounds checks and
make vectorization easier:

```rust
for (src_row, dst_row) in src
    .chunks_exact(src_stride)
    .zip(dst.chunks_exact_mut(dst_stride))
    .take(height)
{
    let src_row = &src_row[..width];
    let dst_row = &mut dst_row[..width];
    for (d, &s) in dst_row.iter_mut().zip(src_row) {
        *d = s;
    }
}
```

This also exposes a predictable contiguous memory pattern to LLVM.

### Prove Non-Aliasing With Split Slices

When two mutable regions come from one buffer, use `split_at_mut` or helper
layout methods to prove they do not overlap:

```rust
let (y_plane, chroma) = frame.split_at_mut(y_len);
let (u_plane, v_plane) = chroma.split_at_mut(chroma_len);
```

This is better than carrying indexes into one large mutable buffer.

### Prefer Fixed Arrays For Small Blocks

Codec kernels often operate on 4x4, 8x8, or 16x16 blocks. Prefer arrays for
small fixed-size working sets:

```rust
let mut coeffs = [0i32; 16];
```

Arrays let LLVM see the exact size, unroll small loops, keep data in registers,
and avoid heap allocation.

### Reuse Scratch Buffers

Avoid allocating per block, per transform, or per symbol group. Put scratch
storage in tile/frame state and clear it between uses:

```rust
scratch.clear();
scratch.extend_from_slice(block_samples);
```

Use `Vec::with_capacity` when the final size is known or tightly bounded.

### Avoid Temporary collect In Hot Paths

Iterator chains are often fine, but `collect::<Vec<_>>()` inside hot loops is a
red flag unless the allocation is essential. Prefer direct iteration, stack
arrays, or reusable scratch.

### Keep Error Formatting Cold

Error strings and `format!` are fine in CLI parsing and setup. In hot codec
paths, validate upfront and keep the inner loop free of formatting. When an
error helper is truly cold, consider:

```rust
#[cold]
fn invalid_geometry_message(width: usize, height: usize) -> String {
    format!("invalid geometry {width}x{height}")
}
```

### Use debug_assert For Proven Invariants

If setup code validates an invariant, use `debug_assert!` in hot code when the
check is only for developer mistakes. `assert!` remains in release builds and
can cost branches or panic paths.

Do not replace validation with `debug_assert!` at public boundaries. Public
input checks must still run in release builds.

### Be Explicit About Overflow Semantics

Release Rust disables overflow checks by default, while debug Rust enables
them. Codec code should not depend on that difference. Use:

- `checked_*` for size and allocation math;
- `saturating_*` for pixel clamp semantics;
- `wrapping_*` only where codec syntax or modular arithmetic requires it;
- wider intermediates for transforms and error scores.

This keeps debug and release behavior aligned.

### Specialize Carefully

When format choice is known after validation, consider separate internal paths
for important cases:

- 8-bit 4:2:0;
- 10-bit 4:2:0;
- 8-bit 4:4:4 RGB/GBR screen content;
- lossless versus lossy residual paths.

Specialization can remove runtime branches and make loops simpler. Avoid
exploding the public API or duplicating whole codecs. Specialize small kernels
and dispatch at construction or frame/tile setup boundaries.

### Avoid Dynamic Dispatch In Inner Loops

Pipeline traits are appropriate at stage boundaries. Inside codec kernels,
prefer enums, generics, direct function calls, or function selection before the
loop. Trait objects and function pointers in per-pixel or per-block loops can
block inlining and vectorization.

### Keep Branches Predictable

Mode decision code naturally has branches. In pixel kernels, prefer moving rare
cases out of the inner loop. For example, handle edges, padding, and partial
blocks outside the full-block fast path when that keeps the main loop straight.

### Use Tables For Repeated Codec Constants

Scan orders, quant tables, CDF defaults, block layouts, and fixed syntax maps
should be static arrays or compact structs. Rebuilding them per block or per
frame wastes time and cache.

## SIMD Strategy

There are three levels of SIMD in Rust:

1. Auto-vectorization from ordinary optimized Rust.
2. Portable SIMD through `std::simd`, currently nightly-only experimental.
3. Architecture intrinsics through `core::arch`.

For this repository, the current default should be:

```text
safe scalar kernel
-> benchmark
-> make loop/vectorization-friendly
-> inspect LLVM remarks or assembly
-> consider SIMD only for proven hotspots
```

Auto-vectorization works best when loops have:

- contiguous slices;
- simple arithmetic;
- no calls inside the inner loop;
- no complicated branches;
- clear non-aliasing;
- fixed or easily analyzable trip counts.

Architecture intrinsics usually require `unsafe` and CPU feature dispatch. That
conflicts with the current workspace `unsafe_code = "forbid"` policy. If SIMD
intrinsics become necessary, isolate them in tiny modules with scalar reference
tests, runtime feature detection, and a clear safety rationale before changing
the lint policy.

For portable binaries, do not compile the whole program with `-Ctarget-feature`
such as `+avx2` unless every deployment CPU supports it. Prefer runtime
dispatch for CPU-specific kernels.

## Post-Link Optimization

LLVM BOLT can optimize an already linked ELF binary using a sampled execution
profile. It is an advanced release step, not a normal development loop.

Use it only after:

- normal profiling has identified stable hot paths;
- LTO and PGO have been tested;
- the release workload is representative;
- the binary is built with enough symbols/relocations for BOLT.

This is probably later-stage work for FrameFinery Engine. It may matter once the CLI
has large hot code and stable production workloads.

## Allocator And Memory Behavior

FrameFinery Engine currently has no external allocator dependency. Before changing the
global allocator, reduce allocations in hot code:

- preallocate output buffers with known capacities;
- reuse per-tile scratch;
- avoid per-block `Vec`;
- avoid cloning frame planes unless ownership truly requires it;
- stream frame input/output instead of materializing larger-than-needed data;
- keep trace JSON and instrumentation behind feature/runtime gates.

If allocation still dominates after refactoring, compare allocators only with a
representative encode workload. An allocator swap can improve one workload and
hurt another.

## Parallelism

Compiler flags will not create codec-level parallelism. Add parallelism where
the codec structure supports deterministic independent work:

- tiles;
- rows of independent prediction/error scoring;
- frame-level lookahead when future encoder design supports it;
- independent validation/compression jobs.

Rules for parallel codec work:

- preserve deterministic bitstream ordering;
- avoid sharing mutable state in inner loops;
- aggregate per-thread outputs in a fixed order;
- keep small clips single-threaded if thread overhead dominates;
- measure both speed and bitstream impact.

## FrameFinery Engine-Specific Hotspot Candidates

Based on the current repository layout, likely optimization targets are:

- `crates/framefinery-codecs/src/av2/lossy420.rs`: prediction, transform,
  quantization, residual scoring, and TXB selection.
- `crates/framefinery-codecs/src/av2/palette_prediction.rs`: palette color
  counting, sorting, dynamic-programming palette choice.
- `crates/framefinery-codecs/src/av2/palette_444.rs`: screen-content palette
  path and block traversal.
- `crates/framefinery-codecs/src/av2/tile.rs` and `tile_payload.rs`: tile
  assembly and entropy payload handling.
- `crates/framefinery-codecs/src/av2/motion.rs` and `ibc.rs`: future motion and
  IntraBC search kernels.
- `crates/framefinery-codecs/src/vvc/residual/`: transform, quantization,
  prediction, and reconstruction.
- `crates/framefinery-api/src/frame.rs` and
  `crates/framefinery-codecs/src/picture.rs`: frame length, bit-depth
  conversion, and planar sample access.

Use profiling before changing any of these. Some files are large because they
contain tests or syntax scaffolding rather than runtime hotspots.

## Validation Requirements For Optimized Kernels

Every optimized correctness-critical kernel should have:

- a simple scalar reference implementation;
- tests over edge values, bit depths, and odd/even dimensions as applicable;
- deterministic random or generated vectors when useful;
- exact reconstruction comparison for lossless;
- PSNR and bitrate comparison for lossy;
- reference decoder checks when a reference decoder is available.

Suggested validation after a codec kernel change:

```sh
make test
make validate-set CODEC=av2 VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=auto
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=auto
make validate-set CODEC=av2 VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=auto
```

For release claims:

```sh
make validate-set CODEC=av2 VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
```

## Things To Avoid

- Do not use `target-cpu=native` as a portable release setting.
- Do not rely on release overflow wrapping unless the operation uses
  `wrapping_*` explicitly.
- Do not turn public validation checks into `debug_assert!`.
- Do not enable whole Clippy groups like `restriction`.
- Do not add `#[inline(always)]` everywhere; poor inlining increases code size
  and can reduce instruction-cache locality.
- Do not keep analysis counters, JSON formatting, or environment checks in
  normal hot paths.
- Do not evaluate speed using instrumentation builds such as `AV2_SB_BITS=1` as
  the final runtime baseline.
- Do not accept a faster lossy path without checking quality and reference
  reconstruction behavior.

## Suggested Next Steps

1. Keep `make clippy-perf` clean before accepting low-risk lint-driven
   performance cleanups.
2. Extend the Criterion microbenchmarks to cover AV2 entropy token emission and
   tile payload assembly.
3. Measure `make build PROFILE=optimized` against the release baseline before
   promoting any profile defaults.
4. Select and version a representative AV2/VVC PGO training set before using
   `make build-pgo` for release comparisons.
5. Review `make llvm-vector-remarks` output before refactoring hot loops for
   vectorization.
6. Refactor hot loops toward row slices, fixed arrays, scratch reuse, and
   branch-light inner loops.
7. Revisit SIMD only after safe scalar code and compiler-assisted
   vectorization have plateaued.

## Measured Checkpoints

### Source Buffer Reuse And Planar Pack/Unpack

Checkpoint: `post-pack-reuse`.

Changes retained:

- AV2 reuses the source frame buffer across frames instead of allocating it per
  frame.
- AV2 `rgb24` <-> planar GBR conversion uses exact pixel chunks instead of
  manually computed byte offsets.
- VVC input sample unpacking and reconstruction packing use bit-depth-specific
  slice loops after public geometry and length validation has already completed.
- Validation runner gained explicit lossy overrides for geometry sweeps.
- `make benchmark-encode-matrix` records bytes, fps, PSNR where available, and
  output/reconstruction hashes for AV2/VVC lossy/lossless matrices.
- `make validate-geometry-sweep` runs small AV2/VVC geometry sweeps in both
  lossless and lossy modes.

One-off compiler flag probe:

```sh
RUSTFLAGS="-Clto=thin -Ccodegen-units=1 -Cembed-bitcode=yes" \
  make benchmark-encode-matrix \
    ENCODE_MATRIX_RUN=probe-thinlto-1 \
    ENCODE_MATRIX_LIMIT=2 \
    ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/baseline-compiler-opt.json
```

Result: not retained. The first AV2 row was only +0.14 fps and the second was
-0.25 fps versus baseline, while release build time increased substantially.

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=post-pack-reuse \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/baseline-compiler-opt.json
```

Matrix totals on `local-aomctc-b2-scc-1080p-lossless-50f`:

| Codec | Mode | Baseline FPS | New FPS | FPS Delta | Byte Delta |
|---|---|---:|---:|---:|---:|
| AV2 | lossless+predictive | 6.91 | 9.03 | +30.7% | 0 |
| AV2 | qp=24+predictive | 3.16 | 3.77 | +19.3% | 0 |
| VVC | lossless | 0.66 | 0.68 | +3.0% | 0 |
| VVC | lossy | 0.95 | 1.02 | +7.4% | 0 |

The full generated reports for this run were written to:

```text
verification/generated/encode_matrix/baseline-compiler-opt.md
verification/generated/encode_matrix/post-pack-reuse.md
```

Geometry sweep command:

```sh
make validate-geometry-sweep
```

Result: passed. This ran `screenshot-sweep-444`,
`screenshot-sweep-444-10bit`, and `screenshot-sweep-420-10bit-canary` for AV2
and VVC in both lossless and lossy modes. Lossless rows used exact
reconstruction checks; lossy rows required encoded output and internal
reconstruction to be produced.

### VVC Native 4:2:2 Residual And Shared Pixel Metrics

Checkpoint: `vvc-parity-native-422-dc-search`.

Changes retained:

- Core `ChromaSampling` now exposes shared subsampling factors, and core
  planar byte-slice SSE is used by the CLI PSNR path.
- VVC non-lossless residual syntax and reconstruction now keep native 4:2:2
  input instead of routing through the old decoder-compatibility frame.
- VVC residual quantization borrows CTU frames instead of cloning them.
- VVC luma DC residual search uses the actual bit depth and inverse-transform
  response before choosing the DC level.
- The validation runner cleanup path tolerates already-removed generated files.

Rejected probe:

- A luma DCT AC estimator increased the first lossy 4:2:0 row from 7.15 MB to
  8.70 MB, slowed encode from 1.11 fps to 0.89 fps, and only improved PSNR by
  about 0.10 dB, so it was not retained.

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-parity-native-422-dc-search \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/post-pack-reuse.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`:

| Mode | Baseline FPS | New FPS | FPS Delta | Byte Delta | Notes |
|---|---:|---:|---:|---:|---|
| lossless | 0.68 | 0.71 | +4.4% | 0 | 4:2:0/4:2:2 rows remain exact; 4:4:4 palette bytes unchanged |
| lossy | 1.02 | 0.63 | -38.2% | +31,508,423 | Native 4:2:2 replaces prior compatibility behavior |

Key lossy row deltas:

| Vector | Format | Bytes Delta | FPS Delta | New PSNR | Notes |
|---|---:|---:|---:|---:|---|
| SceneComposition_1_420 | yuv420p8 | -14,344 | +0.02 | 23.700 | DC search gives a small size win |
| SceneComposition_1_422 | yuv422p8 | +5,005,613 | -0.67 | 24.715 | Native 4:2:2 now measures the real path |
| MissionControlClip1_420 | yuv420p10le | -2,186,574 | -0.12 | 19.005 | Bit-depth-aware DC search fixes a poor high-depth response |
| MissionControlClip1_422 | yuv422p10le | +28,703,728 | -1.10 | 18.364 | Native high-depth 4:2:2 needs better mode/residual decisions |
| MissionControlClip1_444 | yuv444p10le | 0 | -0.03 | 65.611 | Existing palette path unchanged |

The full generated report for this run was written to:

```text
verification/generated/encode_matrix/vvc-parity-native-422-dc-search.md
```

AV2 sanity matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=av2-shared-pixel-metrics-check \
  ENCODE_MATRIX_CODECS=av2 \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/post-pack-reuse.json
```

Result: all 12 AV2 rows were byte-identical to `post-pack-reuse`. Totals were
83,531,302 bytes at 9.01 fps for `lossless+predictive` and 41,098,794 bytes at
3.74 fps for `qp=24+predictive`.

The AV2 generated report for this run was written to:

```text
verification/generated/encode_matrix/av2-shared-pixel-metrics-check.md
```

This checkpoint is correctness-positive but exposes the real VVC lossy parity
gap. Next VVC work should focus on mode decisions and residual coding for
4:2:0 and 4:2:2 rather than treating the old non-native 4:2:2 byte counts as a
valid target.

### VVC CTU Traversal Cleanup

Checkpoint: `vvc-direct-luma-nodes`.

Changes retained:

- VVC residual quantization now uses a luma transform-node walker instead of
  constructing a full CABAC-op vector and filtering luma leaves.
- Quantization constructs `VvcCtuPartitionShape` directly when only traversal
  shape is needed, avoiding large zeroed partition-parameter arrays.
- The streaming encoder reuses one scratch CTU frame per input frame and
  removes the full-frame clone on the residual path.
- Obsolete lossy transform observation helpers are test-only or removed, so the
  release path does not compute discarded transforms.

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-direct-luma-nodes \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-parity-native-422-dc-search.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`:

| Mode | Baseline FPS | New FPS | FPS Delta | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|---:|
| lossless | 0.70 | 0.71 | +1.4% | 0 | 0 |
| lossy | 0.63 | 0.65 | +3.2% | 0 | 0 |

All rows were byte-identical to `vvc-parity-native-422-dc-search`; lossless
rows remained exact and lossy PSNR was unchanged. The largest positive row was
the high-depth 4:2:0 lossy case, which moved from 0.53 fps to 0.57 fps in this
matrix run.

The full generated report for this run was written to:

```text
verification/generated/encode_matrix/vvc-direct-luma-nodes.md
```

AV2 sanity command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=av2-after-vvc-direct-luma-nodes \
  ENCODE_MATRIX_CODECS=av2 \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/av2-shared-pixel-metrics-check.json
```

Result: all 12 AV2 rows were byte-identical to
`av2-shared-pixel-metrics-check`. Totals were 83,531,302 bytes at 8.99 fps for
`lossless+predictive` and 41,098,794 bytes at 3.82 fps for
`qp=24+predictive`.

### VVC Direct Residual Extraction

Checkpoint: `vvc-direct-residual-extract`.

Change retained:

- VVC residual quantization now builds luma/chroma residual vectors directly
  from source samples and predictors, instead of first allocating copied sample
  blocks and then allocating residual blocks from those samples. Off-visible
  padding behavior is unchanged: luma padding remains zero-derived and chroma
  padding remains neutral-sample-derived.

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-direct-residual-extract \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-direct-luma-nodes.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`:

| Mode | Baseline FPS | New FPS | FPS Delta | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|---:|
| lossless | 0.71 | 0.73 | +2.8% | 0 | 0 |
| lossy | 0.65 | 0.65 | 0.0% | 0 | 0 |

All rows were byte-identical to `vvc-direct-luma-nodes`; lossless rows remained
exact and lossy PSNR was unchanged. The high-depth 4:2:2 lossless row improved
from 0.46 fps to 0.48 fps in this run.

The full generated report for this run was written to:

```text
verification/generated/encode_matrix/vvc-direct-residual-extract.md
```

### VVC Prediction Scratch

Checkpoint: `vvc-prediction-stack-scratch`.

Change retained:

- VVC residual quantization and reconstruction reuse the predicted luma/Cb/Cr
  buffers across transform units within a frame.
- DC intra prediction now keeps top and left reference samples in fixed arrays
  sized to the encoder CTU edge, avoiding per-TU reference-vector allocation.
- Residual reconstruction also uses the direct luma transform-node traversal
  instead of constructing CABAC partition ops only to filter luma leaves.

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-prediction-stack-scratch \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-direct-residual-extract.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`:

| Mode | Baseline FPS | New FPS | FPS Delta | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|---:|
| lossless | 0.73 | 0.74 | +1.4% | 0 | 0 |
| lossy | 0.65 | 0.67 | +3.1% | 0 | 0 |

All rows were byte-identical to `vvc-direct-residual-extract`; lossless rows
remained exact and lossy PSNR was unchanged. The 8-bit 4:2:0 and 4:2:2 lossy
rows each gained about 0.04 fps in this run.

The full generated report for this run was written to:

```text
verification/generated/encode_matrix/vvc-prediction-stack-scratch.md
```

AV2 sanity command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=av2-after-vvc-prediction-stack-scratch \
  ENCODE_MATRIX_CODECS=av2 \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/av2-shared-pixel-metrics-check.json
```

Result: all 12 AV2 rows were byte-identical to
`av2-shared-pixel-metrics-check` and lossy PSNR was unchanged. Totals were
83,531,302 bytes at 9.09 fps for `lossless+predictive` and 41,098,794 bytes at
3.83 fps for `qp=24+predictive`.

### VVC Sparse Active Transform

Checkpoint: `vvc-sparse-active-transform`.

Change retained:

- VVC lossy residual quantizers now fill the stored DC/first-4x4 AC subset
  directly instead of constructing full coefficient vectors and copying the
  subset back out.
- VVC inverse transform now has sparse quantized-block entry points that reuse
  dequantized/vertical scratch buffers and only traverse active coefficient
  rows/columns for the stored first-4x4 subset.
- The general full-coefficient inverse transform remains available to tests,
  but the production residual path no longer allocates coefficient,
  dequantized, and vertical vectors per TU.

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-sparse-active-transform \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-prediction-stack-scratch.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`:

| Mode | Baseline FPS | New FPS | FPS Delta | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|---:|
| lossless | 0.74 | 0.74 | 0.0% | 0 | 0 |
| lossy | 0.67 | 0.70 | +4.5% | 0 | 0 |

All rows were byte-identical to `vvc-prediction-stack-scratch`; lossless rows
remained exact and lossy PSNR was unchanged. The largest row gain was the
8-bit 4:2:0 lossy residual path, which moved from 1.24 fps to 1.39 fps in this
run.

The full generated report for this run was written to:

```text
verification/generated/encode_matrix/vvc-sparse-active-transform.md
```

AV2 sanity command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=av2-after-vvc-sparse-active-transform \
  ENCODE_MATRIX_CODECS=av2 \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/av2-shared-pixel-metrics-check.json
```

AV2 sanity result:

| Mode | Bytes | FPS | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|
| lossless+predictive | 83,531,302 | 9.61 | 0 | 0 |
| qp=24+predictive | 41,098,794 | 3.66 | 0 | 0 |

All AV2 rows remained byte-identical and PSNR-identical to the baseline. The
cross-codec report was written to:

```text
verification/generated/encode_matrix/av2-after-vvc-sparse-active-transform.md
```

Additional validation:

```sh
make test
make validate-geometry-sweep
```

Both checks passed after this checkpoint.

### VVC Fixed Active Residual Scan

Checkpoint: `vvc-fixed-active-scan`.

Change retained:

- VVC residual symbol construction now uses a fixed 16-position diagonal scan
  for the active first 4x4 coefficient group.
- This removes the per-TU grouped full-transform scan allocation. The current
  encoder only populates the first 4x4 residual subset, so scanning beyond that
  group could not change the last significant coefficient or emitted syntax.

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-fixed-active-scan \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-sparse-active-transform.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`:

| Mode | Baseline FPS | New FPS | FPS Delta | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|---:|
| lossless | 0.74 | 0.76 | +2.7% | 0 | 0 |
| lossy | 0.70 | 0.71 | +1.4% | 0 | 0 |

All rows were byte-identical to `vvc-sparse-active-transform`; lossless rows
remained exact and lossy PSNR was unchanged. Residual-backed rows improved
consistently, while the 4:4:4 palette rows were effectively unchanged.

The full generated report for this run was written to:

```text
verification/generated/encode_matrix/vvc-fixed-active-scan.md
```

AV2 sanity command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=av2-after-vvc-fixed-active-scan \
  ENCODE_MATRIX_CODECS=av2 \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/av2-shared-pixel-metrics-check.json
```

AV2 sanity result:

| Mode | Bytes | FPS | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|
| lossless+predictive | 83,531,302 | 8.97 | 0 | 0 |
| qp=24+predictive | 41,098,794 | 3.82 | 0 | 0 |

All AV2 rows remained byte-identical and PSNR-identical to the baseline. The
cross-codec report was written to:

```text
verification/generated/encode_matrix/av2-after-vvc-fixed-active-scan.md
```

Additional validation:

```sh
make test
make validate-geometry-sweep
```

Both checks passed after this checkpoint.

### VVC Carried Residual Reconstruction

Checkpoint: `vvc-carried-reconstruction`.

Change retained:

- VVC lossy residual quantization now returns the reconstructed CTU samples it
  already produced for closed-loop prediction.
- The streaming encoder consumes that carried reconstruction instead of running
  a second prediction and inverse-transform pass from the same coefficients.
- The explicit reconstruction helper remains test-only, with a regression test
  proving the carried reconstruction matches the old explicit path.

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-carried-reconstruction \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-fixed-active-scan.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`:

| Mode | Baseline FPS | New FPS | FPS Delta | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|---:|
| lossless | 0.76 | 0.77 | +1.3% | 0 | 0 |
| lossy | 0.71 | 0.76 | +7.0% | 0 | 0 |

All rows were byte-identical to `vvc-fixed-active-scan`; lossless rows
remained exact and lossy PSNR was unchanged. The gain is concentrated in the
subsampled lossy residual rows because 4:4:4 currently routes through the
palette path.

The full generated report for this run was written to:

```text
verification/generated/encode_matrix/vvc-carried-reconstruction.md
```

AV2 sanity command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=av2-after-vvc-carried-reconstruction \
  ENCODE_MATRIX_CODECS=av2 \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/av2-after-vvc-fixed-active-scan.json
```

AV2 sanity result:

| Mode | Bytes | FPS | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|
| lossless+predictive | 83,531,302 | 9.03 | 0 | 0 |
| qp=24+predictive | 41,098,794 | 3.82 | 0 | 0 |

All AV2 rows remained byte-identical and PSNR-identical to the baseline. The
cross-codec report was written to:

```text
verification/generated/encode_matrix/av2-after-vvc-carried-reconstruction.md
```

Additional validation:

```sh
make test
make validate-geometry-sweep
```

Both checks passed after this checkpoint.

## VVC Lean CABAC Events

Checkpoint: `vvc-cabac-lean-events`.

The VVC CABAC writer used to collect CABAC dump symbols, semantic symbols,
context events, and bin-engine events on every normal encode. Those vectors are
only needed for explicit CABAC dump and test paths, but release encodes paid for
per-bin pushes, repeated context model lookups, and debug trace environment
checks. The writer now records those vectors only when constructed through the
dump-enabled path; normal encode uses the same arithmetic state machine and
emits identical bits without the analysis bookkeeping. The two CABAC trace
environment flags are cached once with `OnceLock`.

This change also adds compile-gated VVC stage timing:

```sh
make build VVC_STATS=1
FRAMEFINERY_VVC_STATS=verification/generated/profiling/vvc_stage_scene420_lossless_1f.jsonl \
  ./ff encode "$AOMCTC_ROOT/b2_scc/SceneComposition_1.y4m" \
  --frames 1 \
  --encode vvc:verification/generated/profiling/vvc_stage_scene420_lossless_1f.vvc \
  --recon verification/generated/profiling/vvc_stage_scene420_lossless_1f_recon.yuv \
  --set lossless
python3 scripts/summarize_encoder_instrumentation.py \
  --vvc-stats scene420_lossless/framefinery=verification/generated/profiling/vvc_stage_scene420_lossless_1f.jsonl
```

Normal builds do not compile this instrumentation. Generated traces and
profiling artifacts should stay under `verification/generated/`.

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-cabac-lean-events \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-carried-reconstruction.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`:

| Mode | Baseline FPS | New FPS | FPS Delta | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|---:|
| lossless | 0.77 | 1.65 | +114.3% | 0 | 0 |
| lossy | 0.76 | 1.00 | +31.6% | 0 | 0 |

All rows were byte-identical to `vvc-carried-reconstruction`; lossless rows
remained exact and lossy PSNR was unchanged.

First-frame VVC stage traces on `SceneComposition_1_420` after the CABAC event
cleanup showed:

| Case | Top stage | Time share | Notes |
|---|---|---:|---|
| lossless | `ctu_entropy_write` | 74.8% | residual extraction is now secondary at 20.0% |
| lossy | `ctu_quantize` | 71.0% | entropy write is secondary at 25.0% |

The next VVC parity work should split by path: entropy-symbol/CABAC
specialization for lossless and transform/quantization/reconstruction
specialization for lossy.

AV2 sanity command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=av2-after-vvc-cabac-lean-events \
  ENCODE_MATRIX_CODECS=av2 \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/av2-after-vvc-carried-reconstruction.json
```

AV2 sanity result:

| Mode | Bytes | FPS | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|
| lossless+predictive | 83,531,302 | 8.60 | 0 | 0 |
| qp=24+predictive | 41,098,794 | 3.67 | 0 | 0 |

Additional validation:

```sh
cargo test -p framefinery-codecs --features vvc
cargo test -p framefinery-codecs --features "vvc vvc-stats"
make test
make validate-geometry-sweep
```

All checks passed after this checkpoint.

## VVC Direct Residual Symbol Emission

Checkpoint: `vvc-residual-callback-sink`.

Change retained:

- Normal VVC residual entropy coding now emits residual CABAC syntax directly
  while deriving it, instead of always building a `VvcResidualCabacSymbolStream`
  and then replaying it.
- The old symbol-stream constructors and replay path remain available to tests,
  so residual syntax expectations are still checked against recorded symbols.
- Direct residual emission uses typed sink callbacks for last-position,
  significance, level, remainder, and sign syntax, avoiding enum construction
  and dispatch in the normal encoder path.
- The regular CTU residual path and the 4:4:4 palette/IBC residual helpers now
  both use direct residual emission.

Rejected probe:

- A fixed-array pass-1 residual state removed per-TU state allocation, but the
  six-vector matrix showed mixed fps rows after tightening the arrays to the
  active context footprint. The gain was not clean enough to retain.

Profiling note:

- After `vvc-cabac-lean-events`, 40-run first-frame gprof on
  `SceneComposition_1_420` lossless still showed residual symbol construction
  and replay as a major entropy-side cost: `coefficients_with_tool_flags` plus
  `emit` accounted for about 15.5% self time before direct emission.
- After direct emission, the residual replay hotspot disappeared; the next
  durable hotspots are CABAC probability/context encode, DC prediction, and
  residual context derivation.

Matrix commands:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-direct-residual-symbols \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-cabac-lean-events.json

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-residual-callback-sink \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-direct-residual-symbols.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`, combined from the
previous committed checkpoint:

| Mode | `vvc-cabac-lean-events` FPS | New FPS | FPS Delta | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|---:|
| lossless | 1.65 | 1.85 | +12.1% | 0 | 0 |
| lossy | 1.00 | 1.13 | +13.0% | 0 | 0 |

All rows were byte-identical across the retained runs; lossless rows remained
exact and lossy PSNR was unchanged. The full retained generated reports were
written to:

```text
verification/generated/encode_matrix/vvc-direct-residual-symbols.md
verification/generated/encode_matrix/vvc-residual-callback-sink.md
```

Additional validation:

```sh
cargo test -p framefinery-codecs --features vvc
cargo test -p framefinery-codecs --features "vvc vvc-stats"
make test
make validate-geometry-sweep
```

All checks passed after this checkpoint.

## VVC Batched AC Projection

Checkpoint: `vvc-separable-chroma-ac`.

Changes retained:

- VVC luma lossy AC quantization now computes the 16 source cell sums once per
  transform unit and derives the first 4x4 Hadamard AC levels from those sums,
  instead of recomputing the same cell sums for each AC coefficient.
- VVC chroma lossy AC quantization now computes the active first-4x4 chroma
  coefficients with a separable projection: one vertical DCT accumulation per
  active coefficient row, then reused horizontal projections for each AC level.
- Luma and chroma DC searches now compute residual sum and SSE together in one
  pass before evaluating candidate DC levels.

Rejected probes:

- `vvc-coeff-scratch` added a reusable dense coefficient scratch buffer to the
  CTU CABAC generator. It was byte-identical, but the six-vector matrix
  regressed from 1.85 to 1.77 fps in lossless and from 1.13 to 1.12 fps in
  lossy, likely because the larger hot generator state hurt layout/cache
  behavior more than it saved allocation work.
- Reusing residual buffers inside the VVC quantizer improved one-frame
  lossless `ctu_quantize` timing, but made the lossy first-frame quantizer
  slower than the luma-AC-only checkpoint and immediately regressed the first
  two lossless matrix rows. The run was stopped and the change was reverted.

First-frame VVC stage trace on `SceneComposition_1_420` lossy:

| Checkpoint | `ctu_quantize` | Timed total | Bytes | PSNR |
|---|---:|---:|---:|---:|
| `vvc-residual-callback-sink` | 303.800 ms | 413.955 ms | 128,845 | 24.283 |
| `vvc-luma-ac-cell-sums` | 192.388 ms | 297.819 ms | 128,845 | 24.283 |

Matrix commands:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-luma-ac-cell-sums \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-residual-callback-sink.json

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-separable-chroma-ac \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-luma-ac-cell-sums.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`, compared with the
previous committed checkpoint:

| Mode | Baseline FPS | New FPS | FPS Delta | Byte Delta | PSNR Delta |
|---|---:|---:|---:|---:|---:|
| lossless | 1.85 | 1.82 | -1.6% | 0 | 0 |
| lossy | 1.13 | 1.35 | +19.8% | 0 | 0 |

The lossless code path is not supposed to consume the batched lossy AC
projection; its mixed row movement is treated as run-to-run/code-layout noise.
All rows were byte-identical to the comparison baselines; lossless rows
remained exact and lossy PSNR was unchanged. The retained generated reports
were written to:

```text
verification/generated/encode_matrix/vvc-luma-ac-cell-sums.md
verification/generated/encode_matrix/vvc-separable-chroma-ac.md
```

Additional validation:

```sh
cargo test -p framefinery-codecs --features vvc
cargo test -p framefinery-codecs --features "vvc vvc-stats"
make test
make validate-geometry-sweep
```

All checks passed after this checkpoint.

## VVC Frame-Slice Lossless Residual

Checkpoint: `vvc-frame-slice-residual`.

Changes retained:

- VVC 4:2:0 and 4:2:2 lossless residual pictures now use one frame slice
  instead of one slice per CTU. This removes repeated slice headers and lets
  CABAC contexts carry across CTUs in the lossless residual path.
- The single-slice lossless quantizer predicts against the carried full-frame
  reconstruction and updates that reconstruction as CTUs are emitted.
- VVC 4:2:0 and 4:2:2 lossy residual pictures deliberately remain one slice
  per CTU for now. The CTU-slice path uses CTU-local prediction, which matches
  the decoder's slice-boundary prediction rules and keeps the previous lossy
  byte counts.
- Normal residual entropy emission uses compact first-4x4 coefficient
  accessors for the active coefficient subset, avoiding full coefficient-vector
  materialization in the common residual syntax path.
- `vvc-stats` frame records now include counters such as slice count,
  single-slice use, TU counts, nonzero counts, and CBF counts.
- VVC SPS signalling now raises the current luma MTT depth to 5, which keeps
  thin high-depth 4:2:0 lossless shapes within the coded partition tree.
- High-depth 4:4:4 palette BDPCM/transform-skip residual coding now emits the
  scaled transform-skip levels expected by VTM and rejects the shortcut when a
  coefficient is not exactly representable at that transform-skip scale.

Rejected probe:

- Using a single frame slice for all 4:2:0/4:2:2 residual modes was not
  retained. It kept the lossless size win, but moved lossy totals from
  311,683,720 bytes to 318,394,921 bytes and reduced matrix throughput to 1.27
  fps. The retained split keeps lossy subsampled rows byte-identical to
  `vvc-separable-chroma-ac`.

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-frame-slice-residual \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-separable-chroma-ac.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`, compared with
`vvc-separable-chroma-ac`:

| Mode | Baseline bytes | New bytes | Byte delta | Baseline FPS | New FPS | Notes |
|---|---:|---:|---:|---:|---:|---|
| lossless | 562,246,601 | 547,557,841 | -14,688,760 | 1.82 | 1.78 | size win comes from 4:2:0/4:2:2 frame slices |
| lossy | 311,683,720 | 311,763,094 | +79,374 | 1.35 | 1.31 | subsampled lossy rows are byte-identical; 4:4:4 changed with the high-depth palette fix |

Lossless row deltas:

| Vector | Format | Bytes delta | FPS delta | PSNR |
|---|---|---:|---:|---:|
| SceneComposition_1_420 | yuv420p8 | -3,013,712 | +0.23 | inf |
| SceneComposition_1_422 | yuv422p8 | -3,318,716 | -0.05 | inf |
| MissionControlClip1_420 | yuv420p10le | -4,053,076 | -0.08 | inf |
| MissionControlClip1_422 | yuv422p10le | -4,382,630 | -0.03 | inf |
| MissionControlClip1_444 | yuv444p10le | +79,374 | -0.06 | 65.612 |

The full generated report for this run was written to:

```text
verification/generated/encode_matrix/vvc-frame-slice-residual.md
```

Additional validation:

```sh
cargo test -p framefinery-codecs --features vvc
cargo test -p framefinery-codecs --features "vvc vvc-stats"
make validate-geometry-sweep
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=screenshot-sweep-444-10bit VALIDATION_REFERENCE_MODE=required
```

Results: both VVC test builds passed with 122 tests, the full AV2/VVC geometry
sweep passed, VVC smoke passed 3/3 with the reference decoder required, and
the high-depth VVC 4:4:4 sweep passed 64/64 with the reference decoder
required.

## VVC Residual Metadata And Pass-1 State

Checkpoints: `vvc-tu-ac-presence-flags`, `vvc-fixed-pass1-state`.

Changes retained:

- VVC quantized TU metadata now carries `*_tu_has_ac` flags next to the AC
  coefficient arrays. CABAC CBF decisions use those flags instead of rescanning
  the 15-entry AC arrays for every luma/Cb/Cr TU.
- Lossless AC extraction computes the AC-present flag while copying the
  first-4x4 AC levels, so lossless does not pay an extra coefficient pass.
- The lossy luma and chroma quantizers return AC-present metadata with the
  selected quantized AC coefficients.
- `VvcResidualPass1State` now uses fixed first-4x4 coefficient state and a
  bounded subblock map instead of allocating three `Vec`s for every residual
  TU. Out-of-first-4x4 coefficient context lookups still return zero, matching
  the current emitted coefficient subset.

Rejected probe:

- Replacing `VvcChromaNeighbourState` with fixed CTU-sized arrays was not
  retained. It preserved bytes and PSNR, but total throughput dropped to 1.80
  fps for lossless and 1.29 fps for lossy against `vvc-borrow-ctu-params`.

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-fixed-pass1-state \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-tu-ac-presence-flags.json
```

VVC totals on `local-aomctc-b2-scc-1080p-lossless-50f`, compared with
`vvc-tu-ac-presence-flags`:

| Mode | Baseline bytes | New bytes | Byte delta | Baseline FPS | New FPS | Notes |
|---|---:|---:|---:|---:|---:|---|
| lossless | 547,557,841 | 547,557,841 | +0 | 1.91 | 2.00 | allocation-free residual pass-1 state |
| lossy | 311,763,094 | 311,763,094 | +0 | 1.35 | 1.41 | allocation-free residual pass-1 state |

The preceding AC-presence checkpoint was also byte-identical against
`vvc-borrow-ctu-params` and improved totals to 1.91 fps lossless and 1.35 fps
lossy.

Additional validation:

```sh
cargo test -p framefinery-codecs --features vvc
cargo test -p framefinery-codecs --features "vvc vvc-stats"
```

Results: both VVC test builds passed with 122 tests.

## VVC Intra Feature Plumbing

Checkpoint: `vvc-intra-feature-default`.

Changes retained:

- VVC now accepts CLI `--set qp=<1..255>` and maps it into the emitted slice QP. Chroma QP
  follows the existing VVC lossy chroma offset, preserving the old default when
  `qp` is omitted.
- Packed `rgb24` source handling moved into the common frame conversion layer.
  AV2 and VVC now use the same reversible `rgb24` <-> planar `gbrp8`
  conversion at the CLI boundary, while codec internals continue to consume
  native planar frames.
- VVC compile-gated instrumentation now includes frame-level stage stats and a
  CTU bit JSONL sink through `FRAMEFINERY_VVC_STATS` and
  `FRAMEFINERY_VVC_CTU_BITS`.
- VVC luma intra mode selection now uses a shared candidate-cost path and can
  select horizontal and vertical prediction in addition to DC and planar.
- Generic VVC `Angular(index)` prediction and CABAC mode signalling are wired
  as infrastructure, but non-cardinal angular modes are not selected by
  default yet. The first probe produced mixed bitrate results, so the default
  selector remains on the smaller H/V candidate set until reference filtering
  and rate-aware selection are implemented.

First-frame VVC lossy deltas versus the previous default-DC/planar checkpoint:

| Vector | Format | Bytes delta | FPS delta | PSNR delta |
|---|---|---:|---:|---:|
| SceneComposition_1_420 | yuv420p8 | -12,639 | +0.02 | +0.088 |
| SceneComposition_1_422 | yuv422p8 | -12,639 | -0.02 | +0.066 |
| Wayland screen capture | rgb24 | -23,391 | +0.00 | +0.059 |
| MissionControlClip1_420 | yuv420p10le | +2,110 | +0.05 | +0.087 |
| MissionControlClip1_422 | yuv422p10le | +2,108 | +0.04 | +0.053 |
| MissionControlClip1_444 | yuv444p10le | +2,102 | -0.01 | +0.030 |

Current six-vector comparison, first frame only. Bytes are summed across the
six rows; FPS and PSNR are unweighted row averages, with full per-vector rows
kept in the generated report.

| Codec | Mode | Total bytes | Avg FPS | Avg PSNR |
|---|---|---:|---:|---:|
| AV2 | lossless | 6,586,445 | 3.04 | inf |
| AV2 | qp=24 | 2,400,148 | 1.33 | 49.418 |
| VVC | lossless | 10,659,047 | 1.96 | inf |
| VVC | qp=24 | 9,198,820 | 0.64 | 18.371 |

Current six-vector comparison, 50 frames:

| Codec | Mode | Total bytes | Avg FPS | Avg PSNR |
|---|---|---:|---:|---:|
| AV2 | lossless | 83,531,302 | 8.83 | inf |
| AV2 | qp=24 | 41,098,794 | 4.83 | 51.805 |
| VVC | lossless | 545,598,292 | 1.86 | inf |
| VVC | qp=24 | 463,160,046 | 0.65 | 18.394 |

Matrix commands:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-intra-feature-default-1f \
  ENCODE_MATRIX_CODECS="av2 vvc" \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-intra-feature-default-50f \
  ENCODE_MATRIX_CODECS="av2 vvc" \
  ENCODE_MATRIX_MODES="lossless lossy"
```

Generated reports:

```text
verification/generated/encode_matrix/vvc-intra-feature-default-1f.md
verification/generated/encode_matrix/vvc-intra-feature-default-50f.md
```

Instrumentation smoke command:

```sh
make build VVC_STATS=1
FRAMEFINERY_VVC_STATS=verification/generated/profiling/vvc_stats_probe.jsonl \
FRAMEFINERY_VVC_CTU_BITS=verification/generated/profiling/vvc_ctu_bits_probe.jsonl \
  ./ff encode \
  verification/generated/test_vectors/aomctc_b2_SceneComposition_1_420_1920x1080_15_1f_yuv420p8.yuv \
  --frames 1 \
  --encode vvc:verification/generated/profiling/vvc_stats_probe.obu \
  --recon verification/generated/profiling/vvc_stats_probe.recon \
  --set qp=24
```

Validation:

```sh
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"
cargo test -p framefinery-api --features ""
cargo test -p framefinery encode_job \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale"
cargo test -p framefinery-codecs vvc --features "vvc"
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-geometry-sweep GEOMETRY_SWEEP_REFERENCE_MODE=off
```

Results: all commands completed successfully. The geometry sweep covered AV2
and VVC, lossless and lossy, across the current screenshot sweep manifests.

## VVC Unified Lossless Intra Search

Checkpoint: `vvc-unified-lossless-intra-1f`.

Changes retained:

- VVC lossless luma now uses the same Planar/DC/H/V intra candidate machinery
  as lossy instead of forcing the reduced lossless-only path.
- VVC lossless chroma now evaluates Derived plus the existing explicit
  Planar/DC/H/V candidate list, using the same selector path as lossy.
- No mode-selection constants were tuned in this checkpoint. Non-cardinal
  angular modes and CCLM remain feature work rather than enabled defaults.

First-frame six-vector matrix versus `vvc-intra-feature-default-1f`:

| Codec | Mode | Total bytes | FPS | Notes |
|---|---|---:|---:|---|
| AV2 | lossless | 6,586,445 | 2.65 | unchanged reference context |
| AV2 | qp=24 | 2,400,148 | 1.16 | unchanged reference context |
| VVC | lossless | 6,780,255 | 1.03 | -3,878,792 bytes versus prior VVC checkpoint |
| VVC | qp=24 | 10,385,397 | 0.39 | current context only; this patch removes no lossy candidates |

The feature tradeoff is clear: allowing lossless to use the richer intra
candidate set cuts first-frame VVC lossless size by about 36% on the six-vector
screen-content matrix, at the cost of extra intra-search work. This is an
accepted feature checkpoint, not the final tuned path.

High-depth smoke lossless size spot-check after the change:

| Vector | Before | After | Delta |
|---|---:|---:|---:|
| canary_420_10 | 487 | 321 | -166 |
| canary_422_10 | 646 | 408 | -238 |
| canary_444_10 | 1,034 | 580 | -454 |
| canary_420_12 | 656 | 465 | -191 |
| canary_422_12 | 874 | 594 | -280 |
| canary_444_12 | 1,382 | 843 | -539 |

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-unified-lossless-intra-1f \
  ENCODE_MATRIX_CODECS="av2 vvc" \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-intra-feature-default-1f.json
```

Generated report:

```text
verification/generated/encode_matrix/vvc-unified-lossless-intra-1f.md
```

Validation:

```sh
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"
cargo test -p framefinery-codecs vvc --features "vvc"
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
make validate-geometry-sweep GEOMETRY_SWEEP_REFERENCE_MODE=off
```

## VVC Base CCLM Chroma Mode

Checkpoint: `vvc-cclm-base-1f`.

Changes retained:

- VVC chroma intra mode selection can now choose the base CCLM/LM chroma mode
  where the current dual-tree CTU syntax allows `cclm_mode_flag`.
- The predictor derives LM parameters from reconstructed luma and neighboring
  chroma templates, and is shared by quantization and reconstruction so the
  internal encoder reconstruction stays aligned with reference decode.
- CCLM usage is counted by the compile-gated `vvc-stats` CTU and frame
  counters as `chroma_mode_cclm`.
- No mode-selection constants were tuned. The checkpoint wires a codec feature
  only: MDLM_L/MDLM_T and 4:2:2 CCLM remain TODO feature work.

First-frame six-vector matrix versus `vvc-unified-lossless-intra-1f`:

| Codec | Mode | Total bytes | FPS | Notes |
|---|---|---:|---:|---|
| AV2 | lossless | 6,586,445 | 2.64 | unchanged reference context |
| AV2 | qp=24 | 2,400,148 | 1.14 | unchanged reference context |
| VVC | lossless | 6,436,959 | 0.94 | -343,296 bytes versus prior VVC checkpoint |
| VVC | qp=24 | 8,828,183 | 0.37 | -1,557,214 bytes versus prior VVC checkpoint |

Most of the immediate win came from RGB and 4:4:4 chroma correlation. The
4:2:2 rows are byte-identical because this checkpoint keeps CCLM disabled for
that sampling mode until the compatible syntax/prediction path is completed.

High-depth smoke lossless size spot-check after the change:

| Vector | Previous | After | Delta |
|---|---:|---:|---:|
| canary_420_10 | 321 | 321 | 0 |
| canary_422_10 | 408 | 408 | 0 |
| canary_444_10 | 580 | 580 | 0 |
| canary_420_12 | 465 | 465 | 0 |
| canary_422_12 | 594 | 594 | 0 |
| canary_444_12 | 843 | 765 | -78 |

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-cclm-base-1f \
  ENCODE_MATRIX_CODECS="av2 vvc" \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-unified-lossless-intra-1f.json
```

Generated report:

```text
verification/generated/encode_matrix/vvc-cclm-base-1f.md
```

Validation:

```sh
cargo check -p framefinery-codecs --features "vvc"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"
cargo test -p framefinery-codecs vvc --features "vvc"
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make build
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
make validate-geometry-sweep GEOMETRY_SWEEP_REFERENCE_MODE=off
```

## VVC MDLM Chroma Modes

Checkpoint: `vvc-mdlm-candidates-1f`.

Changes retained:

- VVC now models CCLM as three explicit chroma modes: base LM, MDLM_L, and
  MDLM_T.
- CABAC chroma mode syntax now writes the VTM-shaped `cclm_mode_idx` path:
  base LM uses symbol 0, while MDLM_L and MDLM_T use symbol 1/2 with the
  bypass follow-up bin. `cclm_mode_idx` also has a semantic instrumentation ID
  so CABAC vector dumps stay complete when MDLM is selected.
- The chroma predictor now derives MDLM parameters from extended below-left or
  top-right templates, then reuses the same linear chroma-from-luma fit used by
  base LM.
- The existing lossless/lossy chroma SAD selector evaluates all three LM-family
  candidates when CCLM is legal. No constants or thresholds were tuned.
- `vvc-stats` now records `chroma_mode_cclm_linear`,
  `chroma_mode_mdlm_left`, and `chroma_mode_mdlm_top` in addition to the
  aggregate `chroma_mode_cclm` counter.

First-frame six-vector matrix versus `vvc-cclm-base-1f`:

| Codec | Mode | Total bytes | FPS | Notes |
|---|---|---:|---:|---|
| AV2 | lossless | 6,586,445 | 2.63 | unchanged reference context |
| AV2 | qp=24 | 2,400,148 | 1.15 | unchanged reference context |
| VVC | lossless | 6,395,280 | 0.82 | -41,679 bytes versus prior VVC checkpoint |
| VVC | qp=24 | 6,683,289 | 0.39 | -2,144,894 bytes versus prior VVC checkpoint |

Affected VVC lossy rows improved in both size and PSNR because the new chroma
predictors remove residual energy instead of only moving syntax around. The
largest first-frame wins were the Wayland RGB row, from 2,090,954 bytes at
21.990 dB to 760,612 bytes at 24.373 dB, and the 10-bit 4:4:4 MissionControl
row, from 2,822,393 bytes at 13.830 dB to 2,254,980 bytes at 14.930 dB. The
4:2:2 rows remain byte-identical because CCLM is still disabled for 4:2:2 in
the current syntax gate.

Small reference-validation spot checks versus `vvc-cclm-base-1f`:

| Set | Vector | Previous | After | Delta |
|---|---|---:|---:|---:|
| smoke | checker_420 | 124 | 116 | -8 |
| smoke | blocks_444 | 328 | 251 | -77 |
| high-depth-smoke | canary_444_10 | 580 | 554 | -26 |
| high-depth-smoke | canary_444_12 | 765 | 754 | -11 |

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-mdlm-candidates-1f \
  ENCODE_MATRIX_CODECS="av2 vvc" \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-cclm-base-1f.json
```

Generated report:

```text
verification/generated/encode_matrix/vvc-mdlm-candidates-1f.md
```

Validation:

```sh
cargo check -p framefinery-codecs --features "vvc"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"
cargo test -p framefinery-codecs vvc --features "vvc"
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make build
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
make validate-geometry-sweep GEOMETRY_SWEEP_REFERENCE_MODE=off
```

## VVC Full Angular Intra Modes

Checkpoint: `vvc-full-angular-1f`.

Changes retained:

- VVC luma intra search now evaluates the full angular mode range 2..66
  instead of only the cardinal horizontal/vertical directional modes.
- Chroma explicit-mode validation now accepts the full VVC angular range,
  including the VDIA replacement candidate used when the co-located luma mode
  collides with the chroma candidate list.
- Angular prediction now uses VVC-style modified-wide-angle remapping for
  rectangular blocks.
- Luma angular prediction now has the VVC four-tap interpolation path,
  smoothing interpolation path, and filtered-reference path used by the
  non-planar angular predictors.
- The negative-angle reference extension now clamps against the physical side
  reference length instead of the scratch buffer length. This fixed the
  reference-decoder mismatch exposed by `blocks_444`.
- `vvc-stats` now emits per-angular-index counters such as
  `luma_mode_angular_21` and `chroma_mode_angular_66` so later search work can
  compare mode distribution directly.

This checkpoint intentionally does not tune thresholds or constants. It expands
the implemented VVC feature surface first; later work should make the expanded
mode set faster with rate-aware pruning or staged candidate generation.

First-frame six-vector matrix versus `vvc-mdlm-candidates-1f`:

| Codec | Mode | Total bytes | FPS | Notes |
|---|---|---:|---:|---|
| AV2 | lossless | 6,586,445 | 2.68 | unchanged reference context |
| AV2 | qp=24 | 2,400,148 | 1.17 | unchanged reference context |
| VVC | lossless | 6,009,752 | 0.18 | -385,528 bytes versus prior VVC checkpoint |
| VVC | qp=24 | 6,715,559 | 0.18 | +32,270 bytes versus prior VVC checkpoint |

The lossless path gets a broad bitrate win from the complete angular mode set.
The lossy path is mixed because exhaustive SAD selection now has more choices
but no rate-aware angular syntax cost yet: three rows shrink, two high-depth
rows grow, and total bytes rise slightly. FPS drops substantially in both VVC
modes because the current implementation evaluates all 65 luma angular
directions per candidate block.

Per-row VVC deltas versus `vvc-mdlm-candidates-1f`:

| Mode | Vector | Bytes | Delta bytes | FPS | PSNR mean |
|---|---|---:|---:|---:|---:|
| lossless | SceneComposition 420 8-bit | 357,191 | -28,049 | 0.22 | inf |
| lossless | SceneComposition 422 8-bit | 431,535 | -31,741 | 0.22 | inf |
| lossless | Wayland RGB 8-bit | 504,666 | -32,621 | 0.11 | inf |
| lossless | MissionControl 420 10-bit | 1,227,075 | -88,685 | 0.21 | inf |
| lossless | MissionControl 422 10-bit | 1,510,580 | -100,052 | 0.21 | inf |
| lossless | MissionControl 444 10-bit | 1,978,705 | -104,380 | 0.18 | inf |
| qp=24 | SceneComposition 420 8-bit | 192,454 | -8,805 | 0.27 | 24.650 |
| qp=24 | SceneComposition 422 8-bit | 987,467 | -10,348 | 0.22 | 20.057 |
| qp=24 | Wayland RGB 8-bit | 721,414 | -39,198 | 0.10 | 24.507 |
| qp=24 | MissionControl 420 10-bit | 883,257 | +19,115 | 0.25 | 15.721 |
| qp=24 | MissionControl 422 10-bit | 1,603,833 | -648 | 0.22 | 14.405 |
| qp=24 | MissionControl 444 10-bit | 2,327,134 | +72,154 | 0.15 | 14.773 |

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-full-angular-1f \
  ENCODE_MATRIX_CODECS="av2 vvc" \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-mdlm-candidates-1f.json
```

Generated report:

```text
verification/generated/encode_matrix/vvc-full-angular-1f.md
```

Validation:

```sh
cargo test -p framefinery-codecs vvc --features "vvc"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"
make build
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
make validate-geometry-sweep GEOMETRY_SWEEP_REFERENCE_MODE=off
```

## VVC Staged Angular Search And 4:2:2 CCLM

Checkpoint: `vvc-staged-angular-cclm422-1f`.

Changes retained:

- VVC luma mode selection now keeps the full angular predictor/syntax feature
  surface but no longer evaluates all 65 angular directions for every luma TU.
- The angular search list is generated from VVC default directional families,
  already-coded left/above luma modes, and a source-block structure-tensor
  edge seed. Candidate generation deduplicates by VVC luma mode index.
- After the coarse directional pass, the encoder refines around the best
  angular family before final mode selection. This recovers most of the
  exhaustive-search bitrate while avoiding the global full sweep.
- The edge seed reads visible luma samples with the raw-frame stride, so thin
  coded geometries do not probe padded/coded-space samples.
- CCLM/MDLM chroma prediction is now enabled for 4:2:2. The predictor already
  had 4:2:2 luma downsampling; this checkpoint removes the remaining tool flag
  and residual candidate gates.

First-frame six-vector matrix versus `vvc-full-angular-1f`:

| Codec | Mode | Total bytes | FPS | Notes |
|---|---|---:|---:|---|
| AV2 | lossless | 6,586,445 | 2.48 | unchanged reference context |
| AV2 | qp=24 | 2,400,148 | 1.11 | unchanged reference context |
| VVC | lossless | 5,996,606 | 0.32 | -13,146 bytes, +0.14 fps versus full angular |
| VVC | qp=24 | 5,880,550 | 0.27 | -835,009 bytes, +0.09 fps versus full angular |

The staged search is a speed win without giving up the full predictor feature
surface. The 4:2:2 CCLM enablement more than pays for the small residual
regressions on 4:2:0/RGB/4:4:4 lossy rows: both 4:2:2 lossy rows are much
smaller than the exhaustive-angular baseline and their PSNR improves.

Per-row VVC deltas versus `vvc-full-angular-1f`:

| Mode | Vector | Bytes | Delta bytes | FPS | PSNR mean |
|---|---|---:|---:|---:|---:|
| lossless | SceneComposition 420 8-bit | 357,417 | +226 | 0.42 | inf |
| lossless | SceneComposition 422 8-bit | 424,892 | -6,643 | 0.39 | inf |
| lossless | Wayland RGB 8-bit | 505,362 | +696 | 0.23 | inf |
| lossless | MissionControl 420 10-bit | 1,227,907 | +832 | 0.36 | inf |
| lossless | MissionControl 422 10-bit | 1,500,890 | -9,690 | 0.33 | inf |
| lossless | MissionControl 444 10-bit | 1,980,138 | +1,433 | 0.27 | inf |
| qp=24 | SceneComposition 420 8-bit | 192,458 | +4 | 0.50 | 24.635 |
| qp=24 | SceneComposition 422 8-bit | 273,267 | -714,200 | 0.36 | 24.963 |
| qp=24 | Wayland RGB 8-bit | 723,433 | +2,019 | 0.14 | 24.496 |
| qp=24 | MissionControl 420 10-bit | 891,654 | +8,397 | 0.41 | 15.688 |
| qp=24 | MissionControl 422 10-bit | 1,424,164 | -179,669 | 0.31 | 15.029 |
| qp=24 | MissionControl 444 10-bit | 2,375,574 | +48,440 | 0.20 | 14.690 |

Matrix command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-staged-angular-cclm422-1f \
  ENCODE_MATRIX_CODECS="av2 vvc" \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-full-angular-1f.json
```

Generated report:

```text
verification/generated/encode_matrix/vvc-staged-angular-cclm422-1f.md
```

Validation:

```sh
cargo test -p framefinery-codecs vvc --features "vvc"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"
make build
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
make validate-geometry-sweep GEOMETRY_SWEEP_REFERENCE_MODE=off
```

## VVC Residual Path Unification

Checkpoint: `vvc-residual-path-unified`.

This checkpoint keeps the `vvc-staged-angular-cclm422-1f` coding decisions but
removes another layer of lossy/lossless split from the VVC residual encoder.
The CTU luma/chroma mode-search loops now call common TU finalization helpers:
lossless and lossy still produce different coefficients and reconstructions,
but the selected prediction mode flows through one decision path.

The residual syntax configuration also now derives from one residual tool
profile keyed by `VvcResidualCodingMode`. Lossless still enables transform skip
globally because it is required by the current exact residual syntax, while
lossy keeps transform skip disabled until the block selector can actually pick
profitable transform-skip candidates without adding dead syntax flags.

Validation:

```sh
cargo check -p framefinery-codecs --features "vvc"
cargo test -p framefinery-codecs vvc --features "vvc"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

## VVC Intra Search Instrumentation

Checkpoint: `vvc-intra-search-stats`.

This checkpoint keeps the `vvc-residual-path-unified` bitstreams unchanged in
normal builds while extending the compile-gated `vvc-stats` instrumentation:

- `VvcQuantizedColor` carries gated intra-search counters only when
  `framefinery-codecs/vvc-stats` is enabled.
- Frame stats and CTU bit JSONL records now include luma candidate counts
  split into DC, planar, directional coarse, and directional refinement.
- Chroma counters now split candidate work into derived, explicit, and CCLM
  candidates.
- `scripts/summarize_encoder_instrumentation.py --vvc-stats` now prints a
  compact counter table and caps per-angular-index counters with `--top`.
- The remaining final sampled-color branch now goes through
  `VvcResidualCodingMode`, removing another local lossy/lossless boolean from
  the CTU residual path.

The first-frame six-vector matrix against `vvc-staged-angular-cclm422-1f`
was byte-identical for AV2 and VVC, lossless and QP24 lossy:

| Codec | Mode | Total bytes | Byte delta |
|---|---|---:|---:|
| AV2 | lossless | 6,586,445 | 0 |
| AV2 | qp=24 | 2,400,148 | 0 |
| VVC | lossless | 5,996,606 | 0 |
| VVC | qp=24 | 5,880,550 | 0 |

Instrumentation probe on the first SceneComposition 4:2:0 frame, VVC QP24:

| Counter | Value |
|---|---:|
| `luma_tu_count` | 32,400 |
| `luma_candidate_count` | 665,495 |
| `luma_candidate_directional_coarse` | 501,085 |
| `luma_candidate_directional_refinement` | 99,610 |
| `chroma_tu_count` | 32,400 |
| `chroma_candidate_count` | 259,200 |
| `chroma_candidate_explicit` | 129,600 |
| `chroma_candidate_cclm` | 97,200 |

The probe also confirms `ctu_quantize` remains the dominant timed stage at
about 92% of the recorded encode time. That points the next VVC work toward
reducing candidate cost or improving residual/transform efficiency rather than
micro-optimizing file I/O or final reconstruction packing.

Commands:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-intra-stats-1f \
  ENCODE_MATRIX_CODECS="av2 vvc" \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-staged-angular-cclm422-1f.json

make build VVC_STATS=1
FRAMEFINERY_VVC_STATS=verification/generated/profiling/vvc_intra_candidate_stats_probe.jsonl \
FRAMEFINERY_VVC_CTU_BITS=verification/generated/profiling/vvc_intra_candidate_ctu_probe.jsonl \
  ./ff encode \
  verification/generated/test_vectors/aomctc_b2_SceneComposition_1_420_1920x1080_15_1f_yuv420p8.yuv \
  --frames 1 \
  --encode vvc:verification/generated/profiling/vvc_intra_candidate_probe.vvc \
  --recon verification/generated/profiling/vvc_intra_candidate_probe_recon.yuv \
  --set qp=24

python3 scripts/summarize_encoder_instrumentation.py \
  --vvc-stats scene420/framefinery=verification/generated/profiling/vvc_intra_candidate_stats_probe.jsonl \
  --top 12
```

## VVC Fast Chroma DC Search

Checkpoint: `vvc-chroma-dc-fast-search-1f`.

This checkpoint replaces the VVC chroma DC quantizer's generic exhaustive
`-255..255` level scan with an exact monotonic search. The fast path finds the
first level at or above the DC target, evaluates that reconstructed value and
the previous one, and keeps the existing strict-improvement tie behavior. When
the decoder-side residual mapping would wrap through `i16` at extreme QP and
bit-depth combinations, the encoder falls back to the old exhaustive selector
so bitstreams remain unchanged.

The new unit test compares the fast selector and the public chroma DC quantizer
against the old exhaustive search across 4/8/16/32-wide TUs, 8/10/12-bit input,
and representative QP values from 0 through 63.

First-frame six-vector matrix versus `vvc-intra-stats-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,996,606 | 0.34 | 0 |
| VVC | qp=24 | 5,880,550 | 0.40 | 0 |

Per-row lossy VVC FPS deltas in this run were positive by about +0.07 to
+0.17 fps, while lossless rows were unchanged apart from normal timing noise.

Commands:

```sh
cargo test -p framefinery-codecs vvc_chroma_dc_fast_search_matches_exhaustive_search --features vvc
cargo test -p framefinery-codecs vvc --features vvc
cargo check -p framefinery-codecs --features vvc

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-chroma-dc-fast-search-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-intra-stats-1f.json
```

## VVC Lossy SSE Mode Scoring

Checkpoint: `vvc-lossy-sse-mode-score-1f`.

This checkpoint keeps VVC luma/chroma mode selection on the same shared
candidate path, but makes the candidate score depend on the residual coding
mode:

- lossless still ranks candidates by residual SAD, matching the exact-residual
  entropy proxy used by the current lossless path;
- lossy ranks candidates by residual SSE, matching the distortion term used by
  the QP path and PSNR measurements.

The selector API now stores neutral `score` values instead of SAD-specific
field names. The lossy behavior change is therefore gated at block mode
selection without reintroducing a separate lossy encode path.

First-frame six-vector matrix versus `vvc-chroma-dc-fast-search-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,996,606 | 0.35 | 0 |
| VVC | qp=24 | 5,727,069 | 0.40 | -153,481 |

Per-row VVC QP24 deltas:

| Vector | Bytes delta | FPS delta | PSNR |
|---|---:|---:|---:|
| SceneComposition_1_420 | -6,323 | -0.01 | 24.846 |
| SceneComposition_1_422 | -9,138 | +0.00 | 25.205 |
| screen_wayland_activity_rgb | +18,220 | +0.00 | 24.657 |
| MissionControlClip1_420 | -25,060 | +0.01 | 15.870 |
| MissionControlClip1_422 | -51,137 | +0.01 | 15.243 |
| MissionControlClip1_444 | -80,043 | +0.00 | 14.890 |

Commands:

```sh
cargo test -p framefinery-codecs vvc --features vvc

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-lossy-sse-mode-score-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-chroma-dc-fast-search-1f.json
```

## VVC Luma Mode Map

Checkpoint: `vvc-luma-mode-map-1f`.

This checkpoint removes an O(prior-TU) scan from VVC luma directional candidate
generation. The quantizer now maintains a CTU-local luma mode map as leaves are
finalized, so left and above candidate seeds are direct lookups instead of
searches through previously visited transform nodes.

The candidate set is unchanged, so the first-frame matrix is byte-identical
against `vvc-lossy-sse-mode-score-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,996,606 | 0.36 | 0 |
| VVC | qp=24 | 5,727,069 | 0.40 | 0 |

Lossless rows improved by up to about +0.03 fps in this run. Lossy rows were
mixed within timing noise, but the cleanup keeps neighbour lookup cost bounded
as we add more VVC intra partition and search features.

Commands:

```sh
cargo test -p framefinery-codecs vvc --features vvc

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-luma-mode-map-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-lossy-sse-mode-score-1f.json
```

## VVC Co-Located Luma Mode Map

Checkpoint: `vvc-colocated-mode-map-1f`.

This checkpoint reuses the CTU-local luma mode map for chroma's co-located luma
mode lookup. Chroma mode selection previously scanned the already-coded luma TU
list for every chroma TU. The new lookup reads the same center sample from the
mode map, so the candidate decisions and bitstreams stay unchanged while the
lookup cost remains bounded as partitioning work expands.

The first-frame matrix is byte-identical against `vvc-luma-mode-map-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,996,606 | 0.37 | 0 |
| VVC | qp=24 | 5,727,069 | 0.40 | 0 |

Commands:

```sh
cargo test -p framefinery-codecs vvc --features vvc

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-colocated-mode-map-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-luma-mode-map-1f.json
```

## VVC Per-TU Transform-Skip Flags

Checkpoint: `vvc-tu-transform-skip-flags-1f`.

This checkpoint moves VVC transform-skip selection from a residual-writer
slice-level assumption into quantized TU metadata. The current decisions remain
unchanged: lossless luma/chroma TUs mark transform-skip, while lossy luma/chroma
TUs do not. The CABAC writer now consumes the per-TU flags, so later lossy
transform-skip trials can be selected at block mode decision time without
reintroducing a separate lossy residual writer.

The first-frame matrix is byte-identical against `vvc-colocated-mode-map-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,996,606 | 0.37 | 0 |
| VVC | qp=24 | 5,727,069 | 0.40 | 0 |

Commands:

```sh
cargo test -p framefinery-codecs vvc --features vvc

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-tu-transform-skip-flags-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-colocated-mode-map-1f.json
```

## VVC Per-TU MRL Index

Checkpoint: `vvc-tu-mrl-index-1f`.

This checkpoint moves the VVC multi-reference-line decision into luma TU
metadata. The current selector still emits only MRL index 0, so the CABAC
bitstream remains unchanged. Keeping the index in the quantized CTU lets future
intra prediction trials choose MRL per block without baking that assumption into
the syntax writer.

The first-frame matrix is byte-identical against `vvc-tu-transform-skip-flags-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,996,606 | 0.37 | 0 |
| VVC | qp=24 | 5,727,069 | 0.40 | 0 |

Commands:

```sh
cargo test -p framefinery-codecs vvc --features vvc

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-tu-mrl-index-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-tu-transform-skip-flags-1f.json
```

## VVC TU Residual Coding Selector

Checkpoint: `vvc-tu-residual-coding-selector-1f`.

This checkpoint moves the remaining VVC luma/chroma TU residual coding choice
out of finalization's lossy/lossless branch and into a shared block-mode
selector. The current selector still chooses transform-skip for lossless TUs and
transformed residual coding for lossy TUs, so the bitstream is unchanged. The
important cleanup is that future lossy transform-skip or per-block tool trials
can now be selected by the same per-TU decision path instead of adding another
standalone lossy path.

The first-frame matrix is byte-identical against `vvc-tu-mrl-index-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,996,606 | 0.37 | 0 |
| VVC | qp=24 | 5,727,069 | 0.40 | 0 |

Commands:

```sh
cargo test -p framefinery-codecs vvc --features vvc

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-tu-residual-coding-selector-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-tu-mrl-index-1f.json
```

## VVC TU Residual Coding Instrumentation

Checkpoint: `vvc-tu-residual-coding-stats`.

This checkpoint extends the compile-gated VVC stats path now that residual
coding is a per-TU decision. Frame stats and CTU-bit JSONL records report
transform-skip and transformed TU counts for luma, Cb, and Cr. Normal builds are
unchanged because the counters are behind `framefinery-codecs/vvc-stats`.

Probe on one 16x16 lossy VVC smoke frame:

| Counter | Total |
|---|---:|
| `luma_tu_count` | 4 |
| `luma_tu_transform_skip_count` | 0 |
| `luma_tu_transformed_count` | 4 |
| `chroma_tu_count` | 4 |
| `chroma_tu_transform_skip_count` | 0 |
| `chroma_tu_transformed_count` | 8 |

Commands:

```sh
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make build VVC_STATS=1

FRAMEFINERY_VVC_STATS=verification/generated/profiling/vvc_residual_coding_stats_probe.jsonl \
FRAMEFINERY_VVC_CTU_BITS=verification/generated/profiling/vvc_residual_coding_ctu_probe.jsonl \
  ./ff encode \
  verification/generated/test_vectors/black_420_16x16_30_1f_yuv420p8.yuv \
  --frames 1 \
  --encode vvc:verification/generated/profiling/vvc_residual_coding_stats_probe.vvc \
  --recon verification/generated/profiling/vvc_residual_coding_stats_probe_recon.yuv \
  --set qp=24

python3 scripts/summarize_encoder_instrumentation.py \
  --vvc-stats probe=verification/generated/profiling/vvc_residual_coding_stats_probe.jsonl \
  --top 8
```

## VVC Luma Partition Selector

Checkpoint: `vvc-luma-partition-selector-1f`.

This checkpoint moves the luma leaf-size decision into the shared
`VvcResidualModeDecisionContext` selector layer. The current policy is still
unchanged: lossy uses the current 8x8 luma leaf target, while lossless uses the
4x4 transform-skip target. The practical effect is that future partition
experiments can be made as mode-selection policy instead of as a separate
lossless/lossy encode path.

The first-frame matrix is byte-identical against
`vvc-tu-residual-coding-selector-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,996,606 | 0.36 | 0 |
| VVC | qp=24 | 5,727,069 | 0.41 | 0 |

Commands:

```sh
cargo test -p framefinery-codecs vvc --features vvc

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-luma-partition-selector-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-tu-residual-coding-selector-1f.json
```

## VVC Per-TU MTS Index

Checkpoint: `vvc-tu-mts-index-1f`.

This checkpoint carries an explicit MTS index beside the luma TU residual
coding decision. The selector still chooses index 0 for every TU because
nonzero MTS transform/reconstruction is not wired yet. Keeping the value in
per-TU metadata removes another hardcoded lossy syntax assumption from the
CABAC emitter, while preserving byte-identical streams until mode selection can
legally choose another transform.

The first-frame matrix is byte-identical against
`vvc-luma-partition-selector-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,996,606 | 0.36 | 0 |
| VVC | qp=24 | 5,727,069 | 0.40 | 0 |

## VVC Luma Syntax Tool Selectors

Checkpoint: `vvc-luma-tool-selectors-1f`.

This checkpoint moves the current zero-valued MRL and MTS choices into explicit
luma TU selector functions. The selected values are still zero for every block,
but TU finalization no longer owns those syntax-tool defaults. Future MRL or
MTS experiments can therefore be gated alongside intra mode and residual coding
selection without creating a separate lossy or lossless encode path.

The first-frame matrix is byte-identical against `vvc-tu-mts-index-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,996,606 | 0.37 | 0 |
| VVC | qp=24 | 5,727,069 | 0.40 | 0 |

## VVC Luma MPM Tie-Breaking

Checkpoint: `vvc-luma-mpm-tiebreak-1f`.

This checkpoint makes VVC luma intra mode selection aware of the existing CABAC
MPM coding shape without tuning a rate-distortion constant. Candidate residual
energy remains the primary key; the exact luma mode syntax-bin count is packed
only into the low six bits, so it breaks residual ties in favor of cheaper MPM
signaling. The syntax-bin helper is shared with the CABAC MPM-list logic so the
mode selector and writer stay aligned.

First-frame six-vector matrix versus `vvc-luma-tool-selectors-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,885,070 | 0.36 | -111,536 |
| VVC | qp=24 | 5,714,171 | 0.41 | -12,898 |

Lossy PSNR moved only within small tie-breaker differences: three rows lost
0.014 to 0.038 dB, two rows gained 0.004 to 0.024 dB, and no reconstruction or
reference-validity rule changed.

## VVC Lossless Chroma Syntax Tie-Breaking

Checkpoint: `vvc-lossless-chroma-syntax-tiebreak-1f`.

This checkpoint adds the same residual-dominant syntax tie-breaker to the
shared chroma intra mode selector, but only when the residual mode is lossless.
The syntax helper mirrors the emitted CABAC shape for derived, explicit, and
CCLM chroma modes, so exact residual-score ties prefer the cheaper chroma mode
syntax. An unrestricted lossy probe increased the six-vector QP24 total by
6,875 bytes, so the selector leaves lossy chroma scoring byte-identical to the
previous checkpoint until a fuller rate-distortion cost is available.

First-frame six-vector matrix versus `vvc-luma-mpm-tiebreak-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.37 | -346 |
| VVC | qp=24 | 5,714,171 | 0.41 | 0 |

## VVC Score Policy Selectors

Checkpoint: `vvc-score-policy-selectors-1f`.

This checkpoint moves the remaining VVC residual-score metric choice into an
explicit selector. Lossless still uses SAD, lossy still uses SSE, and the
lossless-only chroma syntax tie-breaker is now selected through the same mode
decision policy layer. The quantizer no longer directly matches on
`VvcResidualCodingMode` while scoring candidates, which keeps lossy/lossless
differences at block mode selection boundaries instead of as a hidden scoring
branch.

The first-frame matrix is byte-identical against
`vvc-lossless-chroma-syntax-tiebreak-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.36 | 0 |
| VVC | qp=24 | 5,714,171 | 0.40 | 0 |

## VVC CTU Bit Categories

Checkpoint: `vvc-ctu-category-stats-1f`.

This checkpoint extends the compile-gated VVC CTU JSONL sink with category
counters for partition, luma mode, chroma mode, residual, intra-block-copy,
inter, palette, and other syntax. The counters are syntax-bin costs derived
from the CABAC semantic dump, while `total_symbol_bits` remains the final
arithmetic-coded CTU bit length. The summarizer now normalizes category
percentages against category totals when those domains differ, so VVC
syntax-bin categories do not report impossible shares above 100%.

The instrumented first-frame six-vector matrix was byte-identical against
`vvc-score-policy-selectors-1f` for VVC lossless and QP24 lossy. The current
VVC residual path remains CTU-quantization bound and residual-syntax dominated:

| Measurement | Value |
|---|---:|
| CTU quantize stage share | 89.0% |
| Frame entropy write stage share | 10.2% |
| Residual syntax-bin share | 93.5% |
| Luma-mode syntax-bin share | 2.5% |
| Partition syntax-bin share | 2.1% |

## VVC Transform-Skip Reconstruction Source

Checkpoint: `vvc-ts-recon-from-coeffs-1f`.

This checkpoint removes a hidden assumption from the unified VVC residual TU
finalizer. Transform-skipped luma and chroma TUs now rebuild their residual
samples from the encoded DC plus first-4x4 AC coefficient payload before
updating the encoder reconstruction, rather than copying the full original
residual buffer. Current lossless residual leaves are still 4x4, so the
reconstructed samples and bitstreams are unchanged. For future lossy
transform-skip trials on larger leaves, the finalizer now models the same
coefficient subset the entropy path can actually signal.

The first-frame six-vector matrix was byte-identical against
`vvc-ctu-category-stats-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.39 | 0 |
| VVC | qp=24 | 5,714,171 | 0.46 | 0 |

## VVC TU Coding Decision Unification

Checkpoint: `vvc-tu-decision-unified-1f`.

This checkpoint groups the remaining per-TU luma and chroma tool selections
into explicit coding-decision structs. The CTU quantizer now asks block mode
selection for one luma decision carrying residual coding, MRL index, and MTS
index, and one chroma decision carrying residual coding. The current policy is
unchanged: lossless TUs still choose transform skip, lossy TUs still choose
transformed residuals, and MRL/MTS stay at index 0 until their predictors and
transforms are wired. The important cleanup is that future lossy-only tool
trials can be gated at block mode selection without forking the residual path.

The first-frame six-vector matrix was byte-identical against
`vvc-ts-recon-from-coeffs-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.36 | 0 |
| VVC | qp=24 | 5,714,171 | 0.40 | 0 |

## VVC Residual Tail Energy Instrumentation

Checkpoint: `vvc-residual-tail-stats`.

This checkpoint adds compile-gated residual-energy counters to the VVC stats
path. Normal builds and bitstreams are unchanged; with
`framefinery-codecs/vvc-stats`, each quantized CTU now reports total residual
SSE, the portion covered by the currently coded first-4x4 coefficient subset,
and the uncoded tail outside that subset for luma and chroma.

The first-frame matrix was byte-identical against
`vvc-tu-decision-unified-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.42 | 0 |
| VVC | qp=24 | 5,714,171 | 0.45 | 0 |

Probe on the first SceneComposition 4:2:0 frame, VVC QP24:

| Component | Total SSE | First4x4 SSE | Tail SSE | Tail share |
|---|---:|---:|---:|---:|
| luma | 712,894,918 | 169,371,320 | 543,523,598 | 76.2% |
| chroma | 37,585,004 | 37,585,004 | 0 | 0.0% |

The same probe still shows residual syntax as the dominant CTU category:
1,701,400 residual syntax-bin bits, or 88.3% of categorized syntax-bin cost.
The largest CTUs spend about 97% of categorized syntax bins on residuals. This
confirms that the next VVC intra feature work should target wider or staged
coefficient coding for luma before more mode-search constants.

Commands:

```sh
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make build VVC_STATS=1
FRAMEFINERY_VVC_STATS=verification/generated/profiling/vvc_residual_tail_stats_probe.jsonl \
FRAMEFINERY_VVC_CTU_BITS=verification/generated/profiling/vvc_residual_tail_ctu_probe.jsonl \
  ./ff encode \
  verification/generated/test_vectors/aomctc_b2_SceneComposition_1_420_1920x1080_15_1f_yuv420p8.yuv \
  --frames 1 \
  --encode vvc:verification/generated/profiling/vvc_residual_tail_probe.vvc \
  --recon verification/generated/profiling/vvc_residual_tail_probe_recon.yuv \
  --set qp=24

python3 scripts/summarize_encoder_instrumentation.py \
  --vvc-stats scene420/framefinery=verification/generated/profiling/vvc_residual_tail_stats_probe.jsonl \
  --top 12

python3 scripts/summarize_encoder_instrumentation.py \
  --sb-bits scene420/framefinery=verification/generated/profiling/vvc_residual_tail_ctu_probe.jsonl \
  --top 5

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-residual-tail-stats-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-tu-decision-unified-1f.json
```

## VVC 8x8 Residual Context State

Checkpoint: `vvc-pass1-8x8-context-1f`.

This checkpoint removes another first-4x4 assumption from VVC residual context
derivation. `VvcResidualPass1State` can now track pass-1 coefficient presence
and template magnitudes across the current production 8x8 luma TU footprint,
while the emitted coefficient scan still remains the existing first-4x4 subset.
That means the normal bitstreams are unchanged, but the context model is ready
for a later grouped-subblock scan to set neighbour state outside the first
subblock.

The first-frame six-vector matrix was byte-identical against
`vvc-residual-tail-stats-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.42 | 0 |
| VVC | qp=24 | 5,714,171 | 0.46 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc_residual_pass1_state_tracks_8x8_neighbour_coefficients --features vvc
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale"

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-pass1-8x8-context-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-residual-tail-stats-1f.json

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

## VVC Grouped 8x8 Residual Syntax

Checkpoint: `vvc-grouped-8x8-syntax-1f`.

This checkpoint wires the generic VVC luma coefficient path for grouped 8x8
residual syntax. It adds last-significant coefficient suffix bins, 4x4 subblock
scan grouping inside 8x8 TUs, reverse subblock traversal, and `sb_coded_flag`
emission for intermediate coded subblocks. The production quantized TU payloads
still feed the existing first-4x4 coefficient subset, so normal bitstreams are
unchanged. This is a syntax prerequisite for later coding wider luma residual
coefficients from the unified TU mode decision.

The first-frame six-vector matrix was byte-identical against
`vvc-pass1-8x8-context-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.35 | 0 |
| VVC | qp=24 | 5,714,171 | 0.40 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc_residual_symbol_stream_supports_grouped_8x8_luma_scan --features vvc
cargo test -p framefinery-codecs vvc_residual_ac_symbol_stream_uses_spec_context_derivations --features vvc
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-grouped-8x8-syntax-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-pass1-8x8-context-1f.json

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

## VVC Luma Coefficient Storage

Checkpoint: `vvc-luma-coeff-storage-1f`.

This checkpoint widens VVC luma TU coefficient storage from the first 4x4 AC
subset to a compact 8x8-capable payload while keeping chroma at its 4x4 AC
shape. The CTU body now calls generalized luma residual emission helpers, and
the inverse transform / transform-skip reconstruction derive luma coefficient
positions from the coded coefficient extent instead of a hard-coded 4x4 shape.

The default luma quantizer still selects the legacy first-subblock projection.
A direct DCT 8x8 candidate is wired as an implementation building block, but it
is not selected by default because the initial matrix increased bitrate
substantially and lowered high-depth PSNR. That keeps this checkpoint as a
non-regressive plumbing step for a future rate/distortion selector rather than
a quality-mode fork.

The first-frame six-vector matrix was byte-identical against
`vvc-grouped-8x8-syntax-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.35 | 0 |
| VVC | qp=24 | 5,714,171 | 0.39 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-luma-coeff-storage-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-grouped-8x8-syntax-1f.json

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

## VVC Gated Luma DCT Candidate

Checkpoint: `vvc-luma-dct-selector-gated-1f`.

This checkpoint adds the implementation pieces for a per-8x8 luma AC candidate
selector: a direct DCT-II coefficient path, reconstructed-residual SSE scoring,
and a QP/bit-depth scaled coefficient-cost estimate. The production selector is
compile-time disabled by `VVC_ENABLE_EXPERIMENTAL_LUMA_DCT_COEFF_SELECTION`
because enabling it exposed a residual syntax mismatch against VTM.

The enabled trial was useful but not committable as production behavior:
`smoke/checker_420` failed VTM decode with `Expecting a terminating bit`, and
the first local SceneComposition vector decoded with a reconstruction checksum
mismatch. The one-frame matrix from that enabled trial improved lossy PSNR by
about 0.4 to 1.5 dB, but increased total lossy bytes by about 282 KiB and
dropped FPS modestly. The next residual feature step should therefore fix
multi-subblock residual syntax/reference compatibility before the selector is
allowed to pick the DCT payload.

With the selector gated off, the first-frame six-vector matrix was
byte-identical against `vvc-luma-coeff-storage-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.35 | 0 |
| VVC | qp=24 | 5,714,171 | 0.39 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale"

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-luma-dct-selector-gated-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-luma-coeff-storage-1f.json

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

## VVC Residual Coding Policy

Checkpoint: `vvc-residual-policy-unified-1f`.

This checkpoint makes the unified VVC residual path explicit by introducing a
single `VvcResidualCodingPolicy` for CTU quantization. The policy carries the
residual-mode context, luma leaf-size selection, residual score metric, chroma
syntax tie-breaker, intra-mode selection, and per-TU coding decisions. Lossless
and lossy still select different tools where needed, but those differences now
live at block-mode selection boundaries instead of being pulled piecemeal by
the quantizer.

The test-only residual reconstruction helper was also updated to consume the
per-TU transform-skip flags. It now reconstructs planar 4:2:0, 4:2:2, and
4:4:4 residual frames through the same transformed or transform-skip metadata
used by the encoder path.

The first-frame six-vector matrix was byte-identical against
`vvc-luma-dct-selector-gated-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.35 | 0 |
| VVC | qp=24 | 5,714,171 | 0.39 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-residual-policy-unified-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-luma-dct-selector-gated-1f.json

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

## VVC Progressive Residual Contexts

Checkpoint: `vvc-progressive-residual-contexts-1f`.

This checkpoint changes production VVC coefficient emission to derive residual
CABAC contexts from a progressively populated pass-1 coefficient state, matching
decoder-order residual traversal. The symbolic residual stream remains
test-only, while production now uses a compact delayed-bypass symbol queue for
second-pass remainders and bypass-coded levels.

The active default path is byte-identical against
`vvc-luma-dct-selector-gated-1f`, so this is a compatibility cleanup for larger
transformed intra-block experiments rather than a tuned coding change:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.35 | 0 |
| VVC | qp=24 | 5,714,171 | 0.39 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-progressive-residual-contexts-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-luma-dct-selector-gated-1f.json

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

## VVC Stored Residual Emitter Unification

Checkpoint: `vvc-stored-residual-unified-1f`.

This checkpoint removes the last production CTU-body branch that chose separate
stored-coefficient emitter wrappers for transformed and transform-skipped VVC
TUs. The luma and chroma CTU emitters now pass the selected TU residual coding
mode as data into one stored-coefficient entry point per component family. This
keeps lossy/lossless behavior gated at block-mode selection while sharing the
same residual syntax implementation.

The change is intentionally byte-neutral against
`vvc-luma-dct-selector-gated-1f`:

| Codec | Mode | Total bytes | Byte delta |
|---|---|---:|---:|
| VVC | lossless | 5,884,724 | 0 |
| VVC | qp=24 | 5,714,171 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-stored-residual-unified-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-luma-dct-selector-gated-1f.json
```

## VVC CU-Level MTS Hook

Checkpoint: `vvc-cu-mts-hook-1f`.

This checkpoint moves the default `mts_idx` syntax hook out of residual
coefficient emission and into the luma CU body after residual coding, matching
the VTM `cu_residual()` order. Nonzero MTS remains asserted off until matching
forward/inverse transform support is implemented, but the syntax gate now has
the right owner: explicit intra MTS, transformed luma TU, non-DC residual, and
MTS-sized CU.

The default product configuration does not enable explicit MTS, so the change
is byte-neutral against `vvc-stored-residual-unified-1f`:

| Codec | Mode | Total bytes | Byte delta |
|---|---|---:|---:|
| VVC | lossless | 5,884,724 | 0 |
| VVC | qp=24 | 5,714,171 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-cu-mts-hook-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-stored-residual-unified-1f.json
```

## VVC Integer Directional Seed

Checkpoint: `vvc-integer-directional-seed-1f`.

This checkpoint changes the VVC luma directional source-seed gradient scan from
per-sample floating-point accumulation to integer accumulation followed by a
single floating-point angle calculation. The selected orientation is unchanged;
the update removes unnecessary FP work from the hot intra candidate-generation
path and keeps the later candidate evaluation identical.

The first-frame six-vector matrix is byte-neutral against `vvc-cu-mts-hook-1f`:

| Codec | Mode | Total bytes | Byte delta |
|---|---|---:|---:|
| VVC | lossless | 5,884,724 | 0 |
| VVC | qp=24 | 5,714,171 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-integer-directional-seed-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-cu-mts-hook-1f.json
```

## VVC Luma Mode Cell Map

Checkpoint: `vvc-luma-mode-cell-map-1f`.

This checkpoint replaces per-sample luma intra-mode neighbour maps with 4x4
cell maps in both the quantization search state and CABAC MPM state. Current
VVC luma leaves are aligned to at least 4x4, so left/above and chroma
co-located mode queries see the same selected modes while mark operations write
up to 16x fewer entries.

The first-frame six-vector matrix is byte-neutral against
`vvc-integer-directional-seed-1f` and shows small fps improvements on several
rows:

| Codec | Mode | Total bytes | Byte delta |
|---|---|---:|---:|
| VVC | lossless | 5,884,724 | 0 |
| VVC | qp=24 | 5,714,171 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-luma-mode-cell-map-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-integer-directional-seed-1f.json
```

## VVC Split Neighbour Cell Maps

Checkpoint: `vvc-neighbour-cell-map-1f`.

This checkpoint extends the cell-map approach to the remaining split-context
neighbour state. Luma split metadata now uses 4x4 cells, and chroma split
metadata uses 2x2 chroma-sample cells so 4:2:0 boundary leaves still keep
distinct context information. This removes per-sample neighbour writes from
both lossy and lossless VVC coding-tree walks without changing syntax or
reconstruction.

The first-frame six-vector matrix is byte-neutral against
`vvc-luma-mode-cell-map-1f`:

| Codec | Mode | Total bytes | Byte delta |
|---|---|---:|---:|
| VVC | lossless | 5,884,724 | 0 |
| VVC | qp=24 | 5,714,171 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-neighbour-cell-map-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-luma-mode-cell-map-1f.json
```

## VVC Finalized Residual Blocks

Checkpoint: `vvc-finalized-residual-blocks-1f`.

This checkpoint removes the remaining duplicated luma/chroma TU finalizer
branches that directly looked like lossy-vs-lossless paths. The finalizers now
consume the per-block `VvcTuResidualCodingMode` selected by block mode policy,
build a common finalized residual block, reconstruct it through the matching
transform-skip or transformed helper, and then fill the visible reconstruction.
This keeps lossy-specific and lossless-specific behavior at TU mode selection
boundaries instead of as independent finalization paths.

The reference-incompatible experimental 8x8 luma DCT coefficient selector
remains compile-time disabled. The associated residual syntax mismatch still
needs to be fixed before that candidate can be selected by production mode
decision.

The first-frame six-vector matrix is byte-neutral against
`vvc-neighbour-cell-map-1f`:

| Codec | Mode | Total bytes | Byte delta |
|---|---|---:|---:|
| VVC | lossless | 5,884,724 | 0 |
| VVC | qp=24 | 5,714,171 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-finalized-residual-blocks-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-neighbour-cell-map-1f.json
```

## VVC Progressive Rice State

Checkpoint: `vvc-progressive-rice-remcap-1f`.

This checkpoint tightens the shared VVC residual syntax path without changing
mode decisions. Go-Rice parameter derivation now uses the progressively decoded
residual state for both second-pass remainders and bypass-coded coefficients,
matching the state visible to the decoder instead of consulting final
coefficients. The CABAC EP absolute-remainder helper also now applies VVC's
`maxLog2TrDynamicRange` prefix cap.

An attempted lossless luma leaf-size unification to 8x8 exposed a remaining
reference incompatibility in 8x8 transform-skip coefficient syntax:
VTM rejected the stream at slice termination. Lossless luma therefore remains
gated to 4x4 leaves at block mode selection while lossy luma keeps the 8x8
leaf path. This keeps the unified finalizer/syntax machinery validated without
weakening reference-decoder checks.

The first-frame six-vector matrix is byte-neutral against
`vvc-finalized-residual-blocks-1f`:

| Codec | Mode | Total bytes | Byte delta |
|---|---|---:|---:|
| VVC | lossless | 5,884,724 | 0 |
| VVC | qp=24 | 5,714,171 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-progressive-rice-remcap-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-finalized-residual-blocks-1f.json
```

## VVC Last-Significant Suffix Order

Checkpoint: `vvc-last-sig-suffix-order-1f`.

This checkpoint fixes the VVC residual emitter order for last-significant
coefficient positions that require suffix bins. The direct CABAC path and the
test-only symbolic stream now emit X and Y prefixes first, then X and Y suffixes,
matching VTM's `last_sig_coeff()` ordering. The current default product path is
byte-neutral because lossless luma still selects 4x4 leaves and lossy luma does
not change mode decisions, but the fix makes 8x8 residual syntax
reference-compatible.

After this fix, a trial that changed lossless luma leaves from 4x4 to 8x8 passed
both VVC smoke and high-depth reference validation. It was not kept as the
default because the first-frame six-vector lossless total rose from 5,884,724
bytes to 6,231,140 bytes, a +346,416 byte regression, while lossy stayed
byte-identical. A future 4x4/8x8 selector should therefore be rate-aware rather
than a global leaf-size switch.

The active first-frame six-vector matrix is byte-neutral against
`vvc-progressive-rice-remcap-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.35 | 0 |
| VVC | qp=24 | 5,714,171 | 0.40 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-last-sig-suffix-order-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-progressive-rice-remcap-1f.json
```

## VVC MRL Syntax Capability

Checkpoint: `vvc-mrl-syntax-1f`.

This checkpoint wires the CABAC emission shape for nonzero VVC luma
multi-reference-line indices. The production selector still returns MRL index
0 because luma prediction has not yet been shifted to the additional reference
lines, but the CTU body can now encode VTM's supported `MULTI_REF_LINE_IDX`
values `[0, 1, 2]` instead of asserting on nonzero values. This keeps MRL as a
future block-mode-selection tool without a separate lossy/lossless path.

The new unit coverage sets a below-top-line luma TU to indices 0, 1, and 2 and
checks that the CABAC bitstreams differ. Normal encoding is byte-neutral against
`vvc-last-sig-suffix-order-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.36 | 0 |
| VVC | qp=24 | 5,714,171 | 0.39 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc_luma_mrl_syntax_supports_nonzero_reference_lines --features vvc
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-mrl-syntax-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-last-sig-suffix-order-1f.json
```

## VVC MTS Syntax Capability

Checkpoint: `vvc-mts-syntax-1f`.

This checkpoint wires the CABAC emission shape for VVC explicit intra MTS
indices. The CTU body now accepts VTM's non-transform-skip MTS types:
`DCT2_DCT2=0`, `DST7_DST7=2`, `DCT8_DST7=3`, `DST7_DCT8=4`, and
`DCT8_DCT8=5`. `SKIP=1` remains represented by the existing transform-skip
flag instead of the post-residual `mts_idx` syntax.

The production selector still returns `DCT2_DCT2` until matching forward and
inverse non-DCT transforms are available, so normal encoding should remain
byte-neutral against `vvc-mrl-syntax-1f`. The added unit coverage forces each
non-default MTS index through a 16x16 luma TU with AC coefficients and checks
that the CABAC bitstreams differ.

The first-frame six-vector matrix is byte-neutral against `vvc-mrl-syntax-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.36 | 0 |
| VVC | qp=24 | 5,714,171 | 0.39 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc_luma_mts_syntax_supports_non_default_transform_indices --features vvc
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-mts-syntax-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-mrl-syntax-1f.json
```

## VVC Chroma Sample Decision Source

Checkpoint: `vvc-chroma-sample-from-tu-decision-1f`.

This checkpoint removes the last direct residual-mode branch from VVC residual
CTU output finalization. The legacy sampled chroma fields now derive their
lossless-versus-quantized value from the finalized chroma TU transform-skip
metadata, which is selected by `VvcChromaTuCodingDecision`. That keeps even the
compatibility fields behind the unified per-block decision path instead of
checking the global lossy/lossless mode.

The first-frame six-vector matrix is byte-neutral against `vvc-mts-syntax-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.36 | 0 |
| VVC | qp=24 | 5,714,171 | 0.39 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-chroma-sample-from-tu-decision-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-mts-syntax-1f.json
```

## VVC Luma DCT Selector Enabled

Checkpoint: `vvc-luma-dct-selector-enabled-1f`.

This checkpoint promotes the luma 8x8 DCT coefficient candidate from a disabled
implementation hook to the default lossy selector candidate. Earlier trials
failed VTM validation, but the later residual suffix-order and grouped-syntax
fixes made the wider luma coefficient payload reference-compatible. The
selector remains a compile-time constant so it can be bisected quickly, but the
normal build now evaluates the legacy first-subblock payload against the DCT
payload and chooses the better reconstructed-residual score.

Lossless remains byte-neutral because the block-mode selector still chooses
transform skip there. Lossy first-frame output trades more bytes and a small
speed hit for PSNR gains on every row:

| Vector | Format | Bytes delta | FPS delta | PSNR delta |
|---|---|---:|---:|---:|
| Scene 420 | yuv420p8 | +52,942 | -0.04 | +1.263 |
| Scene 422 | yuv422p8 | +53,012 | -0.02 | +0.936 |
| Wayland RGB | gbrp8 | +89,544 | -0.00 | +0.700 |
| Mission 420 | yuv420p10le | +55,876 | -0.08 | +0.469 |
| Mission 422 | yuv422p10le | +18,301 | -0.03 | +0.418 |
| Mission 444 | yuv444p10le | +13,202 | -0.02 | +0.249 |

First-frame six-vector totals against
`vvc-chroma-sample-from-tu-decision-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.36 | 0 |
| VVC | qp=24 | 5,997,048 | 0.36 | +282,877 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-luma-dct-selector-enabled-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-chroma-sample-from-tu-decision-1f.json
```

## VVC MTS Transform Plumbing

Checkpoint: `vvc-mts-transform-plumbing-1f`.

This checkpoint passes the selected luma `mts_index` through transformed luma
quantization and inverse reconstruction. The selector still returns
`DCT2_DCT2=0`; this checkpoint made transform choice reach the transform
boundary from the unified block-mode decision instead of staying as syntax-only
metadata.

The first-frame six-vector matrix is byte-neutral against
`vvc-luma-dct-selector-enabled-1f`:

| Codec | Mode | Total bytes | FPS | Byte delta |
|---|---|---:|---:|---:|
| VVC | lossless | 5,884,724 | 0.36 | 0 |
| VVC | qp=24 | 5,997,048 | 0.36 | 0 |

Commands:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"

make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required

make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-mts-transform-plumbing-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-luma-dct-selector-enabled-1f.json
```

## VVC RGB 4:4:4 Signaling And MRL Plumbing

Checkpoint: `vvc-rgb444-mrl-plumbing`.

This cleanup fixes the VVC 4:2:2/4:4:4 profile signaling and makes the RGB
path explicit. Residual 4:2:2/4:4:4 streams now select VVC 4:4:4-capable
profiles (`general_profile_idc` 33 through 10-bit, 34 above 10-bit), including
planar `gbrp8` input. `gbrp8` still flows as full-resolution green, blue, red
planes; VVC/VTM disallow identity matrix coefficient `0` when
`sps_chroma_format_idc=3`, so the VUI signals full-range sRGB-compatible
primaries/transfer with matrix coefficient `2` left unspecified.

The validation/test plumbing stays byte-oriented:

- Native `gbrp8` compares source, internal reconstruction, and reference
  reconstruction directly as planar GBR.
- Legacy packed `rgb24` remains a shared driver conversion boundary and is
  only normalized in validation when a reference decoder emits planar GBR.
- The VVC reference-comparison script repacks legacy packed `rgb24` to planar
  GBR before invoking VTM; native `gbrp8` is passed through unchanged.

Two reference-compatibility bugs were fixed while validating the planar RGB
path:

- Positive VVC angular prediction now replicates the formal main-reference
  extension instead of reading real pixels beyond the VTM reference span.
- Luma MPM remaining-mode coding now uses the correct circular angular
  threshold at the 2/63 boundary.
- Nonzero-MRL luma mode syntax now follows VTM's rule: MRL modes must be
  MPM-coded, skip `IntraLumaMpmFlag`, and cannot use planar prediction.

MRL is now syntax-enabled and selected conservatively. The luma prediction,
final reconstruction, and CABAC syntax plumbing accept an explicit MRL index.
The predictor now builds shifted reference lines for angular/H/V nonzero-MRL
trials, and the quantizer can score DC and angular/H/V nonzero-MRL candidates
when they are eligible for MPM-coded MRL syntax. The block-mode search now keeps
frame-wide luma mode neighbours, matching the CABAC neighbour shape across CTU
boundaries. MRL remains gated on the CTU top line, where higher reference lines
are unavailable.

Validation:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"
python3 -m py_compile \
  scripts/compare_reference_compression.py \
  scripts/run_validation_set.py \
  scripts/benchmark_encode_matrix.py \
  scripts/generate_test_vectors.py \
  scripts/convert_rgb24_to_gbrp8.py \
  scripts/capture_wayland_portal_rgb.py
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
./ff encode verification/generated/rgb_signalling_check/wayland_crop64_gbrp8.rgb \
  --video 64x64:gbrp8 --frames 1 --fps 30 \
  --encode vvc:verification/generated/rgb_signalling_check/wayland_crop64_gbrp8_check.vvc \
  --recon verification/generated/rgb_signalling_check/wayland_crop64_gbrp8_check.recon.rgb \
  --set lossless
python3 scripts/reference_tools.py decode --codec vvc \
  --bitstream verification/generated/rgb_signalling_check/wayland_crop64_gbrp8_check.vvc \
  --output verification/generated/rgb_signalling_check/wayland_crop64_gbrp8_check.reference.rgb \
  --no-build
cmp -s \
  verification/generated/rgb_signalling_check/wayland_crop64_gbrp8_check.recon.rgb \
  verification/generated/rgb_signalling_check/wayland_crop64_gbrp8_check.reference.rgb
```

## VVC Angular MRL, Frame-Wide Modes, And Guarded MTS

Checkpoint: `vvc-mrl-rd-score-1f`.

This checkpoint finishes the first validated slice of the remaining VVC intra
tool gaps:

- Angular/H/V MRL prediction uses shifted reference-line sampling instead of
  reusing the base reference line.
- MRL selection is active below the CTU top line and is gated by the same MPM
  eligibility required by VTM syntax.
- Luma mode neighbours are tracked in a frame-wide 4x4-cell map and reused
  across CTUs, so the selector can make the same left/top MPM decision as the
  CABAC writer.
- Lossy MRL selection scores the quantized candidate it will actually emit:
  reconstructed residual SSE plus coefficient and MRL syntax cost. Lossless
  keeps the cheaper raw residual score.
- Explicit intra MTS signaling is enabled for lossy residual streams, and
  luma MTS indices `2..=5` are carried through quantization, inverse transform,
  reconstruction, and syntax tests.
- Non-DCT2 MTS production selection remains disabled. A checker-smoke probe
  with active non-DCT2 MTS produced a VTM reconstruction checksum mismatch even
  though VTM accepted the bitstream. Keep the gate closed until the transform
  and coefficient constraints are proven VTM-exact.

Validation:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc_luma_mrl --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

Strict VTM validation passed for all smoke rows and all high-depth smoke rows.
The high-depth lossless byte counts were unchanged from the previous validated
checkpoint: 322, 409, 555, 475, 594, and 768 bytes.

First-frame six-vector matrix versus `vvc-mts-transform-plumbing-1f`:

| Mode | Previous bytes | Current bytes | Byte delta | Previous FPS | Current FPS |
|---|---:|---:|---:|---:|---:|
| lossless | 5,884,724 | 5,856,819 | -27,905 | 0.36 | 0.33 |
| qp=24 | 5,997,048 | 5,541,589 | -455,459 | 0.36 | 0.29 |

The RD-aware MRL scoring step by itself was byte-neutral for lossless and
reduced every lossy row versus `vvc-frame-luma-mode-state-1f`:

| Mode | Vector | Bytes | FPS | PSNR | Byte delta |
|---|---|---:|---:|---:|---:|
| qp=24 | SceneComposition_1_420 | 233,302 | 0.49 | 26.131 | -570 |
| qp=24 | SceneComposition_1_422 | 311,833 | 0.41 | 26.170 | -2,434 |
| qp=24 | screen_wayland_activity_rgb | 809,785 | 0.20 | 25.393 | -11,496 |
| qp=24 | MissionControlClip1_420 | 823,645 | 0.32 | 16.850 | -9,961 |
| qp=24 | MissionControlClip1_422 | 1,271,391 | 0.28 | 16.074 | -22,444 |
| qp=24 | MissionControlClip1_444 | 2,091,633 | 0.23 | 15.619 | -35,493 |

Command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-mrl-rd-score-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-frame-luma-mode-state-1f.json
```

## VVC Intra Parity Checklist

Checkpoint in progress: `vvc-intra-parity`.

The AV2 encoder is still ahead of the VVC intra encoder in both implemented
mode surface and mode-selection efficiency. The VVC residual path is now mostly
unified across lossy/lossless, bit depth, and chroma sampling, so new intra work
should land as selectable tools inside that path rather than as separate
format-specific encoders.

Current gaps to close against AV2-style intra behavior:

1. Reuse finalized RD decisions. VVC lossy MRL scoring quantizes candidate
   residuals; TU finalization must reuse the selected quantized block instead
   of recomputing it.
2. Rank more predictor choices by emitted/reconstructed cost. AV2 uses sampled
   RD-style scores for lossy intra decisions, while VVC still selects most
   luma/chroma predictors from raw residual energy.
3. Integrate screen-content mode choice into the residual path. VVC has
   palette, IBC, transform-skip residual, and BDPCM helpers, but they remain a
   separate 4:4:4 path instead of competing against residual intra decisions.
4. Finish safe non-DCT2 MTS selection. Syntax, transform plumbing, a
   one-candidate selector, and stats counters exist. Production selection stays
   disabled because the validated selector probes were not yet rate/FPS
   positive on the six-vector first-frame matrix.
5. Continue refining lossy transform-skip/BDPCM selection. Lossy transform-skip
   and regular luma/chroma BDPCM now compete inside the residual path, but the
   selector should still learn better rate pruning from the stats traces.
6. Add VVC-only intra tools that are not AV2 analogues but are needed for
   parity in practice. MIP/ISP/LFNST CABAC contexts and SPS flags are now
   plumbed; active MIP, active ISP, and LFNST transform selection still need
   predictor/transform ownership before they can compete in production.
7. Improve partition/transform-size decisions. Current VVC residual coding is
   dominated by 8x8 lossy leaves and 4x4 lossless leaves; AV2 has more
   effective rate-aware leaf and block choices.
8. Keep instrumentation compile-gated. Per-CTU bit categories, stage timing,
   residual energy, and candidate counters are useful for this work but must
   stay out of normal product builds.

### VVC MRL RD Residual Reuse

This checkpoint implements item 1. The MRL selector now returns the selected
reference-line index plus the finalized quantized luma residual block when the
candidate was scored through lossy RD. TU finalization consumes that cached
block, preserving the selected coefficients and reconstruction while avoiding a
second quantization pass for MRL-eligible transformed TUs. Lossless MRL scoring
continues to use the cheaper raw residual score and does not cache a residual.

Validation:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc_luma_mrl --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

Strict VTM validation passed for all smoke and high-depth smoke rows. The
six-vector first-frame matrix was byte-identical against
`vvc-mrl-rd-score-1f`:

| Mode | Bytes | FPS | Byte delta |
|---|---:|---:|---:|
| lossless | 5,856,819 | 0.33 | 0 |
| qp=24 | 5,541,589 | 0.30 | 0 |

Per-row VVC QP24 deltas versus `vvc-mrl-rd-score-1f`:

| Vector | Bytes | FPS | PSNR | Byte delta |
|---|---:|---:|---:|---:|
| SceneComposition_1_420 | 233,302 | 0.51 | 26.131 | 0 |
| SceneComposition_1_422 | 311,833 | 0.44 | 26.170 | 0 |
| screen_wayland_activity_rgb | 809,785 | 0.21 | 25.393 | 0 |
| MissionControlClip1_420 | 823,645 | 0.34 | 16.850 | 0 |
| MissionControlClip1_422 | 1,271,391 | 0.28 | 16.074 | 0 |
| MissionControlClip1_444 | 2,091,633 | 0.24 | 15.619 | 0 |

Command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-mrl-rd-cache-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-mrl-rd-score-1f.json
```

### VVC Frame-Aligned MPM And CTU-Bit Stats

This checkpoint fixes a nonzero-MRL robustness issue found while collecting a
fresh first-frame stats trace. The residual quantizer and CABAC writer must use
the same luma-neighbor availability when deciding whether a luma mode is MPM
coded. The quantizer now mirrors the CABAC CTU-top rule for above-neighbor mode
availability, so nonzero MRL is not selected from a neighbor context that the
writer will later suppress.

The compile-gated CTU-bit sink also now keeps frame-level CABAC context and
neighbour state across CTUs. This fixes the stats path for MRL-enabled streams:
per-CTU bit-category rows can be emitted without encoding each CTU as an
isolated picture. Normal builds are unaffected because this state exists only
under `vvc-stats`.

Validation:

```sh
cargo fmt
cargo check -p framefinery-codecs --features "vvc vvc-stats"
cargo test -p framefinery-codecs vvc_luma_mrl --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
cargo check --workspace \
  --features "codec-av2 codec-vvc filter-pattern filter-identity filter-crop filter-scale framefinery-codecs/vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

The stats probe that previously panicked now completes:

```sh
FRAMEFINERY_VVC_STATS=verification/generated/profiling/vvc_mpm_aligned_scene420_lossy_1f.jsonl \
FRAMEFINERY_VVC_CTU_BITS=verification/generated/profiling/vvc_mpm_aligned_scene420_lossy_1f_ctu.jsonl \
./ff encode \
  verification/generated/test_vectors/aomctc_b2_SceneComposition_1_420_1920x1080_15_1f_yuv420p8.yuv \
  --video 1920x1080:yuv420p8 --frames 1 --fps 15 \
  --encode vvc:verification/generated/profiling/vvc_mpm_aligned_scene420_lossy_1f.vvc \
  --recon verification/generated/profiling/vvc_mpm_aligned_scene420_lossy_1f_recon.yuv \
  --set qp=24
```

Probe result: 233,233 encoded bytes, PSNR 26.126 dB, and VVC stats reported
`ctu_quantize` at about 2.06 s versus `frame_entropy_write` at about 0.12 s on
the first SceneComposition 4:2:0 frame. Candidate/residual work remains the
dominant intra bottleneck.

First-frame six-vector matrix versus `vvc-mrl-rd-cache-1f`:

| Mode | Previous bytes | Current bytes | Byte delta | Previous FPS | Current FPS |
|---|---:|---:|---:|---:|---:|
| lossless | 5,856,819 | 5,856,894 | +75 | 0.33 | 0.34 |
| qp=24 | 5,541,589 | 5,530,447 | -11,142 | 0.30 | 0.30 |

Per-row VVC QP24 deltas:

| Vector | Bytes | FPS | PSNR | Byte delta |
|---|---:|---:|---:|---:|
| SceneComposition_1_420 | 233,291 | 0.51 | 26.126 | -11 |
| SceneComposition_1_422 | 313,075 | 0.40 | 26.159 | +1,242 |
| screen_wayland_activity_rgb | 813,021 | 0.21 | 25.371 | +3,236 |
| MissionControlClip1_420 | 821,385 | 0.34 | 16.851 | -2,260 |
| MissionControlClip1_422 | 1,269,904 | 0.30 | 16.080 | -1,487 |
| MissionControlClip1_444 | 2,079,771 | 0.23 | 15.649 | -11,862 |

Command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-intra-parity-mpm-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-mrl-rd-cache-1f.json
```

### VVC Luma RD Shortlist

Checkpoint: `vvc-luma-rd-pareto-1f`.

This checkpoint starts item 2 by adding an output-aware lossy luma mode
refinement pass. The regular predictor search still supplies the candidate
ordering, but the best raw mode is now compared against the next shortlisted
mode after quantization and inverse reconstruction. The selector is conservative
and only switches when the candidate improves estimated residual rate without
raising reconstructed-residual distortion, or improves distortion without
raising estimated residual rate. The selected quantized residual is cached and
reused by TU finalization.

This is useful plumbing but not yet a strong production tradeoff by itself. On
the first-frame six-vector matrix, lossy total size improved by only 1,004 bytes
versus `vvc-intra-parity-mpm-1f`, while several rows lost noticeable FPS.

| Mode | Current bytes | FPS |
|---|---:|---:|
| lossless | 5,857,190 | 0.32 |
| qp=24 | 5,529,443 | 0.25 |

Per-row VVC QP24 deltas versus `vvc-intra-parity-mpm-1f`:

| Vector | Bytes | FPS | PSNR | Byte delta |
|---|---:|---:|---:|---:|
| SceneComposition_1_420 | 231,229 | 0.41 | 26.183 | -2,062 |
| SceneComposition_1_422 | 311,019 | 0.36 | 26.194 | -2,056 |
| screen_wayland_activity_rgb | 812,752 | 0.18 | 25.391 | -269 |
| MissionControlClip1_420 | 825,674 | 0.26 | 16.834 | +4,289 |
| MissionControlClip1_422 | 1,272,654 | 0.23 | 16.070 | +2,750 |
| MissionControlClip1_444 | 2,076,115 | 0.19 | 15.665 | -3,656 |

Command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-luma-rd-pareto-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-intra-parity-mpm-1f.json
```

### VVC Chroma RD Shortlist

Checkpoint: `vvc-chroma-rd-pareto-1f`.

This checkpoint extends item 2 to chroma predictor selection. The chroma search
now keeps its normal derived/explicit/CCLM candidate list, then compares the
selected raw mode against the next shortlisted candidate after quantization and
inverse reconstruction of both Cb and Cr. The selector uses the same Pareto rule
as luma and caches the selected chroma residual blocks for finalization.

The change validated against VTM on smoke and high-depth smoke. Unlike the luma
shortlist, it produced consistent first-frame gains across all six lossy rows:
total QP24 size dropped 208,614 bytes versus `vvc-intra-parity-mpm-1f`, with
PSNR increasing on every row. The cost is extra candidate reconstruction work,
so total QP24 FPS moved from about 0.30 at `vvc-intra-parity-mpm-1f` to 0.23.
The `vvc-stats` instrumentation now also records
`luma_rd_refinement_attempts`, `luma_rd_refinement_switches`,
`chroma_rd_refinement_attempts`, and `chroma_rd_refinement_switches` per CTU and
in aggregate stats.

| Mode | Current bytes | FPS |
|---|---:|---:|
| lossless | 5,857,190 | 0.33 |
| qp=24 | 5,321,833 | 0.23 |

Per-row VVC QP24 deltas versus `vvc-intra-parity-mpm-1f`:

| Vector | Bytes | FPS | PSNR | Byte delta |
|---|---:|---:|---:|---:|
| SceneComposition_1_420 | 221,496 | 0.40 | 26.517 | -11,795 |
| SceneComposition_1_422 | 295,710 | 0.33 | 26.638 | -17,365 |
| screen_wayland_activity_rgb | 769,779 | 0.16 | 25.778 | -43,242 |
| MissionControlClip1_420 | 812,856 | 0.25 | 17.051 | -8,529 |
| MissionControlClip1_422 | 1,241,090 | 0.21 | 16.333 | -28,814 |
| MissionControlClip1_444 | 1,980,902 | 0.17 | 16.061 | -98,869 |

Validation:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

Command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-chroma-rd-pareto-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-intra-parity-mpm-1f.json
```

### VVC CTU Luma Leaf Selector

Checkpoint: `vvc-ctu-leaf-sse-selector-1f`.

This checkpoint addresses item 7 for the current luma partition surface. VVC
already has legal 8x8 and 4x4 luma residual leaves; the encoder now chooses the
per-CTU lossy luma leaf size from a rate-aware split proxy instead of using a
single picture-wide 8x8 choice. The proxy compares each 8x8 luma block's SSE
to its local mean with the sum of its four 4x4 local-mean SSEs, then selects
4x4 CTU leaves only when the estimated distortion reduction clears both a
QP/bit-depth-scaled rate penalty and a meaningful fraction of the CTU's luma
variance. Lossless keeps the existing 4x4 path unchanged.

The first-frame six-vector matrix is reference-compatible and lossless
byte-neutral versus `vvc-chroma-rd-pareto-1f`. QP24 total size drops by 72,342
bytes, and PSNR improves on every row. Two 10-bit 4:2:x rows spend extra luma
bits to buy that quality, while screen/RGB and 4:4:4 rows carry most of the net
bitrate win. A tighter split-gain probe (`vvc-ctu-leaf-sse-selector-tight-1f`)
reduced those row regressions but lost the useful overall size reduction, so
the looser net-positive selector remains active.

| Mode | Current bytes | FPS | Byte delta |
|---|---:|---:|---:|
| lossless | 5,857,190 | 0.33 | 0 |
| qp=24 | 5,249,491 | 0.22 | -72,342 |

Per-row VVC QP24 deltas versus `vvc-chroma-rd-pareto-1f`:

| Vector | Bytes | FPS | PSNR | Byte delta |
|---|---:|---:|---:|---:|
| SceneComposition_1_420 | 213,467 | 0.37 | 28.292 | -8,029 |
| SceneComposition_1_422 | 277,512 | 0.32 | 28.201 | -18,198 |
| screen_wayland_activity_rgb | 735,952 | 0.15 | 26.236 | -33,827 |
| MissionControlClip1_420 | 879,658 | 0.24 | 18.378 | +66,802 |
| MissionControlClip1_422 | 1,251,166 | 0.21 | 17.479 | +10,076 |
| MissionControlClip1_444 | 1,891,736 | 0.17 | 17.070 | -89,166 |

Validation:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc_ctu_luma_leaf_size_selector_uses_local_split_gain --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

Command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-ctu-leaf-sse-selector-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-chroma-rd-pareto-1f.json
```

### VVC MTS Selector Probe

Checkpoint probe: `vvc-mts-enabled-1f`.

The non-DCT2 luma MTS transform and syntax path is now reference-valid on the
smoke and high-depth smoke validation sets when the selector is enabled. The
current selector remains production-disabled, however, because its first-frame
matrix tradeoff is poor: total QP24 size increased from 5,321,833 bytes to
5,357,473 bytes versus `vvc-chroma-rd-pareto-1f`, and FPS fell from 0.23 to
0.07. This should be revisited with a cheaper mode-directed shortlist and a
rate-safer selection rule instead of trying every non-DCT2 transform on every
eligible 8x8 luma TU.

| Mode | Current bytes | FPS | Byte delta |
|---|---:|---:|---:|
| lossless | 5,857,190 | 0.34 | 0 |
| qp=24 | 5,357,473 | 0.07 | +35,640 |

Validation while temporarily enabling `VVC_ENABLE_LUMA_MTS_SELECTION`:

```sh
cargo test -p framefinery-codecs vvc_luma_mts --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

Command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-mts-enabled-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-chroma-rd-pareto-1f.json
```

### VVC Residual BDPCM Selection

Checkpoint: `vvc-bdpcm-residual-1f`.

This checkpoint completes the first usable form of item 5. Residual slices now
signal BDPCM capability in the SPS, and both lossy and lossless luma/chroma TUs
can select regular horizontal or vertical BDPCM inside the unified residual
path. The selector compares BDPCM candidates against the already-selected
regular predictor using reconstructed residual distortion and estimated syntax
rate, then only switches on Pareto wins so a BDPCM candidate does not buy rate
by making the reconstructed residual worse.

The coefficient path applies forward residual DPCM before transform-skip
quantization and inverse residual DPCM before transform-skip dequantization,
matching the VTM ordering. Dedicated BDPCM predictors bypass angular filtering
and PDPC while still using the same left/top availability model as the regular
intra predictor. The compile-gated `vvc-stats` path now records aggregate
luma/chroma horizontal/vertical BDPCM counts and includes `bdpcm_mode` in the
per-TU trace.

First-frame six-vector matrix versus `vvc-ctu-leaf-sse-selector-1f`:

| Mode | Previous bytes | Current bytes | Byte delta | Previous FPS | Current FPS |
|---|---:|---:|---:|---:|---:|
| lossless | 5,857,190 | 5,508,189 | -349,001 | 0.33 | 0.25 |
| qp=24 | 5,249,491 | 1,502,019 | -3,747,472 | 0.22 | 0.19 |

Per-row VVC QP24 deltas versus `vvc-ctu-leaf-sse-selector-1f`:

| Vector | Bytes | FPS | PSNR | Byte delta |
|---|---:|---:|---:|---:|
| SceneComposition_1_420 | 130,885 | 0.32 | 34.731 | -82,582 |
| SceneComposition_1_422 | 141,690 | 0.27 | 35.896 | -135,822 |
| screen_wayland_activity_rgb | 309,755 | 0.13 | 36.574 | -426,197 |
| MissionControlClip1_420 | 280,461 | 0.22 | 31.702 | -599,197 |
| MissionControlClip1_422 | 303,102 | 0.18 | 32.841 | -948,064 |
| MissionControlClip1_444 | 336,126 | 0.15 | 34.396 | -1,555,610 |

Validation:

```sh
cargo fmt
cargo check -p framefinery-codecs --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

Command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-bdpcm-residual-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-ctu-leaf-sse-selector-1f.json
```

### VVC MIP/ISP/LFNST Context Plumbing

This checkpoint wires the currently inactive VVC-only intra tool syntax sites
far enough that future predictors can be added without reworking SPS or CABAC
context ownership:

- MIP, ISP, and LFNST now have explicit CABAC context entries, VTM-derived
  I-slice init/log2 tables, RTL trace ids, and stats bit-category mapping.
- Residual SPS tool flags now carry `isp_enabled` and `mip_enabled` from the
  active slice configuration instead of hard-coding both to false.
- The luma CU syntax path contains inactive MIP, ISP, and LFNST emitters in
  the VTM-shaped order. Normal residual configs keep the flags false, so
  production streams remain byte-neutral. Enabling the flags currently emits
  the no-tool branch; active MIP still needs matrix predictor tables, active
  ISP needs split transform-tree ownership, and active LFNST needs transform
  candidate ownership plus coefficient-group constraints.

Validation:

```sh
cargo fmt
cargo check -p framefinery-codecs --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

### VVC MTS Candidate Accounting

Checkpoint: `vvc-mts-cost-accounting-1f`.

This checkpoint keeps non-DCT2 MTS transform and syntax support available, but
does not enable production selection. Two active selector probes were
reference-valid against VTM, yet neither was a useful default:

- `vvc-mts-directed-pareto-1f` tried one residual-gradient-directed MTS
  candidate per eligible 8x8 luma TU. It finished at 1,502,177 lossy bytes
  versus 1,502,019 for `vvc-bdpcm-residual-1f`, and reduced total lossy FPS
  from 0.19 to 0.16.
- `vvc-mts-directed-pareto-tsfirst-1f` avoided MTS trials when transform skip
  already won against DCT2, but still finished at 1,503,572 lossy bytes and
  did not recover enough FPS to justify enabling the selector.

The retained default-path change is narrower: the shared luma quantized
residual scorer now includes the explicit MTS flag cost for transformed blocks,
and the gated `vvc-stats` counters report nonzero luma MTS index counts by
transform pair. With `VVC_ENABLE_LUMA_MTS_SELECTION=false`, the stats probe on
the first SceneComposition 4:2:0 frame reported all luma MTS counts as zero.

First-frame six-vector matrix versus `vvc-bdpcm-residual-1f`:

| Mode | Previous bytes | Current bytes | Byte delta | Previous FPS | Current FPS |
|---|---:|---:|---:|---:|---:|
| lossless | 5,508,189 | 5,508,189 | 0 | 0.25 | 0.25 |
| qp=24 | 1,502,019 | 1,503,572 | +1,553 | 0.19 | 0.19 |

Per-row VVC QP24 deltas:

| Vector | Bytes | FPS | PSNR | Byte delta |
|---|---:|---:|---:|---:|
| SceneComposition_1_420 | 130,858 | 0.30 | 34.731 | -27 |
| SceneComposition_1_422 | 141,676 | 0.26 | 35.896 | -14 |
| screen_wayland_activity_rgb | 309,776 | 0.13 | 36.575 | +21 |
| MissionControlClip1_420 | 281,027 | 0.21 | 31.696 | +566 |
| MissionControlClip1_422 | 303,523 | 0.18 | 32.834 | +421 |
| MissionControlClip1_444 | 336,712 | 0.15 | 34.395 | +586 |

Validation:

```sh
cargo fmt
cargo check -p framefinery-codecs --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required
```

Command:

```sh
make benchmark-encode-matrix \
  ENCODE_MATRIX_RUN=vvc-mts-cost-accounting-1f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES="lossless lossy" \
  ENCODE_MATRIX_FRAMES=1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-bdpcm-residual-1f.json
```

### VVC Lossy Residual RD Selection

Checkpoint: `vvc-rd1-q19-50f`.

The 0.0.3 pre-release VVC lossy path had regressed into a mostly
distortion-first residual selector. That was reference-compliant, but it spent
too many bits on transform-skip/BDPCM residuals for small PSNR gains. The shared
luma/chroma residual candidate selector now uses a simple rate-distortion score
for non-exact lossy candidates while preserving exact-zero distortion priority.
This keeps the coding paths unified: luma mode, luma residual, chroma mode, and
chroma residual scoring all use the same deepest-level selector instead of
adding separate lossy/lossless implementations.

Rejected first-frame probes:

| Run | Bytes | Mean PSNR | Note |
|---|---:|---:|---|
| `vvc-rd64-q19-1f` | 2,111,133 | 51.965 | Too much PSNR loss for the first default. |
| `vvc-rd32-q19-1f` | 2,111,183 | 51.975 | Same practical mode threshold as weight 64. |
| `vvc-rd8-q19-1f` | 2,111,843 | 52.177 | Still too aggressive on quality. |
| `vvc-rd2-q19-1f` | 2,115,354 | 52.571 | Better, but still not meaningfully safer than weight 1. |
| `vvc-rd1-q19-1f` | 2,116,914 | 52.700 | Kept for validation. |

50-frame six-vector VVC lossy matrix versus the 0.0.3 pre-release checkpoint
`20260820T014839Z-six-vectors-full`:

| Vector | Previous bytes | Current bytes | Byte delta | Previous PSNR | Current PSNR | PSNR delta |
|---|---:|---:|---:|---:|---:|---:|
| SceneComposition_1_420 | 33,137,216 | 9,946,191 | -69.98% | 50.191 | 50.106 | -0.085 |
| SceneComposition_1_422 | 36,625,068 | 10,788,378 | -70.54% | 50.266 | 50.237 | -0.028 |
| screen_wayland_activity_rgb | 103,525,730 | 7,750,724 | -92.51% | 60.287 | 57.961 | -2.325 |
| MissionControlClip1_420 | 79,895,369 | 24,955,084 | -68.77% | 51.571 | 51.500 | -0.072 |
| MissionControlClip1_422 | 89,387,522 | 27,927,103 | -68.76% | 51.605 | 51.588 | -0.017 |
| MissionControlClip1_444 | 101,906,192 | 31,848,733 | -68.75% | 52.722 | 52.807 | +0.085 |

Aggregate:

| Metric | Previous | Current | Delta |
|---|---:|---:|---:|
| Bytes | 444,477,097 | 113,216,213 | -74.53% |
| Mean FPS | 1.404 | 1.369 | -2.49% |
| Mean PSNR | 52.774 | 52.367 | -0.407 |

Validation:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_SETTINGS="lossless fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_SETTINGS="fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=unusual-geometry-smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=unusual-geometry-smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_SETTINGS="fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Command:

```sh
AOMCTC_ROOT=/path/to/aomctc make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=release-six-vectors-full \
  ENCODE_MATRIX_RUN=vvc-rd1-q19-50f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES=lossy \
  ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_CLEANUP_RECON=1 \
  ENCODE_MATRIX_CLEANUP_OUTPUT=1 \
  ENCODE_MATRIX_CLEANUP_VECTORS=1
```

Follow-ups:

- Calibrate the RD weight against QP, bit depth, and chroma format instead of a
  fixed first-pass weight.
- Investigate the RGB/Wayland PSNR drop separately; the byte reduction is large,
  but it may be over-pruning visually important screen-content edges.
- Re-profile after the next mode-decision pass; `ctu_quantize`,
  residual scoring, and entropy build/write remain the primary VVC hotspots.

### VVC Lossy RGB Luma Transform-Skip Comparison

Checkpoint: `vvc-rd1-luma-rgb-ts-compare-q19-50f-limit3`.

The RD selector checkpoint still forced luma transform-skip immediately for all
lossy `fast-search=lossless-speed` mode-decision probes. Chroma already had an
8-bit 4:4:4/RGB exception because forcing transform-skip on screen content can
preserve noisy edges at a worse rate-distortion point. The luma selector now
uses the same scoped exception: lossy 8-bit 4:4:4/RGB compares transform-skip
against transformed residual coding, while other fast-search formats keep the
throughput shortcut.

Rejected probe:

- `vvc-rd1-luma-ts-compare-q19-1f` compared transformed residual coding for all
  lossy fast-search luma TUs. It improved the first Wayland RGB frame, but it
  reduced PSNR on the first SceneComposition 4:2:0/4:2:2 frames, so the final
  change was narrowed to 8-bit 4:4:4/RGB only.

50-frame limited matrix versus `vvc-rd1-q19-50f`:

| Vector | Previous bytes | Current bytes | Byte delta | Previous PSNR | Current PSNR | PSNR delta |
|---|---:|---:|---:|---:|---:|---:|
| SceneComposition_1_420 | 9,946,191 | 9,946,191 | 0 | 50.106 | 50.106 | 0.000 |
| SceneComposition_1_422 | 10,788,378 | 10,788,378 | 0 | 50.237 | 50.237 | 0.000 |
| screen_wayland_activity_rgb | 7,750,724 | 7,728,447 | -22,277 | 57.961 | 58.377 | +0.416 |

Validation:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=unusual-geometry-smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_SETTINGS="lossless fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Command:

```sh
AOMCTC_ROOT=/path/to/aomctc make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=release-six-vectors-full \
  ENCODE_MATRIX_RUN=vvc-rd1-luma-rgb-ts-compare-q19-50f-limit3 \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES=lossy \
  ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_LIMIT=3 \
  ENCODE_MATRIX_CLEANUP_RECON=1 \
  ENCODE_MATRIX_CLEANUP_OUTPUT=1 \
  ENCODE_MATRIX_CLEANUP_VECTORS=1
```

### VVC Lossy DC Candidate In Lossless-Speed Search

Checkpoint: `vvc-rd1-lossy-dc-q19-50f`.

The release fast-search path skipped luma DC for both lossy and lossless modes.
DC is cheap enough to keep in lossy mode, and the 50-frame six-vector matrix
showed it improves both bytes and PSNR on every row after the RD and RGB
transform-skip checkpoints. Lossless `fast-search=lossless-speed` still skips
DC because transform-skip/BDPCM candidates carry exact reconstruction there.

50-frame six-vector VVC lossy matrix versus the current committed baseline
after `vvc-rd1-luma-rgb-ts-compare-q19-50f-limit3`:

| Vector | Previous bytes | Current bytes | Byte delta | Previous PSNR | Current PSNR | PSNR delta |
|---|---:|---:|---:|---:|---:|---:|
| SceneComposition_1_420 | 9,946,191 | 9,937,207 | -8,984 | 50.106 | 50.149 | +0.043 |
| SceneComposition_1_422 | 10,788,378 | 10,778,273 | -10,105 | 50.237 | 50.273 | +0.036 |
| screen_wayland_activity_rgb | 7,728,447 | 7,722,473 | -5,974 | 58.377 | 58.403 | +0.026 |
| MissionControlClip1_420 | 24,955,084 | 24,902,127 | -52,957 | 51.500 | 51.572 | +0.073 |
| MissionControlClip1_422 | 27,927,103 | 27,836,145 | -90,958 | 51.588 | 51.655 | +0.067 |
| MissionControlClip1_444 | 31,848,733 | 31,820,412 | -28,321 | 52.807 | 52.844 | +0.037 |

Aggregate after this checkpoint:

| Metric | Value |
|---|---:|
| Bytes | 112,996,637 |
| Mean FPS | 1.315 |
| Mean PSNR | 52.483 |

Validation:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=unusual-geometry-smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_SETTINGS="lossless fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Command:

```sh
AOMCTC_ROOT=/path/to/aomctc make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=release-six-vectors-full \
  ENCODE_MATRIX_RUN=vvc-rd1-lossy-dc-q19-50f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES=lossy \
  ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_CLEANUP_RECON=1 \
  ENCODE_MATRIX_CLEANUP_OUTPUT=1 \
  ENCODE_MATRIX_CLEANUP_VECTORS=1
```

### VVC Lossy Planar Candidate In Lossless-Speed Search

Checkpoint: `vvc-rd1-lossy-dc-planar-q19-50f`.

After keeping DC in lossy fast-search, the next cheap luma candidate was
Planar. The prior `lossless-speed` path only evaluated Planar when neighboring
mode context suggested it. Lossy mode now keeps Planar unconditionally and lets
the shared residual RD selector reject it when directional modes are better.
Lossless fast-search keeps the neighbor-context pruning.

50-frame six-vector VVC lossy matrix versus `vvc-rd1-lossy-dc-q19-50f`:

| Vector | Previous bytes | Current bytes | Byte delta | Previous PSNR | Current PSNR | PSNR delta |
|---|---:|---:|---:|---:|---:|---:|
| SceneComposition_1_420 | 9,937,207 | 9,917,479 | -19,728 | 50.149 | 50.184 | +0.034 |
| SceneComposition_1_422 | 10,778,273 | 10,753,173 | -25,100 | 50.273 | 50.316 | +0.042 |
| screen_wayland_activity_rgb | 7,722,473 | 7,717,273 | -5,200 | 58.403 | 58.405 | +0.001 |
| MissionControlClip1_420 | 24,902,127 | 24,813,791 | -88,336 | 51.572 | 51.581 | +0.008 |
| MissionControlClip1_422 | 27,836,145 | 27,737,364 | -98,781 | 51.655 | 51.666 | +0.010 |
| MissionControlClip1_444 | 31,820,412 | 31,717,430 | -102,982 | 52.844 | 52.879 | +0.036 |

Aggregate after this checkpoint:

| Metric | Value |
|---|---:|
| Bytes | 112,656,510 |
| Mean FPS | 1.276 |
| Mean PSNR | 52.505 |

Validation:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=unusual-geometry-smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_SETTINGS="lossless fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Command:

```sh
AOMCTC_ROOT=/path/to/aomctc make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=release-six-vectors-full \
  ENCODE_MATRIX_RUN=vvc-rd1-lossy-dc-planar-q19-50f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES=lossy \
  ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_CLEANUP_RECON=1 \
  ENCODE_MATRIX_CLEANUP_OUTPUT=1 \
  ENCODE_MATRIX_CLEANUP_VECTORS=1
```

### VVC Lossy Chroma Candidates In Lossless-Speed Search

Checkpoint: `vvc-rd1-lossy-chroma-all-q19-50f`.

The previous fast-search policy used the lossless derived-only chroma shortcut
for lossy `fast-search=lossless-speed` as well. That kept throughput higher, but
it removed explicit chroma and CCLM candidates before the shared residual RD
selector could reject or accept them. The shortcut is now scoped to lossless
mode only. Lossy chroma searches evaluate the same derived, explicit, and CCLM
candidates and leave the final choice to the existing RD selector, so this does
not create a separate lossy chroma path.

50-frame six-vector VVC lossy matrix versus
`vvc-rd1-lossy-dc-planar-q19-50f`:

| Vector | Previous bytes | Current bytes | Byte delta | Byte delta % | Previous FPS | Current FPS | FPS delta | Previous PSNR | Current PSNR | PSNR delta |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| SceneComposition_1_420 | 9,917,479 | 9,637,494 | -279,985 | -2.82% | 1.870 | 1.628 | -12.9% | 50.184 | 50.233 | +0.049 |
| SceneComposition_1_422 | 10,753,173 | 10,585,806 | -167,367 | -1.56% | 1.548 | 1.315 | -15.1% | 50.316 | 50.437 | +0.121 |
| screen_wayland_activity_rgb | 7,717,273 | 7,717,273 | +0 | +0.00% | 0.668 | 0.689 | +3.2% | 58.405 | 58.405 | +0.000 |
| MissionControlClip1_420 | 24,813,791 | 24,418,152 | -395,639 | -1.59% | 1.440 | 1.112 | -22.7% | 51.581 | 51.672 | +0.092 |
| MissionControlClip1_422 | 27,737,364 | 27,373,987 | -363,377 | -1.31% | 1.249 | 0.805 | -35.5% | 51.666 | 51.783 | +0.118 |
| MissionControlClip1_444 | 31,717,430 | 31,773,424 | +55,994 | +0.18% | 0.884 | 0.532 | -39.8% | 52.879 | 52.947 | +0.067 |

Aggregate after this checkpoint:

| Metric | Previous | Current | Delta |
|---|---:|---:|---:|
| Bytes | 112,656,510 | 111,506,136 | -1,150,374 (-1.02%) |
| Mean FPS | 1.276 | 1.014 | -20.6% |
| Mean PSNR | 52.505 | 52.579 | +0.075 |

Rejected probes:

- `vvc-rd1-lossy-chroma-threshold-q19-1f` added a residual-threshold gate for
  explicit/CCLM candidates. It recovered some speed but lowered first-frame PSNR
  on every affected row, including the RGB row, so the gate was removed.
- `vvc-rd1-lossy-dc-planar-dirref-q19-1f` re-enabled directional refinement for
  lossy fast-search. It helped some rows but hurt RGB, SceneComposition 4:2:2,
  and MissionControl 4:4:4 first-frame quality, so it was left disabled.
- `vvc-rd1-lossy-leaf-select-q19-1f` let lossy fast-search pick smaller luma
  leaves locally instead of using the current 8x8 target. It increased bytes and
  reduced first-frame PSNR on the 8-bit SceneComposition rows.

Validation:

```sh
cargo fmt
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=unusual-geometry-smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_SETTINGS="lossless fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Command:

```sh
AOMCTC_ROOT=/path/to/aomctc make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=release-six-vectors-full \
  ENCODE_MATRIX_RUN=vvc-rd1-lossy-chroma-all-q19-50f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES=lossy \
  ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_CLEANUP_RECON=1 \
  ENCODE_MATRIX_CLEANUP_OUTPUT=1 \
  ENCODE_MATRIX_CLEANUP_VECTORS=1
```

Follow-up:

- The quality/bitrate gain is real but expensive. The next pass should keep the
  unified chroma mode path and replace the crude derived-only shortcut with a
  format-aware shortlist that can recover most of the chroma search speed without
  reproducing the rejected threshold probe's PSNR loss.

### VVC Lossy Chroma RD Top-Two Refinement

Checkpoint: `vvc-rd1-lossy-chroma-rd2-q19-50f`.

The previous chroma-all checkpoint selected the raw chroma predictor from the
full derived/explicit/CCLM set, but `fast-search=lossless-speed` still let RD
refinement inspect only that single raw winner. This checkpoint raises the
lossy chroma RD shortlist from one candidate to two candidates. The raw
candidate generation and final quantized-residual selector remain shared; only
the existing RD shortlist limit changes.

50-frame six-vector VVC lossy matrix versus
`vvc-rd1-lossy-chroma-all-q19-50f`:

| Vector | Previous bytes | Current bytes | Byte delta | Byte delta % | Previous FPS | Current FPS | FPS delta | Previous PSNR | Current PSNR | PSNR delta |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| SceneComposition_1_420 | 9,637,494 | 9,591,552 | -45,942 | -0.48% | 1.628 | 1.487 | -8.7% | 50.233 | 50.259 | +0.027 |
| SceneComposition_1_422 | 10,585,806 | 10,557,349 | -28,457 | -0.27% | 1.315 | 1.257 | -4.4% | 50.437 | 50.453 | +0.016 |
| screen_wayland_activity_rgb | 7,717,273 | 7,498,778 | -218,495 | -2.83% | 0.689 | 0.692 | +0.4% | 58.405 | 58.997 | +0.592 |
| MissionControlClip1_420 | 24,418,152 | 24,354,143 | -64,009 | -0.26% | 1.112 | 1.028 | -7.6% | 51.672 | 51.722 | +0.050 |
| MissionControlClip1_422 | 27,373,987 | 27,235,225 | -138,762 | -0.51% | 0.805 | 0.770 | -4.3% | 51.783 | 51.887 | +0.104 |
| MissionControlClip1_444 | 31,773,424 | 31,802,771 | +29,347 | +0.09% | 0.532 | 0.495 | -7.1% | 52.947 | 53.033 | +0.087 |

Aggregate after this checkpoint:

| Metric | Previous | Current | Delta |
|---|---:|---:|---:|
| Bytes | 111,506,136 | 111,039,818 | -466,318 (-0.42%) |
| Mean FPS | 1.014 | 0.955 | -5.8% |
| Mean PSNR | 52.579 | 52.725 | +0.146 |

Command:

```sh
AOMCTC_ROOT=/path/to/aomctc make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=release-six-vectors-full \
  ENCODE_MATRIX_RUN=vvc-rd1-lossy-chroma-rd2-q19-50f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES=lossy \
  ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_CLEANUP_RECON=1 \
  ENCODE_MATRIX_CLEANUP_OUTPUT=1 \
  ENCODE_MATRIX_CLEANUP_VECTORS=1
```

Follow-up:

- The second RD candidate is worth keeping on the release six-vector matrix, but
  it increases already-expensive chroma scoring. A later speed pass should
  compare top-two against a conditional second candidate keyed by the raw-score
  gap, chroma format, and CCLM participation.

### VVC Lossy Luma RD Top-Two Refinement

Checkpoint: `vvc-rd1-lossy-luma-rd2-gated-chroma-rd2-q19-50f`.

This checkpoint raises the luma RD shortlist from one raw winner to two raw
winners for lossy `fast-search=lossless-speed`, except for 8-bit 4:4:4/RGB
screen-content formats. The screen-content gate keeps the previous top-one
behavior because the ungated probe (`vvc-rd1-lossy-luma-rd2-chroma-rd2-q19-50f`)
regressed Wayland by 5,022 bytes and 0.012 dB. The selector remains unified:
all formats still use the same luma mode search and RD refinement code, with
only the shortlist depth selected by policy.

50-frame six-vector VVC lossy matrix versus
`vvc-rd1-lossy-chroma-rd2-q19-50f`:

| Vector | Previous bytes | Current bytes | Byte delta | Byte delta % | Previous FPS | Current FPS | FPS delta | Previous PSNR | Current PSNR | PSNR delta |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| SceneComposition_1_420 | 9,591,552 | 9,588,482 | -3,070 | -0.03% | 1.487 | 1.574 | +5.9% | 50.259 | 50.284 | +0.025 |
| SceneComposition_1_422 | 10,557,349 | 10,553,882 | -3,467 | -0.03% | 1.257 | 1.250 | -0.6% | 50.453 | 50.471 | +0.017 |
| screen_wayland_activity_rgb | 7,498,778 | 7,498,778 | +0 | +0.00% | 0.692 | 0.674 | -2.5% | 58.997 | 58.997 | +0.000 |
| MissionControlClip1_420 | 24,354,143 | 24,309,986 | -44,157 | -0.18% | 1.028 | 1.036 | +0.9% | 51.722 | 51.840 | +0.118 |
| MissionControlClip1_422 | 27,235,225 | 27,199,189 | -36,036 | -0.13% | 0.770 | 0.761 | -1.2% | 51.887 | 51.979 | +0.091 |
| MissionControlClip1_444 | 31,802,771 | 31,758,589 | -44,182 | -0.14% | 0.495 | 0.494 | -0.0% | 53.033 | 53.075 | +0.042 |

Aggregate after this checkpoint:

| Metric | Previous | Current | Delta |
|---|---:|---:|---:|
| Bytes | 111,039,818 | 110,908,906 | -130,912 (-0.12%) |
| Mean FPS | 0.955 | 0.965 | +1.1% |
| Mean PSNR | 52.725 | 52.774 | +0.049 |

Command:

```sh
AOMCTC_ROOT=/path/to/aomctc make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=release-six-vectors-full \
  ENCODE_MATRIX_RUN=vvc-rd1-lossy-luma-rd2-gated-chroma-rd2-q19-50f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES=lossy \
  ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_CLEANUP_RECON=1 \
  ENCODE_MATRIX_CLEANUP_OUTPUT=1 \
  ENCODE_MATRIX_CLEANUP_VECTORS=1
```

Follow-up:

- The luma top-two gain is small but consistent outside 8-bit 4:4:4/RGB. A
  later pass should test whether the second luma RD candidate can be selected by
  a raw-score-gap threshold instead of by chroma format.

### VVC Lossy RD-Gated CTU Skip

Checkpoint: `vvc-lossy-rd-ctu-skip-q19-50f-limit1`.

Lossy predictive CTU skip is now enabled only after the normal intra CTU path
has produced a reconstruction and payload. A CTU may switch to InterSkip when
the previous reconstruction is no worse by visible-sample SSE and the skipped
payload still saves a conservative CABAC bit margin. The pre-scan also requires
at least half of the frame's CTUs to have skip candidates before switching to
CTU-sliced output, because non-skipped CTUs lose cross-CTU intra availability in
that syntax shape.

This keeps the encoding paths unified: lossy CTUs still run through the same
intra quantization and mode-decision path first, and InterSkip is selected only
as a final block-level replacement. The path is reference-clean against VTM,
including one 50-frame AOM CTC `SceneComposition_1` row.

50-frame six-vector VVC lossy matrix versus
`vvc-rd1-lossy-luma-rd2-gated-chroma-rd2-q19-50f`:

| Vector | Previous bytes | Current bytes | Byte delta | Previous FPS | Current FPS | FPS delta | Previous PSNR | Current PSNR | PSNR delta |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| SceneComposition_1_420 | 9,588,482 | 2,716,281 | -6,872,201 | 1.574 | 0.88 | -0.70 | 50.284 | 50.239 | -0.045 |
| SceneComposition_1_422 | 10,553,882 | 2,957,068 | -7,596,814 | 1.250 | 0.71 | -0.54 | 50.471 | 50.406 | -0.065 |
| screen_wayland_activity_rgb | 7,498,778 | 959,945 | -6,538,833 | 0.674 | 0.41 | -0.26 | 58.997 | 58.842 | -0.155 |
| MissionControlClip1_420 | 24,309,986 | 10,122,157 | -14,187,829 | 1.036 | 0.86 | -0.18 | 51.840 | 51.924 | +0.084 |
| MissionControlClip1_422 | 27,199,189 | 11,275,575 | -15,923,614 | 0.761 | 0.69 | -0.07 | 51.979 | 52.055 | +0.076 |
| MissionControlClip1_444 | 31,758,589 | 12,786,961 | -18,971,628 | 0.494 | 0.45 | -0.05 | 53.075 | 53.069 | -0.006 |

Aggregate after this checkpoint:

| Metric | Previous | Current | Delta |
|---|---:|---:|---:|
| Bytes | 110,908,906 | 40,817,987 | -70,090,919 (-63.2%) |
| Mean FPS | 0.965 | 0.61 | -36.8% |
| Mean PSNR | 52.774 | 52.756 | -0.018 |

Validation:

```sh
cargo fmt
cargo check -p framefinery-codecs --features "vvc vvc-stats"
cargo test -p framefinery-codecs vvc_lossy_predictive --features "vvc vvc-stats" -- --nocapture
cargo test -p framefinery-codecs vvc_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=regression VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=multictu-regression VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=unusual-geometry-smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_SOURCE_FILTERS=1 VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp AOMCTC_ROOT=/path/to/aomctc \
  make validate-set CODEC=vvc VALIDATION_SET=release-six-vectors-full \
  VALIDATION_LIMIT=1 VALIDATION_FRAMES=50 VALIDATION_DIRECT_SOURCE_FILES=1 \
  VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_SETTINGS="lossless gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Command:

```sh
AOMCTC_ROOT=/path/to/aomctc make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=release-six-vectors-full \
  ENCODE_MATRIX_RUN=vvc-lossy-rd-ctu-skip-q19-50f-limit1 \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES=lossy \
  ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_VVC_LOSSY_QP=19 \
  ENCODE_MATRIX_VVC_FAST_SEARCH=lossless-speed \
  ENCODE_MATRIX_VVC_GOP=-1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-rd1-lossy-luma-rd2-gated-chroma-rd2-q19-50f.json \
  ENCODE_MATRIX_CLEANUP_RECON=1 \
  ENCODE_MATRIX_CLEANUP_OUTPUT=1 \
  ENCODE_MATRIX_CLEANUP_VECTORS=1
```

Follow-ups:

- Recover speed. The current gate performs extra per-candidate CTU CABAC
  counting and often moves the frame to CTU-sliced output; both are visible in
  the FPS regression.
- Implement and validate a legal mixed P-slice intra+InterSkip path. If VTM
  accepts it with dual-tree/profile constraints handled correctly, that should
  remove most CTU-slice header overhead and preserve cross-CTU intra context for
  non-skipped CTUs.
- Replace exact CABAC recounting in the gate with a cheaper bit-cost proxy or a
  cached payload-size estimate once the selection policy is stable.

### VVC Lossy CTU Skip Gate Speed Cleanup

Checkpoint: `vvc-lossy-ctu-skip-no-bitcount-q19-50f`.

The first RD-gated CTU skip implementation re-encoded each candidate CTU into a
temporary CABAC payload to enforce a bit-margin check. On the six-vector matrix,
that recount did not change any selected CTU: removing it produced identical
bitstream checksums, byte counts, and PSNR for all six rows. The gate now uses
the already-computed visible-sample SSE comparison only; the frame-level
candidate-count threshold remains the guard against sparse skip candidates
forcing CTU-sliced output.

50-frame six-vector VVC lossy matrix versus
`vvc-lossy-rd-ctu-skip-q19-50f-limit1`:

| Vector | Bytes delta | FPS before | FPS after | FPS delta | PSNR delta | Bitstream checksum |
|---|---:|---:|---:|---:|---:|---|
| SceneComposition_1_420 | +0 | 0.88 | 1.17 | +0.29 | +0.000 | identical |
| SceneComposition_1_422 | +0 | 0.71 | 0.92 | +0.22 | +0.000 | identical |
| screen_wayland_activity_rgb | +0 | 0.41 | 0.59 | +0.18 | +0.000 | identical |
| MissionControlClip1_420 | +0 | 0.86 | 1.12 | +0.27 | +0.000 | identical |
| MissionControlClip1_422 | +0 | 0.69 | 0.82 | +0.13 | +0.000 | identical |
| MissionControlClip1_444 | +0 | 0.45 | 0.52 | +0.07 | +0.000 | identical |

Aggregate after this checkpoint:

| Metric | Previous | Current | Delta |
|---|---:|---:|---:|
| Bytes | 40,817,987 | 40,817,987 | +0 |
| Mean FPS | 0.61 | 0.78 | +27.9% |
| Mean PSNR | 52.756 | 52.756 | +0.000 |

Validation:

```sh
cargo fmt
cargo check -p framefinery-codecs --features "vvc vvc-stats"
cargo test -p framefinery-codecs vvc_lossy_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=regression VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Command:

```sh
AOMCTC_ROOT=/path/to/aomctc make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=release-six-vectors-full \
  ENCODE_MATRIX_RUN=vvc-lossy-ctu-skip-no-bitcount-q19-50f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES=lossy \
  ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_VVC_LOSSY_QP=19 \
  ENCODE_MATRIX_VVC_FAST_SEARCH=lossless-speed \
  ENCODE_MATRIX_VVC_GOP=-1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-lossy-rd-ctu-skip-q19-50f-limit1.json \
  ENCODE_MATRIX_CLEANUP_RECON=1 \
  ENCODE_MATRIX_CLEANUP_OUTPUT=1 \
  ENCODE_MATRIX_CLEANUP_VECTORS=1
```

Follow-up:

- The remaining speed gap is now mostly the deliberate CTU-sliced syntax shape
  and the extra source/reconstruction region scans. A legal mixed P-slice
  intra+InterSkip path is the next likely high-value speed and compression
  cleanup.
- A scratch 128x64 two-frame yuv420p8 probe with the right CTU repeated and the
  left CTU changed failed VTM when mixed intra+InterSkip CTUs were emitted as a
  single P slice: VTM stopped in `decompressSlice` with `Expecting a terminating
  bit`. That confirms the release path cannot be changed by only swapping
  CTU-sliced packaging for single-slice packaging. The follow-up needs a legal
  P-slice coding-tree body for mixed intra/inter CTUs, including the dual-tree
  versus single-tree constraints and matching quantizer/CABAC neighbour state.

### VVC Lossy Skip Distortion Cache

Checkpoint: `vvc-lossy-skip-cached-distortion-q19-50f`.

The lossy CTU-skip pre-scan now caches each accepted candidate's skip
distortion and reuses it in the final post-intra InterSkip gate. Near-skip
candidate detection also combines the max-delta check with SSE accumulation, so
accepted near candidates do not scan the previous reconstruction twice. This is
a cleanup of duplicate reads only: the same CTUs are selected, and the emitted
bitstreams are byte-identical to `vvc-lossy-ctu-skip-no-bitcount-q19-50f`.

50-frame six-vector VVC lossy matrix versus
`vvc-lossy-ctu-skip-no-bitcount-q19-50f`:

| Vector | Bytes delta | FPS before | FPS after | FPS delta | PSNR delta | Bitstream checksum |
|---|---:|---:|---:|---:|---:|---|
| SceneComposition_1_420 | +0 | 1.17 | 1.17 | +0.00 | +0.000 | identical |
| SceneComposition_1_422 | +0 | 0.92 | 0.92 | -0.00 | +0.000 | identical |
| screen_wayland_activity_rgb | +0 | 0.59 | 0.59 | +0.00 | +0.000 | identical |
| MissionControlClip1_420 | +0 | 1.12 | 1.12 | -0.00 | +0.000 | identical |
| MissionControlClip1_422 | +0 | 0.82 | 0.80 | -0.01 | +0.000 | identical |
| MissionControlClip1_444 | +0 | 0.52 | 0.52 | +0.01 | +0.000 | identical |

Aggregate after this checkpoint:

| Metric | Previous | Current | Delta |
|---|---:|---:|---:|
| Bytes | 40,817,987 | 40,817,987 | +0 |
| Mean FPS | 0.78 | 0.78 | +0.0% |
| Mean PSNR | 52.756 | 52.756 | +0.000 |

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo fmt
TMPDIR=verification/generated/agent_scratch/tmp cargo check -p framefinery-codecs --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_lossy_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=regression VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_SETTINGS="lossless gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Command:

```sh
TMPDIR=verification/generated/agent_scratch/tmp AOMCTC_ROOT=/path/to/aomctc \
  make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=release-six-vectors-full \
  ENCODE_MATRIX_RUN=vvc-lossy-skip-cached-distortion-q19-50f \
  ENCODE_MATRIX_CODECS=vvc \
  ENCODE_MATRIX_MODES=lossy \
  ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_VVC_LOSSY_QP=19 \
  ENCODE_MATRIX_VVC_FAST_SEARCH=lossless-speed \
  ENCODE_MATRIX_VVC_GOP=-1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-lossy-ctu-skip-no-bitcount-q19-50f.json \
  ENCODE_MATRIX_CLEANUP_RECON=1 \
  ENCODE_MATRIX_CLEANUP_OUTPUT=1 \
  ENCODE_MATRIX_CLEANUP_VECTORS=1 \
  ENCODE_MATRIX_DIRECT_SOURCE_FILES=1
```

Rejected probe:

- A lossy `fast-search=lossless-speed` BDPCM gate that only tested
  BDPCM-aligned luma modes/neighbours and derived/aligned chroma modes was not
  worth keeping. On a 10-frame `SceneComposition_1_420` probe it improved FPS
  only from 1.05 to 1.08, while bytes increased from 759,800 to 962,958 and
  mean PSNR fell from 51.903 dB to 50.181 dB. BDPCM remains expensive, but a
  coarse alignment-only prune discards useful screen-content candidates.
- A one-candidate RD-refinement cap for lossy `fast-search=lossless-speed`
  4:2:0/4:2:2 was also not worth keeping. On the same 10-frame
  `SceneComposition_1_420` probe it improved FPS only from 1.05 to 1.07, while
  bytes increased from 759,800 to 760,549 and mean PSNR fell from 51.903 dB to
  51.843 dB. The existing two-candidate cap remains the better speed/quality
  point for 4:2:0/4:2:2.

### VVC Lossy Fast-Search QP Retune For High-Chroma Formats

Checkpoint: `vvc-qptune-probe-q19-50f`.

The lossy `fast-search=lossless-speed` QP tune was too conservative for the
high-chroma rows in the six-vector release matrix. 8-bit 4:2:2 and high-depth
4:2:2/4:4:4 were substantially below AV2 PSNR while still below, or close to,
AV2 byte counts. Retuning only those formats closes the aggregate PSNR gap
without touching the encoding syntax paths:

- 8-bit 4:2:2: requested QP 19 now encodes at slice QP 17.
- high-depth 4:2:2: requested QP 19 now encodes at slice QP 11.
- high-depth 4:4:4: requested QP 19 now encodes at slice QP 10.
- 8-bit 4:4:4 remains unchanged because the Wayland RGB row was already ahead
  of AV2 on both bytes and PSNR.

50-frame VVC lossy matrix versus `vvc-lossy-skip-cached-distortion-q19-50f`:

| Vector | Bytes before | Bytes after | Bytes delta | PSNR before | PSNR after | PSNR delta |
|---|---:|---:|---:|---:|---:|---:|
| SceneComposition_1_420 | 2,716,281 | 2,716,281 | +0 | 50.239 | 50.239 | +0.000 |
| SceneComposition_1_422 | 2,957,068 | 3,256,957 | +299,889 | 50.406 | 52.007 | +1.601 |
| screen_wayland_activity_rgb | 959,945 | 959,945 | +0 | 58.842 | 58.842 | +0.000 |
| MissionControlClip1_420 | 10,122,157 | 10,122,157 | +0 | 51.924 | 51.924 | +0.000 |
| MissionControlClip1_422 | 11,275,575 | 12,204,087 | +928,512 | 52.055 | 53.686 | +1.631 |
| MissionControlClip1_444 | 12,786,961 | 14,067,577 | +1,280,616 | 53.069 | 54.807 | +1.738 |

Comparison with the current AV2 lossy q24 matrix:

| Vector | VVC bytes | AV2 bytes | Bytes vs AV2 | VVC PSNR | AV2 PSNR | PSNR delta |
|---|---:|---:|---:|---:|---:|---:|
| SceneComposition_1_420 | 2,716,281 | 2,454,925 | +10.6% | 50.239 | 50.743 | -0.504 |
| SceneComposition_1_422 | 3,256,957 | 3,182,499 | +2.3% | 52.007 | 52.245 | -0.238 |
| screen_wayland_activity_rgb | 959,945 | 1,158,621 | -17.1% | 58.842 | 57.840 | +1.002 |
| MissionControlClip1_420 | 10,122,157 | 9,734,853 | +4.0% | 51.924 | 51.115 | +0.809 |
| MissionControlClip1_422 | 12,204,087 | 13,593,082 | -10.2% | 53.686 | 53.960 | -0.273 |
| MissionControlClip1_444 | 14,067,577 | 17,609,567 | -20.1% | 54.807 | 55.537 | -0.730 |

Aggregate after this checkpoint:

| Metric | VVC previous | VVC current | AV2 current |
|---|---:|---:|---:|
| Bytes | 40,817,987 | 43,327,004 | 47,733,547 |
| Mean PSNR | 52.756 | 53.584 | 53.573 |
| Mean FPS | 0.86 | 0.87 | 5.13 |

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo fmt
TMPDIR=verification/generated/agent_scratch/tmp cargo check -p framefinery-codecs --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_lossless_speed_tunes_lossy_slice_qp_by_format --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=regression VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp AOMCTC_ROOT=/path/to/aomctc \
  make validate-set CODEC=vvc VALIDATION_SET=release-six-vectors-full \
  VALIDATION_LIMIT=2 VALIDATION_FRAMES=10 VALIDATION_DIRECT_SOURCE_FILES=1 \
  VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

A scratch two-row manifest also validated the high-depth 4:2:2 and 4:4:4
patched cases for three frames each with VTM-required reconstruction matching.

Follow-ups:

- The remaining gap is speed, not aggregate quality/bytes. VVC is still about
  0.17x AV2 mean FPS on the six-vector lossy matrix.
- The 2560x1440 Wayland RGB row exposed a separate VTM PPS parser failure for
  CTU-sliced predictive streams. That blocker is resolved by the
  `vvc-large-geometry-predictive-pps` checkpoint below.

### VVC Large-Geometry Predictive PPS Gate

Checkpoint: `vvc-large-geometry-predictive-pps`.

Predictive VVC streams used to emit both a CTU-sliced PPS and a single-slice
PPS up front. The CTU-sliced PPS maps one 64x64 CTU to one tile and one slice
so mixed predictive frames can encode skipped CTUs as P slices while keeping
non-skipped CTUs as I slices. That syntax shape is reference-clean for the
normal 1920x1080 six-vector AOM rows, which have 30 CTU columns, but it is not
valid for wider frames. VTM 24.0 rejects more than 30 explicit tile columns;
the 2560x1440 Wayland row has 40 CTU columns and failed before decoding any
slice:

```text
Number of explicit tile columns exceeds valid range
```

The encoder now gates one-slice-per-CTU partitioning through
`vvc_one_slice_per_ctu_partitioning_supported()`, currently matching VTM's
`MAX_TILE_COLS = 30` and `MAX_TILES = 990` limits. When a predictive stream is
too large for that CTU-as-tile layout:

- the unsupported CTU-sliced PPS is not emitted;
- CTU-level mixed InterSkip is disabled for that stream;
- the single-slice predictive PPS remains available, so repeated full frames
  can still use the reference-clean all-skip P-slice path.

The immediate Wayland RGB 3-frame validation now passes and omits the unused
PPS:

| Row | Geometry | Frames | Result | Bytes |
|---|---:|---:|---|---:|
| screen_wayland_activity_rgb | 2560x1440 gbrp8 | 3 | VTM reconstruction matches | 151,982 |

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo fmt
TMPDIR=verification/generated/agent_scratch/tmp cargo check -p framefinery-codecs --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_ctu_sliced_pps_respects_vtm_tile_column_limit --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_predictive_large_geometry_uses_only_single_slice_pps --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_lossy_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp AOMCTC_ROOT=/path/to/aomctc \
  make validate-set CODEC=vvc VALIDATION_SET=release-six-vectors-full \
  VALIDATION_LIMIT=3 VALIDATION_FRAMES=3 VALIDATION_DIRECT_SOURCE_FILES=1 \
  VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Follow-up:

- If CTU-level mixed InterSkip is worth preserving for wider frames, implement
  a legal large-picture partitioning layout instead of one tile per CTU. The
  likely direction is grouped tile columns or a different slice map that stays
  under VVC/VTM tile limits while still keeping CABAC bodies reference-clean.

### VVC Motion Search And Mode-Select Research Sweep

Checkpoint: `vvc-motion-mode-research-2026-08`.

The VVC speed gap is now primarily an encoder-search problem. Mature encoders
do not rely on one exhaustive pass; they stage cheap signals before expensive
RDO and only run the expensive path for plausible winners:

- x265 exposes `--early-skip`, `--rskip`, `--splitrd-skip`, and `--fast-intra`.
  The important pattern is early no-residual/skip testing, recursion exit from
  skip or edge-density signals, and angular intra search by sparse scan plus
  local refinement rather than all modes.
- VTM carries analogous reference heuristics: `PBIntraFast` suppresses unlikely
  intra checks in inter slices, `ContentBasedFastQtbt` gates split choices from
  local texture gradients, and `checkEarlySkip()` marks no-residual merge/zero
  MVD blocks for early skip handling.
- AOM's AV1 speed features use the same structure in production code: reduced
  MV search windows, zero-MV SSE thresholds that skip motion search, duplicate
  starting-MV suppression, winner-mode limits, gradient/histogram-driven intra
  pruning, and transform-search gates after skip becomes likely.
- VVenC presets are explicitly documented as Pareto runtime/quality tradeoffs,
  and the VVC partitioning papers around VTM/VVenC show why this matters: fast
  partition strategies can save large runtime at small BD-rate cost when the
  early decision is tied to texture and QP instead of a blind mode count.

Useful primary references checked:

- x265 CLI mode decision options:
  <https://x265.readthedocs.io/en/stable/cli.html>
- VVenC usage and preset model:
  <https://github.com/fraunhoferhhi/vvenc/wiki/Usage>
- AOM AV1 speed feature definitions:
  <https://aomedia.googlesource.com/aom/+/6ad85e8ed9c5db196900bffa91161ea947606866/av1/encoder/speed_features.h>
- Fast VVC partitioning in VTM, ICIP 2019:
  <https://publica.fraunhofer.de/entities/publication/9210f1fb-90f8-4759-9bb6-d6fc72a9b731>
- Fast VVC partitioning in VVenC, PCS 2021:
  <https://publica.fraunhofer.de/entities/publication/a6ca1879-7d67-4286-af4f-158e06d60ce9>
- Fast QTMT partition and intra mode decision, JVCIR 2023:
  <https://www.sciencedirect.com/science/article/pii/S1047320323000822>

Current FrameFinery implications:

1. True VVC MV search is not implemented yet. The current predictive path is
   skip/reuse-oriented, so the next inter feature should start with a bounded
   zero/neighbor-MVP search, duplicate-start suppression, and adaptive small
   search windows. Do not add an exhaustive full-frame search as the baseline.
2. The biggest current FPS target is accepted-skip CTUs. The encoder often does
   full intra quantization first, then replaces the CTU with InterSkip. A
   high-confidence skip pre-gate can be valuable if it is tied to zero/previous
   reconstruction SSE, local texture, and QP, and if validation proves the
   byte/PSNR tradeoff row-by-row.
3. The legal mixed P-slice intra+InterSkip path remains high value. It should
   remove the need to switch eligible 1920-wide streams into one-slice-per-CTU
   output while preserving cross-CTU intra context for non-skipped CTUs. The
   previous single-slice mixed probe failed VTM, so this must be implemented
   from the VVC coding-tree syntax rules rather than by changing PPS packaging
   alone.
4. More blind intra top-N pruning is not enough. Our existing gradient source
   seed and spatial-neighbour families are the right shape, but additional
   pruning must happen before residual materialization and must be tied to
   stronger texture/gradient confidence. Raw score gap alone was not stable.
5. The current fixed 8x8 residual tree avoids the full VVC QTMT explosion, but
   it also leaves speed/compression opportunities on the table. The VTM/VVenC
   partitioning literature suggests a future larger-leaf/flexible-split pass
   should use texture and QP gates from the start, not exhaustive recursive RDO.

Tradeoff scoring:

`scripts/benchmark_encode_matrix.py` already projects per-row
`[bytes, PSNR, FPS]` into a single score:

```text
10 * log2(current_fps / baseline_fps)
+ 4 * log2(baseline_bytes / current_bytes)
+ 8 * (current_psnr - baseline_psnr)
```

The classifier also hard-fails large regressions: FPS below 0.90x, bytes above
1.20x, or PSNR below -1.0 dB. A clear accept currently requires score >= 2 and
FPS >= 1.10x when FPS is present. That is the right default for FPS work
because it lets us spend a little bitrate or PSNR only when speed improves
enough to matter.

Rejected probe: raw-score-gap RD shortlist collapse.

A conservative gate was tested that collapsed lossy
`fast-search=lossless-speed` luma/chroma top-two RD refinement to top-one when
the best raw score was at least 2x better than the second raw score. This kept
the selector unified but did not score well enough:

- `vvc-confident-rd-gap-scene420-10f` initially looked promising on
  `SceneComposition_1_420`: +0.20 FPS, +2,047 bytes, -0.006 dB, score
  `+2.4:accept`.
- `vvc-confident-rd-gap-q19-50f` rejected it on the six-vector 50-frame matrix:
  average score `+0.1`, with 0 accepts, 2 watches, and 4 regressions. The
  Wayland delta was not directly comparable because the baseline predated the
  large-geometry PPS gate, but the comparable AOM rows were still too mixed:
  the MissionControl rows regressed or failed to produce a meaningful speed
  win.

The code change was reverted. Do not repeat this exact raw 2x-gap top-one gate
without adding a stronger signal, such as local gradient confidence, skip
likelihood, QP-dependent tolerance, or per-format thresholds validated against
the full six-vector matrix.

### VVC Lossy Zero-SSE CTU Pre-Skip

Checkpoint: `vvc-zero-sse-preskip-q19-50f`.

The lossy predictive CTU skip gate now preselects InterSkip before intra
quantization when the cached skip reconstruction has exactly zero SSE against
the current source CTU. This is deliberately stricter than the normal
near-skip gate. The existing post-intra RD check selects InterSkip when
`skip_distortion <= intra_distortion`; with zero skip distortion, the later RD
gate would always choose the same InterSkip CTU. The shortcut therefore avoids
known-wasted intra search/quantization work without changing the selected
payload, reconstruction, or syntax path.

The path remains unified:

- normal lossy near-skip candidates still run through the existing post-intra
  RD comparison;
- the pre-skip branch only reuses the existing cached CTU decision and
  reconstruction;
- non-skipped CTUs still use the same intra quantization and mode-decision
  implementation;
- the shortcut is only active in the already reference-clean CTU-sliced
  predictive path.

Focused 10-frame two-row benchmark versus
`vvc-zero-sse-preskip-baseline-10f-limit2`:

| Vector | Bytes | Bitstream SHA | FPS before | FPS after | FPS delta | PSNR |
|---|---:|---|---:|---:|---:|---:|
| SceneComposition_1_420 | 759,800 | identical | 1.22 | 1.37 | +0.16 | 51.903 |
| SceneComposition_1_422 | 871,662 | identical | 0.98 | 1.14 | +0.16 | 53.556 |

The same two rows on the 50-frame matrix remained bitstream-identical to
`vvc-qptune-probe-q19-50f` while improving measured FPS:

| Vector | Bytes | Bitstream SHA | FPS delta | PSNR delta | Tradeoff |
|---|---:|---|---:|---:|---|
| SceneComposition_1_420 | 2,716,281 | identical | +0.14 | +0.000 | `+1.6 watch` |
| SceneComposition_1_422 | 3,256,957 | identical | +0.14 | +0.000 | `+2.1 accept` |

The high-depth MissionControl rows were also bitstream-identical. Their FPS
deltas were small negative samples (-0.04, -0.02, -0.01 FPS), so they are
treated as measurement noise rather than a codec decision regression. The
Wayland row is not comparable against `vvc-qptune-probe-q19-50f` because that
baseline predates the large-geometry PPS gate that disables CTU-sliced skip for
2560-wide frames.

Stats probe `vvc-zero-sse-preskip-stats-10f-limit4` showed where the shortcut
fires:

| Row | CTUs | Near-skip candidates | Zero-SSE pre-skips | InterSkip CTUs |
|---|---:|---:|---:|---:|
| SceneComposition_1_420 | 5,100 | 3,593 | 1,012 | 3,479 |
| SceneComposition_1_422 | 5,100 | 3,599 | 1,096 | 3,528 |
| MissionControlClip1_420 | 5,100 | 3,561 | 161 | 3,245 |
| screen_wayland_activity_rgb | 9,200 | 0 | 0 | 7,360 |

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo fmt
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_lossy_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=regression VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp AOMCTC_ROOT=/path/to/aomctc \
  make validate-set CODEC=vvc VALIDATION_SET=release-six-vectors-full \
  VALIDATION_LIMIT=4 VALIDATION_FRAMES=3 VALIDATION_DIRECT_SOURCE_FILES=1 \
  VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Follow-up:

- This is a safe incremental FPS win, not the main catch-up. The next larger
  speed step is still legal mixed P-slice intra+InterSkip or a stronger
  high-confidence skip pre-gate that can skip intra for nonzero distortion
  while proving the bitrate/PSNR tradeoff row-by-row.

### VVC Lossy Low-Distortion CTU Pre-Skip

Checkpoint: `vvc-preskip-avg1-q19-50f`.

The zero-SSE pre-skip was extended to a very tight nonzero threshold: a lossy
predictive CTU may bypass intra quantization when its InterSkip reconstruction
has average SSE no greater than one 8-bit-equivalent squared sample error over
the visible luma+chroma CTU samples. For higher bit depths the threshold scales
by the square of the bit-depth sample step. For a full 8-bit 4:2:0 CTU, the
threshold is 6,144 total SSE across 4,096 luma and 2,048 chroma samples.

This intentionally remains inside the same unified path:

- the normal InterSkip syntax and cached reconstruction are reused;
- CTUs above the threshold still run full intra quantization and the existing
  post-intra RD skip gate;
- lossless predictive reuse is unchanged;
- no separate lossy encoder path was added.

50-frame VVC lossy matrix versus `vvc-zero-sse-preskip-q19-50f`:

| Vector | Bytes before | Bytes after | Byte delta | FPS before | FPS after | FPS delta | PSNR before | PSNR after | PSNR delta | Tradeoff |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| SceneComposition_1_420 | 2,716,281 | 2,659,970 | -56,311 | 1.29 | 2.43 | +1.14 | 50.239 | 50.208 | -0.031 | `+9.0 accept` |
| SceneComposition_1_422 | 3,256,957 | 3,224,447 | -32,510 | 1.06 | 2.34 | +1.27 | 52.007 | 51.880 | -0.127 | `+10.4 accept` |
| screen_wayland_activity_rgb | 4,046,297 | 4,046,297 | +0 | 1.13 | 1.18 | +0.05 | 58.997 | 58.997 | +0.000 | `+0.7 watch` |
| MissionControlClip1_420 | 10,122,157 | 9,457,968 | -664,189 | 1.13 | 2.06 | +0.93 | 51.924 | 51.817 | -0.107 | `+8.2 accept` |
| MissionControlClip1_422 | 12,204,087 | 11,328,521 | -875,566 | 0.81 | 1.64 | +0.84 | 53.686 | 53.607 | -0.079 | `+10.1 accept` |
| MissionControlClip1_444 | 14,067,577 | 13,138,501 | -929,076 | 0.53 | 1.03 | +0.51 | 54.807 | 54.759 | -0.048 | `+9.7 accept` |

Aggregate after this checkpoint:

| Metric | Previous VVC | Current VVC | AV2 current |
|---|---:|---:|---:|
| Bytes | 46,413,356 | 43,855,704 | 47,733,547 |
| Mean row FPS | 0.91 | 1.78 | 5.13 |
| Mean PSNR | 53.586 | 53.545 | 53.573 |

Compared with `av2-current-lossy-q24-50f`, VVC is now about 8% smaller and
within 0.03 dB aggregate PSNR, but still only about 0.35x AV2 mean row FPS.
The remaining 0.0.4 gap is therefore mainly runtime.

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo fmt
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_lossy_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=regression VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp AOMCTC_ROOT=/path/to/aomctc \
  make validate-set CODEC=vvc VALIDATION_SET=release-six-vectors-full \
  VALIDATION_LIMIT=6 VALIDATION_FRAMES=3 VALIDATION_DIRECT_SOURCE_FILES=1 \
  VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Follow-up:

- The average-1 threshold is a strong tradeoff point. Higher pre-skip
  thresholds should be treated as new experiments and compared against this
  checkpoint, not against the older zero-SSE baseline.
- The next larger FPS gains probably need legal mixed P-slice intra+InterSkip
  or a cheap predictor for CTUs where skip is likely but not below this
  threshold.

### VVC Lossy Average-4 CTU Pre-Skip

Checkpoint: `vvc-preskip-avg4-q19-50f`.

The low-distortion pre-skip threshold was raised from average SSE <= 1 to
average SSE <= 4 in 8-bit-equivalent squared units. For a full 8-bit 4:2:0 CTU,
the pre-skip limit is now 24,576 total SSE across visible luma+chroma samples.
The threshold is named in code as
`VVC_LOSSY_PREDICTIVE_PRESKIP_AVG_SSE_8BIT` so future probes can compare
against the current checkpoint directly.

50-frame VVC lossy matrix versus `vvc-preskip-avg1-q19-50f`:

| Vector | Bytes delta | Bitstream change | FPS delta | PSNR delta | Tradeoff |
|---|---:|---|---:|---:|---|
| SceneComposition_1_420 | -8,582 | yes | +1.50 | -0.010 | `+6.9 accept` |
| SceneComposition_1_422 | +0 | no | +0.74 | +0.000 | `+4.0 accept` |
| screen_wayland_activity_rgb | +0 | no | +0.00 | +0.000 | `+0.1 watch` |
| MissionControlClip1_420 | +0 | no | +0.05 | +0.000 | `+0.3 watch` |
| MissionControlClip1_422 | +0 | no | -0.02 | +0.000 | `-0.2 regress` |
| MissionControlClip1_444 | +0 | no | +0.01 | +0.000 | `+0.2 watch` |

Aggregate after this checkpoint:

| Metric | Avg1 | Avg4 |
|---|---:|---:|
| Bytes | 43,855,704 | 43,847,122 |
| Mean row FPS | 1.78 | 2.16 |
| Mean PSNR | 53.545 | 53.543 |

Most of the measured gain comes from preselecting additional CTUs without
changing the final payload decisions. Only the 8-bit 4:2:0 row changed
bitstream output, and that change reduced bytes with a 0.01 dB PSNR cost. The
small 10-bit 4:2:2 negative FPS sample was output-identical noise, not a codec
decision regression.

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo fmt
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_lossy_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=regression VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp AOMCTC_ROOT=/path/to/aomctc \
  make validate-set CODEC=vvc VALIDATION_SET=release-six-vectors-full \
  VALIDATION_LIMIT=6 VALIDATION_FRAMES=3 VALIDATION_DIRECT_SOURCE_FILES=1 \
  VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Follow-up:

- Superseded by the average-8 checkpoint below. The `vvc-preskip-avg4-q19-50f`
  matrix remains useful as the immediate comparison point for that change.
- This remains a conservative pre-gate. Catching up further on FPS probably
  needs mixed P-slice intra+InterSkip legality work or a predictor that can
  skip more CTUs without depending only on reconstruction SSE.

### VVC Lossy Average-8 CTU Pre-Skip

Checkpoint: `vvc-preskip-avg8-q19-50f`.

The lossy predictive pre-skip threshold was raised from average SSE <= 4 to
average SSE <= 8 in 8-bit-equivalent squared units. For a full 8-bit 4:2:0 CTU,
the pre-skip limit is now 49,152 total SSE across visible luma+chroma samples.

50-frame VVC lossy matrix versus `vvc-preskip-avg4-q19-50f`:

| Vector | Bytes delta | Bitstream change | FPS delta | PSNR delta | Tradeoff |
|---|---:|---|---:|---:|---|
| SceneComposition_1_420 | +0 | no | +0.14 | +0.000 | `+0.9 watch` |
| SceneComposition_1_422 | +0 | no | +0.14 | +0.000 | `+0.9 watch` |
| screen_wayland_activity_rgb | +0 | no | +0.05 | +0.000 | `+0.5 watch` |
| MissionControlClip1_420 | +0 | no | +0.14 | +0.000 | `+0.8 watch` |
| MissionControlClip1_422 | +0 | no | +0.06 | +0.000 | `+0.5 watch` |
| MissionControlClip1_444 | +0 | no | +0.06 | +0.000 | `+0.5 watch` |

Aggregate after this checkpoint:

| Metric | Avg4 | Avg8 |
|---|---:|---:|
| Bytes | 43,847,122 | 43,847,122 |
| Mean row FPS | 2.16 | 2.26 |
| Mean PSNR | 53.543 | 53.543 |

All six 50-frame outputs were bitstream-identical to the average-4 baseline, so
the change is a pure speed improvement on this matrix. The likely reason is
that the raised gate preselects CTUs that the later full mode path was already
choosing as InterSkip, avoiding extra analysis without changing the encoded
syntax or reconstruction.

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo fmt
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_lossy_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=regression VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp AOMCTC_ROOT=/path/to/aomctc \
  make validate-set CODEC=vvc VALIDATION_SET=release-six-vectors-full \
  VALIDATION_LIMIT=6 VALIDATION_FRAMES=3 VALIDATION_DIRECT_SOURCE_FILES=1 \
  VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Follow-up:

- Treat average-8 as the new conservative pre-skip baseline. Higher thresholds
  must still clear the 50-frame scorer and VTM checks because they are more
  likely to trade bytes or PSNR for FPS.
- The remaining FPS gap to AV2 is too large for threshold tuning alone. The
  next likely wins are better search ordering, mixed P-slice legality, and
  early mode pruning based on cheap inter/intra predictors.

Rejected follow-up probe:

- `vvc-preskip-avg16-q19-50f` raised the same threshold to average SSE <= 16.
  It produced byte-identical and PSNR-identical output on all six 50-frame
  rows, but the benchmark scorer averaged `+0.0` with two timing-regression
  rows. This is not worth keeping over average-8 unless a future implementation
  makes the extra preselected CTUs materially reduce work.
- `vvc-frame-skip-payload-cache-q19-50f` wired the existing
  `VvcFrameSkipPayloadCache` into full-frame InterSkip emission. The probe was
  byte-identical and PSNR-identical, but the six-vector scorer averaged `-0.1`
  with three timing-regression rows. The repeated-frame entropy payload is not a
  large enough cost center on this matrix to justify the extra encoder state.

### VVC CTU InterSkip Slice Payload Cache

Checkpoint: `vvc-ctu-interskip-slice-cache-q19-50f`.

CTU-sliced predictive frames still spend significant time building and writing
slice payloads for skipped CTUs. Unlike the rejected full-frame skip cache, the
CTU-slice case repeats hundreds of small InterSkip slice bodies per frame. The
picture header is emitted as a separate NAL in this path, so a fixed
slice-address/geometry/config InterSkip CTU-slice RBSP does not depend on POC.

This checkpoint caches complete CTU InterSkip slice RBSP payloads by picture
kind, picture geometry, CTU geometry, slice address, and slice syntax config.
Intra CTU slices still use the existing uncached path. Mode decisions,
reconstruction, slice selection, and CABAC coding-tree semantics are unchanged.

50-frame VVC lossy matrix versus `vvc-preskip-avg8-q19-50f`:

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| SceneComposition_1_420 | +0 | +0.000 | +1.23 | `+3.8 accept` |
| SceneComposition_1_422 | +0 | +0.000 | +1.05 | `+4.1 accept` |
| screen_wayland_activity_rgb | +0 | +0.000 | +0.00 | `+0.0 watch` |
| MissionControlClip1_420 | +0 | +0.000 | +0.13 | `+0.8 watch` |
| MissionControlClip1_422 | +0 | +0.000 | +0.12 | `+1.0 watch` |
| MissionControlClip1_444 | +0 | +0.000 | +0.12 | `+1.4 watch` |

Aggregate scorer summary:

| Rows | Average score | Accept | Watch | Regress |
|---:|---:|---:|---:|---:|
| 6 | +1.9 | 2 | 4 | 0 |

All six rows were byte- and PSNR-identical. The per-row baseline deltas in the
scorer are the acceptance signal because absolute FPS samples vary across
separate benchmark runs.

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo fmt
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_lossy_predictive --features "vvc vvc-stats" -- --nocapture
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=regression VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp AOMCTC_ROOT=/path/to/aomctc \
  make validate-set CODEC=vvc VALIDATION_SET=release-six-vectors-full \
  VALIDATION_LIMIT=6 VALIDATION_FRAMES=3 VALIDATION_DIRECT_SOURCE_FILES=1 \
  VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Follow-up:

- This reduces repeated CTU-slice syntax construction, but it does not solve the
  root cost of one slice per CTU. Legal mixed P-slice intra+InterSkip or grouped
  slice maps remain the larger structural FPS opportunity.
- If grouped CTU-slice maps are implemented later, keep this cache scoped to
  deterministic InterSkip slice bodies and revalidate that POC/state does not
  enter the cached RBSP.

### VVC Motion Search And Mode-Select Research Notes

Research checkpoint: `vvc-motion-mode-research-2026-08-21`.

The current FrameFinery VVC path does not yet encode general nonzero-MV inter
blocks. Predictive frames can emit all-frame/CTU InterSkip and reuse cached
mode decisions, but there is no normal translational/affine motion-vector search
to tune. That means external ME techniques should be treated as design input for
the next inter milestone, while current FPS work should focus on pruning the
existing intra/residual mode path without forking lossy/lossless implementations.

The existing encode-matrix scorer is the right accept/reject function for these
probes:

```text
score = 10*log2(current_fps / baseline_fps)
      +  4*log2(baseline_bytes / current_bytes)
      +  8*(current_psnr_db - baseline_psnr_db)
```

The hard guardrails in `scripts/benchmark_encode_matrix.py` still apply: large
FPS regressions, byte growth, or PSNR loss force `regress` even when the scalar
score is positive. This keeps local speed hacks from silently buying FPS with
too much rate or quality loss.

External encoder/paper notes checked:

- x265 uses speed tiers for motion search instead of exhaustive ME by default:
  diamond/hex/UMH/star/SEA/full, subpel refinement levels, early-skip,
  recursive skip, hierarchical ME, WPP, and optional threaded ME. The useful
  lesson for this encoder is to add future VVC inter search in layers:
  zero/merge first, small diamond/hex around predicted MVs next, then optional
  wider patterns only when the cheap score justifies them.
- x265's `fast-intra` and AOM's speed features both support the same local
  pattern we already use in places: cheap candidate generation, shortlist the
  best modes, then run expensive RD only for winners.
- AOM AV1 speed features explicitly prune partition shapes using local variance,
  the best NONE prediction mode, prior split information, and best-RD limits.
  This maps to future VVC QT/BT/TT pruning and to safer current luma/chroma
  candidate pruning.
- VVC partitioning papers consistently report that most practical speed wins
  come from early partition/mode termination, not from small arithmetic
  cleanups. Reported examples include VTM partition-search speedups around 7x at
  about 1.1% bitrate cost, quantization-adaptive early termination around 38%
  time saving at 0.85% BD-BR cost, and random-forest/gradient-guided intra
  search with large time savings and small BDBR losses.

Rejected or risky mappings:

- A coarse lossy BDPCM alignment-only gate was already rejected because it
  gained almost no FPS and badly hurt bytes/PSNR on SceneComposition. BDPCM is
  expensive, but the gate must use a stronger cheap predictor than mode
  alignment alone.
- A lossy `fast-search=lossless-speed` CCLM low-residual gate was rejected.
  Reusing the existing moderate-search `best_score > near_exact_score` CCLM
  gate regressed the first two 50-frame rows against `vvc-preskip-avg8-q19-50f`:
  `SceneComposition_1_420` grew by 1,263 bytes, lost 0.153 dB, and scored
  `-1.4`; `SceneComposition_1_422` grew by 1,485 bytes with negligible FPS
  change and scored `-0.1`. CCLM remains worth checking in lossy
  lossless-speed mode unless a stronger predictor is added.
- Threaded mode decision or parallel ME should wait until the single-threaded
  decision graph is stable. x265 notes that parallel mode decision can disable
  early-outs; for this codebase, preserving early-out semantics is more
  important until the mode search is less wasteful.
- ML partition classifiers from papers are useful as an upper-bound direction,
  but they add model ownership, training data, and determinism concerns. Start
  with deterministic texture/variance/SATD-style rules that can be exhaustively
  validated against the reference decoder.

Next implementable priorities:

1. Add a legal mixed P-slice path so non-skipped CTUs and InterSkip CTUs share
   one slice context instead of relying on CTU-sliced output. This is the
   prerequisite for normal inter modes and better skip coverage.
2. Add a cheap CTU/leaf classifier from already available quantities:
   reconstruction SSE, max absolute delta, variance/edge density, and selected
   intra mode family. Use it only to shortlist candidates; final selected syntax
   must remain in the unified residual path.
3. For future MV search, start with predicted-zero/neighbor candidates and a
   small diamond/hex search using luma SAD/SATD first; only add chroma and
   wider patterns when the luma score is close to the current best.
4. Keep every accepted probe tied to the six-vector 50-frame scorer plus VTM
   smoke/regression validation. Rejected probes should record the score and
   reason here to avoid repeating bad local optima.

### VVC Motion/Mode Follow-Up Sweep

Research checkpoint: `vvc-motion-mode-followup-2026-08-22`.

External scan result:

- x265 exposes motion-search levels from cheap diamond/hex through UMH/star,
  SEA, and exhaustive full search, plus subpel refinement tiers and
  hierarchical ME. It also documents a practical caveat: parallel mode decision
  can disable early-outs, so threading should not be the first fix while the
  single-thread decision graph is still wasteful.
- VTM's random-access config uses TZ search, adaptive search range, Hadamard
  fractional ME, fast encoder decision, fast merge RD, and fast transform-skip
  decisions. The reference encoder therefore treats fast ME/mode decision as
  normal encoder policy, not bitstream semantics.
- VVenC's medium random-access config layers more production heuristics:
  adaptive/faster affine, MMVD, IMV, merge, QTBT, MIP, ISP, transform-skip,
  SCC, intra-estimation decimation, reduced subpel filter taps, and early
  integer-search termination. This reinforces that our VVC encoder should grow
  optional staged search knobs rather than one exhaustive path.
- VVC fast-mode papers point to the same signals: texture complexity, local
  sub-CU variance/difference, neighbouring context, temporal/motion
  correlation, and cheap SATD/SAD/gradient metrics. The useful deterministic
  subset for FrameFinery is variance/edge density, reconstruction SSE,
  max/mean residual delta, selected neighbour modes, and QP-scaled thresholds.

Current local applicability:

- General MV search is not present yet. The predictive VVC path can emit
  all-frame or CTU InterSkip/reuse decisions, but it does not encode nonzero MV
  inter blocks. Motion-search work should therefore start as a new staged
  implementation: zero/merge candidates first, neighbour MVPs next, then small
  diamond/hex luma SAD/SATD, with chroma/wider refinement only when close to
  the current best.
- The current profiler still says mode/quantization work dominates the
  existing path. Near-term FPS attempts should target cheap mode pruning,
  transform-skip/MTS gating, and high-confidence pre-skip classifiers before
  adding a full ME subsystem.
- ML classifiers from the literature are not the right first implementation
  here. They are useful upper-bound evidence, but deterministic features are
  easier to validate, reproduce, and keep in the unified encoder path.

Accepted/rejected scoring remains the existing encode-matrix projection:

```text
score = 10*log2(current_fps / baseline_fps)
      +  4*log2(baseline_bytes / current_bytes)
      +  8*(current_psnr_db - baseline_psnr_db)
```

Hard guardrails remain active in `scripts/benchmark_encode_matrix.py`: large
FPS regressions, byte growth, or PSNR loss force `regress`. For exact-neutral
cleanups, require a clear timing signal; byte-identical output with noise-level
FPS deltas is not enough to keep source churn.

Rejected probe: fused materialized-residual scoring.

The encoder currently materializes residual vectors and then separately scans
them to compute SAD/SSE for luma/chroma candidate scoring. A fused builder was
tested that produced the same residual vectors and score in one pass, including
edge-clamped unusual-geometry coverage. The source diff was reverted because
the 50-frame scorer did not show a reliable gain:

| Vector summary | Bytes/PSNR | FPS signal | Tradeoff |
|---|---|---:|---|
| 6-row VVC lossy six-vector matrix | all rows identical | mixed `-0.02` to `+0.08` FPS | average `+0.0`, 0 accept / 4 watch / 2 regress |

Benchmark artifact:

```text
verification/generated/encode_matrix/vvc-fused-mode-residual-score-q19-50f.md
```

Do not retry this exact fused residual-score cleanup unless another change
makes residual score calculation a measured hotspot again.

Rejected probe: allocation-free RD-cache placeholder.

`quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes_and_scratch_with_mode_hints()`
temporarily moves the luma/chroma RD caches out of the CTU scratch object. A
probe replaced the temporary `Vvc*ModeRdCache::new()` placeholders with
allocation-free empty placeholders so the only allocated cache should be the
one reused across CTUs. This was source-neutral and kept all six 50-frame rows
byte/PSNR-identical, but the scorer did not show a speed win:

| Vector summary | Bytes/PSNR | FPS signal | Tradeoff |
|---|---|---:|---|
| 6-row VVC lossy six-vector matrix | all rows identical | mixed `-0.18` to `+0.02` FPS | average `-0.1`, 0 accept / 3 watch / 3 regress |

Benchmark artifact:

```text
verification/generated/encode_matrix/vvc-rd-cache-empty-placeholder-q19-50f.md
```

The source diff was reverted. Do not repeat this exact placeholder-only change;
if allocation traffic becomes a proven bottleneck later, measure with an
allocator/profile tool before changing the cache ownership shape.

Accepted probe: lossy temporal mode hints, zero-residual only.

The existing temporal intra-mode hint machinery was lossless-only. A lossy
variant was tested for `fast-search=lossless-speed`: predictive frames may reuse
the previous frame's luma/chroma TU mode only when that mode predicts the
current TU with zero residual before quantization. This preserves the unified
quantization/reconstruction path; only the candidate selection gate changes.

An intermediate wider threshold, average absolute residual <= 2 at 8-bit scale,
was rejected first. It completed the first frames but regressed row 1 before the
matrix was stopped:

```text
vvc-lossy-temporal-mode-hints-active-fixed-avgabs2-q19-50f
SceneComposition_1_420: +42,267 bytes, -1.09 dB PSNR, +0.09 FPS, score -8.6
```

The accepted zero-residual gate used the current syntax candidate table for
chroma explicit hints before accepting a temporal explicit mode. This fixed a
real caller-side hazard exposed by the wider probe: a previous chroma explicit
mode can be legal in general but absent from the current co-located-luma
candidate table, in which case entropy coding cannot signal it.

50-frame scorer against `vvc-ctu-interskip-slice-cache-q19-50f`:

| Vector | Bytes delta | FPS delta | PSNR delta | Score |
|---|---:|---:|---:|---:|
| SceneComposition_1_420 | +7,253 (+0.274%) | +0.463 (+8.7%) | -0.000260 dB | +1.19 |
| SceneComposition_1_422 | +13,471 (+0.418%) | +0.363 (+8.5%) | +0.000480 dB | +1.15 |
| screen_wayland_activity_rgb | +14,482 (+0.358%) | +0.928 (+74.7%) | +0.000040 dB | +8.03 |
| MissionControl_420 | +5,578 (+0.059%) | +0.060 (+2.5%) | +0.002340 dB | +0.37 |
| MissionControl_422 | +9,032 (+0.080%) | +0.018 (+1.0%) | +0.000160 dB | +0.14 |
| MissionControl_444 | +15,997 (+0.122%) | -0.016 (-1.3%) | +0.000960 dB | -0.18 |

Aggregate:

```text
verification/generated/encode_matrix/vvc-lossy-temporal-mode-hints-zero-q19-50f.md
average score +1.8, 1 accept / 4 watch / 1 regress
300 frames, 43,912,935 bytes, total FPS 2.28
```

Validation:

```sh
make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required \
  VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed"

make validate-set CODEC=vvc VALIDATION_SET=regression VALIDATION_REFERENCE_MODE=required \
  VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed"

AOMCTC_ROOT=/path/to/aomctc \
make validate-set CODEC=vvc VALIDATION_SET=release-six-vectors-full \
  VALIDATION_LIMIT=6 VALIDATION_FRAMES=3 VALIDATION_DIRECT_SOURCE_FILES=1 \
  VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed"
```

Result: all listed validation runs passed reference reconstruction matching.

Rejected probe: chroma zero-coded residual RD early-out.

Luma RD refinement already skips the rest of its shortlist when the raw mode
quantizes to a near-zero coded residual. The same rule was tested for chroma:
after scoring the raw chroma mode, skip further chroma RD candidates when both
Cb and Cr coded zero residual and combined distortion was at most one unit per
chroma sample. This looked like a plausible way to reduce the current chroma RD
hotspot, but the 5-frame stats scorer rejected it:

```text
verification/generated/encode_matrix/vvc-chroma-zero-rd-skip-stats-5f.md
average score -0.1, 0 accept / 1 watch / 5 regress
```

Measured stats also showed no actual work reduction:

| Metric | Baseline | Probe |
|---|---:|---:|
| `ctu_quantize` | 13,562.1 ms | 13,748.7 ms |
| `chroma_rd_scoring_nanos` | 2,292.5 ms | 2,296.2 ms |
| `chroma_rd_refinement_attempts` | 420,416 | 420,438 |
| `chroma_mode_search_nanos` | 4,137.2 ms | 4,238.4 ms |

The source diff was reverted. Do not re-add the direct luma-style chroma
zero-coded RD early-out unless a later change makes the raw chroma residual
score more predictive of the final chroma RD winner.

Rejected probe: lossy chroma explicit directional search order.

Selected chroma modes in the 5-frame stats run were dominated by horizontal and
vertical, so a low-risk search-order probe tried evaluating explicit chroma
horizontal/vertical before the syntax-table order under
`lossy + fast-search=lossless-speed`. The syntax candidate table and entropy
coding remained unchanged; only the order of candidate scoring changed.

The probe was rejected:

```text
verification/generated/encode_matrix/vvc-chroma-explicit-order-stats-5f.md
average score -0.4, 0 accept / 1 watch / 5 regress
```

Instrumentation confirmed the change did not reduce work:

| Metric | Baseline | Probe |
|---|---:|---:|
| `ctu_quantize` | 13,562.1 ms | 14,002.3 ms |
| `chroma_mode_search_nanos` | 4,137.2 ms | 4,360.2 ms |
| `chroma_candidate_count` | 3,769,955 | 3,770,817 |
| `chroma_candidate_explicit` | 1,727,200 | 1,727,489 |
| `chroma_candidate_cclm` | 1,290,924 | 1,291,500 |

The source diff was reverted. If chroma mode search order is revisited, it
needs a stronger early-stop condition; reordering alone is noise or worse.

Rejected probe: lossy CCLM Linear-only fast search.

The 5-frame stats run showed that CCLM was evaluated far more often than it was
selected, and MDLM-left/top accounted for a large fraction of CCLM cost. A
probe limited `lossy + fast-search=lossless-speed` CCLM mode generation to
Linear only, keeping the normal full CCLM set for other policies.

It produced the intended timing reduction but failed the quality/rate tradeoff:

```text
verification/generated/encode_matrix/vvc-cclm-linear-only-stats-5f.md
average score -1.3, 0 accept / 2 watch / 4 regress
```

Worst rows:

- `screen_wayland_activity_rgb`: +11,421 bytes, -0.75 dB, +0.05 FPS, score -6.1.
- `SceneComposition_1_420`: +2,536 bytes, -0.22 dB, +0.02 FPS, score -1.7.

Instrumentation showed why the idea was tempting:

| Metric | Baseline | Probe |
|---|---:|---:|
| `ctu_quantize` | 13,562.1 ms | 12,976.7 ms |
| `chroma_mode_search_nanos` | 4,137.2 ms | 3,223.7 ms |
| `chroma_cclm_prediction_nanos` | 1,191.0 ms | 463.3 ms |
| `chroma_candidate_cclm` | 1,290,924 | 430,772 |

The source diff was reverted. CCLM cannot be cut down to Linear-only for the
current lossy fast mode; MDLM-left/top matter enough on screen-content rows
that the quality loss overwhelms the speed win.

Rejected follow-up: CCLM Linear-only for 10-bit 4:2:0/4:2:2 only.

A narrower variant kept full CCLM on 8-bit/RGB and 10-bit 4:4:4 while limiting
only `lossy + fast-search=lossless-speed` 10-bit 4:2:0/4:2:2 rows to Linear
CCLM. This avoided the worst Wayland/RGB failure from the broad Linear-only
probe, but still failed the aggregate score:

```text
verification/generated/encode_matrix/vvc-cclm-linear-only-10bit-420-422-stats-5f.md
average score -0.2, 0 accept / 2 watch / 4 regress
```

Affected rows still paid rate/quality for speed:

- `MissionControl_420`: +3,402 bytes, -0.05 dB, +0.07 FPS, score +0.1.
- `MissionControl_422`: +2,025 bytes, ~0 dB, +0.11 FPS, score +1.0.

Instrumentation moved in the intended direction but not enough to overcome the
tradeoff:

| Metric | Baseline | Probe |
|---|---:|---:|
| `ctu_quantize` | 13,562.1 ms | 13,539.4 ms |
| `chroma_mode_search_nanos` | 4,137.2 ms | 3,814.8 ms |
| `chroma_cclm_prediction_nanos` | 1,191.0 ms | 869.7 ms |
| `chroma_candidate_cclm` | 1,290,924 | 979,265 |

The source diff was reverted. Any future CCLM pruning needs content-adaptive
evidence stronger than format/bit-depth alone.

### Motion Search And Mode-Decision Research Triage

Research and production encoder practice point to fast-search heuristics that
can improve FPS with controlled byte/PSNR risk, but only when the heuristic is
measured as a rate-quality-speed tradeoff rather than as speed alone.

Observed external patterns worth adapting:

- VVenC exposes presets as a Pareto tradeoff over encoder tools. Its medium
  random-access configuration keeps broad motion range but enables adaptive
  search range, fast merge decisions, fast subpel search, SCC-aware BDPCM/IBC,
  content-based QTBT speedups, reduced intra chroma full-RD modes, and several
  inter-tool fast modes.
- x265's speed knobs follow the same structure: early skip after no-residual
  merge, recursion skip using neighbor/homogeneity or edge density, fast intra
  angular scans, reduced merge/ref candidates, diamond ME, and lower RD levels
  for first-pass/turbo analysis.
- Fast inter-prediction literature commonly uses merge/skip outcomes and
  rolling RD-cost history to decide whether later inter modes or larger motion
  searches are worth running. One HEVC inter paper reports cutting search
  range to 2 after a selected skip candidate because the best MV remains within
  a tiny range with high probability.
- VVC fast CU/mode papers generally use cheap texture features before expensive
  RD: entropy/texture contrast, variance of sub-CU variance, Laplacian or HOG
  direction estimates, and soft decisions that keep more candidates when the
  classifier confidence is low.
- SVT-AV1 documents a useful future inter-search architecture: open-loop
  hierarchical ME on downsampled source frames, multiple search centers to
  avoid local minima, then a smaller full-resolution search. It also adjusts
  search ranges based on reference distance, HME SAD, near-zero motion, and
  prunes references whose SAD is far from the best. This is future-only until
  FrameFinery VVC has normal nonzero-MV inter blocks, but it argues against
  starting with an exhaustive single-resolution search.
- VTM and x265 both expose encoder-policy search knobs rather than treating
  motion/mode search as syntax: VTM has full/diamond/selective/enhanced-diamond
  search, search-range, minimum-window, smoother-MV, Hadamard-ME, and adaptive
  search-range controls; x265 exposes merge-candidate limits, reference/mode
  limits, ME patterns from diamond through exhaustive full search, hierarchical
  ME, early-skip, recursion skip, fast intra, and transform-skip fast modes.
- rav1e's AV1 ME code is a useful Rust-side reference for implementation shape:
  it stores per-block ME stats, reuses subset predictors from neighboring and
  previous searches, performs coarse-to-fine passes, and uses an uneven
  multi-hexagon refinement stage for harder motion.

Implications for FrameFinery VVC:

1. Keep correctness gates first: every accepted search/mode shortcut must still
   pass internal reconstruction and reference-decoder validation.
2. Score lossy probes with the encode-matrix tradeoff projection implemented by
   `project_metric_tradeoff()` in `scripts/benchmark_encode_matrix.py`:
   `10*log2(fps ratio) + 4*log2(byte inverse ratio) + 8*PSNR delta`.
   This makes a 10% speed gain worth roughly +1.4 points, a 10% byte increase
   worth roughly -0.55 points, and a 0.1 dB PSNR loss worth -0.8 points.
3. Prefer deterministic, content-adaptive gates before ML classifiers: skip or
   narrow expensive candidates only when cheap residual, texture, or neighbor
   evidence is strong. The rejected CCLM probes show that format-only pruning
   is too blunt.
4. Highest-priority candidate probes:
   - rolling CTU-depth skip/merge RD thresholds for temporal/inter mode
     pruning;
   - search-range narrowing when a temporal/merge candidate has zero or tiny
     residual;
   - staged luma/chroma mode selection using cheap SSE/SATD and directional
     gradients before full RD;
   - allocation- and entropy-build reductions that keep bytes and PSNR exactly
     unchanged.
5. Avoid splitting the encoder into independent lossy/lossless paths. Add
   gates at the deepest candidate/tool-selection level so syntax,
   reconstruction, residual coding, and validation stay shared.

Rejected probe: frame-level VVC quant scratch reuse.

A low-risk allocation probe hoisted `VvcLumaModeSearchState` and
`VvcCtuQuantScratch` out of the frame loop, clearing the luma mode map at the
start of each frame and reusing CTU quantization scratch buffers across frames.
The change was byte/PSNR neutral on the locally available Wayland row, and VTM
reference validation passed:

```text
make validate-set CODEC=vvc \
  VALIDATION_SET=wayland-vvc-probe \
  VALIDATION_SET_DIR=verification/generated/agent_scratch \
  VALIDATION_REFERENCE_MODE=required \
  VALIDATION_DIRECT_SOURCE_FILES=1 \
  VALIDATION_FRAMES=50 \
  VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS='qp=19 fast-search=lossless-speed gop=-1' \
  VALIDATION_CLEANUP_RECON=1 \
  VALIDATION_CLEANUP_OUTPUT=1
```

However the one-row encode-matrix comparison against
`vvc-lossy-temporal-mode-hints-zero-q19-50f` rejected it:

```text
verification/generated/encode_matrix/vvc-frame-scratch-reuse-wayland-q19-50f.md
screen_wayland_activity_rgb: bytes +0, PSNR +0.000, FPS 2.17 -> 2.12,
score -0.3:regress
```

The source diff was reverted. This may simply be timing noise, but it is not a
measurable win. Revisit only if allocation profiling later shows per-frame
scratch construction as a confirmed hotspot or if the full six-vector matrix
with `AOMCTC_ROOT` available shows a different result.

Rejected probe: CTU-slice InterSkip Annex-B streaming.

The stats hotspot suggested that cached InterSkip CTU-slice payloads might be
paying avoidable clone/allocation cost: `VvcCtuInterSkipSlicePayloadCache`
stores RBSP payloads, but `slice_unit_for` clones the cached payload into a
fresh `VvcNalUnit`, and `write_annex_b_to` then builds a second Annex-B `Vec`.

Two byte-equivalent implementations were tried:

- a borrowed-payload Annex-B writer that emitted picture-header and CTU-slice
  NAL units directly into the output writer;
- the same borrowed-payload writer targeting a local Annex-B `Vec`, followed by
  one write to the output stream.

Both variants passed the local byte-equivalence unit test and VTM-required
Wayland validation:

```text
cargo test -p framefinery-codecs --features vvc \
  vvc_predictive_ctu_inter_skip_streaming_writer_matches_cached_units

make validate-set CODEC=vvc \
  VALIDATION_SET=wayland-vvc-probe \
  VALIDATION_SET_DIR=verification/generated/agent_scratch \
  VALIDATION_REFERENCE_MODE=required \
  VALIDATION_DIRECT_SOURCE_FILES=1 \
  VALIDATION_FRAMES=50 \
  VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS='qp=19 fast-search=lossless-speed gop=-1' \
  VALIDATION_CLEANUP_RECON=1 \
  VALIDATION_CLEANUP_OUTPUT=1
```

The scorer rejected both on the available 50-frame Wayland row:

```text
verification/generated/encode_matrix/vvc-ctu-slice-stream-writer-wayland-q19-50f.md
bytes +0, PSNR +0.000, FPS 2.17 -> 2.16, score -0.1:regress

verification/generated/encode_matrix/vvc-ctu-slice-annexb-vec-wayland-q19-50f.md
bytes +0, PSNR +0.000, FPS 2.17 -> 2.08, score -0.6:regress
```

The source diff was reverted. The current evidence says CTU-slice payload
cloning is not worth optimizing this way; if entropy build remains hot, inspect
CABAC payload generation and slice count first rather than just changing
ownership/write plumbing.

Rejected probe: repeated predictive reconstruction byte cache.

The Wayland screen-capture row has many repeated predictive frames. The
repeated-frame path currently calls `VvcReconstructionFrame::to_yuv()` every
time it needs internal reconstruction bytes for metrics or optional
reconstruction output. Two exact-neutral cache variants were tested:

- lazily cache a `Vec<u8>` raw reconstruction inside `VvcPredictiveFrameCache`
  and clone it for repeated frames;
- lazily cache an `Arc<[u8]>` raw reconstruction and return shared bytes for
  repeated frames, avoiding full-frame clones.

Both variants passed focused predictive tests, but the one-row 50-frame Wayland
scorer rejected them against `vvc-lossy-temporal-mode-hints-zero-q19-50f`:

```text
verification/generated/encode_matrix/vvc-repeated-yuv-cache-wayland-q19-50f.md
bytes +0, PSNR +0.000, FPS 2.17 -> 2.18, score +0.1:watch

verification/generated/encode_matrix/vvc-repeated-yuv-arc-cache-wayland-q19-50f.md
bytes +0, PSNR +0.000, FPS 2.17 -> 2.14, score -0.2:regress
```

The source diffs were reverted. Repacking repeated reconstruction bytes is not
a proven bottleneck under the current benchmark shape; if repeated-frame FPS is
revisited, first separate encoder time from `--psnr` metrics and raw
reconstruction-output cost.

Rejected probe: 8-bit 4:4:4 BDPCM-aligned raw chroma search skip.

Wayland stats showed that 8-bit 4:4:4 lossy `fast-search=lossless-speed`
selects many horizontal/vertical chroma modes and spends heavily in chroma
prediction/search. A narrow probe skipped explicit and CCLM raw chroma
candidate generation when the co-located luma mode was already horizontal or
vertical, leaving raw chroma mode as Derived and relying on the existing direct
BDPCM selector to switch to horizontal/vertical BDPCM only if the residual
safety check accepted it.

The implementation remained unified and only changed candidate generation, but
the 50-frame Wayland scorer rejected it against a fresh normal-build baseline:

```text
verification/generated/encode_matrix/vvc-wayland-pre-bdpcm-raw-gate-q19-50f.md
bytes=4,060,779 fps=2.04 psnr=58.997

verification/generated/encode_matrix/vvc-wayland-bdpcm-raw-gate-q19-50f.md
bytes=6,244,380 fps=2.41 psnr=59.724
delta: +2,183,601 bytes, +0.37 FPS, +0.727 dB, score +5.7:regress
```

The scalar score was positive because PSNR/FPS improved, but byte growth was
53.8%, exceeding the hard 1.20x byte-regression guardrail. The source diff was
reverted. Do not skip full raw chroma search purely from luma horizontal/vertical
alignment on 8-bit 4:4:4; if this area is revisited, require a byte-aware cheap
predictor or an RD-preserving early stop rather than forcing Derived+BDPCM.

Rejected probe: 8-bit 4:4:4 luma RD-refinement skip.

The 10-frame Wayland stats run
`vvc-wayland-stats-current-10f-q19` showed 11,617 luma RD-refinement attempts
with zero luma RD switches, costing about 284 ms in the stats build. A scoped
probe disabled luma RD refinement only for lossy `fast-search=lossless-speed`
8-bit 4:4:4 streams. This did not change syntax decisions on the available
Wayland row, but it still regressed runtime:

```text
verification/generated/encode_matrix/vvc-wayland-pre-bdpcm-raw-gate-q19-50f.md
bytes=4,060,779 fps=2.04 psnr=58.997

verification/generated/encode_matrix/vvc-wayland-luma-rd-skip-444-q19-50f.md
bytes=4,060,779 fps=0.95 psnr=58.997
delta: +0 bytes, -1.09 FPS, +0.000 dB, score -11.0:regress
```

The likely cause is that the current luma RD path computes and carries the
selected residual into finalization. Skipping RD refinement avoids the shortlist
comparison but also prevents finalization from reusing that scored residual, so
the work moves rather than disappearing. The source diff was reverted. Revisit
this area only by preserving residual reuse while avoiding redundant candidate
scoring, not by bypassing luma RD wholesale.

Rejected probe: 8-bit 4:4:4 final luma MTS refinement skip.

A narrower follow-up kept luma RD refinement and residual reuse intact, but
skipped only the late explicit-MTS refinement for lossy
`fast-search=lossless-speed` 8-bit 4:4:4 streams. This was based on Wayland
stats where `luma_mts_nonzero_count` was zero, so the probe tried to avoid
redundant post-RD MTS work without changing the earlier shared mode path.

The focused Wayland scorer against a fresh local baseline was byte/PSNR
identical but only reached watch-level timing:

```text
verification/generated/encode_matrix/vvc-wayland-final-mts-skip-444-q19-50f.md
bytes=4,060,779 fps=2.13 psnr=58.997
delta: +0 bytes, +0.10 FPS, +0.000 dB, score +0.7:watch
```

The comparable six-vector 50-frame scorer against
`vvc-lossy-temporal-mode-hints-zero-q19-50f` rejected the affected 8-bit 4:4:4
Wayland row:

```text
verification/generated/encode_matrix/vvc-final-mts-skip-444-q19-50f-50f
screen_wayland_activity_rgb: bytes +0, PSNR +0.000, FPS 2.17 -> 2.16,
score -0.1:regress
```

The source diff was reverted. Do not retry a format-only final-MTS skip unless
new instrumentation proves final explicit-MTS refinement is a stable hotspot
and the skip is selected from per-TU evidence rather than only from 8-bit 4:4:4
format policy.

Rejected probe: direct chroma BDPCM candidate reuse.

The lossy 8-bit 4:4:4 direct chroma BDPCM path evaluates the same H/V BDPCM
candidates as the later normal BDPCM loop when the direct safety gate does not
early-return. A probe tried to reuse those scored direct candidates as the
normal BDPCM decision result, preserving the current early-return rule by
capturing the original baseline residual SSE before any candidate buffer swaps.

An intermediate version exposed the exact hazard in this area: updating the
selected residual buffers before later direct-safety checks changed the
Wayland bitstream by 14 bytes. The corrected version restored byte/PSNR
identity, but the scorer still rejected it:

```text
verification/generated/encode_matrix/vvc-direct-bdpcm-reuse-wayland-q19-50f.md
bytes=4,060,779 fps=2.14 psnr=58.997
delta: +0 bytes, +0.10 FPS, +0.000 dB, score +0.7:watch

verification/generated/encode_matrix/vvc-direct-bdpcm-reuse-fixed-q19-50f-limit3.md
screen_wayland_activity_rgb: bytes +0, PSNR +0.000, FPS 2.17 -> 2.11,
score -0.4:regress
```

The source diff was reverted. The evidence suggests direct BDPCM either
early-returns often enough that duplicate candidate evaluation is not the real
cost, or the extra control-flow/helper overhead cancels the saved work. Do not
retry this exact reuse shape; if chroma BDPCM remains a hotspot, add candidate
hit/miss counters first and optimize only the measured non-early-return cases.

Follow-up stats counters confirmed that direct BDPCM usually early-returns on
the affected Wayland row:

```text
verification/generated/encode_matrix/vvc-bdpcm-counter-smoke-5f.md
chroma_bdpcm_direct_candidates=217,567
chroma_bdpcm_direct_safe_candidates=211,972
chroma_bdpcm_direct_selected=211,971
chroma_bdpcm_regular_candidates=18,645
chroma_bdpcm_regular_best_updates=9,649
```

The remaining chroma BDPCM work is therefore mostly the selected direct path
itself plus the smaller regular fallback, not duplicated direct candidates.

Rejected probe: skip RD scoring for unsafe direct chroma BDPCM candidates.

The direct BDPCM safety gate is computed from raw residual SSE, but the old
order scored the quantized candidate before checking that gate. A probe moved
the safety check before quantized residual scoring so unsafe direct candidates
skip work they cannot use. This preserved bytes and PSNR, but did not produce a
clear timing win:

```text
verification/generated/encode_matrix/vvc-direct-bdpcm-unsafe-score-skip-wayland-q19-50f.md
bytes=4,060,779 fps=2.13 psnr=58.997
delta: +0 bytes, +0.09 FPS, +0.000 dB, score +0.7:watch

verification/generated/encode_matrix/vvc-direct-bdpcm-unsafe-score-skip-q19-50f-limit3.md
screen_wayland_activity_rgb: bytes +0, PSNR +0.000, FPS 2.17 -> 2.16,
score -0.1:regress
```

The source diff was reverted. The unsafe direct-candidate fraction is too small
for this ordering change to matter under the current scorer.

Rejected probe: immediate selection for safe direct chroma BDPCM.

The next probe treated the raw-SSE direct safety gate as sufficient for lossy
8-bit 4:4:4 direct chroma BDPCM and returned immediately without full RD
scoring. This is the larger possible speed tradeoff because Wayland stats show
almost every direct-safe candidate is selected by the normal RD check. It
slightly reduced bytes on the affected row, but still failed the timing scorer:

```text
verification/generated/encode_matrix/vvc-direct-bdpcm-safe-immediate-wayland-q19-50f.md
bytes=4,060,774 fps=2.17 psnr=58.997
delta: -5 bytes, +0.13 FPS, +0.000 dB, score +0.9:watch

verification/generated/encode_matrix/vvc-direct-bdpcm-safe-immediate-q19-50f-limit3.md
screen_wayland_activity_rgb: bytes -5, PSNR +0.000, FPS 2.17 -> 2.12,
score -0.4:regress
```

The source diff was reverted. Direct-safe BDPCM immediate selection is not a
reliable speed win under the current single-threaded benchmark; future work
should target the cost of BDPCM prediction/residual generation itself or move
to structural slice/inter improvements.

Rejected probe: fused chroma BDPCM prediction and residual build.

The direct and regular chroma BDPCM candidate loops currently predict Cb/Cr and
then build residual vectors in separate passes. A byte-equivalent helper was
tested that fused BDPCM prediction and residual construction for each chroma
plane while preserving the same reference availability and visible-edge clamp
behavior. Focused unit coverage proved the fused helper matched the separate
path for horizontal and vertical BDPCM on an edge-clamped 4:2:0 block, and VVC
tests passed with and without `vvc-stats`.

The scorer rejected the change. The focused Wayland row was byte/PSNR-identical
but slightly slower:

```text
verification/generated/encode_matrix/vvc-chroma-bdpcm-fused-predict-residual-wayland-q19-50f.md
screen_wayland_activity_rgb: bytes +0, PSNR +0.000, FPS 2.04 -> 2.03,
score -0.1:regress
```

The comparable first-three six-vector run also rejected all rows:

```text
verification/generated/encode_matrix/vvc-chroma-bdpcm-fused-predict-residual-q19-50f-limit3.md
SceneComposition_1_420: bytes +0, PSNR +0.000, FPS 5.77 -> 5.42, score -0.9:regress
SceneComposition_1_422: bytes +0, PSNR +0.000, FPS 4.64 -> 4.40, score -0.8:regress
screen_wayland_activity_rgb: bytes +0, PSNR +0.000, FPS 2.17 -> 1.98, score -1.3:regress
```

The source diff was reverted. Do not retry this exact fused helper shape; it
likely makes the hot loop less optimizer-friendly or changes cache/control-flow
behavior enough to erase the saved residual pass.

Accepted structural probe: lossy mixed single P-slice packetization.

The CTU-sliced predictive path can now emit 4:2:0 lossy mixed frames with intra
CTUs and `InterSkip` CTUs in one P-slice. The fix was not a
slice-header/PPS-only change: intra CTUs inside the P-slice use inter-slice
split constraints, single-tree intra prediction syntax, and inline chroma
CBF/residual syntax at the VTM `transform_unit()` site. A focused 128x64
two-frame YUV420 probe was generated under
`verification/generated/agent_scratch/mixed_p_slice/` with the left CTU changed
and the right CTU repeated. VTM accepted the lossy QP 19 stream and the
reference reconstruction matched the encoder reconstruction:

```text
lossy_q19.vvc: 229 bytes
sha256(lossy_q19.recon.yuv) =
sha256(lossy_q19.vtm.yuv)   = fef53fa65db414b929185ee8356dfb76d8a942ac8294423ffe342e13efb60e71
```

Lossless mixed CTU skip and lossy 4:2:2/4:4:4 mixed CTU skip intentionally
remain on the CTU-slice fallback. The lossless path uses 4x4 luma leaves for
4:2:0, where VTM infers local separate luma/chroma trees in inter slices. The
focused lossless probe remains reference-clean through the fallback:

```text
lossless.vvc: 550 bytes
sha256(lossless.recon.yuv) =
sha256(lossless.vtm.yuv)   = 062e62b51aa375585ef2d2fae22c39f31d298da3689c25a1e1227af55eb0d808
```

Follow-up probe: high-chroma mixed single P-slice packetization.

The next attempt made quantization carry the active coding-tree policy and use
the inter-slice single-tree transform partition order before entropy. This fixed
the original 4:2:2 mismatch mechanism where dual-tree chroma produced a
different TU count/order than single-tree entropy. Focused 128x64 two-frame
4:2:2 and 4:4:4 QP 19 probes then decoded successfully with VTM, and the VTM
reconstruction matched the encoder reconstruction byte-for-byte.

That was still not sufficient for release enablement. Single-tree 4:2:2/4:4:4
creates rectangular chroma TUs such as 4x8 and 8x8. The current chroma BDPCM and
transform-skip coefficient storage is still fundamentally 4x4-oriented, so
BDPCM had to be gated to 4x4 chroma TUs to keep focused probes
reference-clean. The resulting all-chroma single-slice 50-frame six-vector
matrix was rejected by the score gate:

```text
verification/generated/encode_matrix/vvc-mixed-p-slice-all-chroma-q19-50f.md
average score: -42.2
SceneComposition_1_422: bytes -283,500, FPS +1.35, PSNR -6.16 dB, score -44.9
screen_wayland_activity_rgb: bytes -3,365,591, FPS +4.75, PSNR -13.36 dB, score -79.0
MissionControlClip1_422: bytes +609,948, FPS +0.26, PSNR -6.50 dB, score -50.2
MissionControlClip1_444: bytes +1,898,815, FPS +0.20, PSNR -10.22 dB, score -80.1
```

The release path was narrowed back to 4:2:0-only mixed single P-slices and now
has explicit 4:2:2/4:4:4 fallback tests. The guarded matrix is byte/PSNR
identical to the previous accepted baseline, with only FPS noise:

```text
verification/generated/encode_matrix/vvc-mixed-p-slice-chroma-gated-q19-50f.md
average score: +0.0
all six rows: bytes +0, PSNR +0.00 dB
FPS deltas: -0.12, +0.12, -0.03, -0.07, +0.03, +0.04
```

Remaining TODO: implement the local-separate-tree branch for small 4:2:0
inter-slice intra leaves and add a real rectangular chroma residual/BDPCM model
before enabling 4:2:2/4:4:4 mixed single P-slices or leaf-level predictive skip.

### VVC Motion Search And Mode-Selection Audit

Research checkpoint: `vvc-motion-mode-audit-2026-08-22`.

The current FrameFinery VVC encoder still does not have general nonzero-MV
inter coding. Predictive VVC frames can emit all-frame/CTU `InterSkip` and can
reuse cached reconstruction/decisions, but there is no translational or affine
motion-vector search comparable to VTM, VVenC, x265, AOM, or rav1e yet. For the
current codebase, motion-search research should therefore shape the next inter
milestone rather than drive isolated tuning of a nonexistent ME path.

Primary source implications:

- VTM's `InterSearch` uses TZ-style searches: start from MVP/zero/cached or
  hash candidates, test local diamond/square patterns, optionally refine, then
  proceed to subpel and RD. The direct lesson is to introduce future VVC ME in
  layers: zero/merge first, then a small predictor-centered diamond/TZ search,
  then optional wider/hash search only when the cheap score justifies it.
- VTM merge mode decision is two-pass: SATD-like candidate pruning first, then
  full RD on a shortlist. That maps cleanly to our existing unified-path rule:
  compute cheap source/reconstruction/texture scores before residual
  materialization, then call the normal residual/CABAC path only for survivors.
- x265 exposes the same tradeoff shape through presets, RD levels, merge
  candidate limits, subpel refinement levels, HME, WPP, and threaded ME. The
  practical lesson is that search scope should be staged and preset-controlled,
  not implemented as one exhaustive default path.
- AOM's speed-feature catalog is directly relevant for future AV2/VVC work:
  reduce MV step size, skip motion search when zero-MV SSE is low, reuse simple
  motion search for partition pruning, suppress duplicate MV candidates, and use
  model/estimated RD before full transform search.
- VVenC's preset model is explicitly Pareto-oriented. We should keep using
  measured `[bytes, PSNR, FPS]` score gates rather than accepting a faster path
  merely because it is standard-encoder-inspired.
- VVC QT+MTT papers consistently point at variance, gradient, Laplacian, or
  learned classifiers for partition/mode pruning. This is useful future
  guidance, but FrameFinery currently uses a fixed residual tree; flexible
  partitioning should not be added with exhaustive recursion as the first
  implementation.

Current priority order:

1. Keep exploiting high-confidence skip before intra quantization. The accepted
   average-SSE CTU pre-skip follows the same principle as AOM's zero-MV
   low-SSE skip and VTM/x265 skip-first merge handling: when the cheap skip
   predictor is strong enough, avoid expensive residual work.
2. Extend the legal mixed inter-slice intra syntax to the remaining
   local-separate-tree cases and align 4:2:2/4:4:4 quantization with the
   inter-slice partition plan before enabling those mixed single P-slices or
   leaf-level skip. The 4:2:0 lossy CTU-level mixed P-slice path is now
   reference-clean and should be the baseline for future inter work.
3. Add a cheap mode shortlist before residual materialization. Use luma/chroma
   source gradients, template availability, previous-frame skip distortion, and
   QP-scaled tolerances to decide which intra/chroma candidates deserve full
   RD. Do not repeat the rejected raw 2x-gap top-one gate without stronger
   texture or predictor evidence.
4. When nonzero-MV inter is introduced, start with whole-CTU or current-leaf
   full-pel translational candidates in this order: zero MV, spatial neighbor
   MVPs, temporal/cached MVP, duplicate-suppressed small diamond/TZ around the
   best predictor, and only then optional wider/hash search for screen content.
5. Defer affine, bidirectional refinement, weighted prediction, and large
   flexible partition trees until the simple translational inter path is
   reference-clean and score-positive.

Actionable addendum from the 2026-08-22 external/source audit:

- VVC lossy predictive skip is currently selected with a distortion-only gate:
  the previous reconstruction's CTU SSE must be no worse than the intra
  reconstruction SSE. That is too conservative for a rate-distortion encoder
  because InterSkip can legitimately spend fewer bits at a small distortion
  cost. The next targeted probe should estimate actual CTU-slice bits using the
  existing CABAC payload path and select skip by `distortion + lambda*bits`,
  measured against the six-vector scorer. This must use the same CTU payload
  builders as final emission; do not add a parallel syntax estimator.
- IBC is the only current nonzero-vector-like VVC search path. It is intentionally
  narrow: exact 8x8 hash matches from A1/B1/B0 plus the current BVP/HMVP syntax
  subset. A future screen-content probe can broaden candidate discovery to
  capped CTU-local hash matches or HMVP-directed candidates, but only after
  adding candidate counters and reference validation. Do not turn IBC into an
  uncapped CTU scan.
- General translational inter ME is still future work. Start it as a staged
  search, not as exhaustive ME: zero/merge, spatial MVP, temporal/cached MVP,
  duplicate suppression, small diamond/TZ full-pel search, then optional
  subpel/chroma/wider search only when cheap luma cost is close to the current
  best. This mirrors VTM/x265/VVenC without importing their whole decision
  graphs.
- VVenC and recent VVC complexity-reduction papers both point to partition and
  mode pruning as the largest future source of FPS. FrameFinery currently avoids
  the full QTMT explosion with a fixed residual tree, so the next local
  equivalent is cheap candidate pruning before residual materialization, not a
  large new recursive partition search.
- The existing `[bytes, PSNR, FPS]` projection is adequate for accepting these
  probes. If a change is byte-identical, require a clear FPS signal; if it is
  output-changing, require positive aggregate score with no hard row
  regressions and then VTM-required validation.

Rejected probe: exact CABAC-bit RD gate for lossy CTU InterSkip.

The lossy predictive CTU skip selector was changed from distortion-only
`skip_sse <= intra_sse` to a local RD score using actual CABAC body bit counts
from `vvc_ctu_cabac_payload()`:

```text
cost = distortion + lambda(qp, bit_depth) * cabac_bits
```

The implementation reused the final CTU CABAC payload builder for both the
P-slice InterSkip candidate and the I-slice intra candidate, so it did not add
a parallel syntax estimator. Unit coverage for the selector and broad VVC
tests passed, but the 50-frame six-vector scorer rejected the probe:

```text
verification/generated/encode_matrix/vvc-lossy-skip-rd-bitcost-q19-50f.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| SceneComposition_1_420 | +0 | +0.000 | -0.99 | `-2.8 regress` |
| SceneComposition_1_422 | +0 | +0.000 | -0.56 | `-1.9 regress` |
| screen_wayland_activity_rgb | +0 | +0.000 | -0.06 | `-0.4 regress` |
| MissionControlClip1_420 | +0 | +0.000 | +0.05 | `+0.3 watch` |
| MissionControlClip1_422 | +0 | +0.000 | +0.04 | `+0.3 watch` |
| MissionControlClip1_444 | +0 | +0.000 | +0.03 | `+0.4 watch` |

Aggregate score was `-0.7` with 0 accepts, 3 watches, and 3 regressions. The
source change was reverted. The result means the existing bounded near-skip
candidates did not expose additional RD-positive CTU skips on this matrix, and
running exact CABAC bit estimation inside the hot skip decision adds too much
overhead. If this idea is retried, first add a cheaper precondition or a cached
per-CTU intra bit estimate so exact CABAC scoring only runs when it can change
the decision.

Tradeoff gate:

Use `scripts/benchmark_encode_matrix.py` as the acceptance function. It already
projects each comparable row into:

```text
10*log2(current_fps / baseline_fps)
+4*log2(baseline_bytes / current_bytes)
+8*(current_psnr_db - baseline_psnr_db)
```

Correctness still gates first: the candidate must pass unit tests and VTM
reference decode validation before the score matters. For output-changing
lossy probes, require the six-vector 50-frame scorer to show a positive
aggregate result without hard per-row regressions. For byte-identical probes,
classify small mixed FPS deltas as timing noise unless the aggregate score is
negative or the same row regresses repeatedly. For exact-neutral source edits,
keep the change only when it improves the current hotspot on a representative
matrix; otherwise revert and document it.

The aggregate scorer is centralized in `scripts/encode_tradeoff.py`. It marks
hard row regressions separately from noisy row classifications: FPS below 0.90x,
bytes above 1.20x, or PSNR below -1.0 dB fail the aggregate result even if
another row is much faster. Without a hard regression, average score >= 2.0 is
an aggregate `accept`, average score >= 0 is `watch`, and a negative average is
`regress`.

Rejected probe: lossy luma spatial-consensus seed gate.

A narrow deterministic mode-pruning probe changed `lossy +
fast-search=lossless-speed` luma directional candidate generation so a close
left/above spatial consensus skipped the source-gradient seed scan and used the
consensus candidate first. This matched the research pattern of using
neighbouring context before expensive candidate generation, but it was not a
good local tradeoff: the source-gradient seed still carries useful decisions,
and removing it did not produce a timing win.

The source diff passed the focused luma directional-search unit tests, but the
first-three 50-frame six-vector scorer rejected it against
`vvc-lossy-temporal-mode-hints-zero-q19-50f`:

```text
verification/generated/encode_matrix/vvc-luma-spatial-consensus-seed-q19-50f-limit3.md
```

| Vector | Bytes delta | FPS delta | PSNR delta | Tradeoff |
|---|---:|---:|---:|---|
| SceneComposition_1_420 | +1,681 | -0.23 | -0.00 | `-0.6 regress` |
| SceneComposition_1_422 | +2,158 | -0.06 | +0.00 | `-0.2 regress` |
| screen_wayland_activity_rgb | +222 | -0.11 | -0.00 | `-0.7 regress` |

Average score was `-0.5`, with all three rows classified as regressions. The
source diff was reverted. Do not prefer spatial-neighbour luma consensus over
the source-gradient seed in lossy lossless-speed mode unless a later
precondition proves the seed is duplicate or RD-irrelevant for the specific TU.

Rejected probe: exact `gxy == 0` luma gradient seed fast-path.

The luma source-gradient seed maps an axis-aligned structure tensor
(`gxy == 0`) exactly to mode index 18 or 50 under the current floating-point
formula, so a source-equivalent shortcut was tried to bypass `atan2()` for that
case. The focused seed unit test passed and the first-three 50-frame matrix was
byte/PSNR identical, but timing regressed on every measured row:

```text
verification/generated/encode_matrix/vvc-luma-axis-seed-fastpath-q19-50f-limit3.md
```

| Vector | Bytes delta | FPS delta | PSNR delta | Tradeoff |
|---|---:|---:|---:|---|
| SceneComposition_1_420 | +0 | -0.26 | +0.00 | `-0.7 regress` |
| SceneComposition_1_422 | +0 | -0.20 | +0.00 | `-0.6 regress` |
| screen_wayland_activity_rgb | +0 | -0.13 | +0.00 | `-0.9 regress` |

Average score was `-0.7`, with all three rows classified as regressions. The
source diff was reverted. The branch either does not trigger often enough or
hurts the hot gradient path more than the avoided `atan2()` helps under the
current release build.

Rejected probe: CCLM prediction resize without clear.

The CCLM predictor output buffer is fully overwritten after it is resized, so
an exact-neutral probe removed the preceding `prediction.clear()` and let
`Vec::resize()` keep the existing initialized contents when the block size was
unchanged. This looked like a possible way to avoid zero-filling in a measured
hot CCLM path, but the release scorer rejected it even though bytes and PSNR
were identical:

```text
verification/generated/encode_matrix/vvc-cclm-resize-no-clear-q19-50f-limit3.md
```

| Vector | Bytes delta | FPS delta | PSNR delta | Tradeoff |
|---|---:|---:|---:|---|
| SceneComposition_1_420 | +0 | -0.40 | +0.00 | `-1.0 regress` |
| SceneComposition_1_422 | +0 | -0.34 | +0.00 | `-1.1 regress` |
| screen_wayland_activity_rgb | +0 | -0.19 | +0.00 | `-1.3 regress` |

Average score was `-1.1`, with all three rows classified as regressions. The
source diff was reverted. Do not remove `clear()` from CCLM prediction output
without a lower-level profile proving a different buffer-management path is the
actual bottleneck.

Rejected probe: CCLM constant-parameter fill.

The CCLM prediction helper was briefly changed to fill the output block
directly when `derive_vvc_cclm_parameters_from_selection()` returned `a == 0`.
Mathematically this is equivalent to the existing per-sample loop because
`right_shift_i32(0 * luma_sample, shift) + b == b`, so it looked like a
low-risk way to reduce chroma CCLM prediction work.

Correctness was clean:

```text
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  cclm_flat_template_predicts_constant_chroma --features vvc
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc --features vvc
```

A 5-frame six-vector comparison against the recent stats checkpoint was
byte- and PSNR-identical on all rows, confirming the implementation was
decision-neutral for current content:

```text
verification/generated/encode_matrix/vvc-cclm-constant-fill-q19-5f.md
```

The matched 50-frame A/B run rejected the change on speed:

```text
baseline: verification/generated/encode_matrix/vvc-before-cclm-constant-fill-q19-50f.md
probe:    verification/generated/encode_matrix/vvc-cclm-constant-fill-q19-50f.md
scored:   verification/generated/encode_matrix/vvc-cclm-constant-fill-vs-before-q19-50f.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| SceneComposition_1_420 | +0 | +0.000 | -0.13 | `-0.3 regress` |
| SceneComposition_1_422 | +0 | +0.000 | -0.09 | `-0.3 regress` |
| screen_wayland_activity_rgb | +0 | +0.000 | +0.02 | `+0.2 watch` |
| MissionControlClip1_420 | +0 | +0.000 | +0.11 | `+0.7 watch` |
| MissionControlClip1_422 | +0 | +0.000 | -0.01 | `-0.1 regress` |
| MissionControlClip1_444 | +0 | +0.000 | +0.00 | `+0.0 watch` |

Aggregate score was `+0.0` with 0 accepts, 3 watches, and 3 regressions. The
source change was reverted. Do not retry this exact constant-fill branch unless
a profile trace shows the compiler failed to optimize the existing multiply
path on a new target.

Rejected probe: sort RD shortlists once after collection.

The luma and chroma RD shortlist builders were changed to defer sorting until
all candidates had been collected. Duplicate replacement still kept the lower
raw score, and the overflow fallback scanned for the worst current candidate,
so the final shortlisted candidate set and order were intended to remain
equivalent while avoiding repeated small-array sorts.

Focused and broad tests passed:

```text
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  rd_shortlist --features vvc
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc --features vvc
```

The 50-frame six-vector A/B was byte- and PSNR-identical, but slower overall:

```text
verification/generated/encode_matrix/vvc-rd-shortlist-sort-once-q19-50f.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| SceneComposition_1_420 | +0 | +0.000 | -0.08 | `-0.2 regress` |
| SceneComposition_1_422 | +0 | +0.000 | -0.45 | `-1.5 regress` |
| screen_wayland_activity_rgb | +0 | +0.000 | -0.02 | `-0.2 regress` |
| MissionControlClip1_420 | +0 | +0.000 | +0.05 | `+0.3 watch` |
| MissionControlClip1_422 | +0 | +0.000 | -0.03 | `-0.2 regress` |
| MissionControlClip1_444 | +0 | +0.000 | +0.01 | `+0.1 watch` |

Aggregate score was `-0.3` with 0 accepts, 2 watches, and 4 regressions. The
source change was reverted. The repeated stable sorts are not a relevant hot
cost on the current matrix, or the sort-once rewrite made surrounding control
flow less optimizer-friendly.

Rejected probe: restrict lossy `LosslessSpeed` chroma BDPCM to aligned modes.

The lossy VVC path was briefly changed so `fast-search=lossless-speed` only
checked chroma BDPCM when the selected chroma mode was derived, horizontal, or
vertical. The intent was to avoid expensive BDPCM candidate work after unrelated
raw chroma-mode decisions while keeping a unified coding path.

Focused and broad tests passed:

```text
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_lossy_lossless_speed_chroma_bdpcm_requires_aligned_mode --features vvc
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc --features vvc
```

The 50-frame six-vector A/B against the current q19 baseline rejected the
change. It increased bytes on every row, reduced PSNR on every row, and slowed
five of six rows:

```text
verification/generated/encode_matrix/vvc-chroma-bdpcm-mode-aligned-q19-50f.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| SceneComposition_1_420 | +260,915 | -1.56 | +0.09 | `-12.8 regress` |
| SceneComposition_1_422 | +552,073 | -1.35 | -0.34 | `-12.8 regress` |
| screen_wayland_activity_rgb | +123,421 | -1.79 | -0.03 | `-14.7 regress` |
| MissionControlClip1_420 | +3,173,112 | -7.68 | -0.24 | `-64.8 regress` |
| MissionControlClip1_422 | +6,134,655 | -8.25 | -0.25 | `-70.9 regress` |
| MissionControlClip1_444 | +11,679,056 | -9.17 | -0.19 | `-79.8 regress` |

Aggregate score was `-42.6` with 0 accepts, 0 watches, and 6 regressions. The
source change was reverted. Do not prune lossy chroma BDPCM from the selected
raw chroma mode alone; it is carrying real rate-distortion value on the current
screen-content and AOM CTC vectors.

Rejected/inconclusive probe: compact lossy `lossless-speed` directional
refinement.

External encoder guidance favors staged refinement around the best cheap
candidate. The current VVC luma directional mode search already does that, but
the default lossy `lossless-speed` refinement intentionally widens the second
pass to the normal seven-offset family around the current winner. A probe
narrowed that second pass to the compact five-offset family by using the active
fast-search policy directly.

Focused 50-frame Wayland result against a fresh current-code baseline:

```text
verification/generated/encode_matrix/vvc-lossless-speed-compact-refine-wayland-q19-50f.md
```

```text
bytes +0, PSNR +0.000 dB, FPS 1.87 -> 1.93, score +0.5:watch
```

Generated 640x360 50-frame mode-probe result against a fresh current-code
baseline:

```text
verification/generated/encode_matrix/vvc-compact-refine-generated-q19-50f.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +0 | +0.000 | -0.13 | `-0.2 regress` |
| probe_blocks_420 | +0 | +0.000 | +0.22 | `+0.3 watch` |
| probe_checker_444 | +0 | +0.000 | -0.26 | `-0.5 regress` |
| probe_blocks_444 | +0 | +0.000 | +0.15 | `+0.4 watch` |

The source change was reverted. Do not retry this exact refinement narrowing
unless a future profile shows the seven-offset refinement itself as a stable
hotspot; current timing noise and mixed rows do not justify the quality risk of
dropping the ±4 winner refinement.

Tooling note: the encode-matrix tradeoff scorer is now centralized in
`scripts/encode_tradeoff.py` with unit coverage in
`scripts/test_encode_tradeoff.py`. `scripts/benchmark_encode_matrix.py` imports
that helper so release tables and local optimization probes use the same
accept/watch/regress semantics.

### VVC luma transform-skip-first lossy fast search

The next profiling pass checked the current VVC encoder against the advanced
mode-selection guidance already noted above: evaluate the cheapest likely
winner first and prune expensive alternatives only when the local evidence says
they are not paying for themselves. In the current implementation this maps to
the luma residual selector, not to external motion search yet; VVC inter motion
search is still not a general block-level encoder path.

Two probes were run against the local generated 640x360 mode set at
`qp=19 gop=-1 fast-search=lossless-speed`:

- high-tail 8x8 luma DCT AC probing during fast mode selection;
- transform-skip-first luma residual selection for lossy 8-bit 4:4:4, removing
  the old exception that still compared transformed luma residuals before
  selecting transform skip.

The high-tail DCT AC probe was rejected. It evaluated the direct transformed
AC candidate for high-tail-energy 8x8 luma residuals, but the 50-frame matrix
showed no byte or PSNR change and a clear 4:4:4 speed loss:

```text
verification/generated/encode_matrix/vvc-fast-dct-tail-probe-50f-q19.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +0 | +0.000 | +0.01 | `+0.0 watch` |
| probe_blocks_420 | +0 | +0.000 | +0.15 | `+0.2 watch` |
| probe_checker_444 | +0 | +0.000 | -1.50 | `-3.0 regress` |
| probe_blocks_444 | +0 | +0.000 | -0.63 | `-1.7 regress` |

The accepted change is the narrower transform-skip-first luma gate for lossy
`lossless-speed` mode. Instrumentation confirmed that it removes redundant
4:4:4 luma transformed-quant work without changing the bitstream:

```text
verification/generated/profiling/vvc_444_transform_skip_first_stats_5f/
verification/generated/encode_matrix/vvc-444-transform-skip-first-stats-5f-q19.md
```

| Probe | Bytes | Old luma transformed quant | New luma transformed quant | Old luma RD scoring ns | New luma RD scoring ns |
|---|---:|---:|---:|---:|---:|
| probe_blocks_444 | 250950 | 18000 | 0 | 106638253 | 58558684 |
| probe_checker_444 | 234080 | 17795 | 0 | 110690378 | 55926164 |

The normal 50-frame matrix is noisy at this size, but byte and PSNR results
were unchanged in both reruns. The useful signal is the direct counter drop:
this is a work-removal change, not a new coding path. It keeps lossless
behavior unchanged and remains inside the shared luma residual selector.

### VVC lossy luma planar neighbour gate

The next screen-content pass tested two early-prune ideas from the same
mode-decision guidance:

- skip lossy `lossless-speed` CCLM when derived/explicit chroma was already
  near-exact;
- skip lossy `lossless-speed` luma planar prediction unless a missing or
  planar neighbour makes it likely.

The CCLM threshold was rejected. CCLM remains critical for 4:4:4 screen
content even when the first-stage score looks low; the probe lost quality on
checker content and grew the blocks stream:

```text
verification/generated/encode_matrix/vvc-cclm-near-exact-prune-50f-q19.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +0 | +0.000 | -0.41 | `-0.8 regress` |
| probe_blocks_420 | +0 | +0.000 | +0.30 | `+0.5 watch` |
| probe_checker_444 | +0 | -3.011 | -0.06 | `-24.2 regress` |
| probe_blocks_444 | +56449 | +0.000 | -0.65 | `-1.8 regress` |

The luma planar neighbour gate was accepted. It applies to both lossy and
lossless `lossless-speed` through the existing shared
`vvc_luma_lossless_speed_evaluates_planar` policy gate; the residual selector
and reconstruction path are unchanged. The normal 50-frame matrix produced the
same bytes and PSNR in both runs, while FPS remained noisy:

```text
verification/generated/encode_matrix/vvc-luma-planar-neighbor-gate-50f-q19.md
verification/generated/encode_matrix/vvc-luma-planar-neighbor-gate-50f-q19-rerun.md
```

Instrumentation gave the useful signal: selected luma modes and bytes were
unchanged, but planar prediction work was removed from the lossy mode search:

```text
verification/generated/profiling/vvc_luma_planar_neighbor_gate_stats_5f/
```

| Probe | Bytes | Old planar candidates | New planar candidates | Old luma mode-search ns | New luma mode-search ns |
|---|---:|---:|---:|---:|---:|
| probe_blocks_420 | 226078 | 18000 | 2595 | 81403418 | 64293421 |
| probe_gradient_420 | 389995 | 18000 | 2595 | 117126224 | 94923225 |
| probe_checker_444 | 234080 | 18000 | 2595 | 70975551 | 54554130 |
| probe_blocks_444 | 250950 | 18000 | 2595 | 85527082 | 60805057 |

### VVC lossy CCLM 4:4:4-only fast search

The earlier CCLM near-exact threshold showed that CCLM must stay available for
4:4:4 screen content. A narrower follow-up probe kept CCLM enabled for lossy
`lossless-speed` 4:4:4 and skipped it for lower-chroma formats. The intent is
to remove expensive CCLM prediction/model scoring where chroma resolution is
already reduced, while keeping the path that the 4:4:4 probes showed to be
quality-critical.

The 50-frame generated mode-probe matrix accepted the change on both 4:2:0
rows and left 4:4:4 output unchanged:

```text
verification/generated/encode_matrix/vvc-cclm-444-only-50f-q19.md
verification/generated/encode_matrix/vvc-cclm-444-only-50f-q19-rerun.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | -20329 | -0.051 | +2.29 | `+3.6 accept` |
| probe_blocks_420 | +8083 | +0.000 | +2.33 | `+3.2 accept` |
| probe_checker_444 | +0 | +0.000 | +0.20 | `+0.3 watch` |
| probe_blocks_444 | +0 | +0.000 | -0.03 | `-0.1 regress` |

Instrumentation confirmed that the accepted rows remove the intended work:

```text
verification/generated/profiling/vvc_cclm_444_only_stats_5f/
```

| Probe | Bytes | Old CCLM candidates | New CCLM candidates | Old chroma mode-search ns | New chroma mode-search ns |
|---|---:|---:|---:|---:|---:|
| probe_blocks_420 | 226078 -> 226935 | 53994 | 0 | 224971317 | 90927417 |
| probe_gradient_420 | 389995 -> 388115 | 54000 | 0 | 214642902 | 92594252 |
| probe_checker_444 | 234080 -> 234080 | 54018 | 54018 | 207945409 | 166579340 |
| probe_blocks_444 | 250950 -> 250950 | 98307 | 98307 | 336656124 | 290996890 |

The 4:4:4 timing changes in the stats table are noise or secondary cache
effects because candidate counts and bitstreams are unchanged there. Treat the
accepted signal as the 4:2:0 candidate/count and score result.

### VVC lossy explicit chroma DC prune

The follow-up probe pruned the explicit chroma DC candidate only for
`lossless-speed` lossy search. This keeps default search and lossless behavior
unchanged, and it leaves the VVC chroma candidate table intact for syntax. The
fast path still evaluates the derived mode plus planar, vertical, horizontal,
and any spec-valid co-located replacement angular mode; it only avoids spending
prediction and RD scoring time on explicit DC, which was not selected by the
current generated mode-probe vectors.

The 50-frame rerun used:

```text
verification/generated/encode_matrix/vvc-explicit-chroma-dc-prune-50f-q19-rerun.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +33 | +0.000 | +1.61 | `+2.3 accept` |
| probe_blocks_420 | +0 | +0.000 | +2.15 | `+2.4 accept` |
| probe_checker_444 | +0 | +0.000 | +1.27 | `+2.0 watch` |
| probe_blocks_444 | +0 | +0.000 | +1.24 | `+2.7 accept` |

The `probe_checker_444` row is a timing-threshold watch, not a quality or
bitrate concern: bytes and PSNR were unchanged, while the measured FPS ratio was
near the 10% accept cutoff.

Instrumentation confirmed that the intended candidate class was removed:

```text
verification/generated/profiling/vvc_explicit_chroma_dc_prune_stats_5f/
```

| Probe | Explicit candidates delta | Chroma mode-search delta | Explicit prediction delta | Score-time delta |
|---|---:|---:|---:|---:|
| probe_blocks_420 | -17998 (-25.0%) | -14361277 ns (-15.8%) | -7655759 ns (-16.4%) | -5358504 ns (-19.8%) |
| probe_gradient_420 | -18000 (-25.0%) | -12485418 ns (-13.5%) | -7098377 ns (-14.8%) | -4784666 ns (-17.6%) |
| probe_checker_444 | -18006 (-21.8%) | -10360032 ns (-6.2%) | -6609322 ns (-9.0%) | -3812158 ns (-7.9%) |
| probe_blocks_444 | -32769 (-23.9%) | -4799385 ns (-1.6%) | -9262348 ns (-9.9%) | -3570176 ns (-4.3%) |

The required VTM smoke gate passed after the change:

```text
make validate-set CODEC=vvc VALIDATION_SET=smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_SETTINGS='qp=19 gop=-1 fast-search=lossless-speed' \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1
```

### VVC boxed CTU payload

Clippy's performance lint reported that `VvcQuantizedCtuPayload` had a large
enum variant: `Intra(VvcCtuPartitionParams)` was about 53 KiB while
`InterSkip` was empty. The retained change boxes the intra payload:

```rust
VvcQuantizedCtuPayload::Intra(Box<VvcCtuPartitionParams>)
```

This is syntax- and reconstruction-neutral. It removes large enum moves from
the frame CTU vector and makes mixed predictive frames less memory-heavy, while
adding one allocation for each intra CTU payload.

Correctness gates:

```text
make clippy-perf
cargo test -p framefinery-codecs vvc --features vvc
make validate-set CODEC=vvc VALIDATION_SET=smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_SETTINGS='qp=19 gop=-1 fast-search=lossless-speed' \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1
```

The VTM smoke gate passed 3/3 with matching internal/reference
reconstructions. Two generated 640x360 q19 50-frame runs were byte- and
PSNR-identical versus the current baseline and showed positive FPS movement on
every row. The rerun used:

```text
verification/generated/encode_matrix/vvc-boxed-ctu-payload-generated-q19-50f-rerun.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +0 | +0.000 | +0.79 | `+1.1 watch` |
| probe_blocks_420 | +0 | +0.000 | +0.84 | `+0.9 watch` |
| probe_checker_444 | +0 | +0.000 | +0.74 | `+1.1 watch` |
| probe_blocks_444 | +0 | +0.000 | +0.49 | `+1.0 watch` |

Aggregate score was `+1.0 watch`, not a formal aggregate accept. The change
was still retained because it is exact-neutral, clears the repo's perf lint,
and reproduced all-row speed gains across two generated runs. Treat this as a
small memory-movement cleanup, not a rate/quality improvement. A full
six-vector rerun still requires `AOMCTC_ROOT`, which remains intentionally
mandatory for that manifest.

### VVC non-predictive direct intra payload materialization

The follow-up source/reference sweep confirmed the same shape seen in VTM and
the fast-intra papers: robust speed work should stage cheap candidate signals
before expensive RD, and accepted shortcuts should live in candidate selection
or payload construction rather than as independent lossy/lossless encoders. The
local encode loop had one exact-neutral instance of unnecessary work in the
non-predictive path: it materialized an intra CTU payload from a borrowed
decision, then discarded that payload and rebuilt it from the moved quantized
decision when no predictive frame cache was being recorded.

The retained cleanup only builds the borrowed-decision payload in predictive
mode, where the CTU decision is still needed for the next frame. In
non-predictive mode it directly moves the quantized CTU data into the emitted
payload. Bitstream syntax and reconstruction stay unchanged.

The 50-frame generated q19 non-predictive A/B used:

```text
verification/generated/encode_matrix/vvc-nonpredictive-payload-direct-q19-50f.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +0 | +0.000 | +0.21 | `+0.3 watch` |
| probe_blocks_420 | +0 | +0.000 | -0.12 | `-0.1 regress` |
| probe_checker_444 | +0 | +0.000 | +0.42 | `+0.8 watch` |
| probe_blocks_444 | +0 | +0.000 | +0.26 | `+0.6 watch` |

Aggregate score was `+0.4 watch`, with no hard regressions. Keep this as a
small exact-neutral cleanup, not as a proven throughput optimization. It is
useful because it removes a redundant construction from the non-predictive
code path and keeps predictive/non-predictive payload handling in one shared
branch.

Validation:

```text
cargo test -p framefinery-codecs vvc --features vvc
make validate-set CODEC=vvc VALIDATION_SET=smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_SETTINGS='qp=19 gop=0 fast-search=lossless-speed' \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1
make validate-set CODEC=vvc VALIDATION_SET=smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_SETTINGS='qp=19 gop=-1 fast-search=lossless-speed' \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1
```

Both VTM smoke gates passed 3/3 with matching internal/reference
reconstructions.

### Rejected VVC transform-skip RD-zero quantization probe

A follow-up probe tried a greedy RD-zero rule inside transform-skip coefficient
quantization. The intent was to reduce BDPCM/transform-skip residual bits by
zeroing a non-exact lossy coefficient when the predictor or zero level added
only a small local SSE penalty. Exact transform-skip QPs were left unchanged so
lossless paths would remain bit-exact.

The focused unit tests passed, but the 50-frame generated mode-probe scorer
rejected the change:

```text
verification/generated/encode_matrix/vvc-ts-rd-zero-q19-50f.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +857942 | -1.40 | -0.60 | `-13.3 regress` |
| probe_blocks_420 | +0 | +0.000 | -0.44 | `-0.5 regress` |
| probe_checker_444 | +0 | +0.000 | -0.15 | `-0.2 regress` |
| probe_blocks_444 | +0 | +0.000 | +0.07 | `+0.1 watch` |

Aggregate result: `-3.5 regress`, with one hard row regression. The source
change was reverted. Do not retry coefficient-local zeroing in BDPCM without a
block-level RD check: greedy predictor reuse can change later BDPCM predictors
and made the gradient row both larger and lower quality.

### Rejected VVC skip-bit and luma transform-skip probes

Two follow-up probes from the motion/mode audit were tested and rejected.

The first changed lossy predictive CTU skip from a distortion-only gate to a
payload-local RD gate:

```text
skip_sse + lambda * skip_cabac_bits <= intra_sse + lambda * intra_cabac_bits
```

The bit counts were measured through the existing CTU CABAC payload builder,
not a separate syntax estimator. On the 50-frame generated mode-probe matrix it
did not change any bytes or PSNR, and only added timing noise:

```text
verification/generated/encode_matrix/vvc-skip-rd-bits-q19-50f.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +0 | +0.000 | -0.10 | `-0.1 regress` |
| probe_blocks_420 | +0 | +0.000 | -0.71 | `-0.8 regress` |
| probe_checker_444 | +0 | +0.000 | -0.19 | `-0.3 regress` |
| probe_blocks_444 | +0 | +0.000 | +0.33 | `+0.7 watch` |

Aggregate result: `-0.1 regress`.

The second narrowed lossy `lossless-speed` luma transform-skip-first selection
to 8-bit 4:4:4 so that 4:2:0 luma would compare transformed residual coding
again. This tested whether the poor 4:2:0 block score was caused by blindly
forcing transform skip. The result was also rejected:

```text
verification/generated/encode_matrix/vvc-luma-ts-444-only-q19-50f.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +0 | +0.000 | -1.60 | `-2.5 regress` |
| probe_blocks_420 | +6492 | +0.053 | -2.58 | `-2.8 regress` |
| probe_checker_444 | +0 | +0.000 | +0.08 | `+0.1 watch` |
| probe_blocks_444 | +0 | +0.000 | +0.19 | `+0.4 watch` |

The source changes were reverted. The result suggests the screen-content gap is
not fixed by simply spending more transformed-residual search on the current
fixed 8x8 residual path. The next useful work is a unified palette/IBC
candidate inside the normal CTU mode graph, plus a better block-level
rate/distortion model for deciding between BDPCM, transform skip, and
transformed residuals.

### Rejected VVC chroma Planar texture gate

The VVC mode-search audit and fast intra papers suggest using cheap texture
features before expensive prediction/RD. A direct probe applied that idea to
explicit chroma Planar in lossy `lossless-speed`: when source Cb/Cr gradients
were strongly axis-dominant, explicit Planar was skipped before prediction and
residual materialization. The gate was content-adaptive and lived in the shared
candidate generator, so it did not create a separate coding path.

The generated 640x360 50-frame q19 scorer rejected the change against the
current baseline:

```text
verification/generated/encode_matrix/vvc-chroma-planar-texture-gate-50f-q19.md
```

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | -173 | +0.000 | -0.61 | `-0.9 regress` |
| probe_blocks_420 | +0 | +0.000 | +0.19 | `+0.2 watch` |
| probe_checker_444 | +0 | +0.000 | -0.38 | `-0.6 regress` |
| probe_blocks_444 | +0 | +0.000 | -0.24 | `-0.5 regress` |

Aggregate score was `-0.4 regress`, with 0 accepts, 1 watch, and 3
regressions. The source change was reverted. Do not retry this exact
source-gradient explicit Planar gate; the extra per-block texture scan costs
more than the removed Planar predictions on the current mode-probe set. If
explicit chroma Planar is revisited, reuse already-computed residual or
prediction statistics rather than adding a separate source-plane scan.

### VVC transform-skip AC extraction fast path

Checkpoint: `vvc-transform-skip-ac-fastpath`.

The current VVC hotspot profile still spends substantial time in
transform-skip candidate generation. This cleanup keeps the same unified
residual path but removes redundant bounds checks from luma transform-skip AC
extraction: the active luma extent already guarantees in-bounds accesses. For
chroma, normal `>= 4x4` blocks now use a direct AC-position fast path, while
smaller edge/fallback blocks use explicit active-width/height checks instead of
the older raster-index-only guard.

The 50-frame local VVC mode probe at q19 was byte- and PSNR-identical and
showed positive FPS movement on all rows:

| Vector | Bytes delta | PSNR delta | FPS delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +0 | +0.000 | +0.46 | `+0.6 watch` |
| probe_blocks_420 | +0 | +0.000 | +0.56 | `+0.6 watch` |
| probe_checker_444 | +0 | +0.000 | +0.26 | `+0.4 watch` |
| probe_blocks_444 | +0 | +0.000 | +0.21 | `+0.4 watch` |

Aggregate score: `+0.5 watch`, with 4 watch rows and no regressions or hard
regressions. This is an implementation cleanup rather than a mode-decision
change, so byte-identical output and positive all-row FPS were enough to keep
it.

Validation:

```sh
cargo test -p framefinery-api pattern --features filter-pattern
cargo test -p framefinery-codecs vvc --features vvc
cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
make validate-set CODEC=vvc VALIDATION_SET=smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_SETTINGS='qp=19 gop=-1 fast-search=lossless-speed' \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1
make validate-set CODEC=vvc VALIDATION_SET=unusual-geometry-smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_SETTINGS='qp=19 gop=-1 fast-search=lossless-speed' \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1
make validate-set CODEC=vvc VALIDATION_SET=multictu-regression \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_SETTINGS='qp=19 gop=-1 fast-search=lossless-speed' \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1
make validate-set CODEC=vvc VALIDATION_SET=high-depth-smoke \
  VALIDATION_REFERENCE_MODE=required VALIDATION_SOURCE_FILTERS=1 \
  VALIDATION_SETTINGS='fast-search=lossless-speed' \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1
```

`multictu-regression` now passes all 4 rows, including the 257x129 10-bit
`bitdepth_canary` source-filter row. `high-depth-smoke` passes all 6 lossless
canary rows with source and VTM-required reconstruction matches. The
source-filter API and benchmark runner both accept `bitdepth_canary` so these
manifests no longer need materialized raw files.

Benchmark artifact:

```text
verification/generated/encode_matrix/vvc-transform-skip-ac-fastpath-q19-50f.md
```

### Rejected VVC local implementation-cleanup probes

Checkpoint: `vvc-local-cleanup-probes-2026-08-22`.

The current hotspot pass rechecked small exact-neutral implementation cleanups
before changing mode decisions. None was kept:

- Chroma zero-coded RD early-out, including a wider `<= 4 SSE/sample` variant.
  Output stayed byte/PSNR-identical on `local-vvc-mode-probe-50f`, but the FPS
  signal stayed in timing-noise territory: average score `+0.2`, with one
  `probe_blocks_444` row still negative.
- Allocation-free luma/chroma RD-cache placeholders. The local four-row probe
  was positive (`+0.4` average), but this exact source-neutral cleanup had
  already been rejected on the broader six-vector matrix. Do not reapply it
  without allocator-profile evidence that the cache placeholder is now a real
  bottleneck.
- Always running direct chroma AC quantization instead of first checking whether
  residuals have AC energy. Output was identical, but it regressed 3/4 local
  rows because the full-block AC-energy scan is cheaper than running the
  chroma transform on flat residuals.
- Resizing residual vectors and filling by index instead of clearing and
  pushing residual samples. Output was identical, but the local matrix was
  mixed/slower overall.
- Bounded-top-N luma/chroma RD shortlist insertion. The rewrite preserved the
  same shortlist choices and byte/PSNR output, but the 50-frame generated q19
  scorer was slower on all four rows. The existing simple insert/sort path is
  not a useful hotspot under the current candidate counts.

Benchmark artifacts:

```text
verification/generated/encode_matrix/vvc-chroma-zero-rd-probe-q19-50f.md
verification/generated/encode_matrix/vvc-chroma-zero-rd-threshold4-probe-q19-50f.md
verification/generated/encode_matrix/vvc-rd-cache-empty-placeholder-q19-50f.md
verification/generated/encode_matrix/vvc-chroma-ac-direct-quant-q19-50f.md
verification/generated/encode_matrix/vvc-residual-fill-resize-q19-50f.md
verification/generated/encode_matrix/vvc-rd-shortlist-topn-q19-50f.md
```

### Advanced VVC motion/mode-selection audit

Checkpoint: `vvc-advanced-motion-mode-audit-2026-08-22`.

The current FrameFinery VVC predictive path does not yet have a normal
translational motion-vector search comparable to the AV2 motion map. Predictive
VVC currently relies on repeated-frame/CTU skip plus intra CTUs. That makes
external motion-search work useful as design guidance for the future inter path,
but immediate measured work should target intra/SCC mode selection, residual
scoring, entropy construction, and eventually a unified palette/IBC candidate in
the normal CTU graph.

External encoder and paper patterns that should shape future probes:

- VVenC exposes speed/quality presets as tool-selection policy: adaptive search
  range, fast merge, fast sub-pel ME, SCC-specific search, IBC fast methods,
  BDPCM/SCC detection, transform-skip policy, content-based QTBT, reduced
  chroma full-RD modes, and fast intra-tool gates.
- x265 uses the same cheap-first structure: reduced merge/reference candidates,
  early skip when merge has no residual, recursion skip from neighbor/cost or
  edge-density evidence, and fast intra angular scans before deeper RD.
- SVT-AV1’s open-loop ME is the right model for a future FrameFinery inter
  pass: hierarchical/downsampled search, several search centers to avoid local
  minima, full-resolution refinement in a smaller window, and adaptive range
  cuts for near-zero/low-SAD motion.
- rav1e is a useful Rust implementation reference for search-state shape:
  predictor reuse, coarse-to-fine passes, early SAD thresholds, and UMH-style
  refinement before falling back to exhaustive search.
- VVC fast intra papers repeatedly point to rough mode decision plus MPM/left
  and above modes, then optional texture/HOG/variance classifiers. In this code
  base, prefer reusing already-computed residual/search evidence over adding
  fresh per-block texture scans, because prior source-gradient probes have
  consumed more time than they saved.

The existing tradeoff projection in `scripts/encode_tradeoff.py` is the gate for
lossy probes:

```text
score = 10*log2(current_fps / baseline_fps)
      + 4*log2(baseline_bytes / current_bytes)
      + 8*(current_psnr - baseline_psnr)
```

Hard guardrails reject a row even when the scalar score is positive if bytes
grow by more than 20%, PSNR drops by more than 1 dB, or FPS drops below 90% of
baseline. This is important for VVC mode-selection work: speed wins with small
byte/quality penalties can be accepted, but a large byte regression is still a
failed probe.

Ranked next work:

1. Integrate the existing VVC palette/IBC code as candidates inside the normal
   CTU mode graph instead of keeping it as a separate 4:4:4 path. This is the
   most likely large compression win for screen content and keeps with VVenC’s
   SCC-specific IBC/BDPCM/transform-skip policy.
2. Add a real translational inter mode for VVC, starting with integer-pel L0
   search and a small set of predictor centers, then extend to adaptive search
   range and hierarchical/open-loop search. This is required before motion
   vector search papers can materially improve current VVC.
3. Improve residual/mode scoring so BDPCM, transform skip, and transformed
   residuals are compared with a better block-level RD model. Prior greedy
   coefficient-local and format-only gates failed because they ignored block
   context.
4. Keep exact-neutral implementation work only when it shows repeatable
   benchmark movement or removes a confirmed hotspot. Several allocation and
   candidate-list cleanups were byte-identical but slower after measurement.

Accepted probe: mixed single P-slice for lossy 8-bit 4:4:4 VVC.

The previous release path allowed mixed CTU-level InterSkip/Intra P-slices only
for 4:2:0 and kept both 4:2:2 and 4:4:4 on one-slice-per-CTU fallback. That was
too conservative for 8-bit 4:4:4: single-tree luma/chroma geometry is aligned
there, so the same unified CTU quantization and CABAC path used by 4:2:0 can
carry mixed 8-bit 4:4:4 frames without introducing a separate encoder
implementation.

A focused 128x64 50-frame 4:4:4 probe changed the left half of the frame while
keeping the right half repeatable, forcing the mixed InterSkip/Intra decision.
Against the old CTU-slice fallback at `qp=19 gop=-1 fast-search=lossless-speed`:

```text
old CTU-slice fallback:  4,909 bytes, 1308.05 fps, same PSNR
new single P-slice path: 3,269 bytes, 1520.04 fps, same PSNR
projected tradeoff:      about +4.5, accept, no hard regression
```

VTM 24.0 decoded the final 50-frame bitstream and the decoded reconstruction
matched the encoder reconstruction byte-for-byte. Keep 4:2:2 and high-depth
4:4:4 on the CTU-slice fallback until their rectangular/high-depth chroma
residual quality/rate tradeoffs are validated separately.

Rejected probe: mixed single P-slice for lossy high-depth 4:4:4 VVC.

The 4:4:4 mixed-slice gate was initially chroma-sampling based, which also
enabled the path for 10-bit 4:4:4. A focused 128x64 10-frame 10-bit 4:4:4 probe
with constant chroma showed that this was too broad. VTM decoded the fallback
stream and matched the internal reconstruction, but the single P-slice probe
collapsed Cr quality after the first frame.

```text
single P-slice probe:    16,633 bytes, Cr PSNR 16.292 dB after frame 0
old CTU-slice fallback:  1,139 bytes, PSNR inf on every frame
```

The implementation now limits the 4:4:4 mixed single P-slice optimization to
8-bit streams. Do not re-enable high-depth 4:4:4 there until the high-depth
chroma residual decision is fixed inside the shared quantization path.

Rejected probe: mixed single P-slice for lossy 4:2:2 VVC.

The same mixed-slice gate was tested for 4:2:2 after the 4:4:4 win. The probe
was reference-clean: VTM 24.0 decoded the 50-frame stream and matched the
encoder reconstruction. The reconstructed quality and rate were the problem.

The focused 128x64 50-frame 4:2:2 probe used constant chroma and the same
left-half luma change/right-half repeat pattern as the 4:4:4 check. At
`qp=19 gop=-1 fast-search=lossless-speed`, compared with the current CTU-slice
fallback:

```text
old CTU-slice fallback:  4,626 bytes, 1497.43 fps, PSNR mean 68.587 dB
single P-slice probe:    9,061 bytes, 1040.84 fps, PSNR mean 41.046 dB
projected tradeoff:      -229.45, hard regress
```

The single-tree 4:2:2 path spent almost twice the bytes, ran about 30% slower,
and dropped Cr PSNR to about 34.5 dB on frames where the fallback kept Cr near
72.2 dB. Keep 4:2:2 gated off from mixed single P-slices until rectangular
chroma residual decisions are fixed inside the unified quantization path.

Rejected probe: neighbor-first luma directional fast search.

The VVC fast-intra literature and x265/VTM/VVenC practice all prioritize
MPM/neighbor evidence before expensive full mode decisions. A local probe
adapted that idea narrowly for lossy `fast-search=lossless-speed`: if left or
above luma already supplied a directional candidate, it skipped the separate
source-gradient seed scan and used the neighbor candidate first.

The focused unit test passed and output remained byte/PSNR-identical, but the
50-frame generated q19 scorer rejected the change against
`vvc-transform-skip-ac-fastpath-q19-50f`:

```text
verification/generated/encode_matrix/vvc-neighbor-first-luma-directional-q19-50f.md
probe_gradient_420: bytes +0, PSNR +0.000, FPS +0.11, score +0.1:watch
probe_blocks_420:   bytes +0, PSNR +0.000, FPS -0.74, score -0.8:regress
probe_checker_444:  bytes +0, PSNR +0.000, FPS -0.28, score -0.4:regress
probe_blocks_444:   bytes +0, PSNR +0.000, FPS -0.40, score -0.8:regress
```

The source diff was reverted. Do not retry this exact neighbor-first source
seed skip; the current source-gradient scan is not the dominant cost under this
benchmark, or the candidate-list/order change perturbs later work enough to
lose the small scan saving.

Rejected probe: lossy `lossless-speed` luma RD top-one shortlist.

Stats after the transform-skip AC fast path showed that 4:2:0 rows still scored
one cached luma RD candidate per TU, while 8-bit 4:4:4 already used a top-one
shortlist. A probe reduced the non-4:4:4 lossy `lossless-speed` shortlist from
two candidates to one, targeting the extra transform-skip/RD scoring work on
4:2:0 while leaving 4:4:4 decisions unchanged.

The output stayed byte/PSNR-identical, but the generated q19 matrix did not
show a stable FPS win:

```text
verification/generated/encode_matrix/vvc-lossless-speed-luma-rd-top1-q19-50f.md
probe_gradient_420: bytes +0, PSNR +0.000, FPS +0.42, score +0.5:watch
probe_blocks_420:   bytes +0, PSNR +0.000, FPS +0.11, score +0.1:watch
probe_checker_444:  bytes +0, PSNR +0.000, FPS -0.24, score -0.3:regress
probe_blocks_444:   bytes +0, PSNR +0.000, FPS -0.25, score -0.5:regress

verification/generated/encode_matrix/vvc-lossless-speed-luma-rd-top1-q19-50f-rerun.md
probe_gradient_420: bytes +0, PSNR +0.000, FPS +0.35, score +0.5:watch
probe_blocks_420:   bytes +0, PSNR +0.000, FPS -0.16, score -0.2:regress
probe_checker_444:  bytes +0, PSNR +0.000, FPS -0.23, score -0.3:regress
probe_blocks_444:   bytes +0, PSNR +0.000, FPS -0.29, score -0.6:regress
```

The source diff was reverted. Do not retry this exact shortlist reduction
without a broader affected-row baseline or a stronger policy that produces a
real byte/quality/speed improvement rather than timing noise.

### VVC Motion Search And Mode-Selection Audit

The current VVC path still has a large structural gap versus production
encoders: it has exact repeated-frame/CTU reuse and a narrow CTU-local IBC hash
path, but it does not yet have a general translational inter motion-search path
with coded residuals. That means most changed predictive content still falls
back to intra/TU mode decisions, where the current encoder then spends many bits
and cycles searching residual choices that an inter candidate should have
avoided.

External encoder and paper audit:

- VVenC's medium random-access preset is built around staged fast decisions:
  TZ/fast search, adaptive search range, fast merge, fast sub-pel, SCC-specific
  fast search, IBC fast methods, content-based QTBT pruning, and reduced intra
  mode full-RD search.
- x265 exposes the same family of practical controls: cheap motion search
  methods from diamond/hex through UMH/star/full, subpel refinement levels,
  temporal MVP, hierarchical ME, source-picture analysis for independent ME,
  and `limit-modes` that uses sub-CU costs to skip unlikely rectangular/AMP
  modes.
- SVT-AV1's open-loop ME is the best implementation model for our safe Rust
  first pass: it searches source pictures, performs coarse hierarchical search
  on downsampled frames, selects a search center, then does full-pel refinement
  and derives larger block costs from 8x8 SADs. That makes motion analysis
  parallelizable and mostly independent of reconstruction-side CABAC state.
- The 2024 VVC fast-partitioning review from Fraunhofer is a useful warning
  against starting with complex ML partition predictors. Its common-baseline
  result shows that progressive reduction of available split depths already
  forms a strong speed/BD-rate envelope, with only about half the reviewed
  papers beating that simple baseline.
- Recent VVC SCC work keeps pointing back at hash-based IBC and screen-content
  tool gating. For this codebase, the practical direction is to extend the
  existing hash-search machinery before adding decoder-heavy inter tools such
  as affine, GEO, DMVR, or bidirectional prediction.

Concrete implementation order:

1. Add block-level translational inter candidates for lossy P pictures before
   any broader mode-search tuning. Start with full-pel luma-only SAD over 8x8
   cells, candidates from zero MV, left/above/HMVP, and a small diamond/hex
   refinement. Use chroma and exact residual coding only for the final
   shortlisted candidates. Keep this inside the existing CTU quantization path
   so lossless/lossy and 4:2:0/4:2:2/4:4:4 continue to share code and only gate
   syntax/tool eligibility at the deepest point.
2. Build an open-loop source-frame motion map for the next probe rather than
   starting from reconstruction-dependent search. This follows SVT/x265
   source-picture analysis and lets later parallelization happen without
   changing the bitstream decision path.
3. Derive 16x16/32x32/64x64 inter costs from 8x8 SADs before doing any exact
   residual evaluation. Use those aggregate costs to decide whether larger
   inter leaves are worth testing and to avoid broad intra searches on blocks
   that are clearly translational.
4. Add adaptive search-range limits after the first working inter candidate:
   exact zero/repeated blocks should terminate immediately, smooth neighboring
   MVs should shrink the local range, and only high-SAD or high-motion regions
   should use the wider search.
5. Extend VVC IBC from the current A1/B1/B0 exact-hash subset to a bounded
   picture/CTU-window hash table for screen content. This should be scored as a
   separate SCC probe because it can win heavily on repeated UI/text blocks but
   can also explode search cost if the hash index is not bounded.
6. Defer ML partition/mode predictors. If we need a partition speed knob first,
   implement simple progressive split-depth/content-variance pruning and score
   it against the same matrix before adding model dependencies.

All of these probes must be judged through `scripts/encode_tradeoff.py` after
correctness passes. The local projection remains:

```text
score = 10*log2(current_fps / baseline_fps)
      + 4*log2(baseline_bytes / current_bytes)
      + 8*(current_psnr_db - baseline_psnr_db)
```

A high aggregate score is not enough to commit a probe when a row crosses a
hard guardrail. Current hard row regressions remain FPS below 0.90x, bytes
above 1.20x, or PSNR below -1.0 dB versus the baseline; minor byte or PSNR
losses downgrade otherwise-good probes to `watch`.

Checkpoint note: the first output-neutral step is richer VVC motion-field
instrumentation under `vvc-stats`. The counters now distinguish all nonzero,
exact nonzero, near nonzero, uniform nonzero, exact uniform nonzero, and near
uniform nonzero motion candidates at 8x8, 16x16, 32x32, and 64x64 granularity.
The purpose is to identify where a real coded translational-inter path has a
high enough prior to test residual coding, and where block-level intra search
should remain untouched. This matches the practical shape used by VTM/VVenC:
cheap SAD/MV candidate ordering first, exact residual/CABAC checks only after a
small candidate set exists.

Do not restart from exact-CABAC local skip RD as the first mode-selection
change. A previous probe already used final CABAC builders for `InterSkip`
versus intra cost and was rejected by the 50-frame six-vector scorer. Revisit it
only after the open-loop motion counters show a cheap precondition that avoids
running exact bit counting on CTUs where the decision cannot change.

Rejected probe: disable high-depth 4:2:0 mixed single P-slices.

The high-depth 4:4:4 mixed single-P-slice path was previously rejected because
it destroyed chroma quality. A matching high-depth 4:2:0 gate was checked
before changing the policy. On a 10-frame 128x64 yuv420p10le probe, the current
single-slice path produced 925 bytes at 1013.53 fps with the same PSNR as the
fallback. The forced CTU-slice fallback produced 993 bytes at 1019.61 fps and
identical PSNR. The tradeoff score is negative because the small measured speed
gain does not justify a 7.35% byte increase, so the current gate stays enabled
for all 4:2:0 bit depths.

Accepted scaffold: compile-gated VVC predictive luma motion counters.

The normal VVC encode path does not yet have a legal non-skip translational
inter payload, so motion-search work cannot safely change bitstream decisions
until that syntax/reconstruction path exists. As a measured bridge, the
`vvc-stats` build now runs an integer-pel 8x8 luma diamond search over
predictive lossy CTU regions and emits:

- `predictive_luma_motion_8x8_block_count`
- `predictive_luma_motion_exact_8x8_count`
- `predictive_luma_motion_nonzero_exact_8x8_count`
- `predictive_luma_motion_near_8x8_count`
- `predictive_luma_motion_total_sad`

The probe is stats-only and leaves normal product builds and encoded output
unchanged. A 2-frame `pattern=color_blocks` runtime check at 32x32/q19 emitted
16 analysed luma blocks on the second frame, all exact nonzero-MV matches. The
next output-changing VVC inter pass should use these counters to decide where a
real non-skip inter residual candidate is worth testing before broadening the
search pattern or adding partition pruning.

Accepted scaffold: reusable VVC open-loop luma motion map.

The predictive luma motion probe now materializes a reusable 8x8 source-frame
motion map before summarizing it. The map keeps each full-pel diamond-search
candidate and derives non-overlapping 16x16 and 32x32 aggregate summaries from
the 8x8 SAD cells. The aggregate counters distinguish exact/near regions from
uniform-MV exact regions, which matters because a larger single-MV inter block
is only plausible when the underlying 8x8 candidates agree on the same motion
vector.

New `vvc-stats` counters use these prefixes:

- `predictive_luma_motion_16x16_*`
- `predictive_luma_motion_32x32_*`

The normal product build and encoded output remain unchanged. The runtime
32x32 `pattern=color_blocks` q19 probe emitted four 16x16 aggregate candidates
and one 32x32 aggregate candidate on the second frame; the 32x32 region was
exact but not uniform-MV, which is useful evidence that future inter search
should test partitions before assuming a larger translational block. VVC lossy
smoke validation with VTM required passed 3/3 after this scaffold.

### VVC Streaming Annex-B Emission

Accepted cleanup: VVC frame emission now streams Annex-B start codes, NAL
headers, and escaped RBSP payloads directly to the output writer. The public
`write_annex_b()` helper still returns a byte vector for tests and callers, but
the encoder no longer builds a second full-frame Annex-B `Vec` before writing.

This does not change mode selection, reconstruction, NAL syntax, or emitted
bytes. The local one-frame `local-vvc-mode-probe-50f` q19 A/B against
`vvc-current-hotspot-c08ccb1` was byte- and PSNR-identical. The aggregate score
was only `+0.3 watch` because one 4:4:4 row moved negative within timing noise,
but total FPS moved from 6.05 to 6.14 and the allocation removal is
structurally correct. Treat this as an implementation cleanup, not a proven
compression improvement.

Validation:

```text
TMPDIR=verification/generated/agent_scratch/tmp cargo check -p framefinery-codecs --features vvc
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs streaming_annex_b --features vvc
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

### VVC 64x64 Open-Loop Motion Aggregates

Accepted scaffold: the VVC source-frame motion map now reports 64x64 aggregate
summaries in addition to 8x8, 16x16, and 32x32 counters. This matches the
current CTU size and gives the next translational-inter probe a cheap way to
distinguish whole-CTU uniform motion from CTUs that should be partitioned
before trying residual-coded inter candidates.

The normal product build and bitstreams remain unchanged; these counters are
behind `vvc-stats`.

### VVC Shared-Path SCC Opportunity Counters

Accepted scaffold: the CTU-local IBC hash search now tracks mode/context state
relative to the active CTU origin instead of assuming CTU `(0,0)`. This is
needed before IBC can become a normal CTU-mode candidate on multi-CTU pictures:
left-neighbor BVP/context derivation must work identically for a CTU starting
at x/y offsets other than zero.

The `vvc-stats` build also runs a lightweight 4:4:4 SCC analysis from the
normal encode loop and emits:

- `scc_8x8_block_count`
- `scc_palette_solid_8x8_count`
- `scc_palette_no_escape_8x8_count`
- `scc_palette_escape_8x8_count`
- `scc_ibc_exact_8x8_count`
- `scc_ibc_left_residual_8x8_count`

This still does not enable palette/IBC in production residual slices. It
measures where the shared CTU decision path should consider those candidates
next, without reviving the separate palette-only frame path. A tiny 88x8
`pattern=color_blocks` stats run confirmed that the counters flow through the
normal encode loop. The ad hoc output and stats artifacts from that check were
removed.

Validation:

```text
TMPDIR=verification/generated/agent_scratch/tmp cargo check -p framefinery-codecs --features vvc
TMPDIR=verification/generated/agent_scratch/tmp cargo check -p framefinery-codecs --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

Accepted scaffold: ordinary intra residual CUs can now emit the regular false
SCC prefix when a slice config explicitly enables IBC or palette tools:

- `cu_skip_flag=0` and `pred_mode_ibc_flag=0` for IBC-capable luma CUs;
- `pred_mode_plt_flag=0` for palette-capable luma CUs with area greater than
  16 samples.

This mirrors the VTM CABAC writer ordering for normal intra CUs and keeps the
existing residual syntax path unified. Product residual configs still leave
IBC/palette disabled, so this commit is bitstream-neutral for normal encodes.
The next output-changing SCC step can enable the tools in a controlled probe
and add an actual palette/IBC payload candidate instead of falling back to a
separate palette-only frame path.

Validation:

```text
TMPDIR=verification/generated/agent_scratch/tmp cargo check -p framefinery-codecs --features vvc
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs vvc_ctu_body_ --features vvc
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

### VVC Reusable Aggregate Motion Candidates

Accepted scaffold: the open-loop luma motion map now exposes uniform aggregate
motion candidates in addition to counters. A caller can ask for a 16x16,
32x32, 64x64, or rectangular aggregate candidate and receive the current
origin, reference origin, full-pel MV, dimensions, and summed SAD only when all
covered 8x8 cells agree on the same MV. Mixed-MV regions return `None`.

This is still output-neutral; no normal VVC bitstream decision changes. The
purpose is to make the next translational-inter probe consume the already-built
source-frame motion map instead of rerunning a parallel search or starting from
an exhaustive full-frame ME pass. That keeps future non-skip inter work aligned
with the same cheap-first search shape used by VTM/VVenC/x265/AOM: cheap
full-pel luma evidence first, exact residual/CABAC only for a small validated
candidate set.

Validation:

```text
TMPDIR=verification/generated/agent_scratch/tmp cargo fmt
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_luma_motion_map --features vvc
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp PYTHONPATH=scripts \
  python3 -m unittest scripts/test_encode_tradeoff.py
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

### VVC CTU-Wide IBC Opportunity Counters

Accepted scaffold: the VVC IBC search object now has a stats-only CTU-wide
exact-hash probe. The production IBC decision path is still the adjacent
A1/B1/B0 subset, but the instrumentation can now count repeated 8x8 4:4:4
screen-content blocks that are already coded elsewhere in the current CTU.
The counters separate currently reachable adjacent IBC from broader opportunity:

- `scc_ibc_exact_8x8_count`: current adjacent exact-hash IBC path.
- `scc_ibc_ctu_exact_8x8_count`: all CTU-local exact-hash opportunities seen
  by the shared IBC search object.
- `scc_ibc_ctu_extra_exact_8x8_count`: non-adjacent CTU opportunities beyond
  the current A1/B1/B0 subset.

This is output-neutral. It is meant to decide whether full CTU-local IBC search
is worth enabling in the normal CTU decision graph for screen-content lossy and
lossless encodes. It reuses the same BVP/MVD legality checks as the adjacent
IBC path so a future output-changing patch can promote the candidate without
forking IBC analysis from IBC syntax/reconstruction.

Validation:

```text
TMPDIR=verification/generated/agent_scratch/tmp cargo fmt
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_scc_analysis --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

### VVC Normal CTU SCC Leaf Decisions

Accepted scaffold: `VvcCtuPartitionParams` and `VvcQuantizedColor` now carry a
per-luma-leaf SCC decision. The normal CTU CABAC generator can emit an exact
4:4:4 single-tree IBC leaf through the same recursive leaf traversal used by
regular intra residual CUs:

- `cu_skip_flag=0`;
- `pred_mode_ibc_flag=1`;
- `general_merge_flag=0`;
- explicit BVD/MVD syntax;
- `cu_coded_flag=0`, so no transform tree follows.

The existing palette scaffold now shares the same CABAC EP Exp-Golomb helper,
so future IBC integration can reuse syntax primitives instead of growing a
second entropy path. Normal encoder output remains unchanged because the
quantizer still initializes every SCC leaf decision to regular intra. The next
output-changing step is to let the unified quantizer/mode selector promote
validated 4:4:4 exact-hash IBC candidates into this field, then score the
byte/PSNR/FPS tradeoff with the six-vector 50-frame matrix and VTM-required
validation.

Validation:

```text
TMPDIR=verification/generated/agent_scratch/tmp cargo fmt
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc_ctu_body --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs \
  vvc --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
```

### VVC Single-Tree 4:4:4 IBC Enablement Probe

Rejected/blocked probe: enabling IBC for 4:4:4 `lossless-speed` by switching
production residual slices to single-tree 4:4:4 is not safe yet. A direct
quantizer fixture with 8x8 leaves selected the intended exact IBC decision for
the repeated block, but regular single-tree 4:4:4 reconstruction did not remain
lossless-exact; the first mismatch was in Cb at sample 4100, where the decoded
value stayed neutral `128` instead of source `69`.

The apparent cause is that the current regular single-tree chroma residual path
still covers only the 4x4 chroma subset under an 8x8 luma leaf. Do not enable
production 4:4:4 SCC tools until single-tree 4:4:4 chroma residual coverage is
fixed or luma/chroma reconstruction is interleaved through a shared leaf helper.

### Advanced Motion Search And Mode-Selection Refresh

Research checkpoint: `vvc-motion-mode-refresh-2026-08-23`.

Current conclusion: do not spend time tuning a general VVC motion-vector search
policy until the encoder has a normal non-skip translational inter payload. The
existing predictive VVC path has repeated-frame/CTU skip, open-loop motion
counters, and SCC/IBC scaffolding, but changed motion still mostly falls back to
intra residual decisions. External encoder practice should therefore shape the
next implementation order, not be copied wholesale yet.

Useful patterns to adapt:

- Keep the Lagrangian/RD idea as the final arbiter, but stage cheaper estimates
  before full residual/CABAC work.
- Build motion analysis from source-frame/open-loop evidence first: zero MV,
  neighboring/temporal predictors, low-resolution or aggregate 8x8 SAD, then a
  bounded full-pel refinement window.
- Derive 16x16/32x32/64x64 candidate costs from 8x8 SAD before testing exact
  residual coding, so large blocks get tested only when the smaller cells agree.
- Use texture, QP, neighbor mode, and skip/merge residual evidence to prune
  intra/mode/partition candidates. Avoid format-only gates; prior probes showed
  they are too blunt.
- Treat ML partition classifiers as a later option. The low-risk baseline is
  progressive split-depth/content-variance pruning with conservative fallback.

Recommended next implementation order:

1. Add a legal translational VVC inter leaf that reuses the existing unified
   CTU residual/entropy path for final candidates.
2. Feed that leaf from the existing open-loop luma motion map with zero,
   neighbor/HMVP, and small diamond/hex full-pel searches.
3. Add adaptive search windows only after the first inter leaf is validated:
   exact repeated/zero-SSE blocks should terminate immediately, smooth
   neighboring MVs should shrink the search, and high-SAD blocks should keep the
   wider search.
4. Extend exact-hash IBC as a bounded SCC candidate once the single-tree 4:4:4
   residual blocker above is fixed.
5. Evaluate every lossy shortcut through `scripts/encode_tradeoff.py`, not FPS
   alone. The current scalar projection is:

```text
score = 10*log2(current_fps / baseline_fps)
      + 4*log2(baseline_bytes / current_bytes)
      + 8*(current_psnr_db - baseline_psnr_db)
```

Hard row guardrails still override the score: FPS below `0.90x`, bytes above
`1.20x`, or PSNR below `-1.0 dB` is a regression. A clean accept requires score
`>= 2.0` and, when FPS is present, FPS at least `1.10x` baseline. This allows
small byte/PSNR costs only when the speed win is large enough to matter.

## VVC CCLM Subtype Instrumentation

Checkpoint: `vvc-cclm-subtype-candidate-stats`.

The CCLM path remains a visible hotspot on lossy 4:4:4 screen-content probes,
but historical 50-frame stats do not justify pruning any subtype globally. Local
8-bit generated probes often selected no Linear CCLM winners, while AOM/CTC
10-bit MissionControl rows selected all three CCLM subtypes. A blind Linear or
MDLM prune would therefore be content- and format-sensitive enough to risk a
byte/PSNR regression.

The stats path now reports attempted CCLM candidates by subtype:

- `chroma_candidate_cclm_linear`
- `chroma_candidate_mdlm_left`
- `chroma_candidate_mdlm_top`

This is intentionally output-neutral. The next CCLM speed probe should use these
counters with the existing 50-frame six-vector tradeoff scorer before changing
candidate selection.

## VVC motion-search and mode-select checkpoint

Research checkpoint: vvc-me-mode-select-2026-08-23.

External encoder guidance and recent VVC/HEVC papers agree on the same broad
optimization pattern: generate a cheap predictor/shortlist first, then spend
full RDO only on candidates that can plausibly beat the current best. VVenC's
presets expose this explicitly with search-range, fast-diamond ME, QTBT/TT
speedups, early-CU, fast-merge, fast-subpel, fast-intra, and reduced full-RD
mode lists. x265 exposes the same tradeoff family through ME method, subpel
depth, max merge candidates, early skip, limit-modes, reference-count limits,
and analysis reuse. VVC fast-partition and fast-intra papers report the largest
time wins from early split termination and intra/mode shortlist pruning, while
TZSearch papers focus on predictor-centered search starts, dynamic search-range
selection, and early termination around center-biased motion.

For FrameFinery VVC, the current blocker is more basic than the search pattern:
the production path can emit intra CUs and a constrained zero-MV InterSkip case,
but it cannot yet emit a regular non-merge translational L0 inter CU. The luma
motion-map code is therefore still analysis-only. Do not spend optimization time
on hierarchical ME, hex/TZ variants, or adaptive windows until regular explicit
inter prediction can use the unified CTU reconstruction/residual/entropy path.

The first compliant subset to wire is:

- P slice, list0 only, one active reference;
- non-merge regular inter CU with `cu_skip_flag=0`, `pred_mode_flag=0`,
  `general_merge_flag=0`;
- integer-pel translational MVD converted to VVC quarter-pel signal units;
- `mvp_l0_flag=0` against the VTM-compatible AMVP context;
- `cu_coded_flag=0` only for exact/no-residual copies at first.

This pass adds the missing `MVPIdx` CABAC context scaffold required by that
subset. The follow-up explicit-inter syntax scaffold adds a CTU payload field
for non-merge list0 inter decisions, VTM-order AMVP spatial/HMVP/zero candidate
derivation, quarter-pel MVD signalling, and P-slice detection for explicit
inter payloads. It is still not production motion selection: no normal encode
path currently chooses these leaves or copies the corresponding inter prediction
into the reconstructed frame. Do not mix this explicit-inter path with the
current `InterSkip` shortcut for nonzero-motion frames until merge-candidate
motion derivation is modelled too; otherwise the reference decoder may choose a
different skip motion than the encoder's same-position copy assumption. A future
output-changing explicit-inter attempt must first wire reconstruction through
the existing CTU quantization path and then be evaluated with
`scripts/encode_tradeoff.py`, whose row score currently projects local metrics
as:

```text
score = 10*log2(current_fps / baseline_fps)
      +  4*log2(baseline_bytes / current_bytes)
      +  8*(current_psnr_db - baseline_psnr_db)
```

Reject any candidate crossing the hard guards already encoded by that script:
FPS below 0.90x baseline, bytes above 1.20x baseline, or PSNR below baseline by
more than 1.0 dB. Treat candidates with >5% byte growth or >0.30 dB PSNR loss
as watch-list items even if the scalar score is positive.

### VVC Residual-Coded Explicit Inter, 4:2:0-Gated

Checkpoint: `vvc-residual-explicit-inter-420-entry-gate-q19-50f`.

The explicit-inter scaffold is now wired through production quantization for a
strict 4:2:0 subset. The selector still starts from source-exact full-pel luma
motion over 8x8 leaves, but a leaf is only emitted as explicit inter when:

- the chroma format is 4:2:0;
- the chroma source motion is co-located with the luma motion;
- the previous reconstruction predicts the chroma samples exactly for that
  leaf;
- the inter luma residual beats the already-selected intra luma residual under
  the shared quantized residual RD score.

This keeps the coding path unified: intra mode search, MRL/BDPCM, residual
quantization, reconstruction, and CABAC residual emission remain shared. Inter
is only a leaf-level candidate selected after the normal intra candidate exists.
The CABAC body now emits either no-residual explicit inter or explicit inter
followed by the normal transform-unit residual writer. The no-residual branch
also advances the chroma TU index, fixing the older luma/chroma index skew for
single-tree inter leaves.

Rejected probe:

- Ungated 4:4:4 explicit inter looked attractive in bytes, but it hid a hard
  chroma PSNR regression on `probe_checker_444` and `probe_blocks_444`. The
  immediate cause was using previous reconstruction as the chroma predictor
  without a chroma RD decision. The release path is therefore 4:2:0-only until
  explicit inter can compare luma+chroma cost together.

50-frame local VVC mode-probe matrix versus `vvc-pre-rd-chroma-bdpcm-q19-50f`:

| Vector | Previous bytes | Current bytes | Byte delta | Previous FPS | Current FPS | FPS delta | Previous PSNR | Current PSNR | PSNR delta | Tradeoff |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| probe_gradient_420 | 3,873,167 | 3,875,490 | +2,323 | 10.57 | 9.72 | -0.85 | 43.731 | 43.727 | -0.004 | -1.3 regress |
| probe_blocks_420 | 2,331,853 | 1,911,857 | -419,996 | 12.41 | 12.18 | -0.23 | 42.461 | 43.834 | +1.373 | +11.9 watch |
| probe_checker_444 | 2,340,887 | 2,340,887 | 0 | 10.75 | 10.09 | -0.67 | 94.486 | 94.486 | 0.000 | -0.9 regress |
| probe_blocks_444 | 2,499,854 | 2,499,854 | 0 | 7.37 | 6.99 | -0.37 | 49.653 | 49.653 | 0.000 | -0.7 regress |

Aggregate score was `+2.2`, status `watch`, with no hard regressions. The 4:4:4
rows are unchanged in bytes and PSNR; their negative row scores are timing noise
from the local run. A stats-enabled 5-frame 4:2:0 color-block probe confirmed
production selection (`predictive_luma_explicit_inter_count` rose from 19 on
frame 2 to 139 on frame 5).

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=local-vvc-mode-probe-50f \
  ENCODE_MATRIX_RUN=vvc-residual-explicit-inter-420-entry-gate-q19-50f \
  ENCODE_MATRIX_CODECS=vvc ENCODE_MATRIX_MODES=lossy ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_VVC_LOSSY_QP=19 ENCODE_MATRIX_VVC_FAST_SEARCH=lossless-speed \
  ENCODE_MATRIX_VVC_GOP=-1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-pre-rd-chroma-bdpcm-q19-50f.json \
  ENCODE_MATRIX_FAIL_ON_REGRESS=1 \
  ENCODE_MATRIX_CLEANUP_RECON=1 ENCODE_MATRIX_CLEANUP_OUTPUT=1 ENCODE_MATRIX_CLEANUP_VECTORS=1
```

External-encoder notes for the next pass:

- VTM exposes full, diamond, selective, and enhanced-diamond/TZ-style motion
  search plus adaptive search range and Hadamard ME. The current FrameFinery
  selector is much narrower: full-pel, source-exact, 8x8/aggregate motion only.
- x265's search ladder (`dia`, `hex`, `umh`, `star`, `sea`, `full`) and HME
  controls are a useful template for staged speed/quality settings. Its docs
  also note that chroma residual cost is only included at higher subpel
  refinement levels, which matches the 4:4:4 failure here: chroma must be a
  deliberate mode-decision input, not an afterthought.
- VVenC presets and changelog point to practical speed work around fast inter
  mode decision, merge/affine search pruning, SCC detection, memory reuse, and
  inter-frame parallelism. The next local implementation should prioritize
  legal mixed P-slice intra+inter syntax, merge/skip candidate RD, then
  diamond/TZ-style non-exact full-pel search with a luma+chroma score.

### VVC Small Exact-Motion Search Early Exit

Checkpoint: `vvc-motion-small-exact-early-exit-q19-50f`.

The explicit-inter motion map now stops diamond refinement when the current
best full-pel candidate has zero SAD and a small MV tie cost. This mirrors the
production-encoder pattern of accepting decisive cheap motion candidates
instead of continuing a wider local search. The gate is intentionally narrow:
zero-SAD is required, and the Manhattan MV tie cost must be at most 8. Larger
or non-exact candidates still use the existing diamond refinement path.

This is a mode-search heuristic only. It does not create a separate lossy or
lossless path, does not change reconstruction logic, and remains protected by
the explicit-inter leaf RD selector. If the early-exited MV has worse syntax
cost than intra, the shared selector can still reject it.

50-frame local VVC mode-probe matrix versus
`vvc-residual-explicit-inter-420-entry-gate-q19-50f`:

| Vector | Bytes delta | FPS delta | PSNR delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +0 | +0.04 | +0.000 | +0.1 watch |
| probe_blocks_420 | +0 | +1.38 | +0.000 | +1.5 watch |
| probe_checker_444 | +0 | -0.03 | +0.000 | -0.0 regress |
| probe_blocks_444 | +0 | -0.12 | +0.000 | -0.3 regress |

Aggregate score was `+0.3`, status `watch`, with no hard regressions. The
4:4:4 rows are byte/PSNR-identical and do not use the accepted 4:2:0
explicit-inter path; their negative row scores are timing noise from the local
run. The useful signal is `probe_blocks_420`, where exact small-MV blocks avoid
unneeded search work without changing bytes or PSNR.

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=local-vvc-mode-probe-50f \
  ENCODE_MATRIX_RUN=vvc-motion-small-exact-early-exit-q19-50f \
  ENCODE_MATRIX_CODECS=vvc ENCODE_MATRIX_MODES=lossy ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_VVC_LOSSY_QP=19 ENCODE_MATRIX_VVC_FAST_SEARCH=lossless-speed \
  ENCODE_MATRIX_VVC_GOP=-1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-residual-explicit-inter-420-entry-gate-q19-50f.json \
  ENCODE_MATRIX_FAIL_ON_REGRESS=1 \
  ENCODE_MATRIX_CLEANUP_RECON=1 ENCODE_MATRIX_CLEANUP_OUTPUT=1 ENCODE_MATRIX_CLEANUP_VECTORS=1
```

### VVC Motion SAD Limit

Checkpoint: `vvc-motion-sad-limit-q19-50f`.

The luma diamond motion search now scores non-initial candidates with the
current best SAD as an early-termination limit. Once a candidate's partial SAD
exceeds the current best, that candidate cannot win under the existing
SAD-first comparison, so the block scorer returns immediately. Candidates that
tie or beat the current best still compute the full SAD, preserving the
existing MV tie-break behaviour.

This is output-identical by construction and stays inside the shared motion-map
implementation used by explicit inter and stats analysis. It does not add any
new mode path.

50-frame local VVC mode-probe matrix versus
`vvc-motion-small-exact-early-exit-q19-50f`:

| Vector | Bytes delta | FPS delta | PSNR delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +0 | +0.45 | +0.000 | +0.6 watch |
| probe_blocks_420 | +0 | -0.12 | +0.000 | -0.1 regress |
| probe_checker_444 | +0 | -0.06 | +0.000 | -0.1 regress |
| probe_blocks_444 | +0 | +0.19 | +0.000 | +0.4 watch |

Aggregate score was `+0.2`, status `watch`, with no hard regressions. Bytes and
PSNR were unchanged in every row. The row-level FPS signs are local timing
noise, but the change is retained because it removes strictly unnecessary SAD
work without altering selected candidates and clears the aggregate gate.

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=local-vvc-mode-probe-50f \
  ENCODE_MATRIX_RUN=vvc-motion-sad-limit-q19-50f \
  ENCODE_MATRIX_CODECS=vvc ENCODE_MATRIX_MODES=lossy ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_VVC_LOSSY_QP=19 ENCODE_MATRIX_VVC_FAST_SEARCH=lossless-speed \
  ENCODE_MATRIX_VVC_GOP=-1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-motion-small-exact-early-exit-q19-50f.json \
  ENCODE_MATRIX_FAIL_ON_REGRESS=1 \
  ENCODE_MATRIX_CLEANUP_RECON=1 ENCODE_MATRIX_CLEANUP_OUTPUT=1 ENCODE_MATRIX_CLEANUP_VECTORS=1
```

### VVC 4:4:4 Reconstruction-Exact Explicit Inter

Checkpoint: `vvc-explicit-inter-444-recon-exact-q19-50f`.

The 4:2:0-gated explicit-inter selector has been extended to a strict 8-bit
4:4:4 subset. 4:4:4 candidates are only generated when:

- luma source motion is exact;
- chroma source motion is exact;
- previous reconstruction predicts chroma exactly;
- previous reconstruction also predicts luma exactly.

This makes the accepted 4:4:4 path no-residual/reconstruction-exact. The
existing residual-coded explicit inter path remains available for the proven
4:2:0 case, but 4:4:4 does not yet use residual-coded explicit inter because
single-tree P-slice activation can hurt non-inter chroma leaves badly.

Rejected probes before the final gate:

- `vvc-explicit-inter-444-exact-chroma-q19-50f`,
  `vvc-explicit-inter-444-exact-chroma-quality-q19-50f`, and
  `vvc-explicit-inter-444-exact-only-q19-50f` all kept the large
  `probe_checker_444` win but regressed `probe_blocks_444` to 732,794 bytes
  at 39.471 dB, a hard PSNR regression. Stats showed the issue was not chroma
  source motion; it was admitting residual-coded 4:4:4 inter into a frame where
  non-inter chroma leaves still paid the single-tree P-slice cost. Requiring
  previous-reconstruction-exact luma rejects that row while keeping exact
  repeated checker motion.

50-frame local VVC mode-probe matrix versus `vvc-motion-sad-limit-q19-50f`:

| Vector | Bytes delta | FPS delta | PSNR delta | Tradeoff |
|---|---:|---:|---:|---|
| probe_gradient_420 | +0 | +0.11 | +0.000 | +0.1 watch |
| probe_blocks_420 | +0 | -0.25 | +0.000 | -0.3 regress |
| probe_checker_444 | -2,283,008 | +6.12 | +0.000 | +28.2 accept |
| probe_blocks_444 | +0 | -0.05 | +0.000 | -0.1 regress |

Aggregate score was `+7.0`, status `watch`, with no hard regressions. The
4:2:0 and 4:4:4 color-block rows are byte/PSNR-identical to the baseline; their
negative row scores are timing noise. The useful signal is
`probe_checker_444`, where all predictive frames become tiny exact inter
frames without changing PSNR.

Validation:

```sh
TMPDIR=verification/generated/agent_scratch/tmp cargo test -p framefinery-codecs vvc --features "vvc vvc-stats"
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=smoke VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make validate-set CODEC=vvc \
  VALIDATION_SET=regression VALIDATION_REFERENCE_MODE=required VALIDATION_FORCE_LOSSY=1 \
  VALIDATION_SETTINGS="qp=19 gop=-1 fast-search=lossless-speed" \
  VALIDATION_CLEANUP_RECON=1 VALIDATION_CLEANUP_OUTPUT=1 VALIDATION_CLEANUP_VECTORS=1
TMPDIR=verification/generated/agent_scratch/tmp make benchmark-encode-matrix \
  ENCODE_MATRIX_SET=local-vvc-mode-probe-50f \
  ENCODE_MATRIX_RUN=vvc-explicit-inter-444-recon-exact-q19-50f \
  ENCODE_MATRIX_CODECS=vvc ENCODE_MATRIX_MODES=lossy ENCODE_MATRIX_FRAMES=50 \
  ENCODE_MATRIX_VVC_LOSSY_QP=19 ENCODE_MATRIX_VVC_FAST_SEARCH=lossless-speed \
  ENCODE_MATRIX_VVC_GOP=-1 \
  ENCODE_MATRIX_BASELINE=verification/generated/encode_matrix/vvc-motion-sad-limit-q19-50f.json \
  ENCODE_MATRIX_FAIL_ON_REGRESS=1 \
  ENCODE_MATRIX_CLEANUP_RECON=1 ENCODE_MATRIX_CLEANUP_OUTPUT=1 ENCODE_MATRIX_CLEANUP_VECTORS=1
```

`verification/test_vector_sets/regression.csv` now includes
`checker_444_5f`, a small multi-frame 4:4:4 checker vector that exercises this
exact inter path against the VTM decoder.

Follow-ups:

- Add a true luma+chroma explicit-inter RD candidate so residual-coded 4:4:4
  and 4:2:2 can be reconsidered without chroma PSNR regressions or whole-frame
  single-tree side effects.
- Replace the source-exact-only candidate with diamond/TZ search around AMVP,
  zero, and spatial/HMVP predictors; keep source-exact as an early-accept case.
- Add merge/skip candidate selection before explicit MVD when the legal
  single-slice mixed P-tree path is complete.
- Run six-vector 50-frame scoring after the next inter-search expansion; this
  checkpoint was accepted only on the local mode-probe set.

## References

- Cargo profile settings:
  <https://doc.rust-lang.org/cargo/reference/profiles.html>
- rustc codegen options:
  <https://doc.rust-lang.org/stable/rustc/codegen-options/index.html>
- rustc profile-guided optimization:
  <https://doc.rust-lang.org/nightly/rustc/profile-guided-optimization.html>
- x265 CLI encoder speed and motion-search options:
  <https://x265.readthedocs.io/en/master/cli.html>
- x265 mode decision and early-skip options:
  <https://x265.readthedocs.io/en/master/cli.html#mode-decision-analysis>
- x265 encoder API motion-search, HME, and mode-decision controls:
  <https://raw.githubusercontent.com/videolan/x265/master/source/x265.h>
- x264 motion-estimation method notes:
  <https://x264-dsp.readthedocs.io/en/latest/x264_8h_source.html>
- VTM encoder configuration controls for ME, adaptive search range, Hadamard
  ME, and fast encoder decisions:
  <https://raw.githubusercontent.com/ChristianFeldmann/VTM/master/source/App/EncoderApp/EncAppCfg.cpp>
- VVenC medium random-access preset showing staged ME/mode/partition fast
  controls:
  <https://raw.githubusercontent.com/fraunhoferhhi/vvenc/master/cfg/randomaccess_medium.cfg>
- Fraunhofer VVenC implementation paper and preset tradeoff summary:
  <https://publica.fraunhofer.de/entities/publication/1b1598d4-d074-40aa-8af9-fcf1b2fcd393>
- VVC inter-coding complexity-reduction survey:
  <https://trepo.tuni.fi/handle/10024/233618>
- VVC complexity-reduction comparative review:
  <https://www.sciencedirect.com/science/article/pii/S1051200425000430>
- Low-complexity VVC intra mode selection paper:
  <https://doi.org/10.1016/j.icte.2021.08.018>
- Novel fast VVC intra-mode decision paper:
  <https://doi.org/10.1016/j.jvcir.2020.102849>
- JVET VVC reference-software and common-test-condition index:
  <https://jvet.hhi.fraunhofer.de/>
- Adjustable fast decision method for VVC affine motion estimation:
  <https://rc.signalprocessingsociety.org/conferences/icip-2023/spsicip23vid0574>
- VTM random-access motion-search config:
  <https://jvet.hhi.fraunhofer.de/trac/vvc/attachment/ticket/74/encoder_randomaccess_vtm.cfg>
- VTM inter-search implementation:
  <https://raw.githubusercontent.com/ChristianFeldmann/VTM/master/source/Lib/EncoderLib/InterSearch.cpp>
- VVenC medium random-access fast-tool config:
  <https://raw.githubusercontent.com/fraunhoferhhi/vvenc/master/cfg/randomaccess_medium.cfg>
- VVenC project and presets:
  <https://github.com/fraunhoferhhi/vvenc>
  <https://github.com/fraunhoferhhi/vvenc/wiki/Presets>
- HEVC fast inter-prediction using MV/merge/skip information:
  <https://link.springer.com/article/10.1186/s13640-018-0340-4>
- VVC fast CU size and intra mode decision:
  <https://link.springer.com/article/10.1186/s13640-024-00622-7>
- AOM AV1 encoder speed-feature definitions:
  <https://aomedia.googlesource.com/aom/+/29e0f9faea1f24377b9e0f4ec99f06f1d0545745/av1/encoder/speed_features.h>
- SVT-AV1 open-loop/hierarchical motion-estimation design:
  <https://gitlab.com/AOMediaCodec/SVT-AV1/-/raw/master/Docs/Appendix-Open-Loop-Motion-Estimation.md>
- rav1e motion-estimation implementation:
  <https://codebrowser.dev/slint/crates/rav1e/src/me.rs.html>
- rav1e feature overview and speed-tier note:
  <https://github.com/xiph/rav1e>
- Fast Versatile Video Coding intra-coding paper:
  <https://www.mdpi.com/2079-9292/13/11/2150>
- Quantization-adaptive VVC partition early-termination paper:
  <https://www.mdpi.com/2227-7390/14/10/1587>
- HEVC TZSearch complexity-reduction paper:
  <https://www.sciencedirect.com/science/article/abs/pii/S0923596515001654>
- HEVC TZSearch early-termination paper:
  <https://scholars.ln.edu.hk/en/publications/early-termination-for-tzsearch-in-hevc-motion-estimation/>
- VVC fast/low-complexity survey:
  <https://pmc.ncbi.nlm.nih.gov/articles/PMC9692833/>
- Review and evaluation of VVC fast partitioning search methods using a
  common baseline:
  <https://publica.fraunhofer.de/entities/publication/69c2a152-f47a-4631-922e-5267fba35e63>
- Fast VVC partitioning decision strategies:
  <https://publica.fraunhofer.de/entities/publication/9210f1fb-90f8-4759-9bb6-d6fc72a9b731>
- Fast VVC partitioning strategies in VVenC:
  <https://publica.fraunhofer.de/entities/publication/a6ca1879-7d67-4286-af4f-158e06d60ce9>
- VVC screen-content coding tools overview:
  <https://www.microsoft.com/en-us/research/?p=798274>
- FR-IBC VVC screen-content hash-search paper:
  <https://www.mdpi.com/2079-9292/14/2/221>
- HOG-based VVC fast intra and partition decision:
  <https://www.sciencedirect.com/science/article/pii/S1047320323001384>
- VVC QTMT variance/gradient fast partitioning:
  <https://cir.nii.ac.jp/crid/1360016867546154752>
- VVC QTMT partition and intra mode decision:
  <https://www.sciencedirect.com/science/article/pii/S1047320323000822>
- VVC intra texture/ML fast decision:
  <https://pmc.ncbi.nlm.nih.gov/articles/PMC9489355/>
- VVC chroma intra texture decision:
  <https://www.jstage.jst.go.jp/article/transinf/E104.D/5/E104.D_2020EDL8140/_article>
- VVC fast/low-complexity review:
  <https://pmc.ncbi.nlm.nih.gov/articles/PMC9692833/>
- rustc lints:
  <https://doc.rust-lang.org/rustc/lints/index.html>
- Clippy lint groups and performance lints:
  <https://doc.rust-lang.org/clippy/index.html>
  <https://doc.rust-lang.org/clippy/lints.html>
- Rust code generation attributes:
  <https://doc.rust-lang.org/reference/attributes/codegen.html>
- Rust architecture intrinsics:
  <https://doc.rust-lang.org/stable/core/arch/>
- Rust portable SIMD:
  <https://doc.rust-lang.org/std/simd/index.html>
- LLVM optimization remarks:
  <https://llvm.org/docs/Remarks.html>
- LLVM vectorizers:
  <https://llvm.org/docs/Vectorizers.html>
- LLVM BOLT:
  <https://github.com/llvm/llvm-project/blob/main/bolt/README.md>
- Cargo benchmarks:
  <https://doc.rust-lang.org/cargo/commands/cargo-bench.html>
- Criterion:
  <https://docs.rs/criterion/latest/criterion/>
