#[derive(Debug, Clone, Copy)]
struct VvcSelectedLumaTuCandidate {
    mode: VvcIntraPredictionMode,
    coding_decision: VvcLumaTuCodingDecision,
    residual: Option<VvcScoredSelectedLumaResidual>,
    inter_decision: Option<VvcLumaInterDecision>,
}

struct VvcLumaTuSelectionContext<'a> {
    policy: VvcResidualCodingPolicy,
    metric: VvcResidualScoreMetric,
    source_frame: &'a VvcSampledFrame,
    frame_recon: &'a VvcReconstructionFrame,
    mode_search_state: &'a VvcLumaModeSearchState,
    node: VvcCodingTreeNode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    luma_qp: i32,
    luma_ts_quant: &'a VvcTransformSkipQuantTable,
    temporal_hint: Option<VvcLumaTemporalModeHint>,
    inter_decision: Option<VvcLumaInterDecision>,
    inter_reference: Option<&'a VvcReconstructionFrame>,
}

struct VvcLumaTuSelectionBuffers<'a> {
    cache: &'a mut VvcLumaModeRdCache,
    prediction_scratch: &'a mut VvcDcPredictionScratch,
    selected_prediction: &'a mut Vec<VvcSample>,
    selected_residuals: &'a mut Vec<i16>,
    candidate_prediction: &'a mut Vec<VvcSample>,
    candidate_residuals: &'a mut Vec<i16>,
    stats: &'a mut VvcIntraSearchStats,
    transform_scratch: &'a mut VvcInverseTransformScratch,
    reconstructed_residual: &'a mut Vec<i16>,
}

