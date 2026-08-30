struct Av2LosslessSubsampledTileState<'a> {
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
    mode_search: Av2LosslessSubsampledModeSearch,
    source: &'a [u8],
    recon: &'a mut [u8],
    layout: Av2PlanarTileLayout,
    source_backed_recon: bool,
}

impl<'a> Av2LosslessSubsampledTileState<'a> {
    fn new(
        geometry: Av2VideoGeometry,
        region: Av2TileRegion,
        chroma_format: Av2ChromaFormat,
        bit_depth: SampleBitDepth,
        mode_search: Av2LosslessSubsampledModeSearch,
        source: &'a [u8],
        recon: &'a mut [u8],
    ) -> Self {
        assert!(
            matches!(
                chroma_format,
                Av2ChromaFormat::Yuv420 | Av2ChromaFormat::Yuv422 | Av2ChromaFormat::Yuv444
            ),
            "AV2 planar lossless state expects 4:2:0, 4:2:2, or 4:4:4 input"
        );
        let layout =
            Av2PlanarTileLayout::for_validated_shape(geometry, region, chroma_format, bit_depth);
        let expected_len = layout.frame_len();
        assert_eq!(
            source.len(),
            expected_len,
            "AV2 planar lossless source length must match geometry"
        );
        let source_backed_recon = mode_search == Av2LosslessSubsampledModeSearch::FastScreenContent;
        if source_backed_recon {
            assert!(
                recon.is_empty() || recon.len() == source.len(),
                "AV2 fast planar lossless reconstruction must be empty or match source"
            );
        } else {
            assert_eq!(
                recon.len(),
                source.len(),
                "AV2 planar lossless reconstruction length must match source"
            );
        }
        Self {
            chroma_format,
            bit_depth,
            mode_search,
            source,
            recon,
            layout,
            source_backed_recon,
        }
    }

    fn plane_geometry(&self, plane: Av2LosslessPlane) -> (usize, usize) {
        self.layout.plane_geometry(plane.planar())
    }

    fn plane_origin(&self, plane: Av2LosslessPlane) -> (usize, usize) {
        self.layout.plane_origin(plane.planar())
    }

    fn plane_region_limit(&self, plane: Av2LosslessPlane) -> (usize, usize) {
        self.layout.plane_region_limit(plane.planar())
    }

    fn plane_subsampling(&self, plane: Av2LosslessPlane) -> (usize, usize) {
        self.layout.plane_subsampling(plane.planar())
    }

    fn coded_mi_for_plane_sample(
        &self,
        plane: Av2LosslessPlane,
        x: usize,
        y: usize,
    ) -> (usize, usize) {
        self.layout
            .coded_mi_for_plane_sample(plane.planar(), x, y)
    }

    fn txb_origin(&self, plane: Av2LosslessPlane, col: usize, row: usize) -> (usize, usize) {
        self.layout.txb_origin(plane.planar(), col, row)
    }

    fn source_block4x4(&self, plane: Av2LosslessPlane, x0: usize, y0: usize) -> [i32; 16] {
        let mut block = [0i32; TX4X4_SAMPLES];
        let stride = self.layout.plane_stride(plane.planar());
        if self.bit_depth.bits() <= 8 {
            let start = self.offset(plane, x0, y0);
            for local_y in 0..TX4X4_SIZE {
                let row_start = start + local_y * stride;
                let row = &self.source[row_start..row_start + TX4X4_SIZE];
                let block_row = local_y * TX4X4_SIZE;
                block[block_row] = i32::from(row[0]);
                block[block_row + 1] = i32::from(row[1]);
                block[block_row + 2] = i32::from(row[2]);
                block[block_row + 3] = i32::from(row[3]);
            }
            return block;
        }
        let start = self.offset(plane, x0, y0) * 2;
        let stride_bytes = stride * 2;
        for local_y in 0..TX4X4_SIZE {
            let row_start = start + local_y * stride_bytes;
            let row = &self.source[row_start..row_start + TX4X4_SIZE * 2];
            let block_row = local_y * TX4X4_SIZE;
            block[block_row] = i32::from(Av2Sample::from_le_bytes([row[0], row[1]]));
            block[block_row + 1] = i32::from(Av2Sample::from_le_bytes([row[2], row[3]]));
            block[block_row + 2] = i32::from(Av2Sample::from_le_bytes([row[4], row[5]]));
            block[block_row + 3] = i32::from(Av2Sample::from_le_bytes([row[6], row[7]]));
        }
        block
    }

    fn offset(&self, plane: Av2LosslessPlane, x: usize, y: usize) -> usize {
        self.layout.offset(plane.planar(), x, y)
    }

    fn source_sample(&self, plane: Av2LosslessPlane, x: usize, y: usize) -> Av2Sample {
        read_planar_sample(self.source, self.offset(plane, x, y), self.bit_depth)
    }

