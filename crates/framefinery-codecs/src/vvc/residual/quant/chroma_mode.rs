struct VvcChromaRefinementContext<'a> {
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
    co_located_luma_mode: VvcIntraPredictionMode,
    cclm_syntax_enabled: bool,
    source_frame: &'a VvcSampledFrame,
    frame_recon: &'a VvcReconstructionFrame,
    chroma_x: usize,
    chroma_y: usize,
    chroma_width: usize,
    chroma_height: usize,
    chroma_qp: i32,
    chroma_ts_quant: &'a VvcTransformSkipQuantTable,
}

struct VvcChromaCandidateBuffers<'a> {
    cb_prediction: &'a mut Vec<VvcSample>,
    cr_prediction: &'a mut Vec<VvcSample>,
    cb_residuals: &'a mut Vec<i16>,
    cr_residuals: &'a mut Vec<i16>,
}

struct VvcChromaRefinementBuffers<'a> {
    prediction_scratch: &'a mut VvcDcPredictionScratch,
    selected: VvcChromaCandidateBuffers<'a>,
    candidate: VvcChromaCandidateBuffers<'a>,
    stats: &'a mut VvcIntraSearchStats,
    transform_scratch: &'a mut VvcInverseTransformScratch,
    reconstructed_residual: &'a mut Vec<i16>,
}

impl VvcChromaRefinementBuffers<'_> {
    fn promote_candidate(&mut self) {
        std::mem::swap(self.selected.cb_prediction, self.candidate.cb_prediction);
        std::mem::swap(self.selected.cr_prediction, self.candidate.cr_prediction);
        std::mem::swap(self.selected.cb_residuals, self.candidate.cb_residuals);
        std::mem::swap(self.selected.cr_residuals, self.candidate.cr_residuals);
    }
}

