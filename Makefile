CARGO ?= cargo
PYTHON ?= python3
PRODUCT_FEATURES ?= all-codecs all-filters
CARGO_FEATURES ?= all
CARGO_DEFAULT_FEATURES ?= 1
AV2_SB_BITS ?= 0
AV2_LOSSY_STATS ?= 0
AV2_STATS ?= 0
VVC_STATS ?= 0
AV2_SB_BITS_FEATURE := $(if $(filter 1 true yes,$(AV2_SB_BITS)),framefinery-codecs/av2-sb-bit-profile,)
AV2_LOSSY_STATS_FEATURE := $(if $(filter 1 true yes,$(AV2_LOSSY_STATS)),framefinery-codecs/av2-lossy-stats,)
AV2_STATS_FEATURE := $(if $(filter 1 true yes,$(AV2_STATS)),framefinery-codecs/av2-stats,)
VVC_STATS_FEATURE := $(if $(filter 1 true yes,$(VVC_STATS)),framefinery-codecs/vvc-stats,)
AV2_ANALYSIS_FEATURES := $(strip $(AV2_SB_BITS_FEATURE) $(AV2_LOSSY_STATS_FEATURE) $(AV2_STATS_FEATURE))
VVC_ANALYSIS_FEATURES := $(strip $(VVC_STATS_FEATURE))
CARGO_BASE_FEATURES := $(if $(filter all,$(strip $(CARGO_FEATURES))),$(PRODUCT_FEATURES),$(strip $(CARGO_FEATURES)))
CARGO_NO_DEFAULT_FEATURES_FLAG := $(if $(filter 0 false no off,$(CARGO_DEFAULT_FEATURES)),--no-default-features,)
CARGO_FLAGS := $(CARGO_NO_DEFAULT_FEATURES_FLAG) $(if $(strip $(CARGO_BASE_FEATURES)),--features "$(CARGO_BASE_FEATURES)",) $(if $(strip $(AV2_ANALYSIS_FEATURES)),--features "$(AV2_ANALYSIS_FEATURES)",) $(if $(strip $(VVC_ANALYSIS_FEATURES)),--features "$(VVC_ANALYSIS_FEATURES)",)
PROFILE ?=
GPROF_RUSTFLAGS ?= -C debuginfo=2 -C force-frame-pointers=yes -C symbol-mangling-version=v0 -C codegen-units=1 -C lto=no -C link-arg=-pg
GPROF_TARGET_DIR ?= target/gprof
GPROF_SAMPLE_RUNS ?= 200
GPROF_PROFILE_CODEC ?= av2
GPROF_PROFILE_NAME ?= scenecomposition_1_420_i_lossless_1f
GPROF_PROFILE_INPUT ?= /media/gabriel/storage/YUV/aomctc/b2_scc/SceneComposition_1.y4m
GPROF_PROFILE_FRAMES ?= 1
GPROF_PROFILE_SETTINGS ?= lossless
GPROF_PROFILE_OUT_DIR ?= verification/generated/profiling
GPROF_PROFILE_SAMPLE_DIR ?= $(GPROF_PROFILE_OUT_DIR)/$(GPROF_PROFILE_NAME)_samples
GPROF_PROFILE_OUTPUT ?= $(GPROF_PROFILE_OUT_DIR)/$(GPROF_PROFILE_NAME).obu
GPROF_PROFILE_RECON ?= $(GPROF_PROFILE_OUT_DIR)/$(GPROF_PROFILE_NAME)_recon.yuv
GPROF_PROFILE_REPORT ?= $(GPROF_PROFILE_OUT_DIR)/$(GPROF_PROFILE_NAME)_$(GPROF_SAMPLE_RUNS)x.gprof.txt
GPROF_PROFILE_RUN_LOG ?= $(GPROF_PROFILE_OUT_DIR)/$(GPROF_PROFILE_NAME).last-run.log
PGO_SET ?= smoke
PGO_FRAMES ?= 1
PGO_DIR ?= verification/generated/profiling/pgo
PGO_RUN ?= pgo-training
PGO_CODECS ?= av2 vvc
PGO_MODES ?= lossless lossy
PGO_DIRECT_SOURCE_FILES ?= 0
PGO_GENERATE_VECTORS ?= 1
PGO_PROFILE ?= release
PGO_TARGET_DIR ?= target/pgo
PGO_AV2_LOSSY_QP ?= 24
PGO_VVC_LOSSY_QP ?= 19
LLVM_REMARK_TARGET_DIR ?= target/llvm-remarks
LLVM_REMARK_CRATE ?= framefinery-codecs
LLVM_REMARK_FEATURES ?= av2 vvc
LLVM_REMARK_PASSES ?= loop-vectorize slp-vectorizer
LLVM_REMARK_FLAGS := $(foreach pass,$(LLVM_REMARK_PASSES),-Cremark=$(pass))
BUILD_TARGET_DIR := target
BUILD_BINARY := ./ff
BUILD_ENV :=
BUILD_CARGO_PROFILE_FLAG := --release
BUILD_ARTIFACT_PROFILE := release
CODE_BROWSER_OUT ?= verification/generated/code_browser/framefinery-engine.html
CODE_BROWSER_TITLE ?= FrameFinery Engine Code Browser
CODE_BROWSER_PROFILE_JSON ?=
ifeq ($(strip $(PROFILE)),gprof)
BUILD_TARGET_DIR := $(GPROF_TARGET_DIR)
BUILD_BINARY := ./ff-gprof
BUILD_ENV := RUSTFLAGS="$(GPROF_RUSTFLAGS)" CARGO_TARGET_DIR="$(GPROF_TARGET_DIR)"
else ifeq ($(strip $(PROFILE)),optimized)
BUILD_BINARY := ./ff-optimized
BUILD_CARGO_PROFILE_FLAG := --profile optimized
BUILD_ARTIFACT_PROFILE := optimized
else ifneq ($(strip $(PROFILE)),)
$(error unsupported PROFILE '$(PROFILE)'; expected PROFILE=gprof or PROFILE=optimized)
endif
ARGS ?=
CODEC ?= av2
TEST_VECTOR_SET ?= smoke
VALIDATION_SET ?= $(TEST_VECTOR_SET)
VALIDATION_STOP_ON_FAIL ?= 1
VALIDATION_LIMIT ?=
VALIDATION_SET_DIR ?= verification/test_vector_sets
VALIDATION_OUT_DIR ?= verification/generated/test_vectors
VALIDATION_ENCODED_DIR ?= verification/generated/encoded
VALIDATION_LOG_DIR ?= verification/generated/validation_logs
VALIDATION_SOURCE_FILTERS ?= 0
VALIDATION_DIRECT_SOURCE_FILES ?= 0
VALIDATION_REFERENCE_MODE ?= auto
VALIDATION_SETTINGS ?=
VALIDATION_FRAMES ?=
VALIDATION_FORCE_LOSSY ?= 0
VALIDATION_CLEANUP_RECON ?= 0
VALIDATION_CLEANUP_OUTPUT ?= 0
CI_ENCODE_SET ?= ci-smoke
RELEASE_AOMCTC_SET ?= release-aomctc
RELEASE_AOMCTC_FRAMES ?= 1
RELEASE_AOMCTC_REFERENCE_MODE ?= auto
RELEASE_AOMCTC_AV2_LOSSY_QP ?= 24
RELEASE_AOMCTC_VVC_LOSSY_QP ?= 19
RELEASE_AOMCTC_VVC_SETTINGS ?= predictive fast-search=lossless-speed
RELEASE_PERFORMANCE_SET ?= release-aomctc
RELEASE_PERFORMANCE_OUT_DIR ?= verification/generated/release_performance
RELEASE_PERFORMANCE_RUN ?=
RELEASE_PERFORMANCE_FRAMES ?= 50
RELEASE_PERFORMANCE_CODECS ?=
RELEASE_PERFORMANCE_MODES ?=
RELEASE_PERFORMANCE_LIMIT ?=
RELEASE_PERFORMANCE_KEEP_BITSTREAMS ?= 0
RELEASE_PERFORMANCE_FULL_STREAM ?= 0
COMPRESSION_SET ?= $(VALIDATION_SET)
COMPRESSION_OUT_DIR ?= verification/generated/compression_compare
COMPRESSION_LOG_DIR ?= verification/generated/compression_compare_logs
COMPRESSION_LIMIT ?=
COMPRESSION_REFERENCE_BACKEND ?= reference
COMPRESSION_REFERENCE_PRESET ?= fast
COMPRESSION_REFERENCE_THREADS ?= auto
COMPRESSION_REFERENCE_ARGS ?=
COMPRESSION_SETTINGS ?=
COMPRESSION_QP ?=
COMPRESSION_AVM_TILE_COLUMNS ?= auto
COMPRESSION_AVM_TILE_ROWS ?= 0
COMPRESSION_REFRESH_REFERENCE ?= 0
COMPRESSION_DIRECT_SOURCE_FILES ?= 0
ENCODE_MATRIX_SET ?= local-aomctc-b2-scc-1080p-lossless-50f
ENCODE_MATRIX_OUT_DIR ?= verification/generated/encode_matrix
ENCODE_MATRIX_RUN ?=
ENCODE_MATRIX_CODECS ?=
ENCODE_MATRIX_MODES ?=
ENCODE_MATRIX_BASELINE ?=
ENCODE_MATRIX_LIMIT ?=
ENCODE_MATRIX_FRAMES ?=
ENCODE_MATRIX_AV2_LOSSY_QP ?= 24
ENCODE_MATRIX_VVC_LOSSY_QP ?= 19
ENCODE_MATRIX_VVC_FAST_SEARCH ?= lossless-speed
ENCODE_MATRIX_AV2_PREDICTIVE ?= 1
ENCODE_MATRIX_VVC_PREDICTIVE ?= 1
ENCODE_MATRIX_DIRECT_SOURCE_FILES ?= 1
ENCODE_MATRIX_WRITE_RECON ?= 0
ENCODE_MATRIX_CLEANUP_RECON ?= 0
ENCODE_MATRIX_CLEANUP_OUTPUT ?= 0
EXTERNAL_BENCHMARK_SET ?= local-aomctc-b2-scc-1080p-lossless-50f
EXTERNAL_BENCHMARK_OUT_DIR ?= verification/generated/market_encoder_compare
EXTERNAL_BENCHMARK_RUNNER ?= external-drivers/benchmark_external_encoders.py
EXTERNAL_BENCHMARK_RUN ?=
EXTERNAL_BENCHMARK_DRIVERS ?=
EXTERNAL_BENCHMARK_MODE ?= lossy
EXTERNAL_BENCHMARK_LIMIT ?=
EXTERNAL_BENCHMARK_FRAMES ?=
EXTERNAL_BENCHMARK_THREADS ?= 8
EXTERNAL_BENCHMARK_ALLOW_CONVERSION ?= 0
EXTERNAL_BENCHMARK_TARGET_PSNR ?=
EXTERNAL_BENCHMARK_AUTO_TUNE_PSNR ?= 0
EXTERNAL_BENCHMARK_AUTO_TUNE_MAX_ATTEMPTS ?= 8
EXTERNAL_BENCHMARK_ARGS ?=
VVC_HOTSPOT_SET ?= $(ENCODE_MATRIX_SET)
VVC_HOTSPOT_RUN ?= latest
VVC_HOTSPOT_OUT_DIR ?= verification/generated/profiling/vvc_hotspots
VVC_HOTSPOT_RUN_DIR ?= $(VVC_HOTSPOT_OUT_DIR)/$(VVC_HOTSPOT_RUN)
VVC_HOTSPOT_MATRIX_DIR ?= $(VVC_HOTSPOT_RUN_DIR)/encode_matrix
VVC_HOTSPOT_STATS_DIR ?= $(VVC_HOTSPOT_RUN_DIR)/stats
VVC_HOTSPOT_BASELINE ?=
VVC_HOTSPOT_LIMIT ?=
HOTSPOT_SET ?= $(ENCODE_MATRIX_SET)
HOTSPOT_RUN ?= latest
HOTSPOT_CODECS ?= vvc
HOTSPOT_MODES ?= lossless lossy
HOTSPOT_OUT_DIR ?= verification/generated/profiling/hotspots
HOTSPOT_RUN_DIR ?= $(HOTSPOT_OUT_DIR)/$(HOTSPOT_RUN)
HOTSPOT_MATRIX_DIR ?= $(HOTSPOT_RUN_DIR)/encode_matrix
HOTSPOT_STATS_DIR ?= $(HOTSPOT_RUN_DIR)/stats
HOTSPOT_BASELINE ?=
HOTSPOT_LIMIT ?=
HOTSPOT_VISUALIZE ?= 0
HOTSPOT_BROWSER_OUT ?= $(HOTSPOT_RUN_DIR)/code_browser.html
GEOMETRY_SWEEP_SETS ?= screenshot-sweep-444 screenshot-sweep-444-10bit screenshot-sweep-420-10bit-canary
GEOMETRY_SWEEP_CODECS ?= av2 vvc
GEOMETRY_SWEEP_MODES ?= lossless lossy
GEOMETRY_SWEEP_REFERENCE_MODE ?= off
GEOMETRY_SWEEP_AV2_LOSSY_QP ?= 24
GEOMETRY_SWEEP_VVC_LOSSY_QP ?= 24
GEOMETRY_SWEEP_AV2_SETTINGS ?= predictive
LIBAOM_SB_BITS ?= 0
LIBAOM_SB_BITS_BUILD_DIR ?= verification/references/libaom/libaom/build-sb-bits
LIBAOM_SB_BITS_ENCODER ?= $(LIBAOM_SB_BITS_BUILD_DIR)/aomenc
LIBAOM_SB_BITS_REFERENCE_ENV := $(if $(filter 1 true yes,$(LIBAOM_SB_BITS)),FRAMEFINERY_LIBAOM_SB_BITS_BUILD=1 FRAMEFINERY_LIBAOM_BUILD_DIR="$(abspath $(LIBAOM_SB_BITS_BUILD_DIR))" FRAMEFINERY_LIBAOM_ENCODER="$(abspath $(LIBAOM_SB_BITS_ENCODER))",)
AVM_SB_BITS ?= 0
AVM_SB_BITS_BUILD_DIR ?= verification/references/av2/avm/build-sb-bits
AVM_SB_BITS_ENCODER ?= $(AVM_SB_BITS_BUILD_DIR)/avmenc
AVM_SB_BITS_REFERENCE_ENV := $(if $(filter 1 true yes,$(AVM_SB_BITS)),FRAMEFINERY_AVM_SB_BITS_BUILD=1 FRAMEFINERY_AVM_BUILD_DIR="$(abspath $(AVM_SB_BITS_BUILD_DIR))" FRAMEFINERY_AVM_ENCODER="$(abspath $(AVM_SB_BITS_ENCODER))",)
REFERENCE_ENV := $(LIBAOM_SB_BITS_REFERENCE_ENV) $(AVM_SB_BITS_REFERENCE_ENV)
REFERENCE_CODEC ?= all
VALIDATION_STOP_FLAG := $(if $(filter 1 true yes,$(VALIDATION_STOP_ON_FAIL)),--stop-on-fail,)
VALIDATION_LIMIT_FLAG := $(if $(strip $(VALIDATION_LIMIT)),--limit "$(VALIDATION_LIMIT)",)
VALIDATION_SOURCE_FLAG := $(if $(filter 1 true yes,$(VALIDATION_SOURCE_FILTERS)),--source-filters,)
VALIDATION_DIRECT_SOURCE_FLAG := $(if $(filter 1 true yes,$(VALIDATION_DIRECT_SOURCE_FILES)),--direct-source-files,)
VALIDATION_SETTINGS_FLAG := $(foreach setting,$(VALIDATION_SETTINGS),--setting "$(setting)")
VALIDATION_FRAMES_FLAG := $(if $(strip $(VALIDATION_FRAMES)),--frames "$(VALIDATION_FRAMES)",)
VALIDATION_FORCE_LOSSY_FLAG := $(if $(filter 1 true yes,$(VALIDATION_FORCE_LOSSY)),--force-lossy,)
VALIDATION_CLEANUP_RECON_FLAG := $(if $(filter 1 true yes,$(VALIDATION_CLEANUP_RECON)),--cleanup-recon,)
VALIDATION_CLEANUP_OUTPUT_FLAG := $(if $(filter 1 true yes,$(VALIDATION_CLEANUP_OUTPUT)),--cleanup-output,)
COMPRESSION_LIMIT_FLAG := $(if $(strip $(COMPRESSION_LIMIT)),--limit "$(COMPRESSION_LIMIT)",)
COMPRESSION_REFERENCE_BACKEND_FLAG := --reference-backend "$(COMPRESSION_REFERENCE_BACKEND)"
COMPRESSION_REFERENCE_PRESET_FLAG := --reference-preset "$(COMPRESSION_REFERENCE_PRESET)"
COMPRESSION_REFERENCE_THREADS_FLAG := --reference-threads "$(COMPRESSION_REFERENCE_THREADS)"
COMPRESSION_AVM_TILE_COLUMNS_FLAG := --avm-tile-columns "$(COMPRESSION_AVM_TILE_COLUMNS)"
COMPRESSION_AVM_TILE_ROWS_FLAG := --avm-tile-rows "$(COMPRESSION_AVM_TILE_ROWS)"
COMPRESSION_REFERENCE_ARGS_FLAG := $(if $(strip $(COMPRESSION_REFERENCE_ARGS)),--reference-args "$(COMPRESSION_REFERENCE_ARGS)",)
COMPRESSION_SETTINGS_FLAG := $(foreach setting,$(COMPRESSION_SETTINGS),--setting "$(setting)")
COMPRESSION_QP_FLAG := $(if $(strip $(COMPRESSION_QP)),--setting "qp=$(COMPRESSION_QP)",)
COMPRESSION_REFRESH_REFERENCE_FLAG := $(if $(filter 1 true yes,$(COMPRESSION_REFRESH_REFERENCE)),--refresh-reference,)
COMPRESSION_DIRECT_SOURCE_FILES_FLAG := $(if $(filter 1 true yes,$(COMPRESSION_DIRECT_SOURCE_FILES)),--direct-source-files,)
ENCODE_MATRIX_RUN_FLAG := $(if $(strip $(ENCODE_MATRIX_RUN)),--run-name "$(ENCODE_MATRIX_RUN)",)
ENCODE_MATRIX_CODECS_FLAG := $(foreach codec,$(ENCODE_MATRIX_CODECS),--codec "$(codec)")
ENCODE_MATRIX_MODES_FLAG := $(foreach mode,$(ENCODE_MATRIX_MODES),--mode "$(mode)")
ENCODE_MATRIX_BASELINE_FLAG := $(if $(strip $(ENCODE_MATRIX_BASELINE)),--baseline-json "$(ENCODE_MATRIX_BASELINE)",)
ENCODE_MATRIX_LIMIT_FLAG := $(if $(strip $(ENCODE_MATRIX_LIMIT)),--limit "$(ENCODE_MATRIX_LIMIT)",)
ENCODE_MATRIX_FRAMES_FLAG := $(if $(strip $(ENCODE_MATRIX_FRAMES)),--frames "$(ENCODE_MATRIX_FRAMES)",)
ENCODE_MATRIX_VVC_FAST_SEARCH_FLAG := --vvc-fast-search "$(ENCODE_MATRIX_VVC_FAST_SEARCH)"
ENCODE_MATRIX_AV2_PREDICTIVE_FLAG := $(if $(filter 1 true yes,$(ENCODE_MATRIX_AV2_PREDICTIVE)),--av2-predictive,--no-av2-predictive)
ENCODE_MATRIX_VVC_PREDICTIVE_FLAG := $(if $(filter 1 true yes,$(ENCODE_MATRIX_VVC_PREDICTIVE)),--vvc-predictive,--no-vvc-predictive)
ENCODE_MATRIX_DIRECT_SOURCE_FILES_FLAG := $(if $(filter 1 true yes,$(ENCODE_MATRIX_DIRECT_SOURCE_FILES)),--direct-source-files,--no-direct-source-files)
ENCODE_MATRIX_WRITE_RECON_FLAG := $(if $(filter 1 true yes,$(ENCODE_MATRIX_WRITE_RECON)),--write-recon,)
ENCODE_MATRIX_CLEANUP_RECON_FLAG := $(if $(filter 1 true yes,$(ENCODE_MATRIX_CLEANUP_RECON)),--cleanup-recon,)
ENCODE_MATRIX_CLEANUP_OUTPUT_FLAG := $(if $(filter 1 true yes,$(ENCODE_MATRIX_CLEANUP_OUTPUT)),--cleanup-output,)
RELEASE_PERFORMANCE_RUN_FLAG := $(if $(strip $(RELEASE_PERFORMANCE_RUN)),--run-name "$(RELEASE_PERFORMANCE_RUN)",)
RELEASE_PERFORMANCE_FRAMES_FLAG := $(if $(filter 1 true yes,$(RELEASE_PERFORMANCE_FULL_STREAM)),--full-stream,--frames "$(RELEASE_PERFORMANCE_FRAMES)")
RELEASE_PERFORMANCE_CODECS_FLAG := $(foreach codec,$(RELEASE_PERFORMANCE_CODECS),--codec "$(codec)")
RELEASE_PERFORMANCE_MODES_FLAG := $(foreach mode,$(RELEASE_PERFORMANCE_MODES),--mode "$(mode)")
RELEASE_PERFORMANCE_LIMIT_FLAG := $(if $(strip $(RELEASE_PERFORMANCE_LIMIT)),--limit "$(RELEASE_PERFORMANCE_LIMIT)",)
RELEASE_PERFORMANCE_KEEP_BITSTREAMS_FLAG := $(if $(filter 1 true yes,$(RELEASE_PERFORMANCE_KEEP_BITSTREAMS)),--keep-bitstreams,)
EXTERNAL_BENCHMARK_RUN_FLAG := $(if $(strip $(EXTERNAL_BENCHMARK_RUN)),--run-name "$(EXTERNAL_BENCHMARK_RUN)",)
EXTERNAL_BENCHMARK_DRIVERS_FLAG := $(foreach driver,$(EXTERNAL_BENCHMARK_DRIVERS),--driver "$(driver)")
EXTERNAL_BENCHMARK_LIMIT_FLAG := $(if $(strip $(EXTERNAL_BENCHMARK_LIMIT)),--limit "$(EXTERNAL_BENCHMARK_LIMIT)",)
EXTERNAL_BENCHMARK_FRAMES_FLAG := $(if $(strip $(EXTERNAL_BENCHMARK_FRAMES)),--frames "$(EXTERNAL_BENCHMARK_FRAMES)",)
EXTERNAL_BENCHMARK_ALLOW_CONVERSION_FLAG := $(if $(filter 1 true yes,$(EXTERNAL_BENCHMARK_ALLOW_CONVERSION)),--allow-conversion,)
EXTERNAL_BENCHMARK_TARGET_PSNR_FLAG := $(if $(strip $(EXTERNAL_BENCHMARK_TARGET_PSNR)),--target-psnr "$(EXTERNAL_BENCHMARK_TARGET_PSNR)",)
EXTERNAL_BENCHMARK_AUTO_TUNE_PSNR_FLAG := $(if $(filter 1 true yes,$(EXTERNAL_BENCHMARK_AUTO_TUNE_PSNR)),--auto-tune-psnr,)
VVC_HOTSPOT_BASELINE_FLAG := $(if $(strip $(VVC_HOTSPOT_BASELINE)),--baseline-json "$(VVC_HOTSPOT_BASELINE)",)
VVC_HOTSPOT_LIMIT_FLAG := $(if $(strip $(VVC_HOTSPOT_LIMIT)),--limit "$(VVC_HOTSPOT_LIMIT)",)
HOTSPOT_CODECS_FLAG := $(foreach codec,$(HOTSPOT_CODECS),--codec "$(codec)")
HOTSPOT_MODES_FLAG := $(foreach mode,$(HOTSPOT_MODES),--mode "$(mode)")
HOTSPOT_BASELINE_FLAG := $(if $(strip $(HOTSPOT_BASELINE)),--baseline-json "$(HOTSPOT_BASELINE)",)
HOTSPOT_LIMIT_FLAG := $(if $(strip $(HOTSPOT_LIMIT)),--limit "$(HOTSPOT_LIMIT)",)
HOTSPOT_AV2_STATS_FLAG := $(if $(filter av2,$(HOTSPOT_CODECS)),--av2-stats-dir "$(HOTSPOT_STATS_DIR)",)
HOTSPOT_VVC_STATS_FLAG := $(if $(filter vvc,$(HOTSPOT_CODECS)),--vvc-stats-dir "$(HOTSPOT_STATS_DIR)",)
HOTSPOT_BUILD_AV2_STATS := $(if $(filter av2,$(HOTSPOT_CODECS)),1,0)
HOTSPOT_BUILD_VVC_STATS := $(if $(filter vvc,$(HOTSPOT_CODECS)),1,0)
CODE_BROWSER_PROFILE_FLAG := $(if $(strip $(CODE_BROWSER_PROFILE_JSON)),--profile-json "$(CODE_BROWSER_PROFILE_JSON)",)
GEOMETRY_SWEEP_AV2_SETTINGS_FLAG := $(foreach setting,$(GEOMETRY_SWEEP_AV2_SETTINGS),--setting $(setting))
GPROF_PROFILE_SETTINGS_FLAG := $(foreach setting,$(GPROF_PROFILE_SETTINGS),--set "$(setting)")

