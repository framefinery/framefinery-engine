impl<'a> Av2LossySubsampledTileState<'a> {
    fn chroma_444_intra_tx8x8_analysis(
        &self,
        plane: Av2LossyPlane,
        chroma_span: Av2ChromaTx4x4Span,
        mode: Av2LossySubsampledModeDecision,
    ) -> Av2LossyTx8x8Analysis {
        let plan = self.chroma_444_intra_predictor_plan(plane, chroma_span, mode);
        self.chroma_444_tx8x8_analysis(
            plane,
            plan.leaf_x0,
            plan.leaf_y0,
            plan.visible_width,
            plan.visible_height,
            |state, local_x, local_y| plan.sample(state, plane, local_x, local_y),
        )
    }

    fn fill_chroma_444_tx8x8_leaf(
        &mut self,
        plane: Av2LossyPlane,
        analysis: &Av2LossyTx8x8Analysis,
        residual: &[i32; TX8X8_SAMPLES],
    ) {
        self.fill_chroma_leaf_with(plane, analysis, residual, TX8X8_SIZE);
    }

    fn fill_chroma_422_tx4x8_leaf(
        &mut self,
        plane: Av2LossyPlane,
        analysis: &Av2LossyTx8x8Analysis,
        residual: &[i32; TX4X8_SAMPLES],
    ) {
        debug_assert!(analysis.visible_width <= TX4X8_WIDTH);
        debug_assert!(analysis.visible_height <= TX4X8_HEIGHT);
        self.fill_chroma_leaf_with(plane, analysis, residual, TX4X8_WIDTH);
    }

    fn fill_chroma_leaf_with(
        &mut self,
        plane: Av2LossyPlane,
        analysis: &Av2LossyTx8x8Analysis,
        residual: &[i32],
        residual_stride: usize,
    ) {
        let max_sample = i32::from(self.bit_depth.max_sample());
        for local_y in 0..analysis.visible_height {
            let y = analysis.leaf_y0 + local_y;
            for local_x in 0..analysis.visible_width {
                let x = analysis.leaf_x0 + local_x;
                let analysis_index = local_y * TX8X8_SIZE + local_x;
                let residual_index = local_y * residual_stride + local_x;
                let predictor = i32::from(analysis.predictor[analysis_index]);
                let sample = (predictor + residual[residual_index]).clamp(0, max_sample) as Av2Sample;
                self.set_recon_sample(plane, x, y, sample);
            }
        }
    }

    fn chroma_444_intra_predictor_plan(
        &self,
        plane: Av2LossyPlane,
        chroma_span: Av2ChromaTx4x4Span,
        mode: Av2LossySubsampledModeDecision,
    ) -> Av2Chroma444IntraPredictorPlan {
        debug_assert!(matches!(plane, Av2LossyPlane::U | Av2LossyPlane::V));
        let (leaf_x0, leaf_y0) = self.txb_origin(plane, chroma_span.col, chroma_span.row);
        let width = chroma_span.width * TX4X4_SIZE;
        let height = chroma_span.height * TX4X4_SIZE;
        let (plane_width, plane_height) = self.plane_geometry(plane);
        let visible_width = width.min(plane_width.saturating_sub(leaf_x0)).min(TX8X8_SIZE);
        let visible_height = height
            .min(plane_height.saturating_sub(leaf_y0))
            .min(TX8X8_SIZE);
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let have_left = leaf_x0 > tile_origin_x;
        let have_top = leaf_y0 > tile_origin_y;
        let mode = mode.chroma_intra_mode;

        let dc_pred = if mode == Av2ChromaIntraMode::Dc {
            if !have_left && !have_top {
                av2_lossless_dc_predictor(self.bit_depth)
            } else {
                let mut sum = 0u32;
                let mut count = 0u32;
                if have_top {
                    for x in leaf_x0..(leaf_x0 + visible_width) {
                        let sample = self.recon_sample(plane, x, leaf_y0 - 1);
                        sum += u32::from(sample);
                        count += 1;
                    }
                }
                if have_left {
                    for y in leaf_y0..(leaf_y0 + visible_height) {
                        let sample = self.recon_sample(plane, leaf_x0 - 1, y);
                        sum += u32::from(sample);
                        count += 1;
                    }
                }
                if count == 0 {
                    av2_lossless_dc_predictor(self.bit_depth)
                } else {
                    av2_reference_dc_average(sum, count)
                }
            }
        } else {
            0
        };
        let above_left = if mode == Av2ChromaIntraMode::Paeth {
            if have_left && have_top {
                self.recon_sample(plane, leaf_x0 - 1, leaf_y0 - 1)
            } else if have_top {
                self.recon_sample(plane, leaf_x0, leaf_y0 - 1)
            } else if have_left {
                self.recon_sample(plane, leaf_x0 - 1, leaf_y0)
            } else {
                av2_lossless_dc_predictor(self.bit_depth)
            }
        } else {
            0
        };

        Av2Chroma444IntraPredictorPlan {
            leaf_x0,
            leaf_y0,
            visible_width,
            visible_height,
            have_left,
            have_top,
            dc_pred,
            above_left,
            mode,
            bit_depth: self.bit_depth,
        }
    }

    fn chroma_444_inter_tx8x8_analysis(
        &self,
        reference: &[u8],
        plane: Av2LossyPlane,
        chroma_span: Av2ChromaTx4x4Span,
        mv_row_px: i16,
        mv_col_px: i16,
    ) -> Av2LossyTx8x8Analysis {
        let (leaf_x0, leaf_y0) = self.txb_origin(plane, chroma_span.col, chroma_span.row);
        let (visible_width, visible_height) =
            self.chroma_444_leaf_visible_size(plane, leaf_x0, leaf_y0, chroma_span);
        let (ref_x0, ref_y0) = self.chroma_444_inter_reference_origin(
            reference,
            plane,
            leaf_x0,
            leaf_y0,
            visible_width,
            visible_height,
            mv_row_px,
            mv_col_px,
        );
        self.chroma_444_tx8x8_analysis(
            plane,
            leaf_x0,
            leaf_y0,
            visible_width,
            visible_height,
            |state, local_x, local_y| {
                state.reference_sample(reference, plane, ref_x0 + local_x, ref_y0 + local_y)
            },
        )
    }

    fn chroma_444_leaf_visible_size(
        &self,
        plane: Av2LossyPlane,
        leaf_x0: usize,
        leaf_y0: usize,
        chroma_span: Av2ChromaTx4x4Span,
    ) -> (usize, usize) {
        let width = (chroma_span.width * TX4X4_SIZE).min(TX8X8_SIZE);
        let height = (chroma_span.height * TX4X4_SIZE).min(TX8X8_SIZE);
        let (plane_width, plane_height) = self.plane_geometry(plane);
        (
            width.min(plane_width.saturating_sub(leaf_x0)),
            height.min(plane_height.saturating_sub(leaf_y0)),
        )
    }

    fn chroma_444_inter_reference_origin(
        &self,
        reference: &[u8],
        plane: Av2LossyPlane,
        leaf_x0: usize,
        leaf_y0: usize,
        visible_width: usize,
        visible_height: usize,
        mv_row_px: i16,
        mv_col_px: i16,
    ) -> (usize, usize) {
        assert_eq!(
            reference.len(),
            self.source.len(),
            "AV2 lossy inter reference length must match source"
        );
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        debug_assert_eq!(usize::from(mv_col_px.unsigned_abs()) % sub_x, 0);
        debug_assert_eq!(usize::from(mv_row_px.unsigned_abs()) % sub_y, 0);
        let ref_x0 = leaf_x0 as isize + isize::from(mv_col_px) / sub_x as isize;
        let ref_y0 = leaf_y0 as isize + isize::from(mv_row_px) / sub_y as isize;
        let (plane_width, plane_height) = self.plane_geometry(plane);
        assert!(
            ref_x0 >= 0 && ref_y0 >= 0,
            "AV2 lossy inter reference is out of bounds"
        );
        let ref_x0 = ref_x0 as usize;
        let ref_y0 = ref_y0 as usize;
        assert!(
            ref_x0 + visible_width <= plane_width && ref_y0 + visible_height <= plane_height,
            "AV2 lossy inter reference is out of bounds"
        );
        (ref_x0, ref_y0)
    }

    fn chroma_444_tx8x8_analysis<Predictor>(
        &self,
        plane: Av2LossyPlane,
        leaf_x0: usize,
        leaf_y0: usize,
        visible_width: usize,
        visible_height: usize,
        predictor: Predictor,
    ) -> Av2LossyTx8x8Analysis
    where
        Predictor: Fn(&Self, usize, usize) -> Av2Sample,
    {
        debug_assert!(matches!(plane, Av2LossyPlane::U | Av2LossyPlane::V));
        let mut predictor_block = [0; TX8X8_SAMPLES];
        let mut residual = [0i32; TX8X8_SAMPLES];
        for local_y in 0..TX8X8_SIZE {
            for local_x in 0..TX8X8_SIZE {
                let index = local_y * TX8X8_SIZE + local_x;
                if local_x < visible_width && local_y < visible_height {
                    let pred = predictor(self, local_x, local_y);
                    let source = self.source_sample(plane, leaf_x0 + local_x, leaf_y0 + local_y);
                    predictor_block[index] = pred;
                    residual[index] = i32::from(source) - i32::from(pred);
                }
            }
        }

        Av2LossyTx8x8Analysis {
            leaf_x0,
            leaf_y0,
            visible_width,
            visible_height,
            predictor: predictor_block,
            residual,
        }
    }
}
