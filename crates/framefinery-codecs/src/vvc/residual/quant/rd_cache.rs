struct VvcLumaModeRdCache {
    candidates: Vec<VvcCachedLumaModeRdCandidate>,
    count: usize,
    limit: usize,
}

impl VvcLumaModeRdCache {
    fn new() -> Self {
        let mut candidates = Vec::with_capacity(VVC_LOSSY_LUMA_RD_WINNER_CANDIDATES);
        candidates.resize_with(
            VVC_LOSSY_LUMA_RD_WINNER_CANDIDATES,
            VvcCachedLumaModeRdCandidate::new,
        );
        Self {
            candidates,
            count: 0,
            limit: 0,
        }
    }

    fn reset(&mut self, policy: VvcResidualCodingPolicy, node: VvcCodingTreeNode) {
        self.count = 0;
        self.limit = if policy.residual_mode() == VvcResidualCodingMode::Lossy
            && [4, 8, 16, 32].contains(&node.width)
            && [4, 8, 16, 32].contains(&node.height)
        {
            vvc_luma_mode_rd_shortlist_limit(policy).min(self.candidates.len())
        } else {
            0
        };
    }

    fn materializes_mode_search_residuals(&self) -> bool {
        self.limit > 0
    }

    fn consider(&mut self, mode: VvcIntraPredictionMode, score: u64, residuals: &[i16]) {
        if self.limit == 0 {
            return;
        }
        if let Some(existing) = self.candidates[..self.count]
            .iter()
            .position(|candidate| candidate.mode.luma_mode_index() == mode.luma_mode_index())
        {
            if score < self.candidates[existing].score {
                self.candidates[existing].replace(mode, score, residuals);
            }
            return;
        }
        if self.count < self.limit {
            self.candidates[self.count].replace(mode, score, residuals);
            self.count += 1;
            return;
        }
        let worst = self.worst_index();
        if score < self.candidates[worst].score {
            self.candidates[worst].replace(mode, score, residuals);
        }
    }

    fn worst_index(&self) -> usize {
        let mut worst = 0;
        for index in 1..self.count {
            // Keep the last equal-cost entry as the replacement target. This
            // matches the stable sort's former tail selection exactly.
            if self.candidates[index].score >= self.candidates[worst].score {
                worst = index;
            }
        }
        worst
    }

    fn get(&self, mode: VvcIntraPredictionMode) -> Option<&VvcCachedLumaModeRdCandidate> {
        self.candidates[..self.count]
            .iter()
            .find(|candidate| candidate.mode.luma_mode_index() == mode.luma_mode_index())
    }

}

struct VvcCachedLumaModeRdCandidate {
    mode: VvcIntraPredictionMode,
    score: u64,
    residuals: Vec<i16>,
}

impl VvcCachedLumaModeRdCandidate {
    fn new() -> Self {
        Self {
            mode: VvcIntraPredictionMode::Dc,
            score: u64::MAX,
            residuals: Vec::new(),
        }
    }

    fn replace(&mut self, mode: VvcIntraPredictionMode, score: u64, residuals: &[i16]) {
        self.mode = mode;
        self.score = score;
        self.residuals.clear();
        self.residuals.extend_from_slice(residuals);
    }
}

struct VvcChromaModeRdCache {
    candidates: Vec<VvcCachedChromaModeRdCandidate>,
    count: usize,
    limit: usize,
}

impl VvcChromaModeRdCache {
    fn new() -> Self {
        let mut candidates = Vec::with_capacity(VVC_LOSSY_CHROMA_RD_WINNER_CANDIDATES);
        candidates.resize_with(
            VVC_LOSSY_CHROMA_RD_WINNER_CANDIDATES,
            VvcCachedChromaModeRdCandidate::new,
        );
        Self {
            candidates,
            count: 0,
            limit: 0,
        }
    }

    fn reset(&mut self, policy: VvcResidualCodingPolicy, node: VvcCodingTreeNode) {
        self.count = 0;
        self.limit = if policy.residual_mode() == VvcResidualCodingMode::Lossy
            && [4, 8, 16, 32].contains(&node.width)
            && [4, 8, 16, 32].contains(&node.height)
        {
            vvc_chroma_mode_rd_shortlist_limit(policy).min(self.candidates.len())
        } else {
            0
        };
    }

    fn materializes_mode_search_residuals(&self) -> bool {
        self.limit > 0
    }

    fn consider(
        &mut self,
        mode: VvcChromaIntraPredictionMode,
        score: u64,
        cb_residuals: &[i16],
        cr_residuals: &[i16],
    ) {
        if self.limit == 0 {
            return;
        }
        if let Some(existing) = self.candidates[..self.count]
            .iter()
            .position(|candidate| candidate.mode == mode)
        {
            if score < self.candidates[existing].score {
                self.candidates[existing].replace(mode, score, cb_residuals, cr_residuals);
            }
            return;
        }
        if self.count < self.limit {
            self.candidates[self.count].replace(mode, score, cb_residuals, cr_residuals);
            self.count += 1;
            return;
        }
        let worst = self.worst_index();
        if score < self.candidates[worst].score {
            self.candidates[worst].replace(mode, score, cb_residuals, cr_residuals);
        }
    }

    fn worst_index(&self) -> usize {
        let mut worst = 0;
        for index in 1..self.count {
            if self.candidates[index].score >= self.candidates[worst].score {
                worst = index;
            }
        }
        worst
    }

    fn get(&self, mode: VvcChromaIntraPredictionMode) -> Option<&VvcCachedChromaModeRdCandidate> {
        self.candidates[..self.count]
            .iter()
            .find(|candidate| candidate.mode == mode)
    }

}

struct VvcCachedChromaModeRdCandidate {
    mode: VvcChromaIntraPredictionMode,
    score: u64,
    cb_residuals: Vec<i16>,
    cr_residuals: Vec<i16>,
}

impl VvcCachedChromaModeRdCandidate {
    fn new() -> Self {
        Self {
            mode: VvcChromaIntraPredictionMode::Derived,
            score: u64::MAX,
            cb_residuals: Vec::new(),
            cr_residuals: Vec::new(),
        }
    }

    fn replace(
        &mut self,
        mode: VvcChromaIntraPredictionMode,
        score: u64,
        cb_residuals: &[i16],
        cr_residuals: &[i16],
    ) {
        self.mode = mode;
        self.score = score;
        self.cb_residuals.clear();
        self.cb_residuals.extend_from_slice(cb_residuals);
        self.cr_residuals.clear();
        self.cr_residuals.extend_from_slice(cr_residuals);
    }
}
