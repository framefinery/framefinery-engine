#[cfg(feature = "vvc-stats")]
fn add_vvc_quantized_ctu_counters(stats: &mut VvcFrameStats, quantized: &VvcQuantizedColor) {
    stats.add_counter("luma_tu_count", quantized.luma_tu_count as u64);
    stats.add_counter("chroma_tu_count", quantized.chroma_tu_count as u64);
    let residual_coding = vvc_tu_residual_coding_counts(quantized);
    stats.add_counter(
        "luma_tu_transform_skip_count",
        residual_coding.luma_transform_skip as u64,
    );
    stats.add_counter(
        "luma_tu_transformed_count",
        residual_coding.luma_transformed as u64,
    );
    stats.add_counter(
        "cb_tu_transform_skip_count",
        residual_coding.cb_transform_skip as u64,
    );
    stats.add_counter(
        "cb_tu_transformed_count",
        residual_coding.cb_transformed as u64,
    );
    stats.add_counter(
        "cr_tu_transform_skip_count",
        residual_coding.cr_transform_skip as u64,
    );
    stats.add_counter(
        "cr_tu_transformed_count",
        residual_coding.cr_transformed as u64,
    );
    stats.add_counter(
        "chroma_tu_transform_skip_count",
        residual_coding.chroma_transform_skip() as u64,
    );
    stats.add_counter(
        "chroma_tu_transformed_count",
        residual_coding.chroma_transformed() as u64,
    );
    let bdpcm = vvc_tu_bdpcm_counts(quantized);
    stats.add_counter("luma_bdpcm_horizontal_count", bdpcm.luma_horizontal as u64);
    stats.add_counter("luma_bdpcm_vertical_count", bdpcm.luma_vertical as u64);
    stats.add_counter(
        "chroma_bdpcm_horizontal_count",
        bdpcm.chroma_horizontal as u64,
    );
    stats.add_counter("chroma_bdpcm_vertical_count", bdpcm.chroma_vertical as u64);
    let mts = vvc_luma_tu_mts_counts(quantized);
    stats.add_counter("luma_mts_nonzero_count", mts.nonzero as u64);
    stats.add_counter("luma_mts_dst7_dst7_count", mts.dst7_dst7 as u64);
    stats.add_counter("luma_mts_dct8_dst7_count", mts.dct8_dst7 as u64);
    stats.add_counter("luma_mts_dst7_dct8_count", mts.dst7_dct8 as u64);
    stats.add_counter("luma_mts_dct8_dct8_count", mts.dct8_dct8 as u64);
    let energy = quantized.residual_energy_stats;
    stats.add_counter("luma_residual_sse_total", energy.luma_total_sse);
    stats.add_counter(
        "luma_residual_sse_coded_first4x4",
        energy.luma_coded_first4x4_sse,
    );
    stats.add_counter(
        "luma_residual_sse_uncoded_tail",
        energy.luma_uncoded_tail_sse,
    );
    stats.add_counter("chroma_residual_sse_total", energy.chroma_total_sse);
    stats.add_counter(
        "chroma_residual_sse_coded_first4x4",
        energy.chroma_coded_first4x4_sse,
    );
    stats.add_counter(
        "chroma_residual_sse_uncoded_tail",
        energy.chroma_uncoded_tail_sse,
    );
    let search = quantized.intra_search_stats;
    stats.add_counter("luma_candidate_count", search.luma_candidates() as u64);
    stats.add_counter("luma_candidate_dc", search.luma_dc_candidates as u64);
    stats.add_counter(
        "luma_candidate_planar",
        search.luma_planar_candidates as u64,
    );
    stats.add_counter(
        "luma_candidate_directional",
        search.luma_directional_candidates() as u64,
    );
    stats.add_counter(
        "luma_candidate_directional_coarse",
        search.luma_directional_coarse_candidates as u64,
    );
    stats.add_counter(
        "luma_candidate_directional_refinement",
        search.luma_directional_refinement_candidates as u64,
    );
    stats.add_counter(
        "luma_rd_refinement_attempts",
        search.luma_rd_refinement_attempts as u64,
    );
    stats.add_counter(
        "luma_rd_refinement_switches",
        search.luma_rd_refinement_switches as u64,
    );
    stats.add_counter(
        "luma_rd_cached_candidates",
        search.luma_rd_cached_candidates as u64,
    );
    stats.add_counter(
        "luma_rd_generated_candidates",
        search.luma_rd_generated_candidates as u64,
    );
    stats.add_counter("luma_mode_search_nanos", search.luma_mode_search_nanos);
    stats.add_counter("luma_rd_refinement_nanos", search.luma_rd_refinement_nanos);
    stats.add_counter("luma_mrl_nanos", search.luma_mrl_nanos);
    stats.add_counter("luma_bdpcm_nanos", search.luma_bdpcm_nanos);
    stats.add_counter("luma_finalize_nanos", search.luma_finalize_nanos);
    stats.add_counter("luma_dc_prediction_nanos", search.luma_dc_prediction_nanos);
    stats.add_counter(
        "luma_planar_prediction_nanos",
        search.luma_planar_prediction_nanos,
    );
    stats.add_counter(
        "luma_directional_prediction_nanos",
        search.luma_directional_prediction_nanos,
    );
    stats.add_counter(
        "luma_mrl_prediction_nanos",
        search.luma_mrl_prediction_nanos,
    );
    stats.add_counter(
        "luma_bdpcm_prediction_nanos",
        search.luma_bdpcm_prediction_nanos,
    );
    stats.add_counter(
        "luma_residual_build_nanos",
        search.luma_residual_build_nanos,
    );
    stats.add_counter(
        "luma_residual_build_calls",
        search.luma_residual_build_calls as u64,
    );
    stats.add_counter("luma_mode_score_nanos", search.luma_mode_score_nanos);
    stats.add_counter("luma_rd_prediction_nanos", search.luma_rd_prediction_nanos);
    stats.add_counter(
        "luma_rd_residual_build_nanos",
        search.luma_rd_residual_build_nanos,
    );
    stats.add_counter("luma_rd_scoring_nanos", search.luma_rd_scoring_nanos);
    stats.add_counter(
        "luma_transform_skip_candidate_nanos",
        search.luma_transform_skip_candidate_nanos,
    );
    stats.add_counter(
        "luma_transform_skip_candidate_count",
        search.luma_transform_skip_candidate_count as u64,
    );
    stats.add_counter(
        "luma_transformed_quant_nanos",
        search.luma_transformed_quant_nanos,
    );
    stats.add_counter(
        "luma_transformed_quant_count",
        search.luma_transformed_quant_count as u64,
    );
    stats.add_counter(
        "luma_residual_recon_nanos",
        search.luma_residual_recon_nanos,
    );
    stats.add_counter("luma_fill_nanos", search.luma_fill_nanos);
    stats.add_counter("chroma_candidate_count", search.chroma_candidates() as u64);
    stats.add_counter(
        "chroma_candidate_derived",
        search.chroma_derived_candidates as u64,
    );
    stats.add_counter(
        "chroma_candidate_explicit",
        search.chroma_explicit_candidates as u64,
    );
    stats.add_counter(
        "chroma_candidate_cclm",
        search.chroma_cclm_candidates as u64,
    );
    stats.add_counter(
        "chroma_candidate_cclm_linear",
        search.chroma_cclm_linear_candidates as u64,
    );
    stats.add_counter(
        "chroma_candidate_mdlm_left",
        search.chroma_cclm_mdlm_left_candidates as u64,
    );
    stats.add_counter(
        "chroma_candidate_mdlm_top",
        search.chroma_cclm_mdlm_top_candidates as u64,
    );
    stats.add_counter(
        "chroma_rd_refinement_attempts",
        search.chroma_rd_refinement_attempts as u64,
    );
    stats.add_counter(
        "chroma_rd_refinement_switches",
        search.chroma_rd_refinement_switches as u64,
    );
    stats.add_counter(
        "chroma_rd_cached_candidates",
        search.chroma_rd_cached_candidates as u64,
    );
    stats.add_counter(
        "chroma_rd_generated_candidates",
        search.chroma_rd_generated_candidates as u64,
    );
    stats.add_counter("chroma_mode_search_nanos", search.chroma_mode_search_nanos);
    stats.add_counter(
        "chroma_rd_refinement_nanos",
        search.chroma_rd_refinement_nanos,
    );
    stats.add_counter("chroma_bdpcm_nanos", search.chroma_bdpcm_nanos);
    stats.add_counter(
        "chroma_bdpcm_direct_candidates",
        search.chroma_bdpcm_direct_candidates as u64,
    );
    stats.add_counter(
        "chroma_bdpcm_direct_safe_candidates",
        search.chroma_bdpcm_direct_safe_candidates as u64,
    );
    stats.add_counter(
        "chroma_bdpcm_direct_selected",
        search.chroma_bdpcm_direct_selected as u64,
    );
    stats.add_counter(
        "chroma_bdpcm_regular_candidates",
        search.chroma_bdpcm_regular_candidates as u64,
    );
    stats.add_counter(
        "chroma_bdpcm_regular_best_updates",
        search.chroma_bdpcm_regular_best_updates as u64,
    );
    stats.add_counter("chroma_finalize_nanos", search.chroma_finalize_nanos);
    stats.add_counter(
        "chroma_derived_prediction_nanos",
        search.chroma_derived_prediction_nanos,
    );
    stats.add_counter(
        "chroma_explicit_prediction_nanos",
        search.chroma_explicit_prediction_nanos,
    );
    stats.add_counter(
        "chroma_cclm_prediction_nanos",
        search.chroma_cclm_prediction_nanos,
    );
    stats.add_counter(
        "chroma_bdpcm_prediction_nanos",
        search.chroma_bdpcm_prediction_nanos,
    );
    stats.add_counter(
        "chroma_residual_build_nanos",
        search.chroma_residual_build_nanos,
    );
    stats.add_counter(
        "chroma_residual_build_calls",
        search.chroma_residual_build_calls as u64,
    );
    stats.add_counter("chroma_mode_score_nanos", search.chroma_mode_score_nanos);
    stats.add_counter(
        "chroma_rd_prediction_nanos",
        search.chroma_rd_prediction_nanos,
    );
    stats.add_counter(
        "chroma_rd_residual_build_nanos",
        search.chroma_rd_residual_build_nanos,
    );
    stats.add_counter("chroma_rd_scoring_nanos", search.chroma_rd_scoring_nanos);
    stats.add_counter(
        "chroma_transform_skip_candidate_nanos",
        search.chroma_transform_skip_candidate_nanos,
    );
    stats.add_counter(
        "chroma_transform_skip_candidate_count",
        search.chroma_transform_skip_candidate_count as u64,
    );
    stats.add_counter(
        "chroma_transformed_quant_nanos",
        search.chroma_transformed_quant_nanos,
    );
    stats.add_counter(
        "chroma_transformed_quant_count",
        search.chroma_transformed_quant_count as u64,
    );
    stats.add_counter(
        "chroma_residual_recon_nanos",
        search.chroma_residual_recon_nanos,
    );
    stats.add_counter("chroma_fill_nanos", search.chroma_fill_nanos);
    let modes = vvc_luma_mode_counts(quantized);
    stats.add_counter("luma_mode_dc", modes.dc as u64);
    stats.add_counter("luma_mode_planar", modes.planar as u64);
    stats.add_counter("luma_mode_horizontal", modes.horizontal as u64);
    stats.add_counter("luma_mode_vertical", modes.vertical as u64);
    stats.add_counter("luma_mode_angular", modes.angular as u64);
    add_vvc_mode_index_counters(stats, "luma_mode_angular_", &modes.angular_by_index);
    let chroma_modes = vvc_chroma_mode_counts(quantized);
    stats.add_counter("chroma_mode_derived", chroma_modes.derived as u64);
    stats.add_counter("chroma_mode_dc", chroma_modes.dc as u64);
    stats.add_counter("chroma_mode_planar", chroma_modes.planar as u64);
    stats.add_counter("chroma_mode_horizontal", chroma_modes.horizontal as u64);
    stats.add_counter("chroma_mode_vertical", chroma_modes.vertical as u64);
    stats.add_counter("chroma_mode_angular", chroma_modes.angular as u64);
    add_vvc_mode_index_counters(
        stats,
        "chroma_mode_angular_",
        &chroma_modes.angular_by_index,
    );
    stats.add_counter("chroma_mode_cclm", chroma_modes.cclm as u64);
    stats.add_counter("chroma_mode_cclm_linear", chroma_modes.cclm_linear as u64);
    stats.add_counter("chroma_mode_mdlm_left", chroma_modes.mdlm_left as u64);
    stats.add_counter("chroma_mode_mdlm_top", chroma_modes.mdlm_top as u64);
    for idx in 0..quantized.luma_tu_count {
        let dc_nonzero = quantized.luma_tu_dc_levels[idx] != 0;
        let ac_nonzero = quantized.luma_tu_ac_levels[idx]
            .iter()
            .filter(|level| **level != 0)
            .count();
        debug_assert_eq!(quantized.luma_tu_has_ac[idx], ac_nonzero != 0);
        stats.add_counter("luma_dc_nonzero", u64::from(dc_nonzero));
        stats.add_counter("luma_ac_nonzero", ac_nonzero as u64);
        stats.add_counter(
            "luma_cbf",
            u64::from(dc_nonzero || quantized.luma_tu_has_ac[idx]),
        );
    }
    for idx in 0..quantized.chroma_tu_count {
        let cb_dc_nonzero = quantized.cb_tu_dc_levels[idx] != 0;
        let cr_dc_nonzero = quantized.cr_tu_dc_levels[idx] != 0;
        let cb_ac_nonzero = quantized.cb_tu_ac_levels[idx]
            .iter()
            .filter(|level| **level != 0)
            .count();
        let cr_ac_nonzero = quantized.cr_tu_ac_levels[idx]
            .iter()
            .filter(|level| **level != 0)
            .count();
        debug_assert_eq!(quantized.cb_tu_has_ac[idx], cb_ac_nonzero != 0);
        debug_assert_eq!(quantized.cr_tu_has_ac[idx], cr_ac_nonzero != 0);
        stats.add_counter("cb_dc_nonzero", u64::from(cb_dc_nonzero));
        stats.add_counter("cr_dc_nonzero", u64::from(cr_dc_nonzero));
        stats.add_counter("cb_ac_nonzero", cb_ac_nonzero as u64);
        stats.add_counter("cr_ac_nonzero", cr_ac_nonzero as u64);
        stats.add_counter(
            "cb_cbf",
            u64::from(cb_dc_nonzero || quantized.cb_tu_has_ac[idx]),
        );
        stats.add_counter(
            "cr_cbf",
            u64::from(cr_dc_nonzero || quantized.cr_tu_has_ac[idx]),
        );
    }
}

