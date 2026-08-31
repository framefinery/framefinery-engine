struct VvcChromaModeSearchContext<'a> {
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
}

impl VvcChromaModeSearchContext<'_> {
    #[allow(clippy::too_many_arguments)]
    fn predict_and_score_candidate(
        &self,
        cache: &mut VvcChromaModeRdCache,
        mode: VvcChromaIntraPredictionMode,
        prediction_scratch: &mut VvcDcPredictionScratch,
        predicted_cb: &mut Vec<VvcSample>,
        predicted_cr: &mut Vec<VvcSample>,
        cb_residuals: &mut Vec<i16>,
        cr_residuals: &mut Vec<i16>,
        stats: &mut VvcIntraSearchStats,
    ) -> u64 {
        #[cfg(feature = "vvc-stats")]
        let prediction_start = StageStart::now();
        predict_vvc_chroma_mode_pair_blocks_into_with_availability(
            predicted_cb,
            predicted_cr,
            prediction_scratch,
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
        stats.add_chroma_prediction_nanos(
            vvc_chroma_prediction_stats_family(mode),
            vvc_elapsed_nanos(prediction_start),
        );
        self.score_prediction(
            cache,
            mode,
            predicted_cb,
            predicted_cr,
            cb_residuals,
            cr_residuals,
            stats,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn score_prediction(
        &self,
        cache: &mut VvcChromaModeRdCache,
        mode: VvcChromaIntraPredictionMode,
        predicted_cb: &[VvcSample],
        predicted_cr: &[VvcSample],
        cb_residuals: &mut Vec<i16>,
        cr_residuals: &mut Vec<i16>,
        stats: &mut VvcIntraSearchStats,
    ) -> u64 {
        #[cfg(feature = "vvc-stats")]
        let score_start = StageStart::now();
        let score = score_chroma_mode_candidate(
            cache,
            self.metric,
            mode,
            self.source_frame,
            self.chroma_x,
            self.chroma_y,
            self.chroma_width,
            self.chroma_height,
            predicted_cb,
            predicted_cr,
            self.cclm_enabled,
            self.syntax_tie_breaker_enabled,
            cb_residuals,
            cr_residuals,
            stats,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_mode_score_nanos(vvc_elapsed_nanos(score_start));
        score
    }
}

struct VvcChromaIntraSearch {
    best_mode: VvcChromaIntraPredictionMode,
    best_score: u64,
    candidate_costs: VvcChromaIntraCandidateCosts,
}

impl VvcChromaIntraSearch {
    fn new(derived_score: u64) -> Self {
        Self {
            best_mode: VvcChromaIntraPredictionMode::Derived,
            best_score: derived_score,
            candidate_costs: VvcChromaIntraCandidateCosts::new(derived_score),
        }
    }

    fn best_mode(&self) -> VvcChromaIntraPredictionMode {
        self.best_mode
    }

    fn best_score(&self) -> u64 {
        self.best_score
    }

    fn candidate_costs(&self) -> VvcChromaIntraCandidateCosts {
        self.candidate_costs
    }

    #[allow(clippy::too_many_arguments)]
    fn consider_candidate(
        &mut self,
        mode: VvcChromaIntraPredictionMode,
        score: u64,
        selected_cb_prediction: &mut Vec<VvcSample>,
        selected_cr_prediction: &mut Vec<VvcSample>,
        candidate_cb_prediction: &mut Vec<VvcSample>,
        candidate_cr_prediction: &mut Vec<VvcSample>,
    ) {
        self.candidate_costs = self.candidate_costs.with_candidate(mode, Some(score));
        if score < self.best_score {
            self.best_mode = mode;
            self.best_score = score;
            std::mem::swap(selected_cb_prediction, candidate_cb_prediction);
            std::mem::swap(selected_cr_prediction, candidate_cr_prediction);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn score_chroma_mode_candidate(
    cache: &mut VvcChromaModeRdCache,
    metric: VvcResidualScoreMetric,
    mode: VvcChromaIntraPredictionMode,
    source_frame: &VvcSampledFrame,
    chroma_x: usize,
    chroma_y: usize,
    chroma_width: usize,
    chroma_height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
    cclm_enabled: bool,
    syntax_tie_breaker_enabled: bool,
    cb_residuals: &mut Vec<i16>,
    cr_residuals: &mut Vec<i16>,
    stats: &mut VvcIntraSearchStats,
) -> u64 {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    if cache.materializes_mode_search_residuals() {
        #[cfg(feature = "vvc-stats")]
        let residual_start = StageStart::now();
        residual_chroma_pair_tu_at_into(
            cb_residuals,
            cr_residuals,
            &source_frame.cb,
            &source_frame.cr,
            source_frame.geometry,
            source_frame.format,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            predicted_cb,
            predicted_cr,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        let score = chroma_residual_mode_selection_score(
            metric,
            cb_residuals,
            cr_residuals,
            mode,
            cclm_enabled,
            syntax_tie_breaker_enabled,
        );
        cache.consider(mode, score, cb_residuals, cr_residuals);
        score
    } else {
        chroma_prediction_mode_selection_score(
            metric,
            source_frame,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            predicted_cb,
            predicted_cr,
            mode,
            cclm_enabled,
            syntax_tie_breaker_enabled,
        )
    }
}
