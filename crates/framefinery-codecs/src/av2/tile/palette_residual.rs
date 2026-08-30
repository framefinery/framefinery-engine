fn write_luma_palette_residual_coefficients(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    visible_rows_mi: usize,
    visible_cols_mi: usize,
    palette: &Av2LumaPalette444,
    contexts: &mut Av2TxbEntropyContexts,
    coded_mi_context: &Av2CodedMiContext,
    tile_origin_x: usize,
    tile_origin_y: usize,
    luma_bdpcm_horz: Option<bool>,
    chroma_use_bdpcm: bool,
    chroma_intra_mode: Av2ChromaIntraMode,
    use_fsc: bool,
) {
    // AV2 v1.0.0 Sections 5.20.8.4 palette_tokens() and 5.20.7.27 coeffs():
    // palette supplies a luma predictor, not an escape-coded lossless sample
    // stream. Blocks with more than eight luma values therefore need normal
    // lossless TX_4X4 coefficients for original_y - palette_prediction_y.
    // Chroma palette is not legal in this AV2 branch: av2_allow_palette()
    // accepts PLANE_TYPE_Y only, and AVM keeps palette_size[1] at zero. Chroma
    // therefore remains on an allowed DPCM residual path even though the public
    // FrameFinery leaf and input packet are 8x8.
    let txb_width = decision
        .block_size
        .tx4x4_width()
        .min(visible_cols_mi.saturating_sub(decision.col));
    let txb_height = decision
        .block_size
        .tx4x4_height()
        .min(visible_rows_mi.saturating_sub(decision.row));
    let leaf_x0 = tile_origin_x + decision.col * MI_SIZE;
    let leaf_y0 = tile_origin_y + decision.row * MI_SIZE;
    let leaf_width = decision.block_size.width;
    let leaf_height = decision.block_size.height;
    let luma_region = (!use_fsc && luma_bdpcm_horz.is_none())
        .then(|| palette.syntax_region_palette(leaf_x0, leaf_y0, leaf_width, leaf_height));
    if use_fsc {
        write_lossless_tx_size_4x4(writer, decision.block_size);
    }
    for row in 0..txb_height {
        let abs_row = decision.row + row;
        for col in 0..txb_width {
            let abs_col = decision.col + col;
            let skip_ctx =
                luma_txb_skip_context(contexts.y_above[abs_col], contexts.y_left[abs_row]);
            let dc_sign_ctx = dc_sign_context(contexts.y_above[abs_col], contexts.y_left[abs_row]);
            let txb_x0 = tile_origin_x + abs_col * TX4X4_SIZE;
            let txb_y0 = tile_origin_y + abs_row * TX4X4_SIZE;
            let coefficients = if use_fsc {
                luma_palette_idtx4x4_coefficients(palette, txb_x0, txb_y0)
            } else if let Some(horz) = luma_bdpcm_horz {
                luma_bdpcm_tx4x4_coefficients(
                    palette,
                    txb_x0,
                    txb_y0,
                    tile_origin_x,
                    tile_origin_y,
                    horz,
                )
            } else {
                luma_palette_tx4x4_coefficients(
                    palette,
                    luma_region
                        .as_ref()
                        .expect("luma palette residual needs a region palette"),
                    txb_x0,
                    txb_y0,
                )
            };
            let (context, _) = if use_fsc {
                write_luma_palette_fsc_txb(writer, &coefficients)
            } else {
                write_luma_palette_residual_txb(writer, skip_ctx, dc_sign_ctx, &coefficients)
            };
            contexts.y_above[abs_col] = context;
            contexts.y_left[abs_row] = context;
        }
    }

    let mut last_u_txb_nonzero = false;
    for row in 0..txb_height {
        let abs_row = decision.row + row;
        for col in 0..txb_width {
            let abs_col = decision.col + col;
            let skip_ctx =
                chroma_txb_skip_base_context(contexts.u_above[abs_col], contexts.u_left[abs_row])
                    + 6;
            let txb_x0 = tile_origin_x + abs_col * TX4X4_SIZE;
            let txb_y0 = tile_origin_y + abs_row * TX4X4_SIZE;
            let coefficients = if use_fsc {
                chroma_idtx4x4_coefficients(
                    palette,
                    Av2ChromaPlane::U,
                    txb_x0,
                    txb_y0,
                    tile_origin_x,
                    tile_origin_y,
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    leaf_height,
                    coded_mi_context,
                    chroma_use_bdpcm,
                    chroma_intra_mode,
                )
            } else if chroma_use_bdpcm {
                chroma_bdpcm_tx4x4_coefficients(
                    palette,
                    Av2ChromaPlane::U,
                    txb_x0,
                    txb_y0,
                    tile_origin_x,
                    tile_origin_y,
                    chroma_intra_mode.is_horizontal(),
                )
            } else {
                chroma_intra_tx4x4_coefficients(
                    palette,
                    Av2ChromaPlane::U,
                    txb_x0,
                    txb_y0,
                    tile_origin_x,
                    tile_origin_y,
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    leaf_height,
                    coded_mi_context,
                    chroma_intra_mode,
                )
            };
            let (context, nonzero) =
                write_chroma_bdpcm_txb(writer, Av2ChromaPlane::U, skip_ctx, &coefficients, use_fsc);
            contexts.u_above[abs_col] = context;
            contexts.u_left[abs_row] = context;
            last_u_txb_nonzero = nonzero;
        }
    }

    for row in 0..txb_height {
        let abs_row = decision.row + row;
        for col in 0..txb_width {
            let abs_col = decision.col + col;
            let skip_ctx = v_txb_skip_context(
                contexts.v_above[abs_col],
                contexts.v_left[abs_row],
                last_u_txb_nonzero,
            );
            let txb_x0 = tile_origin_x + abs_col * TX4X4_SIZE;
            let txb_y0 = tile_origin_y + abs_row * TX4X4_SIZE;
            let coefficients = if use_fsc {
                chroma_idtx4x4_coefficients(
                    palette,
                    Av2ChromaPlane::V,
                    txb_x0,
                    txb_y0,
                    tile_origin_x,
                    tile_origin_y,
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    leaf_height,
                    coded_mi_context,
                    chroma_use_bdpcm,
                    chroma_intra_mode,
                )
            } else if chroma_use_bdpcm {
                chroma_bdpcm_tx4x4_coefficients(
                    palette,
                    Av2ChromaPlane::V,
                    txb_x0,
                    txb_y0,
                    tile_origin_x,
                    tile_origin_y,
                    chroma_intra_mode.is_horizontal(),
                )
            } else {
                chroma_intra_tx4x4_coefficients(
                    palette,
                    Av2ChromaPlane::V,
                    txb_x0,
                    txb_y0,
                    tile_origin_x,
                    tile_origin_y,
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    leaf_height,
                    coded_mi_context,
                    chroma_intra_mode,
                )
            };
            let (context, _) =
                write_chroma_bdpcm_txb(writer, Av2ChromaPlane::V, skip_ctx, &coefficients, use_fsc);
            contexts.v_above[abs_col] = context;
            contexts.v_left[abs_row] = context;
        }
    }
}
