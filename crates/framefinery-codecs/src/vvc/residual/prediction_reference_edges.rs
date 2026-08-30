fn top_references_into(
    out: &mut [VvcSample],
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    bit_depth: SampleBitDepth,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    debug_assert!(out.len() >= width);
    let fallback = if start_x > reference_line
        && start_y < plane_height
        && reference_sample_available(availability, start_x - 1 - reference_line, start_y)
    {
        plane[start_y * plane_width + start_x - 1 - reference_line]
    } else {
        vvc_neutral_sample(bit_depth)
    };
    if start_y <= reference_line {
        out[..width].fill(fallback);
        return;
    }

    let row_y = start_y - 1 - reference_line;
    let mut first_available = None;
    let mut last_sample = fallback;
    for (x, dst) in out.iter_mut().take(width).enumerate() {
        let src_x = (start_x + x).min(plane_width.saturating_sub(1));
        if reference_sample_available(availability, src_x, row_y) {
            last_sample = plane[row_y * plane_width + src_x];
            if first_available.is_none() {
                first_available = Some((x, last_sample));
            }
        }
        *dst = last_sample;
    }
    if let Some((first_index, first_sample)) = first_available {
        out[..first_index].fill(first_sample);
    }
}

fn left_references_into(
    out: &mut [VvcSample],
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    debug_assert!(out.len() >= height);
    let fallback = if start_y > reference_line
        && start_x < plane_width
        && reference_sample_available(availability, start_x, start_y - 1 - reference_line)
    {
        plane[(start_y - 1 - reference_line) * plane_width + start_x]
    } else {
        vvc_neutral_sample(bit_depth)
    };
    if start_x <= reference_line {
        out[..height].fill(fallback);
        return;
    }

    let col_x = start_x - 1 - reference_line;
    let mut first_available = None;
    let mut last_sample = fallback;
    for (y, dst) in out.iter_mut().take(height).enumerate() {
        let src_y = (start_y + y).min(plane_height.saturating_sub(1));
        if reference_sample_available(availability, col_x, src_y) {
            last_sample = plane[src_y * plane_width + col_x];
            if first_available.is_none() {
                first_available = Some((y, last_sample));
            }
        }
        *dst = last_sample;
    }
    if let Some((first_index, first_sample)) = first_available {
        out[..first_index].fill(first_sample);
    }
}
