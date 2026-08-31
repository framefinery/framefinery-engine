// Scoring callers retain their established terminology while sharing the
// exact coding-family policy with entropy emission.
type Av2CoefficientProxyKind = Av2CoefficientCodingKind;

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
    let Some(symbol) = kind.next_high_range_symbol(level, pos, high_range_avg) else {
        return 0;
    };
    adaptive_high_range_score_bits(symbol.value, symbol.context) * 64
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

#[cfg(test)]
mod coefficient_high_range_tests {
    use super::*;

    fn assert_writer_matches_proxy(
        kind: Av2CoefficientProxyKind,
        expected_field_name: &'static str,
        sequence: &[(usize, u32)],
        expected_symbol_bits: usize,
        expected_final_average: u32,
        mut write: impl FnMut(&mut Av2EntropyWriter, usize, u32, &mut u32),
    ) {
        let mut writer = Av2EntropyWriter::new();
        let mut writer_average = 0;
        let mut proxy_average = 0;
        let mut proxy_score = 0;

        for &(pos, level) in sequence {
            write(&mut writer, pos, level, &mut writer_average);
            proxy_score +=
                coefficient_high_range_level_proxy_score(level, kind, pos, &mut proxy_average);
        }

        let payload = writer.finish();
        assert_eq!(writer_average, expected_final_average);
        assert_eq!(proxy_average, expected_final_average);
        assert_eq!(payload.symbol_bits, expected_symbol_bits);
        assert_eq!(proxy_score, expected_symbol_bits * 64);
        assert!(!payload.fields.is_empty());
        assert!(payload
            .fields
            .iter()
            .all(|field| field.name == expected_field_name));
    }

    #[test]
    fn coefficient_high_range_writer_matches_rate_proxy() {
        assert_writer_matches_proxy(
            Av2CoefficientProxyKind::LumaTransform,
            "tile.coeff.y.high_range",
            &[(0, 7), (0, 8), (0, 20), (15, 5), (15, 6), (15, 40)],
            25,
            18,
            write_luma_high_range,
        );
        assert_writer_matches_proxy(
            Av2CoefficientProxyKind::LumaIdtx,
            "tile.coeff.y.idtx_high_range",
            &[(0, 5), (0, 6), (7, 21), (15, 64)],
            26,
            32,
            |writer, _, level, average| write_idtx_high_range(writer, level, average),
        );
        assert_writer_matches_proxy(
            Av2CoefficientProxyKind::ChromaTransform,
            "tile.coeff.u.high_range",
            &[(0, 4), (0, 5), (0, 17), (1, 5), (1, 6), (1, 48)],
            27,
            22,
            |writer, pos, level, average| {
                write_chroma_high_range(writer, Av2ChromaPlane::U, pos, level, average)
            },
        );
        assert_writer_matches_proxy(
            Av2CoefficientProxyKind::ChromaTransform,
            "tile.coeff.v.high_range",
            &[(0, 5), (1, 6), (1, 48)],
            18,
            21,
            |writer, pos, level, average| {
                write_chroma_high_range(writer, Av2ChromaPlane::V, pos, level, average)
            },
        );
    }
}
