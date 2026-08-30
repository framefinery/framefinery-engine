pub(in crate::vvc) fn fill_visible_luma_node(
    luma: &mut [VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    predicted: &[VvcSample],
    residuals: &[i16],
    bit_depth: SampleBitDepth,
) {
    fill_visible_plane_node(
        luma,
        geometry.width,
        geometry.height,
        usize::from(node.x),
        usize::from(node.y),
        usize::from(node.width),
        usize::from(node.height),
        predicted,
        residuals,
        bit_depth,
    );
}

pub(in crate::vvc) fn fill_visible_chroma_node(
    chroma: &mut [VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    predicted: &[VvcSample],
    residuals: &[i16],
    bit_depth: SampleBitDepth,
) {
    let subsample_x = chroma_subsample_x(chroma_sampling);
    let subsample_y = chroma_subsample_y(chroma_sampling);
    let node_width = usize::from(node.width) / subsample_x;
    let node_height = usize::from(node.height) / subsample_y;
    let start_x = usize::from(node.x) / subsample_x;
    let start_y = usize::from(node.y) / subsample_y;
    let chroma_width = geometry.width / subsample_x;
    let chroma_height = geometry.height / subsample_y;
    fill_visible_plane_node(
        chroma,
        chroma_width,
        chroma_height,
        start_x,
        start_y,
        node_width,
        node_height,
        predicted,
        residuals,
        bit_depth,
    );
}

fn fill_visible_plane_node(
    plane: &mut [VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    node_width: usize,
    node_height: usize,
    predicted: &[VvcSample],
    residuals: &[i16],
    bit_depth: SampleBitDepth,
) {
    let end_x = (start_x + node_width).min(plane_width);
    let end_y = (start_y + node_height).min(plane_height);
    let max_sample = i32::from(bit_depth.max_sample());
    for y in start_y..end_y {
        let row = y * plane_width;
        let src_y = y - start_y;
        for x in start_x..end_x {
            let src_x = x - start_x;
            let idx = src_y * node_width + src_x;
            plane[row + x] = (i32::from(predicted[idx]) + i32::from(residuals[idx]))
                .clamp(0, max_sample) as VvcSample;
        }
    }
}