.PHONY: help check-tools fmt fmt-check check clippy-perf test doc package-list build debug run code-browser reference-list reference-setup test-vector-sets test-vectors validate-set validate-release-aomctc release-performance-table compare-compression benchmark-encode-matrix benchmark-external-encoders benchmark-external-driver-list bench-av2-micro bench-vvc-micro build-pgo llvm-vector-remarks profile-hotspots profile-vvc-hotspots summarize-hotspots summarize-vvc-hotspots validate-geometry-sweep profile-av2-i-lossless regression clean release-check ci ci-encode-smoke

help:
	@printf '%s\n' \
		'FrameFinery targets:' \
		'  make check-tools      Verify required local tools are available' \
		'  make fmt              Format the Rust workspace' \
		'  make fmt-check        Check Rust formatting without rewriting files' \
		'  make check            Type-check the Rust workspace' \
		'  make clippy-perf      Run Clippy performance lints on product features' \
		'  make test             Run Rust tests' \
		'  make doc              Build workspace API docs without dependencies' \
		'  make package-list     Show files that Cargo would include in each crate' \
		'  make build            Build release CLI and copy it to ./ff' \
		'                         Set CARGO_DEFAULT_FEATURES=0 to build only CARGO_FEATURES' \
		'                         Set AV2_SB_BITS=1 to compile AV2 per-superblock bit JSONL support' \
		'                         Set AV2_LOSSY_STATS=1 to compile AV2 lossy mode/TXB stats' \
		'                         Set AV2_STATS=1 to compile AV2 wall-time JSONL support' \
		'                         Set VVC_STATS=1 to compile VVC wall-time and CTU bit JSONL support' \
		'  make build PROFILE=optimized' \
		'                         Build ThinLTO/codegen-units=1 experiment to ./ff-optimized' \
		'  make build PROFILE=gprof' \
		'                         Build gprof sampling-friendly ./ff-gprof under target/gprof' \
		'  make profile-av2-i-lossless' \
		'                         Aggregate gprof samples for the first lossless AV2 I-frame' \
		'                         Override GPROF_SAMPLE_RUNS, GPROF_PROFILE_INPUT, or GPROF_PROFILE_SETTINGS' \
		'  make debug            Build the debug workspace artifacts' \
		'  make run ARGS="..."   Run the ff CLI' \
		'  make code-browser     Generate a standalone Rust module/code browser' \
		'                         Override CODE_BROWSER_OUT=verification/generated/code_browser/name.html' \
		'                         Add CODE_BROWSER_PROFILE_JSON=path/to/hotspots_profile.json for wall-time heatmaps' \
		'  make reference-list   List declared external reference tools' \
		'  make reference-setup  Clone/build declared references, REFERENCE_CODEC=all' \
		'  make test-vector-sets List generated-vector manifests' \
		'  make test-vectors     Generate TEST_VECTOR_SET=smoke vectors' \
		'  make validate-set     Encode VALIDATION_SET=smoke with CODEC=av2' \
		'                         Add VALIDATION_SOURCE_FILTERS=1 to skip input files' \
		'                         Add VALIDATION_DIRECT_SOURCE_FILES=1 for source_file rows' \
		'                         Add VALIDATION_CLEANUP_OUTPUT=1 to remove successful bitstreams' \
		'                         Use VALIDATION_REFERENCE_MODE=auto|required|off' \
		'                         Pass extra --set values with VALIDATION_SETTINGS="key ..."' \
		'  make validate-release-aomctc' \
		'                         Validate AV2/VVC lossy/lossless on AOM CTC A5/B2 Y4M streams' \
		'                         Uses RELEASE_AOMCTC_FRAMES=1 by default and cleans artifacts' \
		'  make compare-compression' \
		'                         Compare FrameFinery and reference encoder sizes' \
		'                         Uses CODEC=av2 COMPRESSION_SET=$(VALIDATION_SET)' \
		'                         Set COMPRESSION_REFERENCE_BACKEND=rav1e for lossy AV1 baseline' \
		'                         Set COMPRESSION_REFERENCE_BACKEND=ffmpeg-libaom for AV1 libaom baseline' \
		'                         Uses COMPRESSION_REFERENCE_PRESET=fast by default' \
		'                         Set COMPRESSION_REFERENCE_PRESET=realtime-screen for libaom screen-share settings' \
		'                         Set COMPRESSION_REFERENCE_PRESET=default for legacy args' \
		'                         Pass FrameFinery --set values with COMPRESSION_SETTINGS="key ..."' \
		'                         Set COMPRESSION_QP=24 for AV2/VVC lossy qp comparisons' \
		'                         Set COMPRESSION_REFERENCE_BACKEND=libaom for direct aomenc' \
		'                         Set LIBAOM_SB_BITS=1 for instrumented direct libaom builds' \
		'                         Set AVM_SB_BITS=1 for instrumented AVM reference builds' \
		'                         Set COMPRESSION_DIRECT_SOURCE_FILES=1 to feed source_file inputs directly' \
		'                         Set COMPRESSION_REFRESH_REFERENCE=1 to ignore cache' \
		'  make benchmark-encode-matrix' \
		'                         Time AV2/VVC lossy/lossless encodes over ENCODE_MATRIX_SET' \
		'                         Set ENCODE_MATRIX_FRAMES=1 for first-frame checks' \
		'                         Set ENCODE_MATRIX_CLEANUP_OUTPUT=1 to remove successful bitstreams' \
		'                         Set ENCODE_MATRIX_WRITE_RECON=1 to keep raw recon artifacts/checksums' \
		'  make release-performance-table' \
		'                         Generate the versioned release fps/bitrate/PSNR table' \
		'                         Uses RELEASE_PERFORMANCE_FRAMES=50 and cleans bitstreams by default' \
		'  make benchmark-external-encoders' \
		'                         Run an ignored local external-driver benchmark bundle' \
		'                         Set EXTERNAL_BENCHMARK_MODE=lossless for exact-mode checks' \
		'                         Set EXTERNAL_BENCHMARK_TARGET_PSNR=48:52 EXTERNAL_BENCHMARK_AUTO_TUNE_PSNR=1 for lossy quality alignment' \
		'                         Filter with EXTERNAL_BENCHMARK_DRIVERS="driver-id ..."' \
		'                         Pass driver-owned options with EXTERNAL_BENCHMARK_ARGS="..."' \
		'  make benchmark-external-driver-list' \
		'                         List drivers from the ignored local external-driver bundle' \
		'  make bench-av2-micro' \
		'                         Run Criterion microbenchmarks for AV2 palette and TXB kernels' \
		'  make bench-vvc-micro' \
		'                         Run Criterion microbenchmarks for VVC residual CTU kernels' \
		'  make build-pgo' \
		'                         Train PGO on PGO_SET=smoke and build ./ff-pgo' \
		'                         Set PGO_PROFILE=optimized for ThinLTO/codegen-units=1 PGO' \
		'  make llvm-vector-remarks' \
		'                         Emit LLVM vectorization remarks for framefinery-codecs' \
		'  make profile-hotspots' \
		'                         Build gated wall-time stats and profile first-frame codec hotspots' \
		'                         Set HOTSPOT_CODECS="av2 vvc" HOTSPOT_VISUALIZE=1 for heatmap browser output' \
		'                         Writes under HOTSPOT_OUT_DIR/HOTSPOT_RUN' \
		'  make summarize-hotspots' \
		'                         Summarize a previous generic hotspot run' \
		'  make validate-geometry-sweep' \
		'                         Run small geometry sweeps for AV2/VVC lossy/lossless modes' \
		'  make regression       Run smoke validation for AV2 and VVC' \
		'  make release-check    Run the default local quality gate' \
		'  make ci-encode-smoke  Encode generated pattern-source smoke vectors' \
		'  make ci               Run the same quality gate used by GitHub Actions' \
		'  make clean            Remove Cargo build outputs' \
		'' \
		'Optional build-time selection:' \
		'  make build CARGO_FEATURES=all    Build all normal product stages' \
		'  make build CARGO_FEATURES="av2 filter-scale"' \
		'  make build CARGO_FEATURES=        Build without optional stages'

