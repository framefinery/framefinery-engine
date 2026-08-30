impl<'a> Av2LosslessSubsampledTileState<'a> {
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
}
