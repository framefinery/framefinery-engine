pub(in crate::vvc) struct VvcFrameCtuCabacState {
    contexts: VvcCabacContexts,
    luma_neighbours: VvcLumaNeighbourState,
    luma_mode_neighbours: VvcLumaModeNeighbourState,
    chroma_neighbours: VvcChromaNeighbourState,
    inter_skip_neighbours: VvcInterSkipNeighbourState,
    inter_motion_neighbours: VvcInterMotionNeighbourState,
    skip_neighbours: Vec<bool>,
    inter_slice: bool,
    picture_width: u16,
    picture_height: u16,
    ctu_cols: usize,
}

impl VvcFrameCtuCabacState {
    pub(in crate::vvc) fn new(
        picture_geometry: VvcVideoGeometry,
        slice_config: VvcSliceSyntaxConfig,
        inter_slice: bool,
    ) -> Self {
        let picture_width = picture_geometry.coded_width() as u16;
        let picture_height = picture_geometry.coded_height() as u16;
        let init_type = if inter_slice {
            VvcCabacInitType::P
        } else {
            VvcCabacInitType::I
        };
        Self {
            contexts: initial_vvc_cabac_contexts_for_init_type(slice_config, init_type),
            luma_neighbours: VvcLumaNeighbourState::new(picture_width, picture_height),
            luma_mode_neighbours: VvcLumaModeNeighbourState::new(picture_width, picture_height),
            chroma_neighbours: VvcChromaNeighbourState::new(
                picture_width,
                picture_height,
                slice_config.coding_tree.chroma_sampling,
            ),
            inter_skip_neighbours: VvcInterSkipNeighbourState::new(picture_width, picture_height),
            inter_motion_neighbours: VvcInterMotionNeighbourState::new(
                picture_width,
                picture_height,
            ),
            skip_neighbours: vec![
                false;
                picture_geometry.coded_width().div_ceil(VVC_CTU_SIZE)
                    * picture_geometry.coded_height().div_ceil(VVC_CTU_SIZE)
            ],
            inter_slice,
            picture_width,
            picture_height,
            ctu_cols: picture_geometry.coded_width().div_ceil(VVC_CTU_SIZE),
        }
    }

    pub(in crate::vvc) fn encode_ctu(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        slice_address: usize,
        params: &VvcCtuPartitionParams,
        slice_config: VvcSliceSyntaxConfig,
    ) {
        let ctu_x = slice_address % self.ctu_cols;
        let ctu_y = slice_address / self.ctu_cols;
        let origin_x = (ctu_x * VVC_CTU_SIZE) as u16;
        let origin_y = (ctu_y * VVC_CTU_SIZE) as u16;
        let skip_ctx = self.skip_ctx(slice_address);
        let luma_neighbours = &mut self.luma_neighbours;
        let luma_mode_neighbours = &mut self.luma_mode_neighbours;
        let chroma_neighbours = &mut self.chroma_neighbours;
        let inter_motion_neighbours = &mut self.inter_motion_neighbours;
        let shape = if self.inter_slice {
            params.single_tree_shape()
        } else {
            params.shape()
        };
        let mut ctu_encoder = VvcCtuCabacGenerator::new(&mut self.contexts, params, slice_config)
            .with_inter_slice(
                self.inter_slice,
                skip_ctx,
                Some(&mut self.inter_skip_neighbours),
                Some(inter_motion_neighbours),
            );
        if self.inter_slice {
            VvcCtuCabacOp::visit_inter_skip_ctu_partition_with_luma_neighbours(
                luma_neighbours,
                shape,
                origin_x,
                origin_y,
                self.picture_width,
                self.picture_height,
                params.luma_max_leaf_size,
                |op| {
                    ctu_encoder.emit_with_frame_neighbours(
                        cabac,
                        op,
                        luma_mode_neighbours,
                        chroma_neighbours,
                    );
                },
            );
        } else {
            VvcCtuCabacOp::visit_intra_ctu_partition_with_luma_neighbours(
                luma_neighbours,
                shape,
                origin_x,
                origin_y,
                self.picture_width,
                self.picture_height,
                params.luma_max_leaf_size,
                |op| {
                    ctu_encoder.emit_with_frame_neighbours(
                        cabac,
                        op,
                        luma_mode_neighbours,
                        chroma_neighbours,
                    );
                },
            );
        }
        if slice_address < self.skip_neighbours.len() {
            self.skip_neighbours[slice_address] = false;
        }
    }

    pub(in crate::vvc) fn encode_inter_skip_ctu(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        slice_address: usize,
        ctu_geometry: VvcVideoGeometry,
        slice_config: VvcSliceSyntaxConfig,
    ) {
        let ctu_x = slice_address % self.ctu_cols;
        let ctu_y = slice_address / self.ctu_cols;
        encode_inter_skip_ctu_body_with_frame_contexts(
            cabac,
            &mut self.contexts,
            ctu_geometry,
            slice_config,
            &mut self.luma_neighbours,
            &mut self.inter_skip_neighbours,
            &mut self.inter_motion_neighbours,
            (ctu_x * VVC_CTU_SIZE) as u16,
            (ctu_y * VVC_CTU_SIZE) as u16,
            self.picture_width,
            self.picture_height,
        );
        if slice_address < self.skip_neighbours.len() {
            self.skip_neighbours[slice_address] = true;
        }
    }

    fn skip_ctx(&self, slice_address: usize) -> u8 {
        let left = slice_address % self.ctu_cols != 0
            && self
                .skip_neighbours
                .get(slice_address - 1)
                .copied()
                .unwrap_or(false);
        let above = slice_address >= self.ctu_cols
            && self
                .skip_neighbours
                .get(slice_address - self.ctu_cols)
                .copied()
                .unwrap_or(false);
        u8::from(left) + u8::from(above)
    }
}
