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
    write_lossless_luma_residual_coefficients(
        writer,
        decision,
        contexts,
        lossless,
        txb_width,
        txb_height,
        |writer, lossless, x0, y0, skip_ctx, dc_sign_ctx| {
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
            if tx4x4_residual_is_zero(&residual) {
                if mode.use_fsc {
                    write_y_fsc_txb_all_zero(writer);
                } else {
                    write_y_txb_all_zero(writer, skip_ctx);
                }
                0
            } else if mode.use_fsc {
                let coefficients = tx4x4_coefficients_from_residual(&residual, true);
                write_luma_palette_fsc_txb(writer, &coefficients).0
            } else {
                let coefficients = tx4x4_coefficients_from_residual(&residual, false);
                write_luma_palette_residual_txb(writer, skip_ctx, dc_sign_ctx, &coefficients).0
            }
        },
    );

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
    write_lossless_chroma_residual_coefficients(
        writer,
        decision,
        contexts,
        lossless,
        chroma_span,
        Av2LosslessChromaResidualSyntax {
            coefficient_use_fsc: mode.use_fsc,
            u_syntax_use_fsc: mode.use_fsc,
            v_syntax_use_fsc: mode.use_fsc,
        },
        |lossless, plane, x0, y0| {
            lossless.tx4x4_residual_for_mode(
                plane,
                x0,
                y0,
                mode,
                chroma_leaf_x0,
                chroma_leaf_y0,
                chroma_leaf_width,
                chroma_leaf_height,
                coded_mi_context,
            )
        },
    );
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
    write_lossless_luma_residual_coefficients(
        writer,
        decision,
        contexts,
        lossless,
        txb_width,
        txb_height,
        |writer, lossless, x0, y0, skip_ctx, dc_sign_ctx| {
            let residual = lossless.inter_residual4x4(
                reference,
                Av2LosslessPlane::Y,
                x0,
                y0,
                mv_row_px,
                mv_col_px,
            );
            if tx4x4_residual_is_zero(&residual) {
                if use_regular_inter_txb_contexts {
                    write_y_inter_txb_all_zero(writer, skip_ctx);
                } else {
                    write_y_txb_all_zero(writer, skip_ctx);
                }
                0
            } else {
                let coefficients = tx4x4_coefficients_from_residual(&residual, false);
                if use_regular_inter_txb_contexts {
                    write_luma_inter_residual_txb(writer, skip_ctx, dc_sign_ctx, &coefficients).0
                } else {
                    write_luma_palette_residual_txb(writer, skip_ctx, dc_sign_ctx, &coefficients).0
                }
            }
        },
    );

    let chroma_span = chroma_tx4x4_span(
        decision,
        visible_rows_mi,
        visible_cols_mi,
        lossless.chroma_format,
    );
    write_lossless_chroma_residual_coefficients(
        writer,
        decision,
        contexts,
        lossless,
        chroma_span,
        Av2LosslessChromaResidualSyntax {
            coefficient_use_fsc: false,
            u_syntax_use_fsc: use_regular_inter_txb_contexts,
            v_syntax_use_fsc: false,
        },
        |lossless, plane, x0, y0| {
            lossless.inter_residual4x4(reference, plane, x0, y0, mv_row_px, mv_col_px)
        },
    );
}

fn write_lossless_luma_residual_coefficients(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    contexts: &mut Av2TxbEntropyContexts,
    lossless: &mut Av2LosslessSubsampledTileState<'_>,
    txb_width: usize,
    txb_height: usize,
    mut write_txb: impl FnMut(
        &mut Av2EntropyWriter,
        &Av2LosslessSubsampledTileState<'_>,
        usize,
        usize,
        u8,
        u8,
    ) -> u8,
) {
    for row in 0..txb_height {
        let abs_row = decision.row + row;
        for col in 0..txb_width {
            let abs_col = decision.col + col;
            let skip_ctx =
                luma_txb_skip_context(contexts.y_above[abs_col], contexts.y_left[abs_row]);
            let dc_sign_ctx = dc_sign_context(contexts.y_above[abs_col], contexts.y_left[abs_row]);
            let (x0, y0) = lossless.txb_origin(Av2LosslessPlane::Y, abs_col, abs_row);
            let context = write_txb(writer, lossless, x0, y0, skip_ctx, dc_sign_ctx);
            lossless.copy_source_to_recon_txb(Av2LosslessPlane::Y, x0, y0);
            contexts.y_above[abs_col] = context;
            contexts.y_left[abs_row] = context;
        }
    }
}

struct Av2LosslessChromaResidualSyntax {
    coefficient_use_fsc: bool,
    u_syntax_use_fsc: bool,
    v_syntax_use_fsc: bool,
}

fn write_lossless_chroma_residual_coefficients(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    contexts: &mut Av2TxbEntropyContexts,
    lossless: &mut Av2LosslessSubsampledTileState<'_>,
    chroma_span: Av2ChromaTx4x4Span,
    syntax: Av2LosslessChromaResidualSyntax,
    mut residual_for_txb: impl FnMut(
        &Av2LosslessSubsampledTileState<'_>,
        Av2LosslessPlane,
        usize,
        usize,
    ) -> [i32; TX4X4_SAMPLES],
) {
    let mut last_u_txb_nonzero = false;
    for row in 0..chroma_span.height {
        let abs_row = chroma_span.row + row;
        for col in 0..chroma_span.width {
            let abs_col = chroma_span.col + col;
            let skip_ctx =
                chroma_txb_skip_base_context(contexts.u_above[abs_col], contexts.u_left[abs_row])
                    + 6;
            let (x0, y0) = lossless.txb_origin(Av2LosslessPlane::U, abs_col, abs_row);
            let residual = residual_for_txb(lossless, Av2LosslessPlane::U, x0, y0);
            let (context, nonzero) = if tx4x4_residual_is_zero(&residual) {
                write_u_txb_all_zero(writer, skip_ctx, syntax.u_syntax_use_fsc);
                (0, false)
            } else {
                let coefficients =
                    tx4x4_coefficients_from_residual(&residual, syntax.coefficient_use_fsc);
                write_chroma_bdpcm_txb(
                    writer,
                    Av2ChromaPlane::U,
                    skip_ctx,
                    &coefficients,
                    syntax.u_syntax_use_fsc,
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
            let residual = residual_for_txb(lossless, Av2LosslessPlane::V, x0, y0);
            let (context, _) = if tx4x4_residual_is_zero(&residual) {
                write_v_txb_all_zero(writer, skip_ctx);
                (0, false)
            } else {
                let coefficients =
                    tx4x4_coefficients_from_residual(&residual, syntax.coefficient_use_fsc);
                write_chroma_bdpcm_txb(
                    writer,
                    Av2ChromaPlane::V,
                    skip_ctx,
                    &coefficients,
                    syntax.v_syntax_use_fsc,
                )
            };
            lossless.copy_source_to_recon_txb(Av2LosslessPlane::V, x0, y0);
            contexts.v_above[abs_col] = context;
            contexts.v_left[abs_row] = context;
        }
    }
}
