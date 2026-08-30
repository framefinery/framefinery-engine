fn choose_regular_q_lossy_txb(
    candidates: Av2LossyRegularDctCandidates,
    kind: Av2CoefficientProxyKind,
    quant_step: i32,
) -> Av2LossyQuantizedResidualCandidate {
    let transform_candidate = candidates.transform;
    let transform_rate = coefficient_proxy_score(&transform_candidate.coefficients, kind);
    let transform_score = lossy_txb_score(
        transform_rate,
        transform_candidate.sse,
        transform_candidate.variance_loss,
        regular_q_rd_quant_step(quant_step),
    );
    let mut best_candidate = transform_candidate;
    let mut best_score = transform_score;
    let max_extra_sse = regular_q_dc_only_max_extra_sse(quant_step);
    let max_tail_extra_sse = regular_q_tail_pruned_max_extra_sse(quant_step);
    for (candidate, max_extra_sse) in [
        (candidates.tail_pruned, max_tail_extra_sse),
        (candidates.double_tail_pruned, max_tail_extra_sse),
    ] {
        if let Some(tail_pruned_candidate) = candidate {
            let tail_rate = coefficient_proxy_score(&tail_pruned_candidate.coefficients, kind);
            let tail_score = lossy_txb_score(
                tail_rate,
                tail_pruned_candidate.sse,
                tail_pruned_candidate.variance_loss,
                regular_q_rd_quant_step(quant_step),
            );
            if tail_score < best_score
                && tail_pruned_candidate.sse
                    <= transform_candidate.sse.saturating_add(max_extra_sse)
            {
                best_candidate = tail_pruned_candidate;
                best_score = tail_score;
            }
        }
    }
    let dc_only_candidate = candidates.dc_only;
    let dc_rate = coefficient_proxy_score(&dc_only_candidate.coefficients, kind);
    let dc_score = lossy_txb_score(
        dc_rate,
        dc_only_candidate.sse,
        dc_only_candidate.variance_loss,
        regular_q_rd_quant_step(quant_step),
    );
    if dc_score < best_score
        && dc_only_candidate.sse <= transform_candidate.sse.saturating_add(max_extra_sse)
    {
        dc_only_candidate
    } else {
        best_candidate
    }
}

fn regular_quantized_txb_is_zero(candidate: &Av2LossyQuantizedResidualCandidate) -> bool {
    candidate.coefficients.iter().all(|&coefficient| coefficient == 0)
}

fn choose_lossy_txb(
    quantized_delta: i16,
    exact_residual: &[i32; TX4X4_SAMPLES],
    exact_coefficients: &[i32; TX4X4_SAMPLES],
    kind: Av2CoefficientProxyKind,
    quant_step: i32,
    max_txb_sse: usize,
    dc_sse: usize,
    dc_variance_loss: usize,
    quantized_candidate: Option<Av2LossyQuantizedResidualCandidate>,
    refined_quantized_candidate: Option<Av2LossyQuantizedResidualCandidate>,
    transform_quantized_candidate: Option<Av2LossyQuantizedResidualCandidate>,
    allow_dc_delta: bool,
) -> Av2LossyTxbChoice {
    let exact_score = coefficient_proxy_score(exact_coefficients, kind);
    let mut best_choice = Av2LossyTxbChoice::Exact;
    let mut best_score = exact_score;

    if allow_dc_delta {
        let dc_score = dc_delta_coefficient_proxy_score(quantized_delta, kind);
        let dc_candidate_score = lossy_txb_score(dc_score, dc_sse, dc_variance_loss, quant_step);
        if dc_candidate_score < best_score {
            best_score = dc_candidate_score;
            best_choice = Av2LossyTxbChoice::DcDelta(quantized_delta);
        }
    }

    for candidate in [
        quantized_candidate,
        refined_quantized_candidate,
        transform_quantized_candidate,
    ]
    .into_iter()
    .flatten()
    {
        if lossy_ac_candidate_is_reference_clean(&candidate.coefficients) {
            let quantized_score = coefficient_proxy_score(&candidate.coefficients, kind);
            let quantized_candidate_score =
                lossy_txb_score(quantized_score, candidate.sse, candidate.variance_loss, quant_step);
            if quantized_candidate_score < best_score {
                best_score = quantized_candidate_score;
                best_choice = if *exact_residual == candidate.residual {
                    Av2LossyTxbChoice::Exact
                } else {
                    Av2LossyTxbChoice::QuantizedResidual(candidate)
                };
            }
        }
    }

    if lossy_choice_sse(&best_choice, dc_sse) > max_txb_sse {
        return Av2LossyTxbChoice::Exact;
    }

    best_choice
}

fn lossy_choice_sse(choice: &Av2LossyTxbChoice, dc_sse: usize) -> usize {
    match choice {
        Av2LossyTxbChoice::Exact => 0,
        Av2LossyTxbChoice::DcDelta(_) => dc_sse,
        Av2LossyTxbChoice::QuantizedResidual(candidate) => candidate.sse,
    }
}

fn lossy_max_txb_sse(quant_step: i32) -> usize {
    let step = quant_step.max(1) as usize;
    step.saturating_mul(step).saturating_mul(TX4X4_SAMPLES) / 9
}

fn lossy_txb_score(
    rate_score: usize,
    sse: usize,
    variance_loss: usize,
    quant_step: i32,
) -> usize {
    const VARIANCE_LOSS_WEIGHT: usize = 2;
    let distortion = sse.saturating_add(variance_loss.saturating_mul(VARIANCE_LOSS_WEIGHT));
    rate_score.saturating_add(distortion / lossy_rd_distortion_divisor(quant_step))
}

fn lossy_rd_distortion_divisor(quant_step: i32) -> usize {
    quant_step.max(1) as usize
}

fn lossy_quality_rd_quant_step(quant_step: i32) -> i32 {
    (quant_step / 16).max(1)
}

fn regular_q_rd_quant_step(quant_step: i32) -> i32 {
    let _ = quant_step;
    4
}

fn regular_q_dc_only_max_extra_sse(quant_step: i32) -> usize {
    (quant_step.max(1) as usize).saturating_mul(2)
}

fn regular_q_tail_pruned_max_extra_sse(quant_step: i32) -> usize {
    (quant_step.max(1) as usize / 8).max(1)
}

fn lossy_should_try_ac_quantized(dc_sse: usize, quant_step: i32) -> bool {
    const AC_DISTORTION_GATE_MULTIPLIER: usize = 4;
    let step = quant_step.max(1) as usize;
    let threshold = step
        .saturating_mul(step)
        .saturating_mul(TX4X4_SAMPLES)
        .saturating_mul(AC_DISTORTION_GATE_MULTIPLIER);
    dc_sse >= threshold
}

fn lossy_should_try_refined_ac_quantized(quantized_sse: usize, quant_step: i32) -> bool {
    if refined_lossy_quant_step(quant_step) >= quant_step {
        return false;
    }
    let step = quant_step.max(1) as usize;
    quantized_sse >= step.saturating_mul(step).saturating_mul(TX4X4_SAMPLES) / 8
}

fn refined_lossy_quant_step(quant_step: i32) -> i32 {
    (quant_step / 4).max(1)
}

fn lossy_ac_candidate_is_reference_clean(coefficients: &[i32; TX4X4_SAMPLES]) -> bool {
    const MAX_REFERENCE_CLEAN_EOB: usize = TX4X4_SAMPLES;
    let (_, bounds) = lossless_coefficient_levels_and_bounds(coefficients);
    bounds.is_none_or(|(_, eob)| eob <= MAX_REFERENCE_CLEAN_EOB)
}
