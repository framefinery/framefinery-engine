fn filter_vvc_angular_references_in_place(
    top: &mut [VvcSample],
    left: &mut [VvcSample],
    top_left: VvcSample,
    top_ref_len: usize,
    left_ref_len: usize,
) -> VvcSample {
    let filtered_top_left =
        ((u32::from(top_left) * 2 + u32::from(top[0]) + u32::from(left[0]) + 2) >> 2) as VvcSample;

    let mut previous = top_left;
    for index in 0..top_ref_len.saturating_sub(1) {
        let current = top[index];
        top[index] =
            ((u32::from(previous) + u32::from(current) * 2 + u32::from(top[index + 1]) + 2) >> 2)
                as VvcSample;
        previous = current;
    }

    previous = top_left;
    for index in 0..left_ref_len.saturating_sub(1) {
        let current = left[index];
        left[index] =
            ((u32::from(previous) + u32::from(current) * 2 + u32::from(left[index + 1]) + 2) >> 2)
                as VvcSample;
        previous = current;
    }

    filtered_top_left
}

fn filter_vvc_planar_references_in_place(
    top: &mut [VvcSample],
    left: &mut [VvcSample],
    top_left: VvcSample,
    width: usize,
    height: usize,
) {
    let mut previous = top_left;
    for index in 0..=width {
        let current = top[index];
        top[index] =
            ((u32::from(previous) + u32::from(current) * 2 + u32::from(top[index + 1]) + 2) >> 2)
                as VvcSample;
        previous = current;
    }

    previous = top_left;
    for index in 0..=height {
        let current = left[index];
        left[index] =
            ((u32::from(previous) + u32::from(current) * 2 + u32::from(left[index + 1]) + 2) >> 2)
                as VvcSample;
        previous = current;
    }
}

fn dc_prediction_value(
    top: &[VvcSample],
    left: &[VvcSample],
    width: usize,
    height: usize,
) -> VvcSample {
    let mut sum = 0u64;
    if width >= height {
        sum += top.iter().map(|sample| u64::from(*sample)).sum::<u64>();
    }
    if width <= height {
        sum += left.iter().map(|sample| u64::from(*sample)).sum::<u64>();
    }
    let denom = if width == height {
        width << 1
    } else {
        width.max(height)
    } as u64;
    ((sum + (denom >> 1)) >> denom.ilog2()) as VvcSample
}
