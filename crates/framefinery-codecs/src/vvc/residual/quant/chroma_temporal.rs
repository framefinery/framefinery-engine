impl VvcChromaTuSelectionContext<'_> {
    fn select_temporal_hint_candidate(
        &self,
        hint: VvcChromaTemporalModeHint,
        prediction_scratch: &mut VvcDcPredictionScratch,
        buffers: &mut VvcChromaCandidateBuffers<'_>,
        stats: &mut VvcIntraSearchStats,
    ) -> Option<VvcSelectedChromaTuCandidate> {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = StageStart::now();
        let preselected_residual = if hint.bdpcm_mode.is_enabled() {
            predict_vvc_chroma_bdpcm_block_into_with_availability(
                buffers.prediction.cb,
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
                buffers.prediction.cr,
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
                buffers.prediction.cb,
                buffers.prediction.cr,
                buffers.residuals.cb,
                buffers.residuals.cr,
                stats,
            );
            if !self.temporal_hint_residuals_are_cheap(buffers.residuals.cb, buffers.residuals.cr) {
                return None;
            }
            Some(VvcScoredSelectedChromaResidual {
                residual: VvcSelectedChromaResidual {
                    cb: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
                        buffers.residuals.cb,
                        self.chroma_width,
                        self.chroma_height,
                        self.chroma_ts_quant,
                        hint.bdpcm_mode,
                    ),
                    cr: finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
                        buffers.residuals.cr,
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
                buffers.prediction.cb,
                buffers.prediction.cr,
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
                buffers.prediction.cb,
                buffers.prediction.cr,
                buffers.residuals.cb,
                buffers.residuals.cr,
                stats,
            );
            if !self.temporal_hint_residuals_are_cheap(buffers.residuals.cb, buffers.residuals.cr) {
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
}
