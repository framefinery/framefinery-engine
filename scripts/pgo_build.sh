#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--help" ]]; then
    cat <<'EOF'
Build a PGO-trained FrameFinery CLI.

Environment:
  PGO_SET                 Validation set used for training. Default: smoke
  PGO_FRAMES              Optional frame-count override. Default: 1
  PGO_CODECS              Space-separated codecs. Default: av2 vvc
  PGO_MODES               Space-separated modes. Default: lossless lossy
  PGO_DIR                 Profile output directory. Default: verification/generated/profiling/pgo
  PGO_RUN                 Matrix run name. Default: pgo-training
  PGO_DIRECT_SOURCE_FILES Use source_file rows directly when true/1/yes. Default: 0
  PGO_GENERATE_VECTORS    Generate vectors before training when true/1/yes. Default: 1
  PGO_PROFILE             Cargo profile for PGO builds: release or optimized. Default: release
  PGO_TARGET_DIR          Cargo target directory. Default: target/pgo
  LLVM_PROFDATA           llvm-profdata path. Auto-detected from rustup, then PATH.
EOF
    exit 0
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

CARGO="${CARGO:-cargo}"
PYTHON="${PYTHON:-python3}"
PRODUCT_FEATURES="${PRODUCT_FEATURES:-all-codecs all-filters}"
PGO_SET="${PGO_SET:-smoke}"
PGO_FRAMES="${PGO_FRAMES:-1}"
PGO_DIR="${PGO_DIR:-verification/generated/profiling/pgo}"
PGO_RUN="${PGO_RUN:-pgo-training}"
PGO_CODECS="${PGO_CODECS:-av2 vvc}"
PGO_MODES="${PGO_MODES:-lossless lossy}"
PGO_DIRECT_SOURCE_FILES="${PGO_DIRECT_SOURCE_FILES:-0}"
PGO_GENERATE_VECTORS="${PGO_GENERATE_VECTORS:-1}"
PGO_PROFILE="${PGO_PROFILE:-release}"
PGO_TARGET_DIR="${PGO_TARGET_DIR:-target/pgo}"
PGO_AV2_LOSSY_QP="${PGO_AV2_LOSSY_QP:-24}"
PGO_VVC_LOSSY_QP="${PGO_VVC_LOSSY_QP:-24}"
HOST_TARGET="${HOST_TARGET:-$(rustc -vV | awk '/^host: / { print $2 }')}"

if [[ -z "$HOST_TARGET" ]]; then
    echo "error: could not detect rustc host target" >&2
    exit 2
fi

rustup_llvm_profdata=""
if command -v rustup >/dev/null 2>&1; then
    TOOLCHAIN="$(rustup show active-toolchain | awk '{ print $1 }')"
    rustup_llvm_profdata="$HOME/.rustup/toolchains/$TOOLCHAIN/lib/rustlib/$HOST_TARGET/bin/llvm-profdata"
fi
if [[ -z "${LLVM_PROFDATA:-}" ]]; then
    if [[ -x "$rustup_llvm_profdata" ]]; then
        LLVM_PROFDATA="$rustup_llvm_profdata"
    elif command -v llvm-profdata >/dev/null 2>&1; then
        LLVM_PROFDATA="$(command -v llvm-profdata)"
    else
        LLVM_PROFDATA="$rustup_llvm_profdata"
    fi
fi
if [[ ! -x "$LLVM_PROFDATA" ]]; then
    echo "error: missing llvm-profdata at $LLVM_PROFDATA" >&2
    echo "hint: rustup component add llvm-tools-preview, install llvm-profdata, or set LLVM_PROFDATA" >&2
    exit 2
fi

case "$PGO_DIRECT_SOURCE_FILES" in
    1|true|yes) direct_source_flag="--direct-source-files" ;;
    *) direct_source_flag="--no-direct-source-files" ;;
esac

codec_flags=()
for codec in $PGO_CODECS; do
    codec_flags+=(--codec "$codec")
done
mode_flags=()
for mode in $PGO_MODES; do
    mode_flags+=(--mode "$mode")
done
case "$PGO_PROFILE" in
    release)
        cargo_profile_flags=(--release)
        artifact_profile="release"
        ;;
    optimized)
        cargo_profile_flags=(--profile optimized)
        artifact_profile="optimized"
        ;;
    *)
        echo "error: unsupported PGO_PROFILE '$PGO_PROFILE'; expected release or optimized" >&2
        exit 2
        ;;
esac
frame_flags=()
if [[ -n "$PGO_FRAMES" && "$PGO_FRAMES" != "0" ]]; then
    frame_flags=(--frames "$PGO_FRAMES")
fi

raw_dir="$PGO_DIR/raw"
matrix_dir="$PGO_DIR/encode_matrix"
merged_profile="$PGO_DIR/merged.profdata"
rm -rf "$raw_dir" "$matrix_dir" "$merged_profile"
mkdir -p "$raw_dir" "$matrix_dir" "$PGO_TARGET_DIR"
raw_dir_abs="$(cd "$raw_dir" && pwd)"
merged_profile_abs="$(cd "$(dirname "$merged_profile")" && pwd)/$(basename "$merged_profile")"

case "$PGO_GENERATE_VECTORS" in
    1|true|yes)
        "$PYTHON" scripts/generate_test_vectors.py "$PGO_SET" \
            --set-dir verification/test_vector_sets \
            --out-dir verification/generated/test_vectors
        ;;
esac

RUSTFLAGS="${RUSTFLAGS:-} -Cprofile-generate=$raw_dir_abs" \
    CARGO_TARGET_DIR="$PGO_TARGET_DIR" \
    "$CARGO" build "${cargo_profile_flags[@]}" --target "$HOST_TARGET" -p framefinery \
    --features "$PRODUCT_FEATURES"

training_ff="$ROOT/$PGO_TARGET_DIR/$HOST_TARGET/$artifact_profile/ff"
"$PYTHON" scripts/benchmark_encode_matrix.py "$PGO_SET" \
    --ff "$training_ff" \
    --set-dir verification/test_vector_sets \
    --vector-dir verification/generated/test_vectors \
    --out-dir "$matrix_dir" \
    --run-name "$PGO_RUN" \
    --av2-lossy-qp "$PGO_AV2_LOSSY_QP" \
    --vvc-lossy-qp "$PGO_VVC_LOSSY_QP" \
    "${codec_flags[@]}" \
    "${mode_flags[@]}" \
    "${frame_flags[@]}" \
    "$direct_source_flag"

mapfile -d '' profraw_files < <(find "$raw_dir_abs" -name '*.profraw' -print0)
if [[ "${#profraw_files[@]}" -eq 0 ]]; then
    echo "error: PGO training produced no .profraw files under $raw_dir_abs" >&2
    exit 2
fi
"$LLVM_PROFDATA" merge -o "$merged_profile_abs" "${profraw_files[@]}"

RUSTFLAGS="${RUSTFLAGS:-} -Cprofile-use=$merged_profile_abs -Cllvm-args=-pgo-warn-missing-function" \
    CARGO_TARGET_DIR="$PGO_TARGET_DIR" \
    "$CARGO" build "${cargo_profile_flags[@]}" --target "$HOST_TARGET" -p framefinery \
    --features "$PRODUCT_FEATURES"

cp "$training_ff" ./ff-pgo
chmod 755 ./ff-pgo
printf 'wrote ./ff-pgo using %s\n' "$merged_profile"
