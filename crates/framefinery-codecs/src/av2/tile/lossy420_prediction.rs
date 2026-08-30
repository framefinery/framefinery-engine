impl Av2LossySubsampledTileState<'_> {
    fn luma_directional_idif_predictor_state_with<EdgeSample>(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        angle: i16,
        context: Av2LossyLeafPredictorContext<'_>,
        edge_sample: &EdgeSample,
    ) -> (Option<Av2Sample>, Option<DirectionalIdifEdges>)
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let have_top = y0 > tile_origin_y;
        let have_left = x0 > tile_origin_x;
        let base = av2_lossless_dc_predictor(self.bit_depth);

        let constant_predictor = match angle {
            1..=89 if !have_top => Some(if have_left {
                edge_sample(plane, x0 - 1, y0)
            } else {
                base.saturating_sub(1)
            }),
            181..=269 if !have_left => Some(if have_top {
                edge_sample(plane, x0, y0 - 1)
            } else {
                base.saturating_add(1)
            }),
            _ => None,
        };

        let edges = constant_predictor
            .is_none()
            .then(|| self.luma_directional_idif_edges_with(plane, x0, y0, angle, context, edge_sample));

        (constant_predictor, edges)
    }

    fn luma_directional_idif_edges_with<EdgeSample>(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        angle: i16,
        context: Av2LossyLeafPredictorContext<'_>,
        edge_sample: &EdgeSample,
    ) -> DirectionalIdifEdges
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
    {
        let edge_context = Av2IntraEdgeContext {
            bit_depth: self.bit_depth,
            tile_origin: self.plane_origin(plane),
            plane_size: self.plane_geometry(plane),
            edge_limit: self.plane_region_limit(plane),
            subsampling: self.plane_subsampling(plane),
            leaf_origin: (context.leaf_x0, context.leaf_y0),
            leaf_size: (context.leaf_width, context.leaf_height),
        };
        let above_core = av2_directional_above_edge(
            edge_context,
            x0,
            y0,
            |x, y| edge_sample(plane, x, y),
            |x, y| {
                let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, x, y);
                context.coded_mi_context.is_coded(row_mi, col_mi)
            },
        );
        let left_core = av2_directional_left_edge(
            edge_context,
            x0,
            y0,
            |x, y| edge_sample(plane, x, y),
            |x, y| {
                let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, x, y);
                context.coded_mi_context.is_coded(row_mi, col_mi)
            },
        );
        let above_left = self.above_left_predictor_with(plane, x0, y0, edge_sample);
        assemble_directional_idif_edges(self.bit_depth, angle, above_core, left_core, above_left)
    }

    fn above_left_predictor_with<EdgeSample>(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        edge_sample: &EdgeSample,
    ) -> Av2Sample
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
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

    fn dc_predictor_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        av2_dc_predictor_from_edges(
            self.bit_depth,
            tile_origin_x,
            tile_origin_y,
            x0,
            y0,
            |x, y| self.neighbor_sample_for_score(plane, x, y, context),
        )
    }

    fn h_predictor_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        local_y: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        av2_h_predictor_from_edges(
            self.bit_depth,
            tile_origin_x,
            tile_origin_y,
            x0,
            y0,
            local_y,
            |x, y| self.neighbor_sample_for_score(plane, x, y, context),
        )
    }

    fn v_predictor_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        local_x: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        av2_v_predictor_from_edges(
            self.bit_depth,
            tile_origin_x,
            tile_origin_y,
            x0,
            y0,
            local_x,
            |x, y| self.neighbor_sample_for_score(plane, x, y, context),
        )
    }

    fn above_left_predictor_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2Sample {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        av2_above_left_predictor_from_edges(
            self.bit_depth,
            tile_origin_x,
            tile_origin_y,
            x0,
            y0,
            |x, y| self.neighbor_sample_for_score(plane, x, y, context),
        )
    }

    fn smooth_edges(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> ([Av2Sample; TX4X4_SIZE + 1], [Av2Sample; TX4X4_SIZE + 1]) {
        let edge_sample = |plane, x, y| self.recon_sample(plane, x, y);
        self.smooth_edges_with(plane, x0, y0, context, &edge_sample)
    }

    fn smooth_edges_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> ([Av2Sample; TX4X4_SIZE + 1], [Av2Sample; TX4X4_SIZE + 1]) {
        let edge_sample = |plane, x, y| self.neighbor_sample_for_score(plane, x, y, context);
        self.smooth_edges_with(plane, x0, y0, context, &edge_sample)
    }

    fn smooth_edges_with<EdgeSample>(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        edge_sample: &EdgeSample,
    ) -> ([Av2Sample; TX4X4_SIZE + 1], [Av2Sample; TX4X4_SIZE + 1])
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
    {
        av2_smooth_intra_edges(
            Av2IntraEdgeContext {
                bit_depth: self.bit_depth,
                tile_origin: self.plane_origin(plane),
                plane_size: self.plane_geometry(plane),
                edge_limit: self.plane_region_limit(plane),
                subsampling: self.plane_subsampling(plane),
                leaf_origin: (context.leaf_x0, context.leaf_y0),
                leaf_size: (context.leaf_width, context.leaf_height),
            },
            x0,
            y0,
            |x, y| edge_sample(plane, x, y),
            |x, y| {
                let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, x, y);
                context.coded_mi_context.is_coded(row_mi, col_mi)
            },
        )
    }

    fn neighbor_sample_for_score(
        &self,
        plane: Av2LossyPlane,
        x: usize,
        y: usize,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2Sample {
        if x >= context.leaf_x0
            && x < context.leaf_x0 + context.leaf_width
            && y >= context.leaf_y0
            && y < context.leaf_y0 + context.leaf_height
        {
            self.source_sample(plane, x, y)
        } else {
            self.recon_sample(plane, x, y)
        }
    }
}
