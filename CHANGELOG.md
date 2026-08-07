# Changelog

All notable release-facing changes should be recorded here before publishing a
crate version. FrameFinery is still in the `0.0.x` line, so API names and
behavior may change quickly while the public contract settles.

## 0.0.3 - Unreleased

- Shape the v0 Rust encoder API around codec-neutral configs and registry
  helpers.
- Add `find_encoder_manifest` for discovery and keep manifest encoding hooks
  internal to codec registration.
- Add `encode_frame` for one-frame callers and `encode_source` for pull-based
  raw stream adapters.
- Document the v0 API contract and API stability expectations ahead of the next
  crates.io deployment.
- Connect the CLI help/startup summaries to the same codec, filter, and setting
  manifests used by the public API, including default effective settings and
  `--no-progress` for quiet backend-driven CLI runs.
- Implement `crop` and nearest-neighbor `scale` as core transform filters and
  include them in the default product filter catalog.
- Harden generated test-vector manifest parsing so unknown, duplicate, empty,
  or row-value-looking CSV headers fail before validation silently drops a
  field.
- Add a compact raw input and codec support matrix covering tested format
  families, bit depths, lossless support, CLI fallback behavior, and current
  geometry limits.
- Keep generated package/profiling artifacts out of release preparation; final
  dry-runs should start from a clean generated-artifact state.

## 0.0.1 - 2026-08-05

- Publish the initial FrameFinery crate set: `framefinery-core`,
  `framefinery-codecs`, and `framefinery`.
- Ship the `ff` command-line binary with AV2, VVC, and the current core filter
  catalog enabled by default.
- Add CI, release validation helpers, AOM CTC release manifests, and local API
  documentation generation.
