#[derive(Debug, Clone, Copy)]
struct VvcChromaModeRdCandidate {
    distortion: u64,
    rate_cost: u64,
    residual: VvcScoredSelectedChromaResidual,
}

impl VvcChromaModeRdCandidate {
    fn selects_over(self, best: Self) -> bool {
        vvc_rd_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

fn score_vvc_chroma_mode_rd_candidate(
    policy: VvcResidualCodingPolicy,
    coding_decision: VvcChromaTuCodingDecision,
    mode: VvcChromaIntraPredictionMode,
    cclm_syntax_enabled: bool,
    cb_residuals: &[i16],
    cr_residuals: &[i16],
    chroma_width: usize,
    chroma_height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcChromaModeRdCandidate {
    let cb = select_vvc_scored_chroma_residual_block_with_transform_skip(
        policy,
        coding_decision.residual_coding,
        cb_residuals,
        chroma_width,
        chroma_height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    let cr = select_vvc_scored_chroma_residual_block_with_transform_skip(
        policy,
        coding_decision.residual_coding,
        cr_residuals,
        chroma_width,
        chroma_height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    let residual = VvcSelectedChromaResidual {
        cb: cb.residual,
        cr: cr.residual,
    };
    let rd_score = VvcResidualBlockScore {
        distortion: cb.score.distortion.saturating_add(cr.score.distortion),
        rate_cost: cb.score.rate_cost.saturating_add(cr.score.rate_cost),
    };
    let residual = VvcScoredSelectedChromaResidual::from_scored_blocks(
        residual,
        cb.score,
        cr.score,
        chroma_width,
        chroma_height,
    );
    let mode_cost = u64::from(vvc_chroma_intra_mode_syntax_bin_count(
        mode,
        cclm_syntax_enabled,
    ));
    VvcChromaModeRdCandidate {
        distortion: rd_score.distortion,
        rate_cost: rd_score.rate_cost.saturating_add(mode_cost),
        residual,
    }
}

fn vvc_scored_chroma_quantized_residual_score(
    residual: VvcScoredSelectedChromaResidual,
    extra_syntax_cost: u64,
) -> VvcChromaQuantizedResidualScore {
    VvcChromaQuantizedResidualScore {
        distortion: residual.score.distortion,
        rate_cost: residual.score.rate_cost.saturating_add(extra_syntax_cost),
    }
}

fn chroma_reconstructed_residual_sse(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> u64 {
    if residual.transform_skip {
        return chroma_transform_skip_residual_sse(
            source_residuals,
            width,
            height,
            chroma_ts_quant,
            residual,
        );
    }
    if !residual.has_ac {
        return transformed_dc_only_residual_sse(
            source_residuals,
            width as u16,
            height as u16,
            bit_depth,
            chroma_qp,
            residual.dc_level,
        );
    }
    reconstruct_vvc_chroma_residual_block_into(
        residual,
        reconstructed_residual,
        transform_scratch,
        width,
        height,
        bit_depth,
        chroma_qp,
        chroma_ts_quant,
    );
    reconstructed_residual_sse(source_residuals, reconstructed_residual)
}

fn chroma_transform_skip_residual_sse(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
) -> u64 {
    let active_width = width.min(8);
    let active_height = height.min(8);
    vvc_transform_skip_residual_sse(
        source_residuals,
        width,
        height,
        active_width,
        active_height,
        active_width,
        4,
        chroma_ts_quant,
        residual,
    )
}

fn chroma_coeff_syntax_cost_estimate(
    width: usize,
    height: usize,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
) -> u64 {
    let mut nonzero = u64::from(residual.dc_level != 0);
    let mut abs_sum = u64::from(residual.dc_level.unsigned_abs());
    let mut last_pos = 0u64;
    let active_width = width.min(8);
    let active_height = height.min(8);
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            let abs_level = u64::from(residual.ac_levels[y * active_width + x - 1].unsigned_abs());
            if abs_level != 0 {
                nonzero += 1;
                abs_sum += abs_level;
                last_pos = (y * active_width + x) as u64;
            }
        }
    }
    nonzero
        .saturating_mul(18)
        .saturating_add(abs_sum.saturating_mul(4))
        .saturating_add(last_pos.saturating_mul(2))
}

#[derive(Debug, Clone, Copy)]
struct VvcChromaQuantizedResidualScore {
    distortion: u64,
    rate_cost: u64,
}

impl VvcChromaQuantizedResidualScore {
    fn selects_over(self, best: Self) -> bool {
        vvc_rd_candidate_selects_over(
            self.distortion,
            self.rate_cost,
            best.distortion,
            best.rate_cost,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcChromaModeRdShortlist {
    candidates: [VvcChromaIntraCandidateCost; VVC_CHROMA_INTRA_CANDIDATE_CAPACITY],
    count: usize,
}

impl VvcChromaModeRdShortlist {
    fn from_candidate_costs(
        policy: VvcResidualCodingPolicy,
        costs: VvcChromaIntraCandidateCosts,
    ) -> Self {
        let mut shortlist = Self {
            candidates: [VvcChromaIntraCandidateCost::new(
                VvcChromaIntraPredictionMode::Derived,
                u64::MAX,
            ); VVC_CHROMA_INTRA_CANDIDATE_CAPACITY],
            count: 0,
        };
        for candidate in costs.iter() {
            shortlist.add(candidate);
        }
        shortlist.apply_policy_limit(policy);
        shortlist
    }

    fn add(&mut self, candidate: VvcChromaIntraCandidateCost) {
        if let Some(existing) = self
            .candidates
            .iter()
            .take(self.count)
            .position(|entry| entry.mode() == candidate.mode())
        {
            if candidate.score() < self.candidates[existing].score() {
                self.candidates[existing] = candidate;
                self.sort();
            }
            return;
        }
        if self.count < self.candidates.len() {
            self.candidates[self.count] = candidate;
            self.count += 1;
            self.sort();
            return;
        }
        let worst = self.count - 1;
        if candidate.score() < self.candidates[worst].score() {
            self.candidates[worst] = candidate;
            self.sort();
        }
    }

    fn sort(&mut self) {
        self.candidates[..self.count].sort_by_key(|candidate| candidate.score());
    }

    fn apply_policy_limit(&mut self, policy: VvcResidualCodingPolicy) {
        self.count = self
            .count
            .min(vvc_chroma_mode_rd_shortlist_limit(policy).min(self.candidates.len()));
    }

    fn iter(&self) -> impl Iterator<Item = VvcChromaIntraCandidateCost> + '_ {
        self.candidates[..self.count].iter().copied()
    }

    fn admits_lossless_speed_rd(self, candidate: VvcChromaIntraCandidateCost) -> bool {
        self.count <= 1 || candidate.score() <= self.candidates[0].score().saturating_mul(2)
    }
}

fn vvc_chroma_mode_rd_shortlist_limit(policy: VvcResidualCodingPolicy) -> usize {
    match policy.residual_mode() {
        VvcResidualCodingMode::Lossless => VVC_CHROMA_INTRA_CANDIDATE_CAPACITY,
        VvcResidualCodingMode::Lossy => match policy.fast_search() {
            VvcFastSearch::Off | VvcFastSearch::Conservative => {
                VVC_LOSSY_CHROMA_RD_WINNER_CANDIDATES
            }
            VvcFastSearch::LosslessSpeed => 3,
            VvcFastSearch::Moderate => 3,
            VvcFastSearch::Aggressive => 2,
        },
    }
}