check-tools:
	@command -v $(CARGO) >/dev/null || { echo 'error: cargo not found'; exit 1; }
	@$(CARGO) --version

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

check:
	$(CARGO) check --workspace $(CARGO_FLAGS)

clippy-perf:
	$(CARGO) clippy --workspace --features "$(PRODUCT_FEATURES)" -- -A clippy::all -W clippy::perf

test:
	$(CARGO) test --workspace $(CARGO_FLAGS)

doc:
	$(CARGO) doc --workspace --all-features --no-deps

package-list:
	$(CARGO) package --allow-dirty --list -p framefinery-core
	$(CARGO) package --allow-dirty --list -p framefinery-codecs
	$(CARGO) package --allow-dirty --list -p framefinery

build:
	$(BUILD_ENV) $(CARGO) build $(BUILD_CARGO_PROFILE_FLAG) -p framefinery $(CARGO_FLAGS)
	cp $(BUILD_TARGET_DIR)/$(BUILD_ARTIFACT_PROFILE)/ff $(BUILD_BINARY)
	chmod 755 $(BUILD_BINARY)

debug:
	$(CARGO) build --workspace $(CARGO_FLAGS)

run:
	$(CARGO) run -p framefinery $(CARGO_FLAGS) -- $(ARGS)

code-browser:
	$(PYTHON) scripts/generate_rust_code_browser.py --root . --output "$(CODE_BROWSER_OUT)" --title "$(CODE_BROWSER_TITLE)" $(CODE_BROWSER_PROFILE_FLAG)

