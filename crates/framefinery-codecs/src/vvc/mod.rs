//! First-target VVC/H.266 syntax experiments.
//!
//! This module contains a clean-room VVC path for small all-intra validation
//! streams across parameterized geometries. It is still intentionally
//! incomplete: CABAC, CTU syntax generation, transform/quant, prediction, and
//! reconstruction semantics need to keep converging toward real implementations
//! before FrameFinery can encode arbitrary input pictures.

use std::io::{Cursor, Read, Write};

use crate::instrumentation::CountingWriter;
#[cfg(feature = "vvc-stats")]
use crate::instrumentation::JsonlInstrumentationSink;
use crate::picture::{
    chroma_subsample_x as planar_chroma_subsample_x,
    chroma_subsample_y as planar_chroma_subsample_y, pack_planar_samples, read_input_frame,
    unpack_planar_samples, ChromaSampling, FrameLimit, Picture, PixelFormat, PlanarYuvFrameLayout,
    PlanarYuvGeometry, SampleBitDepth,
};
use crate::timing::StageStart;

#[cfg(feature = "bench-internals")]
#[doc(hidden)]
#[path = "benchmarks/bench.rs"]
pub mod bench;
mod cabac;
#[path = "headers/mod.rs"]
mod header;
#[path = "inter/ibc.rs"]
mod ibc;
mod interface;
#[path = "inter/motion.rs"]
mod motion;
#[path = "bitstream/nal.rs"]
mod nal;
mod palette;
mod residual;
#[path = "bitstream/syntax.rs"]
mod syntax;
use cabac::VvcFrameCtuCabacState;
use cabac::{
    encode_ctu_partition_body, vvc_chroma_intra_mode_syntax_bin_count, vvc_chroma_transform_nodes,
    vvc_chroma_transform_nodes_into, vvc_luma_intra_mode_is_mpm,
    vvc_luma_intra_mode_syntax_bin_count, vvc_luma_transform_nodes, vvc_luma_transform_nodes_into,
    VvcCabacContext, VvcCabacContexts, VvcCabacDumpContextEvent, VvcCabacDumpSymbol,
    VvcCabacEncoder, VvcCabacPayload, VvcCodingTreeNode, VvcCtuCabacOp, VvcCtuPartitionParams,
    VvcCtuPartitionShape, VvcLastSigCoeffPrefixCtxInput, VvcPartSplit,
};
#[cfg(test)]
use cabac::{
    encode_ctu_partition_body_with_contexts, initial_vvc_cabac_contexts,
    vvc_luma_mpm_list_for_test, VvcCtuCabacGenerator, VvcQtSplitCtxInput, VvcSplitCtxInput,
    VvcTreeType,
};
use header::{
    vvc_frame_slice_unit, vvc_one_slice_per_ctu_partitioning_supported, vvc_picture_ctu_cols,
    vvc_picture_ctu_count, vvc_picture_ctu_rows, vvc_pps_unit_with_partitioning_and_config,
    vvc_predictive_ctu_slice_units_with_inter_skip_cache, vvc_predictive_frame_slice_unit,
    vvc_sps_unit, VvcCtuInterSkipSlicePayloadCache, VvcPicturePartitioning,
};
#[cfg(test)]
use header::{
    vvc_poc_lsb_for_frame_idx, vvc_pps_rbsp, vvc_pps_rbsp_with_partitioning_and_config,
    vvc_predictive_ctu_slice_units_uncached_for_test, vvc_predictive_frame_skip_slice_unit,
    vvc_predictive_frame_skip_slice_unit_with_cached_payload,
    vvc_predictive_frame_skip_slice_unit_with_payload, vvc_slice_address_bits, vvc_slice_payload,
    vvc_slice_rbsp, vvc_slice_type_for_ctus, vvc_sps_payload, vvc_sps_rbsp,
    write_vvc_coding_tree_entropy, VvcFrameSkipPayloadCache, VvcPictureKind, VvcSliceType,
};
pub use nal::{
    nal_unit_header_bytes, parse_annex_b_nal_units, write_annex_b, write_nal_unit_header,
    VvcNalHeader, VvcNalInfo, VvcNalUnit, VvcNalUnitType,
};
pub use palette::vvc_palette_444_cabac_dump_json;
#[cfg(test)]
use palette::{
    vvc_palette_444_binarized_syntax_bits, vvc_palette_444_cabac_context_bins,
    vvc_palette_444_context_audit_rows, vvc_palette_444_cu_syntax,
    vvc_palette_444_cu_syntax_with_config, vvc_palette_444_decode_reconstruction,
    vvc_palette_444_new_entry_token_bit_counts, vvc_palette_444_reconstruction_yuv,
    vvc_palette_444_reconstruction_yuv_with_config, vvc_palette_444_single_entry_syntax,
    vvc_palette_444_syntax_tokens, vvc_palette_run_copy_context_id_for_audit,
    vvc_palette_transform_skip_coded_coeff_for_test,
    vvc_palette_transform_skip_coded_coeff_with_config_for_test, VvcPalettePredictorMode,
    VvcPaletteTreeType,
};
pub use residual::quantize_vvc_color;
#[cfg(test)]
use residual::VVC_LUMA_DC_BASE;
use residual::{
    quantize_vvc_frame,
    quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes_and_scratch_with_mode_hints,
    VvcCtuQuantScratch, VvcLumaModeSearchState, VvcPlaneAvailability, VvcQuantizedColor,
    VvcResidualCabacOptions, VvcResidualComponent, VvcTransformSkipQuantTables, MAX_VVC_CHROMA_TUS,
    MAX_VVC_LUMA_TUS, VVC_CHROMA_AC_COEFFS_PER_TU, VVC_DEFAULT_LOSSY_LUMA_QP,
    VVC_LUMA_AC_COEFFS_PER_TU,
};
#[cfg(test)]
use residual::{VvcResidualCabacEncoder, VvcResidualCtxConfig, VvcResidualPass1State};
pub use syntax::{VvcSyntaxCode, VvcSyntaxField, VvcSyntaxRbsp, VvcSyntaxWriter};

include!("api.rs");
include!("geometry.rs");
include!("format.rs");
include!("mode_decision.rs");
include!("sampling.rs");
include!("reconstruction.rs");
include!("ctu.rs");
include!("ctu_params.rs");
include!("cabac_dump.rs");
include!("stats.rs");
include!("encode.rs");
#[cfg(test)]
include!("test_support.rs");

pub use interface::{
    VVC_CODEC, VVC_FAST_SEARCH_SETTING, VVC_FAST_SEARCH_SETTING_SPEC, VVC_PROFILE_SETTING,
    VVC_PROFILE_SETTING_SPEC,
};

#[cfg(test)]
mod tests;
