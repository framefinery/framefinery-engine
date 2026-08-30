impl Av2Black444TilePlan {
    fn write_lossy_subsampled_entropy(
        &self,
        writer: &mut Av2EntropyWriter,
        lossy: &mut Av2LossySubsampledTileState<'_>,
    ) {
        let mut partition_context =
            Av2PartitionContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut txb_contexts =
            Av2TxbEntropyContexts::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut intrabc_context =
            Av2IntrabcContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut coded_mi_context =
            Av2CodedMiContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut luma_mode_context =
            Av2LumaModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut fsc_mode_context =
            Av2FscModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut mode_cache: Option<(
            usize,
            usize,
            Av2MvpBlockSize,
            Av2LossySubsampledModeDecision,
        )> = None;
        #[cfg(feature = "av2-sb-bit-profile")]
        let mut sb_bits = Av2SbBitCollector::new(
            "lossy_subsampled",
            self.origin_x,
            self.origin_y,
            self.visible_cols_mi * MI_SIZE,
            self.visible_rows_mi * MI_SIZE,
        );
        for decision in &self.decisions {
            match decision.kind {
                Av2TileDecisionKind::Partition(partition) => {
                    #[cfg(feature = "av2-sb-bit-profile")]
                    let before_bits = writer.symbol_bits();
                    write_partition(
                        writer,
                        *decision,
                        partition,
                        &partition_context,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                    );
                    if partition == Av2MvpPartition::None {
                        partition_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                        );
                    }
                    #[cfg(feature = "av2-sb-bit-profile")]
                    sb_bits.record(
                        decision.row,
                        decision.col,
                        before_bits,
                        writer.symbol_bits(),
                        Av2SbBitCategory::Partition,
                        false,
                    );
                }
                Av2TileDecisionKind::IntraLumaMode {
                    mode: _,
                    use_dpcm_y: _,
                    dpcm_horz: _,
                    use_fsc: _,
                } => {
                    #[cfg(feature = "av2-sb-bit-profile")]
                    let before_bits = writer.symbol_bits();
                    let mode_syntax = luma_mode_context.syntax_for_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                    );
                    let mode = cached_lossy_subsampled_mode_with_syntax(
                        &mut mode_cache,
                        lossy,
                        *decision,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                        &coded_mi_context,
                        mode_syntax,
                    );
                    let coded_luma_mode = mode.coded_luma_mode();
                    let mode_index = mode_syntax.index_for(coded_luma_mode);
                    let fsc_context =
                        fsc_mode_context.context(decision.row, decision.col, decision.block_size);
                    write_intra_luma_mode(
                        writer,
                        *decision,
                        coded_luma_mode,
                        mode_syntax.context,
                        mode_index,
                        false,
                        mode.luma_bdpcm_horz.is_some(),
                        mode.luma_bdpcm_horz.unwrap_or(false),
                        mode.use_fsc,
                        fsc_context,
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
                    #[cfg(feature = "av2-sb-bit-profile")]
                    sb_bits.record(
                        decision.row,
                        decision.col,
                        before_bits,
                        writer.symbol_bits(),
                        Av2SbBitCategory::LumaMode,
                        false,
                    );
                }
                Av2TileDecisionKind::IntraChromaMode {
                    use_bdpcm_uv: _,
                    luma_mode: _,
                    chroma_intra_mode: _,
                } => {
                    #[cfg(feature = "av2-sb-bit-profile")]
                    let before_bits = writer.symbol_bits();
                    let mode = cached_lossy_subsampled_mode(
                        &mut mode_cache,
                        lossy,
                        *decision,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                        &coded_mi_context,
                        &luma_mode_context,
                    );
                    write_intra_chroma_mode(
                        writer,
                        *decision,
                        false,
                        mode.chroma_use_bdpcm,
                        mode.coded_luma_mode(),
                        mode.chroma_intra_mode,
                    );
                    #[cfg(feature = "av2-sb-bit-profile")]
                    sb_bits.record(
                        decision.row,
                        decision.col,
                        before_bits,
                        writer.symbol_bits(),
                        Av2SbBitCategory::ChromaMode,
                        false,
                    );
                }
                Av2TileDecisionKind::BlackDcResidualCoefficients => {
                    #[cfg(feature = "av2-sb-bit-profile")]
                    let before_bits = writer.symbol_bits();
                    let mode = cached_lossy_subsampled_mode(
                        &mut mode_cache,
                        lossy,
                        *decision,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                        &coded_mi_context,
                        &luma_mode_context,
                    );
                    write_lossy_subsampled_residual_coefficients(
                        writer,
                        *decision,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                        &mut txb_contexts,
                        lossy,
                        mode,
                        &coded_mi_context,
                    );
                    intrabc_context.update_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        false,
                        false,
                    );
                    coded_mi_context.update_leaf(decision.row, decision.col, decision.block_size);
                    #[cfg(feature = "av2-sb-bit-profile")]
                    sb_bits.record(
                        decision.row,
                        decision.col,
                        before_bits,
                        writer.symbol_bits(),
                        Av2SbBitCategory::Residual,
                        true,
                    );
                }
                Av2TileDecisionKind::IntrabcFlag(_)
                | Av2TileDecisionKind::IntrabcCopy { .. }
                | Av2TileDecisionKind::LumaPaletteModeInfo
                | Av2TileDecisionKind::LumaPaletteColorMap
                | Av2TileDecisionKind::LumaPaletteResidualCoefficients { .. } => {
                    unreachable!("AV2 lossy residual path disables palette and IntraBC")
                }
            }
        }
        #[cfg(feature = "av2-sb-bit-profile")]
        sb_bits.flush_if_enabled();
    }
}
