#[derive(Debug, Clone, Copy)]
struct VvcSelectedChromaTuCandidate {
    mode: VvcChromaIntraPredictionMode,
    coding_decision: VvcChromaTuCodingDecision,
    residual: Option<VvcScoredSelectedChromaResidual>,
}

struct VvcChromaInterCandidateContext<'a> {
    policy: VvcResidualCodingPolicy,
    source_frame: &'a VvcSampledFrame,
    node: VvcCodingTreeNode,
    chroma_x: usize,
    chroma_y: usize,
    chroma_width: usize,
    chroma_height: usize,
}

impl VvcChromaInterCandidateContext<'_> {
    fn select_candidate(
        &self,
        decision: VvcLumaInterDecision,
        reference: &VvcReconstructionFrame,
        buffers: &mut VvcChromaCandidateBuffers<'_>,
        stats: &mut VvcIntraSearchStats,
    ) -> Option<VvcSelectedChromaTuCandidate> {
        if !VvcReconstructionFrame::predict_chroma_node_from_inter_motion_into(
            reference,
            buffers.prediction.cb,
            buffers.prediction.cr,
            self.node,
            decision,
        ) {
            return None;
        }
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        let (cb_all_zero, cr_all_zero) = residual_chroma_pair_tu_at_into_and_detect_zero(
            buffers.residuals.cb,
            buffers.residuals.cr,
            &self.source_frame.cb,
            &self.source_frame.cr,
            self.source_frame.geometry,
            self.source_frame.format,
            self.chroma_x,
            self.chroma_y,
            self.chroma_width,
            self.chroma_height,
            buffers.prediction.cb,
            buffers.prediction.cr,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(not(feature = "vvc-stats"))]
        let _ = stats;
        let mode = VvcChromaIntraPredictionMode::Derived;
        Some(VvcSelectedChromaTuCandidate {
            mode,
            coding_decision: self
                .policy
                .select_chroma_tu_coding_decision(self.node, mode),
            residual: (cb_all_zero && cr_all_zero)
                .then_some(vvc_zero_chroma_preselected_residual()),
        })
    }
}

struct VvcChromaTuSelectionContext<'a> {
    policy: VvcResidualCodingPolicy,
    metric: VvcResidualScoreMetric,
    source_frame: &'a VvcSampledFrame,
    frame_recon: &'a VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    co_located_luma_mode: VvcIntraPredictionMode,
    chroma_x: usize,
    chroma_y: usize,
    chroma_width: usize,
    chroma_height: usize,
    cclm_enabled: bool,
    syntax_tie_breaker_enabled: bool,
    chroma_qp: i32,
    chroma_ts_quant: &'a VvcTransformSkipQuantTable,
    temporal_hint: Option<VvcChromaTemporalModeHint>,
}

struct VvcChromaTuSelectionBuffers<'a> {
    cache: &'a mut VvcChromaModeRdCache,
    prediction_scratch: &'a mut VvcIntraPredictionScratch,
    selected: VvcChromaCandidateBuffers<'a>,
    candidate: VvcChromaCandidateBuffers<'a>,
    stats: &'a mut VvcIntraSearchStats,
    transform_scratch: &'a mut VvcInverseTransformScratch,
    reconstructed_residual: &'a mut Vec<i16>,
}