impl VvcLumaTuSelectionContext<'_> {
    fn select_candidate(
        &self,
        buffers: VvcLumaTuSelectionBuffers<'_>,
    ) -> VvcSelectedLumaTuCandidate {
        let VvcLumaTuSelectionBuffers {
            cache,
            prediction_scratch,
            selected_prediction,
            selected_residuals,
            candidate_prediction,
            candidate_residuals,
            stats,
            transform_scratch,
            reconstructed_residual,
        } = buffers;
        if let Some(hint) = self.temporal_hint {
            if let Some(candidate) = self.select_temporal_hint_candidate(
                hint,
                prediction_scratch,
                selected_prediction,
                selected_residuals,
                stats,
            ) {
                return candidate;
            }
        }
        if let (Some(decision), Some(reference)) = (self.inter_decision, self.inter_reference) {
            if let Some(candidate) = self.select_exact_inter_candidate(
                decision,
                reference,
                selected_prediction,
                selected_residuals,
            ) {
                return candidate;
            }
        }

        #[cfg(feature = "vvc-stats")]
        let mode_search_start = StageStart::now();
        let VvcLumaModeSearchResult {
            mode: raw_mode,
            candidate_costs,
        } = VvcLumaModeSearchContext {
            policy: self.policy,
            metric: self.metric,
            source_frame: self.source_frame,
            frame_recon: self.frame_recon,
            mode_search_state: self.mode_search_state,
            node: self.node,
            left: self.left,
            above: self.above,
        }
        .select_intra_mode(VvcLumaModeSearchBuffers {
            cache,
            prediction_scratch,
            selected_prediction,
            candidate_prediction,
            candidate_residuals,
            stats,
        });
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_mode_search_nanos(mode_search_start.elapsed().as_nanos() as u64);

        if cache.get(raw_mode).is_some() {
            cache.take_residuals(raw_mode, selected_residuals);
        } else {
            #[cfg(feature = "vvc-stats")]
            let residual_start = StageStart::now();
            residual_luma_tu_at_into(
                selected_residuals,
                self.source_frame,
                usize::from(self.node.x),
                usize::from(self.node.y),
                usize::from(self.node.width),
                usize::from(self.node.height),
                selected_prediction,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        }

        #[cfg(feature = "vvc-stats")]
        let rd_start = StageStart::now();
        let selected_mode = select_vvc_luma_mode_with_rd_refinement(
            self.policy,
            self.node,
            raw_mode,
            candidate_costs,
            cache,
            stats,
            self.left,
            self.above,
            self.source_frame,
            self.frame_recon,
            self.luma_qp,
            self.luma_ts_quant,
            prediction_scratch,
            selected_prediction,
            selected_residuals,
            candidate_prediction,
            candidate_residuals,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_rd_refinement_nanos(rd_start.elapsed().as_nanos() as u64);
        #[cfg(feature = "vvc-stats")]
        if selected_mode.residual.is_some() {
            stats.add_luma_rd_refinement_attempt();
            if selected_mode.mode != raw_mode {
                stats.add_luma_rd_refinement_switch();
            }
        }

        let mut mode = selected_mode.mode;
        let mut coding_decision = self.policy.select_luma_tu_coding_decision(self.node, mode);
        #[cfg(feature = "vvc-stats")]
        let mrl_start = StageStart::now();
        let selected_mrl = select_vvc_luma_mrl_prediction(
            self.policy,
            coding_decision.residual_coding,
            coding_decision.mts_index,
            self.node,
            mode,
            self.left,
            self.above,
            self.luma_qp,
            self.luma_ts_quant,
            selected_mode.residual,
            stats,
            self.frame_recon,
            self.source_frame,
            prediction_scratch,
            selected_prediction,
            selected_residuals,
            candidate_prediction,
            candidate_residuals,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_mrl_nanos(mrl_start.elapsed().as_nanos() as u64);
        coding_decision.mrl_index = selected_mrl.mrl_index;
        let mut selected_residual = selected_mrl.residual;

        #[cfg(feature = "vvc-stats")]
        let bdpcm_start = StageStart::now();
        if let Some(selected_bdpcm) = select_vvc_luma_bdpcm_prediction(
            self.policy,
            self.node,
            mode,
            coding_decision,
            self.left,
            self.above,
            self.luma_qp,
            self.luma_ts_quant,
            selected_residual,
            stats,
            self.frame_recon,
            self.source_frame,
            prediction_scratch,
            selected_prediction,
            selected_residuals,
            candidate_prediction,
            candidate_residuals,
            transform_scratch,
            reconstructed_residual,
        ) {
            mode = selected_bdpcm.mode;
            coding_decision = selected_bdpcm.coding_decision;
            selected_residual = Some(selected_bdpcm.residual);
        }
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_bdpcm_nanos(bdpcm_start.elapsed().as_nanos() as u64);

        let mut selected_inter_decision = None;
        if let (Some(decision), Some(reference)) = (self.inter_decision, self.inter_reference) {
            if let Some(inter_residual) = select_vvc_luma_explicit_inter_candidate(
                decision,
                mode,
                coding_decision,
                selected_residual,
                self.left,
                self.above,
                self.policy,
                self.source_frame,
                reference,
                self.node,
                self.luma_qp,
                self.luma_ts_quant,
                candidate_prediction,
                candidate_residuals,
                stats,
                transform_scratch,
                reconstructed_residual,
            ) {
                mode = VvcIntraPredictionMode::Dc;
                coding_decision = self.policy.select_luma_tu_coding_decision(self.node, mode);
                selected_residual = Some(inter_residual);
                selected_inter_decision = Some(decision);
                std::mem::swap(selected_prediction, candidate_prediction);
                std::mem::swap(selected_residuals, candidate_residuals);
            }
        }

        VvcSelectedLumaTuCandidate {
            mode,
            coding_decision,
            residual: selected_residual,
            inter_decision: selected_inter_decision,
        }
    }

    fn select_temporal_hint_candidate(
        &self,
        hint: VvcLumaTemporalModeHint,
        prediction_scratch: &mut VvcDcPredictionScratch,
        predicted_luma: &mut Vec<VvcSample>,
        luma_residuals: &mut Vec<i16>,
        stats: &mut VvcIntraSearchStats,
    ) -> Option<VvcSelectedLumaTuCandidate> {
        let coding_decision = if hint.bdpcm_mode.is_enabled() {
            VvcLumaTuCodingDecision {
                residual_coding: VvcTuResidualCodingMode::TransformSkip,
                mrl_index: 0,
                mts_index: 0,
            }
        } else {
            self.policy
                .select_luma_tu_coding_decision(self.node, hint.mode)
        };
        let preselected_residual = if hint.bdpcm_mode.is_enabled() {
            #[cfg(feature = "vvc-stats")]
            let prediction_start = StageStart::now();
            predict_vvc_luma_bdpcm_block_into_with_availability(
                predicted_luma,
                prediction_scratch,
                hint.bdpcm_mode,
                &self.frame_recon.luma,
                self.frame_recon.coded_geometry(),
                self.node,
                self.source_frame.format.bit_depth,
                Some(self.frame_recon.luma_availability()),
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_prediction_nanos(
                VvcLumaPredictionStatsFamily::Bdpcm,
                vvc_elapsed_nanos(prediction_start),
            );
            self.materialize_temporal_hint_residual(predicted_luma, luma_residuals, stats);
            if !self.temporal_hint_residual_is_cheap(luma_residuals) {
                return None;
            }
            Some(VvcScoredSelectedLumaResidual {
                residual: VvcSelectedLumaResidual {
                    block: finalize_vvc_luma_bdpcm_transform_skip_residual_block(
                        luma_residuals,
                        self.node.width,
                        self.node.height,
                        self.luma_ts_quant,
                        hint.bdpcm_mode,
                    ),
                    mts_index: 0,
                },
                score: VvcResidualBlockScore {
                    distortion: 0,
                    rate_cost: 0,
                },
            })
        } else {
            #[cfg(feature = "vvc-stats")]
            let prediction_start = StageStart::now();
            if coding_decision.mrl_index == 0 {
                predict_vvc_luma_intra_block_into_with_availability(
                    predicted_luma,
                    prediction_scratch,
                    hint.mode,
                    &self.frame_recon.luma,
                    self.frame_recon.coded_geometry(),
                    self.node,
                    self.source_frame.format.bit_depth,
                    Some(self.frame_recon.luma_availability()),
                );
            } else {
                predict_vvc_luma_intra_block_into_with_mrl_and_availability(
                    predicted_luma,
                    prediction_scratch,
                    hint.mode,
                    &self.frame_recon.luma,
                    self.frame_recon.coded_geometry(),
                    self.node,
                    self.source_frame.format.bit_depth,
                    coding_decision.mrl_index,
                    Some(self.frame_recon.luma_availability()),
                );
            }
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_prediction_nanos(
                vvc_luma_prediction_stats_family(hint.mode),
                vvc_elapsed_nanos(prediction_start),
            );
            self.materialize_temporal_hint_residual(predicted_luma, luma_residuals, stats);
            if !self.temporal_hint_residual_is_cheap(luma_residuals) {
                return None;
            }
            None
        };
        #[cfg(not(feature = "vvc-stats"))]
        let _ = stats;
        Some(VvcSelectedLumaTuCandidate {
            mode: hint.mode,
            coding_decision,
            residual: preselected_residual,
            inter_decision: None,
        })
    }

    fn materialize_temporal_hint_residual(
        &self,
        predicted_luma: &[VvcSample],
        luma_residuals: &mut Vec<i16>,
        stats: &mut VvcIntraSearchStats,
    ) {
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_luma_tu_at_into(
            luma_residuals,
            self.source_frame,
            usize::from(self.node.x),
            usize::from(self.node.y),
            usize::from(self.node.width),
            usize::from(self.node.height),
            predicted_luma,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        #[cfg(not(feature = "vvc-stats"))]
        let _ = stats;
    }

    fn temporal_hint_residual_is_cheap(&self, residuals: &[i16]) -> bool {
        vvc_temporal_mode_hint_residual_is_cheap(
            residuals,
            usize::from(self.node.width) * usize::from(self.node.height),
            self.source_frame.format.bit_depth,
            self.policy,
        )
    }

    fn select_exact_inter_candidate(
        &self,
        decision: VvcLumaInterDecision,
        reference: &VvcReconstructionFrame,
        inter_prediction: &mut Vec<VvcSample>,
        inter_residuals: &mut Vec<i16>,
    ) -> Option<VvcSelectedLumaTuCandidate> {
        if self.source_frame.format.chroma_sampling != ChromaSampling::Cs444
            || (decision.mv_x == 0 && decision.mv_y == 0)
            || !VvcReconstructionFrame::predict_luma_node_from_inter_motion_into(
                reference,
                inter_prediction,
                self.node,
                decision,
            )
            || !vvc_luma_prediction_matches_source(
                self.source_frame,
                self.node,
                inter_prediction,
            )
        {
            return None;
        }
        inter_residuals.clear();
        inter_residuals.resize(
            usize::from(self.node.width) * usize::from(self.node.height),
            0,
        );
        Some(VvcSelectedLumaTuCandidate {
            mode: VvcIntraPredictionMode::Dc,
            coding_decision: self
                .policy
                .select_luma_tu_coding_decision(self.node, VvcIntraPredictionMode::Dc),
            residual: Some(vvc_zero_luma_preselected_residual()),
            inter_decision: Some(decision),
        })
    }
}

fn vvc_zero_luma_preselected_residual() -> VvcScoredSelectedLumaResidual {
    VvcScoredSelectedLumaResidual {
        residual: VvcSelectedLumaResidual {
            block: VvcFinalizedResidualBlock {
                dc_level: 0,
                ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
                has_ac: false,
                transform_skip: true,
                bdpcm_mode: VvcBdpcmMode::None,
            },
            mts_index: 0,
        },
        score: VvcResidualBlockScore {
            distortion: 0,
            rate_cost: 0,
        },
    }
}
