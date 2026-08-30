fn round_div_i32(value: i32, divisor: i32) -> i32 {
    debug_assert!(divisor > 0);
    if value >= 0 {
        (value + divisor / 2) / divisor
    } else {
        -((-value + divisor / 2) / divisor)
    }
}

fn quantize_i32_to_step(value: i32, step: i32) -> i32 {
    debug_assert!(step > 0);
    round_div_i32(value, step) * step
}

fn quantized_txb_eob(coefficients: &[i32; TX4X4_SAMPLES]) -> usize {
    TX4X4_SCAN
        .iter()
        .rposition(|&pos| coefficients[pos] != 0)
        .map_or(0, |index| index + 1)
}
