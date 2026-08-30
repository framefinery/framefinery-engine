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
            leaf_height,
            coded_mi_context,
            edge_sample,
        );
        let left_core = self.directional_left_edge_with(
            Av2LosslessPlane::Y,
            x0,
            y0,
            leaf_x0,
            leaf_y0,
            leaf_width,
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
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
        edge_sample: &EdgeSample,
    ) -> [Av2Sample; 8]
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
    {
        let plane_size = self.plane_geometry(plane);
        av2_directional_above_edge(
            Av2IntraEdgeContext {
                bit_depth: self.bit_depth,
                tile_origin: self.plane_origin(plane),
                plane_size,
                edge_limit: plane_size,
                subsampling: self.plane_subsampling(plane),
                leaf_origin: (leaf_x0, leaf_y0),
                leaf_size: (leaf_width, leaf_height),
            },
            x0,
            y0,
            |x, y| edge_sample(plane, x, y),
            |x, y| {
                let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, x, y);
                coded_mi_context.is_coded(row_mi, col_mi)
            },
        )
    }

    fn directional_left_edge_with<EdgeSample>(
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
    ) -> [Av2Sample; 8]
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
    {
        let plane_size = self.plane_geometry(plane);
        av2_directional_left_edge(
            Av2IntraEdgeContext {
                bit_depth: self.bit_depth,
                tile_origin: self.plane_origin(plane),
                plane_size,
                edge_limit: plane_size,
                subsampling: self.plane_subsampling(plane),
                leaf_origin: (leaf_x0, leaf_y0),
                leaf_size: (leaf_width, leaf_height),
            },
            x0,
            y0,
            |x, y| edge_sample(plane, x, y),
            |x, y| {
                let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, x, y);
                coded_mi_context.is_coded(row_mi, col_mi)
            },
        )
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
        av2_smooth_intra_edges(
            Av2IntraEdgeContext {
                bit_depth: self.bit_depth,
                tile_origin: self.plane_origin(plane),
                plane_size: self.plane_geometry(plane),
                edge_limit: self.plane_region_limit(plane),
                subsampling: self.plane_subsampling(plane),
                leaf_origin: (leaf_x0, leaf_y0),
                leaf_size: (leaf_width, leaf_height),
            },
            x0,
            y0,
            |x, y| edge_sample(plane, x, y),
            |x, y| {
                let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, x, y);
                coded_mi_context.is_coded(row_mi, col_mi)
            },
        )
    }
}
