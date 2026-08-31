pub(in crate::vvc) fn predict_vvc_luma_bdpcm_block_into_with_availability(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcIntraPredictionScratch,
    mode: VvcBdpcmMode,
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    predict_vvc_bdpcm_block_into(
        prediction,
        scratch,
        mode,
        luma,
        geometry.width,
        geometry.height,
        usize::from(node.x),
        usize::from(node.y),
        usize::from(node.width),
        usize::from(node.height),
        bit_depth,
        availability,
    );
}

pub(in crate::vvc) fn residual_vvc_luma_bdpcm_block_into_with_availability(
    residuals: &mut Vec<i16>,
    scratch: &mut VvcIntraPredictionScratch,
    mode: VvcBdpcmMode,
    source_luma: &[VvcSample],
    reference_luma: &[VvcSample],
    source_geometry: VvcVideoGeometry,
    reference_geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    let start_x = usize::from(node.x);
    let start_y = usize::from(node.y);
    let width = usize::from(node.width);
    let height = usize::from(node.height);
    let max_source_x = source_geometry.width.saturating_sub(1);
    let max_source_y = source_geometry.height.saturating_sub(1);

    residuals.clear();
    residuals.resize(width * height, 0);
    match mode {
        VvcBdpcmMode::None => unreachable!("BDPCM residual requires an enabled direction"),
        VvcBdpcmMode::Horizontal => {
            left_references_into(
                &mut scratch.references.left[..height],
                reference_luma,
                reference_geometry.width,
                reference_geometry.height,
                start_x,
                start_y,
                height,
                bit_depth,
                0,
                availability,
            );
            for y in 0..height {
                let predictor = scratch.references.left[y];
                let dst = y * width;
                let src_y = (start_y + y).min(max_source_y);
                let src_row = src_y * source_geometry.width;
                for x in 0..width {
                    let src_x = (start_x + x).min(max_source_x);
                    residuals[dst + x] =
                        vvc_sample_delta_i16(source_luma[src_row + src_x], predictor);
                }
            }
        }
        VvcBdpcmMode::Vertical => {
            top_references_into(
                &mut scratch.references.top[..width],
                reference_luma,
                reference_geometry.width,
                reference_geometry.height,
                start_x,
                start_y,
                width,
                bit_depth,
                0,
                availability,
            );
            for y in 0..height {
                let dst = y * width;
                let src_y = (start_y + y).min(max_source_y);
                let src_row = src_y * source_geometry.width;
                for x in 0..width {
                    let src_x = (start_x + x).min(max_source_x);
                    residuals[dst + x] = vvc_sample_delta_i16(
                        source_luma[src_row + src_x],
                        scratch.references.top[x],
                    );
                }
            }
        }
    }
}

pub(in crate::vvc) fn predict_vvc_chroma_bdpcm_block_into_with_availability(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcIntraPredictionScratch,
    mode: VvcBdpcmMode,
    chroma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    let subsample_x = chroma_subsample_x(chroma_sampling);
    let subsample_y = chroma_subsample_y(chroma_sampling);
    predict_vvc_bdpcm_block_into(
        prediction,
        scratch,
        mode,
        chroma,
        geometry.width / subsample_x,
        geometry.height / subsample_y,
        usize::from(node.x) / subsample_x,
        usize::from(node.y) / subsample_y,
        usize::from(node.width) / subsample_x,
        usize::from(node.height) / subsample_y,
        bit_depth,
        availability,
    );
}

fn predict_vvc_bdpcm_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcIntraPredictionScratch,
    mode: VvcBdpcmMode,
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    debug_assert!(mode.is_enabled());
    prediction.clear();
    prediction.resize(width * height, 0);
    match mode {
        VvcBdpcmMode::None => unreachable!("BDPCM predictor requires an enabled direction"),
        VvcBdpcmMode::Horizontal => {
            left_references_into(
                &mut scratch.references.left[..height],
                plane,
                plane_width,
                plane_height,
                start_x,
                start_y,
                height,
                bit_depth,
                0,
                availability,
            );
            for y in 0..height {
                prediction[y * width..(y + 1) * width].fill(scratch.references.left[y]);
            }
        }
        VvcBdpcmMode::Vertical => {
            top_references_into(
                &mut scratch.references.top[..width],
                plane,
                plane_width,
                plane_height,
                start_x,
                start_y,
                width,
                bit_depth,
                0,
                availability,
            );
            for row in prediction.chunks_exact_mut(width) {
                row.copy_from_slice(&scratch.references.top[..width]);
            }
        }
    }
}
