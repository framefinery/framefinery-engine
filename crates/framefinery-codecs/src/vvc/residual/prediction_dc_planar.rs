fn predict_vvc_dc_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    debug_assert!(width <= VVC_CTU_SIZE);
    debug_assert!(height <= VVC_CTU_SIZE);
    top_references_into(
        &mut scratch.top,
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        width,
        bit_depth,
        reference_line,
        availability,
    );
    left_references_into(
        &mut scratch.left,
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        height,
        bit_depth,
        reference_line,
        availability,
    );
    let top = &scratch.top[..width];
    let left = &scratch.left[..height];
    let dc = dc_prediction_value(top, left, width, height);
    prediction.clear();
    prediction.resize(width * height, dc);

    // VTM IntraPrediction::predIntraAng applies PDPC to DC mode when the
    // luma TU is at least MIN_TB_SIZEY in both dimensions and multiRefIdx is
    // zero.
    if reference_line == 0 && width >= 4 && height >= 4 {
        let scale = ((width.ilog2() as i32 - 2 + height.ilog2() as i32 - 2 + 2) >> 2) as u32;
        let max_sample = i32::from(bit_depth.max_sample());
        for y in 0..height {
            let wt = 32i32 >> ((y << 1) >> scale).min(31);
            let left_sample = i32::from(left[y]);
            for x in 0..width {
                let wl = 32i32 >> ((x << 1) >> scale).min(31);
                let top_sample = i32::from(top[x]);
                let val = i32::from(dc);
                prediction[y * width + x] = (val
                    + ((wl * (left_sample - val) + wt * (top_sample - val) + 32) >> 6))
                    .clamp(0, max_sample) as VvcSample;
            }
        }
    }
}

fn predict_vvc_planar_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    filter_luma_references: bool,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    debug_assert!(width <= VVC_CTU_SIZE);
    debug_assert!(height <= VVC_CTU_SIZE);
    top_references_into(
        &mut scratch.top,
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        width + 2,
        bit_depth,
        reference_line,
        availability,
    );
    left_references_into(
        &mut scratch.left,
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        height + 2,
        bit_depth,
        reference_line,
        availability,
    );
    if reference_line == 0 && filter_luma_references && width * height > 32 {
        let top_left = top_left_reference(
            plane,
            plane_width,
            plane_height,
            start_x,
            start_y,
            bit_depth,
            0,
            availability,
        );
        filter_vvc_planar_references_in_place(
            &mut scratch.top,
            &mut scratch.left,
            top_left,
            width,
            height,
        );
    }
    let log2_w = width.ilog2();
    let log2_h = height.ilog2();
    let offset = 1i32 << (log2_w + log2_h);
    let final_shift = 1 + log2_w + log2_h;
    let bottom_left = i32::from(scratch.left[height]);
    let top_right = i32::from(scratch.top[width]);
    let max_sample = i32::from(bit_depth.max_sample());

    for x in 0..width {
        let top = i32::from(scratch.top[x]);
        scratch.bottom_delta[x] = bottom_left - top;
        scratch.top_work[x] = top << log2_h;
    }

    prediction.clear();
    prediction.resize(width * height, 0);
    for y in 0..height {
        let left = i32::from(scratch.left[y]);
        let right_delta = top_right - left;
        let mut hor_pred = left << log2_w;
        for x in 0..width {
            hor_pred += right_delta;
            scratch.top_work[x] += scratch.bottom_delta[x];
            let vert_pred = scratch.top_work[x];
            let sample = ((hor_pred << log2_h) + (vert_pred << log2_w) + offset) >> final_shift;
            debug_assert!((0..=max_sample).contains(&sample));
            prediction[y * width + x] = sample as VvcSample;
        }
    }

    if reference_line == 0 && width >= 4 && height >= 4 {
        apply_vvc_planar_dc_pdpc(
            prediction,
            &scratch.top[..width],
            &scratch.left[..height],
            width,
            height,
            bit_depth,
        );
    }
}
