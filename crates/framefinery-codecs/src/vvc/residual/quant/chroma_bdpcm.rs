impl VvcChromaRefinementContext<'_> {
    fn select_bdpcm_prediction(
        &self,
        selected_mode: VvcChromaIntraPredictionMode,
        selected_residual: Option<VvcScoredSelectedChromaResidual>,
        buffers: &mut VvcChromaRefinementBuffers<'_>,
    ) -> Option<VvcSelectedChromaBdpcm> {
        if !vvc_chroma_bdpcm_selection_allowed(self.policy, self.chroma_width, self.chroma_height)
            || !vvc_chroma_lossless_speed_bdpcm_format_allowed(
                self.policy,
                self.source_frame.format,
            )
            || !vvc_chroma_bdpcm_fast_search_allowed(self.policy, selected_mode)
        {
            return None;
        }

        let baseline_decision = self
            .policy
            .select_chroma_tu_coding_decision(self.node, selected_mode);
        let baseline_residual = selected_residual.unwrap_or_else(|| {
            #[cfg(feature = "vvc-stats")]
            let score_start = StageStart::now();
            let residual = VvcSelectedChromaResidual {
                cb: finalize_vvc_chroma_residual_block(
                    baseline_decision.residual_coding,
                    buffers.selected.cb_residuals,
                    self.chroma_width,
                    self.chroma_height,
                    self.source_frame.format.bit_depth,
                    self.chroma_qp,
                    self.chroma_ts_quant,
                    buffers.stats,
                    buffers.transform_scratch,
                    buffers.reconstructed_residual,
                ),
                cr: finalize_vvc_chroma_residual_block(
                    baseline_decision.residual_coding,
                    buffers.selected.cr_residuals,
                    self.chroma_width,
                    self.chroma_height,
                    self.source_frame.format.bit_depth,
                    self.chroma_qp,
                    self.chroma_ts_quant,
                    buffers.stats,
                    buffers.transform_scratch,
                    buffers.reconstructed_residual,
                ),
            };
            let residual = VvcScoredSelectedChromaResidual::new(
                buffers.selected.cb_residuals,
                buffers.selected.cr_residuals,
                self.chroma_width,
                self.chroma_height,
                self.source_frame.format.bit_depth,
                self.chroma_qp,
                self.chroma_ts_quant,
                residual,
                buffers.transform_scratch,
                buffers.reconstructed_residual,
            );
            #[cfg(feature = "vvc-stats")]
            buffers
                .stats
                .add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
            residual
        });
        let mut best_score = vvc_scored_chroma_quantized_residual_score(
            baseline_residual,
            u64::from(vvc_bdpcm_mode_syntax_bin_count(VvcBdpcmMode::None)).saturating_add(
                u64::from(vvc_chroma_intra_mode_syntax_bin_count(
                    selected_mode,
                    self.cclm_syntax_enabled,
                )),
            ),
        );

        for bdpcm_mode in vvc_chroma_lossy_speed_direct_bdpcm_candidate_modes(
            self.policy,
            self.source_frame.format.chroma_sampling,
            self.source_frame.format.bit_depth,
            selected_mode,
            self.co_located_luma_mode,
        )
        .into_iter()
        .flatten()
        {
            #[cfg(feature = "vvc-stats")]
            buffers.stats.add_chroma_bdpcm_direct_candidate();
            build_vvc_chroma_bdpcm_candidate(
                self,
                bdpcm_mode,
                buffers.stats,
                buffers.prediction_scratch,
                &mut buffers.candidate,
            );
            let direct_bdpcm_safe = vvc_chroma_direct_bdpcm_residual_is_safe(
                buffers.selected.cb_residuals,
                buffers.selected.cr_residuals,
                buffers.candidate.cb_residuals,
                buffers.candidate.cr_residuals,
            );
            #[cfg(feature = "vvc-stats")]
            if direct_bdpcm_safe {
                buffers.stats.add_chroma_bdpcm_direct_safe_candidate();
            }
            if !direct_bdpcm_safe {
                continue;
            }
            let (residual, candidate_score) = score_vvc_chroma_bdpcm_candidate(
                bdpcm_mode,
                self.chroma_width,
                self.chroma_height,
                self.chroma_qp,
                self.chroma_ts_quant,
                self.source_frame.format.bit_depth,
                buffers.stats,
                buffers.candidate.cb_residuals,
                buffers.candidate.cr_residuals,
                buffers.transform_scratch,
                buffers.reconstructed_residual,
            );
            if candidate_score.selects_over(best_score) {
                #[cfg(feature = "vvc-stats")]
                buffers.stats.add_chroma_bdpcm_direct_selected();
                buffers.promote_candidate();
                let mode = VvcChromaIntraPredictionMode::Explicit(
                    bdpcm_mode
                        .inferred_intra_mode()
                        .expect("enabled BDPCM mode has an inferred intra mode"),
                );
                return Some(VvcSelectedChromaBdpcm { mode, residual });
            }
        }

        let mut best = None;

        for bdpcm_mode in vvc_chroma_bdpcm_candidate_modes(self.policy, self.co_located_luma_mode)
            .into_iter()
            .flatten()
        {
            #[cfg(feature = "vvc-stats")]
            buffers.stats.add_chroma_bdpcm_regular_candidate();
            build_vvc_chroma_bdpcm_candidate(
                self,
                bdpcm_mode,
                buffers.stats,
                buffers.prediction_scratch,
                &mut buffers.candidate,
            );
            let (residual, candidate_score) = score_vvc_chroma_bdpcm_candidate(
                bdpcm_mode,
                self.chroma_width,
                self.chroma_height,
                self.chroma_qp,
                self.chroma_ts_quant,
                self.source_frame.format.bit_depth,
                buffers.stats,
                buffers.candidate.cb_residuals,
                buffers.candidate.cr_residuals,
                buffers.transform_scratch,
                buffers.reconstructed_residual,
            );
            if candidate_score.selects_over(best_score) {
                #[cfg(feature = "vvc-stats")]
                buffers.stats.add_chroma_bdpcm_regular_best_update();
                best_score = candidate_score;
                let mode = VvcChromaIntraPredictionMode::Explicit(
                    bdpcm_mode
                        .inferred_intra_mode()
                        .expect("enabled BDPCM mode has an inferred intra mode"),
                );
                best = Some(VvcSelectedChromaBdpcm { mode, residual });
                buffers.promote_candidate();
            }
        }

        best
    }
}

