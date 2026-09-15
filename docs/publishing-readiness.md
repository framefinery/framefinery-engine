# Publishing Readiness Notes

Last updated: 2026-09-14.

These notes track work to finish before the next crates.io publish and before
each later versioned release.

## Current Status

The 0.0.4 gate requires external reference decoding, checks all current CI
corpora with intra/inter/reset GOPs and lossless/QP24, and explicitly runs the
open-session reference tests. Previous dated passes below describe historical
revisions and do not certify this candidate. VVC now signals the standards-defined
level 15.5/high-tier pair for its unconstrained rate/resource policy; ordinary
bounded-level compatibility is not claimed. Rust 1.87–1.97 compatibility
verification is deferred at the owner's request; local release checks use the
installed compiler. No additional local toolchain installation is required.

- Workspace packages and internal dependencies are set to version `0.0.4`.
- Version `0.0.3` is already published. The current candidate remains unpublished.
- Publishable crate names are:
  - `framefinery-api`
  - `framefinery-codecs`
  - `framefinery`
- The shared media/API package was renamed from `framefinery-core` to
  `framefinery-api` before the `0.0.3` publish to avoid colliding with an
  existing project name.
- The installed binary name remains `ff`.
- `make ci` exists and is the shared local/GitHub Actions quality gate. It now
  includes a `wasm32-unknown-unknown` type-check for the local browser target
  practice crate.
- GitHub Actions workflow exists at `.github/workflows/ci.yml` and runs
  `make ci` on Rust `1.87.0` and `stable`, with the `wasm32-unknown-unknown`
  target installed.
- Local `make ci` passed on 2026-08-07 after the workflow and WASM target
  additions.
- The `ci-smoke` manifest exists and runs source-filter-generated black,
  checker, and color-block frames for AV2 and VVC without local media files.
- The v0 encoder API now uses `find_encoder_manifest` for discovery and
  `create_encoder`, `encode_frame`, or `encode_source` for encode operations
  selected by `VideoEncoderConfig.codec`.
- Default product builds enable the executable `pattern`, `identity`, `crop`,
  and `scale` filter catalog. `all-filters` remains a compatibility alias for
  the complete compiled filter set.
- A target-practice browser WASM example exists under
  `examples/wasm-screen-capture/`. It builds through `make wasm-build`, serves
  locally through `make wasm-screen-demo`, and is intentionally unpublished.
- `CHANGELOG.md` exists and must be updated before each publish.
- `make validate-release-aomctc RELEASE_AOMCTC_FRAMES=1` passed locally before
  this note update.
- A small release performance-table smoke run passed locally before this note
  update.
- `framefinery-api` and `framefinery-codecs` use package-specific categories
  and README files; the user-facing `framefinery` package keeps the workspace
  README and command-line category.
- `cargo package --list` should show small package contents and no generated
  media/reference trees in the archives.
- `verification/generated` is ignored. Release validation should leave only
  logs/reports by default; generated raw vectors, reconstructions, and
  bitstreams should be cleaned by the validation scripts after successful
  cases.

## Publish Runbook

Publish order matters because the packages depend on each other by crates.io
version:

```sh
make ci
make package-verify

cargo publish --dry-run -p framefinery-api
cargo publish -p framefinery-api

# wait until framefinery-api appears in the crates.io index
cargo publish --dry-run -p framefinery-codecs
cargo publish -p framefinery-codecs

# wait until framefinery-codecs appears in the crates.io index
cargo publish --dry-run -p framefinery
cargo publish -p framefinery
```

Before a manual publish:

- Push the current branch and confirm the GitHub Actions `CI` workflow passes.
- Re-check crate-name availability immediately before uploading.
- Run package inspection from a clean tree, preferably without `--allow-dirty`.
- Make sure the crates.io account is configured locally with `cargo login`.
- Tag the exact published commit after all three crates are published.

## Remaining Before Next Publish

