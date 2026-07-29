#[derive(Debug, Clone, PartialEq, Eq)]
struct VvcReconstructionFrame {
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    luma: Vec<VvcSample>,
    cb: Vec<VvcSample>,
    cr: Vec<VvcSample>,
    luma_available: Vec<bool>,
    cb_available: Vec<bool>,
    cr_available: Vec<bool>,
}

impl VvcReconstructionFrame {
    fn new_neutral(geometry: VvcVideoGeometry, format: VvcPictureFormat) -> Self {
        let layout = PlanarYuvGeometry::for_validated_shape(
            geometry.width,
            geometry.height,
            format.chroma_sampling,
            format.bit_depth,
        );
        let neutral = vvc_neutral_sample(format.bit_depth);
        Self {
            geometry,
            format,
            luma: vec![neutral; layout.luma_samples()],
            cb: vec![neutral; layout.chroma_samples()],
            cr: vec![neutral; layout.chroma_samples()],
            luma_available: vec![false; layout.luma_samples()],
            cb_available: vec![false; layout.chroma_samples()],
            cr_available: vec![false; layout.chroma_samples()],
        }
    }

    fn luma_availability(&self) -> VvcPlaneAvailability<'_> {
        VvcPlaneAvailability::new(&self.luma_available, self.geometry.width)
    }

    fn cb_availability(&self) -> VvcPlaneAvailability<'_> {
        VvcPlaneAvailability::new(&self.cb_available, self.chroma_width())
    }

    fn cr_availability(&self) -> VvcPlaneAvailability<'_> {
        VvcPlaneAvailability::new(&self.cr_available, self.chroma_width())
    }

    fn mark_luma_node_available(&mut self, node: VvcCodingTreeNode) {
        mark_vvc_plane_node_available(
            &mut self.luma_available,
            self.geometry.width,
            self.geometry.height,
            usize::from(node.x),
            usize::from(node.y),
            usize::from(node.width),
            usize::from(node.height),
        );
    }

    fn mark_chroma_node_available(&mut self, node: VvcCodingTreeNode) {
        let subsample_x = chroma_subsample_x(self.format.chroma_sampling);
        let subsample_y = chroma_subsample_y(self.format.chroma_sampling);
        let chroma_width = self.chroma_width();
        let chroma_height = self.chroma_height();
        let x = usize::from(node.x) / subsample_x;
        let y = usize::from(node.y) / subsample_y;
        let width = usize::from(node.width) / subsample_x;
        let height = usize::from(node.height) / subsample_y;
        mark_vvc_plane_node_available(
            &mut self.cb_available,
            chroma_width,
            chroma_height,
            x,
            y,
            width,
            height,
        );
        mark_vvc_plane_node_available(
            &mut self.cr_available,
            chroma_width,
            chroma_height,
            x,
            y,
            width,
            height,
        );
    }

    fn copy_ctu_from(&mut self, previous: &Self, region: VvcCtuRegion) {
        debug_assert_eq!(self.geometry, previous.geometry);
        debug_assert_eq!(self.format, previous.format);
        let width = region
            .geometry
            .width
            .min(self.geometry.width.saturating_sub(region.origin_x));
        let height = region
            .geometry
            .height
            .min(self.geometry.height.saturating_sub(region.origin_y));
        copy_vvc_plane_region(
            &mut self.luma,
            &previous.luma,
            self.geometry.width,
            region.origin_x,
            region.origin_y,
            width,
            height,
        );
        mark_vvc_plane_node_available(
            &mut self.luma_available,
            self.geometry.width,
            self.geometry.height,
            region.origin_x,
            region.origin_y,
            width,
            height,
        );

        let subsample_x = chroma_subsample_x(self.format.chroma_sampling);
        let subsample_y = chroma_subsample_y(self.format.chroma_sampling);
        let chroma_width = self.chroma_width();
        let chroma_height = self.chroma_height();
        let chroma_x = region.origin_x / subsample_x;
        let chroma_y = region.origin_y / subsample_y;
        let chroma_copy_width = width / subsample_x;
        let chroma_copy_height = height / subsample_y;
        copy_vvc_plane_region(
            &mut self.cb,
            &previous.cb,
            chroma_width,
            chroma_x,
            chroma_y,
            chroma_copy_width,
            chroma_copy_height,
        );
        copy_vvc_plane_region(
            &mut self.cr,
            &previous.cr,
            chroma_width,
            chroma_x,
            chroma_y,
            chroma_copy_width,
            chroma_copy_height,
        );
        mark_vvc_plane_node_available(
            &mut self.cb_available,
            chroma_width,
            chroma_height,
            chroma_x,
            chroma_y,
            chroma_copy_width,
            chroma_copy_height,
        );
        mark_vvc_plane_node_available(
            &mut self.cr_available,
            chroma_width,
            chroma_height,
            chroma_x,
            chroma_y,
            chroma_copy_width,
            chroma_copy_height,
        );
    }

    fn chroma_width(&self) -> usize {
        self.geometry.width / chroma_subsample_x(self.format.chroma_sampling)
    }

    fn chroma_height(&self) -> usize {
        self.geometry.height / chroma_subsample_y(self.format.chroma_sampling)
    }

    fn to_yuv(&self) -> Vec<u8> {
        let layout = PlanarYuvFrameLayout::for_validated_shape(
            self.geometry.width,
            self.geometry.height,
            self.format.chroma_sampling,
            self.format.bit_depth,
        );
        let mut output = vec![0; layout.frame_len()];
        let (y_plane, cb_plane, cr_plane) = layout.plane_slices_mut(&mut output);
        pack_planar_samples(&self.luma, y_plane, self.format.bit_depth);
        pack_planar_samples(&self.cb, cb_plane, self.format.bit_depth);
        pack_planar_samples(&self.cr, cr_plane, self.format.bit_depth);
        output
    }

}

fn copy_vvc_plane_region(
    dst: &mut [VvcSample],
    src: &[VvcSample],
    stride: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
) {
    for y in start_y..start_y + height {
        let row = y * stride;
        let start = row + start_x;
        let end = start + width;
        dst[start..end].copy_from_slice(&src[start..end]);
    }
}

fn mark_vvc_plane_node_available(
    available: &mut [bool],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
) {
    let end_x = (start_x + width).min(plane_width);
    let end_y = (start_y + height).min(plane_height);
    for y in start_y..end_y {
        let row = y * plane_width;
        for x in start_x..end_x {
            available[row + x] = true;
        }
    }
}
