#[cfg(test)]
pub(in crate::vvc) fn transform_skip_luma_ac_levels_and_flag(
    residuals: &[i16],
    width: usize,
) -> ([i16; super::VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let height = residuals.len() / width;
    let layout = vvc_luma_transform_skip_residual_layout(width, height);
    transform_skip_ac_levels_and_flag(
        residuals,
        layout.source_width,
        layout.active_width,
        layout.active_height,
        |level| level,
    )
}

fn transform_skip_ac_levels_and_flag<const AC_COEFFS: usize>(
    residuals: &[i16],
    width: usize,
    active_width: usize,
    active_height: usize,
    map_level: impl Fn(i16) -> i16,
) -> ([i16; AC_COEFFS], bool) {
    let mut levels = [0; AC_COEFFS];
    let mut has_ac = false;
    for y in 0..active_height {
        for x in 0..active_width {
            if x == 0 && y == 0 {
                continue;
            }
            let Some(&residual) = residuals.get(y * width + x) else {
                continue;
            };
            let level = map_level(residual);
            levels[y * active_width + x - 1] = level;
            has_ac |= level != 0;
        }
    }
    (levels, has_ac)
}

#[derive(Debug, Clone, Copy)]
struct VvcTransformSkipResidualLayout {
    source_width: usize,
    source_height: usize,
    active_width: usize,
    active_height: usize,
    coefficient_stride: usize,
}

impl VvcTransformSkipResidualLayout {
    fn packed(
        source_width: usize,
        source_height: usize,
        active_width: usize,
        active_height: usize,
    ) -> Self {
        Self {
            source_width,
            source_height,
            active_width,
            active_height,
            coefficient_stride: active_width,
        }
    }

    fn with_coefficient_stride(mut self, coefficient_stride: usize) -> Self {
        self.coefficient_stride = coefficient_stride;
        self
    }

    fn debug_assert_source_len(self, residual_len: usize) {
        debug_assert_eq!(
            residual_len,
            self.source_width.saturating_mul(self.source_height)
        );
    }

    fn debug_assert_coefficients_fit<const AC_COEFFS: usize>(self) {
        debug_assert!(self.active_width <= self.source_width);
        debug_assert!(self.active_height <= self.source_height);
        debug_assert!(self.active_width <= self.coefficient_stride);
        debug_assert!(
            self.coefficient_stride.saturating_mul(self.active_height)
                <= VVC_TRANSFORM_SKIP_MAX_SAMPLES
        );
        debug_assert!(
            self.coefficient_stride
                .saturating_mul(self.active_height)
                .saturating_sub(1)
                <= AC_COEFFS
        );
    }
}

fn vvc_luma_transform_skip_residual_layout(
    width: usize,
    height: usize,
) -> VvcTransformSkipResidualLayout {
    let (active_width, active_height) = vvc_luma_transform_skip_active_extent(width, height);
    VvcTransformSkipResidualLayout::packed(width, height, active_width, active_height)
}

fn vvc_chroma_transform_skip_residual_layout(
    width: usize,
    height: usize,
) -> VvcTransformSkipResidualLayout {
    VvcTransformSkipResidualLayout::packed(width, height, width.min(8), height.min(8))
}

fn vvc_chroma_bdpcm_transform_skip_residual_layout(
    width: usize,
    height: usize,
) -> VvcTransformSkipResidualLayout {
    debug_assert_eq!(VVC_CHROMA_AC_POSITIONS_4X4.len() + 1, 4 * 4);
    VvcTransformSkipResidualLayout::packed(width, height, width.min(4), height.min(4))
        .with_coefficient_stride(4)
}

fn finalize_vvc_transform_skip_residual_block<const AC_COEFFS: usize>(
    residuals: &[i16],
    layout: VvcTransformSkipResidualLayout,
    quant_table: &VvcTransformSkipQuantTable,
) -> VvcFinalizedResidualBlock<AC_COEFFS> {
    layout.debug_assert_source_len(residuals.len());
    layout.debug_assert_coefficients_fit::<AC_COEFFS>();
    debug_assert_eq!(layout.coefficient_stride, layout.active_width);
    if residuals.iter().all(|&residual| residual == 0) {
        return VvcFinalizedResidualBlock::zero_transform_skip(VvcBdpcmMode::None);
    }
    let dc_level = residuals
        .first()
        .copied()
        .map(|level| quant_table.level(level))
        .unwrap_or(0);
    let (ac_levels, has_ac) = transform_skip_ac_levels_and_flag(
        residuals,
        layout.source_width,
        layout.active_width,
        layout.active_height,
        |level| quant_table.level(level),
    );
    VvcFinalizedResidualBlock {
        dc_level,
        ac_levels,
        has_ac,
        transform_skip: true,
        bdpcm_mode: VvcBdpcmMode::None,
    }
}

fn finalize_vvc_bdpcm_transform_skip_residual_block<const AC_COEFFS: usize>(
    residuals: &[i16],
    layout: VvcTransformSkipResidualLayout,
    quant_table: &VvcTransformSkipQuantTable,
    bdpcm_mode: VvcBdpcmMode,
) -> VvcFinalizedResidualBlock<AC_COEFFS> {
    debug_assert!(bdpcm_mode.is_enabled());
    layout.debug_assert_source_len(residuals.len());
    layout.debug_assert_coefficients_fit::<AC_COEFFS>();
    if residuals.iter().all(|&residual| residual == 0) {
        return VvcFinalizedResidualBlock::zero_transform_skip(bdpcm_mode);
    }
    let mut level_state = VvcBdpcmLevelState::default();
    let mut ac_levels = [0; AC_COEFFS];
    let mut dc_level = 0i16;
    let mut has_ac = false;
    for y in 0..layout.active_height {
        level_state.begin_row();
        for x in 0..layout.active_width {
            let level = quant_table.level(residuals[y * layout.source_width + x]);
            let coeff = level_state.difference(bdpcm_mode, x, y, level);
            if x == 0 && y == 0 {
                dc_level = coeff;
            } else {
                ac_levels[y * layout.coefficient_stride + x - 1] = coeff;
                has_ac |= coeff != 0;
            }
        }
    }
    VvcFinalizedResidualBlock {
        dc_level,
        ac_levels,
        has_ac,
        transform_skip: true,
        bdpcm_mode,
    }
}

#[cfg(test)]
pub(in crate::vvc) fn transform_skip_luma_ac_levels_and_flag_with_qp(
    residuals: &[i16],
    width: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
) -> ([i16; super::VVC_LUMA_AC_COEFFS_PER_TU], bool) {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    let height = residuals.len() / width;
    let layout = vvc_luma_transform_skip_residual_layout(width, height);
    transform_skip_ac_levels_and_flag(
        residuals,
        layout.source_width,
        layout.active_width,
        layout.active_height,
        |level| {
            quantize_vvc_transform_skip_level_with_params(
                level,
                scale,
                right_shift,
                VVC_TRANSFORM_SKIP_LEVEL_SEARCH_RADIUS,
            )
        },
    )
}

fn vvc_luma_transform_skip_active_extent(width: usize, height: usize) -> (usize, usize) {
    if width == 8 && height == 8 {
        (8, 8)
    } else {
        (width.min(4), height.min(4))
    }
}

#[cfg(test)]
pub(in crate::vvc) fn transform_skip_chroma_ac_levels_and_flag(
    residuals: &[i16],
    width: usize,
) -> ([i16; VVC_CHROMA_AC_COEFFS_PER_TU], bool) {
    let height = residuals.len() / width;
    let layout = vvc_chroma_transform_skip_residual_layout(width, height);
    transform_skip_ac_levels_and_flag(
        residuals,
        layout.source_width,
        layout.active_width,
        layout.active_height,
        |level| level,
    )
}

#[cfg(test)]
pub(in crate::vvc) fn quantize_vvc_transform_skip_level(
    residual: i16,
    bit_depth: SampleBitDepth,
    qp: i32,
) -> i16 {
    quantize_vvc_transform_skip_level_with_radius(
        residual,
        bit_depth,
        qp,
        VVC_TRANSFORM_SKIP_LEVEL_SEARCH_RADIUS,
    )
}

#[cfg(test)]
fn quantize_vvc_transform_skip_level_with_radius(
    residual: i16,
    bit_depth: SampleBitDepth,
    qp: i32,
    search_radius: i64,
) -> i16 {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    quantize_vvc_transform_skip_level_with_params(residual, scale, right_shift, search_radius)
}

#[inline]
fn quantize_vvc_transform_skip_level_with_params(
    residual: i16,
    scale: i32,
    right_shift: i32,
    search_radius: i64,
) -> i16 {
    if residual == 0 {
        return 0;
    }
    let estimate = if right_shift > 0 {
        div_round_nearest_i64(i64::from(residual) << right_shift, i64::from(scale))
    } else {
        div_round_nearest_i64(
            i64::from(residual),
            i64::from(scale) << (-right_shift as u32),
        )
    };
    if search_radius == 1 {
        return quantize_vvc_transform_skip_level_radius_one_with_params(
            estimate,
            residual,
            scale,
            right_shift,
        );
    }
    let mut best_level = estimate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
    let mut best_error =
        vvc_transform_skip_level_error_with_params(best_level, residual, scale, right_shift);
    for candidate in (estimate - search_radius)..=(estimate + search_radius) {
        let level = candidate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
        if level == best_level {
            continue;
        }
        let error = vvc_transform_skip_level_error_with_params(level, residual, scale, right_shift);
        if error < best_error
            || (error == best_error && level.unsigned_abs() < best_level.unsigned_abs())
        {
            best_error = error;
            best_level = level;
        }
    }
    best_level
}

#[inline]
fn quantize_vvc_transform_skip_level_radius_one_with_params(
    estimate: i64,
    residual: i16,
    scale: i32,
    right_shift: i32,
) -> i16 {
    let mut best_level = estimate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
    let mut best_error =
        vvc_transform_skip_level_error_with_params(best_level, residual, scale, right_shift);
    for candidate in [estimate - 1, estimate + 1] {
        let level = candidate.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
        if level == best_level {
            continue;
        }
        let error = vvc_transform_skip_level_error_with_params(level, residual, scale, right_shift);
        if error < best_error
            || (error == best_error && level.unsigned_abs() < best_level.unsigned_abs())
        {
            best_error = error;
            best_level = level;
        }
    }
    best_level
}

fn div_round_nearest_i64(value: i64, divisor: i64) -> i64 {
    debug_assert!(divisor > 0);
    if value < 0 {
        -(((-value) + (divisor / 2)) / divisor)
    } else {
        (value + (divisor / 2)) / divisor
    }
}

#[inline]
fn vvc_transform_skip_level_error_with_params(
    level: i16,
    residual: i16,
    scale: i32,
    right_shift: i32,
) -> u64 {
    let reconstructed = reconstruct_vvc_transform_skip_level_with_params(level, scale, right_shift);
    let diff = i64::from(residual) - i64::from(reconstructed);
    (diff * diff) as u64
}

#[inline]
fn reconstruct_vvc_transform_skip_level_with_params(
    level: i16,
    scale: i32,
    right_shift: i32,
) -> i16 {
    if level == 0 {
        return 0;
    }
    let value = if right_shift > 0 {
        let add = 1i64 << ((right_shift - 1) as u32);
        (i64::from(level) * i64::from(scale) + add) >> (right_shift as u32)
    } else {
        i64::from(level) * i64::from(scale) * (1i64 << ((-right_shift) as u32))
    };
    value.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16
}

fn vvc_transform_skip_dequant_params(bit_depth: SampleBitDepth, qp: i32) -> (i32, i32) {
    let qp_bd_offset = (i32::from(bit_depth.bits()) - 8) * 6;
    let transform_skip_qp = (qp + qp_bd_offset).max(4);
    let qp_rem = transform_skip_qp.rem_euclid(6) as usize;
    let qp_per = transform_skip_qp.div_euclid(6);
    let scale = VVC_TRANSFORM_SKIP_INV_QUANT_SCALES[qp_rem];
    let right_shift = 6 - qp_per;
    (scale, right_shift)
}

fn vvc_transform_skip_qp_reconstructs_exact(bit_depth: SampleBitDepth, qp: i32) -> bool {
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, qp);
    right_shift >= 0 && i64::from(scale) == (1i64 << (right_shift as u32))
}
