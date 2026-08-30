fn predict_vvc_vertical_oriented_angular_block(
    prediction: &mut [VvcSample],
    main: &[VvcSample],
    side: &[VvcSample],
    top_left: VvcSample,
    width: usize,
    height: usize,
    angle: i32,
    abs_inv_angle: i32,
    interpolation: VvcAngularInterpolation,
    reference_line: usize,
    shifted_refs: VvcShiftedAngularReferences,
    pdpc_scale: Option<u32>,
    bit_depth: SampleBitDepth,
) {
    for y in 0..height {
        let delta_pos = angle * (y as i32 + 1 + reference_line as i32);
        let delta_int = delta_pos >> 5;
        let delta_fract = delta_pos & 31;
        for x in 0..width {
            prediction[y * width + x] = angular_reference_prediction(
                main,
                side,
                top_left,
                delta_int + x as i32 + 1,
                delta_fract,
                abs_inv_angle,
                height,
                reference_line,
                shifted_refs,
                interpolation,
                bit_depth,
            );
        }
        apply_vvc_angular_pdpc_to_vertical_row(
            &mut prediction[y * width..(y + 1) * width],
            side,
            top_left,
            y,
            width,
            angle,
            abs_inv_angle,
            pdpc_scale,
            bit_depth,
        );
    }
}

fn predict_vvc_horizontal_oriented_angular_block(
    prediction: &mut [VvcSample],
    main: &[VvcSample],
    side: &[VvcSample],
    top_left: VvcSample,
    width: usize,
    height: usize,
    angle: i32,
    abs_inv_angle: i32,
    interpolation: VvcAngularInterpolation,
    reference_line: usize,
    shifted_refs: VvcShiftedAngularReferences,
    pdpc_scale: Option<u32>,
    bit_depth: SampleBitDepth,
) {
    for x in 0..width {
        let delta_pos = angle * (x as i32 + 1 + reference_line as i32);
        let delta_int = delta_pos >> 5;
        let delta_fract = delta_pos & 31;
        for y in 0..height {
            prediction[y * width + x] = angular_reference_prediction(
                main,
                side,
                top_left,
                delta_int + y as i32 + 1,
                delta_fract,
                abs_inv_angle,
                width,
                reference_line,
                shifted_refs,
                interpolation,
                bit_depth,
            );
        }
        apply_vvc_angular_pdpc_to_horizontal_column(
            prediction,
            side,
            top_left,
            x,
            width,
            height,
            angle,
            abs_inv_angle,
            pdpc_scale,
            bit_depth,
        );
    }
}
