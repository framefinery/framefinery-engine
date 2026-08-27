use crate::picture::{ChromaSampling, SampleBitDepth};

#[cfg(test)]
use super::super::VvcTreeType;
use super::super::{
    chroma_subsample_x, chroma_subsample_y, vvc_chroma_cclm_node_allowed,
    vvc_chroma_explicit_candidate_index, vvc_chroma_explicit_candidates,
    vvc_chroma_intra_mode_syntax_bin_count, vvc_chroma_transform_nodes_into,
    vvc_downshift_sample_to_u8, vvc_luma_intra_mode_from_index, vvc_luma_intra_mode_is_mpm,
    vvc_luma_intra_mode_syntax_bin_count, vvc_neutral_sample,
    vvc_residual_chroma_explicit_candidate_allowed, VvcBdpcmMode, VvcChromaCclmMode,
    VvcChromaIntraCandidateCost, VvcChromaIntraCandidateCosts, VvcChromaIntraPredictionMode,
    VvcChromaTuCodingDecision, VvcCodingTreeNode, VvcCtuPartitionShape, VvcCtuRegion,
    VvcFastSearch, VvcIntraPredictionMode, VvcLumaIntraCandidateCost, VvcLumaIntraCandidateCosts,
    VvcLumaInterDecision, VvcLumaSccDecision, VvcLumaTuCodingDecision, VvcPictureFormat,
    VvcIbcCuDecision, VvcReconstructionFrame, VvcResidualCodingMode, VvcResidualCodingPolicy,
    VvcResidualScoreMetric, VvcSample, VvcSampledColor, VvcSampledFrame,
    VvcTuResidualCodingMode, VvcVideoGeometry, VVC_CHROMA_INTRA_CANDIDATE_CAPACITY, VVC_CTU_SIZE,
    VVC_LUMA_INTRA_CANDIDATE_CAPACITY,
};
use super::transform::{
    luma_ac_syntax_cost_estimate, luma_reconstructed_residual_sse_with_mts_into,
    quantize_vvc_luma_residual_fast_with_qp_and_mts_into,
    quantize_vvc_luma_residual_greedy_with_qp_and_mts_into, transformed_dc_only_residual_sse,
};
use super::{
    fill_visible_chroma_node, fill_visible_luma_node,
    inverse_transform_vvc_chroma_quantized_block_into_with_qp,
    inverse_transform_vvc_luma_quantized_block_into_with_qp_and_mts,
    predict_vvc_chroma_bdpcm_block_into_with_availability,
    predict_vvc_chroma_cclm_block_into_with_availability,
    predict_vvc_chroma_cclm_pair_into_with_availability,
    predict_vvc_chroma_intra_block_into_with_availability,
    predict_vvc_luma_bdpcm_block_into_with_availability,
    predict_vvc_luma_intra_block_into_with_availability,
    predict_vvc_luma_intra_block_into_with_mrl_and_availability,
    quantize_vvc_chroma_residual_greedy_with_qp, quantize_vvc_chroma_sample,
    reconstruct_vvc_chroma, residual_vvc_luma_bdpcm_block_into_with_availability,
    VvcDcPredictionScratch, VvcInverseTransformScratch, VvcQuantizedColor,
    VvcQuantizedResidualFrame,
    MAX_VVC_CHROMA_TUS, MAX_VVC_LUMA_TUS, VVC_CHROMA_AC_COEFFS_PER_TU,
    VVC_CHROMA_AC_POSITIONS_4X4, VVC_LUMA_AC_COEFFS_PER_TU,
};
#[cfg(feature = "vvc-stats")]
use super::{
    VvcChromaPredictionStatsFamily, VvcIntraSearchStats, VvcLumaPredictionStatsFamily,
    VvcResidualEnergyStats,
};
#[cfg(feature = "vvc-stats")]
use crate::instrumentation::JsonlInstrumentationSink;
#[cfg(feature = "vvc-stats")]
use crate::timing::StageStart;

#[cfg(not(feature = "vvc-stats"))]
struct VvcIntraSearchStats;