#[cfg(feature = "vvc-stats")]
fn add_vvc_luma_motion_aggregate_counters(
    stats: &mut VvcFrameStats,
    prefix: &str,
    summary: motion::VvcLumaMotionAggregateSummary,
) {
    if summary.candidate_count == 0 {
        return;
    }
    stats.add_counter_named(
        &format!("{prefix}_candidate_count"),
        summary.candidate_count as u64,
    );
    stats.add_counter_named(&format!("{prefix}_exact_count"), summary.exact_count as u64);
    stats.add_counter_named(
        &format!("{prefix}_nonzero_count"),
        summary.nonzero_count as u64,
    );
    stats.add_counter_named(
        &format!("{prefix}_nonzero_exact_count"),
        summary.nonzero_exact_count as u64,
    );
    stats.add_counter_named(
        &format!("{prefix}_uniform_count"),
        summary.uniform_count as u64,
    );
    stats.add_counter_named(
        &format!("{prefix}_nonzero_uniform_count"),
        summary.nonzero_uniform_count as u64,
    );
    stats.add_counter_named(
        &format!("{prefix}_uniform_exact_count"),
        summary.uniform_exact_count as u64,
    );
    stats.add_counter_named(
        &format!("{prefix}_nonzero_uniform_exact_count"),
        summary.nonzero_uniform_exact_count as u64,
    );
    stats.add_counter_named(&format!("{prefix}_near_count"), summary.near_count as u64);
    stats.add_counter_named(
        &format!("{prefix}_nonzero_near_count"),
        summary.nonzero_near_count as u64,
    );
    stats.add_counter_named(
        &format!("{prefix}_uniform_near_count"),
        summary.uniform_near_count as u64,
    );
    stats.add_counter_named(
        &format!("{prefix}_nonzero_uniform_near_count"),
        summary.nonzero_uniform_near_count as u64,
    );
    stats.add_counter_named(&format!("{prefix}_total_sad"), summary.total_sad);
}

