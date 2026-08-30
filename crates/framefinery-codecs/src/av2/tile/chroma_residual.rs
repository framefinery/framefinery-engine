fn write_lossy_422_intra_chroma_tx4x8(
    writer: &mut Av2EntropyWriter,
    contexts: &mut Av2TxbEntropyContexts,
    lossy: &mut Av2LossySubsampledTileState<'_>,
    mode: Av2LossySubsampledModeDecision,
    chroma_span: Av2ChromaTx4x4Span,
) {
    debug_assert_eq!(lossy.chroma_format, Av2ChromaFormat::Yuv422);
    debug_assert_eq!(chroma_span.width, 1);
    debug_assert!(chroma_span.height <= 2);
    write_lossy_422_chroma_tx4x8(
        writer,
        contexts,
        lossy,
        chroma_span,
        false,
        |lossy, plane| lossy.chroma_444_intra_tx8x8_analysis(plane, chroma_span, mode),
    );
}

fn write_lossy_422_inter_chroma_tx4x8(
    writer: &mut Av2EntropyWriter,
    contexts: &mut Av2TxbEntropyContexts,
    lossy: &mut Av2LossySubsampledTileState<'_>,
    reference: &[u8],
    mv_row_px: i16,
    mv_col_px: i16,
    chroma_span: Av2ChromaTx4x4Span,
    use_inter_contexts: bool,
) {
    debug_assert_eq!(lossy.chroma_format, Av2ChromaFormat::Yuv422);
    debug_assert_eq!(chroma_span.width, 1);
    debug_assert!(chroma_span.height <= 2);
    write_lossy_422_chroma_tx4x8(
        writer,
        contexts,
        lossy,
        chroma_span,
        use_inter_contexts,
        |lossy, plane| {
            lossy.chroma_444_inter_tx8x8_analysis(
                reference,
                plane,
                chroma_span,
                mv_row_px,
                mv_col_px,
            )
        },
    );
}