reference-list:
	$(PYTHON) scripts/reference_tools.py list --codec "$(REFERENCE_CODEC)"

reference-setup:
	$(REFERENCE_ENV) $(PYTHON) scripts/reference_tools.py setup --codec "$(REFERENCE_CODEC)"

test-vector-sets:
	$(PYTHON) scripts/generate_test_vectors.py --set-dir "$(VALIDATION_SET_DIR)" --list-sets

test-vectors:
	$(PYTHON) scripts/generate_test_vectors.py "$(TEST_VECTOR_SET)" --set-dir "$(VALIDATION_SET_DIR)" --out-dir "$(VALIDATION_OUT_DIR)"

validate-set: build
	$(PYTHON) scripts/run_validation_set.py --ff "$(abspath $(BUILD_BINARY))" --codec "$(CODEC)" "$(VALIDATION_SET)" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --encoded-dir "$(VALIDATION_ENCODED_DIR)" --log-dir "$(VALIDATION_LOG_DIR)" --reference-mode "$(VALIDATION_REFERENCE_MODE)" $(VALIDATION_SOURCE_FLAG) $(VALIDATION_DIRECT_SOURCE_FLAG) $(VALIDATION_STOP_FLAG) $(VALIDATION_LIMIT_FLAG) $(VALIDATION_FRAMES_FLAG) $(VALIDATION_FORCE_LOSSY_FLAG) $(VALIDATION_SETTINGS_FLAG) $(VALIDATION_CLEANUP_RECON_FLAG) $(VALIDATION_CLEANUP_OUTPUT_FLAG)

