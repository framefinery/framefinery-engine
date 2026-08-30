fn angular_reference_prediction(
    main: &[VvcSample],
    side: &[VvcSample],
    top_left: VvcSample,
    reference_index: i32,
    fract: i32,
    abs_inv_angle: i32,
    side_size: usize,
    reference_line: usize,
    shifted_refs: VvcShiftedAngularReferences,
    interpolation: VvcAngularInterpolation,
    bit_depth: SampleBitDepth,
) -> VvcSample {
    if fract == 0 {
        return angular_reference_sample(
            main,
            side,
            top_left,
            reference_index,
            abs_inv_angle,
            side_size,
            reference_line,
            shifted_refs,
        );
    }
    match interpolation {
        VvcAngularInterpolation::Linear => {
            let a = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            let b = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index + 1,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            (a + ((fract * (b - a) + 16) >> 5)) as VvcSample
        }
        VvcAngularInterpolation::FourTapDct | VvcAngularInterpolation::FourTapSmoothing => {
            let filter = match interpolation {
                VvcAngularInterpolation::FourTapDct => {
                    VVC_CHROMA_4TAP_INTERPOLATION_FILTER[fract as usize]
                }
                VvcAngularInterpolation::FourTapSmoothing => [
                    16 - (fract >> 1),
                    32 - (fract >> 1),
                    16 + (fract >> 1),
                    fract >> 1,
                ],
                VvcAngularInterpolation::Linear => unreachable!(),
            };
            let p0 = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index - 1,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            let p1 = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            let p2 = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index + 1,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            let p3 = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index + 2,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            ((filter[0] * p0 + filter[1] * p1 + filter[2] * p2 + filter[3] * p3 + 32) >> 6)
                .clamp(0, i32::from(bit_depth.max_sample())) as VvcSample
        }
    }
}

fn angular_reference_sample(
    main: &[VvcSample],
    side: &[VvcSample],
    _top_left: VvcSample,
    index: i32,
    abs_inv_angle: i32,
    side_size: usize,
    reference_line: usize,
    shifted_refs: VvcShiftedAngularReferences,
) -> VvcSample {
    if index == 0 {
        return shifted_refs.main_zero;
    }
    if index > 0 {
        return main[(index as usize - 1).min(main.len().saturating_sub(1))];
    }
    if index >= -(shifted_refs.main_prefix_len as i32) {
        let prefix_idx = (index + shifted_refs.main_prefix_len as i32) as usize;
        return shifted_refs.main_prefix[prefix_idx];
    }

    let old_main_index = index + reference_line as i32;
    let old_side_index =
        (((-old_main_index * abs_inv_angle + 256) >> 9).max(0) as usize).min(side_size);
    match old_side_index.checked_sub(reference_line) {
        Some(0) => shifted_refs.side_zero,
        Some(new_side_index) => side[(new_side_index - 1).min(side.len().saturating_sub(1))],
        None => shifted_refs.side_zero,
    }
}
