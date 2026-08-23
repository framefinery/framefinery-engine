mod prediction;
mod quant;
#[cfg(test)]
mod recon;
mod syntax;
pub(super) mod transform;

#[cfg(feature = "vvc-stats")]
use super::VvcChromaCclmMode;
use super::VvcSample;
use super::{
    VvcBdpcmMode, VvcChromaIntraPredictionMode, VvcIntraPredictionMode, VvcLumaSccDecision,
};

#[cfg(test)]
mod tests;

pub(super) use transform::{
    inverse_transform_vvc_chroma_quantized_block_into_with_qp,
    inverse_transform_vvc_luma_quantized_block_into_with_qp_and_mts,
    quantize_vvc_chroma_residual_greedy_with_qp, quantize_vvc_chroma_sample,
    reconstruct_vvc_chroma, VvcInverseTransformScratch, VVC_DEFAULT_LOSSY_CHROMA_QP,
    VVC_DEFAULT_LOSSY_LUMA_QP,
};
#[cfg(test)]
pub(super) use transform::{
    inverse_transform_vvc_luma_residual_levels, quantize_vvc_chroma,
    quantize_vvc_luma_residual_greedy, quantize_vvc_luma_residual_greedy_with_qp_and_mts,
    transform_vvc_tu, VVC_CHROMA_DC_BASE, VVC_LUMA_DC_BASE,
};