validate-release-aomctc: build
	@df -h . /media/gabriel/storage
	$(PYTHON) scripts/run_validation_set.py --ff "$(abspath $(BUILD_BINARY))" --codec av2 "$(RELEASE_AOMCTC_SET)" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --encoded-dir "$(VALIDATION_ENCODED_DIR)" --log-dir "$(VALIDATION_LOG_DIR)" --reference-mode "$(RELEASE_AOMCTC_REFERENCE_MODE)" --direct-source-files --frames "$(RELEASE_AOMCTC_FRAMES)" --cleanup-recon --cleanup-output --stop-on-fail
	$(PYTHON) scripts/run_validation_set.py --ff "$(abspath $(BUILD_BINARY))" --codec av2 "$(RELEASE_AOMCTC_SET)" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --encoded-dir "$(VALIDATION_ENCODED_DIR)" --log-dir "$(VALIDATION_LOG_DIR)" --reference-mode "$(RELEASE_AOMCTC_REFERENCE_MODE)" --direct-source-files --frames "$(RELEASE_AOMCTC_FRAMES)" --force-lossy --setting "qp=$(RELEASE_AOMCTC_AV2_LOSSY_QP)" --cleanup-recon --cleanup-output --stop-on-fail
	$(PYTHON) scripts/run_validation_set.py --ff "$(abspath $(BUILD_BINARY))" --codec vvc "$(RELEASE_AOMCTC_SET)" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --encoded-dir "$(VALIDATION_ENCODED_DIR)" --log-dir "$(VALIDATION_LOG_DIR)" --reference-mode "$(RELEASE_AOMCTC_REFERENCE_MODE)" --direct-source-files --frames "$(RELEASE_AOMCTC_FRAMES)" $(foreach setting,$(RELEASE_AOMCTC_VVC_SETTINGS),--setting "$(setting)") --cleanup-recon --cleanup-output --stop-on-fail
	$(PYTHON) scripts/run_validation_set.py --ff "$(abspath $(BUILD_BINARY))" --codec vvc "$(RELEASE_AOMCTC_SET)" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --encoded-dir "$(VALIDATION_ENCODED_DIR)" --log-dir "$(VALIDATION_LOG_DIR)" --reference-mode "$(RELEASE_AOMCTC_REFERENCE_MODE)" --direct-source-files --frames "$(RELEASE_AOMCTC_FRAMES)" --force-lossy --setting "qp=$(RELEASE_AOMCTC_VVC_LOSSY_QP)" $(foreach setting,$(RELEASE_AOMCTC_VVC_SETTINGS),--setting "$(setting)") --cleanup-recon --cleanup-output --stop-on-fail
	@df -h . /media/gabriel/storage

