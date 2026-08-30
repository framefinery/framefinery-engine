fn vvc_transform_skip_residual_sse<const AC_COEFFS: usize>(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    active_width: usize,
    active_height: usize,
    ac_stride: usize,
    bdpcm_ac_stride: usize,
    ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<AC_COEFFS>,
) -> u64 {
    debug_assert!(residual.transform_skip);
    debug_assert_eq!(source_residuals.len(), width * height);
    if residual.bdpcm_mode.is_enabled() {
        return vvc_bdpcm_transform_skip_residual_sse(
            source_residuals,
            width,
            height,
            active_width,
            active_height,
            bdpcm_ac_stride,
            ts_quant,
            residual,
        );
    }

    let mut sse = 0u64;
    for y in 0..active_height {
        let source_row = &source_residuals[y * width..(y + 1) * width];
        for x in 0..active_width {
            let level = if x == 0 && y == 0 {
                residual.dc_level
            } else {
                residual.ac_levels[y * ac_stride + x - 1]
            };
            sse += residual_diff_square(source_row[x], ts_quant.reconstructed(level));
        }
        for &source in &source_row[active_width..] {
            sse += residual_square(source);
        }
    }
    for y in active_height..height {
        let source_row = &source_residuals[y * width..(y + 1) * width];
        for &source in source_row {
            sse += residual_square(source);
        }
    }
    sse
}

fn vvc_bdpcm_transform_skip_residual_sse<const AC_COEFFS: usize>(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    active_width: usize,
    active_height: usize,
    ac_stride: usize,
    ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<AC_COEFFS>,
) -> u64 {
    let mut sse = 0u64;
    let mut vertical_predictors = [0i16; 8];
    for y in 0..active_height {
        let source_row = &source_residuals[y * width..(y + 1) * width];
        let mut horizontal_predictor = 0i16;
        for x in 0..active_width {
            let delta = if x == 0 && y == 0 {
                residual.dc_level
            } else {
                residual.ac_levels[y * ac_stride + x - 1]
            };
            let level = match residual.bdpcm_mode {
                VvcBdpcmMode::None => unreachable!("BDPCM SSE requires a direction"),
                VvcBdpcmMode::Horizontal if x > 0 => {
                    add_bdpcm_quantized_levels(delta, horizontal_predictor)
                }
                VvcBdpcmMode::Vertical if y > 0 => {
                    add_bdpcm_quantized_levels(delta, vertical_predictors[x])
                }
                VvcBdpcmMode::Horizontal | VvcBdpcmMode::Vertical => delta,
            };
            horizontal_predictor = level;
            vertical_predictors[x] = level;
            sse += residual_diff_square(source_row[x], ts_quant.reconstructed(level));
        }
        for &source in &source_row[active_width..] {
            sse += residual_square(source);
        }
    }
    for y in active_height..height {
        let source_row = &source_residuals[y * width..(y + 1) * width];
        for &source in source_row {
            sse += residual_square(source);
        }
    }
    sse
}

#[inline]
fn add_bdpcm_quantized_levels(level: i16, predictor: i16) -> i16 {
    (i32::from(level) + i32::from(predictor))
        .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

#[inline]
fn residual_square(sample: i16) -> u64 {
    let sample = i64::from(sample);
    (sample * sample) as u64
}

#[inline]
fn residual_diff_square(source: i16, reconstructed: i16) -> u64 {
    let diff = i64::from(source) - i64::from(reconstructed);
    (diff * diff) as u64
}
