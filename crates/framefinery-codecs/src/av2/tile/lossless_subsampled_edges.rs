impl Av2LosslessSubsampledTileState<'_> {
    fn luma_directional_idif_edges_with<EdgeSample>(
        &self,
        x0: usize,
        y0: usize,
        angle: i16,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
        edge_sample: &EdgeSample,
    ) -> DirectionalIdifEdges
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
    {
        let above_core = self.directional_above_edge_with(
            Av2LosslessPlane::Y,
            x0,
            y0,
            leaf_x0,
            leaf_y0,
            leaf_width,
            coded_mi_context,
            edge_sample,
        );
        let left_core = self.directional_left_edge_with(
            Av2LosslessPlane::Y,
            x0,
            y0,
            leaf_x0,
            leaf_y0,
            leaf_height,
            coded_mi_context,
            edge_sample,
        );
        let above_left = self.above_left_predictor_with(Av2LosslessPlane::Y, x0, y0, edge_sample);
        assemble_directional_idif_edges(self.bit_depth, angle, above_core, left_core, above_left)
    }

    fn above_left_predictor_with<EdgeSample>(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        edge_sample: &EdgeSample,
    ) -> Av2Sample
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        av2_above_left_predictor_from_edges(
            self.bit_depth,
            tile_origin_x,
            tile_origin_y,
            x0,
            y0,
            |x, y| edge_sample(plane, x, y),
        )
    }

    fn directional_above_edge_with<EdgeSample>(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        coded_mi_context: &Av2CodedMiContext,
        edge_sample: &EdgeSample,
    ) -> [Av2Sample; 8]
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let (plane_width, _) = self.plane_geometry(plane);
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        let have_top = y0 > tile_origin_y;
        let have_left = x0 > tile_origin_x;
        let mut above = [av2_lossless_v_pred_above_edge(self.bit_depth); 8];
        if have_top {
            let plane_sb_width = MVP_SUPERBLOCK_SIZE / sub_x;
            let plane_sb_height = MVP_SUPERBLOCK_SIZE / sub_y;
            let sb_origin_x = (x0 / plane_sb_width) * plane_sb_width;
            let sb_right = (sb_origin_x + plane_sb_width).min(plane_width);
            let superblock_top_row = y0 % plane_sb_height == 0;
            for index in 0..above.len() {
                let x = x0 + index;
                let overhang = index >= TX4X4_SIZE;
                let external_top_right_coded = overhang && y0 == leaf_y0 && x < plane_width && {
                    let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, x, y0 - 1);
                    superblock_top_row
                        || (x < sb_right && coded_mi_context.is_coded(row_mi, col_mi))
                };
                if x < plane_width
                    && (!overhang || x < leaf_x0 + leaf_width || external_top_right_coded)
                {
                    above[index] = edge_sample(plane, x, y0 - 1);
                } else if index > 0 {
                    above[index] = above[index - 1];
                }
            }
        } else if have_left {
            above.fill(edge_sample(plane, x0 - 1, y0));
        }
        above
    }

    fn directional_left_edge_with<EdgeSample>(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
        edge_sample: &EdgeSample,
    ) -> [Av2Sample; 8]
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let (_, plane_height) = self.plane_geometry(plane);
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        let have_top = y0 > tile_origin_y;
        let have_left = x0 > tile_origin_x;
        let mut left = [av2_lossless_h_pred_left_edge(self.bit_depth); 8];
        if have_left {
            let plane_sb_width = MVP_SUPERBLOCK_SIZE / sub_x;
            let plane_sb_height = MVP_SUPERBLOCK_SIZE / sub_y;
            let sb_origin_y = (y0 / plane_sb_height) * plane_sb_height;
            let sb_bottom = (sb_origin_y + plane_sb_height).min(plane_height);
            let superblock_left_col = x0 % plane_sb_width == 0;
            for index in 0..left.len() {
                let y = y0 + index;
                let overhang = index >= TX4X4_SIZE;
                let external_bottom_left_coded = overhang && x0 == leaf_x0 && y < sb_bottom && {
                    let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, x0 - 1, y);
                    superblock_left_col || coded_mi_context.is_coded(row_mi, col_mi)
                };
                if y < plane_height
                    && (!overhang
                        || (x0 == leaf_x0
                            && (y < leaf_y0 + leaf_height || external_bottom_left_coded)))
                {
                    left[index] = edge_sample(plane, x0 - 1, y);
                } else if index > 0 {
                    left[index] = left[index - 1];
                }
            }
        } else if have_top {
            left.fill(edge_sample(plane, x0, y0 - 1));
        }
        left
    }

    fn smooth_edges_with<EdgeSample>(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
        edge_sample: &EdgeSample,
    ) -> ([Av2Sample; TX4X4_SIZE + 1], [Av2Sample; TX4X4_SIZE + 1])
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let (plane_width, plane_height) = self.plane_geometry(plane);
        let (plane_region_right, plane_region_bottom) = self.plane_region_limit(plane);
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        let have_top = y0 > tile_origin_y;
        let have_left = x0 > tile_origin_x;
        let mut above = [av2_lossless_v_pred_above_edge(self.bit_depth); TX4X4_SIZE + 1];
        let mut left = [av2_lossless_h_pred_left_edge(self.bit_depth); TX4X4_SIZE + 1];

        if have_top {
            for local_x in 0..TX4X4_SIZE {
                above[local_x] = edge_sample(plane, x0 + local_x, y0 - 1);
            }
        } else if have_left {
            above[..TX4X4_SIZE].fill(edge_sample(plane, x0 - 1, y0));
        }

        if have_left {
            for local_y in 0..TX4X4_SIZE {
                left[local_y] = edge_sample(plane, x0 - 1, y0 + local_y);
            }
        } else if have_top {
            left[..TX4X4_SIZE].fill(edge_sample(plane, x0, y0 - 1));
        }

        let plane_sb_width = MVP_SUPERBLOCK_SIZE / sub_x;
        let plane_sb_height = MVP_SUPERBLOCK_SIZE / sub_y;
        let sb_origin_x = (x0 / plane_sb_width) * plane_sb_width;
        let sb_right = (sb_origin_x + plane_sb_width)
            .min(plane_width)
            .min(plane_region_right);
        let top_right_x = x0 + TX4X4_SIZE;
        let superblock_top_row = y0 % plane_sb_height == 0;
        let external_top_right_coded =
            have_top && y0 == leaf_y0 && top_right_x < plane_region_right && {
                let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, top_right_x, y0 - 1);
                superblock_top_row
                    || (top_right_x < sb_right && coded_mi_context.is_coded(row_mi, col_mi))
            };
        if have_top
            && top_right_x < plane_region_right
            && (top_right_x < leaf_x0 + leaf_width || external_top_right_coded)
        {
            above[TX4X4_SIZE] = edge_sample(plane, top_right_x, y0 - 1);
        } else {
            above[TX4X4_SIZE] = above[TX4X4_SIZE - 1];
        }

        let sb_origin_y = (y0 / plane_sb_height) * plane_sb_height;
        let sb_bottom = (sb_origin_y + plane_sb_height)
            .min(plane_height)
            .min(plane_region_bottom);
        let bottom_left_y = y0 + TX4X4_SIZE;
        let superblock_left_col = x0 % plane_sb_width == 0;
        let external_bottom_left_coded =
            have_left && x0 == leaf_x0 && bottom_left_y < sb_bottom && {
                let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, x0 - 1, bottom_left_y);
                superblock_left_col || coded_mi_context.is_coded(row_mi, col_mi)
            };
        if have_left
            && x0 == leaf_x0
            && bottom_left_y < plane_region_bottom
            && (bottom_left_y < leaf_y0 + leaf_height || external_bottom_left_coded)
        {
            left[TX4X4_SIZE] = edge_sample(plane, x0 - 1, bottom_left_y);
        } else {
            left[TX4X4_SIZE] = left[TX4X4_SIZE - 1];
        }

        (above, left)
    }
}