    fn reference_sample(
        &self,
        reference: &[u8],
        plane: Av2LosslessPlane,
        x: usize,
        y: usize,
    ) -> Av2Sample {
        read_planar_sample(reference, self.offset(plane, x, y), self.bit_depth)
    }

    fn recon_sample(&self, plane: Av2LosslessPlane, x: usize, y: usize) -> Av2Sample {
        if self.source_backed_recon {
            return self.source_sample(plane, x, y);
        }
        read_planar_sample(self.recon, self.offset(plane, x, y), self.bit_depth)
    }

    fn dc_predictor_with<EdgeSample>(
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
        av2_dc_predictor_from_edges(
            self.bit_depth,
            tile_origin_x,
            tile_origin_y,
            x0,
            y0,
            |x, y| edge_sample(plane, x, y),
        )
    }

    fn h_predictor(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        local_y: usize,
    ) -> Av2Sample {
        let edge_sample = |plane, x, y| self.recon_sample(plane, x, y);
        self.h_predictor_with(plane, x0, y0, local_y, &edge_sample)
    }

    fn h_predictor_with<EdgeSample>(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        local_y: usize,
        edge_sample: &EdgeSample,
    ) -> Av2Sample
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        av2_h_predictor_from_edges(
            self.bit_depth,
            tile_origin_x,
            tile_origin_y,
            x0,
            y0,
            local_y,
            |x, y| edge_sample(plane, x, y),
        )
    }

    fn v_predictor(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        local_x: usize,
    ) -> Av2Sample {
        let edge_sample = |plane, x, y| self.recon_sample(plane, x, y);
        self.v_predictor_with(plane, x0, y0, local_x, &edge_sample)
    }

    fn v_predictor_with<EdgeSample>(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        local_x: usize,
        edge_sample: &EdgeSample,
    ) -> Av2Sample
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        av2_v_predictor_from_edges(
            self.bit_depth,
            tile_origin_x,
            tile_origin_y,
            x0,
            y0,
            local_x,
            |x, y| edge_sample(plane, x, y),
        )
    }

    #[cfg(test)]
    fn tx4x4_coefficients(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
    ) -> [i32; TX4X4_SAMPLES] {
        let (luma_width, luma_height) = self.plane_geometry(Av2LosslessPlane::Y);
        self.tx4x4_coefficients_for_mode(
            plane,
            x0,
            y0,
            Av2LosslessSubsampledModeDecision::default(),
            x0,
            y0,
            TX4X4_SIZE,
            TX4X4_SIZE,
            &Av2CodedMiContext::new(
                luma_height.div_ceil(MI_SIZE),
                luma_width.div_ceil(MI_SIZE),
            ),
        )
    }

    #[cfg(test)]
    fn tx4x4_coefficients_for_mode(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        mode: Av2LosslessSubsampledModeDecision,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> [i32; TX4X4_SAMPLES] {
        let residual = self.tx4x4_residual_for_mode(
            plane,
            x0,
            y0,
            mode,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
        );
        tx4x4_coefficients_from_residual(&residual, mode.use_fsc)
    }

    fn tx4x4_residual_for_mode(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        mode: Av2LosslessSubsampledModeDecision,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> [i32; TX4X4_SAMPLES] {
        if self.source_backed_recon && !mode.use_fsc {
            match plane {
                Av2LosslessPlane::Y => {
                    if let Some(horz) = mode.luma_bdpcm_horz {
                        self.source_backed_dpcm_residual4x4(plane, x0, y0, horz)
                    } else if let Some(residual) =
                        self.source_backed_luma_intra_residual4x4(x0, y0, mode.luma_intra_mode)
                    {
                        residual
                    } else {
                        self.luma_intra_residual4x4(
                            x0,
                            y0,
                            mode.luma_intra_mode,
                            leaf_x0,
                            leaf_y0,
                            leaf_width,
                            leaf_height,
                            coded_mi_context,
                        )
                    }
                }
                Av2LosslessPlane::U | Av2LosslessPlane::V => {
                    if mode.chroma_use_bdpcm {
                        self.source_backed_dpcm_residual4x4(
                            plane,
                            x0,
                            y0,
                            mode.chroma_intra_mode.is_horizontal(),
                        )
                    } else if let Some(residual) = self.source_backed_chroma_intra_residual4x4(
                        plane,
                        x0,
                        y0,
                        mode.chroma_intra_mode,
                    ) {
                        residual
                    } else {
                        self.intra_residual4x4(
                            plane,
                            x0,
                            y0,
                            mode.chroma_intra_mode,
                            chroma_directional_angle_for_mode(mode),
                            leaf_x0,
                            leaf_y0,
                            leaf_width,
                            leaf_height,
                            coded_mi_context,
                        )
                    }
                }
            }
        } else {
            match plane {
                Av2LosslessPlane::Y => {
                    if let Some(horz) = mode.luma_bdpcm_horz {
                        self.dpcm_residual4x4(plane, x0, y0, horz)
                    } else {
                        self.luma_intra_residual4x4(
                            x0,
                            y0,
                            mode.luma_intra_mode,
                            leaf_x0,
                            leaf_y0,
                            leaf_width,
                            leaf_height,
                            coded_mi_context,
                        )
                    }
                }
                Av2LosslessPlane::U | Av2LosslessPlane::V => {
                    if mode.chroma_use_bdpcm {
                        self.dpcm_residual4x4(
                            plane,
                            x0,
                            y0,
                            mode.chroma_intra_mode.is_horizontal(),
                        )
                    } else {
                        self.intra_residual4x4(
                            plane,
                            x0,
                            y0,
                            mode.chroma_intra_mode,
                            chroma_directional_angle_for_mode(mode),
                            leaf_x0,
                            leaf_y0,
                            leaf_width,
                            leaf_height,
                            coded_mi_context,
                        )
                    }
                }
            }
        }
    }

    fn luma_palette_residual4x4(
        &self,
        palette: &Av2LumaPalette444,
        region: &Av2LumaPaletteRegion,
        x0: usize,
        y0: usize,
    ) -> [i32; TX4X4_SAMPLES] {
        let mut residual = [0i32; TX4X4_SAMPLES];
        let source = self.source_block4x4(Av2LosslessPlane::Y, x0, y0);
        for local_y in 0..TX4X4_SIZE {
            let y = y0 + local_y;
            for local_x in 0..TX4X4_SIZE {
                let x = x0 + local_x;
                let pos = local_y * TX4X4_SIZE + local_x;
                residual[pos] =
                    source[pos] - i32::from(palette.region_prediction_sample(region, x, y));
            }
        }
        residual
    }

    fn inter_residual4x4(
        &self,
        reference: &[u8],
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        mv_row_px: i16,
        mv_col_px: i16,
    ) -> [i32; TX4X4_SAMPLES] {
        assert_eq!(
            reference.len(),
            self.source.len(),
            "AV2 inter residual reference length must match source"
        );
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        debug_assert_eq!(usize::from(mv_col_px.unsigned_abs()) % sub_x, 0);
        debug_assert_eq!(usize::from(mv_row_px.unsigned_abs()) % sub_y, 0);
        let ref_x0 = x0 as isize + isize::from(mv_col_px) / sub_x as isize;
        let ref_y0 = y0 as isize + isize::from(mv_row_px) / sub_y as isize;
        let (plane_width, plane_height) = self.plane_geometry(plane);
        assert!(ref_x0 >= 0 && ref_y0 >= 0, "AV2 inter residual reference is out of bounds");
        let ref_x0 = ref_x0 as usize;
        let ref_y0 = ref_y0 as usize;
        assert!(
            ref_x0 + TX4X4_SIZE <= plane_width && ref_y0 + TX4X4_SIZE <= plane_height,
            "AV2 inter residual reference is out of bounds"
        );

        let mut residual = [0i32; TX4X4_SAMPLES];
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let source_sample = self.source_sample(plane, x0 + local_x, y0 + local_y);
                let reference_sample =
                    self.reference_sample(reference, plane, ref_x0 + local_x, ref_y0 + local_y);
                residual[local_y * TX4X4_SIZE + local_x] =
                    i32::from(source_sample) - i32::from(reference_sample);
            }
        }
        residual
    }

    fn tx4x4_coefficients_for_mode_score(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        mode: Av2LosslessSubsampledModeDecision,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> [i32; TX4X4_SAMPLES] {
        let residual = match plane {
            Av2LosslessPlane::Y => {
                if let Some(horz) = mode.luma_bdpcm_horz {
                    self.dpcm_residual4x4_for_score(plane, x0, y0, horz, leaf_x0, leaf_y0)
                } else {
                    self.luma_intra_residual4x4_for_score(
                        x0,
                        y0,
                        mode.luma_intra_mode,
                        leaf_x0,
                        leaf_y0,
                        leaf_width,
                        leaf_height,
                        coded_mi_context,
                    )
                }
            }
            Av2LosslessPlane::U | Av2LosslessPlane::V => {
                if mode.chroma_use_bdpcm {
                    self.dpcm_residual4x4_for_score(
                        plane,
                        x0,
                        y0,
                        mode.chroma_intra_mode.is_horizontal(),
                        leaf_x0,
                        leaf_y0,
                    )
                } else {
                    self.intra_residual4x4_for_score(
                        plane,
                        x0,
                        y0,
                        mode.chroma_intra_mode,
                        chroma_directional_angle_for_mode(mode),
                        leaf_x0,
                        leaf_y0,
                        leaf_width,
                        leaf_height,
                        coded_mi_context,
                    )
                }
            }
        };
        if mode.use_fsc {
            idtx4x4_coefficients(&residual)
        } else {
            av2_fwht4x4(&residual)
        }
    }

    fn luma_intra_residual4x4(
        &self,
        x0: usize,
        y0: usize,
        mode: Av2LumaIntraMode,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> [i32; TX4X4_SAMPLES] {
        if let Some((base, delta)) = mode.directional() {
            let angle = base.angle(delta);
            if angle != 90 && angle != 180 {
                return self.luma_directional_idif_residual4x4(
                    x0,
                    y0,
                    angle,
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    leaf_height,
                    coded_mi_context,
                );
            }
        }
        self.intra_residual4x4(
            Av2LosslessPlane::Y,
            x0,
            y0,
            chroma_mode_for_luma_mode(mode),
            None,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
        )
    }

    fn luma_intra_residual4x4_for_score(
        &self,
        x0: usize,
        y0: usize,
        mode: Av2LumaIntraMode,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> [i32; TX4X4_SAMPLES] {
        if let Some((base, delta)) = mode.directional() {
            let angle = base.angle(delta);
            if angle != 90 && angle != 180 {
                return self.luma_directional_idif_residual4x4_for_score(
                    x0,
                    y0,
                    angle,
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    leaf_height,
                    coded_mi_context,
                );
            }
        }
        self.intra_residual4x4_for_score(
            Av2LosslessPlane::Y,
            x0,
            y0,
            chroma_mode_for_luma_mode(mode),
            None,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
        )
    }

    fn intra_residual4x4(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        mode: Av2ChromaIntraMode,
        directional_angle: Option<i16>,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> [i32; TX4X4_SAMPLES] {
        if plane == Av2LosslessPlane::Y {
            if let Some(angle) = av2_chroma_directional_angle(mode) {
                if angle != 90 && angle != 180 {
                    return self.luma_directional_idif_residual4x4(
                        x0,
                        y0,
                        angle,
                        leaf_x0,
                        leaf_y0,
                        leaf_width,
                        leaf_height,
                        coded_mi_context,
                    );
                }
            }
        }
        let edge_sample = |plane, x, y| self.recon_sample(plane, x, y);
        self.intra_residual4x4_with_edge_policy(
            mode,
            directional_angle,
            plane,
            x0,
            y0,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
            edge_sample,
            || self.smooth_edges(
                plane,
                x0,
                y0,
                leaf_x0,
                leaf_y0,
                leaf_width,
                leaf_height,
                coded_mi_context,
            ),
        )
    }

    fn intra_residual4x4_with_edge_policy<EdgeSample, SmoothEdges>(
        &self,
        mode: Av2ChromaIntraMode,
        directional_angle: Option<i16>,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
        edge_sample: EdgeSample,
        smooth_edges: SmoothEdges,
    ) -> [i32; TX4X4_SAMPLES]
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
        SmoothEdges: Fn() -> ([Av2Sample; TX4X4_SIZE + 1], [Av2Sample; TX4X4_SIZE + 1]),
    {
        av2_intra_residual4x4(
            mode,
            directional_angle,
            self.bit_depth,
            |local_x, local_y| self.source_sample(plane, x0 + local_x, y0 + local_y),
            || self.dc_predictor_with(plane, x0, y0, &edge_sample),
            |local_y| self.h_predictor_with(plane, x0, y0, local_y, &edge_sample),
            |local_x| self.v_predictor_with(plane, x0, y0, local_x, &edge_sample),
            || self.above_left_predictor_with(plane, x0, y0, &edge_sample),
            |angle, local_x, local_y| {
                self.directional_predictor_with(
                    plane,
                    x0,
                    y0,
                    angle,
                    local_x,
                    local_y,
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    leaf_height,
                    coded_mi_context,
                    &edge_sample,
                )
            },
            smooth_edges,
        )
    }

    fn directional_predictor_with<EdgeSample>(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        angle: i16,
        local_x: usize,
        local_y: usize,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
        edge_sample: &EdgeSample,
    ) -> Av2Sample
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
    {
        if angle > 0 && angle < 90 {
            let above = self.directional_above_edge_with(
                plane,
                x0,
                y0,
                leaf_x0,
                leaf_y0,
                leaf_width,
                coded_mi_context,
                edge_sample,
            );
            return directional_interpolate_with_delta(
                above,
                av2_directional_dx(angle),
                local_x,
                local_y,
            );
        }
        if angle > 90 && angle < 180 {
            let above = self.directional_above_edge_with(
                plane,
                x0,
                y0,
                leaf_x0,
                leaf_y0,
                leaf_width,
                coded_mi_context,
                edge_sample,
            );
            let left = self.directional_left_edge_with(
                plane,
                x0,
                y0,
                leaf_x0,
                leaf_y0,
                leaf_height,
                coded_mi_context,
                edge_sample,
            );
            let edges = ChromaD135Edges {
                above_left: self.above_left_predictor_with(plane, x0, y0, edge_sample),
                above: [above[0], above[1], above[2], above[3]],
                left: [left[0], left[1], left[2], left[3]],
            };
            return zone2_directional_predictor(
                edges,
                av2_directional_dx(angle),
                av2_directional_dy(angle),
                local_x,
                local_y,
            );
        }
        if angle > 180 && angle < 270 {
            let left = self.directional_left_edge_with(
                plane,
                x0,
                y0,
                leaf_x0,
                leaf_y0,
                leaf_height,
                coded_mi_context,
                edge_sample,
            );
            return directional_interpolate_with_delta(
                left,
                av2_directional_dy(angle),
                local_y,
                local_x,
            );
        }
        unreachable!("generic directional predictor expects a non-cardinal angle")
    }

    fn dpcm_residual4x4(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        horz: bool,
    ) -> [i32; TX4X4_SAMPLES] {
        self.dpcm_residual4x4_with_edge_predictors(plane, x0, y0, horz, |local_y| {
            self.h_predictor(plane, x0, y0, local_y)
        }, |local_x| self.v_predictor(plane, x0, y0, local_x))
    }

    fn dpcm_residual4x4_with_edge_predictors(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        horz: bool,
        h_edge: impl Fn(usize) -> Av2Sample,
        v_edge: impl Fn(usize) -> Av2Sample,
    ) -> [i32; TX4X4_SAMPLES] {
        let mut residual = [0i32; TX4X4_SAMPLES];
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let x = x0 + local_x;
                let y = y0 + local_y;
                let sample = i32::from(self.source_sample(plane, x, y));
                let predicted_delta = if horz {
                    if local_x == 0 {
                        sample - i32::from(h_edge(local_y))
                    } else {
                        sample - i32::from(self.source_sample(plane, x - 1, y))
                    }
                } else if local_y == 0 {
                    sample - i32::from(v_edge(local_x))
                } else {
                    sample - i32::from(self.source_sample(plane, x, y - 1))
                };
                residual[local_y * TX4X4_SIZE + local_x] = predicted_delta;
            }
        }
        residual
    }

    fn source_backed_luma_intra_residual4x4(
        &self,
        x0: usize,
        y0: usize,
        mode: Av2LumaIntraMode,
    ) -> Option<[i32; TX4X4_SAMPLES]> {
        self.source_backed_chroma_intra_residual4x4(
            Av2LosslessPlane::Y,
            x0,
            y0,
            chroma_mode_for_luma_mode(mode),
        )
    }

    fn source_backed_chroma_intra_residual4x4(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        mode: Av2ChromaIntraMode,
    ) -> Option<[i32; TX4X4_SAMPLES]> {
        let mut residual = [0i32; TX4X4_SAMPLES];
        let source = self.source_block4x4(plane, x0, y0);
        match mode {
            Av2ChromaIntraMode::Dc => {
                let predictor = i32::from(self.source_backed_dc_predictor(plane, x0, y0));
                for index in 0..TX4X4_SAMPLES {
                    residual[index] = source[index] - predictor;
                }
                Some(residual)
            }
            Av2ChromaIntraMode::Horizontal => {
                for local_y in 0..TX4X4_SIZE {
                    let predictor =
                        i32::from(self.source_backed_h_predictor(plane, x0, y0, local_y));
                    let row_start = local_y * TX4X4_SIZE;
                    for local_x in 0..TX4X4_SIZE {
                        let pos = row_start + local_x;
                        residual[pos] = source[pos] - predictor;
                    }
                }
                Some(residual)
            }
            Av2ChromaIntraMode::Vertical => {
                let mut predictors = [0i32; TX4X4_SIZE];
                for (local_x, predictor) in predictors.iter_mut().enumerate() {
                    *predictor = i32::from(self.source_backed_v_predictor(plane, x0, y0, local_x));
                }
                for local_y in 0..TX4X4_SIZE {
                    for local_x in 0..TX4X4_SIZE {
                        let pos = local_y * TX4X4_SIZE + local_x;
                        residual[pos] = source[pos] - predictors[local_x];
                    }
                }
                Some(residual)
            }
            _ => None,
        }
    }

    fn source_backed_dpcm_residual4x4(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        horz: bool,
    ) -> [i32; TX4X4_SAMPLES] {
        let mut residual = [0i32; TX4X4_SAMPLES];
        let source = self.source_block4x4(plane, x0, y0);
        if horz {
            for local_y in 0..TX4X4_SIZE {
                let row_start = local_y * TX4X4_SIZE;
                let predictor = i32::from(self.source_backed_h_predictor(plane, x0, y0, local_y));
                residual[row_start] = source[row_start] - predictor;
                for local_x in 1..TX4X4_SIZE {
                    let pos = row_start + local_x;
                    residual[pos] = source[pos] - source[pos - 1];
                }
            }
        } else {
            let mut predictors = [0i32; TX4X4_SIZE];
            for (local_x, predictor) in predictors.iter_mut().enumerate() {
                *predictor = i32::from(self.source_backed_v_predictor(plane, x0, y0, local_x));
            }
            for local_x in 0..TX4X4_SIZE {
                residual[local_x] = source[local_x] - predictors[local_x];
            }
            for local_y in 1..TX4X4_SIZE {
                let row_start = local_y * TX4X4_SIZE;
                for local_x in 0..TX4X4_SIZE {
                    let pos = row_start + local_x;
                    residual[pos] = source[pos] - source[pos - TX4X4_SIZE];
                }
            }
        }
        residual
    }

    fn source_backed_dc_predictor(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
    ) -> Av2Sample {
        let edge_sample = |plane, x, y| self.source_sample(plane, x, y);
        self.dc_predictor_with(plane, x0, y0, &edge_sample)
    }

    fn source_backed_h_predictor(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        local_y: usize,
    ) -> Av2Sample {
        let edge_sample = |plane, x, y| self.source_sample(plane, x, y);
        self.h_predictor_with(plane, x0, y0, local_y, &edge_sample)
    }

    fn source_backed_v_predictor(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        local_x: usize,
    ) -> Av2Sample {
        let edge_sample = |plane, x, y| self.source_sample(plane, x, y);
        self.v_predictor_with(plane, x0, y0, local_x, &edge_sample)
    }

    fn intra_residual4x4_for_score(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        mode: Av2ChromaIntraMode,
        directional_angle: Option<i16>,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> [i32; TX4X4_SAMPLES] {
        if plane == Av2LosslessPlane::Y {
            if let Some(angle) = av2_chroma_directional_angle(mode) {
                if angle != 90 && angle != 180 {
                    return self.luma_directional_idif_residual4x4_for_score(
                        x0,
                        y0,
                        angle,
                        leaf_x0,
                        leaf_y0,
                        leaf_width,
                        leaf_height,
                        coded_mi_context,
                    );
                }
            }
        }
        let edge_sample =
            |plane, x, y| self.neighbor_sample_for_score(plane, x, y, leaf_x0, leaf_y0);
        self.intra_residual4x4_with_edge_policy(
            mode,
            directional_angle,
            plane,
            x0,
            y0,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
            edge_sample,
            || self.smooth_edges_for_score(
                plane,
                x0,
                y0,
                leaf_x0,
                leaf_y0,
                leaf_width,
                leaf_height,
                coded_mi_context,
            ),
        )
    }

    fn dpcm_residual4x4_for_score(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        horz: bool,
        leaf_x0: usize,
        leaf_y0: usize,
    ) -> [i32; TX4X4_SAMPLES] {
        self.dpcm_residual4x4_with_edge_predictors(
            plane,
            x0,
            y0,
            horz,
            |local_y| {
                self.h_predictor_for_score(plane, x0, y0, local_y, leaf_x0, leaf_y0)
            },
            |local_x| {
                self.v_predictor_for_score(plane, x0, y0, local_x, leaf_x0, leaf_y0)
            },
        )
    }

    fn dc_h_v_bdpcm_txb_scores_for_score(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        leaf_x0: usize,
        leaf_y0: usize,
        kind: Av2CoefficientProxyKind,
    ) -> Av2DcHvBdpcmTxbScores {
        let source = self.source_block4x4(plane, x0, y0);
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let have_left = x0 > tile_origin_x;
        let have_top = y0 > tile_origin_y;
        let mut left = [0i32; TX4X4_SIZE];
        let mut above = [0i32; TX4X4_SIZE];
        if have_left {
            for local_y in 0..TX4X4_SIZE {
                left[local_y] = i32::from(self.neighbor_sample_for_score(
                    plane,
                    x0 - 1,
                    y0 + local_y,
                    leaf_x0,
                    leaf_y0,
                ));
            }
        }
        if have_top {
            for local_x in 0..TX4X4_SIZE {
                above[local_x] = i32::from(self.neighbor_sample_for_score(
                    plane,
                    x0 + local_x,
                    y0 - 1,
                    leaf_x0,
                    leaf_y0,
                ));
            }
        }

        let dc = if have_left || have_top {
            let mut sum = 0i32;
            let mut count = 0i32;
            if have_top {
                sum += above.iter().sum::<i32>();
                count += TX4X4_SIZE as i32;
            }
            if have_left {
                sum += left.iter().sum::<i32>();
                count += TX4X4_SIZE as i32;
            }
            (sum + count / 2) / count
        } else {
            i32::from(av2_lossless_dc_predictor(self.bit_depth))
        };

        let mut h_pred = [0i32; TX4X4_SIZE];
        if have_left {
            h_pred = left;
        } else if have_top {
            h_pred.fill(above[0]);
        } else {
            h_pred.fill(i32::from(av2_lossless_h_pred_left_edge(self.bit_depth)));
        }

        let mut v_pred = [0i32; TX4X4_SIZE];
        if have_top {
            v_pred = above;
        } else if have_left {
            v_pred.fill(left[0]);
        } else {
            v_pred.fill(i32::from(av2_lossless_v_pred_above_edge(self.bit_depth)));
        }

        let magnitude_scale = residual_sample_proxy_magnitude_scale(kind);
        let mut scores = Av2DcHvBdpcmTxbScores {
            dc: 16,
            horizontal: 16,
            vertical: 16,
            bdpcm_horizontal: 16,
            bdpcm_vertical: 16,
        };
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let pos = local_y * TX4X4_SIZE + local_x;
                let sample = source[pos];
                add_residual_sample_proxy_score(&mut scores.dc, sample - dc, magnitude_scale);
                add_residual_sample_proxy_score(
                    &mut scores.horizontal,
                    sample - h_pred[local_y],
                    magnitude_scale,
                );
                add_residual_sample_proxy_score(
                    &mut scores.vertical,
                    sample - v_pred[local_x],
                    magnitude_scale,
                );
                let bdpcm_horizontal = if local_x == 0 {
                    sample - h_pred[local_y]
                } else {
                    sample - source[pos - 1]
                };
                add_residual_sample_proxy_score(
                    &mut scores.bdpcm_horizontal,
                    bdpcm_horizontal,
                    magnitude_scale,
                );
                let bdpcm_vertical = if local_y == 0 {
                    sample - v_pred[local_x]
                } else {
                    sample - source[pos - TX4X4_SIZE]
                };
                add_residual_sample_proxy_score(
                    &mut scores.bdpcm_vertical,
                    bdpcm_vertical,
                    magnitude_scale,
                );
            }
        }

        scores
    }

    fn h_predictor_for_score(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        local_y: usize,
        leaf_x0: usize,
        leaf_y0: usize,
    ) -> Av2Sample {
        let edge_sample =
            |plane, x, y| self.neighbor_sample_for_score(plane, x, y, leaf_x0, leaf_y0);
        self.h_predictor_with(plane, x0, y0, local_y, &edge_sample)
    }

    fn v_predictor_for_score(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        local_x: usize,
        leaf_x0: usize,
        leaf_y0: usize,
    ) -> Av2Sample {
        let edge_sample =
            |plane, x, y| self.neighbor_sample_for_score(plane, x, y, leaf_x0, leaf_y0);
        self.v_predictor_with(plane, x0, y0, local_x, &edge_sample)
    }

    fn luma_directional_idif_residual4x4(
        &self,
        x0: usize,
        y0: usize,
        angle: i16,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> [i32; TX4X4_SAMPLES] {
        let edge_sample = |plane, x, y| self.recon_sample(plane, x, y);
        let source_sample = |plane, x, y| self.source_sample(plane, x, y);
        self.luma_directional_idif_residual4x4_with(
            x0,
            y0,
            angle,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
            edge_sample,
            source_sample,
        )
    }

    fn luma_directional_idif_residual4x4_for_score(
        &self,
        x0: usize,
        y0: usize,
        angle: i16,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> [i32; TX4X4_SAMPLES] {
        let edge_sample =
            |plane, x, y| self.neighbor_sample_for_score(plane, x, y, leaf_x0, leaf_y0);
        let source_sample = |plane, x, y| self.source_sample(plane, x, y);
        self.luma_directional_idif_residual4x4_with(
            x0,
            y0,
            angle,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
            edge_sample,
            source_sample,
        )
    }

    fn luma_directional_idif_residual4x4_with<EdgeSample, SourceSample>(
        &self,
        x0: usize,
        y0: usize,
        angle: i16,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
        edge_sample: EdgeSample,
        source_sample: SourceSample,
    ) -> [i32; TX4X4_SAMPLES]
    where
        EdgeSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
        SourceSample: Fn(Av2LosslessPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(Av2LosslessPlane::Y);
        let have_top = y0 > tile_origin_y;
        let have_left = x0 > tile_origin_x;
        let base = av2_lossless_dc_predictor(self.bit_depth);

        let constant_predictor = match angle {
            1..=89 if !have_top => Some(if have_left {
                edge_sample(Av2LosslessPlane::Y, x0 - 1, y0)
            } else {
                base.saturating_sub(1)
            }),
            181..=269 if !have_left => Some(if have_top {
                edge_sample(Av2LosslessPlane::Y, x0, y0 - 1)
            } else {
                base.saturating_add(1)
            }),
            _ => None,
        };

        let edges = constant_predictor.is_none().then(|| {
            self.luma_directional_idif_edges_with(
                x0,
                y0,
                angle,
                leaf_x0,
                leaf_y0,
                leaf_width,
                leaf_height,
                coded_mi_context,
                &edge_sample,
            )
        });

        let mut residual = [0i32; TX4X4_SAMPLES];
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let predictor = constant_predictor.unwrap_or_else(|| {
                    luma_directional_idif_predictor(
                        angle,
                        edges.expect("IDIF edges are precomputed"),
                        local_x,
                        local_y,
                        self.bit_depth,
                    )
                });
                residual[local_y * TX4X4_SIZE + local_x] = i32::from(source_sample(
                    Av2LosslessPlane::Y,
                    x0 + local_x,
                    y0 + local_y,
                )) - i32::from(predictor);
            }
        }
        residual
    }

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
        let mut edges = DirectionalIdifEdges::new(self.bit_depth);
        edges.set_above(-2, above_left);
        edges.set_above(-1, above_left);
        edges.set_left(-2, above_left);
        edges.set_left(-1, above_left);
        for index in 0..8 {
            edges.set_above(index as i32, above_core[index]);
            edges.set_left(index as i32, left_core[index]);
        }
        if angle > 90 && angle < 180 {
            for index in TX4X4_SIZE..8 {
                edges.set_above(index as i32, above_core[TX4X4_SIZE - 1]);
                edges.set_left(index as i32, left_core[TX4X4_SIZE - 1]);
            }
        }
        edges.set_above(8, edges.above(7));
        edges.set_above(9, edges.above(7));
        edges.set_left(8, edges.left(7));
        edges.set_left(9, edges.left(7));
        edges
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

    fn smooth_edges(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> ([Av2Sample; TX4X4_SIZE + 1], [Av2Sample; TX4X4_SIZE + 1]) {
        let edge_sample = |plane, x, y| self.recon_sample(plane, x, y);
        self.smooth_edges_with(
            plane,
            x0,
            y0,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
            &edge_sample,
        )
    }

    fn smooth_edges_for_score(
        &self,
        plane: Av2LosslessPlane,
        x0: usize,
        y0: usize,
        leaf_x0: usize,
        leaf_y0: usize,
        leaf_width: usize,
        leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
    ) -> ([Av2Sample; TX4X4_SIZE + 1], [Av2Sample; TX4X4_SIZE + 1]) {
        let edge_sample =
            |plane, x, y| self.neighbor_sample_for_score(plane, x, y, leaf_x0, leaf_y0);
        self.smooth_edges_with(
            plane,
            x0,
            y0,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
            &edge_sample,
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

    fn neighbor_sample_for_score(
        &self,
        plane: Av2LosslessPlane,
        x: usize,
        y: usize,
        leaf_x0: usize,
        leaf_y0: usize,
    ) -> Av2Sample {
        if x >= leaf_x0 && y >= leaf_y0 {
            self.source_sample(plane, x, y)
        } else {
            self.recon_sample(plane, x, y)
        }
    }

}

const AV2_FAST_LUMA_SAMPLE_GRID: usize = 2;
const AV2_FAST_CHROMA_SAMPLE_GRID: usize = 2;
const AV2_FAST_LUMA_PALETTE_BASE_SCORE: usize = 192;
const AV2_FAST_LUMA_PALETTE_SELECTION_MARGIN: usize = 64;
const AV2_FAST_LUMA_PALETTE_MIN_COMPETING_SCORE: usize = 1536;
const AV2_FAST_LUMA_PALETTE_MAX_LEAF_SIZE: usize = AV2_LUMA_PALETTE_BLOCK_SIZE;
const AV2_FAST_LUMA_PALETTE_SAMPLE_GRID: usize = 2;
const AV2_FAST_LUMA_PALETTE_QUICK_UNIQUE_LIMIT: usize = 8;

pub(crate) fn best_lossless_chroma_candidate_index(
    candidates: &[(bool, Av2ChromaIntraMode, usize)],
    scores: &[usize],
    allow_bdpcm: bool,
) -> Option<usize> {
    debug_assert_eq!(candidates.len(), scores.len());
    candidates
        .iter()
        .enumerate()
        .filter(|(_, &(use_bdpcm, _, _))| allow_bdpcm || !use_bdpcm)
        .min_by_key(|(index, (_, _, syntax_penalty))| scores[*index] + syntax_penalty)
        .map(|(index, _)| index)
}

fn fast_leaf_sample_step(txb_count: usize, sample_grid: usize) -> usize {
    if txb_count <= 2 {
        return txb_count.max(1);
    }
    txb_count.div_ceil(sample_grid).max(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Av2LosslessPlane {
    Y,
    U,
    V,
}

impl Av2LosslessPlane {
    fn planar(self) -> Av2PlanarPlane {
        match self {
            Self::Y => Av2PlanarPlane::Y,
            Self::U => Av2PlanarPlane::U,
            Self::V => Av2PlanarPlane::V,
        }
    }
}