impl VvcChromaRefinementContext<'_> {
    fn select_mode_with_rd_refinement(
        &self,
        raw_mode: VvcChromaIntraPredictionMode,
        candidate_costs: VvcChromaIntraCandidateCosts,
        rd_cache: &mut VvcChromaModeRdCache,
        buffers: &mut VvcChromaRefinementBuffers<'_>,
    ) -> VvcSelectedChromaMode {
        let raw_decision = self
            .policy
            .select_chroma_tu_coding_decision(self.node, raw_mode);
        if !vvc_chroma_lossy_rd_refinement_allowed(self.policy, self.node, raw_decision) {
            return VvcSelectedChromaMode {
                mode: raw_mode,
                residual: None,
            };
        }
        if vvc_chroma_exact_prediction_skips_rd(
            buffers.selected.cb_residuals,
            buffers.selected.cr_residuals,
        ) {
            return VvcSelectedChromaMode {
                mode: raw_mode,
                residual: None,
            };
        }

        let mut best_mode = raw_mode;
        #[cfg(feature = "vvc-stats")]
        let score_start = StageStart::now();
        let mut best_candidate = score_vvc_chroma_mode_rd_candidate(
            self.policy,
            raw_decision,
            raw_mode,
            self.cclm_syntax_enabled,
            buffers.selected.cb_residuals,
            buffers.selected.cr_residuals,
            self.chroma_width,
            self.chroma_height,
            self.source_frame.format.bit_depth,
            self.chroma_qp,
            self.chroma_ts_quant,
            buffers.stats,
            buffers.transform_scratch,
            buffers.reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        buffers
            .stats
            .add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
        let shortlist =
            VvcChromaModeRdShortlist::from_candidate_costs(self.policy, candidate_costs);
        for candidate in shortlist.iter() {
            if self.policy.residual_mode() == VvcResidualCodingMode::Lossy
                && self.policy.fast_search() == VvcFastSearch::LosslessSpeed
                && self.policy.chroma_sampling() == ChromaSampling::Cs420
                && !shortlist.admits_lossless_speed_rd(candidate)
            {
                continue;
            }
            let mode = candidate.mode();
            if mode == raw_mode {
                continue;
            }
            let coding_decision = self
                .policy
                .select_chroma_tu_coding_decision(self.node, mode);
            if !matches!(
                coding_decision.residual_coding,
                VvcTuResidualCodingMode::Transformed
            ) {
                continue;
            }
            if let Some(cached) = rd_cache.get(mode) {
                #[cfg(feature = "vvc-stats")]
                buffers.stats.add_chroma_rd_cached_candidate();
                #[cfg(feature = "vvc-stats")]
                let score_start = StageStart::now();
                let rd_candidate = score_vvc_chroma_mode_rd_candidate(
                    self.policy,
                    coding_decision,
                    mode,
                    self.cclm_syntax_enabled,
                    &cached.cb_residuals,
                    &cached.cr_residuals,
                    self.chroma_width,
                    self.chroma_height,
                    self.source_frame.format.bit_depth,
                    self.chroma_qp,
                    self.chroma_ts_quant,
                    buffers.stats,
                    buffers.transform_scratch,
                    buffers.reconstructed_residual,
                );
                #[cfg(feature = "vvc-stats")]
                buffers
                    .stats
                    .add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
                if rd_candidate.selects_over(best_candidate) {
                    best_mode = mode;
                    best_candidate = rd_candidate;
                    #[cfg(feature = "vvc-stats")]
                    let prediction_start = StageStart::now();
                    predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                        buffers.selected.cb_prediction,
                        buffers.selected.cr_prediction,
                        buffers.prediction_scratch,
                        mode,
                        self.co_located_luma_mode,
                        &self.frame_recon.cb,
                        &self.frame_recon.cr,
                        &self.frame_recon.luma,
                        self.frame_recon.coded_geometry(),
                        self.node,
                        self.source_frame.format.chroma_sampling,
                        self.source_frame.format.bit_depth,
                        Some(self.frame_recon.cb_availability()),
                        Some(self.frame_recon.cr_availability()),
                        Some(self.frame_recon.luma_availability()),
                    );
                    #[cfg(feature = "vvc-stats")]
                    buffers
                        .stats
                        .add_chroma_rd_prediction_nanos(vvc_elapsed_nanos(prediction_start));
                    assert!(
                        rd_cache.take_residuals_if_present(
                            mode,
                            buffers.selected.cb_residuals,
                            buffers.selected.cr_residuals,
                        ),
                        "cached chroma mode disappeared before residual transfer",
                    );
                }
                continue;
            }
            #[cfg(feature = "vvc-stats")]
            buffers.stats.add_chroma_rd_generated_candidate();
            #[cfg(feature = "vvc-stats")]
            let prediction_start = StageStart::now();
            predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                buffers.candidate.cb_prediction,
                buffers.candidate.cr_prediction,
                buffers.prediction_scratch,
                mode,
                self.co_located_luma_mode,
                &self.frame_recon.cb,
                &self.frame_recon.cr,
                &self.frame_recon.luma,
                self.frame_recon.coded_geometry(),
                self.node,
                self.source_frame.format.chroma_sampling,
                self.source_frame.format.bit_depth,
                Some(self.frame_recon.cb_availability()),
                Some(self.frame_recon.cr_availability()),
                Some(self.frame_recon.luma_availability()),
            );
            #[cfg(feature = "vvc-stats")]
            {
                let nanos = vvc_elapsed_nanos(prediction_start);
                buffers.stats.add_chroma_rd_prediction_nanos(nanos);
                buffers
                    .stats
                    .add_chroma_prediction_nanos(vvc_chroma_prediction_stats_family(mode), nanos);
            }
            #[cfg(feature = "vvc-stats")]
            let residual_start = StageStart::now();
            residual_chroma_pair_tu_at_into(
                buffers.candidate.cb_residuals,
                buffers.candidate.cr_residuals,
                &self.source_frame.cb,
                &self.source_frame.cr,
                self.source_frame.geometry,
                self.source_frame.format,
                self.chroma_x,
                self.chroma_y,
                self.chroma_width,
                self.chroma_height,
                buffers.candidate.cb_prediction,
                buffers.candidate.cr_prediction,
            );
            #[cfg(feature = "vvc-stats")]
            {
                let nanos = vvc_elapsed_nanos(residual_start);
                buffers.stats.add_chroma_residual_build_nanos(nanos);
                buffers.stats.add_chroma_rd_residual_build_nanos(nanos);
            }
            #[cfg(feature = "vvc-stats")]
            let score_start = StageStart::now();
            let rd_candidate = score_vvc_chroma_mode_rd_candidate(
                self.policy,
                coding_decision,
                mode,
                self.cclm_syntax_enabled,
                buffers.candidate.cb_residuals,
                buffers.candidate.cr_residuals,
                self.chroma_width,
                self.chroma_height,
                self.source_frame.format.bit_depth,
                self.chroma_qp,
                self.chroma_ts_quant,
                buffers.stats,
                buffers.transform_scratch,
                buffers.reconstructed_residual,
            );
            #[cfg(feature = "vvc-stats")]
            buffers
                .stats
                .add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
            if rd_candidate.selects_over(best_candidate) {
                best_mode = mode;
                best_candidate = rd_candidate;
                buffers.promote_candidate();
            }
        }

        VvcSelectedChromaMode {
            mode: best_mode,
            residual: Some(best_candidate.residual),
        }
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
