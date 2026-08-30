#[derive(Clone, Copy)]
pub(in crate::vvc) struct VvcPlaneAvailability<'a> {
    samples: &'a [bool],
    stride: usize,
}

impl<'a> VvcPlaneAvailability<'a> {
    pub(in crate::vvc) const fn new(samples: &'a [bool], stride: usize) -> Self {
        Self { samples, stride }
    }

    fn is_available(self, x: usize, y: usize) -> bool {
        self.samples
            .get(y.saturating_mul(self.stride).saturating_add(x))
            .copied()
            .unwrap_or(false)
    }
}

fn reference_sample_or(
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    x: Option<usize>,
    y: Option<usize>,
    fallback: VvcSample,
    availability: Option<VvcPlaneAvailability<'_>>,
) -> VvcSample {
    let Some((x, y)) = x.zip(y) else {
        return fallback;
    };
    if x < plane_width && y < plane_height && reference_sample_available(availability, x, y) {
        plane[y * plane_width + x]
    } else {
        fallback
    }
}

fn top_left_reference(
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    bit_depth: SampleBitDepth,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) -> VvcSample {
    if start_x > reference_line
        && start_y > reference_line
        && reference_sample_available(
            availability,
            start_x - 1 - reference_line,
            start_y - 1 - reference_line,
        )
    {
        return plane[(start_y - 1 - reference_line) * plane_width + start_x - 1 - reference_line];
    }
    if start_y > reference_line
        && start_x < plane_width
        && reference_sample_available(availability, start_x, start_y - 1 - reference_line)
    {
        return plane[(start_y - 1 - reference_line) * plane_width + start_x];
    }
    if start_x > reference_line
        && start_y < plane_height
        && reference_sample_available(availability, start_x - 1 - reference_line, start_y)
    {
        return plane[start_y * plane_width + start_x - 1 - reference_line];
    }
    vvc_neutral_sample(bit_depth)
}

fn reference_sample_available(
    availability: Option<VvcPlaneAvailability<'_>>,
    x: usize,
    y: usize,
) -> bool {
    availability.map_or(true, |availability| availability.is_available(x, y))
}
