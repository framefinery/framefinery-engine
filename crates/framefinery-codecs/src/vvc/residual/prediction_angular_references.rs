fn angular_main_zero_reference(
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    bit_depth: SampleBitDepth,
    reference_line: usize,
    is_vertical: bool,
    availability: Option<VvcPlaneAvailability<'_>>,
) -> VvcSample {
    let coordinate = if is_vertical {
        start_x
            .checked_sub(1)
            .zip(start_y.checked_sub(1 + reference_line))
    } else {
        start_x
            .checked_sub(1 + reference_line)
            .zip(start_y.checked_sub(1))
    };
    if let Some((x, y)) = coordinate {
        if x < plane_width && y < plane_height && reference_sample_available(availability, x, y) {
            return plane[y * plane_width + x];
        }
    }
    top_left_reference(
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        bit_depth,
        reference_line,
        availability,
    )
}

fn angular_shifted_reference_samples(
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    _bit_depth: SampleBitDepth,
    reference_line: usize,
    is_vertical: bool,
    main_zero: VvcSample,
    availability: Option<VvcPlaneAvailability<'_>>,
) -> VvcShiftedAngularReferences {
    let reference_line = reference_line.min(VVC_MAX_MULTI_REF_LINE_IDX);
    let mut main_prefix = [main_zero; VVC_MAX_MULTI_REF_LINE_IDX];
    if is_vertical {
        let row_y = start_y.checked_sub(1 + reference_line);
        for (idx, dst) in main_prefix.iter_mut().take(reference_line).enumerate() {
            let x = start_x.checked_sub(reference_line - idx + 1);
            *dst = reference_sample_or(
                plane,
                plane_width,
                plane_height,
                x,
                row_y,
                main_zero,
                availability,
            );
        }
        let side_zero = reference_sample_or(
            plane,
            plane_width,
            plane_height,
            start_x.checked_sub(1 + reference_line),
            start_y.checked_sub(1),
            main_zero,
            availability,
        );
        VvcShiftedAngularReferences {
            main_zero,
            side_zero,
            main_prefix,
            main_prefix_len: reference_line,
        }
    } else {
        let col_x = start_x.checked_sub(1 + reference_line);
        for (idx, dst) in main_prefix.iter_mut().take(reference_line).enumerate() {
            let y = start_y.checked_sub(reference_line - idx + 1);
            *dst = reference_sample_or(
                plane,
                plane_width,
                plane_height,
                col_x,
                y,
                main_zero,
                availability,
            );
        }
        let side_zero = reference_sample_or(
            plane,
            plane_width,
            plane_height,
            start_x.checked_sub(1),
            start_y.checked_sub(1 + reference_line),
            main_zero,
            availability,
        );
        VvcShiftedAngularReferences {
            main_zero,
            side_zero,
            main_prefix,
            main_prefix_len: reference_line,
        }
    }
}