impl VvcChromaTuSelectionContext<'_> {
    fn select_candidate(
        &self,
        buffers: VvcChromaTuSelectionBuffers<'_>,
    ) -> VvcSelectedChromaTuCandidate {
        let VvcChromaTuSelectionBuffers {
            cache,
            prediction_scratch,
            mut selected,
            candidate,
            stats,
            transform_scratch,
            reconstructed_residual,
        } = buffers;
        if let Some(hint) = self.temporal_hint {
            let temporal_candidate =
                self.select_temporal_hint_candidate(hint, prediction_scratch, &mut selected, stats);
            if let Some(candidate) = temporal_candidate {
                return candidate;
            }
        }
        let mode_search_context = VvcChromaModeSearchContext {
            policy: self.policy,
            metric: self.metric,
            source_frame: self.source_frame,
            frame_recon: self.frame_recon,
            node: self.node,
            co_located_luma_mode: self.co_located_luma_mode,
            chroma_x: self.chroma_x,
            chroma_y: self.chroma_y,
            chroma_width: self.chroma_width,
            chroma_height: self.chroma_height,
            cclm_enabled: self.cclm_enabled,
            syntax_tie_breaker_enabled: self.syntax_tie_breaker_enabled,
        };
        #[cfg(feature = "vvc-stats")]
        let mode_search_start = StageStart::now();
        let VvcChromaModeSearchResult {
            mode: raw_mode,
            candidate_costs,
        } = mode_search_context.select_intra_mode(VvcChromaModeSearchBuffers {
            cache,
            prediction_scratch,
            selected_prediction: VvcChromaPredictionBuffers {
                cb: selected.prediction.cb,
                cr: selected.prediction.cr,
            },
            candidate_prediction: VvcChromaPredictionBuffers {
                cb: candidate.prediction.cb,
                cr: candidate.prediction.cr,
            },
            candidate_residuals: VvcChromaResidualBuffers {
                cb: candidate.residuals.cb,
                cr: candidate.residuals.cr,
            },
            stats,
        });
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_mode_search_nanos(mode_search_start.elapsed().as_nanos() as u64);

        if !cache.take_residuals_if_present(raw_mode, selected.residuals.cb, selected.residuals.cr)
        {
            self.materialize_residuals(&mut selected, stats);
        }

        let refinement_context = VvcChromaRefinementContext {
            policy: self.policy,
            node: self.node,
            co_located_luma_mode: self.co_located_luma_mode,
            cclm_syntax_enabled: self.cclm_enabled,
            source_frame: self.source_frame,
            frame_recon: self.frame_recon,
            chroma_x: self.chroma_x,
            chroma_y: self.chroma_y,
            chroma_width: self.chroma_width,
            chroma_height: self.chroma_height,
            chroma_qp: self.chroma_qp,
            chroma_ts_quant: self.chroma_ts_quant,
        };
        let mut refinement_buffers = VvcChromaRefinementBuffers {
            prediction_scratch,
            selected,
            candidate,
            stats,
            transform_scratch,
            reconstructed_residual,
        };

        #[cfg(feature = "vvc-stats")]
        let rd_start = StageStart::now();
        let selected_mode = if vvc_chroma_lossy_speed_direct_bdpcm_candidates_allowed(
            self.policy,
            self.source_frame.format.chroma_sampling,
            self.source_frame.format.bit_depth,
            raw_mode,
        ) {
            VvcSelectedChromaMode {
                mode: raw_mode,
                residual: None,
            }
        } else {
            refinement_context.select_mode_with_rd_refinement(
                raw_mode,
                candidate_costs,
                cache,
                &mut refinement_buffers,
            )
        };
        #[cfg(feature = "vvc-stats")]
        refinement_buffers
            .stats
            .add_chroma_rd_refinement_nanos(rd_start.elapsed().as_nanos() as u64);
        #[cfg(feature = "vvc-stats")]
        if selected_mode.residual.is_some() {
            refinement_buffers.stats.add_chroma_rd_refinement_attempt();
            if selected_mode.mode != raw_mode {
                refinement_buffers.stats.add_chroma_rd_refinement_switch();
            }
        }

        let mut mode = selected_mode.mode;
        let mut selected_residual = selected_mode.residual;
        #[cfg(feature = "vvc-stats")]
        let bdpcm_start = StageStart::now();
        if let Some(selected_bdpcm) = refinement_context.select_bdpcm_prediction(
            mode,
            selected_residual,
            &mut refinement_buffers,
        ) {
            mode = selected_bdpcm.mode;
            selected_residual = Some(selected_bdpcm.residual);
        }
        #[cfg(feature = "vvc-stats")]
        refinement_buffers
            .stats
            .add_chroma_bdpcm_nanos(bdpcm_start.elapsed().as_nanos() as u64);

        VvcSelectedChromaTuCandidate {
            mode,
            coding_decision: self
                .policy
                .select_chroma_tu_coding_decision(self.node, mode),
            residual: selected_residual,
        }
    }

    fn materialize_residuals(
        &self,
        buffers: &mut VvcChromaCandidateBuffers<'_>,
        stats: &mut VvcIntraSearchStats,
    ) {
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_chroma_tu_at_into(
            buffers.residuals.cb,
            &self.source_frame.cb,
            self.source_frame.geometry,
            self.source_frame.format,
            self.chroma_x,
            self.chroma_y,
            self.chroma_width,
            self.chroma_height,
            buffers.prediction.cb,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_chroma_tu_at_into(
            buffers.residuals.cr,
            &self.source_frame.cr,
            self.source_frame.geometry,
            self.source_frame.format,
            self.chroma_x,
            self.chroma_y,
            self.chroma_width,
            self.chroma_height,
            buffers.prediction.cr,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(not(feature = "vvc-stats"))]
        let _ = stats;
    }
}

fn vvc_zero_chroma_preselected_residual() -> VvcScoredSelectedChromaResidual {
    VvcScoredSelectedChromaResidual::preselected(VvcSelectedChromaResidual {
        cb: VvcFinalizedResidualBlock::zero_transform_skip(VvcBdpcmMode::None),
        cr: VvcFinalizedResidualBlock::zero_transform_skip(VvcBdpcmMode::None),
    })
}
