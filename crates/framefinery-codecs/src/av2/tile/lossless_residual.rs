fn write_lossless_subsampled_residual_coefficients(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    visible_rows_mi: usize,
    visible_cols_mi: usize,
    contexts: &mut Av2TxbEntropyContexts,
    coded_mi_context: &Av2CodedMiContext,
    lossless: &mut Av2LosslessSubsampledTileState<'_>,
    mode: Av2LosslessSubsampledModeDecision,
    palette: Option<&Av2LumaPalette444>,
) {
    let txb_width = decision
        .block_size
        .tx4x4_width()
        .min(visible_cols_mi.saturating_sub(decision.col));
    let txb_height = decision
        .block_size
        .tx4x4_height()
        .min(visible_rows_mi.saturating_sub(decision.row));
    if mode.use_fsc {
        write_lossless_tx_size_4x4(writer, decision.block_size);
    }
    let (luma_leaf_x0, luma_leaf_y0) =
        lossless.txb_origin(Av2LosslessPlane::Y, decision.col, decision.row);
    let luma_leaf_width = txb_width * TX4X4_SIZE;
    let luma_leaf_height = txb_height * TX4X4_SIZE;
    let luma_palette_region = mode.use_luma_palette.then(|| {
        palette
            .expect("AV2 luma palette mode needs palette state")
            .syntax_region_palette(
                luma_leaf_x0,
                luma_leaf_y0,
                luma_leaf_width,
                luma_leaf_height,
            )
    });
    for row in 0..txb_height {
        let abs_row = decision.row + row;
        for col in 0..txb_width {
            let abs_col = decision.col + col;
            let skip_ctx =
                luma_txb_skip_context(contexts.y_above[abs_col], contexts.y_left[abs_row]);
            let dc_sign_ctx = dc_sign_context(contexts.y_above[abs_col], contexts.y_left[abs_row]);
            let (x0, y0) = lossless.txb_origin(Av2LosslessPlane::Y, abs_col, abs_row);
            let residual = if let Some(region) = luma_palette_region.as_ref() {
                lossless.luma_palette_residual4x4(
                    palette.expect("AV2 luma palette mode needs palette state"),
                    region,
                    x0,
                    y0,
                )
            } else {
                lossless.tx4x4_residual_for_mode(
                    Av2LosslessPlane::Y,
                    x0,
                    y0,
                    mode,
                    luma_leaf_x0,
                    luma_leaf_y0,
                    luma_leaf_width,
                    luma_leaf_height,
                    coded_mi_context,
                )
            };
            let (context, _) = if tx4x4_residual_is_zero(&residual) {
                if mode.use_fsc {
                    write_y_fsc_txb_all_zero(writer);
                } else {
                    write_y_txb_all_zero(writer, skip_ctx);
                }
                (0, false)
            } else if mode.use_fsc {
                let coefficients = tx4x4_coefficients_from_residual(&residual, true);
                write_luma_palette_fsc_txb(writer, &coefficients)
            } else {
                let coefficients = tx4x4_coefficients_from_residual(&residual, false);
                write_luma_palette_residual_txb(writer, skip_ctx, dc_sign_ctx, &coefficients)
            };
            lossless.copy_source_to_recon_txb(Av2LosslessPlane::Y, x0, y0);
            contexts.y_above[abs_col] = context;
            contexts.y_left[abs_row] = context;
        }
    }

    let chroma_span = chroma_tx4x4_span(
        decision,
        visible_rows_mi,
        visible_cols_mi,
        lossless.chroma_format,
    );
    let (chroma_leaf_x0, chroma_leaf_y0) =
        lossless.txb_origin(Av2LosslessPlane::U, chroma_span.col, chroma_span.row);
    let chroma_leaf_width = chroma_span.width * TX4X4_SIZE;
    let chroma_leaf_height = chroma_span.height * TX4X4_SIZE;
    let mut last_u_txb_nonzero = false;
    for row in 0..chroma_span.height {
        let abs_row = chroma_span.row + row;
        for col in 0..chroma_span.width {
            let abs_col = chroma_span.col + col;
            let skip_ctx =
                chroma_txb_skip_base_context(contexts.u_above[abs_col], contexts.u_left[abs_row])
                    + 6;
            let (x0, y0) = lossless.txb_origin(Av2LosslessPlane::U, abs_col, abs_row);
            let residual = lossless.tx4x4_residual_for_mode(
                Av2LosslessPlane::U,
                x0,
                y0,
                mode,
                chroma_leaf_x0,
                chroma_leaf_y0,
                chroma_leaf_width,
                chroma_leaf_height,
                coded_mi_context,
            );
            let (context, nonzero) = if tx4x4_residual_is_zero(&residual) {
                write_u_txb_all_zero(writer, skip_ctx, mode.use_fsc);
                (0, false)
            } else {
                let coefficients = tx4x4_coefficients_from_residual(&residual, mode.use_fsc);
                write_chroma_bdpcm_txb(
                    writer,
                    Av2ChromaPlane::U,
                    skip_ctx,
                    &coefficients,
                    mode.use_fsc,
                )
            };
            lossless.copy_source_to_recon_txb(Av2LosslessPlane::U, x0, y0);
            contexts.u_above[abs_col] = context;
            contexts.u_left[abs_row] = context;
            last_u_txb_nonzero = nonzero;
        }
    }

    for row in 0..chroma_span.height {
        let abs_row = chroma_span.row + row;
        for col in 0..chroma_span.width {
            let abs_col = chroma_span.col + col;
            let skip_ctx = v_txb_skip_context_for_chroma_format(
                contexts.v_above[abs_col],
                contexts.v_left[abs_row],
                last_u_txb_nonzero,
                lossless.chroma_format,
                decision.block_size,
            );
            let (x0, y0) = lossless.txb_origin(Av2LosslessPlane::V, abs_col, abs_row);
            let residual = lossless.tx4x4_residual_for_mode(
                Av2LosslessPlane::V,
                x0,
                y0,
                mode,
                chroma_leaf_x0,
                chroma_leaf_y0,
                chroma_leaf_width,
                chroma_leaf_height,
                coded_mi_context,
            );
            let (context, _) = if tx4x4_residual_is_zero(&residual) {
                write_v_txb_all_zero(writer, skip_ctx);
                (0, false)
            } else {
                let coefficients = tx4x4_coefficients_from_residual(&residual, mode.use_fsc);
                write_chroma_bdpcm_txb(
                    writer,
                    Av2ChromaPlane::V,
                    skip_ctx,
                    &coefficients,
                    mode.use_fsc,
                )
            };
            lossless.copy_source_to_recon_txb(Av2LosslessPlane::V, x0, y0);
            contexts.v_above[abs_col] = context;
            contexts.v_left[abs_row] = context;
        }
    }
}

