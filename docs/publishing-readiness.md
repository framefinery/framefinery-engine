# Publishing Readiness Notes

Last updated: 2026-08-06.

These notes track work to finish before the next crates.io publish and before
each later versioned release.

## Current Status

- Workspace packages are set to version `0.0.3`.
- Publishable crate names are:
  - `framefinery-core`
  - `framefinery-codecs`
  - `framefinery`
- The installed binary name remains `ff`.
- `make ci` exists and is the shared local/GitHub Actions quality gate.
- GitHub Actions workflow exists at `.github/workflows/ci.yml` and runs
  `make ci` on Rust `1.87.0` and `stable`.
- Local `make ci` passed after the workflow was added.
- The `ci-smoke` manifest exists and runs source-filter-generated black,
  checker, and color-block frames for AV2 and VVC without local media files.
- The v0 encoder API now uses `find_encoder_manifest` for discovery and
  `create_encoder`, `encode_frame`, or `encode_source` for encode operations
  selected by `VideoEncoderConfig.codec`.
- Default product builds enable the executable `pattern`, `identity`, `crop`,
  and `scale` filter catalog. `all-filters` remains a compatibility alias for
  the complete compiled filter set.
- `CHANGELOG.md` exists and must be updated before each publish.
- `make validate-release-aomctc RELEASE_AOMCTC_FRAMES=1` passed locally before
  this note update.
- A small release performance-table smoke run passed locally before this note
  update.
- `framefinery-core` and `framefinery-codecs` use package-specific categories
  and README files; the user-facing `framefinery` package keeps the workspace
  README and command-line category.
- `cargo package --list` should show small package contents and no generated
  media/reference trees in the archives.
- `verification/generated` is ignored but currently large locally, about 17G
  after cleaning profiling/comparison outputs and preserving the Wayland
  `gbrp8` capture files.

## Publish Runbook

Publish order matters because the packages depend on each other by crates.io
version:

```sh
make ci

cargo publish --dry-run -p framefinery-core
cargo publish -p framefinery-core

# wait until framefinery-core appears in the crates.io index
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
  Until that happens for the release commit, the declared MSRV is still only
  locally assumed, not proven by CI for that revision.
- Decide whether package archives should carry local copies of `LICENSE` and
  `COMMERCIAL-LICENSE.md`. The manifests use standard SPDX
  `license = "AGPL-3.0-or-later"`; adding `license-file` as well makes Cargo
  warn because AGPL is a standard SPDX license.
- Keep `CHANGELOG.md` current. Cargo releases are permanent; each public
  version should have a curated note and a git tag.
- Keep package-specific README files current for `framefinery-core` and
  `framefinery-codecs`. The root README remains the README for the user-facing
  `framefinery` package and `ff` CLI.
- Keep public API stability expectations visible. Version `0.0.x` can move
  quickly, but users should know which APIs are intended public surface and
  which internals remain experimental.
- Clean stale local package artifacts before final dry runs. `target/package`
  was cleaned during 0.0.3 preparation and should be empty before the final
  package inspection starts.
- Optionally clean old ignored benchmark artifacts under `verification/generated`
  before longer release validation runs to reduce disk pressure. Keep the local
  Wayland `gbrp8` capture files under `verification/generated/test_vectors/`
  unless replacing them intentionally.

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
  local-machine dependent and currently marks non-multiple-of-8 A5 and mobile
  rows as VVC-only because AV2 rejects those geometries.
- Keep successful validation and benchmark runs from retaining large encoded or
  reconstruction artifacts. The release validation/performance targets should
  continue cleaning successful outputs by default.

## Packaging Checks

Final package inspection should verify the archives are small and intentional:

```sh
cargo package --list -p framefinery-core
cargo package --list -p framefinery-codecs
cargo package --list -p framefinery
```

Current observations:

- `README.md` is included in the `framefinery` package.
- `framefinery-core` and `framefinery-codecs` use package-specific README
  files.
- `LICENSE` and `COMMERCIAL-LICENSE.md` are root repository files; member crate
  package archives do not include them with the current SPDX-only manifest
  setup.
- `framefinery-codecs` includes Criterion benchmark source files. This is
  acceptable, but decide whether benchmarks are useful to publish.
- Generated fixtures, encoded streams, reconstructions, reference checkouts,
  profiling traces, external drivers, and local validation manifests are absent
  from the package lists.
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
