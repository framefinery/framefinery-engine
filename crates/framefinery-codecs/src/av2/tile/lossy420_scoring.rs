impl<'a> Av2LossySubsampledTileState<'a> {
    fn sampled_luma_regular_q_leaf_score(
        &self,
        decision: Av2TileDecision,
        txb_width: usize,
        txb_height: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        mode: Av2LossySubsampledModeDecision,
        luma_mode_syntax: Av2LumaModeSyntax,
    ) -> usize {
        let mut score =
            lossy_luma_refinement_syntax_penalty(mode.coded_luma_mode(), luma_mode_syntax);
        let mut sampled_txbs = 0usize;
        for row in 0..txb_height {
            for col in 0..txb_width {
                if !lossy_mode_search_samples_txb(row, col, txb_width, txb_height) {
                    continue;
                }
                let (x0, y0) =
                    self.txb_origin(Av2LossyPlane::Y, decision.col + col, decision.row + row);
                let analysis = self.analyze_txb(Av2LossyPlane::Y, x0, y0, mode, context);
                score +=
                    self.regular_q_txb_rd_score(&analysis, Av2CoefficientProxyKind::LumaTransform);
                sampled_txbs += 1;
            }
        }
        lossy_scale_sampled_score(score, txb_width * txb_height, sampled_txbs)
    }

    fn sampled_chroma_regular_q_leaf_score(
        &self,
        chroma_span: Av2ChromaTx4x4Span,
        context: Av2LossyLeafPredictorContext<'_>,
        mode: Av2LossySubsampledModeDecision,
    ) -> usize {
        let mut score =
            lossy_chroma_mode_syntax_penalty(mode.coded_luma_mode(), mode.chroma_intra_mode);
        let mut sampled_txbs = 0usize;
        for plane in [Av2LossyPlane::U, Av2LossyPlane::V] {
            for row in 0..chroma_span.height {
                for col in 0..chroma_span.width {
                    if !lossy_mode_search_samples_txb(
                        row,
                        col,
                        chroma_span.width,
                        chroma_span.height,
                    ) {
                        continue;
                    }
                    let (x0, y0) =
                        self.txb_origin(plane, chroma_span.col + col, chroma_span.row + row);
                    let analysis = self.analyze_txb(plane, x0, y0, mode, context);
                    score += self
                        .regular_q_txb_rd_score(&analysis, Av2CoefficientProxyKind::ChromaTransform);
                    sampled_txbs += 1;
                }
            }
        }
        lossy_scale_sampled_score(
            score,
            chroma_span.width * chroma_span.height * 2,
            sampled_txbs,
        )
    }

    fn regular_q_txb_rd_score(
        &self,
        analysis: &Av2LossyTxbAnalysis,
        kind: Av2CoefficientProxyKind,
    ) -> usize {
        let candidate = choose_regular_q_lossy_txb(
            self.regular_dct_quantized_residual_candidates(analysis),
            kind,
            self.quant_step(),
        );
        let rate = coefficient_proxy_score(&candidate.coefficients, kind);
        lossy_txb_score(
            rate,
            candidate.sse,
            candidate.variance_loss,
            regular_q_rd_quant_step(self.quant_step()),
        )
    }

    fn fsc_leaf_scores(
        &self,
        decision: Av2TileDecision,
        txb_width: usize,
        txb_height: usize,
        luma_context: Av2LossyLeafPredictorContext<'_>,
        chroma_span: Av2ChromaTx4x4Span,
        chroma_leaf_x0: usize,
        chroma_leaf_y0: usize,
        chroma_leaf_width: usize,
        chroma_leaf_height: usize,
        coded_mi_context: &Av2CodedMiContext,
        mode: Av2LossySubsampledModeDecision,
    ) -> (usize, usize) {
        let mut fsc_score = 96usize;
        let mut transform_score = 0usize;
        for row in 0..txb_height {
            for col in 0..txb_width {
                if !lossy_mode_search_samples_txb(row, col, txb_width, txb_height) {
                    continue;
                }
                let (x0, y0) =
                    self.txb_origin(Av2LossyPlane::Y, decision.col + col, decision.row + row);
                let analysis = self.analyze_txb(Av2LossyPlane::Y, x0, y0, mode, luma_context);
                fsc_score += coefficient_proxy_score(
                    &tx4x4_coefficients_from_residual(&analysis.residual, true),
                    Av2CoefficientProxyKind::LumaIdtx,
                );
                transform_score += coefficient_proxy_score(
                    &tx4x4_coefficients_from_residual(&analysis.residual, false),
                    Av2CoefficientProxyKind::LumaTransform,
                );
            }
        }

        let chroma_context = Av2LossyLeafPredictorContext {
            leaf_x0: chroma_leaf_x0,
            leaf_y0: chroma_leaf_y0,
            leaf_width: chroma_leaf_width,
            leaf_height: chroma_leaf_height,
            coded_mi_context,
        };
        for plane in [Av2LossyPlane::U, Av2LossyPlane::V] {
            for row in 0..chroma_span.height {
                for col in 0..chroma_span.width {
                    if !lossy_mode_search_samples_txb(
                        row,
                        col,
                        chroma_span.width,
                        chroma_span.height,
                    ) {
                        continue;
                    }
                    let (x0, y0) =
                        self.txb_origin(plane, chroma_span.col + col, chroma_span.row + row);
                    let analysis = self.analyze_txb(plane, x0, y0, mode, chroma_context);
                    fsc_score += coefficient_proxy_score(
                        &tx4x4_coefficients_from_residual(&analysis.residual, true),
                        Av2CoefficientProxyKind::ChromaTransform,
                    );
                    transform_score += coefficient_proxy_score(
                        &tx4x4_coefficients_from_residual(&analysis.residual, false),
                        Av2CoefficientProxyKind::ChromaTransform,
                    );
                }
            }
        }

        (fsc_score, transform_score)
    }

    fn intra_txb_scores_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        kind: Av2CoefficientProxyKind,
        score_paeth: bool,
    ) -> Av2LossyIntraTxbScores {
        let mut source = [0; TX4X4_SAMPLES];
        let dc = i32::from(self.dc_predictor_for_score(plane, x0, y0, context));
        let mut h_pred = [0; TX4X4_SIZE];
        let mut v_pred = [0; TX4X4_SIZE];
        for index in 0..TX4X4_SIZE {
            h_pred[index] = self.h_predictor_for_score(plane, x0, y0, index, context);
            v_pred[index] = self.v_predictor_for_score(plane, x0, y0, index, context);
        }
        let above_left = if score_paeth {
            self.above_left_predictor_for_score(plane, x0, y0, context)
        } else {
            0
        };
        let mut scores = Av2LossyIntraTxbScores {
            dc: 16,
            horizontal: 16,
            vertical: 16,
            paeth: 16,
            smooth: 0,
            smooth_vertical: 0,
            smooth_horizontal: 0,
        };
        let mut dc_sum = 0i32;
        let mut horizontal_sum = 0i32;
        let mut vertical_sum = 0i32;
        let mut paeth_sum = 0i32;
        let magnitude_scale = residual_sample_proxy_magnitude_scale(kind);
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let sample = i32::from(self.source_sample(plane, x0 + local_x, y0 + local_y));
                source[index] = sample as Av2Sample;
                let horizontal = i32::from(h_pred[local_y]);
                let vertical = i32::from(v_pred[local_x]);
                let dc_diff = sample - dc;
                let horizontal_diff = sample - horizontal;
                let vertical_diff = sample - vertical;
                dc_sum += dc_diff;
                horizontal_sum += horizontal_diff;
                vertical_sum += vertical_diff;
                add_residual_sample_proxy_score(&mut scores.dc, dc_diff, magnitude_scale);
                add_residual_sample_proxy_score(
                    &mut scores.horizontal,
                    horizontal_diff,
                    magnitude_scale,
                );
                add_residual_sample_proxy_score(
                    &mut scores.vertical,
                    vertical_diff,
                    magnitude_scale,
                );
                if score_paeth {
                    let paeth =
                        i32::from(paeth_predictor(h_pred[local_y], v_pred[local_x], above_left));
                    let paeth_diff = sample - paeth;
                    paeth_sum += paeth_diff;
                    add_residual_sample_proxy_score(&mut scores.paeth, paeth_diff, magnitude_scale);
                }
            }
        }
        let max_delta = i32::from(self.bit_depth.max_sample());
        let dc_delta = quantize_i32_to_step(
            round_div_i32(dc_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);
        let horizontal_delta = quantize_i32_to_step(
            round_div_i32(horizontal_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);
        let vertical_delta = quantize_i32_to_step(
            round_div_i32(vertical_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);
        let paeth_delta = if score_paeth {
            quantize_i32_to_step(
                round_div_i32(paeth_sum, TX4X4_SAMPLES as i32),
                lossy_dc_delta_quant_step(self.quant_step()),
            )
            .clamp(-max_delta, max_delta)
        } else {
            0
        };
        let mut dc_sse = 0usize;
        let mut horizontal_sse = 0usize;
        let mut vertical_sse = 0usize;
        let mut paeth_sse = 0usize;
        let max_sample = i32::from(self.bit_depth.max_sample());
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let source = i32::from(source[index]);
                let dc_recon = (dc + dc_delta).clamp(0, max_sample);
                let horizontal_recon =
                    (i32::from(h_pred[local_y]) + horizontal_delta).clamp(0, max_sample);
                let vertical_recon =
                    (i32::from(v_pred[local_x]) + vertical_delta).clamp(0, max_sample);
                let dc_diff = source - dc_recon;
                let horizontal_diff = source - horizontal_recon;
                let vertical_diff = source - vertical_recon;
                dc_sse += (dc_diff * dc_diff) as usize;
                horizontal_sse += (horizontal_diff * horizontal_diff) as usize;
                vertical_sse += (vertical_diff * vertical_diff) as usize;
                if score_paeth {
                    let paeth =
                        i32::from(paeth_predictor(h_pred[local_y], v_pred[local_x], above_left));
                    let paeth_recon = (paeth + paeth_delta).clamp(0, max_sample);
                    let paeth_diff = source - paeth_recon;
                    paeth_sse += (paeth_diff * paeth_diff) as usize;
                }
            }
        }

        scores.dc = lossy_txb_score(scores.dc, dc_sse, 0, self.quant_step());
        scores.horizontal = lossy_txb_score(scores.horizontal, horizontal_sse, 0, self.quant_step());
        scores.vertical = lossy_txb_score(scores.vertical, vertical_sse, 0, self.quant_step());
        if score_paeth {
            scores.paeth = lossy_txb_score(scores.paeth, paeth_sse, 0, self.quant_step());
        }
        scores
    }

    fn smooth_txb_scores_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        kind: Av2CoefficientProxyKind,
    ) -> Av2LossyIntraTxbScores {
        let (smooth_above, smooth_left) = self.smooth_edges_for_score(plane, x0, y0, context);
        let mut source = [0; TX4X4_SAMPLES];
        let mut smooth_pred = [0; TX4X4_SAMPLES];
        let mut smooth_vertical_pred = [0; TX4X4_SAMPLES];
        let mut smooth_horizontal_pred = [0; TX4X4_SAMPLES];
        let mut scores = Av2LossyIntraTxbScores {
            dc: 0,
            horizontal: 0,
            vertical: 0,
            paeth: 0,
            smooth: 16,
            smooth_vertical: 16,
            smooth_horizontal: 16,
        };
        let mut smooth_sum = 0i32;
        let mut smooth_vertical_sum = 0i32;
        let mut smooth_horizontal_sum = 0i32;
        let magnitude_scale = residual_sample_proxy_magnitude_scale(kind);
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let sample = i32::from(self.source_sample(plane, x0 + local_x, y0 + local_y));
                source[index] = sample as Av2Sample;
                let (smooth, smooth_v, smooth_h) = av2_highbd_smooth_intra_predictor_set(
                    smooth_above,
                    smooth_left,
                    local_x,
                    local_y,
                    self.bit_depth,
                );
                smooth_pred[index] = smooth;
                smooth_vertical_pred[index] = smooth_v;
                smooth_horizontal_pred[index] = smooth_h;
                let smooth_diff = sample - i32::from(smooth);
                let smooth_vertical_diff = sample - i32::from(smooth_v);
                let smooth_horizontal_diff = sample - i32::from(smooth_h);
                smooth_sum += smooth_diff;
                smooth_vertical_sum += smooth_vertical_diff;
                smooth_horizontal_sum += smooth_horizontal_diff;
                add_residual_sample_proxy_score(&mut scores.smooth, smooth_diff, magnitude_scale);
                add_residual_sample_proxy_score(
                    &mut scores.smooth_vertical,
                    smooth_vertical_diff,
                    magnitude_scale,
                );
                add_residual_sample_proxy_score(
                    &mut scores.smooth_horizontal,
                    smooth_horizontal_diff,
                    magnitude_scale,
                );
            }
        }

        let max_delta = i32::from(self.bit_depth.max_sample());
        let smooth_delta = quantize_i32_to_step(
            round_div_i32(smooth_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);
        let smooth_vertical_delta = quantize_i32_to_step(
            round_div_i32(smooth_vertical_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);
        let smooth_horizontal_delta = quantize_i32_to_step(
            round_div_i32(smooth_horizontal_sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);

        let mut smooth_sse = 0usize;
        let mut smooth_vertical_sse = 0usize;
        let mut smooth_horizontal_sse = 0usize;
        let max_sample = i32::from(self.bit_depth.max_sample());
        for index in 0..TX4X4_SAMPLES {
            let source = i32::from(source[index]);
            let smooth_recon = (i32::from(smooth_pred[index]) + smooth_delta).clamp(0, max_sample);
            let smooth_vertical_recon =
                (i32::from(smooth_vertical_pred[index]) + smooth_vertical_delta)
                    .clamp(0, max_sample);
            let smooth_horizontal_recon =
                (i32::from(smooth_horizontal_pred[index]) + smooth_horizontal_delta)
                    .clamp(0, max_sample);
            let smooth_diff = source - smooth_recon;
            let smooth_vertical_diff = source - smooth_vertical_recon;
            let smooth_horizontal_diff = source - smooth_horizontal_recon;
            smooth_sse += (smooth_diff * smooth_diff) as usize;
            smooth_vertical_sse += (smooth_vertical_diff * smooth_vertical_diff) as usize;
            smooth_horizontal_sse += (smooth_horizontal_diff * smooth_horizontal_diff) as usize;
        }

        scores.smooth = lossy_txb_score(scores.smooth, smooth_sse, 0, self.quant_step());
        scores.smooth_vertical =
            lossy_txb_score(scores.smooth_vertical, smooth_vertical_sse, 0, self.quant_step());
        scores.smooth_horizontal = lossy_txb_score(
            scores.smooth_horizontal,
            smooth_horizontal_sse,
            0,
            self.quant_step(),
        );
        scores
    }

    fn paeth_txb_score_for_score(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        kind: Av2CoefficientProxyKind,
    ) -> usize {
        let mut source = [0; TX4X4_SAMPLES];
        let mut predictor = [0; TX4X4_SAMPLES];
        let mut score = 16usize;
        let mut sum = 0i32;
        let mut h_pred = [0; TX4X4_SIZE];
        let mut v_pred = [0; TX4X4_SIZE];
        for index in 0..TX4X4_SIZE {
            h_pred[index] = self.h_predictor_for_score(plane, x0, y0, index, context);
            v_pred[index] = self.v_predictor_for_score(plane, x0, y0, index, context);
        }
        let above_left = self.above_left_predictor_for_score(plane, x0, y0, context);
        let magnitude_scale = residual_sample_proxy_magnitude_scale(kind);
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let sample = i32::from(self.source_sample(plane, x0 + local_x, y0 + local_y));
                let paeth = i32::from(paeth_predictor(
                    h_pred[local_y],
                    v_pred[local_x],
                    above_left,
                ));
                let diff = sample - paeth;
                source[index] = sample as Av2Sample;
                predictor[index] = paeth as Av2Sample;
                sum += diff;
                add_residual_sample_proxy_score(&mut score, diff, magnitude_scale);
            }
        }

        let max_delta = i32::from(self.bit_depth.max_sample());
        let delta = quantize_i32_to_step(
            round_div_i32(sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);

        let mut sse = 0usize;
        let max_sample = i32::from(self.bit_depth.max_sample());
        for index in 0..TX4X4_SAMPLES {
            let recon = (i32::from(predictor[index]) + delta).clamp(0, max_sample);
            let diff = i32::from(source[index]) - recon;
            sse += (diff * diff) as usize;
        }

        lossy_txb_score(score, sse, 0, self.quant_step())
    }

    fn directional_txb_score_for_score(
        &self,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        kind: Av2CoefficientProxyKind,
        luma_intra_mode: Av2LumaIntraMode,
    ) -> usize {
        let angle = lossy_luma_idif_angle(luma_intra_mode)
            .expect("directional score is only requested for non-cardinal luma IDIF modes");
        let edge_sample =
            |plane, x, y| self.neighbor_sample_for_score(plane, x, y, context);
        let (constant, edges) = self.luma_directional_idif_predictor_state_with(
            Av2LossyPlane::Y,
            x0,
            y0,
            angle,
            context,
            &edge_sample,
        );
        let mut source = [0; TX4X4_SAMPLES];
        let mut predictor = [0; TX4X4_SAMPLES];
        let mut score = 16usize;
        let mut sum = 0i32;
        let magnitude_scale = residual_sample_proxy_magnitude_scale(kind);
        for local_y in 0..TX4X4_SIZE {
            for local_x in 0..TX4X4_SIZE {
                let index = local_y * TX4X4_SIZE + local_x;
                let sample = i32::from(self.source_sample(Av2LossyPlane::Y, x0 + local_x, y0 + local_y));
                let pred = constant.unwrap_or_else(|| {
                    luma_directional_idif_predictor(
                        angle,
                        edges.expect("IDIF edges are precomputed"),
                        local_x,
                        local_y,
                        self.bit_depth,
                    )
                });
                let diff = sample - i32::from(pred);
                source[index] = sample as Av2Sample;
                predictor[index] = pred;
                sum += diff;
                add_residual_sample_proxy_score(&mut score, diff, magnitude_scale);
            }
        }

        let max_delta = i32::from(self.bit_depth.max_sample());
        let delta = quantize_i32_to_step(
            round_div_i32(sum, TX4X4_SAMPLES as i32),
            lossy_dc_delta_quant_step(self.quant_step()),
        )
        .clamp(-max_delta, max_delta);

        let mut sse = 0usize;
        let max_sample = i32::from(self.bit_depth.max_sample());
        for index in 0..TX4X4_SAMPLES {
            let recon = (i32::from(predictor[index]) + delta).clamp(0, max_sample);
            let diff = i32::from(source[index]) - recon;
            sse += (diff * diff) as usize;
        }

        lossy_txb_score(score, sse, 0, self.quant_step())
    }

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
        let above_core = self.directional_above_edge_with(plane, x0, y0, context, edge_sample);
        let left_core = self.directional_left_edge_with(plane, x0, y0, context, edge_sample);
        let above_left = self.above_left_predictor_with(plane, x0, y0, edge_sample);
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

    fn directional_above_edge_with<EdgeSample>(
        &self,
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        edge_sample: &EdgeSample,
    ) -> [Av2Sample; 8]
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let (plane_width, _) = self.plane_geometry(plane);
        let (plane_region_right, _) = self.plane_region_limit(plane);
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        let have_top = y0 > tile_origin_y;
        let have_left = x0 > tile_origin_x;
        let mut above = [av2_lossless_v_pred_above_edge(self.bit_depth); 8];
        if have_top {
            let plane_sb_width = MVP_SUPERBLOCK_SIZE / sub_x;
            let plane_sb_height = MVP_SUPERBLOCK_SIZE / sub_y;
            let sb_origin_x = (x0 / plane_sb_width) * plane_sb_width;
            let sb_right = (sb_origin_x + plane_sb_width)
                .min(plane_width)
                .min(plane_region_right);
            let superblock_top_row = y0 % plane_sb_height == 0;
            for index in 0..above.len() {
                let x = x0 + index;
                let overhang = index >= TX4X4_SIZE;
                let external_top_right_coded =
                    overhang && y0 == context.leaf_y0 && x < plane_region_right && {
                        let (row_mi, col_mi) =
                            self.coded_mi_for_plane_sample(plane, x, y0 - 1);
                        superblock_top_row
                            || (x < sb_right && context.coded_mi_context.is_coded(row_mi, col_mi))
                    };
                if x < plane_region_right
                    && (!overhang
                        || x < context.leaf_x0 + context.leaf_width
                        || external_top_right_coded)
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
        plane: Av2LossyPlane,
        x0: usize,
        y0: usize,
        context: Av2LossyLeafPredictorContext<'_>,
        edge_sample: &EdgeSample,
    ) -> [Av2Sample; 8]
    where
        EdgeSample: Fn(Av2LossyPlane, usize, usize) -> Av2Sample,
    {
        let (tile_origin_x, tile_origin_y) = self.plane_origin(plane);
        let (_, plane_height) = self.plane_geometry(plane);
        let (_, plane_region_bottom) = self.plane_region_limit(plane);
        let (sub_x, sub_y) = self.plane_subsampling(plane);
        let have_top = y0 > tile_origin_y;
        let have_left = x0 > tile_origin_x;
        let mut left = [av2_lossless_h_pred_left_edge(self.bit_depth); 8];
        if have_left {
            let plane_sb_width = MVP_SUPERBLOCK_SIZE / sub_x;
            let plane_sb_height = MVP_SUPERBLOCK_SIZE / sub_y;
            let sb_origin_y = (y0 / plane_sb_height) * plane_sb_height;
            let sb_bottom = (sb_origin_y + plane_sb_height)
                .min(plane_height)
                .min(plane_region_bottom);
            let superblock_left_col = x0 % plane_sb_width == 0;
            for index in 0..left.len() {
                let y = y0 + index;
                let overhang = index >= TX4X4_SIZE;
                let external_bottom_left_coded =
                    overhang && x0 == context.leaf_x0 && y < sb_bottom && {
                        let (row_mi, col_mi) =
                            self.coded_mi_for_plane_sample(plane, x0 - 1, y);
                        superblock_left_col || context.coded_mi_context.is_coded(row_mi, col_mi)
                    };
                if y < plane_region_bottom
                    && (!overhang
                        || (x0 == context.leaf_x0
                            && (y < context.leaf_y0 + context.leaf_height
                                || external_bottom_left_coded)))
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
            have_top && y0 == context.leaf_y0 && top_right_x < plane_region_right && {
                let (row_mi, col_mi) = self.coded_mi_for_plane_sample(plane, top_right_x, y0 - 1);
                superblock_top_row
                    || (top_right_x < sb_right && context.coded_mi_context.is_coded(row_mi, col_mi))
            };
        if have_top
            && top_right_x < plane_region_right
            && (top_right_x < context.leaf_x0 + context.leaf_width || external_top_right_coded)
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
            have_left && x0 == context.leaf_x0 && bottom_left_y < sb_bottom && {
                let (row_mi, col_mi) =
                    self.coded_mi_for_plane_sample(plane, x0 - 1, bottom_left_y);
                superblock_left_col || context.coded_mi_context.is_coded(row_mi, col_mi)
            };
        if have_left
            && x0 == context.leaf_x0
            && bottom_left_y < plane_region_bottom
            && (bottom_left_y < context.leaf_y0 + context.leaf_height
                || external_bottom_left_coded)
        {
            left[TX4X4_SIZE] = edge_sample(plane, x0 - 1, bottom_left_y);
        } else {
            left[TX4X4_SIZE] = left[TX4X4_SIZE - 1];
        }

        (above, left)
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

    fn quant_step(&self) -> i32 {
        i32::from(self.base_qindex) << u32::from(self.bit_depth.bits() - 8)
    }

    fn base_qindex(&self) -> u16 {
        self.base_qindex
    }

    fn record_leaf(
        &self,
        block_size: Av2MvpBlockSize,
        luma_txbs: usize,
        chroma_txbs: usize,
        mode: Av2LossySubsampledModeDecision,
    ) {
        #[cfg(feature = "av2-lossy-stats")]
        if let Some(stats) = &self.stats {
            stats
                .borrow_mut()
                .record_leaf(block_size, luma_txbs, chroma_txbs, mode);
        }
        #[cfg(not(feature = "av2-lossy-stats"))]
        let _ = (block_size, luma_txbs, chroma_txbs, mode);
    }

    fn record_txb_choice(
        &self,
        plane: Av2LossyPlane,
        choice: &Av2LossyTxbChoice,
        analysis: &Av2LossyTxbAnalysis,
    ) {
        #[cfg(feature = "av2-lossy-stats")]
        if let Some(stats) = &self.stats {
            stats
                .borrow_mut()
                .record_txb_choice(plane, choice, analysis);
        }
        #[cfg(not(feature = "av2-lossy-stats"))]
        let _ = (plane, choice, analysis);
    }
}
