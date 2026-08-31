fn predict_vvc_chroma_cclm_block_into_with_availability(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcCclmPredictionScratch,
    mode: VvcChromaCclmMode,
    chroma: &[VvcSample],
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    chroma_availability: Option<VvcPlaneAvailability<'_>>,
    luma_availability: Option<VvcPlaneAvailability<'_>>,
) {
    predict_vvc_chroma_cclm_block_with_luma_scratch_into(
        prediction,
        &mut scratch.inner_luma,
        mode,
        chroma,
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        chroma_availability,
        luma_availability,
    );
}

pub(in crate::vvc) fn predict_vvc_chroma_cclm_pair_into_with_availability(
    cb_prediction: &mut Vec<VvcSample>,
    cr_prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcChromaCclmMode,
    cb: &[VvcSample],
    cr: &[VvcSample],
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    cb_availability: Option<VvcPlaneAvailability<'_>>,
    cr_availability: Option<VvcPlaneAvailability<'_>>,
    luma_availability: Option<VvcPlaneAvailability<'_>>,
) {
    let cb_layout = vvc_cclm_layout(mode, cb_availability, geometry, node, chroma_sampling);
    let cr_layout = vvc_cclm_layout(mode, cr_availability, geometry, node, chroma_sampling);
    if cb_layout != cr_layout {
        predict_vvc_chroma_cclm_block_with_luma_scratch_into(
            cb_prediction,
            &mut scratch.cclm.inner_luma,
            mode,
            cb,
            luma,
            geometry,
            node,
            chroma_sampling,
            bit_depth,
            cb_availability,
            luma_availability,
        );
        predict_vvc_chroma_cclm_block_with_luma_scratch_into(
            cr_prediction,
            &mut scratch.cclm.inner_luma,
            mode,
            cr,
            luma,
            geometry,
            node,
            chroma_sampling,
            bit_depth,
            cr_availability,
            luma_availability,
        );
        return;
    }

    prepare_vvc_cclm_inner_luma_into(
        &mut scratch.cclm.inner_luma,
        cb_layout,
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        luma_availability,
    );
    let luma_selection = derive_vvc_cclm_luma_selection(
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        luma_availability,
        cb_layout,
    );
    predict_vvc_chroma_cclm_block_from_inner_luma_into(
        cb_prediction,
        &scratch.cclm.inner_luma,
        cb_layout,
        luma_selection,
        cb,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        cb_availability,
    );
    predict_vvc_chroma_cclm_block_from_inner_luma_into(
        cr_prediction,
        &scratch.cclm.inner_luma,
        cb_layout,
        luma_selection,
        cr,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        cr_availability,
    );
}

fn predict_vvc_chroma_cclm_block_with_luma_scratch_into(
    prediction: &mut Vec<VvcSample>,
    inner_luma: &mut Vec<i32>,
    mode: VvcChromaCclmMode,
    chroma: &[VvcSample],
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    chroma_availability: Option<VvcPlaneAvailability<'_>>,
    luma_availability: Option<VvcPlaneAvailability<'_>>,
) {
    let layout = vvc_cclm_layout(mode, chroma_availability, geometry, node, chroma_sampling);
    prepare_vvc_cclm_inner_luma_into(
        inner_luma,
        layout,
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        luma_availability,
    );
    let luma_selection = derive_vvc_cclm_luma_selection(
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        luma_availability,
        layout,
    );
    predict_vvc_chroma_cclm_block_from_inner_luma_into(
        prediction,
        inner_luma,
        layout,
        luma_selection,
        chroma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        chroma_availability,
    );
}

fn predict_vvc_chroma_cclm_block_from_inner_luma_into(
    prediction: &mut Vec<VvcSample>,
    inner_luma: &[i32],
    layout: VvcCclmLayout,
    luma_selection: VvcCclmLumaSelection,
    chroma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    chroma_availability: Option<VvcPlaneAvailability<'_>>,
) {
    let params = derive_vvc_cclm_parameters_from_selection(
        chroma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        chroma_availability,
        layout,
        luma_selection,
    );
    prediction.clear();
    prediction.resize(layout.width * layout.height, 0);
    let max_sample = i32::from(bit_depth.max_sample());
    for (dst, luma_sample) in prediction.iter_mut().zip(inner_luma.iter().copied()) {
        let predicted = right_shift_i32(params.a * luma_sample, params.shift) + params.b;
        *dst = predicted.clamp(0, max_sample) as VvcSample;
    }
}

fn prepare_vvc_cclm_inner_luma_into(
    inner_luma: &mut Vec<i32>,
    layout: VvcCclmLayout,
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    luma_availability: Option<VvcPlaneAvailability<'_>>,
) {
    inner_luma.clear();
    inner_luma.reserve(layout.width * layout.height);
    for y in 0..layout.height {
        for x in 0..layout.width {
            inner_luma.push(cclm_downsample_inner_luma(
                luma,
                geometry,
                node,
                chroma_sampling,
                bit_depth,
                luma_availability,
                x,
                y,
                layout.template.downsample_left_available,
            ));
        }
    }
}

fn vvc_cclm_layout(
    mode: VvcChromaCclmMode,
    chroma_availability: Option<VvcPlaneAvailability<'_>>,
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
) -> VvcCclmLayout {
    let subsample_x = chroma_subsample_x(chroma_sampling);
    let subsample_y = chroma_subsample_y(chroma_sampling);
    let chroma_width = usize::from(node.width) / subsample_x;
    let chroma_height = usize::from(node.height) / subsample_y;
    let chroma_x = usize::from(node.x) / subsample_x;
    let chroma_y = usize::from(node.y) / subsample_y;
    let plane_width = geometry.width / subsample_x;
    let plane_height = geometry.height / subsample_y;
    let template = vvc_cclm_template(
        mode,
        chroma_availability,
        plane_width,
        plane_height,
        chroma_x,
        chroma_y,
        chroma_width,
        chroma_height,
        chroma_sampling,
    );
    VvcCclmLayout {
        width: chroma_width,
        height: chroma_height,
        template,
    }
}
