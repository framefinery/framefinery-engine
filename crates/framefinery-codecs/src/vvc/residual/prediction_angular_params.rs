#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VvcAngularInterpolation {
    Linear,
    FourTapDct,
    FourTapSmoothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcShiftedAngularReferences {
    main_zero: VvcSample,
    side_zero: VvcSample,
    main_prefix: [VvcSample; VVC_MAX_MULTI_REF_LINE_IDX],
    main_prefix_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcAngularPredictionParams {
    is_vertical: bool,
    angle: i32,
    abs_inv_angle: i32,
    interpolation: VvcAngularInterpolation,
    pdpc_scale: Option<u32>,
    filter_luma_references: bool,
}

fn vvc_angular_prediction_params(
    width: usize,
    height: usize,
    mode_index: u8,
    is_luma: bool,
) -> VvcAngularPredictionParams {
    let pred_mode = vvc_modified_wide_angle(width, height, mode_index);
    let is_vertical = pred_mode >= i16::from(VVC_LUMA_MODE_DIAGONAL);
    let intra_pred_angle_mode = if is_vertical {
        pred_mode - i16::from(VVC_LUMA_MODE_VERTICAL)
    } else {
        -(pred_mode - i16::from(VVC_LUMA_MODE_HORIZONTAL))
    };
    let abs_ang_mode = intra_pred_angle_mode.unsigned_abs() as usize;
    let abs_angle = VVC_INTRA_ANG_TABLE[abs_ang_mode];
    let angle = if intra_pred_angle_mode < 0 {
        -abs_angle
    } else {
        abs_angle
    };
    let abs_inv_angle = VVC_INTRA_INV_ANG_TABLE[abs_ang_mode];
    let pdpc_scale = vvc_angular_pdpc_scale(width, height, angle, abs_inv_angle, is_vertical);
    let (interpolation, filter_luma_references) = if is_luma {
        vvc_luma_angular_filter_params(width, height, pred_mode, abs_angle)
    } else {
        (VvcAngularInterpolation::Linear, false)
    };

    VvcAngularPredictionParams {
        is_vertical,
        angle,
        abs_inv_angle,
        interpolation,
        pdpc_scale,
        filter_luma_references,
    }
}

fn vvc_modified_wide_angle(width: usize, height: usize, mode_index: u8) -> i16 {
    let mut pred_mode = i16::from(mode_index);
    if (2..=66).contains(&mode_index) {
        const MODE_SHIFT: [i16; 6] = [0, 6, 10, 12, 14, 15];
        let delta_size = width.ilog2().abs_diff(height.ilog2()) as usize;
        let shift = MODE_SHIFT[delta_size.min(MODE_SHIFT.len() - 1)];
        if width > height && pred_mode < 2 + shift {
            pred_mode += 65;
        } else if height > width && pred_mode > 66 - shift {
            pred_mode -= 65;
        }
    }
    pred_mode
}

fn vvc_luma_angular_filter_params(
    width: usize,
    height: usize,
    pred_mode: i16,
    abs_angle: i32,
) -> (VvcAngularInterpolation, bool) {
    let diff = (pred_mode - i16::from(VVC_LUMA_MODE_HORIZONTAL))
        .abs()
        .min((pred_mode - i16::from(VVC_LUMA_MODE_VERTICAL)).abs());
    let log2_size = ((width.ilog2() + height.ilog2()) >> 1) as usize;
    let threshold = i16::from(
        VVC_INTRA_REFERENCE_FILTER_THRESHOLD
            [log2_size.min(VVC_INTRA_REFERENCE_FILTER_THRESHOLD.len() - 1)],
    );
    if diff <= threshold {
        return (VvcAngularInterpolation::FourTapDct, false);
    }
    if vvc_angular_integer_slope(abs_angle) {
        (VvcAngularInterpolation::FourTapDct, true)
    } else {
        (VvcAngularInterpolation::FourTapSmoothing, false)
    }
}