#[cfg(feature = "vvc-stats")]
fn add_vvc_scc_analysis_counters(
    stats: &mut VvcFrameStats,
    analysis: ibc::VvcSccCtuAnalysis,
) {
    stats.add_counter("scc_8x8_block_count", analysis.block_count as u64);
    stats.add_counter(
        "scc_palette_solid_8x8_count",
        analysis.palette_solid_8x8_count as u64,
    );
    stats.add_counter(
        "scc_palette_no_escape_8x8_count",
        analysis.palette_no_escape_8x8_count as u64,
    );
    stats.add_counter(
        "scc_palette_escape_8x8_count",
        analysis.palette_escape_8x8_count as u64,
    );
    stats.add_counter(
        "scc_ibc_exact_8x8_count",
        analysis.ibc_exact_8x8_count as u64,
    );
    stats.add_counter(
        "scc_ibc_ctu_exact_8x8_count",
        analysis.ibc_ctu_exact_8x8_count as u64,
    );
    stats.add_counter(
        "scc_ibc_ctu_extra_exact_8x8_count",
        analysis.ibc_ctu_extra_exact_8x8_count as u64,
    );
    stats.add_counter(
        "scc_ibc_left_residual_8x8_count",
        analysis.ibc_left_residual_8x8_count as u64,
    );
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Default, Clone, Copy)]
struct VvcTuResidualCodingCounts {
    luma_transform_skip: usize,
    luma_transformed: usize,
    cb_transform_skip: usize,
    cb_transformed: usize,
    cr_transform_skip: usize,
    cr_transformed: usize,
}