fn build_vvc_chroma_bdpcm_candidate(
    context: &VvcChromaRefinementContext<'_>,
    bdpcm_mode: VvcBdpcmMode,
    stats: &mut VvcIntraSearchStats,
    prediction_scratch: &mut VvcDcPredictionScratch,
    candidate: &mut VvcChromaCandidateBuffers<'_>,
) {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    #[cfg(feature = "vvc-stats")]
    let prediction_start = StageStart::now();
    predict_vvc_chroma_bdpcm_block_into_with_availability(
        candidate.cb_prediction,
        prediction_scratch,
        bdpcm_mode,
        &context.frame_recon.cb,
        context.frame_recon.coded_geometry(),
        context.node,
        context.source_frame.format.chroma_sampling,
        context.source_frame.format.bit_depth,
        Some(context.frame_recon.cb_availability()),
    );
    predict_vvc_chroma_bdpcm_block_into_with_availability(
        candidate.cr_prediction,
        prediction_scratch,
        bdpcm_mode,
        &context.frame_recon.cr,
        context.frame_recon.coded_geometry(),
        context.node,
        context.source_frame.format.chroma_sampling,
        context.source_frame.format.bit_depth,
        Some(context.frame_recon.cr_availability()),
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_prediction_nanos(
        VvcChromaPredictionStatsFamily::Bdpcm,
        vvc_elapsed_nanos(prediction_start),
    );

    #[cfg(feature = "vvc-stats")]
    let residual_start = StageStart::now();
    residual_chroma_pair_tu_at_into(
        candidate.cb_residuals,
        candidate.cr_residuals,
        &context.source_frame.cb,
        &context.source_frame.cr,
        context.source_frame.geometry,
        context.source_frame.format,
        context.chroma_x,
        context.chroma_y,
        context.chroma_width,
        context.chroma_height,
        candidate.cb_prediction,
        candidate.cr_prediction,
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
}

fn score_vvc_chroma_bdpcm_candidate(
    bdpcm_mode: VvcBdpcmMode,
    chroma_width: usize,
    chroma_height: usize,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    bit_depth: SampleBitDepth,
    stats: &mut VvcIntraSearchStats,
    candidate_cb_residuals: &[i16],
    candidate_cr_residuals: &[i16],
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> (
    VvcScoredSelectedChromaResidual,
    VvcChromaQuantizedResidualScore,
) {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    #[cfg(feature = "vvc-stats")]
    let score_start = StageStart::now();
    let finalized = VvcSelectedChromaResidual {
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
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
        finalized,
        transform_scratch,
        reconstructed_residual,
    );
    let score = vvc_scored_chroma_quantized_residual_score(
        residual,
        u64::from(vvc_bdpcm_mode_syntax_bin_count(bdpcm_mode)),
    );
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
    (residual, score)
}

fn vvc_chroma_bdpcm_candidate_modes(
    policy: VvcResidualCodingPolicy,
    co_located_luma_mode: VvcIntraPredictionMode,
) -> [Option<VvcBdpcmMode>; 2] {
    if policy.fast_search() == VvcFastSearch::LosslessSpeed {
        match co_located_luma_mode {
            VvcIntraPredictionMode::Horizontal => [Some(VvcBdpcmMode::Horizontal), None],
            VvcIntraPredictionMode::Vertical => [Some(VvcBdpcmMode::Vertical), None],
            VvcIntraPredictionMode::Planar
            | VvcIntraPredictionMode::Dc
            | VvcIntraPredictionMode::Angular(_) => {
                [Some(VvcBdpcmMode::Horizontal), Some(VvcBdpcmMode::Vertical)]
            }
        }
    } else {
        [Some(VvcBdpcmMode::Horizontal), Some(VvcBdpcmMode::Vertical)]
    }
}

fn vvc_chroma_lossy_speed_direct_bdpcm_candidate_modes(
    policy: VvcResidualCodingPolicy,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    selected_mode: VvcChromaIntraPredictionMode,
    co_located_luma_mode: VvcIntraPredictionMode,
) -> [Option<VvcBdpcmMode>; 2] {
    if !vvc_chroma_lossy_speed_direct_bdpcm_candidates_allowed(
        policy,
        chroma_sampling,
        bit_depth,
        selected_mode,
    ) {
        return [None, None];
    }
    match co_located_luma_mode {
        VvcIntraPredictionMode::Horizontal => [Some(VvcBdpcmMode::Horizontal), None],
        VvcIntraPredictionMode::Vertical => [Some(VvcBdpcmMode::Vertical), None],
        VvcIntraPredictionMode::Planar
        | VvcIntraPredictionMode::Dc
        | VvcIntraPredictionMode::Angular(_) => {
            [Some(VvcBdpcmMode::Horizontal), Some(VvcBdpcmMode::Vertical)]
        }
    }
}

fn vvc_chroma_lossy_speed_direct_bdpcm_candidates_allowed(
    policy: VvcResidualCodingPolicy,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    selected_mode: VvcChromaIntraPredictionMode,
) -> bool {
    policy.residual_mode() == VvcResidualCodingMode::Lossy
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
        && chroma_sampling == ChromaSampling::Cs444
        && bit_depth.bits() == 8
        && matches!(selected_mode, VvcChromaIntraPredictionMode::Derived)
}

fn vvc_chroma_direct_bdpcm_residual_is_safe(
    selected_cb_residuals: &[i16],
    selected_cr_residuals: &[i16],
    candidate_cb_residuals: &[i16],
    candidate_cr_residuals: &[i16],
) -> bool {
    let selected_sse = vvc_chroma_pair_residual_sse(selected_cb_residuals, selected_cr_residuals);
    let candidate_sse =
        vvc_chroma_pair_residual_sse(candidate_cb_residuals, candidate_cr_residuals);
    // Bypass the RD check only when BDPCM materially improves raw prediction SSE.
    candidate_sse.saturating_mul(16) <= selected_sse.saturating_mul(15)
}

fn vvc_chroma_pair_residual_sse(cb_residuals: &[i16], cr_residuals: &[i16]) -> u64 {
    cb_residuals
        .iter()
        .chain(cr_residuals.iter())
        .fold(0u64, |sse, residual| {
            let residual = i64::from(*residual);
            sse.saturating_add((residual * residual) as u64)
        })
}

fn vvc_chroma_bdpcm_fast_search_allowed(
    policy: VvcResidualCodingPolicy,
    selected_mode: VvcChromaIntraPredictionMode,
) -> bool {
    if policy.residual_mode() == VvcResidualCodingMode::Lossless
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
    {
        return matches!(selected_mode, VvcChromaIntraPredictionMode::Derived);
    }
    match policy.fast_search() {
        VvcFastSearch::Off | VvcFastSearch::Conservative => true,
        VvcFastSearch::LosslessSpeed if policy.residual_mode() == VvcResidualCodingMode::Lossy => {
            true
        }
        VvcFastSearch::Moderate | VvcFastSearch::LosslessSpeed => {
            matches!(selected_mode, VvcChromaIntraPredictionMode::Derived)
                || vvc_chroma_mode_is_bdpcm_aligned(selected_mode)
        }
        VvcFastSearch::Aggressive => vvc_chroma_mode_is_bdpcm_aligned(selected_mode),
    }
}

fn vvc_chroma_lossless_speed_bdpcm_format_allowed(
    policy: VvcResidualCodingPolicy,
    format: VvcPictureFormat,
) -> bool {
    if policy.residual_mode() != VvcResidualCodingMode::Lossless
        || policy.fast_search() != VvcFastSearch::LosslessSpeed
    {
        return true;
    }
    format.bit_depth.bits() == 8 || format.chroma_sampling != ChromaSampling::Cs420
}

fn vvc_chroma_mode_is_bdpcm_aligned(mode: VvcChromaIntraPredictionMode) -> bool {
    matches!(
        mode,
        VvcChromaIntraPredictionMode::Explicit(
            VvcIntraPredictionMode::Horizontal | VvcIntraPredictionMode::Vertical
        )
    )
}

fn vvc_chroma_bdpcm_selection_allowed(
    policy: VvcResidualCodingPolicy,
    chroma_width: usize,
    chroma_height: usize,
) -> bool {
    VVC_ENABLE_BDPCM_SELECTION
        && chroma_width == 4
        && chroma_height == 4
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
