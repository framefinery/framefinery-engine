struct VvcLumaModeSearchContext<'a> {
    policy: VvcResidualCodingPolicy,
    metric: VvcResidualScoreMetric,
    source_frame: &'a VvcSampledFrame,
    frame_recon: &'a VvcReconstructionFrame,
    mode_search_state: &'a VvcLumaModeSearchState,
    node: VvcCodingTreeNode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
}

struct VvcLumaModeSearchBuffers<'a> {
    cache: &'a mut VvcLumaModeRdCache,
    prediction_scratch: &'a mut VvcIntraPredictionScratch,
    selected_prediction: &'a mut Vec<VvcSample>,
    candidate_prediction: &'a mut Vec<VvcSample>,
    candidate_residuals: &'a mut Vec<i16>,
    stats: &'a mut VvcIntraSearchStats,
}

#[derive(Debug, Clone, Copy)]
struct VvcLumaModeSearchResult {
    mode: VvcIntraPredictionMode,
    candidate_costs: VvcLumaIntraCandidateCosts,
}

impl VvcLumaModeSearchContext<'_> {
    fn select_intra_mode(&self, buffers: VvcLumaModeSearchBuffers<'_>) -> VvcLumaModeSearchResult {
        let VvcLumaModeSearchBuffers {
            cache,
            prediction_scratch,
            selected_prediction,
            candidate_prediction,
            candidate_residuals,
            stats,
        } = buffers;
        let initial_score = if !vvc_luma_lossless_speed_skips_dc(self.policy) {
            let score = self.predict_and_score_candidate(
                cache,
                VvcIntraPredictionMode::Dc,
                prediction_scratch,
                selected_prediction,
                candidate_residuals,
                stats,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_dc();
            score
        } else {
            u64::MAX
        };
        let mut search = VvcLumaIntraSearch::new(initial_score);
        if self.policy.luma_planar_candidate_allowed(self.node)
            && vvc_luma_lossless_speed_evaluates_planar(self.policy, self.left, self.above)
        {
            let score = self.predict_and_score_candidate(
                cache,
                VvcIntraPredictionMode::Planar,
                prediction_scratch,
                candidate_prediction,
                candidate_residuals,
                stats,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_luma_planar();
            search.consider_candidate(
                VvcIntraPredictionMode::Planar,
                score,
                selected_prediction,
                candidate_prediction,
            );
        }
        if self.policy.luma_directional_candidate_allowed(self.node)
            && !vvc_luma_exact_min_syntax_mode_search_done(search.best_score())
        {
            let mut directional_candidates = vvc_luma_directional_search_candidates(
                self.policy,
                self.source_frame,
                self.mode_search_state,
                self.node,
            );
            for mode in directional_candidates.iter() {
                let score = self.predict_and_score_candidate(
                    cache,
                    mode,
                    prediction_scratch,
                    candidate_prediction,
                    candidate_residuals,
                    stats,
                );
                #[cfg(feature = "vvc-stats")]
                stats.add_luma_directional_coarse();
                search.consider_candidate(mode, score, selected_prediction, candidate_prediction);
                if vvc_luma_exact_min_syntax_mode_search_done(search.best_score()) {
                    break;
                }
            }
            if (2..=66).contains(&search.best_mode().luma_mode_index())
                && !vvc_luma_exact_min_syntax_mode_search_done(search.best_score())
                && !vvc_luma_lossless_speed_skips_directional_refinement(self.policy)
            {
                let refinement_start = directional_candidates.count();
                let refinement_fast_search = if self.policy.residual_mode()
                    == VvcResidualCodingMode::Lossy
                    && self.policy.fast_search() == VvcFastSearch::LosslessSpeed
                {
                    VvcFastSearch::Off
                } else {
                    self.policy.fast_search()
                };
                directional_candidates
                    .add_refinement(search.best_mode().luma_mode_index(), refinement_fast_search);
                for mode in directional_candidates.iter_from(refinement_start) {
                    let score = self.predict_and_score_candidate(
                        cache,
                        mode,
                        prediction_scratch,
                        candidate_prediction,
                        candidate_residuals,
                        stats,
                    );
                    #[cfg(feature = "vvc-stats")]
                    stats.add_luma_directional_refinement();
                    search.consider_candidate(
                        mode,
                        score,
                        selected_prediction,
                        candidate_prediction,
                    );
                    if vvc_luma_exact_min_syntax_mode_search_done(search.best_score()) {
                        break;
                    }
                }
            }
        }
        let candidate_costs = search.candidate_costs();
        let mode = self
            .policy
            .select_luma_intra_mode(self.node, candidate_costs);
        debug_assert_eq!(mode, search.best_mode());
        VvcLumaModeSearchResult {
            mode,
            candidate_costs,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn predict_and_score_candidate(
        &self,
        cache: &mut VvcLumaModeRdCache,
        mode: VvcIntraPredictionMode,
        prediction_scratch: &mut VvcIntraPredictionScratch,
        predicted: &mut Vec<VvcSample>,
        residuals: &mut Vec<i16>,
        stats: &mut VvcIntraSearchStats,
    ) -> u64 {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = StageStart::now();
        predict_vvc_luma_intra_block_into_with_availability(
            predicted,
            prediction_scratch,
            mode,
            &self.frame_recon.luma,
            self.frame_recon.coded_geometry(),
            self.node,
            self.source_frame.format.bit_depth,
            Some(self.frame_recon.luma_availability()),
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_prediction_nanos(
            vvc_luma_prediction_stats_family(mode),
            vvc_elapsed_nanos(prediction_start),
        );
        #[cfg(feature = "vvc-stats")]
        let score_start = StageStart::now();
        let score = score_luma_mode_candidate(
            cache,
            self.metric,
            mode,
            self.source_frame,
            self.node,
            predicted,
            self.left,
            self.above,
            residuals,
            stats,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_mode_score_nanos(vvc_elapsed_nanos(score_start));
        score
    }
}

struct VvcLumaIntraSearch {
    best_mode: VvcIntraPredictionMode,
    best_score: u64,
    candidate_costs: VvcLumaIntraCandidateCosts,
}

impl VvcLumaIntraSearch {
    fn new(dc_score: u64) -> Self {
        Self {
            best_mode: VvcIntraPredictionMode::Dc,
            best_score: dc_score,
            candidate_costs: VvcLumaIntraCandidateCosts::new(dc_score),
        }
    }

    fn best_mode(&self) -> VvcIntraPredictionMode {
        self.best_mode
    }

    fn best_score(&self) -> u64 {
        self.best_score
    }

    fn candidate_costs(&self) -> VvcLumaIntraCandidateCosts {
        self.candidate_costs
    }

    fn consider_candidate(
        &mut self,
        mode: VvcIntraPredictionMode,
        score: u64,
        selected_prediction: &mut Vec<VvcSample>,
        candidate_prediction: &mut Vec<VvcSample>,
    ) {
        self.candidate_costs = self.candidate_costs.with_candidate(mode, Some(score));
        if score < self.best_score {
            self.best_mode = mode;
            self.best_score = score;
            std::mem::swap(selected_prediction, candidate_prediction);
        }
    }
}

fn vvc_luma_lossless_speed_skips_directional_refinement(policy: VvcResidualCodingPolicy) -> bool {
    policy.fast_search() == VvcFastSearch::LosslessSpeed
}

fn vvc_luma_lossless_speed_skips_dc(policy: VvcResidualCodingPolicy) -> bool {
    // DC is cheap enough to keep for lossy fast search and improves the RD
    // point on flat or near-flat TUs. Lossless-speed lossless still skips it
    // because transform-skip/BDPCM candidates carry exact reconstruction.
    policy.fast_search() == VvcFastSearch::LosslessSpeed
        && policy.residual_mode() == VvcResidualCodingMode::Lossless
}

fn vvc_luma_lossless_speed_evaluates_planar(
    policy: VvcResidualCodingPolicy,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> bool {
    if policy.fast_search() != VvcFastSearch::LosslessSpeed {
        return true;
    }
    matches!(left, None | Some(VvcIntraPredictionMode::Planar))
        || matches!(above, None | Some(VvcIntraPredictionMode::Planar))
}