#[cfg(feature = "vvc-stats")]
impl VvcTuResidualCodingCounts {
    const fn chroma_transform_skip(self) -> usize {
        self.cb_transform_skip + self.cr_transform_skip
    }

    const fn chroma_transformed(self) -> usize {
        self.cb_transformed + self.cr_transformed
    }
}

#[cfg(feature = "vvc-stats")]
fn vvc_tu_residual_coding_counts(quantized: &VvcQuantizedColor) -> VvcTuResidualCodingCounts {
    let mut counts = VvcTuResidualCodingCounts::default();
    for idx in 0..quantized.luma_tu_count {
        if quantized.luma_tu_transform_skip[idx] {
            counts.luma_transform_skip += 1;
        } else {
            counts.luma_transformed += 1;
        }
    }
    for idx in 0..quantized.chroma_tu_count {
        if quantized.cb_tu_transform_skip[idx] {
            counts.cb_transform_skip += 1;
        } else {
            counts.cb_transformed += 1;
        }
        if quantized.cr_tu_transform_skip[idx] {
            counts.cr_transform_skip += 1;
        } else {
            counts.cr_transformed += 1;
        }
    }
    counts
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Default, Clone, Copy)]
struct VvcTuBdpcmCounts {
    luma_horizontal: usize,
    luma_vertical: usize,
    chroma_horizontal: usize,
    chroma_vertical: usize,
}

