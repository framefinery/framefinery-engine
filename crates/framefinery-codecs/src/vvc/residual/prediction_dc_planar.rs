fn predict_vvc_dc_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcIntraPredictionScratch,
    plane: &[VvcSample],
    region: VvcIntraPredictionRegion,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    let width = region.width;
    let height = region.height;
    debug_assert!(width <= VVC_CTU_SIZE);
    debug_assert!(height <= VVC_CTU_SIZE);
    prepare_vvc_intra_reference_edges(
        &mut scratch.references,
        plane,
        region,
        width,
        height,
        bit_depth,
        availability,
    );
    let top = &scratch.references.top[..width];
    let left = &scratch.references.left[..height];
    let dc = dc_prediction_value(top, left, width, height);
    prediction.clear();
    prediction.resize(width * height, dc);

    // VTM IntraPrediction::predIntraAng applies PDPC to DC mode when the
    // luma TU is at least MIN_TB_SIZEY in both dimensions and multiRefIdx is
    // zero.
    if region.reference_line == 0 && width >= 4 && height >= 4 {
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
    scratch: &mut VvcIntraPredictionScratch,
    plane: &[VvcSample],
    region: VvcIntraPredictionRegion,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    let width = region.width;
    let height = region.height;
    debug_assert!(width <= VVC_CTU_SIZE);
    debug_assert!(height <= VVC_CTU_SIZE);
    prepare_vvc_intra_reference_edges(
        &mut scratch.references,
        plane,
        region,
        width + 2,
        height + 2,
        bit_depth,
        availability,
    );
    if region.reference_line == 0 && region.is_luma && width * height > 32 {
        let top_left = top_left_reference(
            plane,
            region.plane_width,
            region.plane_height,
            region.x,
            region.y,
            bit_depth,
            0,
            availability,
        );
        filter_vvc_planar_references_in_place(
            &mut scratch.references.top,
            &mut scratch.references.left,
            top_left,
            width,
            height,
        );
    }
    let log2_w = width.ilog2();
    let log2_h = height.ilog2();
    let offset = 1i32 << (log2_w + log2_h);
    let final_shift = 1 + log2_w + log2_h;
    let bottom_left = i32::from(scratch.references.left[height]);
    let top_right = i32::from(scratch.references.top[width]);
    let max_sample = i32::from(bit_depth.max_sample());

    for x in 0..width {
        let top = i32::from(scratch.references.top[x]);
        scratch.planar.bottom_delta[x] = bottom_left - top;
        scratch.planar.top_work[x] = top << log2_h;
    }

    prediction.clear();
    prediction.resize(width * height, 0);
    for y in 0..height {
        let left = i32::from(scratch.references.left[y]);
        let right_delta = top_right - left;
        let mut hor_pred = left << log2_w;
        for x in 0..width {
            hor_pred += right_delta;
            scratch.planar.top_work[x] += scratch.planar.bottom_delta[x];
            let vert_pred = scratch.planar.top_work[x];
            let sample = ((hor_pred << log2_h) + (vert_pred << log2_w) + offset) >> final_shift;
            debug_assert!((0..=max_sample).contains(&sample));
            prediction[y * width + x] = sample as VvcSample;
        }
    }

    if region.reference_line == 0 && width >= 4 && height >= 4 {
        apply_vvc_planar_dc_pdpc(
            prediction,
            &scratch.references.top[..width],
            &scratch.references.left[..height],
            width,
            height,
            bit_depth,
        );
    }
}
