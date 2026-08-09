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
        let coded_geometry = geometry.coded();
        let layout = PlanarYuvGeometry::for_validated_shape(
            coded_geometry.width,
            coded_geometry.height,
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
        VvcPlaneAvailability::new(&self.luma_available, self.luma_width())
    }

    fn cb_availability(&self) -> VvcPlaneAvailability<'_> {
        VvcPlaneAvailability::new(&self.cb_available, self.chroma_width())
    }

    fn cr_availability(&self) -> VvcPlaneAvailability<'_> {
        VvcPlaneAvailability::new(&self.cr_available, self.chroma_width())
    }

    fn mark_luma_node_available(&mut self, node: VvcCodingTreeNode) {
        let luma_width = self.luma_width();
        let luma_height = self.luma_height();
        mark_vvc_plane_node_available(
            &mut self.luma_available,
            luma_width,
            luma_height,
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
        let luma_width = self.luma_width();
        let luma_height = self.luma_height();
        let width = region
            .geometry
            .width
            .min(luma_width.saturating_sub(region.origin_x));
        let height = region
            .geometry
            .height
            .min(luma_height.saturating_sub(region.origin_y));
        copy_vvc_plane_region(
            &mut self.luma,
            &previous.luma,
            luma_width,
            region.origin_x,
            region.origin_y,
            width,
            height,
        );
        mark_vvc_plane_node_available(
            &mut self.luma_available,
            luma_width,
            luma_height,
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

    fn coded_geometry(&self) -> VvcVideoGeometry {
        VvcVideoGeometry {
            width: self.luma_width(),
            height: self.luma_height(),
        }
    }

    fn luma_width(&self) -> usize {
        self.geometry.coded_width()
    }

    fn luma_height(&self) -> usize {
        self.geometry.coded_height()
    }

    fn chroma_width(&self) -> usize {
        self.luma_width() / chroma_subsample_x(self.format.chroma_sampling)
    }

    fn chroma_height(&self) -> usize {
        self.luma_height() / chroma_subsample_y(self.format.chroma_sampling)
    }

    fn visible_chroma_width(&self) -> usize {
        self.geometry.width / chroma_subsample_x(self.format.chroma_sampling)
    }

    fn visible_chroma_height(&self) -> usize {
        self.geometry.height / chroma_subsample_y(self.format.chroma_sampling)
    }

    fn to_sample_yuv(&self) -> Vec<VvcSample> {
        let mut output = Vec::with_capacity(
            self.geometry.luma_samples() + self.visible_chroma_width() * self.visible_chroma_height() * 2,
        );
        append_visible_plane_samples(
            &mut output,
            &self.luma,
            self.luma_width(),
            self.geometry.width,
            self.geometry.height,
        );
        append_visible_plane_samples(
            &mut output,
            &self.cb,
            self.chroma_width(),
            self.visible_chroma_width(),
            self.visible_chroma_height(),
        );
        append_visible_plane_samples(
            &mut output,
            &self.cr,
            self.chroma_width(),
            self.visible_chroma_width(),
            self.visible_chroma_height(),
        );
        output
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
        let visible_samples = self.to_sample_yuv();
        let y_len = self.geometry.luma_samples();
        let chroma_len = self.visible_chroma_width() * self.visible_chroma_height();
        pack_planar_samples(&visible_samples[..y_len], y_plane, self.format.bit_depth);
        pack_planar_samples(
            &visible_samples[y_len..y_len + chroma_len],
            cb_plane,
            self.format.bit_depth,
        );
        pack_planar_samples(
            &visible_samples[y_len + chroma_len..],
            cr_plane,
            self.format.bit_depth,
        );
        output
    }

}

fn append_visible_plane_samples(
    output: &mut Vec<VvcSample>,
    plane: &[VvcSample],
    stride: usize,
    visible_width: usize,
    visible_height: usize,
) {
    for y in 0..visible_height {
        let row = y * stride;
        output.extend_from_slice(&plane[row..row + visible_width]);
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
