fn predict_vvc_angular_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcIntraPredictionScratch,
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    mode_index: u8,
    bit_depth: SampleBitDepth,
    is_luma: bool,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    debug_assert!(width <= VVC_CTU_SIZE);
    debug_assert!(height <= VVC_CTU_SIZE);
    debug_assert!((2..=66).contains(&mode_index));
    let reference_len = ((width.max(height)) << 1) + 4;
    top_references_into(
        &mut scratch.top,
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        reference_len,
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
        reference_len,
        bit_depth,
        reference_line,
        availability,
    );
    let mut params = vvc_angular_prediction_params(width, height, mode_index, is_luma);
    if is_luma && reference_line != 0 {
        params.interpolation = VvcAngularInterpolation::FourTapDct;
        params.filter_luma_references = false;
    }
    if params.angle == 0 && reference_line == 0 {
        let top_left = top_left_reference(
            plane,
            plane_width,
            plane_height,
            start_x,
            start_y,
            bit_depth,
            reference_line,
            availability,
        );
        predict_vvc_zero_angle_angular_block_into(
            prediction,
            scratch,
            width,
            height,
            params.is_vertical,
            params.abs_inv_angle,
            params.pdpc_scale,
            top_left,
            bit_depth,
        );
        return;
    }
    let mut top_left = angular_main_zero_reference(
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        bit_depth,
        reference_line,
        params.is_vertical,
        availability,
    );
    let mut angular_refs = angular_shifted_reference_samples(
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        bit_depth,
        reference_line,
        params.is_vertical,
        top_left,
        availability,
    );
    if reference_line == 0 && params.filter_luma_references {
        top_left = filter_vvc_angular_references_in_place(
            &mut scratch.top,
            &mut scratch.left,
            top_left,
            width << 1,
            height << 1,
        );
        angular_refs.main_zero = top_left;
        angular_refs.side_zero = top_left;
    }
    if params.angle >= 0 {
        if params.is_vertical {
            replicate_vvc_positive_angular_main_extension(&mut scratch.top, width << 1);
        } else {
            replicate_vvc_positive_angular_main_extension(&mut scratch.left, height << 1);
        }
    }

    prediction.clear();
    prediction.resize(width * height, 0);
    if params.is_vertical {
        predict_vvc_vertical_oriented_angular_block(
            prediction,
            &scratch.top[..reference_len],
            &scratch.left[..reference_len],
            top_left,
            width,
            height,
            params.angle,
            params.abs_inv_angle,
            params.interpolation,
            reference_line,
            angular_refs,
            (reference_line == 0).then_some(params.pdpc_scale).flatten(),
            bit_depth,
        );
    } else {
        predict_vvc_horizontal_oriented_angular_block(
            prediction,
            &scratch.left[..reference_len],
            &scratch.top[..reference_len],
            top_left,
            width,
            height,
            params.angle,
            params.abs_inv_angle,
            params.interpolation,
            reference_line,
            angular_refs,
            (reference_line == 0).then_some(params.pdpc_scale).flatten(),
            bit_depth,
        );
    }
}

fn predict_vvc_zero_angle_angular_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &VvcIntraPredictionScratch,
    width: usize,
    height: usize,
    is_vertical: bool,
    abs_inv_angle: i32,
    pdpc_scale: Option<u32>,
    top_left: VvcSample,
    bit_depth: SampleBitDepth,
) {
    prediction.clear();
    prediction.resize(width * height, 0);
    if is_vertical {
        for y in 0..height {
            let row = &mut prediction[y * width..(y + 1) * width];
            row.copy_from_slice(&scratch.top[..width]);
            apply_vvc_angular_pdpc_to_vertical_row(
                row,
                &scratch.left,
                top_left,
                y,
                width,
                0,
                abs_inv_angle,
                pdpc_scale,
                bit_depth,
            );
        }
    } else {
        for y in 0..height {
            prediction[y * width..(y + 1) * width].fill(scratch.left[y]);
        }
        for x in 0..width {
            apply_vvc_angular_pdpc_to_horizontal_column(
                prediction,
                &scratch.top,
                top_left,
                x,
                width,
                height,
                0,
                abs_inv_angle,
                pdpc_scale,
                bit_depth,
            );
        }
    }
}

fn vvc_angular_integer_slope(abs_angle: i32) -> bool {
    (abs_angle & 0x1f) == 0
}

fn angular_side_reference_sample(side: &[VvcSample], index: usize) -> VvcSample {
    if index == 0 {
        return side[0];
    }
    side[(index - 1).min(side.len().saturating_sub(1))]
}

fn replicate_vvc_positive_angular_main_extension(main: &mut [VvcSample], reference_len: usize) {
    if reference_len == 0 || reference_len > main.len() {
        return;
    }
    let edge = main[reference_len - 1];
    for sample in &mut main[reference_len..] {
        *sample = edge;
    }
}
