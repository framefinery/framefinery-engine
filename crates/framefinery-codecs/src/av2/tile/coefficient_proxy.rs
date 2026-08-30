#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Av2CoefficientProxyKind {
    LumaTransform,
    LumaIdtx,
    ChromaTransform,
}

fn coefficient_proxy_score(
    coefficients: &[i32; TX4X4_SAMPLES],
    kind: Av2CoefficientProxyKind,
) -> usize {
    let (levels, bounds) = lossless_coefficient_levels_and_bounds(coefficients);
    let Some((first, eob)) = bounds else {
        return 16;
    };

    let range = match kind {
        Av2CoefficientProxyKind::LumaIdtx => first..TX4X4_SAMPLES,
        Av2CoefficientProxyKind::LumaTransform | Av2CoefficientProxyKind::ChromaTransform => 0..eob,
    };
    let mut score = 96 + range.len() * 10;
    for scan_index in range {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos] as usize;
        if level == 0 {
            continue;
        }
        score += 80 + level.min(12) * 14;
    }

    let mut high_range_avg = 0u32;
    match kind {
        Av2CoefficientProxyKind::LumaIdtx => {
            for scan_index in 0..TX4X4_SAMPLES {
                score += coefficient_high_range_proxy_score(
                    &levels,
                    kind,
                    TX4X4_SCAN[scan_index],
                    &mut high_range_avg,
                );
            }
        }
        Av2CoefficientProxyKind::LumaTransform | Av2CoefficientProxyKind::ChromaTransform => {
            for scan_index in (0..eob).rev() {
                score += coefficient_high_range_proxy_score(
                    &levels,
                    kind,
                    TX4X4_SCAN[scan_index],
                    &mut high_range_avg,
                );
            }
        }
    }

    score
}

fn dc_delta_coefficient_proxy_score(delta: i16, kind: Av2CoefficientProxyKind) -> usize {
    let level = u32::from(delta.unsigned_abs()) * 4;
    if level == 0 {
        return 16;
    }

    let mut score = 96 + 10 + 80 + level.min(12) as usize * 14;
    let mut high_range_avg = 0u32;
    score += coefficient_high_range_level_proxy_score(level, kind, 0, &mut high_range_avg);
    score
}

fn coefficient_high_range_proxy_score(
    levels: &[u32; TX4X4_SAMPLES],
    kind: Av2CoefficientProxyKind,
    pos: usize,
    high_range_avg: &mut u32,
) -> usize {
    let level = levels[pos];
    coefficient_high_range_level_proxy_score(level, kind, pos, high_range_avg)
}

fn coefficient_high_range_level_proxy_score(
    level: u32,
    kind: Av2CoefficientProxyKind,
    pos: usize,
    high_range_avg: &mut u32,
) -> usize {
    if level == 0 {
        return 0;
    }
    let (threshold, decoded_base) = match kind {
        Av2CoefficientProxyKind::LumaIdtx => (5, 6),
        Av2CoefficientProxyKind::LumaTransform if luma_lf_limits(pos) => (7, 8),
        Av2CoefficientProxyKind::LumaTransform => (5, 6),
        Av2CoefficientProxyKind::ChromaTransform if chroma_lf_limits(pos) => (4, 5),
        Av2CoefficientProxyKind::ChromaTransform => (5, 6),
    };
    if level <= threshold {
        return 0;
    }
    let high_range = level.saturating_sub(decoded_base);
    let score = adaptive_high_range_score_bits(high_range, *high_range_avg) * 64;
    *high_range_avg = (*high_range_avg + high_range) >> 1;
    score
}

fn adaptive_high_range_score_bits(value: u32, context: u32) -> usize {
    let m = adaptive_high_range_rice_parameter(context);
    truncated_rice_score_bits(value, m, m + 1, (m + 4).min(6))
}

fn truncated_rice_score_bits(value: u32, m: u8, k: u8, cmax: u8) -> usize {
    let q = value >> m;
    if q >= u32::from(cmax) {
        usize::from(cmax) + exp_golomb_score_bits(value - (u32::from(cmax) << m), k)
    } else {
        q as usize + 1 + usize::from(m)
    }
}

fn exp_golomb_score_bits(value: u32, k: u8) -> usize {
    let x = value + (1u32 << k);
    let length = (u32::BITS - x.leading_zeros()) as u8;
    usize::from(length - 1 - k + length)
}
