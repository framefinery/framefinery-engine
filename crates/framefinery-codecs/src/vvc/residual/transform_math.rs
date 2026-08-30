fn residuals_have_ac_energy(residuals: &[i16]) -> bool {
    residuals
        .first()
        .is_some_and(|first| residuals.iter().any(|value| value != first))
}

fn residual_sum_and_sse(residuals: &[i16]) -> (i64, i64) {
    residuals.iter().fold((0, 0), |(sum, sse), value| {
        let value = i64::from(*value);
        (sum + value, sse + value * value)
    })
}

fn div_round_nearest_i64(value: i64, divisor: i64) -> i64 {
    debug_assert!(divisor > 0);
    if value < 0 {
        -(((-value) + (divisor / 2)) / divisor)
    } else {
        (value + (divisor / 2)) / divisor
    }
}
