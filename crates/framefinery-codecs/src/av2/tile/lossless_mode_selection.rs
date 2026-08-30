impl<'a> Av2LosslessSubsampledTileState<'a> {
    fn mode_decision_for_leaf(
        &self,
        decision: Av2TileDecision,
        visible_rows_mi: usize,
        visible_cols_mi: usize,
        coded_mi_context: &Av2CodedMiContext,
        palette: Option<&Av2LumaPalette444>,
    ) -> Av2LosslessSubsampledModeDecision {
        if self.mode_search == Av2LosslessSubsampledModeSearch::FastScreenContent {
            return self.fast_mode_decision_for_leaf(
                decision,
                visible_rows_mi,
                visible_cols_mi,
                coded_mi_context,
                palette,
            );
        }
        let txb_width = decision
            .block_size
            .tx4x4_width()
            .min(visible_cols_mi.saturating_sub(decision.col));
        let txb_height = decision
            .block_size
            .tx4x4_height()
            .min(visible_rows_mi.saturating_sub(decision.row));
        let chroma_span = chroma_tx4x4_span(
            decision,
            visible_rows_mi,
            visible_cols_mi,
            self.chroma_format,
        );
        // The AVM path validates 32-wide/high transform-coded leaves, but
        // 32-wide/high FSC/IDTX leaves can corrupt natural-content tiles with
        // the current coefficient writer. Keep FSC to smaller leaves until the
        // 32xN IDTX path is audited end to end.
        let fsc_allowed = decision.block_size.fsc_size_group().is_some()
            && decision.block_size.width < 32
            && decision.block_size.height < 32;
        let mut best = (Av2LosslessSubsampledModeDecision::default(), usize::MAX);

        for use_fsc in [false, true] {
            if use_fsc && !fsc_allowed {
                continue;
            }
            let luma_candidates = [
                (Av2LumaIntraMode::Dc, None, 0usize),
                (Av2LumaIntraMode::Smooth, None, 192usize),
                (Av2LumaIntraMode::SmoothVertical, None, 192usize),
                (Av2LumaIntraMode::SmoothHorizontal, None, 192usize),
                (Av2LumaIntraMode::Paeth, None, 128usize),
                (Av2LumaIntraMode::Directional45, None, 192usize),
                (Av2LumaIntraMode::Directional67, None, 192usize),
                (Av2LumaIntraMode::Horizontal, None, 32usize),
                (Av2LumaIntraMode::Vertical, None, 32usize),
                (Av2LumaIntraMode::Directional113, None, 192usize),
                (Av2LumaIntraMode::Directional135, None, 192usize),
                (Av2LumaIntraMode::Directional157, None, 192usize),
                (Av2LumaIntraMode::Directional203, None, 192usize),
                (Av2LumaIntraMode::Horizontal, Some(true), 64usize),
                (Av2LumaIntraMode::Vertical, Some(false), 64usize),
            ];
            // Chroma BDPCM is reference-clean in the transform-coded base
            // mode search. FSC/IDTX pairings and luma directional-delta
            // pairings diverge from AVM on natural 4:2:0 content.
            let chroma_bdpcm_allowed = !use_fsc;
            let chroma_candidates = [
                (false, Av2ChromaIntraMode::Horizontal, 0usize),
                (false, Av2ChromaIntraMode::Vertical, 0usize),
                (false, Av2ChromaIntraMode::Dc, 0usize),
                (false, Av2ChromaIntraMode::Directional45, 192usize),
                (false, Av2ChromaIntraMode::Directional135, 192usize),
                (false, Av2ChromaIntraMode::Directional67, 192usize),
                (false, Av2ChromaIntraMode::Directional203, 192usize),
                (false, Av2ChromaIntraMode::Directional113, 192usize),
                (false, Av2ChromaIntraMode::Directional157, 192usize),
                (false, Av2ChromaIntraMode::Smooth, 192usize),
                (false, Av2ChromaIntraMode::SmoothVertical, 192usize),
                (false, Av2ChromaIntraMode::SmoothHorizontal, 192usize),
                (false, Av2ChromaIntraMode::Paeth, 128usize),
                (true, Av2ChromaIntraMode::Horizontal, 64usize),
                (true, Av2ChromaIntraMode::Vertical, 64usize),
            ];
            let chroma_scores = chroma_candidates.map(
                |(chroma_use_bdpcm, chroma_intra_mode, _)| {
                    if chroma_use_bdpcm && !chroma_bdpcm_allowed {
                        return 0;
                    }
                    self.chroma_leaf_coefficient_score(
                        chroma_span,
                        Av2LosslessSubsampledModeDecision {
                            luma_intra_mode: Av2LumaIntraMode::Dc,
                            luma_bdpcm_horz: None,
                            chroma_use_bdpcm,
                            chroma_intra_mode,
                            use_luma_palette: false,
                            use_fsc,
                        },
                        coded_mi_context,
                    )
                },
            );
            // The luma and chroma terms are independent in this heuristic:
            // the combined score contains no luma/chroma cross-term. Select
            // the best chroma term once instead of evaluating the full
            // Cartesian product for every luma candidate. Keep the first
            // equal-cost entry so the former iteration-order tie break is
            // unchanged.
            let best_chroma = best_lossless_chroma_candidate_index(
                &chroma_candidates,
                &chroma_scores,
                chroma_bdpcm_allowed,
            );
            for (luma_intra_mode, luma_bdpcm_horz, luma_syntax_penalty) in luma_candidates {
                let luma_score = self.luma_leaf_coefficient_score(
                    decision,
                    txb_width,
                    txb_height,
                    luma_intra_mode,
                    luma_bdpcm_horz,
                    use_fsc,
                    coded_mi_context,
                );
                if let Some(index) = best_chroma {
                    let (chroma_use_bdpcm, chroma_intra_mode, chroma_syntax_penalty) =
                        chroma_candidates[index];
                    let mode = Av2LosslessSubsampledModeDecision {
                        luma_intra_mode,
                        luma_bdpcm_horz,
                        chroma_use_bdpcm,
                        chroma_intra_mode,
                        use_luma_palette: false,
                        use_fsc,
                    };
                    let fsc_syntax_penalty = usize::from(use_fsc) * 96;
                    let score = luma_score + chroma_scores[index]
                            + luma_syntax_penalty
                            + chroma_syntax_penalty
                            + fsc_syntax_penalty;
                    if score < best.1 {
                        best = (mode, score);
                    }
                }
            }

            let best_non_bdpcm_chroma = best_lossless_chroma_candidate_index(
                &chroma_candidates,
                &chroma_scores,
                false,
            );

            for base in [
                Av2LumaDirectionalMode::Directional45,
                Av2LumaDirectionalMode::Directional67,
                Av2LumaDirectionalMode::Vertical,
                Av2LumaDirectionalMode::Directional113,
                Av2LumaDirectionalMode::Directional135,
                Av2LumaDirectionalMode::Directional157,
                Av2LumaDirectionalMode::Horizontal,
                Av2LumaDirectionalMode::Directional203,
            ] {
                for delta in [-1i8, 1, -2, 2, -3, 3] {
                    let luma_intra_mode = Av2LumaIntraMode::DirectionalDelta { base, delta };
                    let luma_syntax_penalty = 224usize + usize::from(delta.unsigned_abs()) * 48;
                    let luma_score = self.luma_leaf_coefficient_score(
                        decision,
                        txb_width,
                        txb_height,
                        luma_intra_mode,
                        None,
                        use_fsc,
                        coded_mi_context,
                    );
                    if let Some(index) = best_non_bdpcm_chroma {
                        let (_, chroma_intra_mode, chroma_syntax_penalty) = chroma_candidates[index];
                        let mode = Av2LosslessSubsampledModeDecision {
                            luma_intra_mode,
                            luma_bdpcm_horz: None,
                            chroma_use_bdpcm: false,
                            chroma_intra_mode,
                            use_luma_palette: false,
                            use_fsc,
                        };
                        let fsc_syntax_penalty = usize::from(use_fsc) * 96;
                        let score = luma_score
                            + chroma_scores[index]
                            + luma_syntax_penalty
                            + chroma_syntax_penalty
                            + fsc_syntax_penalty;
                        if score < best.1 {
                            best = (mode, score);
                        }
                    }
                }
            }
        }

        best.0
    }

    fn fast_mode_decision_for_leaf(
        &self,
        decision: Av2TileDecision,
        visible_rows_mi: usize,
        visible_cols_mi: usize,
        coded_mi_context: &Av2CodedMiContext,
        palette: Option<&Av2LumaPalette444>,
    ) -> Av2LosslessSubsampledModeDecision {
        let txb_width = decision
            .block_size
            .tx4x4_width()
            .min(visible_cols_mi.saturating_sub(decision.col));
        let txb_height = decision
            .block_size
            .tx4x4_height()
            .min(visible_rows_mi.saturating_sub(decision.row));
        let chroma_span = chroma_tx4x4_span(
            decision,
            visible_rows_mi,
            visible_cols_mi,
            self.chroma_format,
        );
        let mut mode = Av2LosslessSubsampledModeDecision::default();

        let luma_candidates = [
            (Av2LumaIntraMode::Dc, None, 0usize),
            (Av2LumaIntraMode::Horizontal, None, 32usize),
            (Av2LumaIntraMode::Vertical, None, 32usize),
            (Av2LumaIntraMode::Horizontal, Some(true), 64usize),
            (Av2LumaIntraMode::Vertical, Some(false), 64usize),
        ];
        let luma_scores =
            self.fast_luma_leaf_sampled_dc_h_v_bdpcm_scores(decision, txb_width, txb_height);
        let mut best_luma = (mode.luma_intra_mode, mode.luma_bdpcm_horz, usize::MAX);
        for (luma_intra_mode, luma_bdpcm_horz, syntax_penalty) in luma_candidates {
            let score = match (luma_intra_mode, luma_bdpcm_horz) {
                (Av2LumaIntraMode::Dc, None) => luma_scores.dc,
                (Av2LumaIntraMode::Horizontal, None) => luma_scores.horizontal,
                (Av2LumaIntraMode::Vertical, None) => luma_scores.vertical,
                (Av2LumaIntraMode::Horizontal, Some(true)) => luma_scores.bdpcm_horizontal,
                (Av2LumaIntraMode::Vertical, Some(false)) => luma_scores.bdpcm_vertical,
                _ => unreachable!("fast luma mode search only scores DC/H/V and BDPCM"),
            } + syntax_penalty;
            if score < best_luma.2 {
                best_luma = (luma_intra_mode, luma_bdpcm_horz, score);
            }
        }
        mode.luma_intra_mode = best_luma.0;
        mode.luma_bdpcm_horz = best_luma.1;
        if let Some(palette) = palette {
            if self.fast_luma_palette_leaf_is_worthy(decision, txb_width, txb_height, best_luma.2)
            {
                let palette_score = self.fast_luma_palette_leaf_score(
                    decision,
                    txb_width,
                    txb_height,
                    palette,
                    coded_mi_context,
                );
                if palette_score + AV2_FAST_LUMA_PALETTE_SELECTION_MARGIN < best_luma.2 {
                    mode.luma_intra_mode = Av2LumaIntraMode::Dc;
                    mode.luma_bdpcm_horz = None;
                    mode.use_luma_palette = true;
                    mode.use_fsc = false;
                }
            }
        }

        let chroma_candidates = [
            (false, Av2ChromaIntraMode::Horizontal, 0usize),
            (false, Av2ChromaIntraMode::Vertical, 0usize),
            (false, Av2ChromaIntraMode::Dc, 0usize),
            (true, Av2ChromaIntraMode::Horizontal, 64usize),
            (true, Av2ChromaIntraMode::Vertical, 64usize),
        ];
        let chroma_scores =
            self.fast_chroma_leaf_sampled_dc_h_v_bdpcm_scores(chroma_span);
        let mut best_chroma = (mode.chroma_use_bdpcm, mode.chroma_intra_mode, usize::MAX);
        for (chroma_use_bdpcm, chroma_intra_mode, syntax_penalty) in chroma_candidates {
            let score = match (chroma_use_bdpcm, chroma_intra_mode) {
                (false, Av2ChromaIntraMode::Horizontal) => chroma_scores.horizontal,
                (false, Av2ChromaIntraMode::Vertical) => chroma_scores.vertical,
                (false, Av2ChromaIntraMode::Dc) => chroma_scores.dc,
                (true, Av2ChromaIntraMode::Horizontal) => chroma_scores.bdpcm_horizontal,
                (true, Av2ChromaIntraMode::Vertical) => chroma_scores.bdpcm_vertical,
                _ => unreachable!("fast chroma mode search only scores DC/H/V and BDPCM"),
            } + syntax_penalty;
            if score < best_chroma.2 {
                best_chroma = (chroma_use_bdpcm, chroma_intra_mode, score);
            }
        }
        mode.chroma_use_bdpcm = best_chroma.0;
        mode.chroma_intra_mode = best_chroma.1;
        mode
    }

    fn fast_luma_palette_leaf_is_worthy(
        &self,
        decision: Av2TileDecision,
        txb_width: usize,
        txb_height: usize,
        competing_luma_score: usize,
    ) -> bool {
        if competing_luma_score < AV2_FAST_LUMA_PALETTE_MIN_COMPETING_SCORE {
            return false;
        }
        let leaf_width = txb_width * TX4X4_SIZE;
        let leaf_height = txb_height * TX4X4_SIZE;
        if leaf_width < AV2_LUMA_PALETTE_BLOCK_SIZE
            || leaf_height < AV2_LUMA_PALETTE_BLOCK_SIZE
            || leaf_width > AV2_FAST_LUMA_PALETTE_MAX_LEAF_SIZE
            || leaf_height > AV2_FAST_LUMA_PALETTE_MAX_LEAF_SIZE
        {
            return false;
        }
        let normalize_shift = self.bit_depth.bits().saturating_sub(8);
        let row_step = fast_leaf_sample_step(txb_height, AV2_FAST_LUMA_PALETTE_SAMPLE_GRID);
        let col_step = fast_leaf_sample_step(txb_width, AV2_FAST_LUMA_PALETTE_SAMPLE_GRID);
        let mut values = [0u16; AV2_FAST_LUMA_PALETTE_QUICK_UNIQUE_LIMIT + 1];
        let mut unique = 0usize;
        for row in (0..txb_height).step_by(row_step) {
            for col in (0..txb_width).step_by(col_step) {
                let (sample_x, sample_y) =
                    self.txb_origin(Av2LosslessPlane::Y, decision.col + col, decision.row + row);
                let sample = self.source_sample(Av2LosslessPlane::Y, sample_x, sample_y)
                    >> normalize_shift;
                if values[..unique].contains(&sample) {
                    continue;
                }
                if unique == values.len() {
                    return false;
                }
                values[unique] = sample;
                unique += 1;
            }
        }
        (2..=AV2_FAST_LUMA_PALETTE_QUICK_UNIQUE_LIMIT).contains(&unique)
    }

    fn fast_luma_palette_leaf_score(
        &self,
        decision: Av2TileDecision,
        txb_width: usize,
        txb_height: usize,
        palette: &Av2LumaPalette444,
        _coded_mi_context: &Av2CodedMiContext,
    ) -> usize {
        let (leaf_x0, leaf_y0) = self.txb_origin(Av2LosslessPlane::Y, decision.col, decision.row);
        let leaf_width = txb_width * TX4X4_SIZE;
        let leaf_height = txb_height * TX4X4_SIZE;
        let region = palette.syntax_region_palette(leaf_x0, leaf_y0, leaf_width, leaf_height);
        let vertical_scan = choose_luma_palette_map_vertical_for_region(
            palette,
            &region,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
        );
        let map_score = luma_palette_color_map_rate_q8(
            palette,
            &region,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            vertical_scan,
        ) as usize
            / 64;
        let mut score = AV2_FAST_LUMA_PALETTE_BASE_SCORE
            + region.color_count() * usize::from(self.bit_depth.bits()) * 4
            + map_score;
        for row in 0..txb_height {
            let abs_row = decision.row + row;
            for col in 0..txb_width {
                let abs_col = decision.col + col;
                let (x0, y0) = self.txb_origin(Av2LosslessPlane::Y, abs_col, abs_row);
                let residual = self.luma_palette_residual4x4(palette, &region, x0, y0);
                let coefficients = av2_fwht4x4(&residual);
                score += coefficient_proxy_score(&coefficients, Av2CoefficientProxyKind::LumaTransform);
            }
        }
        score
    }

    fn fast_luma_leaf_sampled_dc_h_v_bdpcm_scores(
        &self,
        decision: Av2TileDecision,
        txb_width: usize,
        txb_height: usize,
    ) -> Av2DcHvBdpcmTxbScores {
        let mut scores = Av2DcHvBdpcmTxbScores::default();
        let (leaf_x0, leaf_y0) = self.txb_origin(Av2LosslessPlane::Y, decision.col, decision.row);
        let row_step = fast_leaf_sample_step(txb_height, AV2_FAST_LUMA_SAMPLE_GRID);
        let col_step = fast_leaf_sample_step(txb_width, AV2_FAST_LUMA_SAMPLE_GRID);
        for row in (0..txb_height).step_by(row_step) {
            let abs_row = decision.row + row;
            for col in (0..txb_width).step_by(col_step) {
                let abs_col = decision.col + col;
                let (x0, y0) = self.txb_origin(Av2LosslessPlane::Y, abs_col, abs_row);
                scores.add_assign(self.dc_h_v_bdpcm_txb_scores_for_score(
                    Av2LosslessPlane::Y,
                    x0,
                    y0,
                    leaf_x0,
                    leaf_y0,
                    Av2CoefficientProxyKind::LumaTransform,
                ));
            }
        }
        scores
    }

    fn fast_chroma_leaf_sampled_dc_h_v_bdpcm_scores(
        &self,
        chroma_span: Av2ChromaTx4x4Span,
    ) -> Av2DcHvBdpcmTxbScores {
        let mut scores = Av2DcHvBdpcmTxbScores::default();
        let row_step = fast_leaf_sample_step(chroma_span.height, AV2_FAST_CHROMA_SAMPLE_GRID);
        let col_step = fast_leaf_sample_step(chroma_span.width, AV2_FAST_CHROMA_SAMPLE_GRID);
        for plane in [Av2LosslessPlane::U, Av2LosslessPlane::V] {
            let (leaf_x0, leaf_y0) = self.txb_origin(plane, chroma_span.col, chroma_span.row);
            for row in (0..chroma_span.height).step_by(row_step) {
                let abs_row = chroma_span.row + row;
                for col in (0..chroma_span.width).step_by(col_step) {
                    let abs_col = chroma_span.col + col;
                    let (x0, y0) = self.txb_origin(plane, abs_col, abs_row);
                    scores.add_assign(self.dc_h_v_bdpcm_txb_scores_for_score(
                        plane,
                        x0,
                        y0,
                        leaf_x0,
                        leaf_y0,
                        Av2CoefficientProxyKind::ChromaTransform,
                    ));
                }
            }
        }
        scores
    }

    fn luma_leaf_coefficient_score(
        &self,
        decision: Av2TileDecision,
        txb_width: usize,
        txb_height: usize,
        luma_intra_mode: Av2LumaIntraMode,
        luma_bdpcm_horz: Option<bool>,
        use_fsc: bool,
        coded_mi_context: &Av2CodedMiContext,
    ) -> usize {
        let mut score = 0usize;
        let mode = Av2LosslessSubsampledModeDecision {
            luma_intra_mode,
            luma_bdpcm_horz,
            chroma_use_bdpcm: false,
            chroma_intra_mode: Av2ChromaIntraMode::Dc,
            use_luma_palette: false,
            use_fsc,
        };
        let (leaf_x0, leaf_y0) = self.txb_origin(Av2LosslessPlane::Y, decision.col, decision.row);
        let leaf_width = txb_width * TX4X4_SIZE;
        let leaf_height = txb_height * TX4X4_SIZE;
        for row in 0..txb_height {
            let abs_row = decision.row + row;
            for col in 0..txb_width {
                let abs_col = decision.col + col;
                let (x0, y0) = self.txb_origin(Av2LosslessPlane::Y, abs_col, abs_row);
                let coefficients = self.tx4x4_coefficients_for_mode_score(
                    Av2LosslessPlane::Y,
                    x0,
                    y0,
                    mode,
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    leaf_height,
                    coded_mi_context,
                );
                let kind = if mode.use_fsc {
                    Av2CoefficientProxyKind::LumaIdtx
                } else {
                    Av2CoefficientProxyKind::LumaTransform
                };
                score += coefficient_proxy_score(&coefficients, kind);
            }
        }
        score
    }

    fn chroma_leaf_coefficient_score(
        &self,
        chroma_span: Av2ChromaTx4x4Span,
        mode: Av2LosslessSubsampledModeDecision,
        coded_mi_context: &Av2CodedMiContext,
    ) -> usize {
        let mut score = 0usize;
        for plane in [Av2LosslessPlane::U, Av2LosslessPlane::V] {
            let (leaf_x0, leaf_y0) = self.txb_origin(plane, chroma_span.col, chroma_span.row);
            let leaf_width = chroma_span.width * TX4X4_SIZE;
            let leaf_height = chroma_span.height * TX4X4_SIZE;
            for row in 0..chroma_span.height {
                let abs_row = chroma_span.row + row;
                for col in 0..chroma_span.width {
                    let abs_col = chroma_span.col + col;
                    let (x0, y0) = self.txb_origin(plane, abs_col, abs_row);
                    let coefficients = self.tx4x4_coefficients_for_mode_score(
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
                    score += coefficient_proxy_score(
                        &coefficients,
                        Av2CoefficientProxyKind::ChromaTransform,
                    );
                }
            }
        }
        score
    }

    fn copy_source_to_recon_txb(&mut self, plane: Av2LosslessPlane, x0: usize, y0: usize) {
        if self.source_backed_recon {
            return;
        }
        let (plane_width, plane_height) = self.plane_geometry(plane);
        let bytes_per_sample = self.bit_depth.bytes_per_sample();
        for local_y in 0..TX4X4_SIZE {
            let y = y0 + local_y;
            if y >= plane_height {
                continue;
            }
            let row_samples = TX4X4_SIZE.min(plane_width.saturating_sub(x0));
            let offset = self.offset(plane, x0, y) * bytes_per_sample;
            let row_bytes = row_samples * bytes_per_sample;
            self.recon[offset..offset + row_bytes]
                .copy_from_slice(&self.source[offset..offset + row_bytes]);
        }
    }

    fn copy_source_to_recon_region(&mut self) {
        if self.source_backed_recon && self.recon.is_empty() {
            return;
        }
        for plane in [Av2LosslessPlane::Y, Av2LosslessPlane::U, Av2LosslessPlane::V] {
            self.copy_source_to_recon_plane_region(plane);
        }
    }

    fn copy_source_to_recon_plane_region(&mut self, plane: Av2LosslessPlane) {
        if self.source_backed_recon && self.recon.is_empty() {
            return;
        }
        let (origin_x, origin_y) = self.plane_origin(plane);
        let (end_x, end_y) = self.layout.clipped_plane_region_limit(plane.planar());
        if origin_x >= end_x || origin_y >= end_y {
            return;
        }
        let bytes_per_sample = self.bit_depth.bytes_per_sample();
        let row_bytes = (end_x - origin_x) * bytes_per_sample;
        for y in origin_y..end_y {
            let offset = self.offset(plane, origin_x, y) * bytes_per_sample;
            self.recon[offset..offset + row_bytes]
                .copy_from_slice(&self.source[offset..offset + row_bytes]);
        }
    }

    fn copy_source_to_recon_leaf(
        &mut self,
        decision: Av2TileDecision,
        visible_rows_mi: usize,
        visible_cols_mi: usize,
    ) {
        let txb_width = decision
            .block_size
            .tx4x4_width()
            .min(visible_cols_mi.saturating_sub(decision.col));
        let txb_height = decision
            .block_size
            .tx4x4_height()
            .min(visible_rows_mi.saturating_sub(decision.row));
        for row in 0..txb_height {
            let abs_row = decision.row + row;
            for col in 0..txb_width {
                let abs_col = decision.col + col;
                let (x0, y0) = self.txb_origin(Av2LosslessPlane::Y, abs_col, abs_row);
                self.copy_source_to_recon_txb(Av2LosslessPlane::Y, x0, y0);
            }
        }

        let chroma_span = chroma_tx4x4_span(
            decision,
            visible_rows_mi,
            visible_cols_mi,
            self.chroma_format,
        );
        for plane in [Av2LosslessPlane::U, Av2LosslessPlane::V] {
            for row in 0..chroma_span.height {
                let abs_row = chroma_span.row + row;
                for col in 0..chroma_span.width {
                    let abs_col = chroma_span.col + col;
                    let (x0, y0) = self.txb_origin(plane, abs_col, abs_row);
                    self.copy_source_to_recon_txb(plane, x0, y0);
                }
            }
        }
    }
}