- Confirm the remote GitHub Actions run passes on Rust `1.87.0` and `stable`.
  Rust-version compatibility checks are deferred at the owner's request;
  the installed compiler and WASM target checks do not establish the declared
  MSRV. Remote CI evidence must name the actual release commit.
- All three archives must carry byte-identical copies of the root `LICENSE`
  and `COMMERCIAL-LICENSE.md`. `make package-verify` checks these files and
  internal dependency versions after Cargo builds the archives. Keep the SPDX
  license declaration; no duplicate `license-file` metadata is needed.
- Keep `CHANGELOG.md` current. Cargo releases are permanent; each public
  version should have a curated note and a git tag.
- Keep package-specific README files current for `framefinery-api` and
  `framefinery-codecs`. The root README remains the README for the user-facing
  `framefinery` package and `ff` CLI.
- Keep public API stability expectations visible. Version `0.0.x` can move
  quickly, but users should know which APIs are intended public surface and
  which internals remain experimental.
- Inspect the exact candidate-version archives after `make package-verify`;
  older archives are not evidence for a new release. Remove disposable package
  build directories after verification to limit disk use.
- Optionally clean old ignored benchmark artifacts under `verification/generated`
  before longer release validation runs to reduce disk pressure. Preserve the
  persistent Wayland source under `verification/generated/test_vectors/screen_capture/`;
  remove only disposable derivatives and successful test outputs.

## Post-0.0.3 TODOs

- Revisit AV2 lossy bitrate-quality tuning for non-4:2:0 formats. The
  reference-valid 4:2:2 `TX_4X8` path and current 4:4:4/RGB-family chroma
  residual behavior spend materially more bytes than the previous
  six-vector baseline while improving PSNR. Correctness/reference
  compatibility is not the concern; the open issue is that the byte increase
  is too large for lossy screen-content use. Investigate chroma QP weighting,
  transform skip/FSC decisions, and RD selection so 4:2:2 stays close to
  4:2:0 bitrate unless the extra bytes buy clearly intentional quality.

## Validation Before Release Claims

- Run the local CI gate:

  ```sh
  make ci
  ```

- Run the local AOM CTC crash/regression pass with more than one frame before
  making broad codec-quality claims:

  ```sh
  make validate-release-aomctc RELEASE_AOMCTC_FRAMES=50
  ```

- Run release validation with required reference decoders before claiming
  reference compatibility:

  ```sh
  make validate-release-aomctc RELEASE_AOMCTC_REFERENCE_MODE=required
  ```

- Generate and save the version-to-version performance table:

  ```sh
  make release-performance-table
  ```

- Keep release validation wording precise. The `release-aomctc` manifest is
  local-machine dependent and may include non-multiple-of-8 visible dimensions
  for both AV2 and VVC when the source geometry is legal for the input format.
  Both codecs pad internally to their current coded-canvas granularity and
  signal the visible crop to the reference decoder.
- Keep successful validation and benchmark runs from retaining large encoded or
  reconstruction artifacts. The release validation/performance targets should
  continue cleaning successful outputs by default.

### Release-Candidate Validation Profile

`make ci` now runs portable cases with required decoding across intra,
predictive and reset GOPs. The commands below are additional focused probes;
their intra controls do not replace predictive validation:

```sh
make validate-geometry-sweep GEOMETRY_SWEEP_REFERENCE_MODE=required
```

Then run the source-filter unusual geometry set in both codec modes. The explicit intra runs below isolate geometry; `make ci` also runs
the same corpus with predictive and reset GOPs:

