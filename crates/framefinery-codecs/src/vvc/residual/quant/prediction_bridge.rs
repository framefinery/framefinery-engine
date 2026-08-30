fn vvc_global_ctu_node(mut node: VvcCodingTreeNode, region: VvcCtuRegion) -> VvcCodingTreeNode {
    node.x += region.origin_x as u16;
    node.y += region.origin_y as u16;
    node
}

fn predict_vvc_chroma_mode_pair_blocks_into_with_availability(
    cb_prediction: &mut Vec<VvcSample>,
    cr_prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcChromaIntraPredictionMode,
    co_located_luma_mode: VvcIntraPredictionMode,
    cb: &[VvcSample],
    cr: &[VvcSample],
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    cb_availability: Option<super::VvcPlaneAvailability<'_>>,
    cr_availability: Option<super::VvcPlaneAvailability<'_>>,
    luma_availability: Option<super::VvcPlaneAvailability<'_>>,
) {
    if let VvcChromaIntraPredictionMode::Cclm(cclm_mode) = mode {
        predict_vvc_chroma_cclm_pair_into_with_availability(
            cb_prediction,
            cr_prediction,
            scratch,
            cclm_mode,
            cb,
            cr,
            luma,
            geometry,
            node,
            chroma_sampling,
            bit_depth,
            cb_availability,
            cr_availability,
            luma_availability,
        );
        return;
    }
    predict_vvc_chroma_mode_block_into_with_availability(
        cb_prediction,
        scratch,
        mode,
        co_located_luma_mode,
        cb,
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        cb_availability,
        luma_availability,
    );
    predict_vvc_chroma_mode_block_into_with_availability(
        cr_prediction,
        scratch,
        mode,
        co_located_luma_mode,
        cr,
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        cr_availability,
        luma_availability,
    );
}
