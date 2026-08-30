impl Av2Black444TilePlan {
    fn for_region(
        region: Av2TileRegion,
        profile: Av2Black444MvpProfile,
        chroma_format: Av2ChromaFormat,
        luma_palette: bool,
        allow_intrabc: bool,
        ibc: Option<&Av2LocalIbc444>,
        palette: Option<&Av2LumaPalette444>,
    ) -> Self {
        Self::for_region_with_partition_policy_and_features(
            region,
            profile,
            chroma_format,
            Av2PartitionPolicy::Fixed8x8Leaves,
            luma_palette,
            allow_intrabc,
            ibc,
            palette,
            None,
            None,
        )
    }

    fn for_region_with_partition_policy(
        region: Av2TileRegion,
        profile: Av2Black444MvpProfile,
        chroma_format: Av2ChromaFormat,
        partition_policy: Av2PartitionPolicy,
        luma_palette: bool,
        allow_intrabc: bool,
        ibc: Option<&Av2LocalIbc444>,
        palette: Option<&Av2LumaPalette444>,
    ) -> Self {
        Self::for_region_with_partition_policy_and_features(
            region,
            profile,
            chroma_format,
            partition_policy,
            luma_palette,
            allow_intrabc,
            ibc,
            palette,
            None,
            None,
        )
    }

    fn for_region_with_inter_partition_modes(
        region: Av2TileRegion,
        profile: Av2Black444MvpProfile,
        chroma_format: Av2ChromaFormat,
        block_modes: &Av2LosslessInterTileBlockModes,
    ) -> Self {
        Self::for_region_with_partition_policy_and_features(
            region,
            profile,
            chroma_format,
            Av2PartitionPolicy::LosslessInterModes,
            false,
            false,
            None,
            None,
            None,
            Some(block_modes.clone()),
        )
    }

    fn for_region_with_fixed_inter_partition_modes(
        region: Av2TileRegion,
        profile: Av2Black444MvpProfile,
        chroma_format: Av2ChromaFormat,
        block_modes: &Av2LosslessInterTileBlockModes,
    ) -> Self {
        Self::for_region_with_partition_policy_and_features(
            region,
            profile,
            chroma_format,
            Av2PartitionPolicy::Fixed8x8Leaves,
            false,
            false,
            None,
            None,
            None,
            Some(block_modes.clone()),
        )
    }

    fn for_region_with_partition_policy_and_features(
        region: Av2TileRegion,
        profile: Av2Black444MvpProfile,
        chroma_format: Av2ChromaFormat,
        partition_policy: Av2PartitionPolicy,
        luma_palette: bool,
        allow_intrabc: bool,
        ibc: Option<&Av2LocalIbc444>,
        palette: Option<&Av2LumaPalette444>,
        adaptive_partition_features: Option<Av2AdaptivePartitionFeatures>,
        inter_partition_modes: Option<Av2LosslessInterTileBlockModes>,
    ) -> Self {
        assert!(
            !profile.enable_sdp,
            "AV2 MVP tile plan expects a shared luma/chroma partition tree"
        );
        assert!(
            region.origin_x % MVP_SUPERBLOCK_SIZE == 0
                && region.origin_y % MVP_SUPERBLOCK_SIZE == 0,
            "AV2 MVP tiles are aligned to 64x64 superblock origins"
        );
        assert!(
            region.width % 8 == 0 && region.height % 8 == 0,
            "AV2 MVP tile plan expects coded tile dimensions in 8-pixel units"
        );
        let geometry = region.geometry();
        let visible_rows_mi = geometry.height / MI_SIZE;
        let visible_cols_mi = geometry.width / MI_SIZE;
        let max_ref_bv_count = usize::from(profile.def_max_bvp_drl_bits_minus_min) + 2;
        let mut plan = Self {
            decisions: Vec::new(),
            origin_x: region.origin_x,
            origin_y: region.origin_y,
            chroma_format,
            partition_policy,
            visible_rows_mi,
            visible_cols_mi,
            luma_palette,
            allow_intrabc,
            max_ref_bv_count,
            adaptive_partition_features,
            inter_partition_modes,
        };
        let mut partition_context = Av2PartitionContext::new(visible_rows_mi, visible_cols_mi);
        for row_mi in (0..visible_rows_mi).step_by(PARTITION_CONTEXT_DIM) {
            for col_mi in (0..visible_cols_mi).step_by(PARTITION_CONTEXT_DIM) {
                plan.visit_block(
                    row_mi,
                    col_mi,
                    Av2MvpBlockSize::BLOCK_64X64,
                    visible_rows_mi,
                    visible_cols_mi,
                    &mut partition_context,
                    ibc,
                    palette,
                );
            }
        }
        plan
    }

    fn visit_block(
        &mut self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
        visible_rows_mi: usize,
        visible_cols_mi: usize,
        partition_context: &mut Av2PartitionContext,
        ibc: Option<&Av2LocalIbc444>,
        palette: Option<&Av2LumaPalette444>,
    ) {
        if row_mi >= visible_rows_mi || col_mi >= visible_cols_mi {
            return;
        }

        let partition = if self.luma_palette {
            choose_luma_palette_partition(
                row_mi,
                col_mi,
                block_size,
                visible_rows_mi,
                visible_cols_mi,
                self.partition_policy,
                palette,
            )
        } else {
            match self.partition_policy {
                Av2PartitionPolicy::Fixed8x8Leaves => {
                    choose_partition(row_mi, col_mi, block_size, visible_rows_mi, visible_cols_mi)
                }
                #[cfg(test)]
                Av2PartitionPolicy::LargestLosslessLeaves => choose_largest_lossless_partition(
                    row_mi,
                    col_mi,
                    block_size,
                    visible_rows_mi,
                    visible_cols_mi,
                ),
                Av2PartitionPolicy::AdaptiveScreenContent => choose_adaptive_screen_content_partition(
                    row_mi,
                    col_mi,
                    block_size,
                    visible_rows_mi,
                    visible_cols_mi,
                    self.adaptive_partition_features
                        .as_ref()
                        .expect("adaptive partitioning needs source features"),
                ),
                Av2PartitionPolicy::LosslessInterModes => choose_lossless_inter_partition(
                    row_mi,
                    col_mi,
                    block_size,
                    visible_rows_mi,
                    visible_cols_mi,
                    self.inter_partition_modes
                        .as_ref()
                        .expect("inter partitioning needs block mode features"),
                ),
            }
        };
        self.decisions.push(Av2TileDecision {
            kind: Av2TileDecisionKind::Partition(partition),
            row: row_mi,
            col: col_mi,
            block_size,
        });

        match partition {
            Av2MvpPartition::None => {
                self.visit_leaf(row_mi, col_mi, block_size, ibc, palette);
                partition_context.update_leaf(row_mi, col_mi, block_size);
            }
            Av2MvpPartition::Horz => {
                let subsize = block_size
                    .subsize(partition)
                    .expect("AV2 MVP horizontal partition must have a subsize");
                self.visit_block(
                    row_mi,
                    col_mi,
                    subsize,
                    visible_rows_mi,
                    visible_cols_mi,
                    partition_context,
                    ibc,
                    palette,
                );
                self.visit_block(
                    row_mi + block_size.mi_height() / 2,
                    col_mi,
                    subsize,
                    visible_rows_mi,
                    visible_cols_mi,
                    partition_context,
                    ibc,
                    palette,
                );
            }
            Av2MvpPartition::Vert => {
                let subsize = block_size
                    .subsize(partition)
                    .expect("AV2 MVP vertical partition must have a subsize");
                self.visit_block(
                    row_mi,
                    col_mi,
                    subsize,
                    visible_rows_mi,
                    visible_cols_mi,
                    partition_context,
                    ibc,
                    palette,
                );
                self.visit_block(
                    row_mi,
                    col_mi + block_size.mi_width() / 2,
                    subsize,
                    visible_rows_mi,
                    visible_cols_mi,
                    partition_context,
                    ibc,
                    palette,
                );
            }
        }
    }

    fn visit_leaf(
        &mut self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
        ibc: Option<&Av2LocalIbc444>,
        palette: Option<&Av2LumaPalette444>,
    ) {
        assert!(
            block_size.width >= MVP_LEAF_BLOCK_SIZE && block_size.height >= MVP_LEAF_BLOCK_SIZE,
            "AV2 MVP coding leaves must be at least 8x8 blocks"
        );
        let x0 = self.origin_x + col_mi * MI_SIZE;
        let y0 = self.origin_y + row_mi * MI_SIZE;
        let ibc_copy = ibc.and_then(|ibc| ibc.candidate_copy(x0, y0));
        let ibc_drl_idx = ibc_copy.map(|copy| copy.drl_idx());
        let merged_luma_palette_leaf = self.luma_palette
            && (block_size.width > AV2_LUMA_PALETTE_BLOCK_SIZE
                || block_size.height > AV2_LUMA_PALETTE_BLOCK_SIZE);
        let luma_mode = if merged_luma_palette_leaf {
            Av2LumaIntraMode::Dc
        } else {
            palette
                .map(|palette| palette.luma_mode_for_block(x0, y0))
                .unwrap_or(Av2LumaIntraMode::Dc)
        };
        let luma_bdpcm_horz = if merged_luma_palette_leaf {
            None
        } else {
            palette.and_then(|palette| palette.luma_bdpcm_horz_for_block(x0, y0))
        };
        let mut chroma_intra_mode = palette
            .map(|palette| palette.chroma_intra_mode_for_block(x0, y0))
            .unwrap_or(Av2ChromaIntraMode::Horizontal);
        let chroma_use_bdpcm = palette
            .map(|palette| palette.chroma_use_bdpcm_for_block(x0, y0))
            .unwrap_or(false);
        if self.luma_palette
            && !chroma_use_bdpcm
            && chroma_intra_mode == Av2ChromaIntraMode::Dc
        {
            // Avoid fragile single-DC chroma residual patterns in the current
            // 4:4:4 palette path; vertical prediction keeps AVM lossless.
            chroma_intra_mode = Av2ChromaIntraMode::Vertical;
        }
        let prediction = decide_leaf_prediction(
            self.allow_intrabc,
            ibc_drl_idx,
            self.luma_palette,
            luma_mode,
            luma_bdpcm_horz,
            chroma_use_bdpcm,
            chroma_intra_mode,
        );
        if self.allow_intrabc {
            self.decisions.push(Av2TileDecision {
                kind: Av2TileDecisionKind::IntrabcFlag(prediction.intrabc_flag),
                row: row_mi,
                col: col_mi,
                block_size,
            });
        }
        match prediction.prediction {
            Av2LeafPredictionMode::IntrabcCopy { drl_idx } => {
                self.decisions.push(Av2TileDecision {
                    kind: Av2TileDecisionKind::IntrabcCopy {
                        drl_idx,
                        explicit_dv: ibc_copy.and_then(|copy| copy.explicit_dv()),
                    },
                    row: row_mi,
                    col: col_mi,
                    block_size,
                });
            }
            Av2LeafPredictionMode::Intra {
                luma_mode,
                use_luma_palette,
                use_dpcm_y,
                luma_bdpcm_horz,
                use_bdpcm_uv,
                chroma_intra_mode,
            } => {
                let use_fsc = AV2_ENABLE_LUMA_PALETTE_FSC_444
                    && use_luma_palette
                    && block_size.width == AV2_LUMA_PALETTE_BLOCK_SIZE
                    && block_size.height == AV2_LUMA_PALETTE_BLOCK_SIZE
                    && !use_dpcm_y
                    && palette.is_some_and(|palette| {
                        luma_palette_fsc_is_rate_worthy(
                            palette,
                            x0,
                            y0,
                            self.origin_x,
                            self.origin_y,
                            chroma_use_bdpcm,
                            chroma_intra_mode,
                        )
                    });
                self.decisions.push(Av2TileDecision {
                    kind: Av2TileDecisionKind::IntraLumaMode {
                        mode: luma_mode,
                        use_dpcm_y,
                        dpcm_horz: luma_bdpcm_horz,
                        use_fsc,
                    },
                    row: row_mi,
                    col: col_mi,
                    block_size,
                });
                let coded_luma_mode = if use_dpcm_y {
                    if luma_bdpcm_horz {
                        Av2LumaIntraMode::Horizontal
                    } else {
                        Av2LumaIntraMode::Vertical
                    }
                } else {
                    luma_mode
                };
                self.decisions.push(Av2TileDecision {
                    kind: Av2TileDecisionKind::IntraChromaMode {
                        use_bdpcm_uv,
                        luma_mode: coded_luma_mode,
                        chroma_intra_mode,
                    },
                    row: row_mi,
                    col: col_mi,
                    block_size,
                });
                if use_luma_palette {
                    self.decisions.push(Av2TileDecision {
                        kind: Av2TileDecisionKind::LumaPaletteModeInfo,
                        row: row_mi,
                        col: col_mi,
                        block_size,
                    });
                    self.decisions.push(Av2TileDecision {
                        kind: Av2TileDecisionKind::LumaPaletteColorMap,
                        row: row_mi,
                        col: col_mi,
                        block_size,
                    });
                }
                match prediction.residual {
                    Av2LeafResidualMode::BlackDc => {
                        self.decisions.push(Av2TileDecision {
                            kind: Av2TileDecisionKind::BlackDcResidualCoefficients,
                            row: row_mi,
                            col: col_mi,
                            block_size,
                        });
                    }
                    Av2LeafResidualMode::LumaPalette {
                        luma_bdpcm_horz,
                        chroma_use_bdpcm,
                        chroma_intra_mode,
                    } => {
                        self.decisions.push(Av2TileDecision {
                            kind: Av2TileDecisionKind::LumaPaletteResidualCoefficients {
                                luma_bdpcm_horz,
                                chroma_use_bdpcm,
                                chroma_intra_mode,
                                use_fsc,
                            },
                            row: row_mi,
                            col: col_mi,
                            block_size,
                        });
                    }
                    Av2LeafResidualMode::None => {}
                }
            }
        }
    }

    fn write_entropy(
        &self,
        writer: &mut Av2EntropyWriter,
        palette: Option<&Av2LumaPalette444>,
        _ibc: Option<&Av2LocalIbc444>,
    ) {
        let mut partition_context =
            Av2PartitionContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut txb_contexts =
            Av2TxbEntropyContexts::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut intrabc_context =
            Av2IntrabcContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut coded_mi_context =
            Av2CodedMiContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut palette_cache_context =
            Av2PaletteColorCacheContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut luma_mode_context =
            Av2LumaModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut fsc_mode_context =
            Av2FscModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
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
                        partition_context.update_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                        );
                    }
                }
                Av2TileDecisionKind::IntrabcFlag(use_intrabc) => {
                    write_intrabc_flag(writer, *decision, &intrabc_context, use_intrabc);
                }
                Av2TileDecisionKind::IntrabcCopy {
                    drl_idx,
                    explicit_dv,
                } => {
                    write_intrabc_copy(
                        writer,
                        *decision,
                        &intrabc_context,
                        self.profile_max_ref_bv_count(),
                        drl_idx,
                        explicit_dv,
                    );
                    intrabc_context.update_leaf(
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
                    palette_cache_context.clear_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                    );
                    // AVM av2_get_joint_mode() reports DC_PRED for inter and
                    // IntraBC neighbors. Keep the luma-mode context tied to
                    // actual coded leaves rather than palette pre-analysis so
                    // enabling more IBC copies cannot desynchronize later
                    // intra-mode symbols.
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
                }
                Av2TileDecisionKind::IntraLumaMode {
                    mode,
                    use_dpcm_y,
                    dpcm_horz,
                    use_fsc,
                } => {
                    let mode_syntax = luma_mode_context.syntax_for_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                    );
                    let mode_context = mode_syntax.context;
                    let mode_index = mode_syntax.index_for(mode);
                    let fsc_context =
                        fsc_mode_context.context(decision.row, decision.col, decision.block_size);
                    write_intra_luma_mode(
                        writer,
                        *decision,
                        mode,
                        mode_context,
                        mode_index,
                        true,
                        use_dpcm_y,
                        dpcm_horz,
                        use_fsc,
                        fsc_context,
                    );
                    if mode != Av2LumaIntraMode::Dc || use_dpcm_y {
                        palette_cache_context.clear_leaf(
                            decision.row,
                            decision.col,
                            decision.block_size,
                        );
                    }
                    let coded_mode = if use_dpcm_y {
                        if dpcm_horz {
                            Av2LumaIntraMode::Horizontal
                        } else {
                            Av2LumaIntraMode::Vertical
                        }
                    } else {
                        mode
                    };
                    luma_mode_context.update_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        coded_mode,
                    );
                    fsc_mode_context.update_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        use_fsc,
                    );
                }
                Av2TileDecisionKind::IntraChromaMode {
                    use_bdpcm_uv,
                    luma_mode,
                    chroma_intra_mode,
                } => {
                    write_intra_chroma_mode(
                        writer,
                        *decision,
                        true,
                        use_bdpcm_uv,
                        luma_mode,
                        chroma_intra_mode,
                    );
                }
                Av2TileDecisionKind::LumaPaletteModeInfo => {
                    write_luma_palette_mode_info(
                        writer,
                        *decision,
                        palette.expect("luma palette decision needs palette state"),
                        &mut palette_cache_context,
                        self.origin_x,
                        self.origin_y,
                    );
                }
                Av2TileDecisionKind::LumaPaletteColorMap => {
                    write_luma_palette_color_map(
                        writer,
                        *decision,
                        palette.expect("luma palette decision needs palette state"),
                        self.origin_x,
                        self.origin_y,
                    );
                }
                Av2TileDecisionKind::BlackDcResidualCoefficients => {
                    write_black_dc_residual_coefficients(
                        writer,
                        *decision,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                        self.chroma_format,
                        &mut txb_contexts,
                    );
                    intrabc_context.update_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        false,
                        false,
                    );
                    coded_mi_context.update_leaf(decision.row, decision.col, decision.block_size);
                }
                Av2TileDecisionKind::LumaPaletteResidualCoefficients {
                    luma_bdpcm_horz,
                    chroma_use_bdpcm,
                    chroma_intra_mode,
                    use_fsc,
                } => {
                    write_luma_palette_residual_coefficients(
                        writer,
                        *decision,
                        self.visible_rows_mi,
                        self.visible_cols_mi,
                        palette.expect("luma palette residual needs palette state"),
                        &mut txb_contexts,
                        &coded_mi_context,
                        self.origin_x,
                        self.origin_y,
                        luma_bdpcm_horz,
                        chroma_use_bdpcm,
                        chroma_intra_mode,
                        use_fsc,
                    );
                    intrabc_context.update_leaf(
                        decision.row,
                        decision.col,
                        decision.block_size,
                        false,
                        false,
                    );
                    coded_mi_context.update_leaf(decision.row, decision.col, decision.block_size);
                }
            }
        }
    }

    fn profile_max_ref_bv_count(&self) -> usize {
        self.max_ref_bv_count
    }

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

    fn write_lossless_subsampled_entropy(
        &self,
        writer: &mut Av2EntropyWriter,
        lossless: &mut Av2LosslessSubsampledTileState<'_>,
        palette: Option<&Av2LumaPalette444>,
        regular_inter_frame: bool,
    ) {
        let mut partition_context =
            Av2PartitionContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut txb_contexts =
            Av2TxbEntropyContexts::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut intrabc_context =
            Av2IntrabcContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut coded_mi_context =
            Av2CodedMiContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut palette_cache_context =
            Av2PaletteColorCacheContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut luma_mode_context =
            Av2LumaModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut fsc_mode_context =
            Av2FscModeContext::new(self.visible_rows_mi, self.visible_cols_mi);
        let mut inter_context = regular_inter_frame
            .then(|| Av2InterModeContext::new(self.visible_rows_mi, self.visible_cols_mi));
        let mut mode_cache: Option<(Av2TileDecision, Av2LosslessSubsampledModeDecision)> = None;
        #[cfg(feature = "av2-sb-bit-profile")]
        let mut sb_bits = Av2SbBitCollector::new(
            "lossless_subsampled",
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
                        if let Some(inter_context) = inter_context.as_mut() {
                            write_inter_intra_flag(writer, *decision, inter_context, false);
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
                Av2TileDecisionKind::IntrabcFlag(use_intrabc) => {
                    #[cfg(feature = "av2-sb-bit-profile")]
                    let before_bits = writer.symbol_bits();
                    write_intrabc_flag(writer, *decision, &intrabc_context, use_intrabc);
                    #[cfg(feature = "av2-sb-bit-profile")]
                    sb_bits.record(
                        decision.row,
                        decision.col,
                        before_bits,
                        writer.symbol_bits(),
                        Av2SbBitCategory::Intrabc,
                        false,
                    );
                }
                Av2TileDecisionKind::IntrabcCopy {
                    drl_idx,
                    explicit_dv,
                } => {
                    #[cfg(feature = "av2-sb-bit-profile")]
                    let before_bits = writer.symbol_bits();
                    write_intrabc_copy(
                        writer,
                        *decision,
                        &intrabc_context,
                        self.profile_max_ref_bv_count(),
                        drl_idx,
                        explicit_dv,
                    );
                    intrabc_context.update_leaf(
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
                    #[cfg(feature = "av2-sb-bit-profile")]
                    sb_bits.record(
                        decision.row,
                        decision.col,
                        before_bits,
                        writer.symbol_bits(),
                        Av2SbBitCategory::Intrabc,
                        true,
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
                    let fsc_context = if regular_inter_frame {
                        3
                    } else {
                        fsc_mode_context.context(decision.row, decision.col, decision.block_size)
                    };
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
                    #[cfg(feature = "av2-sb-bit-profile")]
                    sb_bits.record(
                        decision.row,
                        decision.col,
                        before_bits,
                        writer.symbol_bits(),
                        if mode.use_luma_palette {
                            Av2SbBitCategory::Palette
                        } else {
                            Av2SbBitCategory::ChromaMode
                        },
                        false,
                    );
                }
                Av2TileDecisionKind::BlackDcResidualCoefficients => {
                    #[cfg(feature = "av2-sb-bit-profile")]
                    let before_bits = writer.symbol_bits();
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
                Av2TileDecisionKind::LumaPaletteModeInfo
                | Av2TileDecisionKind::LumaPaletteColorMap
                | Av2TileDecisionKind::LumaPaletteResidualCoefficients { .. } => {
                    unreachable!("AV2 planar lossless path emits palette inline")
                }
            }
        }
        #[cfg(feature = "av2-sb-bit-profile")]
        sb_bits.flush_if_enabled();
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
}