compare-compression: build
	$(REFERENCE_ENV) $(PYTHON) scripts/compare_reference_compression.py --ff "$(abspath $(BUILD_BINARY))" --codec "$(CODEC)" "$(COMPRESSION_SET)" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --out-dir "$(COMPRESSION_OUT_DIR)" --log-dir "$(COMPRESSION_LOG_DIR)" $(COMPRESSION_LIMIT_FLAG) $(COMPRESSION_REFERENCE_BACKEND_FLAG) $(COMPRESSION_REFERENCE_PRESET_FLAG) $(COMPRESSION_REFERENCE_THREADS_FLAG) $(COMPRESSION_AVM_TILE_COLUMNS_FLAG) $(COMPRESSION_AVM_TILE_ROWS_FLAG) $(COMPRESSION_REFERENCE_ARGS_FLAG) $(COMPRESSION_SETTINGS_FLAG) $(COMPRESSION_QP_FLAG) $(COMPRESSION_REFRESH_REFERENCE_FLAG) $(COMPRESSION_DIRECT_SOURCE_FILES_FLAG)

benchmark-encode-matrix: build
	$(PYTHON) scripts/benchmark_encode_matrix.py "$(ENCODE_MATRIX_SET)" --ff "$(abspath $(BUILD_BINARY))" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --out-dir "$(ENCODE_MATRIX_OUT_DIR)" --av2-lossy-qp "$(ENCODE_MATRIX_AV2_LOSSY_QP)" --vvc-lossy-qp "$(ENCODE_MATRIX_VVC_LOSSY_QP)" $(ENCODE_MATRIX_VVC_FAST_SEARCH_FLAG) $(ENCODE_MATRIX_RUN_FLAG) $(ENCODE_MATRIX_CODECS_FLAG) $(ENCODE_MATRIX_MODES_FLAG) $(ENCODE_MATRIX_BASELINE_FLAG) $(ENCODE_MATRIX_LIMIT_FLAG) $(ENCODE_MATRIX_FRAMES_FLAG) $(ENCODE_MATRIX_AV2_PREDICTIVE_FLAG) $(ENCODE_MATRIX_VVC_PREDICTIVE_FLAG) $(ENCODE_MATRIX_DIRECT_SOURCE_FILES_FLAG) $(ENCODE_MATRIX_WRITE_RECON_FLAG) $(ENCODE_MATRIX_CLEANUP_RECON_FLAG) $(ENCODE_MATRIX_CLEANUP_OUTPUT_FLAG)

