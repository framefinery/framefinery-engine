impl<'a> Av2LossySubsampledTileState<'a> {
    fn analyze_txb(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        mode: Av2LossySubsampledModeDecision,
        context: Av2LossyLeafPredictorContext<'_>,
    ) -> Av2LossyTxbAnalysis {
        let mut source = [0; TX4X4_SAMPLES];
        let mut predictor = [0; TX4X4_SAMPLES];
        let predictor_mode = match plane {
            Av2LossyPlane::Y => chroma_mode_for_luma_mode(mode.luma_intra_mode),
            Av2LossyPlane::U | Av2LossyPlane::V => mode.chroma_intra_mode,
        };
        let dc_pred = if predictor_mode == Av2ChromaIntraMode::Dc {
            self.dc_predictor(plane, x0, y0)
        } else {
            0
        };
        let mut h_pred = [0; TX4X4_SIZE];
        if matches!(
            predictor_mode,
            Av2ChromaIntraMode::Horizontal | Av2ChromaIntraMode::Paeth
        ) {
            for (local_y, pred) in h_pred.iter_mut().enumerate() {
                *pred = self.h_predictor(plane, x0, y0, local_y);
            }
        }
        let mut v_pred = [0; TX4X4_SIZE];
        if matches!(
            predictor_mode,
            Av2ChromaIntraMode::Vertical | Av2ChromaIntraMode::Paeth
        ) {
            for (local_x, pred) in v_pred.iter_mut().enumerate() {
                *pred = self.v_predictor(plane, x0, y0, local_x);
            }
        }
        let above_left = if predictor_mode == Av2ChromaIntraMode::Paeth {
            self.above_left_predictor(plane, x0, y0)
        } else {
            0
        };
        let smooth_edges = matches!(
            predictor_mode,
            Av2ChromaIntraMode::Smooth
                | Av2ChromaIntraMode::SmoothVertical
                | Av2ChromaIntraMode::SmoothHorizontal
        )
        .then(|| self.smooth_edges(plane, x0, y0, context));
        let luma_directional_angle = (plane == Av2LossyPlane::Y)
            .then(|| lossy_luma_idif_angle(mode.luma_intra_mode))
            .flatten();
        let luma_directional_predictor_state = luma_directional_angle.map(|angle| {
            let edge_sample = |plane, x, y| self.recon_sample(plane, x, y);
            let (constant, edges) = self.luma_directional_idif_predictor_state_with(
                plane,
                x0,
                y0,
                angle,
                context,
                &edge_sample,
            );
            (angle, constant, edges)
        });
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let predictor_sample =
                    if let Some((angle, constant, edges)) = luma_directional_predictor_state {
                        constant.unwrap_or_else(|| {
                            luma_directional_idif_predictor(
                                angle,
                                edges.expect("IDIF edges are precomputed"),
                                local_x,
                                local_y,
                                self.bit_depth,
                            )
                        })
                    } else {
                        match predictor_mode {
                            Av2ChromaIntraMode::Dc => dc_pred,
                            Av2ChromaIntraMode::Horizontal => h_pred[local_y],
                            Av2ChromaIntraMode::Vertical => v_pred[local_x],
                            Av2ChromaIntraMode::Paeth => {
                                paeth_predictor(h_pred[local_y], v_pred[local_x], above_left)
                            }
                            Av2ChromaIntraMode::Smooth
                            | Av2ChromaIntraMode::SmoothVertical
                            | Av2ChromaIntraMode::SmoothHorizontal => {
                                let (above, left) =
                                    smooth_edges.expect("smooth edges are precomputed");
                                let (smooth, smooth_v, smooth_h) =
                                    av2_highbd_smooth_intra_predictor_set(
                                        above,
                                        left,
                                        local_x,
                                        local_y,
                                        self.bit_depth,
                                    );
                                match predictor_mode {
                                    Av2ChromaIntraMode::Smooth => smooth,
                                    Av2ChromaIntraMode::SmoothVertical => smooth_v,
                                    Av2ChromaIntraMode::SmoothHorizontal => smooth_h,
                                    _ => unreachable!(
                                        "smooth predictor branch only handles smooth modes"
                                    ),
                                }
                            }
                            _ => unreachable!(
                                "AV2 lossy mode search selects DC, H, V, Paeth, smooth, or luma IDIF"
                            ),
                        }
                    };
                let source_sample = self.source_sample(plane, x0 + local_x, y0 + local_y);
                source[index] = source_sample;
                predictor[index] = predictor_sample;
            }
        }
        self.finish_txb_analysis(source, predictor)
    }

    fn analyze_inter_txb(
        &self,
        reference: &[u8],
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        mv_row_px: i16,
        mv_col_px: i16,
    ) -> Av2LossyTxbAnalysis {
        assert_eq!(
            reference.len(),
            self.source.len(),
            "AV2 lossy inter reference length must match source"
        );
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        debug_assert_eq!(usize::from(mv_col_px.unsigned_abs()) % sub_x, 0);
        debug_assert_eq!(usize::from(mv_row_px.unsigned_abs()) % sub_y, 0);
        let ref_x0 = x0 as isize + isize::from(mv_col_px) / sub_x as isize;
        let ref_y0 = y0 as isize + isize::from(mv_row_px) / sub_y as isize;
        let (plane_width, plane_height) = self.plane_geometry(plane);
        assert!(
            ref_x0 >= 0 && ref_y0 >= 0,
            "AV2 lossy inter reference is out of bounds"
        );
        let ref_x0 = ref_x0 as usize;
        let ref_y0 = ref_y0 as usize;
        assert!(
            ref_x0 + TX4X4_SIZE <= plane_width && ref_y0 + TX4X4_SIZE <= plane_height,
            "AV2 lossy inter reference is out of bounds"
        );

        let mut source = [0; TX4X4_SAMPLES];
        let mut predictor = [0; TX4X4_SAMPLES];
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let source_sample = self.source_sample(plane, x0 + local_x, y0 + local_y);
                let predictor_sample =
                    self.reference_sample(reference, plane, ref_x0 + local_x, ref_y0 + local_y);
                source[index] = source_sample;
                predictor[index] = predictor_sample;
            }
        }
        self.finish_txb_analysis(source, predictor)
    }

    fn finish_txb_analysis(
        &self,
        source: [Av2Sample; TX4X4_SAMPLES],
        predictor: [Av2Sample; TX4X4_SAMPLES],
    ) -> Av2LossyTxbAnalysis {
        let mut residual = [0i32; TX4X4_SAMPLES];
        let mut sum = 0i32;
        for index in 0..TX4X4_SAMPLES {
            let diff = i32::from(source[index]) - i32::from(predictor[index]);
            residual[index] = diff;
            sum += diff;
        }
        let average = round_div_i32(sum, TX4X4_SAMPLES as i32);
        let max_delta = i32::from(self.bit_depth.max_sample());
        let delta = quantize_i32_to_step(average, lossy_dc_delta_quant_step(self.quant_step()))
            .clamp(-max_delta, max_delta) as i16;
        let source_variance = txb_source_variance(&source);
        let (dc_sse, dc_variance_loss) = txb_dc_recon_distortion_with_source_variance(
            &source,
            &predictor,
            delta,
            self.bit_depth,
            source_variance,
        );
        Av2LossyTxbAnalysis {
            source,
            predictor,
            residual,
            delta,
            dc_sse,
            dc_variance_loss,
            source_variance,
        }
    }
}