#[cfg(feature = "vvc-stats")]
const VVC_TU_TRACE_ENV: &str = "FRAMEFINERY_VVC_TU_TRACE";
const VVC_ENABLE_LUMA_MRL_SELECTION: bool = true;
const VVC_ENABLE_LUMA_MTS_SELECTION: bool = true;
const VVC_ENABLE_LOSSY_TRANSFORM_SKIP_SELECTION: bool = true;
const VVC_ENABLE_BDPCM_SELECTION: bool = true;
const VVC_TRANSFORM_SKIP_MAX_SIZE: u16 = 8;
const VVC_TRANSFORM_SKIP_INV_QUANT_SCALES: [i32; 6] = [40, 45, 51, 57, 64, 72];
const VVC_TRANSFORM_SKIP_LEVEL_SEARCH_RADIUS: i64 = 1;
const VVC_TRANSFORM_SKIP_QUANT_TABLE_LEN: usize = 1 << 16;
const VVC_LUMA_EXPLICIT_MTS_CANDIDATES: [u8; 4] = [2, 3, 4, 5];
const VVC_LOSSY_LUMA_RD_WINNER_CANDIDATES: usize = 5;
const VVC_LOSSY_CHROMA_RD_WINNER_CANDIDATES: usize = 5;
const VVC_LUMA_MIN_INTRA_MODE_SYNTAX_BINS: u8 = 2;

#[cfg(feature = "vvc-stats")]
fn vvc_elapsed_nanos(start: StageStart) -> u64 {
    start.elapsed_nanos()
}

#[cfg(feature = "vvc-stats")]
fn vvc_luma_prediction_stats_family(mode: VvcIntraPredictionMode) -> VvcLumaPredictionStatsFamily {
    match mode {
        VvcIntraPredictionMode::Dc => VvcLumaPredictionStatsFamily::Dc,
        VvcIntraPredictionMode::Planar => VvcLumaPredictionStatsFamily::Planar,
        VvcIntraPredictionMode::Horizontal
        | VvcIntraPredictionMode::Vertical
        | VvcIntraPredictionMode::Angular(_) => VvcLumaPredictionStatsFamily::Directional,
    }
}

#[cfg(feature = "vvc-stats")]
fn vvc_chroma_prediction_stats_family(
    mode: VvcChromaIntraPredictionMode,
) -> VvcChromaPredictionStatsFamily {
    match mode {
        VvcChromaIntraPredictionMode::Derived => VvcChromaPredictionStatsFamily::Derived,
        VvcChromaIntraPredictionMode::Explicit(_) => VvcChromaPredictionStatsFamily::Explicit,
        VvcChromaIntraPredictionMode::Cclm(_) => VvcChromaPredictionStatsFamily::Cclm,
    }
}

#[derive(Debug, Clone)]
pub(in crate::vvc) struct VvcTransformSkipQuantTables {
    luma: VvcTransformSkipQuantTable,
    chroma: VvcTransformSkipQuantTable,
}

impl VvcTransformSkipQuantTables {
    pub(in crate::vvc) fn new(bit_depth: SampleBitDepth, luma_qp: i32, chroma_qp: i32) -> Self {
        Self {
            luma: VvcTransformSkipQuantTable::new(bit_depth, luma_qp),
            chroma: VvcTransformSkipQuantTable::new(bit_depth, chroma_qp),
        }
    }

    fn luma(&self) -> &VvcTransformSkipQuantTable {
        &self.luma
    }

    fn chroma(&self) -> &VvcTransformSkipQuantTable {
        &self.chroma
    }
}

#[derive(Debug, Clone)]
struct VvcTransformSkipQuantTable {
    levels: Vec<i16>,
    reconstructed: Vec<i16>,
}

impl VvcTransformSkipQuantTable {
    fn new(bit_depth: SampleBitDepth, qp: i32) -> Self {
        let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
        let mut levels = Vec::with_capacity(VVC_TRANSFORM_SKIP_QUANT_TABLE_LEN);
        let mut reconstructed = Vec::with_capacity(VVC_TRANSFORM_SKIP_QUANT_TABLE_LEN);
        for value_bits in 0..VVC_TRANSFORM_SKIP_QUANT_TABLE_LEN {
            let residual = (value_bits as u16) as i16;
            levels.push(quantize_vvc_transform_skip_level_with_params(
                residual,
                scale,
                right_shift,
                VVC_TRANSFORM_SKIP_LEVEL_SEARCH_RADIUS,
            ));
        }
        for value_bits in 0..VVC_TRANSFORM_SKIP_QUANT_TABLE_LEN {
            let level = (value_bits as u16) as i16;
            reconstructed.push(reconstruct_vvc_transform_skip_level_with_params(
                level,
                scale,
                right_shift,
            ));
        }
        Self {
            levels,
            reconstructed,
        }
    }

    #[inline]
    fn level(&self, residual: i16) -> i16 {
        self.levels[usize::from(residual as u16)]
    }

    #[inline]
    fn reconstructed(&self, level: i16) -> i16 {
        self.reconstructed[usize::from(level as u16)]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VvcLumaResidualQuantizationSearch {
    Full,
    FastModeDecision,
    TransformSkipFirstModeDecision,
}