release-performance-table: build
	$(PYTHON) scripts/release_performance_table.py "$(RELEASE_PERFORMANCE_SET)" --ff "$(abspath $(BUILD_BINARY))" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --out-dir "$(RELEASE_PERFORMANCE_OUT_DIR)" $(RELEASE_PERFORMANCE_RUN_FLAG) $(RELEASE_PERFORMANCE_FRAMES_FLAG) $(RELEASE_PERFORMANCE_CODECS_FLAG) $(RELEASE_PERFORMANCE_MODES_FLAG) $(RELEASE_PERFORMANCE_LIMIT_FLAG) $(RELEASE_PERFORMANCE_KEEP_BITSTREAMS_FLAG)

benchmark-external-encoders: build
	@test -f "$(EXTERNAL_BENCHMARK_RUNNER)" || { printf '%s\n' "missing local external-driver runner: $(EXTERNAL_BENCHMARK_RUNNER)" "Place local comparison drivers under external-drivers/; that directory is gitignored."; exit 1; }
	PYTHONPATH="$(abspath scripts):$${PYTHONPATH}" $(PYTHON) "$(EXTERNAL_BENCHMARK_RUNNER)" "$(EXTERNAL_BENCHMARK_SET)" --ff "$(abspath $(BUILD_BINARY))" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --out-dir "$(EXTERNAL_BENCHMARK_OUT_DIR)" --mode "$(EXTERNAL_BENCHMARK_MODE)" --threads "$(EXTERNAL_BENCHMARK_THREADS)" --auto-tune-max-attempts "$(EXTERNAL_BENCHMARK_AUTO_TUNE_MAX_ATTEMPTS)" $(EXTERNAL_BENCHMARK_RUN_FLAG) $(EXTERNAL_BENCHMARK_DRIVERS_FLAG) $(EXTERNAL_BENCHMARK_LIMIT_FLAG) $(EXTERNAL_BENCHMARK_FRAMES_FLAG) $(EXTERNAL_BENCHMARK_TARGET_PSNR_FLAG) $(EXTERNAL_BENCHMARK_AUTO_TUNE_PSNR_FLAG) $(EXTERNAL_BENCHMARK_ALLOW_CONVERSION_FLAG) $(EXTERNAL_BENCHMARK_ARGS)

benchmark-external-driver-list:
	@test -f "$(EXTERNAL_BENCHMARK_RUNNER)" || { printf '%s\n' "missing local external-driver runner: $(EXTERNAL_BENCHMARK_RUNNER)" "Place local comparison drivers under external-drivers/; that directory is gitignored."; exit 1; }
	PYTHONPATH="$(abspath scripts):$${PYTHONPATH}" $(PYTHON) "$(EXTERNAL_BENCHMARK_RUNNER)" --list-drivers

bench-av2-micro:
	$(CARGO) bench -p framefinery-codecs --bench av2_micro --features "bench-internals vvc"

bench-vvc-micro:
	$(CARGO) bench -p framefinery-codecs --bench vvc_micro --features bench-internals

build-pgo:
	CARGO="$(CARGO)" \
	PYTHON="$(PYTHON)" \
	PRODUCT_FEATURES="$(PRODUCT_FEATURES)" \
	PGO_SET="$(PGO_SET)" \
	PGO_FRAMES="$(PGO_FRAMES)" \
	PGO_DIR="$(PGO_DIR)" \
	PGO_RUN="$(PGO_RUN)" \
	PGO_CODECS="$(PGO_CODECS)" \
	PGO_MODES="$(PGO_MODES)" \
	PGO_DIRECT_SOURCE_FILES="$(PGO_DIRECT_SOURCE_FILES)" \
	PGO_GENERATE_VECTORS="$(PGO_GENERATE_VECTORS)" \
	PGO_PROFILE="$(PGO_PROFILE)" \
	PGO_TARGET_DIR="$(PGO_TARGET_DIR)" \
	PGO_AV2_LOSSY_QP="$(PGO_AV2_LOSSY_QP)" \
	PGO_VVC_LOSSY_QP="$(PGO_VVC_LOSSY_QP)" \
	scripts/pgo_build.sh

llvm-vector-remarks:
	RUSTFLAGS="-Cdebuginfo=line-tables-only $(LLVM_REMARK_FLAGS)" \
	CARGO_TARGET_DIR="$(LLVM_REMARK_TARGET_DIR)" \
	$(CARGO) rustc --release -p "$(LLVM_REMARK_CRATE)" --features "$(LLVM_REMARK_FEATURES)" --lib

profile-hotspots:
	$(MAKE) build AV2_STATS=$(HOTSPOT_BUILD_AV2_STATS) VVC_STATS=$(HOTSPOT_BUILD_VVC_STATS)
	$(PYTHON) scripts/benchmark_encode_matrix.py "$(HOTSPOT_SET)" --ff "$(abspath $(BUILD_BINARY))" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --out-dir "$(HOTSPOT_MATRIX_DIR)" --run-name "$(HOTSPOT_RUN)" $(HOTSPOT_CODECS_FLAG) $(HOTSPOT_MODES_FLAG) --frames 1 --av2-lossy-qp "$(ENCODE_MATRIX_AV2_LOSSY_QP)" --vvc-lossy-qp "$(ENCODE_MATRIX_VVC_LOSSY_QP)" $(ENCODE_MATRIX_VVC_FAST_SEARCH_FLAG) $(HOTSPOT_AV2_STATS_FLAG) $(HOTSPOT_VVC_STATS_FLAG) $(HOTSPOT_BASELINE_FLAG) $(HOTSPOT_LIMIT_FLAG) $(ENCODE_MATRIX_DIRECT_SOURCE_FILES_FLAG) $(ENCODE_MATRIX_WRITE_RECON_FLAG) $(ENCODE_MATRIX_CLEANUP_RECON_FLAG)
	$(PYTHON) scripts/summarize_hotspots.py "$(HOTSPOT_RUN_DIR)" --encode-matrix-json "$(HOTSPOT_MATRIX_DIR)/$(HOTSPOT_RUN).json" $(HOTSPOT_CODECS_FLAG)
	@if [ "$(HOTSPOT_VISUALIZE)" = "1" ] || [ "$(HOTSPOT_VISUALIZE)" = "true" ] || [ "$(HOTSPOT_VISUALIZE)" = "yes" ]; then \
		$(PYTHON) scripts/generate_rust_code_browser.py --root . --output "$(HOTSPOT_BROWSER_OUT)" --title "FrameFinery Engine Hotspots: $(HOTSPOT_RUN)" --profile-json "$(HOTSPOT_RUN_DIR)/hotspots_profile.json"; \
	fi

