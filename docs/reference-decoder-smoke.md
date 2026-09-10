# Reproducible reference-decoder smoke checks

Use the existing validation runner to compare lossless source bytes, the
encoder's internal reconstruction, and an external reference reconstruction.
One frame per selected row checks basic interoperability with the named decoder
revisions; it does not establish full-stream or release certification.

The reference manifests declare upstream repositories and revision overrides,
but do not pin a default revision. These revisions match the versions named by
the current FFE implementation and validation history:

| Codec | Upstream tag | Commit | Project evidence |
|---|---|---|---|
| AV2 | [AVM v1.0.0](https://github.com/AOMediaCodec/avm/tree/v1.0.0) | `966a7d7cd6fcf60360caf5dc413b2aeeb65e144d` | `av2/headers/sequence.rs` and `av2/tile/txb.rs` explicitly name AV2 v1.0.0. |
| VVC | [VTM-24.0](https://vcgit.hhi.fraunhofer.de/jvet/VVCSoftware_VTM/-/tags/VTM-24.0) | `ca331de230f27e0489df9d2fa6c006e2dcd2ed89` | `vvc/residual/syntax_helpers.rs` names VTM 24.0; `compiler-optimizations.md` records matching reconstructions with that version. |

Codec source paths above are relative to `crates/framefinery-codecs/src/`.
The AVM tag is `v1.0.0`, not `research-v1.0.0`. Changing either revision is a
separate compatibility decision; do not silently follow an upstream branch.

Run the commands below from the repository root in a POSIX shell. Prerequisites
are Git, CMake, a C/C++ toolchain, Make or another CMake generator, Perl, Python 3,
and an existing release `./ff` binary. AVM x86 builds also need YASM or NASM.
Use the upstream build prerequisites for other target platforms.

For a fresh reference directory, clone anonymously and verify the tag commits:

```sh
set -eu
export GIT_TERMINAL_PROMPT=0
export FRAMEFINERY_AV2_REF=v1.0.0
export FRAMEFINERY_VTM_REF=VTM-24.0

git -c credential.helper= clone --depth 1 --single-branch \
  --branch "$FRAMEFINERY_AV2_REF" https://github.com/AOMediaCodec/avm.git \
  verification/references/av2/avm
test "$(git -C verification/references/av2/avm rev-parse HEAD)" = \
  966a7d7cd6fcf60360caf5dc413b2aeeb65e144d

git -c credential.helper= clone --depth 1 --single-branch \
  --branch "$FRAMEFINERY_VTM_REF" \
  https://vcgit.hhi.fraunhofer.de/jvet/VVCSoftware_VTM.git \
  verification/references/vvc/vtm
test "$(git -C verification/references/vvc/vtm rev-parse HEAD)" = \
  ca331de230f27e0489df9d2fa6c006e2dcd2ed89
```

If these directories already exist, inspect their revisions and local changes
first. Preserve existing work; do not reset or overwrite it. The revision
environment overrides affect cloning, not an existing checkout.

Build only the decoder targets, serially:

```sh
cmake -S verification/references/av2/avm \
  -B verification/references/av2/avm/build -DCMAKE_BUILD_TYPE=Release \
  -DCONFIG_AV2_ENCODER=0 -DENABLE_TESTS=OFF -DENABLE_DOCS=OFF \
  -DENABLE_TOOLS=OFF -DENABLE_EXAMPLES=ON
cmake --build verification/references/av2/avm/build --target avmdec --parallel 1

cmake -S verification/references/vvc/vtm \
  -B verification/references/vvc/vtm/build -DCMAKE_BUILD_TYPE=Release
cmake --build verification/references/vvc/vtm/build --target DecoderApp --parallel 1
```

These commands leave architecture selection to the upstream build systems.
For older CPUs or cross-compilation, supply the upstream CPU/toolchain options
appropriate to the target and record those exact options with the local results.
Do not reuse another machine's ISA flags without checking support. Keep codec
syntax defaults unchanged. The optional AVM test/doc/tool targets are omitted
because this procedure builds a decoder; FFE validation checks remain enabled.

Resolve and run the resulting executables on the target host before validation:

```sh
# Clear old decoder/build-directory selections in this shell only. Explicit
# roots below take precedence over global reference-directory overrides.
unset FRAMEFINERY_DECODER FRAMEFINERY_AV2_DECODER FRAMEFINERY_AVM_DECODER
unset FRAMEFINERY_VTM_DECODER FRAMEFINERY_AV2_BUILD_DIR
unset FRAMEFINERY_AVM_BUILD_DIR FRAMEFINERY_VTM_BUILD_DIR
export FRAMEFINERY_AV2_ROOT="$PWD/verification/references/av2/avm"
export FRAMEFINERY_VTM_ROOT="$PWD/verification/references/vvc/vtm"
FRAMEFINERY_AV2_DECODER=$(python3 scripts/reference_tools.py decoder --codec av2 --no-build)
FRAMEFINERY_VTM_DECODER=$(python3 scripts/reference_tools.py decoder --codec vvc --no-build)
case "$FRAMEFINERY_AV2_DECODER" in
  "$FRAMEFINERY_AV2_ROOT"/build/*) ;;
  *) echo 'AVM decoder is outside the pinned build directory' >&2; exit 1 ;;
esac
case "$FRAMEFINERY_VTM_DECODER" in
  "$FRAMEFINERY_VTM_ROOT"/*) ;;
  *) echo 'VTM decoder is outside the pinned checkout' >&2; exit 1 ;;
esac
export FRAMEFINERY_AV2_DECODER FRAMEFINERY_VTM_DECODER
printf 'AVM decoder: %s\nVTM decoder: %s\n' \
  "$FRAMEFINERY_AV2_DECODER" "$FRAMEFINERY_VTM_DECODER"
"$FRAMEFINERY_AV2_DECODER" --help
vtm_help_status=0
"$FRAMEFINERY_VTM_DECODER" --help || vtm_help_status=$?
test "$vtm_help_status" -eq 1
```

VTM 24.0 deliberately returns status 1 after printing help; this check applies
only to that help invocation. Decode commands must still return success.
Confirm that the help output identifies AV2 Decoder 1.0.0 and VTM Decoder 24.0.
Record the resolved paths, versions, CPU dispatch, build options, and exit
statuses in the local report. Successful startup alone is
insufficient; the required decoding checks below must also complete.

The default `smoke` manifest generates its inputs and requires no external media.
An optional user-selected `VALIDATION_SET` must have all of its source fixtures
available first. For external manifests, supply their documented variables
(such as `AOMCTC_ROOT`) and verify source identity before use. Chroma-conversion
rows still require the original Y4M sources. Do not commit local media or create
machine-specific manifests as part of this setup.

```sh
VALIDATION_SET=${VALIDATION_SET:-smoke}
for codec in av2 vvc; do
  python3 -u scripts/run_validation_set.py "$VALIDATION_SET" \
    --ff ./ff --codec "$codec" --frames 1 --force-lossless \
    --direct-source-files --reference-mode required \
    --vector-dir verification/generated/reference_smoke/vectors \
    --encoded-dir "verification/generated/reference_smoke/$codec/encoded" \
    --recon-dir "verification/generated/reference_smoke/$codec/recon" \
    --log-dir "verification/generated/reference_smoke/$codec/logs" \
    --cleanup-recon --cleanup-vectors
done
```

For these source-preserving lossless cases, every row must report source/internal
equality and reference/internal equality. Missing decoders, decode errors, and
checksum mismatches are failures in `required` mode. Report failures and retain
their artifacts; do not relax checks to obtain a pass. Successful reconstruction
and derivative files are cleaned, while original sources, logs, and encoded
streams remain. Use separate output directories when preserving a previous run.

Keep machine-specific commands, timings/RSS, hashes, per-row outcomes, and any
skips or failures in an untracked local report. Reference checkouts, build
artifacts, generated files, private fixture information, and that report belong
outside the commit. The saved result must state the tested revisions and the
one-frame scope.
