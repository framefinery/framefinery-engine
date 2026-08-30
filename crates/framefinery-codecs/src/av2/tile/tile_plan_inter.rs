impl Av2Black444TilePlan {
    fn write_lossless_mixed_inter_intra_entropy(
        &self,
        writer: &mut Av2EntropyWriter,
        total_refs: usize,
        block_modes: &Av2LosslessInterTileBlockModes,
        lossless: &mut Av2LosslessSubsampledTileState<'_>,
        reference: &[u8],
        palette: Option<&Av2LumaPalette444>,
        use_regular_inter_txb_contexts: bool,
    ) {
        let mut partition_context =
            Av2PartitionContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut txb_contexts =
            Av2TxbEntropyContexts::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut skip_context =
            Av2IntrabcContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut coded_mi_context =
            Av2CodedMiContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut palette_cache_context =
            Av2PaletteColorCacheContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut luma_mode_context =
            Av2LumaModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut fsc_mode_context =
            Av2FscModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut inter_context =
            Av2InterModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut mode_cache: Option<(Av2TileDecision, Av2LosslessSubsampledModeDecision)> = None;
        let mut active_inter_leaf: Option<Av2ActiveInterLeaf> = None;
        for decision in &self.decisions {
            match decision.kind {
                Av2TileDecisionKind::Partition(partition) => {
                    active_inter_leaf = None;
                    write_partition(
                        writer,
                        *decision,
                        partition,
                        &partition_context,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                    );
                    if partition == Av2MvpPartition::None {
                        match block_modes.mode_for_decision(*decision) {
                            Av2LosslessInterBlockMode::Intra => {
                                write_inter_intra_flag(writer, *decision, &inter_context, false);
                                inter_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    false,
                                    0,
                                    false,
                                    0,
                                    0,
                                );
                            }
                            Av2LosslessInterBlockMode::ZeroMv => {
                                write_inter_globalmv_skip(
                                    writer,
                                    *decision,
                                    &skip_context,
                                    &inter_context,
                                    total_refs,
                                );
                                skip_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    false,
                                    true,
                                );
                                inter_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    true,
                                    0,
                                    false,
                                    0,
                                    0,
                                );
                                txb_contexts.clear_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    self.visible_rows_mi,
                                    self.visible_cols_mi,
                                    self.chroma_format,
                                );
                                coded_mi_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                );
                                palette_cache_context.clear_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                );
                                lossless.copy_source_to_recon_leaf(
                                    *decision,
                                    self.visible_rows_mi,
                                    self.visible_cols_mi,
                                );
                                active_inter_leaf = Some(Av2ActiveInterLeaf::Skip {
                                    row: decision.row,
                                    col: decision.col,
                                });
                            }
                            Av2LosslessInterBlockMode::ZeroMvResidual => {
                                write_inter_globalmv_residual(
                                    writer,
                                    *decision,
                                    &skip_context,
                                    &inter_context,
                                    total_refs,
                                );
                                if use_regular_inter_txb_contexts {
                                    write_lossless_tx_size_4x4(writer, decision.block_size);
                                }
                                skip_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    false,
                                    false,
                                );
                                inter_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    true,
                                    0,
                                    false,
                                    0,
                                    0,
                                );
                                palette_cache_context.clear_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                );
                                active_inter_leaf = Some(Av2ActiveInterLeaf::Residual {
                                    row: decision.row,
                                    col: decision.col,
                                    mv_row_px: 0,
                                    mv_col_px: 0,
                                });
                            }
                            Av2LosslessInterBlockMode::NewMv { row_px, col_px } => {
                                write_inter_newmv_skip(
                                    writer,
                                    *decision,
                                    &skip_context,
                                    &inter_context,
                                    total_refs,
                                    row_px,
                                    col_px,
                                );
                                skip_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    false,
                                    true,
                                );
                                inter_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    true,
                                    0,
                                    true,
                                    row_px,
                                    col_px,
                                );
                                txb_contexts.clear_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    self.visible_rows_mi,
                                    self.visible_cols_mi,
                                    self.chroma_format,
                                );
                                coded_mi_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                );
                                palette_cache_context.clear_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                );
                                lossless.copy_source_to_recon_leaf(
                                    *decision,
                                    self.visible_rows_mi,
                                    self.visible_cols_mi,
                                );
                                active_inter_leaf = Some(Av2ActiveInterLeaf::Skip {
                                    row: decision.row,
                                    col: decision.col,
                                });
                            }
                            Av2LosslessInterBlockMode::NewMvResidual { .. } => {
                                unreachable!(
                                    "lossless mixed inter writer does not accept NEWMV residual blocks"
                                )
                            }
                        }
                        partition_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                        );
                    }
                }
                Av2TileDecisionKind::IntrabcFlag(use_intrabc) => {
                    if active_inter_leaf_matches(active_inter_leaf, *decision).is_some() {
                        continue;
                    }
                    write_intrabc_flag(writer, *decision, &skip_context, use_intrabc);
                }
                Av2TileDecisionKind::IntrabcCopy {
                    drl_idx,
                    explicit_dv,
                } => {
                    if active_inter_leaf_matches(active_inter_leaf, *decision).is_some() {
                        continue;
                    }
                    write_intrabc_copy(
                        writer,
                        *decision,
                        &skip_context,
                        self.profile_max_ref_bv_count(),
                        drl_idx,
                        explicit_dv,
                    );
                    skip_context.update_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        true,
                        true,
                    );
                    txb_contexts.clear_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                        self.chroma_format,
                    );
                    luma_mode_context.update_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        Av2LumaIntraMode::Dc,
                    );
                    fsc_mode_context.update_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        false,
                    );
                    coded_mi_context.update_leaf(decision.row, decision.col, decision.block_size);
                    palette_cache_context.clear_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                    );
                    lossless.copy_source_to_recon_leaf(
                        *decision,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                    );
                }
                Av2TileDecisionKind::IntraLumaMode {
                    mode: _,
                    use_dpcm_y: _,
                    dpcm_horz: _,
                    use_fsc: _,
                } => {
                    if active_inter_leaf_matches(active_inter_leaf, *decision).is_some() {
                        continue;
                    }
                    let mode = cached_lossless_subsampled_mode(
                        &mut mode_cache,
                        lossless,
                        *decision,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                        &coded_mi_context,
                        palette,
                    );
                    let coded_luma_mode = mode.coded_luma_mode();
                    let mode_syntax = luma_mode_context.syntax_for_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                    );
                    let mode_index = mode_syntax.index_for(coded_luma_mode);
                    write_intra_luma_mode(
                        writer,
                        *decision,
                        coded_luma_mode,
                        mode_syntax.context,
                        mode_index,
                        true,
                        mode.luma_bdpcm_horz.is_some(),
                        mode.luma_bdpcm_horz.unwrap_or(false),
                        mode.use_fsc,
                        3,
                    );
                    luma_mode_context.update_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        coded_luma_mode,
                    );
                    fsc_mode_context.update_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        mode.use_fsc,
                    );
                }
                Av2TileDecisionKind::IntraChromaMode {
                    use_bdpcm_uv: _,
                    luma_mode: _,
                    chroma_intra_mode: _,
                } => {
                    if active_inter_leaf_matches(active_inter_leaf, *decision).is_some() {
                        continue;
                    }
                    let mode = cached_lossless_subsampled_mode(
                        &mut mode_cache,
                        lossless,
                        *decision,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                        &coded_mi_context,
                        palette,
                    );
                    write_intra_chroma_mode(
                        writer,
                        *decision,
                        true,
                        mode.chroma_use_bdpcm,
                        mode.coded_luma_mode(),
                        mode.chroma_intra_mode,
                    );
                    if mode.use_luma_palette {
                        if let Some(palette) = palette {
                            write_luma_palette_mode_info(
                                writer,
                                *decision,
                                palette,
                                &mut palette_cache_context,
                                self.origin_x,
                                self.origin_y,
                            );
                            write_luma_palette_color_map(
                                writer,
                                *decision,
                                palette,
                                self.origin_x,
                                self.origin_y,
                            );
                        }
                    } else if (self.allow_intrabc || palette.is_some())
                        && mode.coded_luma_mode() == Av2LumaIntraMode::Dc
                        && mode.luma_bdpcm_horz.is_none()
                    {
                        write_luma_palette_absent_mode_info(
                            writer,
                            *decision,
                            &mut palette_cache_context,
                        );
                    } else if palette.is_some() {
                        palette_cache_context.clear_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                        );
                    }
                }
                Av2TileDecisionKind::BlackDcResidualCoefficients => {
                    if let Some(active_inter_leaf) =
                        active_inter_leaf_matches(active_inter_leaf, *decision)
                    {
                        match active_inter_leaf {
                            Av2ActiveInterLeaf::Skip { .. } => continue,
                            Av2ActiveInterLeaf::Residual {
                                mv_row_px,
                                mv_col_px,
                                ..
                            } => {
                                write_lossless_inter_residual_coefficients(
                                    writer,
                                    *decision,
                                    self.visible_rows_mi,
                                    self.visible_cols_mi,
                                    &mut txb_contexts,
                                    lossless,
                                    reference,
                                    mv_row_px,
                                    mv_col_px,
                                    use_regular_inter_txb_contexts,
                                );
                                coded_mi_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                );
                                continue;
                            }
                        }
                    }
                    let mode = cached_lossless_subsampled_mode(
                        &mut mode_cache,
                        lossless,
                        *decision,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                        &coded_mi_context,
                        palette,
                    );
                    write_lossless_subsampled_residual_coefficients(
                        writer,
                        *decision,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                        &mut txb_contexts,
                        &coded_mi_context,
                        lossless,
                        mode,
                        palette,
                    );
                    skip_context.update_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        false,
                        false,
                    );
                    coded_mi_context.update_leaf(decision.row, decision.col, decision.block_size);
                }
                Av2TileDecisionKind::LumaPaletteModeInfo
                | Av2TileDecisionKind::LumaPaletteColorMap
                | Av2TileDecisionKind::LumaPaletteResidualCoefficients { .. } => {
                    unreachable!("AV2 planar lossless path emits palette inline")
                }
            }
        }
    }

    fn write_lossless_new_mv_inter_entropy(
        &self,
        writer: &mut Av2EntropyWriter,
        total_refs: usize,
        mv_row_px: i16,
        mv_col_px: i16,
    ) {
        assert!(
            mv_row_px != 0 || mv_col_px != 0,
            "NEWMV entropy helper expects a non-zero motion vector"
        );
        let mut partition_context =
            Av2PartitionContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut skip_context =
            Av2IntrabcContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut inter_context =
            Av2InterModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        for decision in &self.decisions {
            match decision.kind {
                Av2TileDecisionKind::Partition(partition) => {
                    write_partition(
                        writer,
                        *decision,
                        partition,
                        &partition_context,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                    );
                    if partition == Av2MvpPartition::None {
                        write_inter_newmv_skip(
                            writer,
                            *decision,
                            &skip_context,
                            &inter_context,
                            total_refs,
                            mv_row_px,
                            mv_col_px,
                        );
                        partition_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                        );
                        skip_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                            false,
                            true,
                        );
                        inter_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                            true,
                            0,
                            true,
                            mv_row_px,
                            mv_col_px,
                        );
                    }
                }
                Av2TileDecisionKind::IntrabcFlag(_)
                | Av2TileDecisionKind::IntrabcCopy { .. }
                | Av2TileDecisionKind::IntraLumaMode { .. }
                | Av2TileDecisionKind::IntraChromaMode { .. }
                | Av2TileDecisionKind::LumaPaletteModeInfo
                | Av2TileDecisionKind::LumaPaletteColorMap
                | Av2TileDecisionKind::BlackDcResidualCoefficients
                | Av2TileDecisionKind::LumaPaletteResidualCoefficients { .. } => {}
            }
        }
    }

    fn write_lossy_zero_mv_residual_inter_entropy(
        &self,
        writer: &mut Av2EntropyWriter,
        total_refs: usize,
        block_modes: &Av2LosslessInterTileBlockModes,
        lossy: &mut Av2LossySubsampledTileState<'_>,
        reference: &[u8],
    ) {
        let mut partition_context =
            Av2PartitionContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut txb_contexts =
            Av2TxbEntropyContexts::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut skip_context =
            Av2IntrabcContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut coded_mi_context =
            Av2CodedMiContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut palette_cache_context =
            Av2PaletteColorCacheContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut inter_context =
            Av2InterModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut active_inter_leaf: Option<Av2ActiveInterLeaf> = None;
        for decision in &self.decisions {
            match decision.kind {
                Av2TileDecisionKind::Partition(partition) => {
                    active_inter_leaf = None;
                    write_partition(
                        writer,
                        *decision,
                        partition,
                        &partition_context,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                    );
                    if partition == Av2MvpPartition::None {
                        match block_modes.mode_for_decision(*decision) {
                            Av2LosslessInterBlockMode::ZeroMvResidual => {
                                write_inter_globalmv_residual(
                                    writer,
                                    *decision,
                                    &skip_context,
                                    &inter_context,
                                    total_refs,
                                );
                                write_regular_q_inter_tx_partition_split_8x8(writer, *decision);
                                skip_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    false,
                                    false,
                                );
                                inter_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    true,
                                    0,
                                    false,
                                    0,
                                    0,
                                );
                                palette_cache_context.clear_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                );
                                active_inter_leaf = Some(Av2ActiveInterLeaf::Residual {
                                    row: decision.row,
                                    col: decision.col,
                                    mv_row_px: 0,
                                    mv_col_px: 0,
                                });
                            }
                            Av2LosslessInterBlockMode::NewMvResidual { row_px, col_px } => {
                                write_inter_newmv_residual(
                                    writer,
                                    *decision,
                                    &skip_context,
                                    &inter_context,
                                    total_refs,
                                    row_px,
                                    col_px,
                                );
                                write_regular_q_inter_tx_partition_split_8x8(writer, *decision);
                                skip_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    false,
                                    false,
                                );
                                inter_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    true,
                                    0,
                                    true,
                                    row_px,
                                    col_px,
                                );
                                palette_cache_context.clear_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                );
                                active_inter_leaf = Some(Av2ActiveInterLeaf::Residual {
                                    row: decision.row,
                                    col: decision.col,
                                    mv_row_px: row_px,
                                    mv_col_px: col_px,
                                });
                            }
                            Av2LosslessInterBlockMode::Intra
                            | Av2LosslessInterBlockMode::ZeroMv
                            | Av2LosslessInterBlockMode::NewMv { .. } => {
                                unreachable!(
                                    "lossy zero-MV residual tile writer only accepts residual blocks"
                                )
                            }
                        }
                        partition_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                        );
                    }
                }
                Av2TileDecisionKind::BlackDcResidualCoefficients => {
                    if let Some(Av2ActiveInterLeaf::Residual {
                        mv_row_px,
                        mv_col_px,
                        ..
                    }) = active_inter_leaf_matches(active_inter_leaf, *decision)
                    {
                        write_lossy_inter_residual_coefficients(
                            writer,
                            *decision,
                            self.visible_rows_mi,
                            self.visible_cols_mi,
                            &mut txb_contexts,
                            lossy,
                            reference,
                            mv_row_px,
                            mv_col_px,
                            true,
                        );
                        coded_mi_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                        );
                    }
                }
                Av2TileDecisionKind::IntrabcFlag(_)
                | Av2TileDecisionKind::IntrabcCopy { .. }
                | Av2TileDecisionKind::IntraLumaMode { .. }
                | Av2TileDecisionKind::IntraChromaMode { .. }
                | Av2TileDecisionKind::LumaPaletteModeInfo
                | Av2TileDecisionKind::LumaPaletteColorMap
                | Av2TileDecisionKind::LumaPaletteResidualCoefficients { .. } => {}
            }
        }
    }

    fn write_lossless_zero_mv_inter_entropy(
        &self,
        writer: &mut Av2EntropyWriter,
        total_refs: usize,
    ) {
        let mut partition_context =
            Av2PartitionContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut skip_context =
            Av2IntrabcContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut inter_context =
            Av2InterModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        for decision in &self.decisions {
            match decision.kind {
                Av2TileDecisionKind::Partition(partition) => {
                    write_partition(
                        writer,
                        *decision,
                        partition,
                        &partition_context,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                    );
                    if partition == Av2MvpPartition::None {
                        write_inter_globalmv_skip(
                            writer,
                            *decision,
                            &skip_context,
                            &inter_context,
                            total_refs,
                        );
                        partition_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                        );
                        skip_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                            false,
                            true,
                        );
                        inter_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                            true,
                            0,
                            false,
                            0,
                            0,
                        );
                    }
                }
                Av2TileDecisionKind::IntrabcFlag(_)
                | Av2TileDecisionKind::IntrabcCopy { .. }
                | Av2TileDecisionKind::IntraLumaMode { .. }
                | Av2TileDecisionKind::IntraChromaMode { .. }
                | Av2TileDecisionKind::LumaPaletteModeInfo
                | Av2TileDecisionKind::LumaPaletteColorMap
                | Av2TileDecisionKind::BlackDcResidualCoefficients
                | Av2TileDecisionKind::LumaPaletteResidualCoefficients { .. } => {}
            }
        }
    }

    fn write_lossless_mixed_inter_entropy(
        &self,
        writer: &mut Av2EntropyWriter,
        total_refs: usize,
        block_modes: &Av2LosslessInterTileBlockModes,
    ) {
        let mut partition_context =
            Av2PartitionContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut skip_context =
            Av2IntrabcContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut inter_context =
            Av2InterModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        for decision in &self.decisions {
            match decision.kind {
                Av2TileDecisionKind::Partition(partition) => {
                    write_partition(
                        writer,
                        *decision,
                        partition,
                        &partition_context,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                    );
                    if partition == Av2MvpPartition::None {
                        match block_modes.mode_for_decision(*decision) {
                            Av2LosslessInterBlockMode::Intra => {
                                unreachable!("all-inter skip tile writer received an intra block")
                            }
                            Av2LosslessInterBlockMode::ZeroMvResidual => {
                                unreachable!(
                                    "all-inter skip tile writer received a residual block"
                                )
                            }
                            Av2LosslessInterBlockMode::NewMvResidual { .. } => {
                                unreachable!(
                                    "all-inter skip tile writer received a NEWMV residual block"
                                )
                            }
                            Av2LosslessInterBlockMode::ZeroMv => {
                                write_inter_globalmv_skip(
                                    writer,
                                    *decision,
                                    &skip_context,
                                    &inter_context,
                                    total_refs,
                                );
                                inter_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    true,
                                    0,
                                    false,
                                    0,
                                    0,
                                );
                            }
                            Av2LosslessInterBlockMode::NewMv { row_px, col_px } => {
                                write_inter_newmv_skip(
                                    writer,
                                    *decision,
                                    &skip_context,
                                    &inter_context,
                                    total_refs,
                                    row_px,
                                    col_px,
                                );
                                inter_context.update_leaf(
                                    decision.row,
                                    decision.col,
                                    decision.block_size,
                                    true,
                                    0,
                                    true,
                                    row_px,
                                    col_px,
                                );
                            }
                        }
                        partition_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                        );
                        skip_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                            false,
                            true,
                        );
                    }
                }
                Av2TileDecisionKind::IntrabcFlag(_)
                | Av2TileDecisionKind::IntrabcCopy { .. }
                | Av2TileDecisionKind::IntraLumaMode { .. }
                | Av2TileDecisionKind::IntraChromaMode { .. }
                | Av2TileDecisionKind::LumaPaletteModeInfo
                | Av2TileDecisionKind::LumaPaletteColorMap
                | Av2TileDecisionKind::BlackDcResidualCoefficients
                | Av2TileDecisionKind::LumaPaletteResidualCoefficients { .. } => {}
            }
        }
    }
}