profile-vvc-hotspots:
	$(MAKE) profile-hotspots HOTSPOT_SET="$(VVC_HOTSPOT_SET)" HOTSPOT_RUN="$(VVC_HOTSPOT_RUN)" HOTSPOT_CODECS=vvc HOTSPOT_OUT_DIR="$(VVC_HOTSPOT_OUT_DIR)" HOTSPOT_BASELINE="$(VVC_HOTSPOT_BASELINE)" HOTSPOT_LIMIT="$(VVC_HOTSPOT_LIMIT)" HOTSPOT_VISUALIZE="$(HOTSPOT_VISUALIZE)"

summarize-hotspots:
	$(PYTHON) scripts/summarize_hotspots.py "$(HOTSPOT_RUN_DIR)" --encode-matrix-json "$(HOTSPOT_MATRIX_DIR)/$(HOTSPOT_RUN).json" $(HOTSPOT_CODECS_FLAG)

summarize-vvc-hotspots:
	$(MAKE) summarize-hotspots HOTSPOT_RUN="$(VVC_HOTSPOT_RUN)" HOTSPOT_CODECS=vvc HOTSPOT_OUT_DIR="$(VVC_HOTSPOT_OUT_DIR)"

validate-geometry-sweep: build
	for codec in $(GEOMETRY_SWEEP_CODECS); do \
		for mode in $(GEOMETRY_SWEEP_MODES); do \
			for set in $(GEOMETRY_SWEEP_SETS); do \
				extra=""; \
				settings=""; \
				if [ "$$codec" = "av2" ]; then settings='$(GEOMETRY_SWEEP_AV2_SETTINGS_FLAG)'; fi; \
				if [ "$$mode" = "lossy" ]; then extra="--force-lossy"; fi; \
				if [ "$$codec" = "av2" ] && [ "$$mode" = "lossy" ]; then extra="$$extra --setting qp=$(GEOMETRY_SWEEP_AV2_LOSSY_QP)"; fi; \
				if [ "$$codec" = "vvc" ] && [ "$$mode" = "lossy" ]; then extra="$$extra --setting qp=$(GEOMETRY_SWEEP_VVC_LOSSY_QP)"; fi; \
				$(PYTHON) scripts/run_validation_set.py --ff "$(abspath $(BUILD_BINARY))" --codec "$$codec" "$$set" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --encoded-dir "$(VALIDATION_ENCODED_DIR)" --log-dir "$(VALIDATION_LOG_DIR)" --reference-mode "$(GEOMETRY_SWEEP_REFERENCE_MODE)" --stop-on-fail $$settings $$extra; \
			done; \
		done; \
	done

profile-av2-i-lossless:
	$(MAKE) build PROFILE=gprof
	mkdir -p "$(GPROF_PROFILE_SAMPLE_DIR)"
	rm -f "$(GPROF_PROFILE_SAMPLE_DIR)"/gmon.* "$(GPROF_PROFILE_REPORT)" "$(GPROF_PROFILE_OUTPUT)" "$(GPROF_PROFILE_RECON)" "$(GPROF_PROFILE_RUN_LOG)"
	for i in $$(seq 1 $(GPROF_SAMPLE_RUNS)); do \
		if ! GMON_OUT_PREFIX="$(GPROF_PROFILE_SAMPLE_DIR)/gmon" ./ff-gprof encode "$(GPROF_PROFILE_INPUT)" --frames "$(GPROF_PROFILE_FRAMES)" --encode "$(GPROF_PROFILE_CODEC):$(GPROF_PROFILE_OUTPUT)" --recon "$(GPROF_PROFILE_RECON)" $(GPROF_PROFILE_SETTINGS_FLAG) >"$(GPROF_PROFILE_RUN_LOG)" 2>&1; then \
			cat "$(GPROF_PROFILE_RUN_LOG)"; \
			exit 1; \
		fi; \
	done
	gprof -b ./ff-gprof "$(GPROF_PROFILE_SAMPLE_DIR)"/gmon.* > "$(GPROF_PROFILE_REPORT)"
	@printf 'wrote %s from %s first-frame run(s)\n' "$(GPROF_PROFILE_REPORT)" "$(GPROF_SAMPLE_RUNS)"
	@head -40 "$(GPROF_PROFILE_REPORT)"

regression: build
	$(PYTHON) scripts/run_validation_set.py --ff "$(abspath $(BUILD_BINARY))" --codec av2 smoke --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --encoded-dir "$(VALIDATION_ENCODED_DIR)" --log-dir "$(VALIDATION_LOG_DIR)" --reference-mode "$(VALIDATION_REFERENCE_MODE)" --stop-on-fail
	$(PYTHON) scripts/run_validation_set.py --ff "$(abspath $(BUILD_BINARY))" --codec vvc smoke --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --encoded-dir "$(VALIDATION_ENCODED_DIR)" --log-dir "$(VALIDATION_LOG_DIR)" --reference-mode "$(VALIDATION_REFERENCE_MODE)" --stop-on-fail

release-check: check-tools fmt-check check test doc package-list build

ci-encode-smoke: build
	$(PYTHON) scripts/run_validation_set.py --ff "$(abspath $(BUILD_BINARY))" --codec av2 "$(CI_ENCODE_SET)" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --encoded-dir "$(VALIDATION_ENCODED_DIR)" --log-dir "$(VALIDATION_LOG_DIR)" --reference-mode off --source-filters --cleanup-recon --cleanup-output --stop-on-fail
	$(PYTHON) scripts/run_validation_set.py --ff "$(abspath $(BUILD_BINARY))" --codec vvc "$(CI_ENCODE_SET)" --set-dir "$(VALIDATION_SET_DIR)" --vector-dir "$(VALIDATION_OUT_DIR)" --encoded-dir "$(VALIDATION_ENCODED_DIR)" --log-dir "$(VALIDATION_LOG_DIR)" --reference-mode off --source-filters --cleanup-recon --cleanup-output --stop-on-fail

ci: release-check ci-encode-smoke

clean:
	$(CARGO) clean
	rm -f ./ff ./ff-gprof gmon.out gprof.txt