fn write_lossless_inter_residual_coefficients(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    visible_rows_mi: usize,
    visible_cols_mi: usize,
    contexts: &mut Av2TxbEntropyContexts,
    lossless: &mut Av2LosslessSubsampledTileState<'_>,
    reference: &[u8],
    mv_row_px: i16,
    mv_col_px: i16,
    use_regular_inter_txb_contexts: bool,
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
            let skip_ctx =
                luma_txb_skip_context(contexts.y_above[abs_col], contexts.y_left[abs_row]);
            let dc_sign_ctx = dc_sign_context(contexts.y_above[abs_col], contexts.y_left[abs_row]);
            let (x0, y0) = lossless.txb_origin(Av2LosslessPlane::Y, abs_col, abs_row);
            let residual = lossless.inter_residual4x4(
                reference,
                Av2LosslessPlane::Y,
                x0,
                y0,
                mv_row_px,
                mv_col_px,
            );
            let (context, _) = if tx4x4_residual_is_zero(&residual) {
                if use_regular_inter_txb_contexts {
                    write_y_inter_txb_all_zero(writer, skip_ctx);
                } else {
                    write_y_txb_all_zero(writer, skip_ctx);
                }
                (0, false)
            } else {
                let coefficients = tx4x4_coefficients_from_residual(&residual, false);
                if use_regular_inter_txb_contexts {
                    write_luma_inter_residual_txb(writer, skip_ctx, dc_sign_ctx, &coefficients)
                } else {
                    write_luma_palette_residual_txb(writer, skip_ctx, dc_sign_ctx, &coefficients)
                }
            };
            lossless.copy_source_to_recon_txb(Av2LosslessPlane::Y, x0, y0);
            contexts.y_above[abs_col] = context;
            contexts.y_left[abs_row] = context;
        }
    }

    let chroma_span = chroma_tx4x4_span(
        decision,
        visible_rows_mi,
        visible_cols_mi,
        lossless.chroma_format,
    );
    let mut last_u_txb_nonzero = false;
    for row in 0..chroma_span.height {
        let abs_row = chroma_span.row + row;
        for col in 0..chroma_span.width {
            let abs_col = chroma_span.col + col;
            let skip_ctx =
                chroma_txb_skip_base_context(contexts.u_above[abs_col], contexts.u_left[abs_row])
                    + 6;
            let (x0, y0) = lossless.txb_origin(Av2LosslessPlane::U, abs_col, abs_row);
            let residual = lossless.inter_residual4x4(
                reference,
                Av2LosslessPlane::U,
                x0,
                y0,
                mv_row_px,
                mv_col_px,
            );
            let (context, nonzero) = if tx4x4_residual_is_zero(&residual) {
                write_u_txb_all_zero(writer, skip_ctx, use_regular_inter_txb_contexts);
                (0, false)
            } else {
                let coefficients = tx4x4_coefficients_from_residual(&residual, false);
                write_chroma_bdpcm_txb(
                    writer,
                    Av2ChromaPlane::U,
                    skip_ctx,
                    &coefficients,
                    use_regular_inter_txb_contexts,
                )
            };
            lossless.copy_source_to_recon_txb(Av2LosslessPlane::U, x0, y0);
            contexts.u_above[abs_col] = context;
            contexts.u_left[abs_row] = context;
            last_u_txb_nonzero = nonzero;
        }
    }

    for row in 0..chroma_span.height {
        let abs_row = chroma_span.row + row;
        for col in 0..chroma_span.width {
            let abs_col = chroma_span.col + col;
            let skip_ctx = v_txb_skip_context_for_chroma_format(
                contexts.v_above[abs_col],
                contexts.v_left[abs_row],
                last_u_txb_nonzero,
                lossless.chroma_format,
                decision.block_size,
            );
            let (x0, y0) = lossless.txb_origin(Av2LosslessPlane::V, abs_col, abs_row);
            let residual = lossless.inter_residual4x4(
                reference,
                Av2LosslessPlane::V,
                x0,
                y0,
                mv_row_px,
                mv_col_px,
            );
            let (context, _) = if tx4x4_residual_is_zero(&residual) {
                write_v_txb_all_zero(writer, skip_ctx);
                (0, false)
            } else {
                let coefficients = tx4x4_coefficients_from_residual(&residual, false);
                write_chroma_bdpcm_txb(
                    writer,
                    Av2ChromaPlane::V,
                    skip_ctx,
                    &coefficients,
                    false,
                )
            };
            lossless.copy_source_to_recon_txb(Av2LosslessPlane::V, x0, y0);
            contexts.v_above[abs_col] = context;
            contexts.v_left[abs_row] = context;
        }
    }
}