```sh
python3 scripts/run_validation_set.py --ff ./ff --codec av2 unusual-geometry-smoke \
  --set-dir verification/test_vector_sets \
  --vector-dir verification/generated/test_vectors \
  --encoded-dir verification/generated/encoded \
  --log-dir verification/generated/validation_logs \
  --reference-mode required --source-filters --setting gop=0 \
  --cleanup-recon --cleanup-output --stop-on-fail

python3 scripts/run_validation_set.py --ff ./ff --codec av2 unusual-geometry-smoke \
  --set-dir verification/test_vector_sets \
  --vector-dir verification/generated/test_vectors \
  --encoded-dir verification/generated/encoded \
  --log-dir verification/generated/validation_logs \
  --reference-mode required --source-filters --force-lossy --setting qp=24 \
  --cleanup-recon --cleanup-output --stop-on-fail

python3 scripts/run_validation_set.py --ff ./ff --codec vvc unusual-geometry-smoke \
  --set-dir verification/test_vector_sets \
  --vector-dir verification/generated/test_vectors \
  --encoded-dir verification/generated/encoded \
  --log-dir verification/generated/validation_logs \
  --reference-mode required --source-filters \
  --cleanup-recon --cleanup-output --stop-on-fail

python3 scripts/run_validation_set.py --ff ./ff --codec vvc unusual-geometry-smoke \
  --set-dir verification/test_vector_sets \
  --vector-dir verification/generated/test_vectors \
  --encoded-dir verification/generated/encoded \
  --log-dir verification/generated/validation_logs \
  --reference-mode required --source-filters --force-lossy \
  --setting qp=19 --setting fast-search=lossless-speed \
  --cleanup-recon --cleanup-output --stop-on-fail
```

Run the small regression/multi-CTU manifest in both codec modes:

```sh
python3 scripts/run_validation_set.py --ff ./ff --codec av2 regression \
  --set-dir verification/test_vector_sets \
  --vector-dir verification/generated/test_vectors \
  --encoded-dir verification/generated/encoded \
  --log-dir verification/generated/validation_logs \
  --reference-mode required --setting gop=0 \
  --cleanup-recon --cleanup-output --stop-on-fail

python3 scripts/run_validation_set.py --ff ./ff --codec av2 regression \
  --set-dir verification/test_vector_sets \
  --vector-dir verification/generated/test_vectors \
  --encoded-dir verification/generated/encoded \
  --log-dir verification/generated/validation_logs \
  --reference-mode required --force-lossy --setting qp=24 \
  --cleanup-recon --cleanup-output --stop-on-fail

python3 scripts/run_validation_set.py --ff ./ff --codec vvc regression \
  --set-dir verification/test_vector_sets \
  --vector-dir verification/generated/test_vectors \
  --encoded-dir verification/generated/encoded \
  --log-dir verification/generated/validation_logs \
  --reference-mode required \
  --cleanup-recon --cleanup-output --stop-on-fail

python3 scripts/run_validation_set.py --ff ./ff --codec vvc regression \
  --set-dir verification/test_vector_sets \
  --vector-dir verification/generated/test_vectors \
  --encoded-dir verification/generated/encoded \
  --log-dir verification/generated/validation_logs \
  --reference-mode required --force-lossy \
  --setting qp=19 --setting fast-search=lossless-speed \
  --cleanup-recon --cleanup-output --stop-on-fail
```

Run the AOM CTC A5/B2 release set with required references:

```sh
make validate-release-aomctc \
  AOMCTC_ROOT=/path/to/aomctc \
  RELEASE_AOMCTC_REFERENCE_MODE=required \
  RELEASE_AOMCTC_FRAMES=50
```

For the full pre-release checkpoint, including full-length predictive streams,
geometry sweep, unusual geometry, `regression`, `multictu-regression`, AOM CTC
A5/B2, and the six-vector scoreboard, run:

```sh
make pre-release-validation \
  AOMCTC_ROOT=/path/to/aomctc
```

For a standalone six-vector screen-content scoreboard, run the encode matrix
against the last recorded baseline and inspect byte, FPS, and PSNR deltas:

