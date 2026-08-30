impl Av2Black444TilePlan {
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