#[cfg(feature = "vvc-stats")]
fn vvc_tu_bdpcm_counts(quantized: &VvcQuantizedColor) -> VvcTuBdpcmCounts {
    let mut counts = VvcTuBdpcmCounts::default();
    for idx in 0..quantized.luma_tu_count {
        match quantized.luma_tu_bdpcm_modes[idx] {
            VvcBdpcmMode::None => {}
            VvcBdpcmMode::Horizontal => counts.luma_horizontal += 1,
            VvcBdpcmMode::Vertical => counts.luma_vertical += 1,
        }
    }
    for idx in 0..quantized.chroma_tu_count {
        match quantized.chroma_tu_bdpcm_modes[idx] {
            VvcBdpcmMode::None => {}
            VvcBdpcmMode::Horizontal => counts.chroma_horizontal += 1,
            VvcBdpcmMode::Vertical => counts.chroma_vertical += 1,
        }
    }
    counts
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Default, Clone, Copy)]
struct VvcLumaTuMtsCounts {
    nonzero: usize,
    dst7_dst7: usize,
    dct8_dst7: usize,
    dst7_dct8: usize,
    dct8_dct8: usize,
}

#[cfg(feature = "vvc-stats")]
fn vvc_luma_tu_mts_counts(quantized: &VvcQuantizedColor) -> VvcLumaTuMtsCounts {
    let mut counts = VvcLumaTuMtsCounts::default();
    for idx in 0..quantized.luma_tu_count {
        match quantized.luma_tu_mts_index[idx] {
            0 => {}
            2 => {
                counts.nonzero += 1;
                counts.dst7_dst7 += 1;
            }
            3 => {
                counts.nonzero += 1;
                counts.dct8_dst7 += 1;
            }
            4 => {
                counts.nonzero += 1;
                counts.dst7_dct8 += 1;
            }
            5 => {
                counts.nonzero += 1;
                counts.dct8_dct8 += 1;
            }
            _ => counts.nonzero += 1,
        }
    }
    counts
}

