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

fn prepare_vvc_intra_reference_edges(
    scratch: &mut VvcIntraReferenceScratch,
    plane: &[VvcSample],
    region: VvcIntraPredictionRegion,
    top_len: usize,
    left_len: usize,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    top_references_into(
        &mut scratch.top,
        plane,
        region.plane_width,
        region.plane_height,
        region.x,
        region.y,
        top_len,
        bit_depth,
        region.reference_line,
        availability,
    );
    left_references_into(
        &mut scratch.left,
        plane,
        region.plane_width,
        region.plane_height,
        region.x,
        region.y,
        left_len,
        bit_depth,
        region.reference_line,
        availability,
    );
}

#[cfg(test)]
mod reference_edge_tests {
    use super::*;

    #[test]
    fn shared_reference_preparation_preserves_region_and_mrl() {
        let plane: Vec<VvcSample> = (0..64).map(|sample| sample as VvcSample).collect();
        let region = VvcIntraPredictionRegion {
            plane_width: 8,
            plane_height: 8,
            x: 3,
            y: 4,
            width: 2,
            height: 3,
            is_luma: true,
            reference_line: 1,
        };
        let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
        let mut scratch = VvcIntraPredictionScratch::default();

        prepare_vvc_intra_reference_edges(
            &mut scratch.references,
            &plane,
            region,
            5,
            6,
            bit_depth,
            None,
        );

        assert_eq!(&scratch.references.top[..5], &[19, 20, 21, 22, 23]);
        assert_eq!(&scratch.references.left[..6], &[33, 41, 49, 57, 57, 57]);
    }

    #[test]
    fn bdpcm_prepares_only_the_selected_reference_edge() {
        let plane: Vec<VvcSample> = (0..64).map(|sample| sample as VvcSample).collect();
        let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
        let mut scratch = VvcIntraPredictionScratch::default();
        let mut prediction = Vec::new();

        scratch.references.top.fill(1_000);
        predict_vvc_bdpcm_block_into(
            &mut prediction,
            &mut scratch,
            VvcBdpcmMode::Horizontal,
            &plane,
            8,
            8,
            2,
            2,
            2,
            2,
            bit_depth,
            None,
        );
        assert_eq!(prediction, [17, 17, 25, 25]);
        assert!(scratch.references.top.iter().all(|sample| *sample == 1_000));

        scratch.references.left.fill(1_001);
        predict_vvc_bdpcm_block_into(
            &mut prediction,
            &mut scratch,
            VvcBdpcmMode::Vertical,
            &plane,
            8,
            8,
            2,
            2,
            2,
            2,
            bit_depth,
            None,
        );
        assert_eq!(prediction, [10, 11, 10, 11]);
        assert!(scratch
            .references
            .left
            .iter()
            .all(|sample| *sample == 1_001));
    }
}