```sh
python3 scripts/benchmark_encode_matrix.py release-six-vectors-full \
  --ff ./ff \
  --set-dir verification/test_vector_sets \
  --vector-dir verification/generated/test_vectors \
  --out-dir verification/generated/encode_matrix \
  --run-name release-0.0.4-six-vectors-full \
  --av2-lossy-qp 24 \
  --vvc-lossy-qp 19 \
  --vvc-fast-search lossless-speed \
  --cleanup-output \
  --cleanup-vectors
```

The native AOM CTC rows are read directly from `AOMCTC_ROOT`. The derived
4:2:2/4:4:4 rows are materialized with `source_y4m_convert`, reused within the
matrix run, and cleaned by `--cleanup-vectors`; the RGB row reads the
persistent Wayland capture at
`verification/generated/test_vectors/screen_capture/current_screen_wayland_activity_2560x1440_30_500f_gbrp8.rgb`.
Cleanup helpers preserve the `screen_capture/` source directory.

Treat reference-decoder mismatches, lossless byte changes, and PSNR drops as
release blockers unless they are explained and intentionally accepted. Timing
rows are useful but should be interpreted as noisy unless the same regression
repeats across clean runs.

Future release-candidate streams worth adding deliberately:

- a source-filter crop set with odd visible 4:4:4/GBRP dimensions and both
  one-frame and multi-frame cases;
- 4:2:0 and 4:2:2 visible geometries whose coded canvas requires right/bottom
  padding, including one case that exercises AV2 `TX_4X8` chroma residuals;
- 10-bit 4:2:0, 4:2:2, and 4:4:4 source-filter crops for both AV2 and VVC;
- at least one longer predictive screen-content source with non-repeated
  consecutive frames once block-level inter prediction is mature enough for
  it to be a normal release criterion.

## Packaging Checks

Final package inspection should verify the archives are small and intentional:

```sh
cargo package --list -p framefinery-api
cargo package --list -p framefinery-codecs
cargo package --list -p framefinery
```

Current observations:

- `README.md` is included in the `framefinery` package.
- `framefinery-api` and `framefinery-codecs` use package-specific README
  files.
- `LICENSE` and `COMMERCIAL-LICENSE.md` are included in each member crate.
  Their canonical versions remain at the repository root.
- `framefinery-codecs` includes Criterion benchmark source files. This is
  acceptable, but decide whether benchmarks are useful to publish.
- Generated fixtures, encoded streams, reconstructions, reference checkouts,
  profiling traces, external drivers, and local validation manifests are absent
  from the package lists.
- The local WASM screen-capture example is outside the publishable package
  archives. Its copied `.wasm` build artifact is ignored.
- `Cargo.toml.orig` appears in package lists as Cargo's normalized-manifest
  companion and is expected.

## Future Publishing Automation

- Releases still publish manually unless a dedicated release workflow is added.
- Consider configuring crates.io Trusted Publishing for future versions. That
  would let a GitHub Actions release workflow publish with OIDC short-lived
  credentials instead of a long-lived crates.io API token.
- If adding a publish workflow, keep it separate from CI and trigger it only
  from tags or manual dispatch. Do not publish from pull request workflows.

## Documentation TODOs

- Keep the CLI guide aligned with `ff --help`, especially when settings move
  between top-level flags and `--set key=value`.
- Add docs.rs examples for library users once the public pipeline API settles.
- Keep codec patent wording visible near install and release documentation; the
  source license and commercial-license notice do not grant codec patent rights.

## References Checked

- Cargo publishing guide: <https://doc.rust-lang.org/cargo/reference/publishing.html>
- Cargo manifest fields: <https://doc.rust-lang.org/cargo/reference/manifest.html>
- Cargo workspace package metadata:
  <https://doc.rust-lang.org/cargo/reference/workspaces.html>
- docs.rs package metadata: <https://docs.rs/about/metadata>
- docs.rs target-default change:
  <https://blog.rust-lang.org/2026/04/04/docsrs-only-default-targets/>
- crates.io trusted publishing update:
  <https://blog.rust-lang.org/2025/07/11/crates-io-development-update-2025-07/>
