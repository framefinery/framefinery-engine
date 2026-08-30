impl Av2Black444TilePlan {
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
