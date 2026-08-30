fn apply_vvc_planar_dc_pdpc(
    prediction: &mut [VvcSample],
    top: &[VvcSample],
    left: &[VvcSample],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
) {
    let scale = ((width.ilog2() as i32 - 2 + height.ilog2() as i32 - 2 + 2) >> 2) as u32;
    let max_sample = i32::from(bit_depth.max_sample());
    for y in 0..height {
        let wt = 32i32 >> ((y << 1) >> scale).min(31);
        let left_sample = i32::from(left[y]);
        for x in 0..width {
            let wl = 32i32 >> ((x << 1) >> scale).min(31);
            let top_sample = i32::from(top[x]);
            let val = i32::from(prediction[y * width + x]);
            prediction[y * width + x] = (val
                + ((wl * (left_sample - val) + wt * (top_sample - val) + 32) >> 6))
                .clamp(0, max_sample) as VvcSample;
        }
    }
}

fn apply_vvc_angular_pdpc_to_vertical_row(
    row: &mut [VvcSample],
    side: &[VvcSample],
    top_left: VvcSample,
    y: usize,
    width: usize,
    angle: i32,
    abs_inv_angle: i32,
    pdpc_scale: Option<u32>,
    bit_depth: SampleBitDepth,
) {
    let Some(scale) = pdpc_scale else {
        return;
    };
    let span = (3usize << scale).min(width);
    let max_sample = i32::from(bit_depth.max_sample());
    let top_left = i32::from(top_left);
    let mut inv_angle_sum = 256;
    for (x, sample) in row.iter_mut().take(span).enumerate() {
        let side_index = if angle == 0 {
            y + 1
        } else {
            inv_angle_sum += abs_inv_angle;
            y + (inv_angle_sum >> 9).max(0) as usize + 1
        };
        let weight = 32i32 >> ((2 * x) >> scale).min(31);
        let val = i32::from(*sample);
        let side_sample = i32::from(angular_side_reference_sample(side, side_index));
        let anchor = if angle == 0 { top_left } else { val };
        *sample =
            (val + ((weight * (side_sample - anchor) + 32) >> 6)).clamp(0, max_sample) as VvcSample;
    }
}

fn apply_vvc_angular_pdpc_to_horizontal_column(
    prediction: &mut [VvcSample],
    side: &[VvcSample],
    top_left: VvcSample,
    x: usize,
    width: usize,
    height: usize,
    angle: i32,
    abs_inv_angle: i32,
    pdpc_scale: Option<u32>,
    bit_depth: SampleBitDepth,
) {
    let Some(scale) = pdpc_scale else {
        return;
    };
    let span = (3usize << scale).min(height);
    let max_sample = i32::from(bit_depth.max_sample());
    let top_left = i32::from(top_left);
    let mut inv_angle_sum = 256;
    for y in 0..span {
        let side_index = if angle == 0 {
            x + 1
        } else {
            inv_angle_sum += abs_inv_angle;
            x + (inv_angle_sum >> 9).max(0) as usize + 1
        };
        let weight = 32i32 >> ((2 * y) >> scale).min(31);
        let idx = y * width + x;
        let val = i32::from(prediction[idx]);
        let side_sample = i32::from(angular_side_reference_sample(side, side_index));
        let anchor = if angle == 0 { top_left } else { val };
        prediction[idx] =
            (val + ((weight * (side_sample - anchor) + 32) >> 6)).clamp(0, max_sample) as VvcSample;
    }
}

fn vvc_angular_pdpc_scale(
    width: usize,
    height: usize,
    angle: i32,
    abs_inv_angle: i32,
    is_vertical: bool,
) -> Option<u32> {
    if width < 4 || height < 4 || angle < 0 {
        return None;
    }
    if angle == 0 {
        return Some(((width.ilog2() + height.ilog2() - 2) >> 2).min(31));
    }
    let side_size = if is_vertical { height } else { width };
    let scale = (side_size.ilog2() as i32 - ((3 * abs_inv_angle - 2).ilog2() as i32 - 8)).min(2);
    (scale >= 0).then_some(scale as u32)
}
