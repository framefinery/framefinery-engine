use crate::picture::{ChromaSampling, SampleBitDepth};

#[cfg(test)]
use super::super::VvcTreeType;
use super::super::{
    chroma_subsample_x, chroma_subsample_y, vvc_chroma_cclm_node_allowed,
    vvc_chroma_explicit_candidates, vvc_chroma_intra_mode_syntax_bin_count,
    vvc_chroma_transform_nodes, vvc_downshift_sample_to_u8, vvc_luma_intra_mode_from_index,
    vvc_luma_intra_mode_is_mpm, vvc_luma_intra_mode_syntax_bin_count, vvc_luma_transform_nodes,
    vvc_neutral_sample, vvc_residual_chroma_explicit_candidate_allowed, VvcBdpcmMode,
    VvcChromaCclmMode, VvcChromaIntraCandidateCost, VvcChromaIntraCandidateCosts,
    VvcChromaIntraPredictionMode, VvcChromaTuCodingDecision, VvcCodingTreeNode,
    VvcCtuPartitionShape, VvcCtuRegion, VvcIntraPredictionMode, VvcLumaIntraCandidateCost,
    VvcLumaIntraCandidateCosts, VvcLumaTuCodingDecision, VvcPictureFormat, VvcReconstructionFrame,
    VvcResidualCodingMode, VvcResidualCodingPolicy, VvcResidualScoreMetric, VvcSample,
    VvcSampledColor, VvcSampledFrame, VvcTuResidualCodingMode, VvcVideoGeometry,
    VVC_CHROMA_INTRA_CANDIDATE_CAPACITY, VVC_CTU_SIZE, VVC_LUMA_INTRA_CANDIDATE_CAPACITY,
};
use super::transform::{
    luma_ac_syntax_cost_estimate, luma_reconstructed_residual_sse_with_mts_into,
    quantize_vvc_luma_residual_fast_with_qp_and_mts_into,
    quantize_vvc_luma_residual_greedy_with_qp_and_mts_into,
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
    reconstruct_vvc_chroma, VvcDcPredictionScratch, VvcInverseTransformScratch, VvcQuantizedColor,
    VvcQuantizedResidualFrame, MAX_VVC_CHROMA_TUS, MAX_VVC_LUMA_TUS, VVC_CHROMA_AC_COEFFS_PER_TU,
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
use std::time::Instant;

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
fn vvc_elapsed_nanos(start: Instant) -> u64 {
    start.elapsed().as_nanos() as u64
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
            reconstructed.push(reconstruct_vvc_transform_skip_level_with_params(
                residual,
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
}

pub fn quantize_vvc_color(color: VvcSampledColor) -> VvcQuantizedColor {
    quantize_vvc_frame(&VvcSampledFrame::solid(color))
}

pub(in crate::vvc) fn quantize_vvc_frame(frame: &VvcSampledFrame) -> VvcQuantizedColor {
    quantize_vvc_frame_with_reconstruction(frame).quantized
}

pub(in crate::vvc) fn quantize_vvc_frame_with_reconstruction(
    frame: &VvcSampledFrame,
) -> VvcQuantizedResidualFrame {
    let mut reconstruction = VvcReconstructionFrame::new_neutral(frame.geometry, frame.format);
    let region = VvcCtuRegion {
        slice_address: 0,
        origin_x: 0,
        origin_y: 0,
        geometry: frame.geometry,
    };
    let quantized = quantize_vvc_residual_ctu_into_frame_reconstruction(
        frame,
        &mut reconstruction,
        region,
        VvcResidualCodingMode::Lossy,
    );
    let mut reconstruction_yuv =
        Vec::with_capacity(frame.geometry.luma_samples() + frame.chroma_len * 2);
    reconstruction_yuv.extend_from_slice(&reconstruction.luma);
    reconstruction_yuv.extend_from_slice(&reconstruction.cb);
    reconstruction_yuv.extend_from_slice(&reconstruction.cr);
    VvcQuantizedResidualFrame {
        quantized,
        reconstruction_yuv,
    }
}

pub(in crate::vvc) fn quantize_vvc_residual_ctu_into_frame_reconstruction(
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    region: VvcCtuRegion,
    residual_mode: VvcResidualCodingMode,
) -> VvcQuantizedColor {
    let policy = VvcResidualCodingPolicy::new(source_frame.format, residual_mode);
    let (luma_qp, chroma_qp) = match residual_mode {
        VvcResidualCodingMode::Lossless => {
            let qp = super::super::vvc_lossless_slice_qp(source_frame.format.bit_depth);
            (qp, qp)
        }
        VvcResidualCodingMode::Lossy => (
            super::VVC_DEFAULT_LOSSY_LUMA_QP,
            super::VVC_DEFAULT_LOSSY_CHROMA_QP,
        ),
    };
    quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp(
        source_frame,
        frame_recon,
        region,
        policy,
        luma_qp,
        chroma_qp,
    )
}

pub(in crate::vvc) fn quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp(
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    region: VvcCtuRegion,
    policy: VvcResidualCodingPolicy,
    luma_qp: i32,
    chroma_qp: i32,
) -> VvcQuantizedColor {
    let mut luma_mode_search_state =
        VvcLumaModeSearchState::new_for_geometry(source_frame.geometry);
    let transform_skip_quant_tables =
        VvcTransformSkipQuantTables::new(source_frame.format.bit_depth, luma_qp, chroma_qp);
    quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes(
        source_frame,
        frame_recon,
        region,
        policy,
        luma_qp,
        chroma_qp,
        &mut luma_mode_search_state,
        &transform_skip_quant_tables,
    )
}

pub(in crate::vvc) fn quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes(
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    region: VvcCtuRegion,
    policy: VvcResidualCodingPolicy,
    luma_qp: i32,
    chroma_qp: i32,
    luma_mode_search_state: &mut VvcLumaModeSearchState,
    transform_skip_quant_tables: &VvcTransformSkipQuantTables,
) -> VvcQuantizedColor {
    let mut luma_tu_remainders = [0; MAX_VVC_LUMA_TUS];
    let mut luma_tu_negative = [false; MAX_VVC_LUMA_TUS];
    let mut luma_tu_dc_levels = [0; MAX_VVC_LUMA_TUS];
    let mut luma_tu_intra_modes = [VvcIntraPredictionMode::Dc; MAX_VVC_LUMA_TUS];
    let mut luma_tu_ac_levels = [[0; VVC_LUMA_AC_COEFFS_PER_TU]; MAX_VVC_LUMA_TUS];
    let mut luma_tu_has_ac = [false; MAX_VVC_LUMA_TUS];
    let mut luma_tu_transform_skip = [false; MAX_VVC_LUMA_TUS];
    let mut luma_tu_bdpcm_modes = [VvcBdpcmMode::None; MAX_VVC_LUMA_TUS];
    let mut luma_tu_mrl_index = [0; MAX_VVC_LUMA_TUS];
    let mut luma_tu_mts_index = [0; MAX_VVC_LUMA_TUS];
    let mut cb_tu_dc_levels = [0; MAX_VVC_CHROMA_TUS];
    let mut cr_tu_dc_levels = [0; MAX_VVC_CHROMA_TUS];
    let mut cb_tu_ac_levels = [[0; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS];
    let mut cr_tu_ac_levels = [[0; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS];
    let mut cb_tu_has_ac = [false; MAX_VVC_CHROMA_TUS];
    let mut cr_tu_has_ac = [false; MAX_VVC_CHROMA_TUS];
    let mut cb_tu_transform_skip = [false; MAX_VVC_CHROMA_TUS];
    let mut cr_tu_transform_skip = [false; MAX_VVC_CHROMA_TUS];
    let mut chroma_tu_bdpcm_modes = [VvcBdpcmMode::None; MAX_VVC_CHROMA_TUS];
    let mut chroma_tu_intra_modes = [VvcChromaIntraPredictionMode::Derived; MAX_VVC_CHROMA_TUS];
    let mut prediction_scratch = VvcDcPredictionScratch::default();
    let mut predicted_luma = Vec::new();
    let mut predicted_cb = Vec::new();
    let mut predicted_cr = Vec::new();
    let mut transform_scratch = VvcInverseTransformScratch::default();
    let mut reconstructed_residual = Vec::new();
    let mut luma_residuals = Vec::new();
    let mut candidate_luma_prediction = Vec::new();
    let mut candidate_luma_residuals = Vec::new();
    let mut luma_rd_cache = VvcLumaModeRdCache::new();
    let mut cb_residuals = Vec::new();
    let mut cr_residuals = Vec::new();
    let mut candidate_cb_prediction = Vec::new();
    let mut candidate_cr_prediction = Vec::new();
    let mut candidate_cb_residuals = Vec::new();
    let mut candidate_cr_residuals = Vec::new();
    let mut chroma_rd_cache = VvcChromaModeRdCache::new();
    #[cfg(feature = "vvc-stats")]
    let mut intra_search_stats = VvcIntraSearchStats::default();
    #[cfg(not(feature = "vvc-stats"))]
    let mut intra_search_stats = VvcIntraSearchStats;
    #[cfg(feature = "vvc-stats")]
    let mut residual_energy_stats = VvcResidualEnergyStats::default();
    #[cfg(feature = "vvc-stats")]
    let mut tu_trace_sink = vvc_tu_trace_sink();

    let score_metric = policy.score_metric();
    let chroma_syntax_tie_breaker = policy.chroma_syntax_tie_breaker();
    let luma_max_leaf_size = policy.luma_max_leaf_size();
    let luma_ts_quant = transform_skip_quant_tables.luma();
    let chroma_ts_quant = transform_skip_quant_tables.chroma();
    let ctu_shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: region.geometry.coded_width() as u16,
        visible_height: region.geometry.coded_height() as u16,
        chroma_sampling: source_frame.format.chroma_sampling,
        dual_tree_intra: true,
    };

    let mut luma_tu_count = 0usize;
    let luma_nodes = vvc_luma_transform_nodes(ctu_shape, luma_max_leaf_size);
    for local_node in luma_nodes.iter().copied() {
        if luma_tu_count >= MAX_VVC_LUMA_TUS {
            break;
        }
        let node = vvc_global_ctu_node(local_node, region);
        luma_rd_cache.reset(policy, node);
        let left_luma_mode = luma_mode_search_state.left_of(node);
        let above_luma_mode = luma_mode_search_state.above_of(node);
        #[cfg(feature = "vvc-stats")]
        let luma_mode_search_start = Instant::now();
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_luma_intra_block_into_with_availability(
            &mut predicted_luma,
            &mut prediction_scratch,
            VvcIntraPredictionMode::Dc,
            &frame_recon.luma,
            source_frame.geometry,
            node,
            source_frame.format.bit_depth,
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_prediction_nanos(
            VvcLumaPredictionStatsFamily::Dc,
            vvc_elapsed_nanos(prediction_start),
        );
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let dc_score = score_luma_mode_candidate(
            &mut luma_rd_cache,
            score_metric,
            VvcIntraPredictionMode::Dc,
            source_frame,
            node,
            &predicted_luma,
            left_luma_mode,
            above_luma_mode,
            &mut candidate_luma_residuals,
            &mut intra_search_stats,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_mode_score_nanos(vvc_elapsed_nanos(score_start));
        let mut best_luma_mode = VvcIntraPredictionMode::Dc;
        let mut best_luma_score = dc_score;
        let mut luma_candidate_costs = VvcLumaIntraCandidateCosts::new(dc_score);
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_dc();
        if policy.luma_planar_candidate_allowed(node) {
            #[cfg(feature = "vvc-stats")]
            let prediction_start = Instant::now();
            predict_vvc_luma_intra_block_into_with_availability(
                &mut candidate_luma_prediction,
                &mut prediction_scratch,
                VvcIntraPredictionMode::Planar,
                &frame_recon.luma,
                source_frame.geometry,
                node,
                source_frame.format.bit_depth,
                Some(frame_recon.luma_availability()),
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_luma_prediction_nanos(
                VvcLumaPredictionStatsFamily::Planar,
                vvc_elapsed_nanos(prediction_start),
            );
            #[cfg(feature = "vvc-stats")]
            let score_start = Instant::now();
            let candidate_score = score_luma_mode_candidate(
                &mut luma_rd_cache,
                score_metric,
                VvcIntraPredictionMode::Planar,
                source_frame,
                node,
                &candidate_luma_prediction,
                left_luma_mode,
                above_luma_mode,
                &mut candidate_luma_residuals,
                &mut intra_search_stats,
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_luma_mode_score_nanos(vvc_elapsed_nanos(score_start));
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_luma_planar();
            luma_candidate_costs = luma_candidate_costs
                .with_candidate(VvcIntraPredictionMode::Planar, Some(candidate_score));
            if candidate_score < best_luma_score {
                best_luma_score = candidate_score;
                best_luma_mode = VvcIntraPredictionMode::Planar;
                std::mem::swap(&mut predicted_luma, &mut candidate_luma_prediction);
            }
        }
        if policy.luma_directional_candidate_allowed(node)
            && !vvc_luma_exact_min_syntax_mode_search_done(best_luma_score)
        {
            let mut luma_directional_candidates = vvc_luma_directional_search_candidates(
                policy,
                source_frame,
                &luma_mode_search_state,
                node,
            );
            for mode in luma_directional_candidates.iter() {
                #[cfg(feature = "vvc-stats")]
                let prediction_start = Instant::now();
                predict_vvc_luma_intra_block_into_with_availability(
                    &mut candidate_luma_prediction,
                    &mut prediction_scratch,
                    mode,
                    &frame_recon.luma,
                    source_frame.geometry,
                    node,
                    source_frame.format.bit_depth,
                    Some(frame_recon.luma_availability()),
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_luma_prediction_nanos(
                    VvcLumaPredictionStatsFamily::Directional,
                    vvc_elapsed_nanos(prediction_start),
                );
                #[cfg(feature = "vvc-stats")]
                let score_start = Instant::now();
                let candidate_score = score_luma_mode_candidate(
                    &mut luma_rd_cache,
                    score_metric,
                    mode,
                    source_frame,
                    node,
                    &candidate_luma_prediction,
                    left_luma_mode,
                    above_luma_mode,
                    &mut candidate_luma_residuals,
                    &mut intra_search_stats,
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_luma_mode_score_nanos(vvc_elapsed_nanos(score_start));
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_luma_directional_coarse();
                luma_candidate_costs =
                    luma_candidate_costs.with_candidate(mode, Some(candidate_score));
                if candidate_score < best_luma_score {
                    best_luma_score = candidate_score;
                    best_luma_mode = mode;
                    std::mem::swap(&mut predicted_luma, &mut candidate_luma_prediction);
                }
                if vvc_luma_exact_min_syntax_mode_search_done(best_luma_score) {
                    break;
                }
            }
            if (2..=66).contains(&best_luma_mode.luma_mode_index())
                && !vvc_luma_exact_min_syntax_mode_search_done(best_luma_score)
            {
                let refinement_start = luma_directional_candidates.count();
                luma_directional_candidates
                    .add_refinement(policy, best_luma_mode.luma_mode_index());
                for mode in luma_directional_candidates.iter_from(refinement_start) {
                    #[cfg(feature = "vvc-stats")]
                    let prediction_start = Instant::now();
                    predict_vvc_luma_intra_block_into_with_availability(
                        &mut candidate_luma_prediction,
                        &mut prediction_scratch,
                        mode,
                        &frame_recon.luma,
                        source_frame.geometry,
                        node,
                        source_frame.format.bit_depth,
                        Some(frame_recon.luma_availability()),
                    );
                    #[cfg(feature = "vvc-stats")]
                    intra_search_stats.add_luma_prediction_nanos(
                        VvcLumaPredictionStatsFamily::Directional,
                        vvc_elapsed_nanos(prediction_start),
                    );
                    #[cfg(feature = "vvc-stats")]
                    let score_start = Instant::now();
                    let candidate_score = score_luma_mode_candidate(
                        &mut luma_rd_cache,
                        score_metric,
                        mode,
                        source_frame,
                        node,
                        &candidate_luma_prediction,
                        left_luma_mode,
                        above_luma_mode,
                        &mut candidate_luma_residuals,
                        &mut intra_search_stats,
                    );
                    #[cfg(feature = "vvc-stats")]
                    intra_search_stats.add_luma_mode_score_nanos(vvc_elapsed_nanos(score_start));
                    #[cfg(feature = "vvc-stats")]
                    intra_search_stats.add_luma_directional_refinement();
                    luma_candidate_costs =
                        luma_candidate_costs.with_candidate(mode, Some(candidate_score));
                    if candidate_score < best_luma_score {
                        best_luma_score = candidate_score;
                        best_luma_mode = mode;
                        std::mem::swap(&mut predicted_luma, &mut candidate_luma_prediction);
                    }
                    if vvc_luma_exact_min_syntax_mode_search_done(best_luma_score) {
                        break;
                    }
                }
            }
        }
        let raw_luma_mode = policy.select_luma_intra_mode(node, luma_candidate_costs);
        debug_assert_eq!(raw_luma_mode, best_luma_mode);
        let _best_luma_score = best_luma_score;
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_luma_mode_search_nanos(luma_mode_search_start.elapsed().as_nanos() as u64);
        if let Some(cached) = luma_rd_cache.get(raw_luma_mode) {
            luma_residuals.clear();
            luma_residuals.extend_from_slice(&cached.residuals);
        } else {
            #[cfg(feature = "vvc-stats")]
            let residual_start = Instant::now();
            residual_luma_tu_at_into(
                &mut luma_residuals,
                source_frame,
                usize::from(node.x),
                usize::from(node.y),
                usize::from(node.width),
                usize::from(node.height),
                &predicted_luma,
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        }
        #[cfg(feature = "vvc-stats")]
        let luma_rd_start = Instant::now();
        let selected_luma_mode = select_vvc_luma_mode_with_rd_refinement(
            policy,
            node,
            raw_luma_mode,
            luma_candidate_costs,
            &luma_rd_cache,
            &mut intra_search_stats,
            left_luma_mode,
            above_luma_mode,
            source_frame,
            frame_recon,
            luma_qp,
            luma_ts_quant,
            &mut prediction_scratch,
            &mut predicted_luma,
            &mut luma_residuals,
            &mut candidate_luma_prediction,
            &mut candidate_luma_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_rd_refinement_nanos(luma_rd_start.elapsed().as_nanos() as u64);
        #[cfg(feature = "vvc-stats")]
        if selected_luma_mode.residual.is_some() {
            intra_search_stats.add_luma_rd_refinement_attempt();
            if selected_luma_mode.mode != raw_luma_mode {
                intra_search_stats.add_luma_rd_refinement_switch();
            }
        }
        let mut luma_mode = selected_luma_mode.mode;
        let mut luma_coding_decision = policy.select_luma_tu_coding_decision(node, luma_mode);
        #[cfg(feature = "vvc-stats")]
        let luma_mrl_start = Instant::now();
        let selected_luma_mrl = select_vvc_luma_mrl_prediction(
            policy,
            luma_coding_decision.residual_coding,
            luma_coding_decision.mts_index,
            node,
            luma_mode,
            left_luma_mode,
            above_luma_mode,
            luma_qp,
            luma_ts_quant,
            selected_luma_mode.residual,
            &mut intra_search_stats,
            frame_recon,
            source_frame,
            &mut prediction_scratch,
            &mut predicted_luma,
            &mut luma_residuals,
            &mut candidate_luma_prediction,
            &mut candidate_luma_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_mrl_nanos(luma_mrl_start.elapsed().as_nanos() as u64);
        luma_coding_decision.mrl_index = selected_luma_mrl.mrl_index;
        let mut selected_luma_residual = selected_luma_mrl.residual;
        #[cfg(feature = "vvc-stats")]
        let luma_bdpcm_start = Instant::now();
        if let Some(selected_bdpcm) = select_vvc_luma_bdpcm_prediction(
            policy,
            node,
            luma_mode,
            luma_coding_decision,
            left_luma_mode,
            above_luma_mode,
            luma_qp,
            luma_ts_quant,
            selected_luma_residual,
            &mut intra_search_stats,
            frame_recon,
            source_frame,
            &mut prediction_scratch,
            &mut predicted_luma,
            &mut luma_residuals,
            &mut candidate_luma_prediction,
            &mut candidate_luma_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        ) {
            luma_mode = selected_bdpcm.mode;
            luma_coding_decision = selected_bdpcm.coding_decision;
            selected_luma_residual = Some(selected_bdpcm.residual);
        }
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_bdpcm_nanos(luma_bdpcm_start.elapsed().as_nanos() as u64);
        luma_tu_intra_modes[luma_tu_count] = luma_mode;
        luma_mode_search_state.mark_node(node, luma_mode);
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats.add_luma_residuals(
            &luma_residuals,
            usize::from(node.width),
            usize::from(node.height),
        );
        #[cfg(feature = "vvc-stats")]
        let luma_finalize_start = Instant::now();
        let luma_tu = finalize_vvc_luma_tu(
            luma_coding_decision,
            source_frame,
            frame_recon,
            node,
            &predicted_luma,
            &luma_residuals,
            luma_qp,
            luma_ts_quant,
            selected_luma_residual,
            &mut intra_search_stats,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_finalize_nanos(luma_finalize_start.elapsed().as_nanos() as u64);
        luma_tu_remainders[luma_tu_count] = luma_tu.abs_remainder;
        luma_tu_negative[luma_tu_count] = luma_tu.negative;
        luma_tu_dc_levels[luma_tu_count] = luma_tu.dc_level;
        luma_tu_ac_levels[luma_tu_count] = luma_tu.ac_levels;
        luma_tu_has_ac[luma_tu_count] = luma_tu.has_ac;
        luma_tu_transform_skip[luma_tu_count] = luma_tu.transform_skip;
        luma_tu_bdpcm_modes[luma_tu_count] = luma_tu.bdpcm_mode;
        luma_tu_mrl_index[luma_tu_count] = luma_tu.mrl_index;
        luma_tu_mts_index[luma_tu_count] = luma_tu.mts_index;
        #[cfg(feature = "vvc-stats")]
        write_vvc_luma_tu_trace(
            tu_trace_sink.as_mut(),
            region,
            luma_tu_count,
            node,
            luma_mode,
            luma_tu,
            &predicted_luma,
            &luma_residuals,
        );
        luma_tu_count += 1;
    }

    let mut chroma_tu_count = 0usize;
    for local_node in vvc_chroma_transform_nodes(ctu_shape) {
        if chroma_tu_count >= MAX_VVC_CHROMA_TUS {
            break;
        }
        let node = vvc_global_ctu_node(local_node, region);
        chroma_rd_cache.reset(policy, node);
        let subsample_x = chroma_subsample_x(source_frame.format.chroma_sampling);
        let subsample_y = chroma_subsample_y(source_frame.format.chroma_sampling);
        let chroma_x = usize::from(node.x) / subsample_x;
        let chroma_y = usize::from(node.y) / subsample_y;
        let chroma_width = usize::from(node.width) / subsample_x;
        let chroma_height = usize::from(node.height) / subsample_y;
        let co_located_luma_mode = luma_mode_search_state.co_located_mode_for_chroma_node(node);
        let cclm_syntax_enabled = vvc_chroma_cclm_node_allowed(node);
        let initial_chroma_mode = VvcChromaIntraPredictionMode::Derived;
        #[cfg(feature = "vvc-stats")]
        let chroma_mode_search_start = Instant::now();
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_chroma_mode_pair_blocks_into_with_availability(
            &mut predicted_cb,
            &mut predicted_cr,
            &mut prediction_scratch,
            initial_chroma_mode,
            co_located_luma_mode,
            &frame_recon.cb,
            &frame_recon.cr,
            &frame_recon.luma,
            source_frame.geometry,
            node,
            source_frame.format.chroma_sampling,
            source_frame.format.bit_depth,
            Some(frame_recon.cb_availability()),
            Some(frame_recon.cr_availability()),
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_prediction_nanos(
            VvcChromaPredictionStatsFamily::Derived,
            vvc_elapsed_nanos(prediction_start),
        );
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let initial_score = score_chroma_mode_candidate(
            &mut chroma_rd_cache,
            score_metric,
            initial_chroma_mode,
            source_frame,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            &predicted_cb,
            &predicted_cr,
            cclm_syntax_enabled,
            chroma_syntax_tie_breaker,
            &mut candidate_cb_residuals,
            &mut candidate_cr_residuals,
            &mut intra_search_stats,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_mode_score_nanos(vvc_elapsed_nanos(score_start));
        let mut best_chroma_mode = initial_chroma_mode;
        let mut best_chroma_score = initial_score;
        let mut chroma_candidate_costs = VvcChromaIntraCandidateCosts::new(initial_score);
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_derived();
        if !vvc_chroma_lossy_exact_mode_search_done(chroma_syntax_tie_breaker, best_chroma_score) {
            for explicit_mode in vvc_chroma_explicit_candidates(co_located_luma_mode) {
                if !vvc_residual_chroma_explicit_candidate_allowed(explicit_mode) {
                    continue;
                }
                let chroma_mode = VvcChromaIntraPredictionMode::Explicit(explicit_mode);
                #[cfg(feature = "vvc-stats")]
                let prediction_start = Instant::now();
                predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                    &mut candidate_cb_prediction,
                    &mut candidate_cr_prediction,
                    &mut prediction_scratch,
                    chroma_mode,
                    co_located_luma_mode,
                    &frame_recon.cb,
                    &frame_recon.cr,
                    &frame_recon.luma,
                    source_frame.geometry,
                    node,
                    source_frame.format.chroma_sampling,
                    source_frame.format.bit_depth,
                    Some(frame_recon.cb_availability()),
                    Some(frame_recon.cr_availability()),
                    Some(frame_recon.luma_availability()),
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_prediction_nanos(
                    VvcChromaPredictionStatsFamily::Explicit,
                    vvc_elapsed_nanos(prediction_start),
                );
                #[cfg(feature = "vvc-stats")]
                let score_start = Instant::now();
                let candidate_score = score_chroma_mode_candidate(
                    &mut chroma_rd_cache,
                    score_metric,
                    chroma_mode,
                    source_frame,
                    chroma_x,
                    chroma_y,
                    chroma_width,
                    chroma_height,
                    &candidate_cb_prediction,
                    &candidate_cr_prediction,
                    cclm_syntax_enabled,
                    chroma_syntax_tie_breaker,
                    &mut candidate_cb_residuals,
                    &mut candidate_cr_residuals,
                    &mut intra_search_stats,
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_mode_score_nanos(vvc_elapsed_nanos(score_start));
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_explicit();
                chroma_candidate_costs =
                    chroma_candidate_costs.with_candidate(chroma_mode, Some(candidate_score));
                if candidate_score < best_chroma_score {
                    best_chroma_score = candidate_score;
                    best_chroma_mode = chroma_mode;
                    std::mem::swap(&mut predicted_cb, &mut candidate_cb_prediction);
                    std::mem::swap(&mut predicted_cr, &mut candidate_cr_prediction);
                }
                if vvc_chroma_lossy_exact_mode_search_done(
                    chroma_syntax_tie_breaker,
                    best_chroma_score,
                ) {
                    break;
                }
            }
        }
        if policy.chroma_cclm_candidate_allowed(node, source_frame.geometry)
            && !vvc_chroma_lossy_exact_mode_search_done(
                chroma_syntax_tie_breaker,
                best_chroma_score,
            )
        {
            for cclm_mode in [
                VvcChromaCclmMode::Linear,
                VvcChromaCclmMode::MdlmLeft,
                VvcChromaCclmMode::MdlmTop,
            ] {
                let chroma_mode = VvcChromaIntraPredictionMode::Cclm(cclm_mode);
                #[cfg(feature = "vvc-stats")]
                let prediction_start = Instant::now();
                predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                    &mut candidate_cb_prediction,
                    &mut candidate_cr_prediction,
                    &mut prediction_scratch,
                    chroma_mode,
                    co_located_luma_mode,
                    &frame_recon.cb,
                    &frame_recon.cr,
                    &frame_recon.luma,
                    source_frame.geometry,
                    node,
                    source_frame.format.chroma_sampling,
                    source_frame.format.bit_depth,
                    Some(frame_recon.cb_availability()),
                    Some(frame_recon.cr_availability()),
                    Some(frame_recon.luma_availability()),
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_prediction_nanos(
                    VvcChromaPredictionStatsFamily::Cclm,
                    vvc_elapsed_nanos(prediction_start),
                );
                #[cfg(feature = "vvc-stats")]
                let score_start = Instant::now();
                let candidate_score = score_chroma_mode_candidate(
                    &mut chroma_rd_cache,
                    score_metric,
                    chroma_mode,
                    source_frame,
                    chroma_x,
                    chroma_y,
                    chroma_width,
                    chroma_height,
                    &candidate_cb_prediction,
                    &candidate_cr_prediction,
                    cclm_syntax_enabled,
                    chroma_syntax_tie_breaker,
                    &mut candidate_cb_residuals,
                    &mut candidate_cr_residuals,
                    &mut intra_search_stats,
                );
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_mode_score_nanos(vvc_elapsed_nanos(score_start));
                #[cfg(feature = "vvc-stats")]
                intra_search_stats.add_chroma_cclm();
                chroma_candidate_costs =
                    chroma_candidate_costs.with_candidate(chroma_mode, Some(candidate_score));
                if candidate_score < best_chroma_score {
                    best_chroma_score = candidate_score;
                    best_chroma_mode = chroma_mode;
                    std::mem::swap(&mut predicted_cb, &mut candidate_cb_prediction);
                    std::mem::swap(&mut predicted_cr, &mut candidate_cr_prediction);
                }
            }
        }
        let raw_chroma_mode = policy.select_chroma_intra_mode(node, chroma_candidate_costs);
        debug_assert_eq!(raw_chroma_mode, best_chroma_mode);
        let _best_chroma_score = best_chroma_score;
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_chroma_mode_search_nanos(chroma_mode_search_start.elapsed().as_nanos() as u64);
        if let Some(cached) = chroma_rd_cache.get(raw_chroma_mode) {
            cb_residuals.clear();
            cb_residuals.extend_from_slice(&cached.cb_residuals);
            cr_residuals.clear();
            cr_residuals.extend_from_slice(&cached.cr_residuals);
        } else {
            #[cfg(feature = "vvc-stats")]
            let residual_start = Instant::now();
            residual_chroma_tu_at_into(
                &mut cb_residuals,
                &source_frame.cb,
                source_frame.geometry,
                source_frame.format,
                chroma_x,
                chroma_y,
                chroma_width,
                chroma_height,
                &predicted_cb,
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
            #[cfg(feature = "vvc-stats")]
            let residual_start = Instant::now();
            residual_chroma_tu_at_into(
                &mut cr_residuals,
                &source_frame.cr,
                source_frame.geometry,
                source_frame.format,
                chroma_x,
                chroma_y,
                chroma_width,
                chroma_height,
                &predicted_cr,
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        }
        #[cfg(feature = "vvc-stats")]
        let chroma_rd_start = Instant::now();
        let selected_chroma_mode = select_vvc_chroma_mode_with_rd_refinement(
            policy,
            node,
            raw_chroma_mode,
            chroma_candidate_costs,
            &chroma_rd_cache,
            &mut intra_search_stats,
            co_located_luma_mode,
            cclm_syntax_enabled,
            source_frame,
            frame_recon,
            chroma_width,
            chroma_height,
            chroma_qp,
            chroma_ts_quant,
            &mut prediction_scratch,
            &mut predicted_cb,
            &mut predicted_cr,
            &mut cb_residuals,
            &mut cr_residuals,
            &mut candidate_cb_prediction,
            &mut candidate_cr_prediction,
            &mut candidate_cb_residuals,
            &mut candidate_cr_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_chroma_rd_refinement_nanos(chroma_rd_start.elapsed().as_nanos() as u64);
        #[cfg(feature = "vvc-stats")]
        if selected_chroma_mode.residual.is_some() {
            intra_search_stats.add_chroma_rd_refinement_attempt();
            if selected_chroma_mode.mode != raw_chroma_mode {
                intra_search_stats.add_chroma_rd_refinement_switch();
            }
        }
        let mut chroma_mode = selected_chroma_mode.mode;
        let mut selected_chroma_residual = selected_chroma_mode.residual;
        #[cfg(feature = "vvc-stats")]
        let chroma_bdpcm_start = Instant::now();
        if let Some(selected_bdpcm) = select_vvc_chroma_bdpcm_prediction(
            policy,
            node,
            chroma_mode,
            cclm_syntax_enabled,
            source_frame,
            frame_recon,
            chroma_width,
            chroma_height,
            chroma_qp,
            chroma_ts_quant,
            selected_chroma_residual,
            &mut intra_search_stats,
            &mut prediction_scratch,
            &mut predicted_cb,
            &mut predicted_cr,
            &mut cb_residuals,
            &mut cr_residuals,
            &mut candidate_cb_prediction,
            &mut candidate_cr_prediction,
            &mut candidate_cb_residuals,
            &mut candidate_cr_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        ) {
            chroma_mode = selected_bdpcm.mode;
            selected_chroma_residual = Some(selected_bdpcm.residual);
        }
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_bdpcm_nanos(chroma_bdpcm_start.elapsed().as_nanos() as u64);
        chroma_tu_intra_modes[chroma_tu_count] = chroma_mode;
        let chroma_coding_decision = policy.select_chroma_tu_coding_decision(node, chroma_mode);
        #[cfg(feature = "vvc-stats")]
        {
            residual_energy_stats.add_chroma_residuals(&cb_residuals, chroma_width, chroma_height);
            residual_energy_stats.add_chroma_residuals(&cr_residuals, chroma_width, chroma_height);
        }
        #[cfg(feature = "vvc-stats")]
        let chroma_finalize_start = Instant::now();
        let chroma_tu = finalize_vvc_chroma_tu(
            chroma_coding_decision,
            source_frame,
            frame_recon,
            node,
            &predicted_cb,
            &predicted_cr,
            &cb_residuals,
            &cr_residuals,
            chroma_width,
            chroma_height,
            chroma_qp,
            chroma_ts_quant,
            selected_chroma_residual,
            &mut intra_search_stats,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_chroma_finalize_nanos(chroma_finalize_start.elapsed().as_nanos() as u64);
        cb_tu_dc_levels[chroma_tu_count] = chroma_tu.cb_dc_level;
        cr_tu_dc_levels[chroma_tu_count] = chroma_tu.cr_dc_level;
        cb_tu_ac_levels[chroma_tu_count] = chroma_tu.cb_ac_levels;
        cr_tu_ac_levels[chroma_tu_count] = chroma_tu.cr_ac_levels;
        cb_tu_has_ac[chroma_tu_count] = chroma_tu.cb_has_ac;
        cr_tu_has_ac[chroma_tu_count] = chroma_tu.cr_has_ac;
        cb_tu_transform_skip[chroma_tu_count] = chroma_tu.cb_transform_skip;
        cr_tu_transform_skip[chroma_tu_count] = chroma_tu.cr_transform_skip;
        chroma_tu_bdpcm_modes[chroma_tu_count] = chroma_tu.bdpcm_mode;
        #[cfg(feature = "vvc-stats")]
        write_vvc_chroma_tu_trace(
            tu_trace_sink.as_mut(),
            region,
            chroma_tu_count,
            node,
            chroma_mode,
            co_located_luma_mode,
            chroma_tu,
            chroma_width,
            chroma_height,
            &predicted_cb,
            &predicted_cr,
            &cb_residuals,
            &cr_residuals,
        );
        chroma_tu_count += 1;
    }

    let color = source_frame.sampled_color();
    let cb_rem = quantize_vvc_chroma_sample(vvc_downshift_sample_to_u8(
        color.u,
        source_frame.format.bit_depth,
    ));
    let cr_rem = quantize_vvc_chroma_sample(vvc_downshift_sample_to_u8(
        color.v,
        source_frame.format.bit_depth,
    ));
    VvcQuantizedColor {
        y: vvc_downshift_sample_to_u8(color.y, source_frame.format.bit_depth),
        u: finalized_vvc_chroma_sample(
            cb_tu_transform_skip.first().copied().unwrap_or(false),
            color.u,
            cb_rem,
            source_frame.format.bit_depth,
        ),
        v: finalized_vvc_chroma_sample(
            cr_tu_transform_skip.first().copied().unwrap_or(false),
            color.v,
            cr_rem,
            source_frame.format.bit_depth,
        ),
        luma_tu_intra_modes,
        luma_tu_remainders,
        luma_tu_negative,
        luma_tu_dc_levels,
        luma_tu_ac_levels,
        luma_tu_has_ac,
        luma_tu_transform_skip,
        luma_tu_bdpcm_modes,
        luma_tu_mrl_index,
        luma_tu_mts_index,
        luma_tu_count,
        chroma_tu_count,
        chroma_tu_intra_modes,
        cb_tu_dc_levels,
        cr_tu_dc_levels,
        cb_tu_ac_levels,
        cr_tu_ac_levels,
        cb_tu_has_ac,
        cr_tu_has_ac,
        cb_tu_transform_skip,
        cr_tu_transform_skip,
        chroma_tu_bdpcm_modes,
        cb_rem,
        cr_rem,
        #[cfg(feature = "vvc-stats")]
        intra_search_stats,
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats,
    }
}

#[cfg(feature = "vvc-stats")]
fn vvc_tu_trace_sink() -> Option<JsonlInstrumentationSink> {
    match JsonlInstrumentationSink::append_from_env(VVC_TU_TRACE_ENV) {
        Ok(sink) => sink,
        Err(err) => {
            eprintln!("failed to open {VVC_TU_TRACE_ENV}: {err}");
            None
        }
    }
}

#[cfg(feature = "vvc-stats")]
fn write_vvc_luma_tu_trace(
    sink: Option<&mut JsonlInstrumentationSink>,
    region: VvcCtuRegion,
    tu_index: usize,
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
    tu: VvcFinalizedLumaTu,
    predicted: &[VvcSample],
    residuals: &[i16],
) {
    let Some(sink) = sink else {
        return;
    };
    let nonzero_ac = tu.ac_levels.iter().filter(|level| **level != 0).count();
    let line = format!(
        "{{\"event\":\"vvc_tu\",\"component\":\"luma\",\"slice\":{},\"tu\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"mode\":\"{:?}\",\"mode_index\":{},\"transform_skip\":{},\"bdpcm_mode\":\"{:?}\",\"mrl_index\":{},\"mts_index\":{},\"dc\":{},\"has_ac\":{},\"nonzero_ac\":{},\"predicted\":{},\"residuals\":{}}}",
        region.slice_address,
        tu_index,
        node.x,
        node.y,
        node.width,
        node.height,
        mode,
        mode.luma_mode_index(),
        tu.transform_skip,
        tu.bdpcm_mode,
        tu.mrl_index,
        tu.mts_index,
        tu.dc_level,
        tu.has_ac,
        nonzero_ac,
        json_u16_slice(predicted),
        json_i16_slice(residuals),
    );
    if let Err(err) = sink.write_json_line(&line).and_then(|()| sink.flush()) {
        eprintln!("failed to write {VVC_TU_TRACE_ENV}: {err}");
    }
}

#[cfg(feature = "vvc-stats")]
fn write_vvc_chroma_tu_trace(
    sink: Option<&mut JsonlInstrumentationSink>,
    region: VvcCtuRegion,
    tu_index: usize,
    node: VvcCodingTreeNode,
    mode: VvcChromaIntraPredictionMode,
    co_located_luma_mode: VvcIntraPredictionMode,
    tu: VvcFinalizedChromaTu,
    chroma_width: usize,
    chroma_height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
    cb_residuals: &[i16],
    cr_residuals: &[i16],
) {
    let Some(sink) = sink else {
        return;
    };
    let cb_nonzero_ac = tu.cb_ac_levels.iter().filter(|level| **level != 0).count();
    let cr_nonzero_ac = tu.cr_ac_levels.iter().filter(|level| **level != 0).count();
    let chroma_x = usize::from(node.x);
    let chroma_y = usize::from(node.y);
    let line = format!(
        "{{\"event\":\"vvc_tu\",\"component\":\"chroma\",\"slice\":{},\"tu\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"chroma_w\":{},\"chroma_h\":{},\"mode\":\"{:?}\",\"co_located_luma_mode\":\"{:?}\",\"co_located_luma_mode_index\":{},\"cb_transform_skip\":{},\"cr_transform_skip\":{},\"bdpcm_mode\":\"{:?}\",\"cb_dc\":{},\"cr_dc\":{},\"cb_has_ac\":{},\"cr_has_ac\":{},\"cb_nonzero_ac\":{},\"cr_nonzero_ac\":{},\"predicted_cb\":{},\"predicted_cr\":{},\"cb_residuals\":{},\"cr_residuals\":{}}}",
        region.slice_address,
        tu_index,
        chroma_x,
        chroma_y,
        node.width,
        node.height,
        chroma_width,
        chroma_height,
        mode,
        co_located_luma_mode,
        co_located_luma_mode.luma_mode_index(),
        tu.cb_transform_skip,
        tu.cr_transform_skip,
        tu.bdpcm_mode,
        tu.cb_dc_level,
        tu.cr_dc_level,
        tu.cb_has_ac,
        tu.cr_has_ac,
        cb_nonzero_ac,
        cr_nonzero_ac,
        json_u16_slice(predicted_cb),
        json_u16_slice(predicted_cr),
        json_i16_slice(cb_residuals),
        json_i16_slice(cr_residuals),
    );
    if let Err(err) = sink.write_json_line(&line).and_then(|()| sink.flush()) {
        eprintln!("failed to write {VVC_TU_TRACE_ENV}: {err}");
    }
}

struct VvcLumaModeRdCache {
    candidates: Vec<VvcCachedLumaModeRdCandidate>,
    count: usize,
    limit: usize,
}

impl VvcLumaModeRdCache {
    fn new() -> Self {
        let mut candidates = Vec::with_capacity(VVC_LOSSY_LUMA_RD_WINNER_CANDIDATES);
        candidates.resize_with(
            VVC_LOSSY_LUMA_RD_WINNER_CANDIDATES,
            VvcCachedLumaModeRdCandidate::new,
        );
        Self {
            candidates,
            count: 0,
            limit: 0,
        }
    }

    fn reset(&mut self, policy: VvcResidualCodingPolicy, node: VvcCodingTreeNode) {
        self.count = 0;
        self.limit = if policy.residual_mode() == VvcResidualCodingMode::Lossy
            && [4, 8, 16, 32].contains(&node.width)
            && [4, 8, 16, 32].contains(&node.height)
        {
            vvc_luma_mode_rd_shortlist_limit(policy).min(self.candidates.len())
        } else {
            0
        };
    }

    fn materializes_mode_search_residuals(&self) -> bool {
        self.limit > 0
    }

    fn consider(&mut self, mode: VvcIntraPredictionMode, score: u64, residuals: &[i16]) {
        if self.limit == 0 {
            return;
        }
        if let Some(existing) = self.candidates[..self.count]
            .iter()
            .position(|candidate| candidate.mode.luma_mode_index() == mode.luma_mode_index())
        {
            if score < self.candidates[existing].score {
                self.candidates[existing].replace(mode, score, residuals);
                self.sort();
            }
            return;
        }
        if self.count < self.limit {
            self.candidates[self.count].replace(mode, score, residuals);
            self.count += 1;
            self.sort();
            return;
        }
        let worst = self.count - 1;
        if score < self.candidates[worst].score {
            self.candidates[worst].replace(mode, score, residuals);
            self.sort();
        }
    }

    fn get(&self, mode: VvcIntraPredictionMode) -> Option<&VvcCachedLumaModeRdCandidate> {
        self.candidates[..self.count]
            .iter()
            .find(|candidate| candidate.mode.luma_mode_index() == mode.luma_mode_index())
    }

    fn sort(&mut self) {
        self.candidates[..self.count].sort_by_key(|candidate| candidate.score);
    }
}

struct VvcCachedLumaModeRdCandidate {
    mode: VvcIntraPredictionMode,
    score: u64,
    residuals: Vec<i16>,
}

impl VvcCachedLumaModeRdCandidate {
    fn new() -> Self {
        Self {
            mode: VvcIntraPredictionMode::Dc,
            score: u64::MAX,
            residuals: Vec::new(),
        }
    }

    fn replace(&mut self, mode: VvcIntraPredictionMode, score: u64, residuals: &[i16]) {
        self.mode = mode;
        self.score = score;
        self.residuals.clear();
        self.residuals.extend_from_slice(residuals);
    }
}

struct VvcChromaModeRdCache {
    candidates: Vec<VvcCachedChromaModeRdCandidate>,
    count: usize,
    limit: usize,
}

impl VvcChromaModeRdCache {
    fn new() -> Self {
        let mut candidates = Vec::with_capacity(VVC_LOSSY_CHROMA_RD_WINNER_CANDIDATES);
        candidates.resize_with(
            VVC_LOSSY_CHROMA_RD_WINNER_CANDIDATES,
            VvcCachedChromaModeRdCandidate::new,
        );
        Self {
            candidates,
            count: 0,
            limit: 0,
        }
    }

    fn reset(&mut self, policy: VvcResidualCodingPolicy, node: VvcCodingTreeNode) {
        self.count = 0;
        self.limit = if policy.residual_mode() == VvcResidualCodingMode::Lossy
            && [4, 8, 16, 32].contains(&node.width)
            && [4, 8, 16, 32].contains(&node.height)
        {
            vvc_chroma_mode_rd_shortlist_limit(policy).min(self.candidates.len())
        } else {
            0
        };
    }

    fn materializes_mode_search_residuals(&self) -> bool {
        self.limit > 0
    }

    fn consider(
        &mut self,
        mode: VvcChromaIntraPredictionMode,
        score: u64,
        cb_residuals: &[i16],
        cr_residuals: &[i16],
    ) {
        if self.limit == 0 {
            return;
        }
        if let Some(existing) = self.candidates[..self.count]
            .iter()
            .position(|candidate| candidate.mode == mode)
        {
            if score < self.candidates[existing].score {
                self.candidates[existing].replace(mode, score, cb_residuals, cr_residuals);
                self.sort();
            }
            return;
        }
        if self.count < self.limit {
            self.candidates[self.count].replace(mode, score, cb_residuals, cr_residuals);
            self.count += 1;
            self.sort();
            return;
        }
        let worst = self.count - 1;
        if score < self.candidates[worst].score {
            self.candidates[worst].replace(mode, score, cb_residuals, cr_residuals);
            self.sort();
        }
    }

    fn get(&self, mode: VvcChromaIntraPredictionMode) -> Option<&VvcCachedChromaModeRdCandidate> {
        self.candidates[..self.count]
            .iter()
            .find(|candidate| candidate.mode == mode)
    }

    fn sort(&mut self) {
        self.candidates[..self.count].sort_by_key(|candidate| candidate.score);
    }
}

struct VvcCachedChromaModeRdCandidate {
    mode: VvcChromaIntraPredictionMode,
    score: u64,
    cb_residuals: Vec<i16>,
    cr_residuals: Vec<i16>,
}

impl VvcCachedChromaModeRdCandidate {
    fn new() -> Self {
        Self {
            mode: VvcChromaIntraPredictionMode::Derived,
            score: u64::MAX,
            cb_residuals: Vec::new(),
            cr_residuals: Vec::new(),
        }
    }

    fn replace(
        &mut self,
        mode: VvcChromaIntraPredictionMode,
        score: u64,
        cb_residuals: &[i16],
        cr_residuals: &[i16],
    ) {
        self.mode = mode;
        self.score = score;
        self.cb_residuals.clear();
        self.cb_residuals.extend_from_slice(cb_residuals);
        self.cr_residuals.clear();
        self.cr_residuals.extend_from_slice(cr_residuals);
    }
}

fn score_luma_mode_candidate(
    cache: &mut VvcLumaModeRdCache,
    metric: VvcResidualScoreMetric,
    mode: VvcIntraPredictionMode,
    source_frame: &VvcSampledFrame,
    node: VvcCodingTreeNode,
    predicted: &[VvcSample],
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    residuals: &mut Vec<i16>,
    stats: &mut VvcIntraSearchStats,
) -> u64 {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    if cache.materializes_mode_search_residuals() {
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_luma_tu_at_into(
            residuals,
            source_frame,
            usize::from(node.x),
            usize::from(node.y),
            usize::from(node.width),
            usize::from(node.height),
            predicted,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        let score = luma_residual_mode_selection_score(metric, residuals, left, above, mode);
        cache.consider(mode, score, residuals);
        score
    } else {
        luma_prediction_mode_selection_score(
            metric,
            source_frame,
            node,
            predicted,
            left,
            above,
            mode,
        )
    }
}

fn score_chroma_mode_candidate(
    cache: &mut VvcChromaModeRdCache,
    metric: VvcResidualScoreMetric,
    mode: VvcChromaIntraPredictionMode,
    source_frame: &VvcSampledFrame,
    chroma_x: usize,
    chroma_y: usize,
    chroma_width: usize,
    chroma_height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
    cclm_enabled: bool,
    syntax_tie_breaker_enabled: bool,
    cb_residuals: &mut Vec<i16>,
    cr_residuals: &mut Vec<i16>,
    stats: &mut VvcIntraSearchStats,
) -> u64 {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    if cache.materializes_mode_search_residuals() {
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            cb_residuals,
            &source_frame.cb,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            predicted_cb,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            cr_residuals,
            &source_frame.cr,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            predicted_cr,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        let score = chroma_residual_mode_selection_score(
            metric,
            cb_residuals,
            cr_residuals,
            mode,
            cclm_enabled,
            syntax_tie_breaker_enabled,
        );
        cache.consider(mode, score, cb_residuals, cr_residuals);
        score
    } else {
        chroma_prediction_mode_selection_score(
            metric,
            source_frame,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            predicted_cb,
            predicted_cr,
            mode,
            cclm_enabled,
            syntax_tie_breaker_enabled,
        )
    }
}

fn select_vvc_luma_mode_with_rd_refinement(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    raw_mode: VvcIntraPredictionMode,
    candidate_costs: VvcLumaIntraCandidateCosts,
    rd_cache: &VvcLumaModeRdCache,
    stats: &mut VvcIntraSearchStats,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    source_frame: &VvcSampledFrame,
    frame_recon: &VvcReconstructionFrame,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    prediction_scratch: &mut VvcDcPredictionScratch,
    selected_prediction: &mut Vec<VvcSample>,
    selected_residuals: &mut Vec<i16>,
    candidate_prediction: &mut Vec<VvcSample>,
    candidate_residuals: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcSelectedLumaMode {
    let raw_decision = policy.select_luma_tu_coding_decision(node, raw_mode);
    if !vvc_luma_lossy_rd_refinement_allowed(policy, node, raw_decision) {
        return VvcSelectedLumaMode {
            mode: raw_mode,
            residual: None,
        };
    }
    if vvc_luma_exact_prediction_skips_rd(selected_residuals) {
        return VvcSelectedLumaMode {
            mode: raw_mode,
            residual: None,
        };
    }

    let mut best_mode = raw_mode;
    #[cfg(feature = "vvc-stats")]
    let score_start = Instant::now();
    let mut best_candidate = score_vvc_luma_mode_rd_candidate(
        raw_decision,
        node,
        raw_mode,
        left,
        above,
        selected_residuals,
        source_frame.format.bit_depth,
        luma_qp,
        luma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
    if vvc_luma_zero_coded_residual_skips_rd(best_candidate.residual, selected_residuals.len()) {
        return VvcSelectedLumaMode {
            mode: raw_mode,
            residual: Some(best_candidate.residual),
        };
    }
    let shortlist = VvcLumaModeRdShortlist::from_candidate_costs(policy, node, candidate_costs);
    for candidate in shortlist.iter() {
        let mode = candidate.mode();
        if mode.luma_mode_index() == raw_mode.luma_mode_index() {
            continue;
        }
        let coding_decision = policy.select_luma_tu_coding_decision(node, mode);
        if !matches!(
            coding_decision.residual_coding,
            VvcTuResidualCodingMode::Transformed
        ) {
            continue;
        }
        if let Some(cached) = rd_cache.get(mode) {
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_rd_cached_candidate();
            #[cfg(feature = "vvc-stats")]
            let score_start = Instant::now();
            let rd_candidate = score_vvc_luma_mode_rd_candidate(
                coding_decision,
                node,
                mode,
                left,
                above,
                &cached.residuals,
                source_frame.format.bit_depth,
                luma_qp,
                luma_ts_quant,
                stats,
                transform_scratch,
                reconstructed_residual,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
            if rd_candidate.selects_over(best_candidate) {
                best_mode = mode;
                best_candidate = rd_candidate;
                #[cfg(feature = "vvc-stats")]
                let prediction_start = Instant::now();
                predict_vvc_luma_intra_block_into_with_availability(
                    selected_prediction,
                    prediction_scratch,
                    mode,
                    &frame_recon.luma,
                    source_frame.geometry,
                    node,
                    source_frame.format.bit_depth,
                    Some(frame_recon.luma_availability()),
                );
                #[cfg(feature = "vvc-stats")]
                stats.add_luma_rd_prediction_nanos(vvc_elapsed_nanos(prediction_start));
                selected_residuals.clear();
                selected_residuals.extend_from_slice(&cached.residuals);
            }
            continue;
        }
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_rd_generated_candidate();
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_luma_intra_block_into_with_availability(
            candidate_prediction,
            prediction_scratch,
            mode,
            &frame_recon.luma,
            source_frame.geometry,
            node,
            source_frame.format.bit_depth,
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(prediction_start);
            stats.add_luma_rd_prediction_nanos(nanos);
            stats.add_luma_prediction_nanos(vvc_luma_prediction_stats_family(mode), nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_luma_tu_at_into(
            candidate_residuals,
            source_frame,
            usize::from(node.x),
            usize::from(node.y),
            usize::from(node.width),
            usize::from(node.height),
            candidate_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(residual_start);
            stats.add_luma_rd_residual_build_nanos(nanos);
            stats.add_luma_residual_build_nanos(nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let rd_candidate = score_vvc_luma_mode_rd_candidate(
            coding_decision,
            node,
            mode,
            left,
            above,
            candidate_residuals,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
            stats,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        if rd_candidate.selects_over(best_candidate) {
            best_mode = mode;
            best_candidate = rd_candidate;
            std::mem::swap(selected_prediction, candidate_prediction);
            std::mem::swap(selected_residuals, candidate_residuals);
        }
    }

    VvcSelectedLumaMode {
        mode: best_mode,
        residual: Some(best_candidate.residual),
    }
}

fn vvc_luma_lossy_rd_refinement_allowed(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    decision: VvcLumaTuCodingDecision,
) -> bool {
    policy.residual_mode() == VvcResidualCodingMode::Lossy
        && matches!(
            decision.residual_coding,
            VvcTuResidualCodingMode::Transformed
        )
        && [4, 8, 16, 32].contains(&node.width)
        && [4, 8, 16, 32].contains(&node.height)
}

fn vvc_luma_exact_prediction_skips_rd(residuals: &[i16]) -> bool {
    residuals.iter().all(|residual| *residual == 0)
}

fn vvc_luma_zero_coded_residual_skips_rd(
    residual: VvcScoredSelectedLumaResidual,
    sample_count: usize,
) -> bool {
    residual.residual.block.dc_level == 0
        && !residual.residual.block.has_ac
        && residual.score.distortion <= sample_count as u64
}

#[derive(Debug, Clone, Copy)]
struct VvcSelectedLumaMode {
    mode: VvcIntraPredictionMode,
    residual: Option<VvcScoredSelectedLumaResidual>,
}

#[derive(Debug, Clone, Copy)]
struct VvcLumaModeRdCandidate {
    distortion: u64,
    rate_cost: u64,
    residual: VvcScoredSelectedLumaResidual,
}

impl VvcLumaModeRdCandidate {
    fn selects_over(self, best: Self) -> bool {
        vvc_quality_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

fn score_vvc_luma_mode_rd_candidate(
    coding_decision: VvcLumaTuCodingDecision,
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    residuals: &[i16],
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcLumaModeRdCandidate {
    let scored_residual = select_vvc_scored_luma_residual_block_with_mts(
        coding_decision.residual_coding,
        coding_decision.mts_index,
        residuals,
        node.width,
        node.height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        false,
        VvcLumaResidualQuantizationSearch::FastModeDecision,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    let residual = VvcScoredSelectedLumaResidual::from_scored_block(scored_residual);
    let mode_cost = u64::from(vvc_luma_intra_mode_syntax_bin_count(mode, left, above));
    VvcLumaModeRdCandidate {
        distortion: scored_residual.score.distortion,
        rate_cost: scored_residual.score.rate_cost.saturating_add(mode_cost),
        residual,
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcLumaModeRdShortlist {
    candidates: [VvcLumaIntraCandidateCost; VVC_LUMA_INTRA_CANDIDATE_CAPACITY],
    count: usize,
}

impl VvcLumaModeRdShortlist {
    fn from_candidate_costs(
        policy: VvcResidualCodingPolicy,
        node: VvcCodingTreeNode,
        costs: VvcLumaIntraCandidateCosts,
    ) -> Self {
        let mut shortlist = Self {
            candidates: [VvcLumaIntraCandidateCost::new(VvcIntraPredictionMode::Dc, u64::MAX);
                VVC_LUMA_INTRA_CANDIDATE_CAPACITY],
            count: 0,
        };
        for candidate in costs.iter() {
            shortlist.add(candidate);
        }
        shortlist.apply_policy_limit(policy, node);
        shortlist
    }

    fn add(&mut self, candidate: VvcLumaIntraCandidateCost) {
        if let Some(existing) =
            self.candidates.iter().take(self.count).position(|entry| {
                entry.mode().luma_mode_index() == candidate.mode().luma_mode_index()
            })
        {
            if candidate.score() < self.candidates[existing].score() {
                self.candidates[existing] = candidate;
                self.sort();
            }
            return;
        }
        if self.count < self.candidates.len() {
            self.candidates[self.count] = candidate;
            self.count += 1;
            self.sort();
            return;
        }
        let worst = self.count - 1;
        if candidate.score() < self.candidates[worst].score() {
            self.candidates[worst] = candidate;
            self.sort();
        }
    }

    fn sort(&mut self) {
        self.candidates[..self.count].sort_by_key(|candidate| candidate.score());
    }

    fn apply_policy_limit(&mut self, policy: VvcResidualCodingPolicy, _node: VvcCodingTreeNode) {
        self.count = self
            .count
            .min(vvc_luma_mode_rd_shortlist_limit(policy).min(self.candidates.len()));
    }

    fn iter(self) -> impl Iterator<Item = VvcLumaIntraCandidateCost> {
        self.candidates.into_iter().take(self.count)
    }
}

fn vvc_luma_mode_rd_shortlist_limit(policy: VvcResidualCodingPolicy) -> usize {
    match policy.residual_mode() {
        VvcResidualCodingMode::Lossless => VVC_LUMA_INTRA_CANDIDATE_CAPACITY,
        VvcResidualCodingMode::Lossy => VVC_LOSSY_LUMA_RD_WINNER_CANDIDATES,
    }
}

fn select_vvc_chroma_mode_with_rd_refinement(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    raw_mode: VvcChromaIntraPredictionMode,
    candidate_costs: VvcChromaIntraCandidateCosts,
    rd_cache: &VvcChromaModeRdCache,
    stats: &mut VvcIntraSearchStats,
    co_located_luma_mode: VvcIntraPredictionMode,
    cclm_syntax_enabled: bool,
    source_frame: &VvcSampledFrame,
    frame_recon: &VvcReconstructionFrame,
    chroma_width: usize,
    chroma_height: usize,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    prediction_scratch: &mut VvcDcPredictionScratch,
    selected_cb_prediction: &mut Vec<VvcSample>,
    selected_cr_prediction: &mut Vec<VvcSample>,
    selected_cb_residuals: &mut Vec<i16>,
    selected_cr_residuals: &mut Vec<i16>,
    candidate_cb_prediction: &mut Vec<VvcSample>,
    candidate_cr_prediction: &mut Vec<VvcSample>,
    candidate_cb_residuals: &mut Vec<i16>,
    candidate_cr_residuals: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcSelectedChromaMode {
    let raw_decision = policy.select_chroma_tu_coding_decision(node, raw_mode);
    if !vvc_chroma_lossy_rd_refinement_allowed(policy, node, raw_decision) {
        return VvcSelectedChromaMode {
            mode: raw_mode,
            residual: None,
        };
    }
    if vvc_chroma_exact_prediction_skips_rd(selected_cb_residuals, selected_cr_residuals) {
        return VvcSelectedChromaMode {
            mode: raw_mode,
            residual: None,
        };
    }

    let mut best_mode = raw_mode;
    #[cfg(feature = "vvc-stats")]
    let score_start = Instant::now();
    let mut best_candidate = score_vvc_chroma_mode_rd_candidate(
        raw_decision,
        raw_mode,
        cclm_syntax_enabled,
        selected_cb_residuals,
        selected_cr_residuals,
        chroma_width,
        chroma_height,
        source_frame.format.bit_depth,
        chroma_qp,
        chroma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
    let shortlist = VvcChromaModeRdShortlist::from_candidate_costs(policy, candidate_costs);
    for candidate in shortlist.iter() {
        let mode = candidate.mode();
        if mode == raw_mode {
            continue;
        }
        let coding_decision = policy.select_chroma_tu_coding_decision(node, mode);
        if !matches!(
            coding_decision.residual_coding,
            VvcTuResidualCodingMode::Transformed
        ) {
            continue;
        }
        if let Some(cached) = rd_cache.get(mode) {
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_rd_cached_candidate();
            #[cfg(feature = "vvc-stats")]
            let score_start = Instant::now();
            let rd_candidate = score_vvc_chroma_mode_rd_candidate(
                coding_decision,
                mode,
                cclm_syntax_enabled,
                &cached.cb_residuals,
                &cached.cr_residuals,
                chroma_width,
                chroma_height,
                source_frame.format.bit_depth,
                chroma_qp,
                chroma_ts_quant,
                stats,
                transform_scratch,
                reconstructed_residual,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
            if rd_candidate.selects_over(best_candidate) {
                best_mode = mode;
                best_candidate = rd_candidate;
                #[cfg(feature = "vvc-stats")]
                let prediction_start = Instant::now();
                predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                    selected_cb_prediction,
                    selected_cr_prediction,
                    prediction_scratch,
                    mode,
                    co_located_luma_mode,
                    &frame_recon.cb,
                    &frame_recon.cr,
                    &frame_recon.luma,
                    source_frame.geometry,
                    node,
                    source_frame.format.chroma_sampling,
                    source_frame.format.bit_depth,
                    Some(frame_recon.cb_availability()),
                    Some(frame_recon.cr_availability()),
                    Some(frame_recon.luma_availability()),
                );
                #[cfg(feature = "vvc-stats")]
                stats.add_chroma_rd_prediction_nanos(vvc_elapsed_nanos(prediction_start));
                selected_cb_residuals.clear();
                selected_cb_residuals.extend_from_slice(&cached.cb_residuals);
                selected_cr_residuals.clear();
                selected_cr_residuals.extend_from_slice(&cached.cr_residuals);
            }
            continue;
        }
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_rd_generated_candidate();
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_chroma_mode_pair_blocks_into_with_availability(
            candidate_cb_prediction,
            candidate_cr_prediction,
            prediction_scratch,
            mode,
            co_located_luma_mode,
            &frame_recon.cb,
            &frame_recon.cr,
            &frame_recon.luma,
            source_frame.geometry,
            node,
            source_frame.format.chroma_sampling,
            source_frame.format.bit_depth,
            Some(frame_recon.cb_availability()),
            Some(frame_recon.cr_availability()),
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(prediction_start);
            stats.add_chroma_rd_prediction_nanos(nanos);
            stats.add_chroma_prediction_nanos(vvc_chroma_prediction_stats_family(mode), nanos);
        }
        let chroma_x =
            usize::from(node.x) / chroma_subsample_x(source_frame.format.chroma_sampling);
        let chroma_y =
            usize::from(node.y) / chroma_subsample_y(source_frame.format.chroma_sampling);
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            candidate_cb_residuals,
            &source_frame.cb,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            candidate_cb_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(residual_start);
            stats.add_chroma_residual_build_nanos(nanos);
            stats.add_chroma_rd_residual_build_nanos(nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            candidate_cr_residuals,
            &source_frame.cr,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            candidate_cr_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(residual_start);
            stats.add_chroma_residual_build_nanos(nanos);
            stats.add_chroma_rd_residual_build_nanos(nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let rd_candidate = score_vvc_chroma_mode_rd_candidate(
            coding_decision,
            mode,
            cclm_syntax_enabled,
            candidate_cb_residuals,
            candidate_cr_residuals,
            chroma_width,
            chroma_height,
            source_frame.format.bit_depth,
            chroma_qp,
            chroma_ts_quant,
            stats,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        if rd_candidate.selects_over(best_candidate) {
            best_mode = mode;
            best_candidate = rd_candidate;
            std::mem::swap(selected_cb_prediction, candidate_cb_prediction);
            std::mem::swap(selected_cr_prediction, candidate_cr_prediction);
            std::mem::swap(selected_cb_residuals, candidate_cb_residuals);
            std::mem::swap(selected_cr_residuals, candidate_cr_residuals);
        }
    }

    VvcSelectedChromaMode {
        mode: best_mode,
        residual: Some(best_candidate.residual),
    }
}

fn vvc_chroma_lossy_rd_refinement_allowed(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    decision: VvcChromaTuCodingDecision,
) -> bool {
    policy.residual_mode() == VvcResidualCodingMode::Lossy
        && matches!(
            decision.residual_coding,
            VvcTuResidualCodingMode::Transformed
        )
        && [4, 8, 16, 32].contains(&node.width)
        && [4, 8, 16, 32].contains(&node.height)
}

fn vvc_chroma_exact_prediction_skips_rd(cb_residuals: &[i16], cr_residuals: &[i16]) -> bool {
    cb_residuals.iter().all(|residual| *residual == 0)
        && cr_residuals.iter().all(|residual| *residual == 0)
}

#[derive(Debug, Clone, Copy)]
struct VvcSelectedChromaMode {
    mode: VvcChromaIntraPredictionMode,
    residual: Option<VvcScoredSelectedChromaResidual>,
}

fn select_vvc_chroma_bdpcm_prediction(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    selected_mode: VvcChromaIntraPredictionMode,
    cclm_syntax_enabled: bool,
    source_frame: &VvcSampledFrame,
    frame_recon: &VvcReconstructionFrame,
    chroma_width: usize,
    chroma_height: usize,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    selected_residual: Option<VvcScoredSelectedChromaResidual>,
    stats: &mut VvcIntraSearchStats,
    prediction_scratch: &mut VvcDcPredictionScratch,
    selected_cb_prediction: &mut Vec<VvcSample>,
    selected_cr_prediction: &mut Vec<VvcSample>,
    selected_cb_residuals: &mut Vec<i16>,
    selected_cr_residuals: &mut Vec<i16>,
    candidate_cb_prediction: &mut Vec<VvcSample>,
    candidate_cr_prediction: &mut Vec<VvcSample>,
    candidate_cb_residuals: &mut Vec<i16>,
    candidate_cr_residuals: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> Option<VvcSelectedChromaBdpcm> {
    if !vvc_chroma_bdpcm_selection_allowed(policy, chroma_width, chroma_height) {
        return None;
    }

    let baseline_decision = policy.select_chroma_tu_coding_decision(node, selected_mode);
    let baseline_residual = selected_residual.unwrap_or_else(|| {
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let residual = VvcSelectedChromaResidual {
            cb: finalize_vvc_chroma_residual_block(
                baseline_decision.residual_coding,
                selected_cb_residuals,
                chroma_width,
                chroma_height,
                source_frame.format.bit_depth,
                chroma_qp,
                chroma_ts_quant,
                stats,
            ),
            cr: finalize_vvc_chroma_residual_block(
                baseline_decision.residual_coding,
                selected_cr_residuals,
                chroma_width,
                chroma_height,
                source_frame.format.bit_depth,
                chroma_qp,
                chroma_ts_quant,
                stats,
            ),
        };
        let residual = VvcScoredSelectedChromaResidual::new(
            selected_cb_residuals,
            selected_cr_residuals,
            chroma_width,
            chroma_height,
            source_frame.format.bit_depth,
            chroma_qp,
            chroma_ts_quant,
            residual,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        residual
    });
    let mut best_score = vvc_scored_chroma_quantized_residual_score(
        baseline_residual,
        u64::from(vvc_bdpcm_mode_syntax_bin_count(VvcBdpcmMode::None)).saturating_add(u64::from(
            vvc_chroma_intra_mode_syntax_bin_count(selected_mode, cclm_syntax_enabled),
        )),
    );
    let mut best = None;

    for bdpcm_mode in [VvcBdpcmMode::Horizontal, VvcBdpcmMode::Vertical] {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_chroma_bdpcm_block_into_with_availability(
            candidate_cb_prediction,
            prediction_scratch,
            bdpcm_mode,
            &frame_recon.cb,
            source_frame.geometry,
            node,
            source_frame.format.chroma_sampling,
            source_frame.format.bit_depth,
            Some(frame_recon.cb_availability()),
        );
        predict_vvc_chroma_bdpcm_block_into_with_availability(
            candidate_cr_prediction,
            prediction_scratch,
            bdpcm_mode,
            &frame_recon.cr,
            source_frame.geometry,
            node,
            source_frame.format.chroma_sampling,
            source_frame.format.bit_depth,
            Some(frame_recon.cr_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_prediction_nanos(
            VvcChromaPredictionStatsFamily::Bdpcm,
            vvc_elapsed_nanos(prediction_start),
        );
        let chroma_x =
            usize::from(node.x) / chroma_subsample_x(source_frame.format.chroma_sampling);
        let chroma_y =
            usize::from(node.y) / chroma_subsample_y(source_frame.format.chroma_sampling);
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            candidate_cb_residuals,
            &source_frame.cb,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            candidate_cb_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_chroma_tu_at_into(
            candidate_cr_residuals,
            &source_frame.cr,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            candidate_cr_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let residual = VvcSelectedChromaResidual {
            cb: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
                candidate_cb_residuals,
                chroma_width,
                chroma_height,
                chroma_ts_quant,
                bdpcm_mode,
            ),
            cr: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
                candidate_cr_residuals,
                chroma_width,
                chroma_height,
                chroma_ts_quant,
                bdpcm_mode,
            ),
        };
        let residual = VvcScoredSelectedChromaResidual::new(
            candidate_cb_residuals,
            candidate_cr_residuals,
            chroma_width,
            chroma_height,
            source_frame.format.bit_depth,
            chroma_qp,
            chroma_ts_quant,
            residual,
            transform_scratch,
            reconstructed_residual,
        );
        let candidate_score = vvc_scored_chroma_quantized_residual_score(
            residual,
            u64::from(vvc_bdpcm_mode_syntax_bin_count(bdpcm_mode)),
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        if candidate_score.selects_over(best_score) {
            best_score = candidate_score;
            let mode = VvcChromaIntraPredictionMode::Explicit(
                bdpcm_mode
                    .inferred_intra_mode()
                    .expect("enabled BDPCM mode has an inferred intra mode"),
            );
            best = Some(VvcSelectedChromaBdpcm { mode, residual });
            std::mem::swap(selected_cb_prediction, candidate_cb_prediction);
            std::mem::swap(selected_cr_prediction, candidate_cr_prediction);
            std::mem::swap(selected_cb_residuals, candidate_cb_residuals);
            std::mem::swap(selected_cr_residuals, candidate_cr_residuals);
        }
    }

    best
}

fn vvc_chroma_bdpcm_selection_allowed(
    policy: VvcResidualCodingPolicy,
    chroma_width: usize,
    chroma_height: usize,
) -> bool {
    VVC_ENABLE_BDPCM_SELECTION
        && chroma_width <= usize::from(VVC_TRANSFORM_SKIP_MAX_SIZE)
        && chroma_height <= usize::from(VVC_TRANSFORM_SKIP_MAX_SIZE)
        && chroma_width >= 4
        && chroma_height >= 4
        && matches!(
            policy.residual_mode(),
            VvcResidualCodingMode::Lossy | VvcResidualCodingMode::Lossless
        )
}

#[derive(Debug, Clone, Copy)]
struct VvcSelectedChromaBdpcm {
    mode: VvcChromaIntraPredictionMode,
    residual: VvcScoredSelectedChromaResidual,
}

#[derive(Debug, Clone, Copy)]
struct VvcChromaModeRdCandidate {
    distortion: u64,
    rate_cost: u64,
    residual: VvcScoredSelectedChromaResidual,
}

impl VvcChromaModeRdCandidate {
    fn selects_over(self, best: Self) -> bool {
        vvc_quality_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

fn score_vvc_chroma_mode_rd_candidate(
    coding_decision: VvcChromaTuCodingDecision,
    mode: VvcChromaIntraPredictionMode,
    cclm_syntax_enabled: bool,
    cb_residuals: &[i16],
    cr_residuals: &[i16],
    chroma_width: usize,
    chroma_height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcChromaModeRdCandidate {
    let cb = select_vvc_scored_chroma_residual_block_with_transform_skip(
        coding_decision.residual_coding,
        cb_residuals,
        chroma_width,
        chroma_height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    let cr = select_vvc_scored_chroma_residual_block_with_transform_skip(
        coding_decision.residual_coding,
        cr_residuals,
        chroma_width,
        chroma_height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    let residual = VvcSelectedChromaResidual {
        cb: cb.block,
        cr: cr.block,
    };
    let rd_score = VvcResidualBlockScore {
        distortion: cb.score.distortion.saturating_add(cr.score.distortion),
        rate_cost: cb.score.rate_cost.saturating_add(cr.score.rate_cost),
    };
    let residual_score = VvcResidualBlockScore {
        distortion: rd_score.distortion,
        rate_cost: chroma_coeff_syntax_cost_estimate(chroma_width, chroma_height, residual.cb)
            .saturating_add(chroma_coeff_syntax_cost_estimate(
                chroma_width,
                chroma_height,
                residual.cr,
            )),
    };
    let residual = VvcScoredSelectedChromaResidual {
        residual,
        score: residual_score,
    };
    let mode_cost = u64::from(vvc_chroma_intra_mode_syntax_bin_count(
        mode,
        cclm_syntax_enabled,
    ));
    VvcChromaModeRdCandidate {
        distortion: rd_score.distortion,
        rate_cost: rd_score.rate_cost.saturating_add(mode_cost),
        residual,
    }
}

fn vvc_scored_chroma_quantized_residual_score(
    residual: VvcScoredSelectedChromaResidual,
    extra_syntax_cost: u64,
) -> VvcChromaQuantizedResidualScore {
    VvcChromaQuantizedResidualScore {
        distortion: residual.score.distortion,
        rate_cost: residual.score.rate_cost.saturating_add(extra_syntax_cost),
    }
}

fn chroma_reconstructed_residual_sse(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> u64 {
    if residual.transform_skip {
        return chroma_transform_skip_residual_sse(
            source_residuals,
            width,
            height,
            chroma_ts_quant,
            residual,
        );
    }
    reconstruct_vvc_chroma_residual_block_into(
        residual,
        reconstructed_residual,
        transform_scratch,
        width,
        height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
    );
    source_residuals
        .iter()
        .zip(reconstructed_residual.iter())
        .map(|(source, reconstructed)| {
            let diff = i64::from(*source) - i64::from(*reconstructed);
            (diff * diff) as u64
        })
        .sum()
}

fn chroma_transform_skip_residual_sse(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
) -> u64 {
    debug_assert!(residual.transform_skip);
    let mut reconstructed = [0i16; 16];
    let active_width = width.min(4);
    let active_height = height.min(4);
    if active_width != 0 && active_height != 0 {
        reconstructed[0] = residual.dc_level;
        for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
            if x < active_width && y < active_height {
                reconstructed[y * 4 + x] = residual.ac_levels[slot];
            }
        }
        if residual.bdpcm_mode.is_enabled() {
            inverse_bdpcm_quantized_levels_in_place(
                &mut reconstructed,
                4,
                active_height,
                residual.bdpcm_mode,
            );
        }
        for y in 0..active_height {
            let row = y * 4;
            for x in 0..active_width {
                reconstructed[row + x] = chroma_ts_quant.reconstructed(reconstructed[row + x]);
            }
        }
    }
    transform_skip_residual_sse(
        source_residuals,
        width,
        height,
        active_width,
        active_height,
        4,
        &reconstructed,
    )
}

fn chroma_coeff_syntax_cost_estimate(
    width: usize,
    height: usize,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
) -> u64 {
    let mut nonzero = u64::from(residual.dc_level != 0);
    let mut abs_sum = u64::from(residual.dc_level.unsigned_abs());
    let mut last_pos = 0u64;
    let active_width = width.min(4);
    let active_height = height.min(4);
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x >= active_width || y >= active_height {
            continue;
        }
        let abs_level = u64::from(residual.ac_levels[slot].unsigned_abs());
        if abs_level != 0 {
            nonzero += 1;
            abs_sum += abs_level;
            last_pos = (y * active_width + x) as u64;
        }
    }
    nonzero
        .saturating_mul(18)
        .saturating_add(abs_sum.saturating_mul(4))
        .saturating_add(last_pos.saturating_mul(2))
}

#[derive(Debug, Clone, Copy)]
struct VvcChromaQuantizedResidualScore {
    distortion: u64,
    rate_cost: u64,
}

impl VvcChromaQuantizedResidualScore {
    fn selects_over(self, best: Self) -> bool {
        vvc_quality_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcChromaModeRdShortlist {
    candidates: [VvcChromaIntraCandidateCost; VVC_CHROMA_INTRA_CANDIDATE_CAPACITY],
    count: usize,
}

impl VvcChromaModeRdShortlist {
    fn from_candidate_costs(
        policy: VvcResidualCodingPolicy,
        costs: VvcChromaIntraCandidateCosts,
    ) -> Self {
        let mut shortlist = Self {
            candidates: [VvcChromaIntraCandidateCost::new(
                VvcChromaIntraPredictionMode::Derived,
                u64::MAX,
            ); VVC_CHROMA_INTRA_CANDIDATE_CAPACITY],
            count: 0,
        };
        for candidate in costs.iter() {
            shortlist.add(candidate);
        }
        shortlist.apply_policy_limit(policy);
        shortlist
    }

    fn add(&mut self, candidate: VvcChromaIntraCandidateCost) {
        if let Some(existing) = self
            .candidates
            .iter()
            .take(self.count)
            .position(|entry| entry.mode() == candidate.mode())
        {
            if candidate.score() < self.candidates[existing].score() {
                self.candidates[existing] = candidate;
                self.sort();
            }
            return;
        }
        if self.count < self.candidates.len() {
            self.candidates[self.count] = candidate;
            self.count += 1;
            self.sort();
            return;
        }
        let worst = self.count - 1;
        if candidate.score() < self.candidates[worst].score() {
            self.candidates[worst] = candidate;
            self.sort();
        }
    }

    fn sort(&mut self) {
        self.candidates[..self.count].sort_by_key(|candidate| candidate.score());
    }

    fn apply_policy_limit(&mut self, policy: VvcResidualCodingPolicy) {
        self.count = self
            .count
            .min(vvc_chroma_mode_rd_shortlist_limit(policy).min(self.candidates.len()));
    }

    fn iter(self) -> impl Iterator<Item = VvcChromaIntraCandidateCost> {
        self.candidates.into_iter().take(self.count)
    }
}

fn vvc_chroma_mode_rd_shortlist_limit(policy: VvcResidualCodingPolicy) -> usize {
    match policy.residual_mode() {
        VvcResidualCodingMode::Lossless => VVC_CHROMA_INTRA_CANDIDATE_CAPACITY,
        VvcResidualCodingMode::Lossy => VVC_LOSSY_CHROMA_RD_WINNER_CANDIDATES,
    }
}

#[cfg(feature = "vvc-stats")]
fn json_i16_slice(values: &[i16]) -> String {
    let mut out = String::from("[");
    for (idx, value) in values.iter().enumerate() {
        if idx != 0 {
            out.push(',');
        }
        out.push_str(&value.to_string());
    }
    out.push(']');
    out
}

#[cfg(feature = "vvc-stats")]
fn json_u16_slice(values: &[VvcSample]) -> String {
    let mut out = String::from("[");
    for (idx, value) in values.iter().enumerate() {
        if idx != 0 {
            out.push(',');
        }
        out.push_str(&value.to_string());
    }
    out.push(']');
    out
}

fn select_vvc_luma_mrl_prediction(
    policy: VvcResidualCodingPolicy,
    residual_coding: VvcTuResidualCodingMode,
    requested_mts_index: u8,
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    preselected_residual: Option<VvcScoredSelectedLumaResidual>,
    stats: &mut VvcIntraSearchStats,
    frame_recon: &VvcReconstructionFrame,
    source_frame: &VvcSampledFrame,
    prediction_scratch: &mut VvcDcPredictionScratch,
    selected_prediction: &mut Vec<VvcSample>,
    selected_residuals: &mut Vec<i16>,
    candidate_prediction: &mut Vec<VvcSample>,
    candidate_residuals: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcSelectedLumaMrl {
    if !VVC_ENABLE_LUMA_MRL_SELECTION
        || !policy.luma_mrl_candidate_allowed(node, mode)
        || !vvc_luma_intra_mode_is_mpm(mode, left, above)
        || vvc_luma_exact_prediction_skips_rd(selected_residuals)
    {
        return VvcSelectedLumaMrl {
            mrl_index: 0,
            residual: preselected_residual,
        };
    }

    let score_metric = policy.score_metric();
    let mut best_mrl_index = 0u8;
    let mut best_candidate = match preselected_residual {
        Some(residual) => VvcLumaMrlCandidate::from_scored_residual(
            residual,
            u64::from(vvc_luma_mrl_syntax_bin_count(node, 0)),
        ),
        None => {
            #[cfg(feature = "vvc-stats")]
            let score_start = Instant::now();
            let candidate = score_vvc_luma_mrl_candidate(
                score_metric,
                residual_coding,
                requested_mts_index,
                node,
                0,
                selected_residuals,
                source_frame.format.bit_depth,
                luma_qp,
                luma_ts_quant,
                stats,
                transform_scratch,
                reconstructed_residual,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
            candidate
        }
    };
    for mrl_index in 1..=2 {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_luma_intra_block_into_with_mrl_and_availability(
            candidate_prediction,
            prediction_scratch,
            mode,
            &frame_recon.luma,
            source_frame.geometry,
            node,
            source_frame.format.bit_depth,
            mrl_index,
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(prediction_start);
            stats.add_luma_prediction_nanos(VvcLumaPredictionStatsFamily::Mrl, nanos);
            stats.add_luma_rd_prediction_nanos(nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_luma_tu_at_into(
            candidate_residuals,
            source_frame,
            usize::from(node.x),
            usize::from(node.y),
            usize::from(node.width),
            usize::from(node.height),
            candidate_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        {
            let nanos = vvc_elapsed_nanos(residual_start);
            stats.add_luma_residual_build_nanos(nanos);
            stats.add_luma_rd_residual_build_nanos(nanos);
        }
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let candidate = score_vvc_luma_mrl_candidate(
            score_metric,
            residual_coding,
            requested_mts_index,
            node,
            mrl_index,
            candidate_residuals,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
            stats,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        if candidate.selects_over(best_candidate) {
            best_candidate = candidate;
            best_mrl_index = mrl_index;
            std::mem::swap(selected_prediction, candidate_prediction);
            std::mem::swap(selected_residuals, candidate_residuals);
        }
    }
    VvcSelectedLumaMrl {
        mrl_index: best_mrl_index,
        residual: best_candidate.residual,
    }
}

fn select_vvc_luma_bdpcm_prediction(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    selected_mode: VvcIntraPredictionMode,
    selected_decision: VvcLumaTuCodingDecision,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    selected_residual: Option<VvcScoredSelectedLumaResidual>,
    stats: &mut VvcIntraSearchStats,
    frame_recon: &VvcReconstructionFrame,
    source_frame: &VvcSampledFrame,
    prediction_scratch: &mut VvcDcPredictionScratch,
    selected_prediction: &mut Vec<VvcSample>,
    selected_residuals: &mut Vec<i16>,
    candidate_prediction: &mut Vec<VvcSample>,
    candidate_residuals: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> Option<VvcSelectedLumaBdpcm> {
    if !vvc_luma_bdpcm_selection_allowed(policy, node) {
        return None;
    }

    let baseline_residual = selected_residual.unwrap_or_else(|| {
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let scored_residual = select_vvc_scored_luma_residual_block_with_mts(
            selected_decision.residual_coding,
            selected_decision.mts_index,
            selected_residuals,
            node.width,
            node.height,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
            true,
            VvcLumaResidualQuantizationSearch::Full,
            stats,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        VvcScoredSelectedLumaResidual::from_scored_block(scored_residual)
    });
    let mut best_score = vvc_scored_luma_quantized_residual_score(
        baseline_residual,
        vvc_luma_regular_prediction_syntax_cost(
            node,
            selected_mode,
            left,
            above,
            selected_decision,
        ),
    );
    let mut best = None;

    for bdpcm_mode in [VvcBdpcmMode::Horizontal, VvcBdpcmMode::Vertical] {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = Instant::now();
        predict_vvc_luma_bdpcm_block_into_with_availability(
            candidate_prediction,
            prediction_scratch,
            bdpcm_mode,
            &frame_recon.luma,
            source_frame.geometry,
            node,
            source_frame.format.bit_depth,
            Some(frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_prediction_nanos(
            VvcLumaPredictionStatsFamily::Bdpcm,
            vvc_elapsed_nanos(prediction_start),
        );
        #[cfg(feature = "vvc-stats")]
        let residual_start = Instant::now();
        residual_luma_tu_at_into(
            candidate_residuals,
            source_frame,
            usize::from(node.x),
            usize::from(node.y),
            usize::from(node.width),
            usize::from(node.height),
            candidate_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(feature = "vvc-stats")]
        let score_start = Instant::now();
        let residual = VvcSelectedLumaResidual {
            block: finalize_vvc_luma_bdpcm_transform_skip_residual_block(
                candidate_residuals,
                node.width,
                node.height,
                luma_ts_quant,
                bdpcm_mode,
            ),
            mts_index: 0,
        };
        let residual = VvcScoredSelectedLumaResidual::new(
            candidate_residuals,
            node,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
            residual,
            transform_scratch,
            reconstructed_residual,
        );
        let candidate_score = vvc_scored_luma_quantized_residual_score(
            residual,
            u64::from(vvc_bdpcm_mode_syntax_bin_count(bdpcm_mode)),
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        if candidate_score.selects_over(best_score) {
            best_score = candidate_score;
            let mode = bdpcm_mode
                .inferred_intra_mode()
                .expect("enabled BDPCM mode has an inferred intra mode");
            best = Some(VvcSelectedLumaBdpcm {
                mode,
                coding_decision: VvcLumaTuCodingDecision {
                    residual_coding: VvcTuResidualCodingMode::TransformSkip,
                    mrl_index: 0,
                    mts_index: 0,
                },
                residual,
            });
            std::mem::swap(selected_prediction, candidate_prediction);
            std::mem::swap(selected_residuals, candidate_residuals);
        }
    }

    best
}

fn vvc_luma_bdpcm_selection_allowed(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
) -> bool {
    VVC_ENABLE_BDPCM_SELECTION
        && node.width <= VVC_TRANSFORM_SKIP_MAX_SIZE
        && node.height <= VVC_TRANSFORM_SKIP_MAX_SIZE
        && node.width >= 4
        && node.height >= 4
        && node.width.is_power_of_two()
        && node.height.is_power_of_two()
        && matches!(
            policy.residual_mode(),
            VvcResidualCodingMode::Lossy | VvcResidualCodingMode::Lossless
        )
}

fn vvc_luma_regular_prediction_syntax_cost(
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    decision: VvcLumaTuCodingDecision,
) -> u64 {
    u64::from(vvc_bdpcm_mode_syntax_bin_count(VvcBdpcmMode::None))
        .saturating_add(u64::from(vvc_luma_mrl_syntax_bin_count(
            node,
            decision.mrl_index,
        )))
        .saturating_add(u64::from(vvc_luma_intra_mode_syntax_bin_count(
            mode, left, above,
        )))
}

fn vvc_bdpcm_mode_syntax_bin_count(mode: VvcBdpcmMode) -> u8 {
    match mode {
        VvcBdpcmMode::None => 1,
        VvcBdpcmMode::Horizontal | VvcBdpcmMode::Vertical => 2,
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcSelectedLumaBdpcm {
    mode: VvcIntraPredictionMode,
    coding_decision: VvcLumaTuCodingDecision,
    residual: VvcScoredSelectedLumaResidual,
}

#[derive(Debug, Clone, Copy, Default)]
struct VvcSelectedLumaMrl {
    mrl_index: u8,
    residual: Option<VvcScoredSelectedLumaResidual>,
}

#[derive(Debug, Clone, Copy)]
struct VvcLumaMrlCandidate {
    distortion: u64,
    rate_cost: u64,
    residual: Option<VvcScoredSelectedLumaResidual>,
}

impl VvcLumaMrlCandidate {
    fn from_scored_residual(
        residual: VvcScoredSelectedLumaResidual,
        extra_syntax_cost: u64,
    ) -> Self {
        let score = vvc_scored_luma_quantized_residual_score(residual, extra_syntax_cost);
        Self {
            distortion: score.distortion,
            rate_cost: score.rate_cost,
            residual: Some(residual),
        }
    }

    fn selects_over(self, best: Self) -> bool {
        vvc_quality_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

fn score_vvc_luma_mrl_candidate(
    metric: VvcResidualScoreMetric,
    residual_coding: VvcTuResidualCodingMode,
    requested_mts_index: u8,
    node: VvcCodingTreeNode,
    mrl_index: u8,
    residuals: &[i16],
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcLumaMrlCandidate {
    if matches!(residual_coding, VvcTuResidualCodingMode::Transformed) {
        let scored_residual = select_vvc_scored_luma_residual_block_with_mts(
            residual_coding,
            requested_mts_index,
            residuals,
            node.width,
            node.height,
            bit_depth,
            luma_qp,
            luma_ts_quant,
            false,
            VvcLumaResidualQuantizationSearch::FastModeDecision,
            stats,
            transform_scratch,
            reconstructed_residual,
        );
        let mrl_cost = u64::from(vvc_luma_mrl_syntax_bin_count(node, mrl_index));
        let selected_residual = VvcScoredSelectedLumaResidual::from_scored_block(scored_residual);
        return VvcLumaMrlCandidate {
            distortion: scored_residual.score.distortion,
            rate_cost: scored_residual.score.rate_cost.saturating_add(mrl_cost),
            residual: Some(selected_residual),
        };
    }
    VvcLumaMrlCandidate {
        distortion: residual_mode_selection_score(metric, residuals),
        rate_cost: u64::from(vvc_luma_mrl_syntax_bin_count(node, mrl_index)),
        residual: None,
    }
}

fn vvc_scored_luma_quantized_residual_score(
    residual: VvcScoredSelectedLumaResidual,
    extra_syntax_cost: u64,
) -> VvcLumaQuantizedResidualScore {
    VvcLumaQuantizedResidualScore {
        distortion: residual.score.distortion,
        rate_cost: residual.score.rate_cost.saturating_add(extra_syntax_cost),
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcLumaQuantizedResidualScore {
    distortion: u64,
    rate_cost: u64,
}

impl VvcLumaQuantizedResidualScore {
    fn selects_over(self, best: Self) -> bool {
        vvc_quality_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

fn vvc_quality_candidate_selects_over(
    distortion: u64,
    rate_cost: u64,
    best_distortion: u64,
    best_rate_cost: u64,
) -> bool {
    distortion < best_distortion || (distortion == best_distortion && rate_cost < best_rate_cost)
}

fn luma_reconstructed_residual_sse(
    source_residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    qp: i32,
    ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    mts_index: u8,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> u64 {
    if residual.transform_skip {
        return luma_transform_skip_residual_sse(
            source_residuals,
            usize::from(width),
            usize::from(height),
            ts_quant,
            residual,
        );
    }
    luma_reconstructed_residual_sse_with_mts_into(
        source_residuals,
        width,
        height,
        bit_depth,
        qp,
        residual.dc_level,
        &residual.ac_levels,
        mts_index,
        transform_scratch,
        reconstructed_residual,
    )
}

fn luma_transform_skip_residual_sse(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
) -> u64 {
    debug_assert!(residual.transform_skip);
    let (active_width, active_height) = vvc_luma_transform_skip_active_extent(width, height);
    let mut reconstructed = [0i16; 64];
    if active_width != 0 && active_height != 0 {
        reconstructed[0] = residual.dc_level;
        for y in 0..active_height {
            for x in 0..active_width {
                if x == 0 && y == 0 {
                    continue;
                }
                reconstructed[y * active_width + x] = residual.ac_levels[y * active_width + x - 1];
            }
        }
        if residual.bdpcm_mode.is_enabled() {
            inverse_bdpcm_quantized_levels_in_place(
                &mut reconstructed,
                active_width,
                active_height,
                residual.bdpcm_mode,
            );
        }
        for y in 0..active_height {
            let row = y * active_width;
            for x in 0..active_width {
                reconstructed[row + x] = ts_quant.reconstructed(reconstructed[row + x]);
            }
        }
    }
    transform_skip_residual_sse(
        source_residuals,
        width,
        height,
        active_width,
        active_height,
        active_width,
        &reconstructed,
    )
}

fn transform_skip_residual_sse(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    active_width: usize,
    active_height: usize,
    reconstructed_stride: usize,
    reconstructed: &[i16],
) -> u64 {
    debug_assert_eq!(source_residuals.len(), width * height);
    let mut sse = 0u64;
    for y in 0..height {
        for x in 0..width {
            let reconstructed_sample = if x < active_width && y < active_height {
                let idx = y * reconstructed_stride + x;
                reconstructed.get(idx).copied().unwrap_or(0)
            } else {
                0
            };
            let diff = i64::from(source_residuals[y * width + x]) - i64::from(reconstructed_sample);
            sse = sse.saturating_add((diff * diff) as u64);
        }
    }
    sse
}

fn vvc_luma_mrl_syntax_bin_count(node: VvcCodingTreeNode, mrl_index: u8) -> u8 {
    if node.y % VVC_CTU_SIZE as u16 == 0 {
        0
    } else if mrl_index == 0 {
        1
    } else {
        2
    }
}

fn finalized_vvc_chroma_sample(
    transform_skip: bool,
    source: VvcSample,
    quantized_remainder: u8,
    bit_depth: SampleBitDepth,
) -> u8 {
    if transform_skip {
        vvc_downshift_sample_to_u8(source, bit_depth)
    } else {
        reconstruct_vvc_chroma(quantized_remainder)
    }
}

const VVC_LUMA_DIRECTIONAL_SEARCH_CANDIDATE_CAPACITY: usize = 65;
const VVC_LUMA_DEFAULT_DIRECTIONAL_SEEDS: [u8; 9] = [18, 50, 34, 10, 26, 42, 58, 2, 66];
const VVC_LUMA_LOSSY_FALLBACK_DIRECTIONAL_SEEDS: [u8; 5] = [18, 50, 34, 2, 66];
const VVC_LUMA_NEARBY_DIRECTIONAL_OFFSETS: [i16; 7] = [0, -1, 1, -2, 2, -4, 4];
const VVC_LUMA_MODE_CELL_SIZE: usize = 4;

#[derive(Debug, Clone, Copy)]
struct VvcLumaDirectionalSearchCandidates {
    modes: [VvcIntraPredictionMode; VVC_LUMA_DIRECTIONAL_SEARCH_CANDIDATE_CAPACITY],
    count: usize,
}

impl VvcLumaDirectionalSearchCandidates {
    fn new() -> Self {
        Self {
            modes: [VvcIntraPredictionMode::Horizontal;
                VVC_LUMA_DIRECTIONAL_SEARCH_CANDIDATE_CAPACITY],
            count: 0,
        }
    }

    fn add_mode(&mut self, mode: VvcIntraPredictionMode) {
        debug_assert!((2..=66).contains(&mode.luma_mode_index()));
        if self
            .modes
            .iter()
            .take(self.count)
            .any(|candidate| candidate.luma_mode_index() == mode.luma_mode_index())
        {
            return;
        }
        assert!(self.count < self.modes.len());
        self.modes[self.count] = mode;
        self.count += 1;
    }

    fn add_index(&mut self, index: u8) {
        if (2..=66).contains(&index) {
            self.add_mode(vvc_luma_intra_mode_from_index(index));
        }
    }

    fn add_family(&mut self, center: u8) {
        for offset in VVC_LUMA_NEARBY_DIRECTIONAL_OFFSETS {
            let index = i16::from(center) + offset;
            if (2..=66).contains(&index) {
                self.add_index(index as u8);
            }
        }
    }

    fn add_refinement(&mut self, policy: VvcResidualCodingPolicy, center: u8) {
        if policy.residual_mode() == VvcResidualCodingMode::Lossy {
            self.add_family(center);
            return;
        }
        for offset in -8..=8 {
            let index = i16::from(center) + offset;
            if (2..=66).contains(&index) {
                self.add_index(index as u8);
            }
        }
    }

    fn count(self) -> usize {
        self.count
    }

    fn iter(self) -> impl Iterator<Item = VvcIntraPredictionMode> {
        self.modes.into_iter().take(self.count)
    }

    fn iter_from(self, start: usize) -> impl Iterator<Item = VvcIntraPredictionMode> {
        self.modes.into_iter().skip(start).take(self.count - start)
    }
}

#[derive(Debug, Clone)]
pub(in crate::vvc) struct VvcLumaModeSearchState {
    width: usize,
    height: usize,
    cell_cols: usize,
    valid: Vec<bool>,
    modes: Vec<VvcIntraPredictionMode>,
}

impl VvcLumaModeSearchState {
    pub(in crate::vvc) fn new_for_geometry(geometry: VvcVideoGeometry) -> Self {
        let width = geometry.coded_width();
        let height = geometry.coded_height();
        let cell_cols = width.div_ceil(VVC_LUMA_MODE_CELL_SIZE);
        let cell_rows = height.div_ceil(VVC_LUMA_MODE_CELL_SIZE);
        let cell_count = cell_cols.saturating_mul(cell_rows);
        Self {
            width,
            height,
            cell_cols,
            valid: vec![false; cell_count],
            modes: vec![VvcIntraPredictionMode::Planar; cell_count],
        }
    }

    fn left_of(&self, node: VvcCodingTreeNode) -> Option<VvcIntraPredictionMode> {
        let x = node.x.checked_sub(1)?;
        let y = node.y.saturating_add(node.height).saturating_sub(1);
        self.mode_at(x, y)
    }

    fn above_of(&self, node: VvcCodingTreeNode) -> Option<VvcIntraPredictionMode> {
        let y = node.y.checked_sub(1)?;
        if node.y % VVC_CTU_SIZE as u16 == 0 {
            return None;
        }
        let x = node.x.saturating_add(node.width).saturating_sub(1);
        self.mode_at(x, y)
    }

    fn mode_at(&self, x: u16, y: u16) -> Option<VvcIntraPredictionMode> {
        let x = usize::from(x);
        let y = usize::from(y);
        if x >= self.width || y >= self.height {
            return None;
        }
        let cell_x = x / VVC_LUMA_MODE_CELL_SIZE;
        let cell_y = y / VVC_LUMA_MODE_CELL_SIZE;
        let idx = cell_y * self.cell_cols + cell_x;
        self.valid[idx].then_some(self.modes[idx])
    }

    fn mark_node(&mut self, node: VvcCodingTreeNode, mode: VvcIntraPredictionMode) {
        let start_x = usize::from(node.x).min(self.width);
        let start_y = usize::from(node.y).min(self.height);
        let end_x = usize::from(node.x)
            .saturating_add(usize::from(node.width))
            .min(self.width);
        let end_y = usize::from(node.y)
            .saturating_add(usize::from(node.height))
            .min(self.height);
        if end_x <= start_x || end_y <= start_y {
            return;
        }
        let start_cell_x = usize::from(node.x) / VVC_LUMA_MODE_CELL_SIZE;
        let start_cell_y = usize::from(node.y) / VVC_LUMA_MODE_CELL_SIZE;
        let end_cell_x = end_x.div_ceil(VVC_LUMA_MODE_CELL_SIZE);
        let end_cell_y = end_y.div_ceil(VVC_LUMA_MODE_CELL_SIZE);
        for cell_y in start_cell_y..end_cell_y {
            for cell_x in start_cell_x..end_cell_x {
                let idx = cell_y * self.cell_cols + cell_x;
                self.valid[idx] = true;
                self.modes[idx] = mode;
            }
        }
    }

    fn co_located_mode_for_chroma_node(
        &self,
        chroma_node: VvcCodingTreeNode,
    ) -> VvcIntraPredictionMode {
        let max_x = self.width.saturating_sub(1).min(usize::from(u16::MAX)) as u16;
        let max_y = self.height.saturating_sub(1).min(usize::from(u16::MAX)) as u16;
        let ref_x = chroma_node
            .x
            .saturating_add(chroma_node.width >> 1)
            .min(max_x);
        let ref_y = chroma_node
            .y
            .saturating_add(chroma_node.height >> 1)
            .min(max_y);
        self.mode_at(ref_x, ref_y)
            .unwrap_or(VvcIntraPredictionMode::Dc)
    }
}

fn vvc_luma_directional_search_candidates(
    policy: VvcResidualCodingPolicy,
    source_frame: &VvcSampledFrame,
    mode_state: &VvcLumaModeSearchState,
    global_node: VvcCodingTreeNode,
) -> VvcLumaDirectionalSearchCandidates {
    let mut candidates = VvcLumaDirectionalSearchCandidates::new();
    if policy.residual_mode() == VvcResidualCodingMode::Lossy {
        if vvc_source_luma_directional_seed_allowed(policy, global_node) {
            if let Some(index) = vvc_source_luma_directional_seed(source_frame, global_node) {
                candidates.add_family(index);
            }
        }
        for mode in [
            mode_state.left_of(global_node),
            mode_state.above_of(global_node),
        ]
        .into_iter()
        .flatten()
        {
            candidates.add_index(mode.luma_mode_index());
        }
        for index in VVC_LUMA_LOSSY_FALLBACK_DIRECTIONAL_SEEDS {
            candidates.add_index(index);
        }
    } else {
        for index in VVC_LUMA_DEFAULT_DIRECTIONAL_SEEDS {
            candidates.add_index(index);
        }
        for mode in [
            mode_state.left_of(global_node),
            mode_state.above_of(global_node),
        ]
        .into_iter()
        .flatten()
        {
            candidates.add_family(mode.luma_mode_index());
        }
        if let Some(index) = vvc_source_luma_directional_seed(source_frame, global_node) {
            candidates.add_family(index);
        }
    }
    candidates
}

fn vvc_source_luma_directional_seed_allowed(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
) -> bool {
    policy.residual_mode() == VvcResidualCodingMode::Lossless
        || (node.width >= 8 && node.height >= 8)
}

fn vvc_source_luma_directional_seed(
    source_frame: &VvcSampledFrame,
    node: VvcCodingTreeNode,
) -> Option<u8> {
    let x0 = usize::from(node.x);
    let y0 = usize::from(node.y);
    let x1 = x0
        .saturating_add(usize::from(node.width))
        .min(source_frame.geometry.width);
    let y1 = y0
        .saturating_add(usize::from(node.height))
        .min(source_frame.geometry.height);
    if x1 <= x0 + 1 || y1 <= y0 + 1 {
        return None;
    }

    let stride = source_frame.geometry.width;
    let mut gxx = 0i64;
    let mut gyy = 0i64;
    let mut gxy = 0i64;
    for y in (y0 + 1)..y1 {
        for x in (x0 + 1)..x1 {
            let sample = i64::from(source_frame.luma[y * stride + x]);
            let dx = sample - i64::from(source_frame.luma[y * stride + x - 1]);
            let dy = sample - i64::from(source_frame.luma[(y - 1) * stride + x]);
            gxx += dx * dx;
            gyy += dy * dy;
            gxy += dx * dy;
        }
    }
    if gxx == 0 && gyy == 0 {
        return None;
    }

    let gradient_angle = 0.5 * (2.0 * gxy as f64).atan2((gxx - gyy) as f64);
    let mut edge_angle = gradient_angle + std::f64::consts::FRAC_PI_2;
    while edge_angle < 0.0 {
        edge_angle += std::f64::consts::PI;
    }
    while edge_angle >= std::f64::consts::PI {
        edge_angle -= std::f64::consts::PI;
    }
    let folded_edge_angle = if edge_angle > std::f64::consts::FRAC_PI_2 {
        std::f64::consts::PI - edge_angle
    } else {
        edge_angle
    };
    let mode_offset = (folded_edge_angle / std::f64::consts::FRAC_PI_2 * 32.0).round() as i16;
    Some((18 + mode_offset).clamp(2, 66) as u8)
}

fn luma_prediction_mode_selection_score(
    metric: VvcResidualScoreMetric,
    source_frame: &VvcSampledFrame,
    node: VvcCodingTreeNode,
    predicted: &[VvcSample],
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    mode: VvcIntraPredictionMode,
) -> u64 {
    const SYNTAX_TIE_BREAKER_SCALE: u64 = 64;
    luma_prediction_residual_score(metric, source_frame, node, predicted)
        .saturating_mul(SYNTAX_TIE_BREAKER_SCALE)
        .saturating_add(u64::from(vvc_luma_intra_mode_syntax_bin_count(
            mode, left, above,
        )))
}

fn luma_residual_mode_selection_score(
    metric: VvcResidualScoreMetric,
    residuals: &[i16],
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    mode: VvcIntraPredictionMode,
) -> u64 {
    const SYNTAX_TIE_BREAKER_SCALE: u64 = 64;
    residual_mode_selection_score(metric, residuals)
        .saturating_mul(SYNTAX_TIE_BREAKER_SCALE)
        .saturating_add(u64::from(vvc_luma_intra_mode_syntax_bin_count(
            mode, left, above,
        )))
}

fn luma_prediction_residual_score(
    metric: VvcResidualScoreMetric,
    frame: &VvcSampledFrame,
    node: VvcCodingTreeNode,
    predicted: &[VvcSample],
) -> u64 {
    let origin_x = usize::from(node.x);
    let origin_y = usize::from(node.y);
    let width = usize::from(node.width);
    let height = usize::from(node.height);
    debug_assert_eq!(predicted.len(), width * height);
    let copy_width = width.min(frame.geometry.width.saturating_sub(origin_x));
    let copy_height = height.min(frame.geometry.height.saturating_sub(origin_y));
    let mut score = 0u64;
    for y in 0..height {
        let dst = y * width;
        if y < copy_height {
            let src = (origin_y + y) * frame.geometry.width + origin_x;
            for x in 0..width {
                let sample = if x < copy_width {
                    frame.luma[src + x]
                } else {
                    0
                };
                score = score.saturating_add(vvc_sample_delta_score(
                    metric,
                    sample,
                    predicted[dst + x],
                ));
            }
        } else {
            for x in 0..width {
                score = score.saturating_add(vvc_sample_delta_score(metric, 0, predicted[dst + x]));
            }
        }
    }
    score
}

fn chroma_prediction_mode_selection_score(
    metric: VvcResidualScoreMetric,
    source_frame: &VvcSampledFrame,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
    mode: VvcChromaIntraPredictionMode,
    cclm_enabled: bool,
    syntax_tie_breaker_enabled: bool,
) -> u64 {
    const SYNTAX_TIE_BREAKER_SCALE: u64 = 64;
    let residual_score = chroma_prediction_residual_score(
        metric,
        source_frame,
        origin_x,
        origin_y,
        width,
        height,
        predicted_cb,
        predicted_cr,
    );
    let syntax_tie_breaker = if syntax_tie_breaker_enabled {
        vvc_chroma_intra_mode_syntax_bin_count(mode, cclm_enabled)
    } else {
        0
    };
    residual_score
        .saturating_mul(SYNTAX_TIE_BREAKER_SCALE)
        .saturating_add(u64::from(syntax_tie_breaker))
}

fn chroma_residual_mode_selection_score(
    metric: VvcResidualScoreMetric,
    cb_residuals: &[i16],
    cr_residuals: &[i16],
    mode: VvcChromaIntraPredictionMode,
    cclm_enabled: bool,
    syntax_tie_breaker_enabled: bool,
) -> u64 {
    const SYNTAX_TIE_BREAKER_SCALE: u64 = 64;
    let residual_score = residual_mode_selection_score(metric, cb_residuals)
        .saturating_add(residual_mode_selection_score(metric, cr_residuals));
    let syntax_tie_breaker = if syntax_tie_breaker_enabled {
        vvc_chroma_intra_mode_syntax_bin_count(mode, cclm_enabled)
    } else {
        0
    };
    residual_score
        .saturating_mul(SYNTAX_TIE_BREAKER_SCALE)
        .saturating_add(u64::from(syntax_tie_breaker))
}

fn chroma_prediction_residual_score(
    metric: VvcResidualScoreMetric,
    frame: &VvcSampledFrame,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
) -> u64 {
    chroma_plane_prediction_residual_score(
        metric,
        &frame.cb,
        frame.geometry,
        frame.format,
        origin_x,
        origin_y,
        width,
        height,
        predicted_cb,
    )
    .saturating_add(chroma_plane_prediction_residual_score(
        metric,
        &frame.cr,
        frame.geometry,
        frame.format,
        origin_x,
        origin_y,
        width,
        height,
        predicted_cr,
    ))
}

fn chroma_plane_prediction_residual_score(
    metric: VvcResidualScoreMetric,
    samples: &[VvcSample],
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted: &[VvcSample],
) -> u64 {
    debug_assert_eq!(predicted.len(), width * height);
    let chroma_width = geometry.width / chroma_subsample_x(format.chroma_sampling);
    let chroma_height = geometry.height / chroma_subsample_y(format.chroma_sampling);
    let neutral = vvc_neutral_sample(format.bit_depth);
    let copy_width = width.min(chroma_width.saturating_sub(origin_x));
    let copy_height = height.min(chroma_height.saturating_sub(origin_y));
    let mut score = 0u64;
    for y in 0..height {
        let dst = y * width;
        if y < copy_height {
            let src = (origin_y + y) * chroma_width + origin_x;
            for x in 0..width {
                let sample = if x < copy_width {
                    samples[src + x]
                } else {
                    neutral
                };
                score = score.saturating_add(vvc_sample_delta_score(
                    metric,
                    sample,
                    predicted[dst + x],
                ));
            }
        } else {
            for x in 0..width {
                score = score.saturating_add(vvc_sample_delta_score(
                    metric,
                    neutral,
                    predicted[dst + x],
                ));
            }
        }
    }
    score
}

fn vvc_sample_delta_score(
    metric: VvcResidualScoreMetric,
    sample: VvcSample,
    predicted: VvcSample,
) -> u64 {
    let residual = vvc_sample_delta_i16(sample, predicted);
    match metric {
        VvcResidualScoreMetric::Sad => u64::from(residual.unsigned_abs()),
        VvcResidualScoreMetric::Sse => {
            let residual = i64::from(residual);
            (residual * residual) as u64
        }
    }
}

fn residual_sad(residuals: &[i16]) -> u64 {
    residuals
        .iter()
        .map(|residual| u64::from(residual.unsigned_abs()))
        .sum()
}

fn vvc_luma_exact_min_syntax_mode_search_done(best_score: u64) -> bool {
    best_score <= u64::from(VVC_LUMA_MIN_INTRA_MODE_SYNTAX_BINS)
}

fn vvc_chroma_lossy_exact_mode_search_done(
    syntax_tie_breaker_enabled: bool,
    best_score: u64,
) -> bool {
    !syntax_tie_breaker_enabled && best_score == 0
}

fn residual_mode_selection_score(metric: VvcResidualScoreMetric, residuals: &[i16]) -> u64 {
    match metric {
        VvcResidualScoreMetric::Sad => residual_sad(residuals),
        VvcResidualScoreMetric::Sse => residual_sse(residuals),
    }
}

fn residual_sse(residuals: &[i16]) -> u64 {
    residuals
        .iter()
        .map(|residual| {
            let residual = i64::from(*residual);
            (residual * residual) as u64
        })
        .sum()
}

#[derive(Debug, Clone, Copy)]
struct VvcFinalizedResidualBlock<const AC_COEFFS: usize> {
    dc_level: i16,
    ac_levels: [i16; AC_COEFFS],
    has_ac: bool,
    transform_skip: bool,
    bdpcm_mode: VvcBdpcmMode,
}

impl<const AC_COEFFS: usize> VvcFinalizedResidualBlock<AC_COEFFS> {
    fn abs_remainder(self) -> u8 {
        self.dc_level.unsigned_abs().min(u8::MAX as u16) as u8
    }

    fn negative(self) -> bool {
        self.dc_level < 0 && self.abs_remainder() != 0
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcSelectedLumaResidual {
    block: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    mts_index: u8,
}

#[derive(Debug, Clone, Copy)]
struct VvcScoredSelectedLumaResidual {
    residual: VvcSelectedLumaResidual,
    score: VvcResidualBlockScore,
}

impl VvcScoredSelectedLumaResidual {
    fn new(
        source_residuals: &[i16],
        node: VvcCodingTreeNode,
        bit_depth: SampleBitDepth,
        luma_qp: i32,
        luma_ts_quant: &VvcTransformSkipQuantTable,
        residual: VvcSelectedLumaResidual,
        transform_scratch: &mut VvcInverseTransformScratch,
        reconstructed_residual: &mut Vec<i16>,
    ) -> Self {
        let score = vvc_luma_residual_block_score(
            source_residuals,
            node.width,
            node.height,
            bit_depth,
            luma_qp,
            luma_ts_quant,
            residual.block,
            residual.mts_index,
            transform_scratch,
            reconstructed_residual,
        );
        Self { residual, score }
    }

    fn from_scored_block(scored: VvcScoredLumaResidualBlock) -> Self {
        Self {
            residual: VvcSelectedLumaResidual {
                block: scored.block,
                mts_index: scored.mts_index,
            },
            score: scored.score,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcFinalizedLumaTu {
    abs_remainder: u8,
    negative: bool,
    dc_level: i16,
    ac_levels: [i16; VVC_LUMA_AC_COEFFS_PER_TU],
    has_ac: bool,
    transform_skip: bool,
    bdpcm_mode: VvcBdpcmMode,
    mrl_index: u8,
    mts_index: u8,
}

fn finalize_vvc_luma_tu(
    coding_decision: VvcLumaTuCodingDecision,
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    predicted_luma: &[VvcSample],
    residuals: &[i16],
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    preselected_residual: Option<VvcScoredSelectedLumaResidual>,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedLumaTu {
    #[cfg(feature = "vvc-stats")]
    let score_start = Instant::now();
    let selected_residual = match preselected_residual {
        Some(residual) => refine_vvc_luma_final_mts_residual(
            residual.residual,
            coding_decision,
            residuals,
            node,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
            stats,
            transform_scratch,
            reconstructed_residual,
        ),
        None => {
            let (block, mts_index) = select_vvc_luma_residual_block_with_mts(
                coding_decision.residual_coding,
                coding_decision.mts_index,
                residuals,
                node.width,
                node.height,
                source_frame.format.bit_depth,
                luma_qp,
                luma_ts_quant,
                true,
                VvcLumaResidualQuantizationSearch::Full,
                stats,
                transform_scratch,
                reconstructed_residual,
            );
            VvcSelectedLumaResidual { block, mts_index }
        }
    };
    #[cfg(feature = "vvc-stats")]
    stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
    let residual = selected_residual.block;
    let mts_index = selected_residual.mts_index;
    #[cfg(feature = "vvc-stats")]
    let recon_start = Instant::now();
    reconstruct_vvc_luma_residual_block_into(
        residual,
        mts_index,
        reconstructed_residual,
        transform_scratch,
        node.width,
        node.height,
        source_frame.format.bit_depth,
        luma_qp,
        luma_ts_quant,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_luma_residual_recon_nanos(vvc_elapsed_nanos(recon_start));
    #[cfg(feature = "vvc-stats")]
    let fill_start = Instant::now();
    fill_visible_luma_node(
        &mut frame_recon.luma,
        source_frame.geometry,
        node,
        predicted_luma,
        reconstructed_residual,
        source_frame.format.bit_depth,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_luma_fill_nanos(vvc_elapsed_nanos(fill_start));
    let finalized = VvcFinalizedLumaTu {
        abs_remainder: residual.abs_remainder(),
        negative: residual.negative(),
        dc_level: residual.dc_level,
        ac_levels: residual.ac_levels,
        has_ac: residual.has_ac,
        transform_skip: residual.transform_skip,
        bdpcm_mode: residual.bdpcm_mode,
        mrl_index: coding_decision.mrl_index,
        mts_index,
    };
    frame_recon.mark_luma_node_available(node);
    finalized
}

fn refine_vvc_luma_final_mts_residual(
    selected: VvcSelectedLumaResidual,
    coding_decision: VvcLumaTuCodingDecision,
    residuals: &[i16],
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcSelectedLumaResidual {
    if selected.block.transform_skip
        || !vvc_luma_mts_selection_allowed(
            coding_decision.residual_coding,
            coding_decision.mts_index,
            node.width,
            node.height,
            luma_qp,
            selected.block.has_ac,
        )
    {
        return selected;
    }
    let (block, mts_index) = select_vvc_luma_residual_block_with_mts(
        coding_decision.residual_coding,
        coding_decision.mts_index,
        residuals,
        node.width,
        node.height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        true,
        VvcLumaResidualQuantizationSearch::Full,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    VvcSelectedLumaResidual { block, mts_index }
}

fn select_vvc_luma_residual_block_with_mts(
    residual_coding: VvcTuResidualCodingMode,
    requested_mts_index: u8,
    residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    allow_explicit_mts: bool,
    quantization_search: VvcLumaResidualQuantizationSearch,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> (VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>, u8) {
    let best = select_vvc_scored_luma_residual_block_with_mts(
        residual_coding,
        requested_mts_index,
        residuals,
        width,
        height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        allow_explicit_mts,
        quantization_search,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    (best.block, best.mts_index)
}

fn select_vvc_scored_luma_residual_block_with_mts(
    residual_coding: VvcTuResidualCodingMode,
    requested_mts_index: u8,
    residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    allow_explicit_mts: bool,
    quantization_search: VvcLumaResidualQuantizationSearch,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcScoredLumaResidualBlock {
    if matches!(residual_coding, VvcTuResidualCodingMode::TransformSkip) {
        let block = finalize_vvc_luma_residual_block(
            residual_coding,
            0,
            residuals,
            width,
            height,
            bit_depth,
            luma_qp,
            luma_ts_quant,
            quantization_search,
            stats,
            transform_scratch,
            reconstructed_residual,
        );
        return VvcScoredLumaResidualBlock::new(
            residuals,
            width,
            height,
            bit_depth,
            luma_qp,
            luma_ts_quant,
            block,
            0,
            transform_scratch,
            reconstructed_residual,
        );
    }

    let transform_skip = select_vvc_scored_luma_transform_skip_candidate(
        residual_coding,
        residuals,
        width,
        height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    if let Some(transform_skip) = transform_skip {
        if vvc_transform_skip_short_circuits_transformed(transform_skip.score) {
            return transform_skip;
        }
    }

    let base = finalize_vvc_luma_residual_block(
        residual_coding,
        0,
        residuals,
        width,
        height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        quantization_search,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    let mut best = VvcScoredLumaResidualBlock::new(
        residuals,
        width,
        height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        base,
        0,
        transform_scratch,
        reconstructed_residual,
    );

    if let Some(transform_skip) = transform_skip {
        if transform_skip.selects_over(best) {
            best = transform_skip;
        }
    }

    if allow_explicit_mts
        && vvc_luma_mts_selection_allowed(
            residual_coding,
            requested_mts_index,
            width,
            height,
            luma_qp,
            base.has_ac,
        )
    {
        if matches!(requested_mts_index, 2..=5) {
            let candidate = finalize_vvc_luma_residual_block(
                residual_coding,
                requested_mts_index,
                residuals,
                width,
                height,
                bit_depth,
                luma_qp,
                luma_ts_quant,
                quantization_search,
                stats,
                transform_scratch,
                reconstructed_residual,
            );
            let candidate = VvcScoredLumaResidualBlock::new(
                residuals,
                width,
                height,
                bit_depth,
                luma_qp,
                luma_ts_quant,
                candidate,
                requested_mts_index,
                transform_scratch,
                reconstructed_residual,
            );
            if candidate.selects_over(best) {
                best = candidate;
            }
        } else {
            for candidate_mts_index in VVC_LUMA_EXPLICIT_MTS_CANDIDATES {
                let candidate = finalize_vvc_luma_residual_block(
                    residual_coding,
                    candidate_mts_index,
                    residuals,
                    width,
                    height,
                    bit_depth,
                    luma_qp,
                    luma_ts_quant,
                    quantization_search,
                    stats,
                    transform_scratch,
                    reconstructed_residual,
                );
                if !candidate.has_ac && candidate.dc_level == 0 {
                    continue;
                }
                let candidate = VvcScoredLumaResidualBlock::new(
                    residuals,
                    width,
                    height,
                    bit_depth,
                    luma_qp,
                    luma_ts_quant,
                    candidate,
                    candidate_mts_index,
                    transform_scratch,
                    reconstructed_residual,
                );
                if candidate.selects_over(best) {
                    best = candidate;
                }
            }
        }
    }

    best
}

#[derive(Debug, Clone, Copy)]
struct VvcScoredLumaResidualBlock {
    block: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    mts_index: u8,
    score: VvcResidualBlockScore,
}

impl VvcScoredLumaResidualBlock {
    fn new(
        residuals: &[i16],
        width: u16,
        height: u16,
        bit_depth: SampleBitDepth,
        luma_qp: i32,
        luma_ts_quant: &VvcTransformSkipQuantTable,
        block: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
        mts_index: u8,
        transform_scratch: &mut VvcInverseTransformScratch,
        reconstructed_residual: &mut Vec<i16>,
    ) -> Self {
        let score = vvc_luma_residual_block_score(
            residuals,
            width,
            height,
            bit_depth,
            luma_qp,
            luma_ts_quant,
            block,
            mts_index,
            transform_scratch,
            reconstructed_residual,
        );
        Self {
            block,
            mts_index,
            score,
        }
    }

    fn selects_over(self, best: Self) -> bool {
        self.score.selects_quality_over(best.score)
    }
}

fn vvc_luma_mts_selection_allowed(
    residual_coding: VvcTuResidualCodingMode,
    requested_mts_index: u8,
    width: u16,
    height: u16,
    luma_qp: i32,
    base_has_ac: bool,
) -> bool {
    let valid_request = requested_mts_index == 0 || matches!(requested_mts_index, 2..=5);
    VVC_ENABLE_LUMA_MTS_SELECTION
        && valid_request
        && matches!(residual_coding, VvcTuResidualCodingMode::Transformed)
        && luma_qp > 0
        && base_has_ac
        && matches!(width, 4 | 8)
        && matches!(height, 4 | 8)
}

fn select_vvc_scored_luma_transform_skip_candidate(
    residual_coding: VvcTuResidualCodingMode,
    residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> Option<VvcScoredLumaResidualBlock> {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    if !vvc_luma_lossy_transform_skip_selection_allowed(residual_coding, width, height, luma_qp) {
        return None;
    }

    #[cfg(feature = "vvc-stats")]
    let quant_start = Instant::now();
    let transform_skip =
        finalize_vvc_luma_transform_skip_residual_block(residuals, width, height, luma_ts_quant);
    #[cfg(feature = "vvc-stats")]
    stats.add_luma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
    if !transform_skip.has_ac && transform_skip.dc_level == 0 {
        return None;
    }

    let transform_skip = VvcScoredLumaResidualBlock::new(
        residuals,
        width,
        height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        transform_skip,
        0,
        transform_scratch,
        reconstructed_residual,
    );
    Some(transform_skip)
}

fn vvc_luma_lossy_transform_skip_selection_allowed(
    residual_coding: VvcTuResidualCodingMode,
    width: u16,
    height: u16,
    luma_qp: i32,
) -> bool {
    VVC_ENABLE_LOSSY_TRANSFORM_SKIP_SELECTION
        && matches!(residual_coding, VvcTuResidualCodingMode::Transformed)
        && luma_qp > 0
        && width <= VVC_TRANSFORM_SKIP_MAX_SIZE
        && height <= VVC_TRANSFORM_SKIP_MAX_SIZE
}

fn vvc_luma_residual_block_score(
    source_residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    qp: i32,
    ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    mts_index: u8,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcResidualBlockScore {
    let distortion = luma_reconstructed_residual_sse(
        source_residuals,
        width,
        height,
        bit_depth,
        qp,
        ts_quant,
        residual,
        mts_index,
        transform_scratch,
        reconstructed_residual,
    );
    let rate_cost = u64::from(residual.dc_level != 0)
        .saturating_mul(8)
        .saturating_add(luma_ac_syntax_cost_estimate(
            width,
            height,
            &residual.ac_levels,
        ))
        .saturating_add(luma_mts_syntax_cost_estimate(
            residual.has_ac && !residual.transform_skip,
            mts_index,
        ))
        .saturating_add(u64::from(residual.transform_skip));
    VvcResidualBlockScore {
        distortion,
        rate_cost,
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcResidualBlockScore {
    distortion: u64,
    rate_cost: u64,
}

impl VvcResidualBlockScore {
    fn selects_quality_over(self, best: Self) -> bool {
        vvc_quality_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

fn vvc_transform_skip_short_circuits_transformed(score: VvcResidualBlockScore) -> bool {
    score.distortion == 0
}

fn luma_mts_syntax_cost_estimate(has_ac: bool, mts_index: u8) -> u64 {
    if !has_ac {
        return 0;
    }
    match mts_index {
        0 => 1,
        2 => 2,
        3 => 3,
        4 | 5 => 4,
        _ => 8,
    }
}

fn finalize_vvc_luma_residual_block(
    residual_coding: VvcTuResidualCodingMode,
    mts_index: u8,
    residuals: &[i16],
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    quantization_search: VvcLumaResidualQuantizationSearch,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU> {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    match residual_coding {
        VvcTuResidualCodingMode::TransformSkip => {
            debug_assert_eq!(mts_index, 0);
            #[cfg(feature = "vvc-stats")]
            let quant_start = Instant::now();
            let block = finalize_vvc_luma_transform_skip_residual_block(
                residuals,
                width,
                height,
                luma_ts_quant,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
            block
        }
        VvcTuResidualCodingMode::Transformed => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = Instant::now();
            let quantized = match quantization_search {
                VvcLumaResidualQuantizationSearch::Full => {
                    quantize_vvc_luma_residual_greedy_with_qp_and_mts_into(
                        residuals,
                        width,
                        height,
                        bit_depth,
                        luma_qp,
                        mts_index,
                        transform_scratch,
                        reconstructed_residual,
                    )
                }
                VvcLumaResidualQuantizationSearch::FastModeDecision => {
                    quantize_vvc_luma_residual_fast_with_qp_and_mts_into(
                        residuals,
                        width,
                        height,
                        bit_depth,
                        luma_qp,
                        mts_index,
                        transform_scratch,
                        reconstructed_residual,
                    )
                }
            };
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_transformed_quant_nanos(vvc_elapsed_nanos(quant_start));
            VvcFinalizedResidualBlock {
                dc_level: quantized.reconstructed_dc_coeff,
                ac_levels: quantized.reconstructed_ac_coeffs,
                has_ac: quantized.has_ac,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            }
        }
    }
}

fn reconstruct_vvc_luma_residual_block_into(
    residual: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    mts_index: u8,
    reconstructed_residual: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
) {
    if residual.transform_skip {
        if residual.bdpcm_mode.is_enabled() {
            reconstruct_vvc_luma_bdpcm_transform_skip_residuals_into_with_table(
                reconstructed_residual,
                residual.dc_level,
                &residual.ac_levels,
                usize::from(width),
                usize::from(height),
                luma_ts_quant,
                residual.bdpcm_mode,
            );
        } else {
            reconstruct_vvc_luma_transform_skip_residuals_into_with_table(
                reconstructed_residual,
                residual.dc_level,
                &residual.ac_levels,
                usize::from(width),
                usize::from(height),
                luma_ts_quant,
            );
        }
    } else {
        inverse_transform_vvc_luma_quantized_block_into_with_qp_and_mts(
            reconstructed_residual,
            transform_scratch,
            width,
            height,
            residual.dc_level,
            &residual.ac_levels,
            bit_depth,
            luma_qp,
            mts_index,
        );
    }
}

fn finalize_vvc_luma_transform_skip_residual_block(
    residuals: &[i16],
    width: u16,
    height: u16,
    quant_table: &VvcTransformSkipQuantTable,
) -> VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU> {
    debug_assert_eq!(residuals.len(), usize::from(width) * usize::from(height));
    let dc_level = residuals
        .first()
        .copied()
        .map(|level| quant_table.level(level))
        .unwrap_or(0);
    let (ac_levels, has_ac) = transform_skip_luma_ac_levels_and_flag_with_table(
        residuals,
        usize::from(width),
        quant_table,
    );
    VvcFinalizedResidualBlock {
        dc_level,
        ac_levels,
        has_ac,
        transform_skip: true,
        bdpcm_mode: VvcBdpcmMode::None,
    }
}

fn finalize_vvc_luma_bdpcm_transform_skip_residual_block(
    residuals: &[i16],
    width: u16,
    height: u16,
    quant_table: &VvcTransformSkipQuantTable,
    bdpcm_mode: VvcBdpcmMode,
) -> VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU> {
    debug_assert!(bdpcm_mode.is_enabled());
    debug_assert_eq!(residuals.len(), usize::from(width) * usize::from(height));
    let (active_width, active_height) =
        vvc_luma_transform_skip_active_extent(usize::from(width), usize::from(height));
    let mut quantized_levels = [0i16; 64];
    let mut ac_levels = [0; VVC_LUMA_AC_COEFFS_PER_TU];
    let mut dc_level = 0i16;
    let mut has_ac = false;
    for y in 0..active_height {
        for x in 0..active_width {
            let level = quant_table.level(residuals[y * usize::from(width) + x]);
            quantized_levels[y * active_width + x] = level;
            let predictor = match bdpcm_mode {
                VvcBdpcmMode::None => unreachable!("BDPCM block requires a direction"),
                VvcBdpcmMode::Horizontal if x > 0 => quantized_levels[y * active_width + x - 1],
                VvcBdpcmMode::Vertical if y > 0 => quantized_levels[(y - 1) * active_width + x],
                VvcBdpcmMode::Horizontal | VvcBdpcmMode::Vertical => 0,
            };
            let coeff = (i32::from(level) - i32::from(predictor))
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
            if x == 0 && y == 0 {
                dc_level = coeff;
            } else {
                ac_levels[y * active_width + x - 1] = coeff;
                has_ac |= coeff != 0;
            }
        }
    }
    VvcFinalizedResidualBlock {
        dc_level,
        ac_levels,
        has_ac,
        transform_skip: true,
        bdpcm_mode,
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcFinalizedChromaTu {
    cb_dc_level: i16,
    cr_dc_level: i16,
    cb_ac_levels: [i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    cr_ac_levels: [i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    cb_has_ac: bool,
    cr_has_ac: bool,
    cb_transform_skip: bool,
    cr_transform_skip: bool,
    bdpcm_mode: VvcBdpcmMode,
}

#[derive(Debug, Clone, Copy)]
struct VvcSelectedChromaResidual {
    cb: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    cr: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
}

#[derive(Debug, Clone, Copy)]
struct VvcScoredSelectedChromaResidual {
    residual: VvcSelectedChromaResidual,
    score: VvcResidualBlockScore,
}

impl VvcScoredSelectedChromaResidual {
    fn new(
        cb_residuals: &[i16],
        cr_residuals: &[i16],
        width: usize,
        height: usize,
        bit_depth: SampleBitDepth,
        chroma_qp: i32,
        chroma_ts_quant: &VvcTransformSkipQuantTable,
        residual: VvcSelectedChromaResidual,
        transform_scratch: &mut VvcInverseTransformScratch,
        reconstructed_residual: &mut Vec<i16>,
    ) -> Self {
        let cb_score = vvc_chroma_residual_block_score(
            cb_residuals,
            width,
            height,
            bit_depth,
            chroma_qp,
            chroma_ts_quant,
            residual.cb,
            transform_scratch,
            reconstructed_residual,
        );
        let cr_score = vvc_chroma_residual_block_score(
            cr_residuals,
            width,
            height,
            bit_depth,
            chroma_qp,
            chroma_ts_quant,
            residual.cr,
            transform_scratch,
            reconstructed_residual,
        );
        Self {
            residual,
            score: VvcResidualBlockScore {
                distortion: cb_score.distortion.saturating_add(cr_score.distortion),
                rate_cost: chroma_coeff_syntax_cost_estimate(width, height, residual.cb)
                    .saturating_add(chroma_coeff_syntax_cost_estimate(
                        width,
                        height,
                        residual.cr,
                    )),
            },
        }
    }
}

fn finalize_vvc_chroma_tu(
    coding_decision: VvcChromaTuCodingDecision,
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
    cb_residuals: &[i16],
    cr_residuals: &[i16],
    chroma_width: usize,
    chroma_height: usize,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    preselected_residual: Option<VvcScoredSelectedChromaResidual>,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedChromaTu {
    #[cfg(feature = "vvc-stats")]
    let score_start = Instant::now();
    let selected_residual = preselected_residual
        .map(|residual| residual.residual)
        .unwrap_or_else(|| VvcSelectedChromaResidual {
            cb: finalize_vvc_chroma_residual_block(
                coding_decision.residual_coding,
                cb_residuals,
                chroma_width,
                chroma_height,
                source_frame.format.bit_depth,
                chroma_qp,
                chroma_ts_quant,
                stats,
            ),
            cr: finalize_vvc_chroma_residual_block(
                coding_decision.residual_coding,
                cr_residuals,
                chroma_width,
                chroma_height,
                source_frame.format.bit_depth,
                chroma_qp,
                chroma_ts_quant,
                stats,
            ),
        });
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
    let cb_residual = selected_residual.cb;
    let cr_residual = selected_residual.cr;
    #[cfg(feature = "vvc-stats")]
    let recon_start = Instant::now();
    reconstruct_vvc_chroma_residual_block_into(
        cb_residual,
        reconstructed_residual,
        transform_scratch,
        chroma_width,
        chroma_height,
        source_frame.format.bit_depth,
        chroma_qp,
        chroma_ts_quant,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_residual_recon_nanos(vvc_elapsed_nanos(recon_start));
    #[cfg(feature = "vvc-stats")]
    let fill_start = Instant::now();
    fill_visible_chroma_node(
        &mut frame_recon.cb,
        source_frame.geometry,
        node,
        source_frame.format.chroma_sampling,
        predicted_cb,
        reconstructed_residual,
        source_frame.format.bit_depth,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_fill_nanos(vvc_elapsed_nanos(fill_start));
    #[cfg(feature = "vvc-stats")]
    let recon_start = Instant::now();
    reconstruct_vvc_chroma_residual_block_into(
        cr_residual,
        reconstructed_residual,
        transform_scratch,
        chroma_width,
        chroma_height,
        source_frame.format.bit_depth,
        chroma_qp,
        chroma_ts_quant,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_residual_recon_nanos(vvc_elapsed_nanos(recon_start));
    #[cfg(feature = "vvc-stats")]
    let fill_start = Instant::now();
    fill_visible_chroma_node(
        &mut frame_recon.cr,
        source_frame.geometry,
        node,
        source_frame.format.chroma_sampling,
        predicted_cr,
        reconstructed_residual,
        source_frame.format.bit_depth,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_fill_nanos(vvc_elapsed_nanos(fill_start));
    let finalized = VvcFinalizedChromaTu {
        cb_dc_level: cb_residual.dc_level,
        cr_dc_level: cr_residual.dc_level,
        cb_ac_levels: cb_residual.ac_levels,
        cr_ac_levels: cr_residual.ac_levels,
        cb_has_ac: cb_residual.has_ac,
        cr_has_ac: cr_residual.has_ac,
        cb_transform_skip: cb_residual.transform_skip,
        cr_transform_skip: cr_residual.transform_skip,
        bdpcm_mode: cb_residual
            .bdpcm_mode
            .is_enabled()
            .then_some(cb_residual.bdpcm_mode)
            .unwrap_or(cr_residual.bdpcm_mode),
    };
    frame_recon.mark_chroma_node_available(node);
    finalized
}

fn finalize_vvc_chroma_residual_block(
    residual_coding: VvcTuResidualCodingMode,
    residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    match residual_coding {
        VvcTuResidualCodingMode::TransformSkip => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = Instant::now();
            let block = finalize_vvc_chroma_transform_skip_residual_block(
                residuals,
                width,
                height,
                chroma_ts_quant,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
            block
        }
        VvcTuResidualCodingMode::Transformed => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = Instant::now();
            let quantized = quantize_vvc_chroma_residual_greedy_with_qp(
                residuals,
                width as u16,
                height as u16,
                bit_depth,
                chroma_qp,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transformed_quant_nanos(vvc_elapsed_nanos(quant_start));
            let transformed = VvcFinalizedResidualBlock {
                dc_level: quantized.reconstructed_dc_coeff,
                ac_levels: quantized.reconstructed_ac_coeffs,
                has_ac: quantized.has_ac,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            };
            select_vvc_chroma_residual_block_with_transform_skip(
                residual_coding,
                residuals,
                width,
                height,
                bit_depth,
                chroma_qp,
                chroma_ts_quant,
                transformed,
                stats,
            )
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcScoredChromaResidualBlock {
    block: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    score: VvcResidualBlockScore,
}

impl VvcScoredChromaResidualBlock {
    fn new(
        residuals: &[i16],
        width: usize,
        height: usize,
        bit_depth: SampleBitDepth,
        chroma_qp: i32,
        chroma_ts_quant: &VvcTransformSkipQuantTable,
        block: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
        transform_scratch: &mut VvcInverseTransformScratch,
        reconstructed_residual: &mut Vec<i16>,
    ) -> Self {
        let score = vvc_chroma_residual_block_score(
            residuals,
            width,
            height,
            bit_depth,
            chroma_qp,
            chroma_ts_quant,
            block,
            transform_scratch,
            reconstructed_residual,
        );
        Self { block, score }
    }

    fn selects_over(self, best: Self) -> bool {
        self.score.selects_quality_over(best.score)
    }
}

fn select_vvc_scored_chroma_residual_block_with_transform_skip(
    residual_coding: VvcTuResidualCodingMode,
    residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcScoredChromaResidualBlock {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    match residual_coding {
        VvcTuResidualCodingMode::TransformSkip => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = Instant::now();
            let block = finalize_vvc_chroma_transform_skip_residual_block(
                residuals,
                width,
                height,
                chroma_ts_quant,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
            VvcScoredChromaResidualBlock::new(
                residuals,
                width,
                height,
                bit_depth,
                chroma_qp,
                chroma_ts_quant,
                block,
                transform_scratch,
                reconstructed_residual,
            )
        }
        VvcTuResidualCodingMode::Transformed => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = Instant::now();
            let quantized = quantize_vvc_chroma_residual_greedy_with_qp(
                residuals,
                width as u16,
                height as u16,
                bit_depth,
                chroma_qp,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transformed_quant_nanos(vvc_elapsed_nanos(quant_start));
            let transformed = VvcFinalizedResidualBlock {
                dc_level: quantized.reconstructed_dc_coeff,
                ac_levels: quantized.reconstructed_ac_coeffs,
                has_ac: quantized.has_ac,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            };
            let mut best = VvcScoredChromaResidualBlock::new(
                residuals,
                width,
                height,
                bit_depth,
                chroma_qp,
                chroma_ts_quant,
                transformed,
                transform_scratch,
                reconstructed_residual,
            );
            if vvc_chroma_lossy_transform_skip_selection_allowed(
                residual_coding,
                width,
                height,
                chroma_qp,
            ) {
                #[cfg(feature = "vvc-stats")]
                let quant_start = Instant::now();
                let transform_skip = finalize_vvc_chroma_transform_skip_residual_block(
                    residuals,
                    width,
                    height,
                    chroma_ts_quant,
                );
                #[cfg(feature = "vvc-stats")]
                stats.add_chroma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
                if transform_skip.has_ac || transform_skip.dc_level != 0 {
                    let transform_skip = VvcScoredChromaResidualBlock::new(
                        residuals,
                        width,
                        height,
                        bit_depth,
                        chroma_qp,
                        chroma_ts_quant,
                        transform_skip,
                        transform_scratch,
                        reconstructed_residual,
                    );
                    if transform_skip.selects_over(best) {
                        best = transform_skip;
                    }
                }
            }
            best
        }
    }
}

fn select_vvc_chroma_residual_block_with_transform_skip(
    residual_coding: VvcTuResidualCodingMode,
    residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    transformed: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    stats: &mut VvcIntraSearchStats,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    if !vvc_chroma_lossy_transform_skip_selection_allowed(residual_coding, width, height, chroma_qp)
    {
        return transformed;
    }
    let mut scratch = VvcInverseTransformScratch::default();
    let mut reconstructed = Vec::new();

    #[cfg(feature = "vvc-stats")]
    let quant_start = Instant::now();
    let transform_skip = finalize_vvc_chroma_transform_skip_residual_block(
        residuals,
        width,
        height,
        chroma_ts_quant,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
    if !transform_skip.has_ac && transform_skip.dc_level == 0 {
        return transformed;
    }

    let transformed_score = vvc_chroma_residual_block_score(
        residuals,
        width,
        height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
        transformed,
        &mut scratch,
        &mut reconstructed,
    );
    let transform_skip_score = vvc_chroma_residual_block_score(
        residuals,
        width,
        height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
        transform_skip,
        &mut scratch,
        &mut reconstructed,
    );
    if transform_skip_score.selects_quality_over(transformed_score) {
        transform_skip
    } else {
        transformed
    }
}

fn vvc_chroma_lossy_transform_skip_selection_allowed(
    residual_coding: VvcTuResidualCodingMode,
    width: usize,
    height: usize,
    chroma_qp: i32,
) -> bool {
    VVC_ENABLE_LOSSY_TRANSFORM_SKIP_SELECTION
        && matches!(residual_coding, VvcTuResidualCodingMode::Transformed)
        && chroma_qp > 0
        && width <= usize::from(VVC_TRANSFORM_SKIP_MAX_SIZE)
        && height <= usize::from(VVC_TRANSFORM_SKIP_MAX_SIZE)
}

fn vvc_chroma_residual_block_score(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcResidualBlockScore {
    let distortion = chroma_reconstructed_residual_sse(
        source_residuals,
        width,
        height,
        bit_depth,
        qp,
        chroma_ts_quant,
        residual,
        transform_scratch,
        reconstructed_residual,
    );
    let rate_cost = u64::from(residual.dc_level != 0)
        .saturating_mul(8)
        .saturating_add(chroma_coeff_syntax_cost_estimate(width, height, residual))
        .saturating_add(u64::from(residual.transform_skip));
    VvcResidualBlockScore {
        distortion,
        rate_cost,
    }
}

fn finalize_vvc_chroma_transform_skip_residual_block(
    residuals: &[i16],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    debug_assert_eq!(residuals.len(), width * height);
    let dc_level = residuals
        .first()
        .copied()
        .map(|level| quant_table.level(level))
        .unwrap_or(0);
    let (ac_levels, has_ac) =
        transform_skip_chroma_ac_levels_and_flag_with_table(residuals, width, quant_table);
    VvcFinalizedResidualBlock {
        dc_level,
        ac_levels,
        has_ac,
        transform_skip: true,
        bdpcm_mode: VvcBdpcmMode::None,
    }
}

fn finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
    residuals: &[i16],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
    bdpcm_mode: VvcBdpcmMode,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    debug_assert!(bdpcm_mode.is_enabled());
    debug_assert_eq!(residuals.len(), width * height);
    let active_width = width.min(4);
    let active_height = height.min(4);
    let mut quantized_levels = [0i16; 16];
    let mut ac_levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
    let mut dc_level = 0i16;
    let mut has_ac = false;
    for y in 0..active_height {
        for x in 0..active_width {
            let level = quant_table.level(residuals[y * width + x]);
            quantized_levels[y * 4 + x] = level;
            let predictor = match bdpcm_mode {
                VvcBdpcmMode::None => unreachable!("BDPCM block requires a direction"),
                VvcBdpcmMode::Horizontal if x > 0 => quantized_levels[y * 4 + x - 1],
                VvcBdpcmMode::Vertical if y > 0 => quantized_levels[(y - 1) * 4 + x],
                VvcBdpcmMode::Horizontal | VvcBdpcmMode::Vertical => 0,
            };
            let coeff = (i32::from(level) - i32::from(predictor))
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
            if x == 0 && y == 0 {
                dc_level = coeff;
            } else {
                let slot = y * 4 + x - 1;
                ac_levels[slot] = coeff;
                has_ac |= coeff != 0;
            }
        }
    }
    VvcFinalizedResidualBlock {
        dc_level,
        ac_levels,
        has_ac,
        transform_skip: true,
        bdpcm_mode,
    }
}

fn reconstruct_vvc_chroma_residual_block_into(
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    reconstructed_residual: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
) {
    if residual.transform_skip {
        if residual.bdpcm_mode.is_enabled() {
            reconstruct_vvc_chroma_bdpcm_transform_skip_residuals_into_with_table(
                reconstructed_residual,
                residual.dc_level,
                &residual.ac_levels,
                width,
                height,
                chroma_ts_quant,
                residual.bdpcm_mode,
            );
        } else {
            reconstruct_vvc_chroma_transform_skip_residuals_into_with_table(
                reconstructed_residual,
                residual.dc_level,
                &residual.ac_levels,
                width,
                height,
                chroma_ts_quant,
            );
        }
    } else {
        inverse_transform_vvc_chroma_quantized_block_into_with_qp(
            reconstructed_residual,
            transform_scratch,
            width as u16,
            height as u16,
            residual.dc_level,
            &residual.ac_levels,
            bit_depth,
            chroma_qp,
        );
    }
}

fn vvc_global_ctu_node(mut node: VvcCodingTreeNode, region: VvcCtuRegion) -> VvcCodingTreeNode {
    node.x += region.origin_x as u16;
    node.y += region.origin_y as u16;
    node
}

fn predict_vvc_chroma_mode_block_into_with_availability(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcChromaIntraPredictionMode,
    co_located_luma_mode: VvcIntraPredictionMode,
    chroma: &[VvcSample],
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    chroma_availability: Option<super::VvcPlaneAvailability<'_>>,
    luma_availability: Option<super::VvcPlaneAvailability<'_>>,
) {
    match mode {
        VvcChromaIntraPredictionMode::Derived => {
            predict_vvc_chroma_intra_block_into_with_availability(
                prediction,
                scratch,
                co_located_luma_mode,
                chroma,
                geometry,
                node,
                chroma_sampling,
                bit_depth,
                chroma_availability,
            );
        }
        VvcChromaIntraPredictionMode::Explicit(mode) => {
            predict_vvc_chroma_intra_block_into_with_availability(
                prediction,
                scratch,
                mode,
                chroma,
                geometry,
                node,
                chroma_sampling,
                bit_depth,
                chroma_availability,
            );
        }
        VvcChromaIntraPredictionMode::Cclm(cclm_mode) => {
            predict_vvc_chroma_cclm_block_into_with_availability(
                prediction,
                cclm_mode,
                chroma,
                luma,
                geometry,
                node,
                chroma_sampling,
                bit_depth,
                chroma_availability,
                luma_availability,
            );
        }
    }
}

fn predict_vvc_chroma_mode_pair_blocks_into_with_availability(
    cb_prediction: &mut Vec<VvcSample>,
    cr_prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcChromaIntraPredictionMode,
    co_located_luma_mode: VvcIntraPredictionMode,
    cb: &[VvcSample],
    cr: &[VvcSample],
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    cb_availability: Option<super::VvcPlaneAvailability<'_>>,
    cr_availability: Option<super::VvcPlaneAvailability<'_>>,
    luma_availability: Option<super::VvcPlaneAvailability<'_>>,
) {
    if let VvcChromaIntraPredictionMode::Cclm(cclm_mode) = mode {
        predict_vvc_chroma_cclm_pair_into_with_availability(
            cb_prediction,
            cr_prediction,
            scratch,
            cclm_mode,
            cb,
            cr,
            luma,
            geometry,
            node,
            chroma_sampling,
            bit_depth,
            cb_availability,
            cr_availability,
            luma_availability,
        );
        return;
    }
    predict_vvc_chroma_mode_block_into_with_availability(
        cb_prediction,
        scratch,
        mode,
        co_located_luma_mode,
        cb,
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        cb_availability,
        luma_availability,
    );
    predict_vvc_chroma_mode_block_into_with_availability(
        cr_prediction,
        scratch,
        mode,
        co_located_luma_mode,
        cr,
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        cr_availability,
        luma_availability,
    );
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_luma_transform_skip_residuals_into(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
) {
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    residuals[0] = dc_level;
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            residuals[y * width + x] = ac_levels[y * active_width + x - 1];
        }
    }
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_luma_transform_skip_residuals_into_with_qp(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
) {
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    residuals[0] = reconstruct_vvc_transform_skip_level_with_params(dc_level, scale, right_shift);
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            residuals[y * width + x] = reconstruct_vvc_transform_skip_level_with_params(
                ac_levels[y * active_width + x - 1],
                scale,
                right_shift,
            );
        }
    }
}

fn reconstruct_vvc_luma_transform_skip_residuals_into_with_table(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
) {
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    residuals[0] = quant_table.reconstructed(dc_level);
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            residuals[y * width + x] =
                quant_table.reconstructed(ac_levels[y * active_width + x - 1]);
        }
    }
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_luma_bdpcm_transform_skip_residuals_into_with_qp(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
    bdpcm_mode: VvcBdpcmMode,
) {
    debug_assert!(bdpcm_mode.is_enabled());
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let (active_width, active_height) = vvc_luma_transform_skip_active_extent(width, height);
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    let mut levels = [0i16; 64];
    levels[0] = dc_level;
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            levels[y * active_width + x] = ac_levels[y * active_width + x - 1];
        }
    }
    inverse_bdpcm_quantized_levels_in_place(&mut levels, active_width, active_height, bdpcm_mode);
    for y in 0..active_height {
        for x in 0..active_width {
            residuals[y * width + x] = reconstruct_vvc_transform_skip_level_with_params(
                levels[y * active_width + x],
                scale,
                right_shift,
            );
        }
    }
}

fn reconstruct_vvc_luma_bdpcm_transform_skip_residuals_into_with_table(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; super::VVC_LUMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
    bdpcm_mode: VvcBdpcmMode,
) {
    debug_assert!(bdpcm_mode.is_enabled());
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let (active_width, active_height) = vvc_luma_transform_skip_active_extent(width, height);
    let mut levels = [0i16; 64];
    levels[0] = dc_level;
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            levels[y * active_width + x] = ac_levels[y * active_width + x - 1];
        }
    }
    inverse_bdpcm_quantized_levels_in_place(&mut levels, active_width, active_height, bdpcm_mode);
    for y in 0..active_height {
        for x in 0..active_width {
            residuals[y * width + x] = quant_table.reconstructed(levels[y * active_width + x]);
        }
    }
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_chroma_transform_skip_residuals_into(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
) {
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    residuals[0] = dc_level;
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x < width && y < height {
            residuals[y * width + x] = ac_levels[slot];
        }
    }
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_chroma_transform_skip_residuals_into_with_qp(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
) {
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    residuals[0] = reconstruct_vvc_transform_skip_level_with_params(dc_level, scale, right_shift);
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x < width && y < height {
            residuals[y * width + x] = reconstruct_vvc_transform_skip_level_with_params(
                ac_levels[slot],
                scale,
                right_shift,
            );
        }
    }
}

fn reconstruct_vvc_chroma_transform_skip_residuals_into_with_table(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
) {
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    residuals[0] = quant_table.reconstructed(dc_level);
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x < width && y < height {
            residuals[y * width + x] = quant_table.reconstructed(ac_levels[slot]);
        }
    }
}

#[cfg(test)]
pub(in crate::vvc) fn reconstruct_vvc_chroma_bdpcm_transform_skip_residuals_into_with_qp(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
    bdpcm_mode: VvcBdpcmMode,
) {
    debug_assert!(bdpcm_mode.is_enabled());
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let active_width = width.min(4);
    let active_height = height.min(4);
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    let mut levels = [0i16; 16];
    levels[0] = dc_level;
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x < active_width && y < active_height {
            levels[y * 4 + x] = ac_levels[slot];
        }
    }
    inverse_bdpcm_quantized_levels_in_place(&mut levels, 4, active_height, bdpcm_mode);
    for y in 0..active_height {
        for x in 0..active_width {
            residuals[y * width + x] = reconstruct_vvc_transform_skip_level_with_params(
                levels[y * 4 + x],
                scale,
                right_shift,
            );
        }
    }
}

fn reconstruct_vvc_chroma_bdpcm_transform_skip_residuals_into_with_table(
    residuals: &mut Vec<i16>,
    dc_level: i16,
    ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
    bdpcm_mode: VvcBdpcmMode,
) {
    debug_assert!(bdpcm_mode.is_enabled());
    residuals.clear();
    residuals.resize(width * height, 0);
    if residuals.is_empty() {
        return;
    }
    let active_width = width.min(4);
    let active_height = height.min(4);
    let mut levels = [0i16; 16];
    levels[0] = dc_level;
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        if x < active_width && y < active_height {
            levels[y * 4 + x] = ac_levels[slot];
        }
    }
    inverse_bdpcm_quantized_levels_in_place(&mut levels, 4, active_height, bdpcm_mode);
    for y in 0..active_height {
        for x in 0..active_width {
            residuals[y * width + x] = quant_table.reconstructed(levels[y * 4 + x]);
        }
    }
}

#[cfg(test)]
pub(in crate::vvc) fn transform_skip_luma_ac_levels_and_flag(
    residuals: &[i16],
    width: usize,
) -> ([i16; super::VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let mut levels = [0; super::VVC_LUMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    let height = residuals.len() / width;
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            let raster_idx = y * width + x;
            if raster_idx < residuals.len() {
                let level = residuals[raster_idx];
                levels[y * active_width + x - 1] = level;
                has_ac |= level != 0;
            }
        }
    }
    (levels, has_ac)
}

#[cfg(test)]
pub(in crate::vvc) fn transform_skip_luma_ac_levels_and_flag_with_qp(
    residuals: &[i16],
    width: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
) -> ([i16; super::VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    transform_skip_luma_ac_levels_and_flag_with_params(residuals, width, scale, right_shift)
}

#[cfg(test)]
fn transform_skip_luma_ac_levels_and_flag_with_params(
    residuals: &[i16],
    width: usize,
    scale: i32,
    right_shift: i32,
) -> ([i16; super::VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let mut levels = [0; super::VVC_LUMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    let height = residuals.len() / width;
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            let raster_idx = y * width + x;
            if raster_idx < residuals.len() {
                let level = quantize_vvc_transform_skip_level_with_params(
                    residuals[raster_idx],
                    scale,
                    right_shift,
                    VVC_TRANSFORM_SKIP_LEVEL_SEARCH_RADIUS,
                );
                levels[y * active_width + x - 1] = level;
                has_ac |= level != 0;
            }
        }
    }
    (levels, has_ac)
}

fn transform_skip_luma_ac_levels_and_flag_with_table(
    residuals: &[i16],
    width: usize,
    quant_table: &VvcTransformSkipQuantTable,
) -> ([i16; super::VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let mut levels = [0; super::VVC_LUMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    let height = residuals.len() / width;
    let active_width = if width == 8 && height == 8 {
        8
    } else {
        width.min(4)
    };
    let active_height = if width == 8 && height == 8 {
        8
    } else {
        height.min(4)
    };
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            let raster_idx = y * width + x;
            if raster_idx < residuals.len() {
                let level = quant_table.level(residuals[raster_idx]);
                levels[y * active_width + x - 1] = level;
                has_ac |= level != 0;
            }
        }
    }
    (levels, has_ac)
}

fn vvc_luma_transform_skip_active_extent(width: usize, height: usize) -> (usize, usize) {
    if width == 8 && height == 8 {
        (8, 8)
    } else {
        (width.min(4), height.min(4))
    }
}

fn inverse_bdpcm_quantized_levels_in_place(
    levels: &mut [i16],
    stride: usize,
    height: usize,
    bdpcm_mode: VvcBdpcmMode,
) {
    match bdpcm_mode {
        VvcBdpcmMode::None => unreachable!("BDPCM inverse requires a direction"),
        VvcBdpcmMode::Horizontal => {
            for y in 0..height {
                let row = y * stride;
                for x in 1..stride {
                    let idx = row + x;
                    levels[idx] = (i32::from(levels[idx]) + i32::from(levels[idx - 1]))
                        .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                        as i16;
                }
            }
        }
        VvcBdpcmMode::Vertical => {
            for y in 1..height {
                let row = y * stride;
                let above = row - stride;
                for x in 0..stride {
                    let idx = row + x;
                    levels[idx] = (i32::from(levels[idx]) + i32::from(levels[above + x]))
                        .clamp(i32::from(i16::MIN), i32::from(i16::MAX))
                        as i16;
                }
            }
        }
    }
}

#[cfg(test)]
pub(in crate::vvc) fn transform_skip_chroma_ac_levels_and_flag(
    residuals: &[i16],
    width: usize,
) -> ([i16; VVC_CHROMA_AC_COEFFS_PER_TU], bool) {
    let mut levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        let raster_idx = y * width + x;
        if raster_idx < residuals.len() {
            let level = residuals[raster_idx];
            levels[slot] = level;
            has_ac |= level != 0;
        }
    }
    (levels, has_ac)
}

fn transform_skip_chroma_ac_levels_and_flag_with_table(
    residuals: &[i16],
    width: usize,
    quant_table: &VvcTransformSkipQuantTable,
) -> ([i16; VVC_CHROMA_AC_COEFFS_PER_TU], bool) {
    let mut levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
    let mut has_ac = false;
    for (slot, (x, y)) in VVC_CHROMA_AC_POSITIONS_4X4.iter().copied().enumerate() {
        let raster_idx = y * width + x;
        if raster_idx < residuals.len() {
            let level = quant_table.level(residuals[raster_idx]);
            levels[slot] = level;
            has_ac |= level != 0;
        }
    }
    (levels, has_ac)
}

#[cfg(test)]
pub(in crate::vvc) fn quantize_vvc_transform_skip_level(
    residual: i16,
    bit_depth: SampleBitDepth,
    qp: i32,
) -> i16 {
    quantize_vvc_transform_skip_level_with_radius(
        residual,
        bit_depth,
        qp,
        VVC_TRANSFORM_SKIP_LEVEL_SEARCH_RADIUS,
    )
}

#[cfg(test)]
fn quantize_vvc_transform_skip_level_with_radius(
    residual: i16,
    bit_depth: SampleBitDepth,
    qp: i32,
    search_radius: i64,
) -> i16 {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    quantize_vvc_transform_skip_level_with_params(residual, scale, right_shift, search_radius)
}

#[inline]
fn quantize_vvc_transform_skip_level_with_params(
    residual: i16,
    scale: i32,
    right_shift: i32,
    search_radius: i64,
) -> i16 {
    if residual == 0 {
        return 0;
    }
    let estimate = if right_shift > 0 {
        div_round_nearest_i64(i64::from(residual) << right_shift, i64::from(scale))
    } else {
        div_round_nearest_i64(
            i64::from(residual),
            i64::from(scale) << (-right_shift as u32),
        )
    };
    if search_radius == 1 {
        return quantize_vvc_transform_skip_level_radius_one_with_params(
            estimate,
            residual,
            scale,
            right_shift,
        );
    }
    let mut best_level = estimate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
    let mut best_error =
        vvc_transform_skip_level_error_with_params(best_level, residual, scale, right_shift);
    for candidate in (estimate - search_radius)..=(estimate + search_radius) {
        let level = candidate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
        if level == best_level {
            continue;
        }
        let error = vvc_transform_skip_level_error_with_params(level, residual, scale, right_shift);
        if error < best_error
            || (error == best_error && level.unsigned_abs() < best_level.unsigned_abs())
        {
            best_error = error;
            best_level = level;
        }
    }
    best_level
}

#[inline]
fn quantize_vvc_transform_skip_level_radius_one_with_params(
    estimate: i64,
    residual: i16,
    scale: i32,
    right_shift: i32,
) -> i16 {
    let mut best_level = estimate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
    let mut best_error =
        vvc_transform_skip_level_error_with_params(best_level, residual, scale, right_shift);
    for candidate in [estimate - 1, estimate + 1] {
        let level = candidate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
        if level == best_level {
            continue;
        }
        let error = vvc_transform_skip_level_error_with_params(level, residual, scale, right_shift);
        if error < best_error
            || (error == best_error && level.unsigned_abs() < best_level.unsigned_abs())
        {
            best_error = error;
            best_level = level;
        }
    }
    best_level
}

fn div_round_nearest_i64(value: i64, divisor: i64) -> i64 {
    debug_assert!(divisor > 0);
    if value < 0 {
        -(((-value) + (divisor / 2)) / divisor)
    } else {
        (value + (divisor / 2)) / divisor
    }
}

#[inline]
fn vvc_transform_skip_level_error_with_params(
    level: i16,
    residual: i16,
    scale: i32,
    right_shift: i32,
) -> u64 {
    let reconstructed = reconstruct_vvc_transform_skip_level_with_params(level, scale, right_shift);
    let diff = i64::from(residual) - i64::from(reconstructed);
    (diff * diff) as u64
}

#[inline]
fn reconstruct_vvc_transform_skip_level_with_params(
    level: i16,
    scale: i32,
    right_shift: i32,
) -> i16 {
    if level == 0 {
        return 0;
    }
    let value = if right_shift > 0 {
        let add = 1i64 << ((right_shift - 1) as u32);
        (i64::from(level) * i64::from(scale) + add) >> (right_shift as u32)
    } else {
        i64::from(level) * i64::from(scale) * (1i64 << ((-right_shift) as u32))
    };
    value.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
}

fn vvc_transform_skip_dequant_params(bit_depth: SampleBitDepth, qp: i32) -> (i32, i32) {
    let qp_bd_offset = (i32::from(bit_depth.bits()) - 8) * 6;
    let transform_skip_qp = (qp + qp_bd_offset).max(4);
    let qp_rem = transform_skip_qp.rem_euclid(6) as usize;
    let qp_per = transform_skip_qp.div_euclid(6);
    let scale = VVC_TRANSFORM_SKIP_INV_QUANT_SCALES[qp_rem];
    let right_shift = 6 - qp_per;
    (scale, right_shift)
}

pub(in crate::vvc) fn residual_luma_tu_at_into(
    residuals: &mut Vec<i16>,
    frame: &VvcSampledFrame,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted: &[VvcSample],
) {
    debug_assert_eq!(predicted.len(), width * height);
    let copy_width = width.min(frame.geometry.width.saturating_sub(origin_x));
    let copy_height = height.min(frame.geometry.height.saturating_sub(origin_y));
    residuals.clear();
    if copy_width == width && copy_height == height {
        residuals.reserve(predicted.len());
        for y in 0..height {
            let src = (origin_y + y) * frame.geometry.width + origin_x;
            let dst = y * width;
            for (sample, predicted) in frame.luma[src..src + width]
                .iter()
                .zip(&predicted[dst..dst + width])
            {
                residuals.push(vvc_sample_delta_i16(*sample, *predicted));
            }
        }
        debug_assert_eq!(residuals.len(), predicted.len());
        return;
    }
    residuals.extend(
        predicted
            .iter()
            .map(|predicted| vvc_sample_delta_i16(0, *predicted)),
    );
    for y in 0..copy_height {
        let src = (origin_y + y) * frame.geometry.width + origin_x;
        let dst = y * width;
        for ((residual, sample), predicted) in residuals[dst..dst + copy_width]
            .iter_mut()
            .zip(&frame.luma[src..src + copy_width])
            .zip(&predicted[dst..dst + copy_width])
        {
            *residual = vvc_sample_delta_i16(*sample, *predicted);
        }
    }
}

pub(in crate::vvc) fn residual_chroma_tu_at_into(
    residuals: &mut Vec<i16>,
    samples: &[VvcSample],
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted: &[VvcSample],
) {
    debug_assert_eq!(predicted.len(), width * height);
    let chroma_width = geometry.width / chroma_subsample_x(format.chroma_sampling);
    let chroma_height = geometry.height / chroma_subsample_y(format.chroma_sampling);
    let neutral = vvc_neutral_sample(format.bit_depth);
    let copy_width = width.min(chroma_width.saturating_sub(origin_x));
    let copy_height = height.min(chroma_height.saturating_sub(origin_y));
    residuals.clear();
    if copy_width == width && copy_height == height {
        residuals.reserve(predicted.len());
        for y in 0..height {
            let src = (origin_y + y) * chroma_width + origin_x;
            let dst = y * width;
            for (sample, predicted) in samples[src..src + width]
                .iter()
                .zip(&predicted[dst..dst + width])
            {
                residuals.push(vvc_sample_delta_i16(*sample, *predicted));
            }
        }
        debug_assert_eq!(residuals.len(), predicted.len());
        return;
    }
    residuals.extend(
        predicted
            .iter()
            .map(|predicted| vvc_sample_delta_i16(neutral, *predicted)),
    );
    for y in 0..copy_height {
        let src = (origin_y + y) * chroma_width + origin_x;
        let dst = y * width;
        for ((residual, sample), predicted) in residuals[dst..dst + copy_width]
            .iter_mut()
            .zip(&samples[src..src + copy_width])
            .zip(&predicted[dst..dst + copy_width])
        {
            *residual = vvc_sample_delta_i16(*sample, *predicted);
        }
    }
}

fn vvc_sample_delta_i16(sample: VvcSample, predicted: VvcSample) -> i16 {
    (i32::from(sample) - i32::from(predicted)).clamp(i32::from(i16::MIN), i32::from(i16::MAX))
        as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sampled_luma_frame(width: usize, height: usize, luma: Vec<VvcSample>) -> VvcSampledFrame {
        assert_eq!(luma.len(), width * height);
        let format = VvcPictureFormat {
            chroma_sampling: ChromaSampling::Cs420,
            bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
        };
        let chroma_len = (width / 2) * (height / 2);
        VvcSampledFrame {
            geometry: VvcVideoGeometry { width, height },
            format,
            luma,
            cb: vec![128; chroma_len],
            cr: vec![128; chroma_len],
            chroma_len,
        }
    }

    #[test]
    fn vvc_luma_prediction_score_matches_materialized_residual_score() {
        let frame = sampled_luma_frame(4, 4, (0..16).map(|idx| (idx * 7) as VvcSample).collect());
        let node = VvcCodingTreeNode::root(4, 4, VvcTreeType::DualTreeLuma);
        let predicted: Vec<_> = (0..16).map(|idx| (idx * 5 + 3) as VvcSample).collect();
        let mut residuals = Vec::new();
        residual_luma_tu_at_into(&mut residuals, &frame, 0, 0, 4, 4, &predicted);

        assert_eq!(
            luma_prediction_residual_score(VvcResidualScoreMetric::Sad, &frame, node, &predicted),
            residual_mode_selection_score(VvcResidualScoreMetric::Sad, &residuals)
        );
        assert_eq!(
            luma_prediction_residual_score(VvcResidualScoreMetric::Sse, &frame, node, &predicted),
            residual_mode_selection_score(VvcResidualScoreMetric::Sse, &residuals)
        );
    }

    #[test]
    fn vvc_chroma_prediction_score_matches_materialized_residual_score() {
        let mut frame = sampled_luma_frame(4, 4, vec![0; 16]);
        frame.cb = vec![100, 112, 124, 136];
        frame.cr = vec![130, 118, 106, 94];
        let predicted_cb = vec![96, 116, 120, 140];
        let predicted_cr = vec![128, 120, 108, 96];
        let mut cb_residuals = Vec::new();
        let mut cr_residuals = Vec::new();
        residual_chroma_tu_at_into(
            &mut cb_residuals,
            &frame.cb,
            frame.geometry,
            frame.format,
            0,
            0,
            2,
            2,
            &predicted_cb,
        );
        residual_chroma_tu_at_into(
            &mut cr_residuals,
            &frame.cr,
            frame.geometry,
            frame.format,
            0,
            0,
            2,
            2,
            &predicted_cr,
        );

        let direct = chroma_prediction_residual_score(
            VvcResidualScoreMetric::Sse,
            &frame,
            0,
            0,
            2,
            2,
            &predicted_cb,
            &predicted_cr,
        );
        let materialized =
            residual_mode_selection_score(VvcResidualScoreMetric::Sse, &cb_residuals)
                .saturating_add(residual_mode_selection_score(
                    VvcResidualScoreMetric::Sse,
                    &cr_residuals,
                ));
        assert_eq!(direct, materialized);
    }

    #[test]
    fn vvc_source_luma_directional_seed_maps_integer_gradients() {
        let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
        let flat = sampled_luma_frame(8, 8, vec![64; 64]);
        assert_eq!(vvc_source_luma_directional_seed(&flat, node), None);

        let horizontal_ramp = sampled_luma_frame(
            8,
            8,
            (0..8)
                .flat_map(|_| (0..8).map(|x| (x * 16) as VvcSample))
                .collect(),
        );
        assert_eq!(
            vvc_source_luma_directional_seed(&horizontal_ramp, node),
            Some(50)
        );

        let vertical_ramp = sampled_luma_frame(
            8,
            8,
            (0..8)
                .flat_map(|y| (0..8).map(move |_| (y * 16) as VvcSample))
                .collect(),
        );
        assert_eq!(
            vvc_source_luma_directional_seed(&vertical_ramp, node),
            Some(18)
        );

        let diagonal_ramp = sampled_luma_frame(
            8,
            8,
            (0..8)
                .flat_map(|y| (0..8).map(move |x| ((x + y) * 8) as VvcSample))
                .collect(),
        );
        assert_eq!(
            vvc_source_luma_directional_seed(&diagonal_ramp, node),
            Some(34)
        );
    }

    #[test]
    fn vvc_lossy_luma_directional_search_uses_focused_gradient_family() {
        let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
        let frame = sampled_luma_frame(
            8,
            8,
            (0..8)
                .flat_map(|_| (0..8).map(|x| (x * 16) as VvcSample))
                .collect(),
        );
        let state = VvcLumaModeSearchState::new_for_geometry(frame.geometry);
        let lossy_policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy);
        let lossless_policy =
            VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossless);

        let lossy_candidates =
            vvc_luma_directional_search_candidates(lossy_policy, &frame, &state, node);
        let lossy_indexes: Vec<_> = lossy_candidates
            .iter()
            .map(|mode| mode.luma_mode_index())
            .collect();
        let lossless_candidates =
            vvc_luma_directional_search_candidates(lossless_policy, &frame, &state, node);
        let lossless_indexes: Vec<_> = lossless_candidates
            .iter()
            .map(|mode| mode.luma_mode_index())
            .collect();
        assert!(lossy_candidates.count() > VVC_LUMA_NEARBY_DIRECTIONAL_OFFSETS.len());
        assert!(lossy_candidates.count() < lossless_candidates.count());
        for index in [2, 18, 34, 46, 48, 49, 50, 51, 52, 54, 66] {
            assert!(lossy_indexes.contains(&index));
        }
        for index in [18, 34, 50] {
            assert!(lossless_indexes.contains(&index));
        }
    }

    #[test]
    fn vvc_lossy_luma_directional_search_skips_source_seed_for_4x4() {
        let node = VvcCodingTreeNode::root(4, 4, VvcTreeType::DualTreeLuma);
        let frame = sampled_luma_frame(
            4,
            4,
            (0..4)
                .flat_map(|_| (0..4).map(|x| (x * 16) as VvcSample))
                .collect(),
        );
        let state = VvcLumaModeSearchState::new_for_geometry(frame.geometry);
        let lossy_policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy);
        let lossless_policy =
            VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossless);

        let lossy_candidates =
            vvc_luma_directional_search_candidates(lossy_policy, &frame, &state, node);
        let lossy_indexes: Vec<_> = lossy_candidates
            .iter()
            .map(|mode| mode.luma_mode_index())
            .collect();
        assert_eq!(lossy_indexes, vec![18, 50, 34, 2, 66]);

        let lossless_candidates =
            vvc_luma_directional_search_candidates(lossless_policy, &frame, &state, node);
        let lossless_indexes: Vec<_> = lossless_candidates
            .iter()
            .map(|mode| mode.luma_mode_index())
            .collect();
        assert!(lossless_indexes.contains(&48));
        assert!(lossless_indexes.contains(&52));
    }

    #[test]
    fn vvc_lossy_luma_directional_search_keeps_exact_neighbor_modes() {
        let mut target = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
        target.x = 8;
        let frame = sampled_luma_frame(16, 8, vec![64; 128]);
        let mut state = VvcLumaModeSearchState::new_for_geometry(frame.geometry);
        state.mark_node(
            VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma),
            VvcIntraPredictionMode::Angular(42),
        );
        let lossy_policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy);

        let candidates =
            vvc_luma_directional_search_candidates(lossy_policy, &frame, &state, target);
        let indexes: Vec<_> = candidates
            .iter()
            .map(|mode| mode.luma_mode_index())
            .collect();
        assert_eq!(indexes, vec![42, 18, 50, 34, 2, 66]);
    }

    #[test]
    fn vvc_lossy_luma_rd_shortlist_keeps_best_winners() {
        let format = VvcPictureFormat {
            chroma_sampling: ChromaSampling::Cs420,
            bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
        };
        let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy);
        let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
        let costs = VvcLumaIntraCandidateCosts::new(1_000_000)
            .with_candidate(VvcIntraPredictionMode::Planar, Some(200_000))
            .with_candidate(VvcIntraPredictionMode::Horizontal, Some(50_000))
            .with_candidate(VvcIntraPredictionMode::Vertical, Some(75_000))
            .with_candidate(VvcIntraPredictionMode::Angular(34), Some(25_000))
            .with_candidate(VvcIntraPredictionMode::Angular(66), Some(10_000))
            .with_candidate(VvcIntraPredictionMode::Angular(2), Some(125_000))
            .with_candidate(VvcIntraPredictionMode::Angular(10), Some(150_000))
            .with_candidate(VvcIntraPredictionMode::Angular(20), Some(175_000))
            .with_candidate(VvcIntraPredictionMode::Angular(30), Some(180_000))
            .with_candidate(VvcIntraPredictionMode::Angular(34), Some(5_000));

        let shortlist = VvcLumaModeRdShortlist::from_candidate_costs(policy, node, costs);
        let indexes: Vec<_> = shortlist
            .iter()
            .map(|candidate| candidate.mode().luma_mode_index())
            .collect();

        assert_eq!(indexes.len(), VVC_LOSSY_LUMA_RD_WINNER_CANDIDATES);
        assert_eq!(indexes, vec![34, 66, 18, 50, 2]);
        assert!(!indexes.contains(&0));
        assert!(!indexes.contains(&1));
    }

    #[test]
    fn vvc_lossless_luma_rd_shortlist_keeps_all_candidates() {
        let format = VvcPictureFormat {
            chroma_sampling: ChromaSampling::Cs420,
            bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
        };
        let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossless);
        let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
        let costs = VvcLumaIntraCandidateCosts::new(1_000)
            .with_candidate(VvcIntraPredictionMode::Planar, Some(200))
            .with_candidate(VvcIntraPredictionMode::Horizontal, Some(50))
            .with_candidate(VvcIntraPredictionMode::Vertical, Some(75))
            .with_candidate(VvcIntraPredictionMode::Angular(34), Some(25))
            .with_candidate(VvcIntraPredictionMode::Angular(66), Some(10));

        let shortlist = VvcLumaModeRdShortlist::from_candidate_costs(policy, node, costs);
        let indexes: Vec<_> = shortlist
            .iter()
            .map(|candidate| candidate.mode().luma_mode_index())
            .collect();

        assert_eq!(indexes, vec![66, 34, 18, 50, 0, 1]);
    }

    #[test]
    fn vvc_lossy_chroma_rd_shortlist_keeps_best_winners() {
        let format = VvcPictureFormat {
            chroma_sampling: ChromaSampling::Cs420,
            bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
        };
        let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy);
        let costs = VvcChromaIntraCandidateCosts::new(1_000_000)
            .with_candidate(
                VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Planar),
                Some(200_000),
            )
            .with_candidate(
                VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
                Some(50_000),
            )
            .with_candidate(
                VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Vertical),
                Some(75_000),
            )
            .with_candidate(
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
                Some(25_000),
            )
            .with_candidate(
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmLeft),
                Some(10_000),
            )
            .with_candidate(
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmTop),
                Some(125_000),
            );

        let shortlist = VvcChromaModeRdShortlist::from_candidate_costs(policy, costs);
        let modes: Vec<_> = shortlist.iter().map(|candidate| candidate.mode()).collect();

        assert_eq!(modes.len(), VVC_LOSSY_CHROMA_RD_WINNER_CANDIDATES);
        assert_eq!(
            modes,
            vec![
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmLeft),
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
                VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
                VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Vertical),
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmTop),
            ]
        );
    }

    #[test]
    fn vvc_lossless_chroma_rd_shortlist_keeps_all_candidates() {
        let format = VvcPictureFormat {
            chroma_sampling: ChromaSampling::Cs420,
            bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
        };
        let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossless);
        let costs = VvcChromaIntraCandidateCosts::new(1_000)
            .with_candidate(
                VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Planar),
                Some(200),
            )
            .with_candidate(
                VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
                Some(50),
            )
            .with_candidate(
                VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Vertical),
                Some(75),
            )
            .with_candidate(
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
                Some(25),
            )
            .with_candidate(
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmLeft),
                Some(10),
            )
            .with_candidate(
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmTop),
                Some(125),
            );

        let shortlist = VvcChromaModeRdShortlist::from_candidate_costs(policy, costs);
        let modes: Vec<_> = shortlist.iter().map(|candidate| candidate.mode()).collect();

        assert_eq!(
            modes,
            vec![
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmLeft),
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
                VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
                VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Vertical),
                VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmTop),
                VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Planar),
                VvcChromaIntraPredictionMode::Derived,
            ]
        );
    }

    #[test]
    fn vvc_luma_quality_gate_can_spend_bits_for_lower_distortion() {
        let best = VvcLumaQuantizedResidualScore {
            distortion: 1_000,
            rate_cost: 0,
        };
        let candidate = VvcLumaQuantizedResidualScore {
            distortion: 700,
            rate_cost: 20,
        };

        assert!(candidate.selects_over(best));
        assert!(!VvcLumaQuantizedResidualScore {
            distortion: 1_100,
            rate_cost: 0,
        }
        .selects_over(best));

        let best = VvcResidualBlockScore {
            distortion: 1_000,
            rate_cost: 0,
        };
        let candidate = VvcResidualBlockScore {
            distortion: 700,
            rate_cost: 20,
        };

        assert!(candidate.selects_quality_over(best));
        assert!(!VvcResidualBlockScore {
            distortion: 1_100,
            rate_cost: 0,
        }
        .selects_quality_over(best));

        assert!(vvc_transform_skip_short_circuits_transformed(
            VvcResidualBlockScore {
                distortion: 0,
                rate_cost: 128,
            }
        ));
        assert!(!vvc_transform_skip_short_circuits_transformed(
            VvcResidualBlockScore {
                distortion: 1,
                rate_cost: 0,
            }
        ));
    }

    #[test]
    fn vvc_luma_exact_prediction_bypasses_rd() {
        assert!(vvc_luma_exact_prediction_skips_rd(&[0, 0, 0, 0]));
        assert!(!vvc_luma_exact_prediction_skips_rd(&[0, 1, 0, 0]));

        let zero_residual = VvcSelectedLumaResidual {
            block: VvcFinalizedResidualBlock {
                dc_level: 0,
                ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
                has_ac: false,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            },
            mts_index: 0,
        };
        let scored_zero_residual = VvcScoredSelectedLumaResidual {
            residual: zero_residual,
            score: VvcResidualBlockScore {
                distortion: 4,
                rate_cost: 0,
            },
        };
        assert!(vvc_luma_zero_coded_residual_skips_rd(
            scored_zero_residual,
            4,
        ));
        assert!(!vvc_luma_zero_coded_residual_skips_rd(
            VvcScoredSelectedLumaResidual {
                score: VvcResidualBlockScore {
                    distortion: 5,
                    rate_cost: 0,
                },
                ..scored_zero_residual
            },
            4,
        ));

        let nonzero_residual = VvcSelectedLumaResidual {
            block: VvcFinalizedResidualBlock {
                dc_level: 1,
                ..zero_residual.block
            },
            mts_index: 0,
        };
        assert!(!vvc_luma_zero_coded_residual_skips_rd(
            VvcScoredSelectedLumaResidual {
                residual: nonzero_residual,
                score: VvcResidualBlockScore {
                    distortion: 0,
                    rate_cost: 0,
                },
            },
            4,
        ));
    }

    #[test]
    fn vvc_luma_exact_min_syntax_score_stops_mode_search() {
        assert!(vvc_luma_exact_min_syntax_mode_search_done(u64::from(
            VVC_LUMA_MIN_INTRA_MODE_SYNTAX_BINS,
        )));
        assert!(!vvc_luma_exact_min_syntax_mode_search_done(u64::from(
            VVC_LUMA_MIN_INTRA_MODE_SYNTAX_BINS + 1,
        )));

        for (left, above) in [
            (None, None),
            (Some(VvcIntraPredictionMode::Horizontal), None),
            (
                Some(VvcIntraPredictionMode::Horizontal),
                Some(VvcIntraPredictionMode::Vertical),
            ),
            (
                Some(VvcIntraPredictionMode::Angular(34)),
                Some(VvcIntraPredictionMode::Angular(35)),
            ),
        ] {
            let min_bins = (0..=66)
                .map(|index| match index {
                    0 => VvcIntraPredictionMode::Planar,
                    1 => VvcIntraPredictionMode::Dc,
                    18 => VvcIntraPredictionMode::Horizontal,
                    50 => VvcIntraPredictionMode::Vertical,
                    _ => VvcIntraPredictionMode::Angular(index),
                })
                .map(|mode| vvc_luma_intra_mode_syntax_bin_count(mode, left, above))
                .min()
                .expect("luma has intra mode candidates");
            assert_eq!(min_bins, VVC_LUMA_MIN_INTRA_MODE_SYNTAX_BINS);
        }
    }

    #[test]
    fn vvc_chroma_exact_prediction_bypasses_rd() {
        assert!(vvc_chroma_exact_prediction_skips_rd(
            &[0, 0, 0, 0],
            &[0, 0, 0, 0],
        ));
        assert!(!vvc_chroma_exact_prediction_skips_rd(
            &[0, 0, 0, 0],
            &[0, -1, 0, 0],
        ));
    }

    #[test]
    fn vvc_chroma_lossy_exact_prediction_stops_mode_search() {
        assert!(vvc_chroma_lossy_exact_mode_search_done(false, 0));
        assert!(!vvc_chroma_lossy_exact_mode_search_done(false, 1));
        assert!(!vvc_chroma_lossy_exact_mode_search_done(true, 0));
    }

    #[test]
    fn vvc_direct_luma_transform_skip_sse_matches_reconstruction() {
        let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
        let qp = super::super::VVC_DEFAULT_LOSSY_LUMA_QP;
        let width = 8;
        let height = 8;
        let source_residuals: Vec<i16> = (0..width * height)
            .map(|idx| ((idx as i16 * 7) % 31) - 15)
            .collect();
        let mut ac_levels = [0; VVC_LUMA_AC_COEFFS_PER_TU];
        for (idx, level) in ac_levels.iter_mut().enumerate().take(63) {
            *level = ((idx as i16 % 5) - 2).clamp(-2, 2);
        }
        let block = VvcFinalizedResidualBlock {
            dc_level: 3,
            ac_levels,
            has_ac: true,
            transform_skip: true,
            bdpcm_mode: VvcBdpcmMode::None,
        };
        let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);

        let actual =
            luma_transform_skip_residual_sse(&source_residuals, width, height, &quant_table, block);
        let mut reconstructed = Vec::new();
        reconstruct_vvc_luma_transform_skip_residuals_into_with_qp(
            &mut reconstructed,
            block.dc_level,
            &block.ac_levels,
            width,
            height,
            bit_depth,
            qp,
        );
        assert_eq!(
            actual,
            residual_sse_for_test(&source_residuals, &reconstructed)
        );
    }

    #[test]
    fn vvc_direct_luma_bdpcm_transform_skip_sse_matches_reconstruction() {
        let bit_depth = SampleBitDepth::new(10).expect("valid bit depth");
        let qp = super::super::VVC_DEFAULT_LOSSY_LUMA_QP;
        let width = 4;
        let height = 8;
        let source_residuals: Vec<i16> = (0..width * height)
            .map(|idx| ((idx as i16 * 11) % 43) - 21)
            .collect();
        let mut ac_levels = [0; VVC_LUMA_AC_COEFFS_PER_TU];
        for (idx, level) in ac_levels.iter_mut().enumerate().take(15) {
            *level = (idx as i16 % 7) - 3;
        }
        let block = VvcFinalizedResidualBlock {
            dc_level: -4,
            ac_levels,
            has_ac: true,
            transform_skip: true,
            bdpcm_mode: VvcBdpcmMode::Horizontal,
        };
        let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);

        let actual =
            luma_transform_skip_residual_sse(&source_residuals, width, height, &quant_table, block);
        let mut reconstructed = Vec::new();
        reconstruct_vvc_luma_bdpcm_transform_skip_residuals_into_with_qp(
            &mut reconstructed,
            block.dc_level,
            &block.ac_levels,
            width,
            height,
            bit_depth,
            qp,
            block.bdpcm_mode,
        );
        assert_eq!(
            actual,
            residual_sse_for_test(&source_residuals, &reconstructed)
        );
    }

    #[test]
    fn vvc_direct_chroma_transform_skip_sse_matches_reconstruction() {
        let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
        let qp = super::super::VVC_DEFAULT_LOSSY_CHROMA_QP;
        let width = 3;
        let height = 4;
        let source_residuals: Vec<i16> = (0..width * height)
            .map(|idx| ((idx as i16 * 5) % 23) - 11)
            .collect();
        let mut ac_levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
        for (idx, level) in ac_levels.iter_mut().enumerate() {
            *level = (idx as i16 % 5) - 2;
        }
        let block = VvcFinalizedResidualBlock {
            dc_level: 2,
            ac_levels,
            has_ac: true,
            transform_skip: true,
            bdpcm_mode: VvcBdpcmMode::None,
        };
        let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);

        let actual = chroma_transform_skip_residual_sse(
            &source_residuals,
            width,
            height,
            &quant_table,
            block,
        );
        let mut reconstructed = Vec::new();
        reconstruct_vvc_chroma_transform_skip_residuals_into_with_qp(
            &mut reconstructed,
            block.dc_level,
            &block.ac_levels,
            width,
            height,
            bit_depth,
            qp,
        );
        assert_eq!(
            actual,
            residual_sse_for_test(&source_residuals, &reconstructed)
        );
    }

    #[test]
    fn vvc_direct_chroma_bdpcm_transform_skip_sse_matches_reconstruction() {
        let bit_depth = SampleBitDepth::new(10).expect("valid bit depth");
        let qp = super::super::VVC_DEFAULT_LOSSY_CHROMA_QP;
        let width = 4;
        let height = 3;
        let source_residuals: Vec<i16> = (0..width * height)
            .map(|idx| ((idx as i16 * 13) % 37) - 18)
            .collect();
        let mut ac_levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
        for (idx, level) in ac_levels.iter_mut().enumerate() {
            *level = (idx as i16 % 7) - 3;
        }
        let block = VvcFinalizedResidualBlock {
            dc_level: -2,
            ac_levels,
            has_ac: true,
            transform_skip: true,
            bdpcm_mode: VvcBdpcmMode::Vertical,
        };
        let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);

        let actual = chroma_transform_skip_residual_sse(
            &source_residuals,
            width,
            height,
            &quant_table,
            block,
        );
        let mut reconstructed = Vec::new();
        reconstruct_vvc_chroma_bdpcm_transform_skip_residuals_into_with_qp(
            &mut reconstructed,
            block.dc_level,
            &block.ac_levels,
            width,
            height,
            bit_depth,
            qp,
            block.bdpcm_mode,
        );
        assert_eq!(
            actual,
            residual_sse_for_test(&source_residuals, &reconstructed)
        );
    }

    #[test]
    fn vvc_transform_skip_quant_radius_one_matches_previous_wide_search() {
        for bits in [8u8, 10, 12] {
            let bit_depth = SampleBitDepth::new(bits).expect("valid bit depth");
            let max_residual = (1i32 << bits) - 1;
            let sampled_step = 1usize << usize::from(bits.saturating_sub(8));
            for qp in 0..=63 {
                for residual in (-max_residual..=max_residual).step_by(sampled_step) {
                    assert_eq!(
                        quantize_vvc_transform_skip_level(residual as i16, bit_depth, qp),
                        quantize_vvc_transform_skip_level_with_radius(
                            residual as i16,
                            bit_depth,
                            qp,
                            2,
                        ),
                        "bits={bits} qp={qp} residual={residual}"
                    );
                }
                for residual in -64..=64 {
                    assert_eq!(
                        quantize_vvc_transform_skip_level(residual, bit_depth, qp),
                        quantize_vvc_transform_skip_level_with_radius(residual, bit_depth, qp, 2),
                        "bits={bits} qp={qp} residual={residual}"
                    );
                }
                for residual in [
                    -max_residual,
                    -max_residual + 1,
                    -1,
                    0,
                    1,
                    max_residual - 1,
                    max_residual,
                ] {
                    assert_eq!(
                        quantize_vvc_transform_skip_level(residual as i16, bit_depth, qp),
                        quantize_vvc_transform_skip_level_with_radius(
                            residual as i16,
                            bit_depth,
                            qp,
                            2,
                        ),
                        "bits={bits} qp={qp} residual={residual}"
                    );
                }
            }
        }
    }

    fn residual_sse_for_test(source: &[i16], reconstructed: &[i16]) -> u64 {
        source
            .iter()
            .zip(reconstructed.iter())
            .map(|(source, reconstructed)| {
                let diff = i64::from(*source) - i64::from(*reconstructed);
                (diff * diff) as u64
            })
            .sum()
    }

    #[test]
    fn vvc_luma_residual_tool_selection_is_quality_first() {
        let best = VvcScoredLumaResidualBlock {
            block: VvcFinalizedResidualBlock {
                dc_level: 0,
                ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
                has_ac: false,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            },
            mts_index: 0,
            score: VvcResidualBlockScore {
                distortion: 1_000,
                rate_cost: 0,
            },
        };
        let candidate = VvcScoredLumaResidualBlock {
            block: VvcFinalizedResidualBlock {
                dc_level: 1,
                ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
                has_ac: false,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            },
            mts_index: 2,
            score: VvcResidualBlockScore {
                distortion: 700,
                rate_cost: 1_000,
            },
        };

        assert!(candidate.selects_over(best));
        assert!(!VvcScoredLumaResidualBlock {
            score: VvcResidualBlockScore {
                distortion: 1_100,
                rate_cost: 0,
            },
            ..best
        }
        .selects_over(best));
    }

    #[test]
    fn vvc_luma_mrl_selection_is_quality_first() {
        let best = VvcLumaMrlCandidate {
            distortion: 1_000,
            rate_cost: 0,
            residual: None,
        };
        let candidate = VvcLumaMrlCandidate {
            distortion: 700,
            rate_cost: 20,
            residual: None,
        };

        assert!(candidate.selects_over(best));
        assert!(!VvcLumaMrlCandidate {
            distortion: 1_100,
            rate_cost: 0,
            residual: None,
        }
        .selects_over(best));
    }

    #[test]
    fn vvc_chroma_quality_gate_can_spend_bits_for_lower_distortion() {
        let best = VvcChromaQuantizedResidualScore {
            distortion: 1_000,
            rate_cost: 0,
        };
        let candidate = VvcChromaQuantizedResidualScore {
            distortion: 700,
            rate_cost: 20,
        };

        assert!(candidate.selects_over(best));
        assert!(!VvcChromaQuantizedResidualScore {
            distortion: 1_100,
            rate_cost: 0,
        }
        .selects_over(best));
    }

    #[test]
    fn vvc_luma_mts_search_is_gated_to_supported_lossy_blocks() {
        assert!(vvc_luma_mts_selection_allowed(
            VvcTuResidualCodingMode::Transformed,
            0,
            8,
            8,
            super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
            true,
        ));
        assert!(vvc_luma_mts_selection_allowed(
            VvcTuResidualCodingMode::Transformed,
            5,
            4,
            4,
            super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
            true,
        ));
        assert!(!vvc_luma_mts_selection_allowed(
            VvcTuResidualCodingMode::TransformSkip,
            0,
            8,
            8,
            super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
            true,
        ));
        assert!(!vvc_luma_mts_selection_allowed(
            VvcTuResidualCodingMode::Transformed,
            1,
            8,
            8,
            super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
            true,
        ));
        assert!(!vvc_luma_mts_selection_allowed(
            VvcTuResidualCodingMode::Transformed,
            0,
            16,
            8,
            super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
            true,
        ));
        assert!(!vvc_luma_mts_selection_allowed(
            VvcTuResidualCodingMode::Transformed,
            0,
            8,
            8,
            0,
            true,
        ));
        assert!(!vvc_luma_mts_selection_allowed(
            VvcTuResidualCodingMode::Transformed,
            0,
            8,
            8,
            super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
            false,
        ));
    }
}