fn write_lossy_422_chroma_tx4x8<Analysis>(
    writer: &mut Av2EntropyWriter,
    contexts: &mut Av2TxbEntropyContexts,
    lossy: &mut Av2LossySubsampledTileState<'_>,
    chroma_span: Av2ChromaTx4x4Span,
    use_inter_contexts: bool,
    analysis: Analysis,
) where
    Analysis: Fn(&Av2LossySubsampledTileState<'_>, Av2LossyPlane) -> Av2LossyTx8x8Analysis,
{
    // AVM's regular-q 4:2:2 chroma path maps an 8x8 luma coding block to one
    // TX_4X8 per chroma plane.
    let abs_row = chroma_span.row;
    let abs_col = chroma_span.col;
    let u_skip_ctx =
        chroma_txb_skip_base_context(contexts.u_above[abs_col], contexts.u_left[abs_row]) + 6;
    let u_analysis = analysis(lossy, Av2LossyPlane::U);
    let u_candidate = lossy.regular_dct_quantized_tx4x8_residual_candidate(&u_analysis);
    let (u_context, u_nonzero) = write_chroma_tx4x8_txb(
        writer,
        Av2ChromaPlane::U,
        u_skip_ctx,
        &u_candidate.coefficients,
        use_inter_contexts,
    );
    lossy.fill_chroma_422_tx4x8_leaf(Av2LossyPlane::U, &u_analysis, &u_candidate.residual);
    for col in abs_col..(abs_col + chroma_span.width).min(contexts.u_above.len()) {
        contexts.u_above[col] = u_context;
    }
    for row in abs_row..(abs_row + chroma_span.height).min(contexts.u_left.len()) {
        contexts.u_left[row] = u_context;
    }

    let v_skip_ctx = chroma_txb_skip_base_context(contexts.v_above[abs_col], contexts.v_left[abs_row])
        + if u_nonzero { 6 } else { 0 };
    let v_analysis = analysis(lossy, Av2LossyPlane::V);
    let v_candidate = lossy.regular_dct_quantized_tx4x8_residual_candidate(&v_analysis);
    let (v_context, _) = write_chroma_tx4x8_txb(
        writer,
        Av2ChromaPlane::V,
        v_skip_ctx,
        &v_candidate.coefficients,
        use_inter_contexts,
    );
    lossy.fill_chroma_422_tx4x8_leaf(Av2LossyPlane::V, &v_analysis, &v_candidate.residual);
    for col in abs_col..(abs_col + chroma_span.width).min(contexts.v_above.len()) {
        contexts.v_above[col] = v_context;
    }
    for row in abs_row..(abs_row + chroma_span.height).min(contexts.v_left.len()) {
        contexts.v_left[row] = v_context;
    }
}

fn write_lossy_444_chroma_tx8x8(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    contexts: &mut Av2TxbEntropyContexts,
    lossy: &mut Av2LossySubsampledTileState<'_>,
    mode: Option<Av2LossySubsampledModeDecision>,
    chroma_span: Av2ChromaTx4x4Span,
    use_inter_contexts: bool,
) {
    debug_assert_eq!(lossy.chroma_format, Av2ChromaFormat::Yuv444);
    debug_assert_eq!(decision.block_size.width, 8);
    debug_assert_eq!(decision.block_size.height, 8);

    let abs_row = chroma_span.row;
    let abs_col = chroma_span.col;
    let u_skip_ctx =
        chroma_txb_skip_base_context(contexts.u_above[abs_col], contexts.u_left[abs_row]) + 6;
    let u_analysis =
        mode.map(|mode| lossy.chroma_444_intra_tx8x8_analysis(Av2LossyPlane::U, chroma_span, mode));
    let (u_context, u_nonzero, u_residual) = if let Some(analysis) = u_analysis.as_ref() {
        let candidate = lossy.regular_dct_quantized_tx8x8_residual_candidate(analysis);
        let (context, nonzero) = write_chroma_tx8x8_txb(
            writer,
            Av2ChromaPlane::U,
            u_skip_ctx,
            &candidate.coefficients,
            use_inter_contexts,
        );
        (context, nonzero, Some(candidate.residual))
    } else {
        write_u_txb_all_zero_tx8x8(writer, u_skip_ctx, use_inter_contexts);
        (0, false, None)
    };
    for col in abs_col..(abs_col + chroma_span.width).min(contexts.u_above.len()) {
        contexts.u_above[col] = u_context;
    }
    for row in abs_row..(abs_row + chroma_span.height).min(contexts.u_left.len()) {
        contexts.u_left[row] = u_context;
    }

    let v_skip_ctx = chroma_txb_skip_base_context(contexts.v_above[abs_col], contexts.v_left[abs_row])
        + if u_nonzero { 6 } else { 0 };
    let v_analysis =
        mode.map(|mode| lossy.chroma_444_intra_tx8x8_analysis(Av2LossyPlane::V, chroma_span, mode));
    let (v_context, v_residual) = if let Some(analysis) = v_analysis.as_ref() {
        let candidate = lossy.regular_dct_quantized_tx8x8_residual_candidate(analysis);
        let (context, _) = write_chroma_tx8x8_txb(
            writer,
            Av2ChromaPlane::V,
            v_skip_ctx,
            &candidate.coefficients,
            use_inter_contexts,
        );
        (context, Some(candidate.residual))
    } else {
        write_v_txb_all_zero(writer, v_skip_ctx);
        (0, None)
    };
    for col in abs_col..(abs_col + chroma_span.width).min(contexts.v_above.len()) {
        contexts.v_above[col] = v_context;
    }
    for row in abs_row..(abs_row + chroma_span.height).min(contexts.v_left.len()) {
        contexts.v_left[row] = v_context;
    }

    if let (Some(analysis), Some(residual)) = (u_analysis.as_ref(), u_residual.as_ref()) {
        lossy.fill_chroma_444_tx8x8_leaf(Av2LossyPlane::U, analysis, residual);
    }
    if let (Some(analysis), Some(residual)) = (v_analysis.as_ref(), v_residual.as_ref()) {
        lossy.fill_chroma_444_tx8x8_leaf(Av2LossyPlane::V, analysis, residual);
    }
}

fn write_lossy_444_inter_chroma_txb(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    contexts: &mut Av2TxbEntropyContexts,
    lossy: &mut Av2LossySubsampledTileState<'_>,
    reference: &[u8],
    mv_row_px: i16,
    mv_col_px: i16,
    chroma_span: Av2ChromaTx4x4Span,
    use_inter_contexts: bool,
) {
    debug_assert_eq!(lossy.chroma_format, Av2ChromaFormat::Yuv444);
    debug_assert_eq!(decision.block_size.width, 8);
    debug_assert_eq!(decision.block_size.height, 8);

    let abs_row = chroma_span.row;
    let abs_col = chroma_span.col;
    let u_skip_ctx =
        chroma_txb_skip_base_context(contexts.u_above[abs_col], contexts.u_left[abs_row]) + 6;
    let u_analysis = lossy.chroma_444_inter_tx8x8_analysis(
        reference,
        Av2LossyPlane::U,
        chroma_span,
        mv_row_px,
        mv_col_px,
    );
    let u_candidate = lossy.regular_dct_quantized_tx8x8_residual_candidate(&u_analysis);
    let (u_context, u_nonzero) = write_chroma_tx8x8_txb(
        writer,
        Av2ChromaPlane::U,
        u_skip_ctx,
        &u_candidate.coefficients,
        use_inter_contexts,
    );
    lossy.fill_chroma_444_tx8x8_leaf(
        Av2LossyPlane::U,
        &u_analysis,
        &u_candidate.residual,
    );
    for col in abs_col..(abs_col + chroma_span.width).min(contexts.u_above.len()) {
        contexts.u_above[col] = u_context;
    }
    for row in abs_row..(abs_row + chroma_span.height).min(contexts.u_left.len()) {
        contexts.u_left[row] = u_context;
    }

    let v_skip_ctx = chroma_txb_skip_base_context(contexts.v_above[abs_col], contexts.v_left[abs_row])
        + if u_nonzero { 6 } else { 0 };
    let v_analysis = lossy.chroma_444_inter_tx8x8_analysis(
        reference,
        Av2LossyPlane::V,
        chroma_span,
        mv_row_px,
        mv_col_px,
    );
    let v_candidate = lossy.regular_dct_quantized_tx8x8_residual_candidate(&v_analysis);
    let (v_context, _) = write_chroma_tx8x8_txb(
        writer,
        Av2ChromaPlane::V,
        v_skip_ctx,
        &v_candidate.coefficients,
        use_inter_contexts,
    );
    lossy.fill_chroma_444_tx8x8_leaf(
        Av2LossyPlane::V,
        &v_analysis,
        &v_candidate.residual,
    );
    for col in abs_col..(abs_col + chroma_span.width).min(contexts.v_above.len()) {
        contexts.v_above[col] = v_context;
    }
    for row in abs_row..(abs_row + chroma_span.height).min(contexts.v_left.len()) {
        contexts.v_left[row] = v_context;
    }
}