pub(super) use prediction::{
    fill_visible_chroma_node, fill_visible_luma_node,
    predict_vvc_chroma_bdpcm_block_into_with_availability,
    predict_vvc_chroma_cclm_block_into_with_availability,
    predict_vvc_chroma_cclm_pair_into_with_availability,
    predict_vvc_chroma_intra_block_into_with_availability,
    predict_vvc_luma_bdpcm_block_into_with_availability,
    predict_vvc_luma_intra_block_into_with_availability,
    predict_vvc_luma_intra_block_into_with_mrl_and_availability,
    residual_vvc_luma_bdpcm_block_into_with_availability, VvcDcPredictionScratch,
    VvcPlaneAvailability,
};
pub use quant::quantize_vvc_color;
#[cfg(test)]
pub(super) use quant::quantize_vvc_frame_with_reconstruction;
#[cfg(test)]
pub(super) use quant::quantize_vvc_residual_ctu_into_frame_reconstruction;
#[cfg(feature = "bench-internals")]
pub(super) use quant::quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes;
pub(super) use quant::{
    quantize_vvc_frame,
    quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes_and_scratch_with_mode_hints,
    VvcCtuQuantScratch, VvcLumaModeSearchState, VvcTransformSkipQuantTables,
};
#[cfg(test)]
pub(super) use recon::{reconstruct_vvc_residual_frame, reconstruct_vvc_residual_frame_with_qp};
pub(super) use syntax::{
    VvcResidualCabacEncoder, VvcResidualCabacOptions, VvcResidualCabacSymbolStream,
};
#[cfg(test)]
pub(super) use syntax::{
    VvcResidualCabacSymbol, VvcResidualCtxConfig, VvcResidualLocalStats, VvcResidualPass1State,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VvcQuantizedColor {
    pub y: u8,
    pub u: u8,
    pub v: u8,
    pub(super) luma_tu_intra_modes: [VvcIntraPredictionMode; MAX_VVC_LUMA_TUS],
    pub(super) luma_tu_remainders: [u8; MAX_VVC_LUMA_TUS],
    pub(super) luma_tu_negative: [bool; MAX_VVC_LUMA_TUS],
    pub(super) luma_tu_dc_levels: [i16; MAX_VVC_LUMA_TUS],
    pub(super) luma_tu_ac_levels: [[i16; VVC_LUMA_AC_COEFFS_PER_TU]; MAX_VVC_LUMA_TUS],
    pub(super) luma_tu_has_ac: [bool; MAX_VVC_LUMA_TUS],
    pub(super) luma_tu_scc_decisions: [VvcLumaSccDecision; MAX_VVC_LUMA_TUS],
    pub(super) luma_tu_transform_skip: [bool; MAX_VVC_LUMA_TUS],
    pub(super) luma_tu_bdpcm_modes: [VvcBdpcmMode; MAX_VVC_LUMA_TUS],
    pub(super) luma_tu_mrl_index: [u8; MAX_VVC_LUMA_TUS],
    pub(super) luma_tu_mts_index: [u8; MAX_VVC_LUMA_TUS],
    pub(super) luma_tu_count: usize,
    pub(super) chroma_tu_count: usize,
    pub(super) chroma_tu_intra_modes: [VvcChromaIntraPredictionMode; MAX_VVC_CHROMA_TUS],
    pub(super) cb_tu_dc_levels: [i16; MAX_VVC_CHROMA_TUS],
    pub(super) cr_tu_dc_levels: [i16; MAX_VVC_CHROMA_TUS],
    pub(super) cb_tu_ac_levels: [[i16; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS],
    pub(super) cr_tu_ac_levels: [[i16; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS],
    pub(super) cb_tu_has_ac: [bool; MAX_VVC_CHROMA_TUS],
    pub(super) cr_tu_has_ac: [bool; MAX_VVC_CHROMA_TUS],
    pub(super) cb_tu_transform_skip: [bool; MAX_VVC_CHROMA_TUS],
    pub(super) cr_tu_transform_skip: [bool; MAX_VVC_CHROMA_TUS],
    pub(super) chroma_tu_bdpcm_modes: [VvcBdpcmMode; MAX_VVC_CHROMA_TUS],
    pub(super) cb_rem: u8,
    pub(super) cr_rem: u8,
    #[cfg(feature = "vvc-stats")]
    pub(super) intra_search_stats: VvcIntraSearchStats,
    #[cfg(feature = "vvc-stats")]
    pub(super) residual_energy_stats: VvcResidualEnergyStats,
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct VvcIntraSearchStats {
    pub(super) luma_dc_candidates: usize,
    pub(super) luma_planar_candidates: usize,
    pub(super) luma_directional_coarse_candidates: usize,
    pub(super) luma_directional_refinement_candidates: usize,
    pub(super) luma_rd_refinement_attempts: usize,
    pub(super) luma_rd_refinement_switches: usize,
    pub(super) luma_rd_cached_candidates: usize,
    pub(super) luma_rd_generated_candidates: usize,
    pub(super) luma_mode_search_nanos: u64,
    pub(super) luma_rd_refinement_nanos: u64,
    pub(super) luma_mrl_nanos: u64,
    pub(super) luma_bdpcm_nanos: u64,
    pub(super) luma_finalize_nanos: u64,
    pub(super) luma_dc_prediction_nanos: u64,
    pub(super) luma_planar_prediction_nanos: u64,
    pub(super) luma_directional_prediction_nanos: u64,
    pub(super) luma_mrl_prediction_nanos: u64,
    pub(super) luma_bdpcm_prediction_nanos: u64,
    pub(super) luma_residual_build_nanos: u64,
    pub(super) luma_residual_build_calls: usize,
    pub(super) luma_mode_score_nanos: u64,
    pub(super) luma_rd_prediction_nanos: u64,
    pub(super) luma_rd_residual_build_nanos: u64,
    pub(super) luma_rd_scoring_nanos: u64,
    pub(super) luma_transform_skip_candidate_nanos: u64,
    pub(super) luma_transform_skip_candidate_count: usize,
    pub(super) luma_transformed_quant_nanos: u64,
    pub(super) luma_transformed_quant_count: usize,
    pub(super) luma_residual_recon_nanos: u64,
    pub(super) luma_fill_nanos: u64,
    pub(super) chroma_derived_candidates: usize,
    pub(super) chroma_explicit_candidates: usize,
    pub(super) chroma_cclm_candidates: usize,
    pub(super) chroma_cclm_linear_candidates: usize,
    pub(super) chroma_cclm_mdlm_left_candidates: usize,
    pub(super) chroma_cclm_mdlm_top_candidates: usize,
    pub(super) chroma_rd_refinement_attempts: usize,
    pub(super) chroma_rd_refinement_switches: usize,
    pub(super) chroma_rd_cached_candidates: usize,
    pub(super) chroma_rd_generated_candidates: usize,
    pub(super) chroma_mode_search_nanos: u64,
    pub(super) chroma_rd_refinement_nanos: u64,
    pub(super) chroma_bdpcm_nanos: u64,
    pub(super) chroma_bdpcm_direct_candidates: usize,
    pub(super) chroma_bdpcm_direct_safe_candidates: usize,
    pub(super) chroma_bdpcm_direct_selected: usize,
    pub(super) chroma_bdpcm_regular_candidates: usize,
    pub(super) chroma_bdpcm_regular_best_updates: usize,
    pub(super) chroma_finalize_nanos: u64,
    pub(super) chroma_derived_prediction_nanos: u64,
    pub(super) chroma_explicit_prediction_nanos: u64,
    pub(super) chroma_cclm_prediction_nanos: u64,
    pub(super) chroma_bdpcm_prediction_nanos: u64,
    pub(super) chroma_residual_build_nanos: u64,
    pub(super) chroma_residual_build_calls: usize,
    pub(super) chroma_mode_score_nanos: u64,
    pub(super) chroma_rd_prediction_nanos: u64,
    pub(super) chroma_rd_residual_build_nanos: u64,
    pub(super) chroma_rd_scoring_nanos: u64,
    pub(super) chroma_transform_skip_candidate_nanos: u64,
    pub(super) chroma_transform_skip_candidate_count: usize,
    pub(super) chroma_transformed_quant_nanos: u64,
    pub(super) chroma_transformed_quant_count: usize,
    pub(super) chroma_residual_recon_nanos: u64,
    pub(super) chroma_fill_nanos: u64,
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VvcLumaPredictionStatsFamily {
    Dc,
    Planar,
    Directional,
    Mrl,
    Bdpcm,
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VvcChromaPredictionStatsFamily {
    Derived,
    Explicit,
    Cclm,
    Bdpcm,
}

#[cfg(feature = "vvc-stats")]
impl VvcIntraSearchStats {
    pub(super) const fn luma_directional_candidates(self) -> usize {
        self.luma_directional_coarse_candidates + self.luma_directional_refinement_candidates
    }

    pub(super) const fn luma_candidates(self) -> usize {
        self.luma_dc_candidates + self.luma_planar_candidates + self.luma_directional_candidates()
    }

    pub(super) const fn chroma_candidates(self) -> usize {
        self.chroma_derived_candidates
            + self.chroma_explicit_candidates
            + self.chroma_cclm_candidates
    }

    pub(super) fn add_luma_dc(&mut self) {
        self.luma_dc_candidates += 1;
    }

    pub(super) fn add_luma_planar(&mut self) {
        self.luma_planar_candidates += 1;
    }

    pub(super) fn add_luma_directional_coarse(&mut self) {
        self.luma_directional_coarse_candidates += 1;
    }

    pub(super) fn add_luma_directional_refinement(&mut self) {
        self.luma_directional_refinement_candidates += 1;
    }

    pub(super) fn add_luma_rd_refinement_attempt(&mut self) {
        self.luma_rd_refinement_attempts += 1;
    }

    pub(super) fn add_luma_rd_refinement_switch(&mut self) {
        self.luma_rd_refinement_switches += 1;
    }

    pub(super) fn add_luma_rd_cached_candidate(&mut self) {
        self.luma_rd_cached_candidates += 1;
    }

    pub(super) fn add_luma_rd_generated_candidate(&mut self) {
        self.luma_rd_generated_candidates += 1;
    }

    pub(super) fn add_luma_mode_search_nanos(&mut self, nanos: u64) {
        self.luma_mode_search_nanos = self.luma_mode_search_nanos.saturating_add(nanos);
    }

    pub(super) fn add_luma_rd_refinement_nanos(&mut self, nanos: u64) {
        self.luma_rd_refinement_nanos = self.luma_rd_refinement_nanos.saturating_add(nanos);
    }

    pub(super) fn add_luma_mrl_nanos(&mut self, nanos: u64) {
        self.luma_mrl_nanos = self.luma_mrl_nanos.saturating_add(nanos);
    }

    pub(super) fn add_luma_bdpcm_nanos(&mut self, nanos: u64) {
        self.luma_bdpcm_nanos = self.luma_bdpcm_nanos.saturating_add(nanos);
    }

    pub(super) fn add_luma_finalize_nanos(&mut self, nanos: u64) {
        self.luma_finalize_nanos = self.luma_finalize_nanos.saturating_add(nanos);
    }

    pub(super) fn add_luma_prediction_nanos(
        &mut self,
        family: VvcLumaPredictionStatsFamily,
        nanos: u64,
    ) {
        match family {
            VvcLumaPredictionStatsFamily::Dc => {
                self.luma_dc_prediction_nanos = self.luma_dc_prediction_nanos.saturating_add(nanos);
            }
            VvcLumaPredictionStatsFamily::Planar => {
                self.luma_planar_prediction_nanos =
                    self.luma_planar_prediction_nanos.saturating_add(nanos);
            }
            VvcLumaPredictionStatsFamily::Directional => {
                self.luma_directional_prediction_nanos =
                    self.luma_directional_prediction_nanos.saturating_add(nanos);
            }
            VvcLumaPredictionStatsFamily::Mrl => {
                self.luma_mrl_prediction_nanos =
                    self.luma_mrl_prediction_nanos.saturating_add(nanos);
            }
            VvcLumaPredictionStatsFamily::Bdpcm => {
                self.luma_bdpcm_prediction_nanos =
                    self.luma_bdpcm_prediction_nanos.saturating_add(nanos);
            }
        }
    }

    pub(super) fn add_luma_residual_build_nanos(&mut self, nanos: u64) {
        self.luma_residual_build_nanos = self.luma_residual_build_nanos.saturating_add(nanos);
        self.luma_residual_build_calls += 1;
    }

    pub(super) fn add_luma_mode_score_nanos(&mut self, nanos: u64) {
        self.luma_mode_score_nanos = self.luma_mode_score_nanos.saturating_add(nanos);
    }

    pub(super) fn add_luma_rd_prediction_nanos(&mut self, nanos: u64) {
        self.luma_rd_prediction_nanos = self.luma_rd_prediction_nanos.saturating_add(nanos);
    }

    pub(super) fn add_luma_rd_residual_build_nanos(&mut self, nanos: u64) {
        self.luma_rd_residual_build_nanos = self.luma_rd_residual_build_nanos.saturating_add(nanos);
    }

    pub(super) fn add_luma_rd_scoring_nanos(&mut self, nanos: u64) {
        self.luma_rd_scoring_nanos = self.luma_rd_scoring_nanos.saturating_add(nanos);
    }

    pub(super) fn add_luma_transform_skip_candidate_nanos(&mut self, nanos: u64) {
        self.luma_transform_skip_candidate_nanos = self
            .luma_transform_skip_candidate_nanos
            .saturating_add(nanos);
        self.luma_transform_skip_candidate_count += 1;
    }

    pub(super) fn add_luma_transformed_quant_nanos(&mut self, nanos: u64) {
        self.luma_transformed_quant_nanos = self.luma_transformed_quant_nanos.saturating_add(nanos);
        self.luma_transformed_quant_count += 1;
    }

    pub(super) fn add_luma_residual_recon_nanos(&mut self, nanos: u64) {
        self.luma_residual_recon_nanos = self.luma_residual_recon_nanos.saturating_add(nanos);
    }

    pub(super) fn add_luma_fill_nanos(&mut self, nanos: u64) {
        self.luma_fill_nanos = self.luma_fill_nanos.saturating_add(nanos);
    }

    pub(super) fn add_chroma_derived(&mut self) {
        self.chroma_derived_candidates += 1;
    }

    pub(super) fn add_chroma_explicit(&mut self) {
        self.chroma_explicit_candidates += 1;
    }

    pub(super) fn add_chroma_cclm(&mut self) {
        self.chroma_cclm_candidates += 1;
    }

    pub(super) fn add_chroma_cclm_mode(&mut self, mode: VvcChromaCclmMode) {
        self.add_chroma_cclm();
        match mode {
            VvcChromaCclmMode::Linear => self.chroma_cclm_linear_candidates += 1,
            VvcChromaCclmMode::MdlmLeft => self.chroma_cclm_mdlm_left_candidates += 1,
            VvcChromaCclmMode::MdlmTop => self.chroma_cclm_mdlm_top_candidates += 1,
        }
    }

    pub(super) fn add_chroma_rd_refinement_attempt(&mut self) {
        self.chroma_rd_refinement_attempts += 1;
    }

    pub(super) fn add_chroma_rd_refinement_switch(&mut self) {
        self.chroma_rd_refinement_switches += 1;
    }

    pub(super) fn add_chroma_rd_cached_candidate(&mut self) {
        self.chroma_rd_cached_candidates += 1;
    }

    pub(super) fn add_chroma_rd_generated_candidate(&mut self) {
        self.chroma_rd_generated_candidates += 1;
    }

    pub(super) fn add_chroma_mode_search_nanos(&mut self, nanos: u64) {
        self.chroma_mode_search_nanos = self.chroma_mode_search_nanos.saturating_add(nanos);
    }

    pub(super) fn add_chroma_rd_refinement_nanos(&mut self, nanos: u64) {
        self.chroma_rd_refinement_nanos = self.chroma_rd_refinement_nanos.saturating_add(nanos);
    }

    pub(super) fn add_chroma_bdpcm_nanos(&mut self, nanos: u64) {
        self.chroma_bdpcm_nanos = self.chroma_bdpcm_nanos.saturating_add(nanos);
    }

    pub(super) fn add_chroma_bdpcm_direct_candidate(&mut self) {
        self.chroma_bdpcm_direct_candidates += 1;
    }

    pub(super) fn add_chroma_bdpcm_direct_safe_candidate(&mut self) {
        self.chroma_bdpcm_direct_safe_candidates += 1;
    }

    pub(super) fn add_chroma_bdpcm_direct_selected(&mut self) {
        self.chroma_bdpcm_direct_selected += 1;
    }

    pub(super) fn add_chroma_bdpcm_regular_candidate(&mut self) {
        self.chroma_bdpcm_regular_candidates += 1;
    }

    pub(super) fn add_chroma_bdpcm_regular_best_update(&mut self) {
        self.chroma_bdpcm_regular_best_updates += 1;
    }

    pub(super) fn add_chroma_finalize_nanos(&mut self, nanos: u64) {
        self.chroma_finalize_nanos = self.chroma_finalize_nanos.saturating_add(nanos);
    }

    pub(super) fn add_chroma_prediction_nanos(
        &mut self,
        family: VvcChromaPredictionStatsFamily,
        nanos: u64,
    ) {
        match family {
            VvcChromaPredictionStatsFamily::Derived => {
                self.chroma_derived_prediction_nanos =
                    self.chroma_derived_prediction_nanos.saturating_add(nanos);
            }
            VvcChromaPredictionStatsFamily::Explicit => {
                self.chroma_explicit_prediction_nanos =
                    self.chroma_explicit_prediction_nanos.saturating_add(nanos);
            }
            VvcChromaPredictionStatsFamily::Cclm => {
                self.chroma_cclm_prediction_nanos =
                    self.chroma_cclm_prediction_nanos.saturating_add(nanos);
            }
            VvcChromaPredictionStatsFamily::Bdpcm => {
                self.chroma_bdpcm_prediction_nanos =
                    self.chroma_bdpcm_prediction_nanos.saturating_add(nanos);
            }
        }
    }

    pub(super) fn add_chroma_residual_build_nanos(&mut self, nanos: u64) {
        self.chroma_residual_build_nanos = self.chroma_residual_build_nanos.saturating_add(nanos);
        self.chroma_residual_build_calls += 1;
    }

    pub(super) fn add_chroma_mode_score_nanos(&mut self, nanos: u64) {
        self.chroma_mode_score_nanos = self.chroma_mode_score_nanos.saturating_add(nanos);
    }

    pub(super) fn add_chroma_rd_prediction_nanos(&mut self, nanos: u64) {
        self.chroma_rd_prediction_nanos = self.chroma_rd_prediction_nanos.saturating_add(nanos);
    }

    pub(super) fn add_chroma_rd_residual_build_nanos(&mut self, nanos: u64) {
        self.chroma_rd_residual_build_nanos =
            self.chroma_rd_residual_build_nanos.saturating_add(nanos);
    }

    pub(super) fn add_chroma_rd_scoring_nanos(&mut self, nanos: u64) {
        self.chroma_rd_scoring_nanos = self.chroma_rd_scoring_nanos.saturating_add(nanos);
    }

    pub(super) fn add_chroma_transform_skip_candidate_nanos(&mut self, nanos: u64) {
        self.chroma_transform_skip_candidate_nanos = self
            .chroma_transform_skip_candidate_nanos
            .saturating_add(nanos);
        self.chroma_transform_skip_candidate_count += 1;
    }

    pub(super) fn add_chroma_transformed_quant_nanos(&mut self, nanos: u64) {
        self.chroma_transformed_quant_nanos =
            self.chroma_transformed_quant_nanos.saturating_add(nanos);
        self.chroma_transformed_quant_count += 1;
    }

    pub(super) fn add_chroma_residual_recon_nanos(&mut self, nanos: u64) {
        self.chroma_residual_recon_nanos = self.chroma_residual_recon_nanos.saturating_add(nanos);
    }

    pub(super) fn add_chroma_fill_nanos(&mut self, nanos: u64) {
        self.chroma_fill_nanos = self.chroma_fill_nanos.saturating_add(nanos);
    }
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct VvcResidualEnergyStats {
    pub(super) luma_total_sse: u64,
    pub(super) luma_coded_first4x4_sse: u64,
    pub(super) luma_uncoded_tail_sse: u64,
    pub(super) chroma_total_sse: u64,
    pub(super) chroma_coded_first4x4_sse: u64,
    pub(super) chroma_uncoded_tail_sse: u64,
}

#[cfg(feature = "vvc-stats")]
impl VvcResidualEnergyStats {
    pub(super) fn add_luma_residuals(&mut self, residuals: &[i16], width: usize, height: usize) {
        let split = residual_energy_split(residuals, width, height);
        self.luma_total_sse = self.luma_total_sse.saturating_add(split.total_sse);
        self.luma_coded_first4x4_sse = self
            .luma_coded_first4x4_sse
            .saturating_add(split.coded_first4x4_sse);
        self.luma_uncoded_tail_sse = self
            .luma_uncoded_tail_sse
            .saturating_add(split.uncoded_tail_sse);
    }

    pub(super) fn add_chroma_residuals(&mut self, residuals: &[i16], width: usize, height: usize) {
        let split = residual_energy_split(residuals, width, height);
        self.chroma_total_sse = self.chroma_total_sse.saturating_add(split.total_sse);
        self.chroma_coded_first4x4_sse = self
            .chroma_coded_first4x4_sse
            .saturating_add(split.coded_first4x4_sse);
        self.chroma_uncoded_tail_sse = self
            .chroma_uncoded_tail_sse
            .saturating_add(split.uncoded_tail_sse);
    }
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcResidualEnergySplit {
    total_sse: u64,
    coded_first4x4_sse: u64,
    uncoded_tail_sse: u64,
}

#[cfg(feature = "vvc-stats")]
fn residual_energy_split(residuals: &[i16], width: usize, height: usize) -> VvcResidualEnergySplit {
    debug_assert_eq!(residuals.len(), width * height);
    let mut split = VvcResidualEnergySplit {
        total_sse: 0,
        coded_first4x4_sse: 0,
        uncoded_tail_sse: 0,
    };
    for y in 0..height {
        for x in 0..width {
            let residual = i64::from(residuals[y * width + x]);
            let sse = (residual * residual) as u64;
            split.total_sse = split.total_sse.saturating_add(sse);
            if x < 4 && y < 4 {
                split.coded_first4x4_sse = split.coded_first4x4_sse.saturating_add(sse);
            } else {
                split.uncoded_tail_sse = split.uncoded_tail_sse.saturating_add(sse);
            }
        }
    }
    split
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VvcQuantizedResidualFrame {
    pub(super) quantized: VvcQuantizedColor,
    pub(super) reconstruction_yuv: Vec<VvcSample>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VvcTransformComponent {
    Luma,
    ChromaCb,
    ChromaCr,
}

#[cfg(test)]
impl VvcTransformComponent {
    pub(super) const fn dc_base(self) -> i16 {
        match self {
            Self::Luma => VVC_LUMA_DC_BASE,
            Self::ChromaCb | Self::ChromaCr => VVC_CHROMA_DC_BASE,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VvcTuTransformBlock {
    pub(super) component: VvcTransformComponent,
    pub(super) width: u16,
    pub(super) height: u16,
    pub(super) dc_coeff: i16,
    pub(super) ac_coeffs: Vec<i16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VvcQuantizedTransformBlock<const AC_COEFFS: usize> {
    pub(super) reconstructed_dc_coeff: i16,
    pub(super) reconstructed_ac_coeffs: [i16; AC_COEFFS],
    pub(super) has_ac: bool,
    pub(super) abs_remainder: u8,
}

pub(super) type VvcQuantizedLumaTransformBlock =
    VvcQuantizedTransformBlock<VVC_LUMA_AC_COEFFS_PER_TU>;
pub(super) type VvcQuantizedChromaTransformBlock =
    VvcQuantizedTransformBlock<VVC_CHROMA_AC_COEFFS_PER_TU>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VvcResidualComponent {
    Luma,
    ChromaCb,
    ChromaCr,
}

impl VvcResidualComponent {
    pub(super) const fn transform_skip_ctx_inc(self) -> u8 {
        match self {
            Self::Luma => 0,
            Self::ChromaCb | Self::ChromaCr => 1,
        }
    }
}

pub(super) const VVC_LUMA_AC_COEFFS_PER_TU: usize = 63;
pub(super) const VVC_CHROMA_AC_COEFFS_PER_TU: usize = 15;
pub(super) const VVC_CHROMA_AC_POSITIONS_4X4: [(usize, usize); VVC_CHROMA_AC_COEFFS_PER_TU] = [
    (1, 0),
    (2, 0),
    (3, 0),
    (0, 1),
    (1, 1),
    (2, 1),
    (3, 1),
    (0, 2),
    (1, 2),
    (2, 2),
    (3, 2),
    (0, 3),
    (1, 3),
    (2, 3),
    (3, 3),
];
pub(super) const MAX_VVC_LUMA_TUS: usize = 16 * 16;
pub(super) const MAX_VVC_CHROMA_TUS: usize = MAX_VVC_LUMA_TUS;
