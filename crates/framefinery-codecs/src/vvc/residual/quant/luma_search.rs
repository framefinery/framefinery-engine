struct VvcLumaModeSearchContext<'a> {
    metric: VvcResidualScoreMetric,
    source_frame: &'a VvcSampledFrame,
    frame_recon: &'a VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
}

impl VvcLumaModeSearchContext<'_> {
    #[allow(clippy::too_many_arguments)]
    fn predict_and_score_directional_candidate(
        &self,
        cache: &mut VvcLumaModeRdCache,
        mode: VvcIntraPredictionMode,
        prediction_scratch: &mut VvcDcPredictionScratch,
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
            VvcLumaPredictionStatsFamily::Directional,
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
