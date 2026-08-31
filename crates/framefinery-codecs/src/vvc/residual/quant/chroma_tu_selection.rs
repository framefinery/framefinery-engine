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

struct VvcChromaInterCandidateBuffers<'a> {
    cb_prediction: &'a mut Vec<VvcSample>,
    cr_prediction: &'a mut Vec<VvcSample>,
    cb_residuals: &'a mut Vec<i16>,
    cr_residuals: &'a mut Vec<i16>,
    stats: &'a mut VvcIntraSearchStats,
}

impl VvcChromaInterCandidateContext<'_> {
    fn select_candidate(
        &self,
        decision: VvcLumaInterDecision,
        reference: &VvcReconstructionFrame,
        buffers: VvcChromaInterCandidateBuffers<'_>,
    ) -> Option<VvcSelectedChromaTuCandidate> {
        let VvcChromaInterCandidateBuffers {
            cb_prediction,
            cr_prediction,
            cb_residuals,
            cr_residuals,
            stats,
        } = buffers;
        if !VvcReconstructionFrame::predict_chroma_node_from_inter_motion_into(
            reference,
            cb_prediction,
            cr_prediction,
            self.node,
            decision,
        ) {
            return None;
        }
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        let (cb_all_zero, cr_all_zero) = residual_chroma_pair_tu_at_into_and_detect_zero(
            cb_residuals,
            cr_residuals,
            &self.source_frame.cb,
            &self.source_frame.cr,
            self.source_frame.geometry,
            self.source_frame.format,
            self.chroma_x,
            self.chroma_y,
            self.chroma_width,
            self.chroma_height,
            cb_prediction,
            cr_prediction,
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
    prediction_scratch: &'a mut VvcDcPredictionScratch,
    selected_cb_prediction: &'a mut Vec<VvcSample>,
    selected_cr_prediction: &'a mut Vec<VvcSample>,
    selected_cb_residuals: &'a mut Vec<i16>,
    selected_cr_residuals: &'a mut Vec<i16>,
    candidate_cb_prediction: &'a mut Vec<VvcSample>,
    candidate_cr_prediction: &'a mut Vec<VvcSample>,
    candidate_cb_residuals: &'a mut Vec<i16>,
    candidate_cr_residuals: &'a mut Vec<i16>,
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
            selected_cb_prediction,
            selected_cr_prediction,
            selected_cb_residuals,
            selected_cr_residuals,
            candidate_cb_prediction,
            candidate_cr_prediction,
            candidate_cb_residuals,
            candidate_cr_residuals,
            stats,
            transform_scratch,
            reconstructed_residual,
        } = buffers;
        if let Some(hint) = self.temporal_hint {
            if let Some(candidate) = self.select_temporal_hint_candidate(
                hint,
                prediction_scratch,
                selected_cb_prediction,
                selected_cr_prediction,
                selected_cb_residuals,
                selected_cr_residuals,
                stats,
            ) {
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
            selected_cb_prediction,
            selected_cr_prediction,
            candidate_cb_prediction,
            candidate_cr_prediction,
            candidate_cb_residuals,
            candidate_cr_residuals,
            stats,
        });
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_mode_search_nanos(mode_search_start.elapsed().as_nanos() as u64);

        if cache.get(raw_mode).is_some() {
            cache.take_residuals(raw_mode, selected_cb_residuals, selected_cr_residuals);
        } else {
            self.materialize_residuals(
                selected_cb_prediction,
                selected_cr_prediction,
                selected_cb_residuals,
                selected_cr_residuals,
                stats,
            );
        }

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
            select_vvc_chroma_mode_with_rd_refinement(
                self.policy,
                self.node,
                raw_mode,
                candidate_costs,
                cache,
                stats,
                self.co_located_luma_mode,
                self.cclm_enabled,
                self.source_frame,
                self.frame_recon,
                self.chroma_width,
                self.chroma_height,
                self.chroma_qp,
                self.chroma_ts_quant,
                prediction_scratch,
                selected_cb_prediction,
                selected_cr_prediction,
                selected_cb_residuals,
                selected_cr_residuals,
                candidate_cb_prediction,
                candidate_cr_prediction,
                candidate_cb_residuals,
                candidate_cr_residuals,
                transform_scratch,
                reconstructed_residual,
            )
        };
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_rd_refinement_nanos(rd_start.elapsed().as_nanos() as u64);
        #[cfg(feature = "vvc-stats")]
        if selected_mode.residual.is_some() {
            stats.add_chroma_rd_refinement_attempt();
            if selected_mode.mode != raw_mode {
                stats.add_chroma_rd_refinement_switch();
            }
        }

        let mut mode = selected_mode.mode;
        let mut selected_residual = selected_mode.residual;
        #[cfg(feature = "vvc-stats")]
        let bdpcm_start = StageStart::now();
        if let Some(selected_bdpcm) = select_vvc_chroma_bdpcm_prediction(
            self.policy,
            self.node,
            mode,
            self.co_located_luma_mode,
            self.cclm_enabled,
            self.source_frame,
            self.frame_recon,
            self.chroma_width,
            self.chroma_height,
            self.chroma_qp,
            self.chroma_ts_quant,
            selected_residual,
            stats,
            prediction_scratch,
            selected_cb_prediction,
            selected_cr_prediction,
            selected_cb_residuals,
            selected_cr_residuals,
            candidate_cb_prediction,
            candidate_cr_prediction,
            candidate_cb_residuals,
            candidate_cr_residuals,
            transform_scratch,
            reconstructed_residual,
        ) {
            mode = selected_bdpcm.mode;
            selected_residual = Some(selected_bdpcm.residual);
        }
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_bdpcm_nanos(bdpcm_start.elapsed().as_nanos() as u64);

        VvcSelectedChromaTuCandidate {
            mode,
            coding_decision: self.policy.select_chroma_tu_coding_decision(self.node, mode),
            residual: selected_residual,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn select_temporal_hint_candidate(
        &self,
        hint: VvcChromaTemporalModeHint,
        prediction_scratch: &mut VvcDcPredictionScratch,
        predicted_cb: &mut Vec<VvcSample>,
        predicted_cr: &mut Vec<VvcSample>,
        cb_residuals: &mut Vec<i16>,
        cr_residuals: &mut Vec<i16>,
        stats: &mut VvcIntraSearchStats,
    ) -> Option<VvcSelectedChromaTuCandidate> {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = StageStart::now();
        let preselected_residual = if hint.bdpcm_mode.is_enabled() {
            predict_vvc_chroma_bdpcm_block_into_with_availability(
                predicted_cb,
                prediction_scratch,
                hint.bdpcm_mode,
                &self.frame_recon.cb,
                self.frame_recon.coded_geometry(),
                self.node,
                self.source_frame.format.chroma_sampling,
                self.source_frame.format.bit_depth,
                Some(self.frame_recon.cb_availability()),
            );
            predict_vvc_chroma_bdpcm_block_into_with_availability(
                predicted_cr,
                prediction_scratch,
                hint.bdpcm_mode,
                &self.frame_recon.cr,
                self.frame_recon.coded_geometry(),
                self.node,
                self.source_frame.format.chroma_sampling,
                self.source_frame.format.bit_depth,
                Some(self.frame_recon.cr_availability()),
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_prediction_nanos(
                VvcChromaPredictionStatsFamily::Explicit,
                vvc_elapsed_nanos(prediction_start),
            );
            self.materialize_residuals(
                predicted_cb,
                predicted_cr,
                cb_residuals,
                cr_residuals,
                stats,
            );
            if !self.temporal_hint_residuals_are_cheap(cb_residuals, cr_residuals) {
                return None;
            }
            Some(VvcScoredSelectedChromaResidual {
                residual: VvcSelectedChromaResidual {
                    cb: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
                        cb_residuals,
                        self.chroma_width,
                        self.chroma_height,
                        self.chroma_ts_quant,
                        hint.bdpcm_mode,
                    ),
                    cr: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
                        cr_residuals,
                        self.chroma_width,
                        self.chroma_height,
                        self.chroma_ts_quant,
                        hint.bdpcm_mode,
                    ),
                },
                score: VvcResidualBlockScore {
                    distortion: 0,
                    rate_cost: 0,
                },
            })
        } else {
            predict_vvc_chroma_mode_pair_blocks_into_with_availability(
                predicted_cb,
                predicted_cr,
                prediction_scratch,
                hint.mode,
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
            stats.add_chroma_prediction_nanos(
                vvc_chroma_prediction_stats_family(hint.mode),
                vvc_elapsed_nanos(prediction_start),
            );
            self.materialize_residuals(
                predicted_cb,
                predicted_cr,
                cb_residuals,
                cr_residuals,
                stats,
            );
            if !self.temporal_hint_residuals_are_cheap(cb_residuals, cr_residuals) {
                return None;
            }
            None
        };
        #[cfg(not(feature = "vvc-stats"))]
        let _ = stats;
        Some(VvcSelectedChromaTuCandidate {
            mode: hint.mode,
            coding_decision: self
                .policy
                .select_chroma_tu_coding_decision(self.node, hint.mode),
            residual: preselected_residual,
        })
    }

    fn temporal_hint_residuals_are_cheap(
        &self,
        cb_residuals: &[i16],
        cr_residuals: &[i16],
    ) -> bool {
        let sample_count = self.chroma_width * self.chroma_height;
        vvc_temporal_mode_hint_residual_is_cheap(
            cb_residuals,
            sample_count,
            self.source_frame.format.bit_depth,
            self.policy,
        ) && vvc_temporal_mode_hint_residual_is_cheap(
            cr_residuals,
            sample_count,
            self.source_frame.format.bit_depth,
            self.policy,
        )
    }

    fn materialize_residuals(
        &self,
        cb_prediction: &[VvcSample],
        cr_prediction: &[VvcSample],
        cb_residuals: &mut Vec<i16>,
        cr_residuals: &mut Vec<i16>,
        stats: &mut VvcIntraSearchStats,
    ) {
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_chroma_tu_at_into(
            cb_residuals,
            &self.source_frame.cb,
            self.source_frame.geometry,
            self.source_frame.format,
            self.chroma_x,
            self.chroma_y,
            self.chroma_width,
            self.chroma_height,
            cb_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_chroma_tu_at_into(
            cr_residuals,
            &self.source_frame.cr,
            self.source_frame.geometry,
            self.source_frame.format,
            self.chroma_x,
            self.chroma_y,
            self.chroma_width,
            self.chroma_height,
            cr_prediction,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(not(feature = "vvc-stats"))]
        let _ = stats;
    }
}

fn vvc_zero_chroma_preselected_residual() -> VvcScoredSelectedChromaResidual {
    VvcScoredSelectedChromaResidual {
        residual: VvcSelectedChromaResidual {
            cb: VvcFinalizedResidualBlock {
                dc_level: 0,
                ac_levels: [0; VVC_CHROMA_AC_COEFFS_PER_TU],
                has_ac: false,
                transform_skip: true,
                bdpcm_mode: VvcBdpcmMode::None,
            },
            cr: VvcFinalizedResidualBlock {
                dc_level: 0,
                ac_levels: [0; VVC_CHROMA_AC_COEFFS_PER_TU],
                has_ac: false,
                transform_skip: true,
                bdpcm_mode: VvcBdpcmMode::None,
            },
        },
        score: VvcResidualBlockScore {
            distortion: 0,
            rate_cost: 0,
        },
    }
}