#[cfg(feature = "vvc-stats")]
fn add_vvc_mode_index_counters(stats: &mut VvcFrameStats, prefix: &str, counts: &[usize; 67]) {
    for (index, count) in counts.iter().copied().enumerate() {
        if count == 0 {
            continue;
        }
        stats.add_counter_named(&format!("{prefix}{index:02}"), count as u64);
    }
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Clone, Copy)]
struct VvcLumaModeCounts {
    dc: usize,
    planar: usize,
    horizontal: usize,
    vertical: usize,
    angular: usize,
    angular_by_index: [usize; 67],
}

#[cfg(feature = "vvc-stats")]
impl Default for VvcLumaModeCounts {
    fn default() -> Self {
        Self {
            dc: 0,
            planar: 0,
            horizontal: 0,
            vertical: 0,
            angular: 0,
            angular_by_index: [0; 67],
        }
    }
}

#[cfg(feature = "vvc-stats")]
fn vvc_luma_mode_counts(quantized: &VvcQuantizedColor) -> VvcLumaModeCounts {
    let mut counts = VvcLumaModeCounts::default();
    for idx in 0..quantized.luma_tu_count {
        match quantized.luma_tu_intra_modes[idx] {
            VvcIntraPredictionMode::Dc => counts.dc += 1,
            VvcIntraPredictionMode::Planar => counts.planar += 1,
            VvcIntraPredictionMode::Horizontal => counts.horizontal += 1,
            VvcIntraPredictionMode::Vertical => counts.vertical += 1,
            VvcIntraPredictionMode::Angular(index) => {
                counts.angular += 1;
                counts.angular_by_index[usize::from(index)] += 1;
            }
        }
    }
    counts
}

#[cfg(feature = "vvc-stats")]
#[derive(Debug, Clone, Copy)]
struct VvcChromaModeCounts {
    derived: usize,
    dc: usize,
    planar: usize,
    horizontal: usize,
    vertical: usize,
    angular: usize,
    angular_by_index: [usize; 67],
    cclm: usize,
    cclm_linear: usize,
    mdlm_left: usize,
    mdlm_top: usize,
}

#[cfg(feature = "vvc-stats")]
impl Default for VvcChromaModeCounts {
    fn default() -> Self {
        Self {
            derived: 0,
            dc: 0,
            planar: 0,
            horizontal: 0,
            vertical: 0,
            angular: 0,
            angular_by_index: [0; 67],
            cclm: 0,
            cclm_linear: 0,
            mdlm_left: 0,
            mdlm_top: 0,
        }
    }
}

#[cfg(feature = "vvc-stats")]
fn vvc_chroma_mode_counts(quantized: &VvcQuantizedColor) -> VvcChromaModeCounts {
    let mut counts = VvcChromaModeCounts::default();
    for idx in 0..quantized.chroma_tu_count {
        match quantized.chroma_tu_intra_modes[idx] {
            VvcChromaIntraPredictionMode::Derived => counts.derived += 1,
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Dc) => counts.dc += 1,
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Planar) => {
                counts.planar += 1
            }
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal) => {
                counts.horizontal += 1
            }
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Vertical) => {
                counts.vertical += 1
            }
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Angular(index)) => {
                counts.angular += 1;
                counts.angular_by_index[usize::from(index)] += 1;
            }
            VvcChromaIntraPredictionMode::Cclm(mode) => {
                counts.cclm += 1;
                match mode {
                    VvcChromaCclmMode::Linear => counts.cclm_linear += 1,
                    VvcChromaCclmMode::MdlmLeft => counts.mdlm_left += 1,
                    VvcChromaCclmMode::MdlmTop => counts.mdlm_top += 1,
                }
            }
        }
    }
    counts
}
