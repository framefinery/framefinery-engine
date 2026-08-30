const AV2_STATIC_CDF_TX_PARTITION_INTRA_DO_8X8: usize = 580;
const AV2_STATIC_CDF_TX_PARTITION_INTRA_TYPE_8X8: usize = 581;
const AV2_STATIC_CDF_TX_PARTITION_INTER_DO_8X8: usize = 582;
const AV2_STATIC_CDF_TX_PARTITION_INTER_TYPE_8X8: usize = 583;

fn write_lossy_subsampled_residual_coefficients(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    visible_rows_mi: usize,
    visible_cols_mi: usize,
    contexts: &mut Av2TxbEntropyContexts,
    lossy: &mut Av2LossySubsampledTileState<'_>,
    mode: Av2LossySubsampledModeDecision,
    coded_mi_context: &Av2CodedMiContext,
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
    } else {
        write_regular_q_tx_partition_split_8x8(writer, decision);
    }
    let (luma_leaf_x0, luma_leaf_y0) =
        lossy.txb_origin(Av2LossyPlane::Y, decision.col, decision.row);
    let luma_context = Av2LossyLeafPredictorContext {
        leaf_x0: luma_leaf_x0,
        leaf_y0: luma_leaf_y0,
        leaf_width: txb_width * TX4X4_SIZE,
        leaf_height: txb_height * TX4X4_SIZE,
        coded_mi_context,
    };
    for row in 0..txb_height {
        let abs_row = decision.row + row;
        for col in 0..txb_width {
            let abs_col = decision.col + col;
            let skip_ctx =
                luma_txb_skip_context(contexts.y_above[abs_col], contexts.y_left[abs_row]);
            let dc_sign_ctx = dc_sign_context(contexts.y_above[abs_col], contexts.y_left[abs_row]);
            let (x0, y0) = lossy.txb_origin(Av2LossyPlane::Y, abs_col, abs_row);
            let context = write_lossy_luma_txb(
                writer,
                skip_ctx,
                dc_sign_ctx,
                lossy,
                x0,
                y0,
                mode,
                luma_context,
            );
            contexts.y_above[abs_col] = context;
            contexts.y_left[abs_row] = context;
        }
    }

    let chroma_span = chroma_tx4x4_span(
        decision,
        visible_rows_mi,
        visible_cols_mi,
        lossy.chroma_format,
    );
    let (chroma_leaf_x0, chroma_leaf_y0) =
        lossy.txb_origin(Av2LossyPlane::U, chroma_span.col, chroma_span.row);
    let chroma_context = Av2LossyLeafPredictorContext {
        leaf_x0: chroma_leaf_x0,
        leaf_y0: chroma_leaf_y0,
        leaf_width: chroma_span.width * TX4X4_SIZE,
        leaf_height: chroma_span.height * TX4X4_SIZE,
        coded_mi_context,
    };
    if lossy.chroma_format == Av2ChromaFormat::Yuv444 && !mode.use_fsc {
        write_lossy_444_chroma_tx8x8(
            writer,
            decision,
            contexts,
            lossy,
            Some(mode),
            chroma_span,
            false,
        );
        return;
    }
    if lossy.chroma_format == Av2ChromaFormat::Yuv422 && !mode.use_fsc {
        write_lossy_422_intra_chroma_tx4x8(writer, contexts, lossy, mode, chroma_span);
        return;
    }
    let mut last_u_txb_nonzero = false;
    for row in 0..chroma_span.height {
        let abs_row = chroma_span.row + row;
        for col in 0..chroma_span.width {
            let abs_col = chroma_span.col + col;
            let skip_ctx =
                chroma_txb_skip_base_context(contexts.u_above[abs_col], contexts.u_left[abs_row])
                    + 6;
            let (x0, y0) = lossy.txb_origin(Av2LossyPlane::U, abs_col, abs_row);
            let (context, nonzero) = write_lossy_chroma_txb(
                writer,
                Av2ChromaPlane::U,
                skip_ctx,
                lossy,
                x0,
                y0,
                mode,
                chroma_context,
            );
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
                lossy.chroma_format,
                decision.block_size,
            );
            let (x0, y0) = lossy.txb_origin(Av2LossyPlane::V, abs_col, abs_row);
            let (context, _) = write_lossy_chroma_txb(
                writer,
                Av2ChromaPlane::V,
                skip_ctx,
                lossy,
                x0,
                y0,
                mode,
                chroma_context,
            );
            contexts.v_above[abs_col] = context;
            contexts.v_left[abs_row] = context;
        }
    }
}

fn write_lossy_luma_txb(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    dc_sign_ctx: u8,
    lossy: &mut Av2LossySubsampledTileState<'_>,
    x0: usize,
    y0: usize,
    mode: Av2LossySubsampledModeDecision,
    context: Av2LossyLeafPredictorContext<'_>,
) -> u8 {
    let plane = Av2LossyPlane::Y;
    if let Some(horz) = mode.luma_bdpcm_horz {
        return write_lossy_luma_dpcm_txb(
            writer,
            skip_ctx,
            dc_sign_ctx,
            lossy,
            x0,
            y0,
            horz,
        );
    }
    let analysis = lossy.analyze_txb(plane, x0, y0, mode, context);
    if !mode.use_fsc {
        let candidate = choose_regular_q_lossy_txb(
            lossy.regular_dct_quantized_residual_candidates(&analysis),
            Av2CoefficientProxyKind::LumaTransform,
            lossy.quant_step(),
        );
        let (context, _) = if regular_quantized_txb_is_zero(&candidate) {
            write_y_txb_all_zero(writer, skip_ctx);
            (0, false)
        } else {
            write_luma_palette_residual_txb(
                writer,
                skip_ctx,
                dc_sign_ctx,
                &candidate.coefficients,
            )
        };
        lossy.fill_residual_recon_txb(plane, x0, y0, &analysis, &candidate.residual);
        lossy.record_txb_choice(plane, &Av2LossyTxbChoice::QuantizedResidual(candidate), &analysis);
        return context;
    }
    let coefficients = tx4x4_coefficients_from_residual(&analysis.residual, mode.use_fsc);
    let quantized_candidate =
        if lossy_should_try_ac_quantized(analysis.dc_sse, lossy.quant_step()) {
            Some(lossy.quantized_residual_candidate(&analysis, mode.use_fsc))
        } else {
            None
        };
    let refined_quantized_candidate =
        quantized_candidate
            .as_ref()
            .filter(|candidate| {
                lossy_should_try_refined_ac_quantized(candidate.sse, lossy.quant_step())
            })
            .map(|_| lossy.refined_quantized_residual_candidate(&analysis, mode.use_fsc));
    let transform_quantized_candidate = (!mode.use_fsc).then(|| ()).and(quantized_candidate
        .as_ref()
        .map(|_| lossy.transform_quantized_residual_candidate(&analysis)));
    let choice = choose_lossy_txb(
        analysis.delta,
        &analysis.residual,
        &coefficients,
        if mode.use_fsc {
            Av2CoefficientProxyKind::LumaIdtx
        } else {
            Av2CoefficientProxyKind::LumaTransform
        },
        lossy_quality_rd_quant_step(lossy.quant_step()),
        lossy_max_txb_sse(lossy.quant_step()),
        analysis.dc_sse,
        analysis.dc_variance_loss,
        quantized_candidate,
        refined_quantized_candidate,
        transform_quantized_candidate,
        !mode.use_fsc,
    );
    lossy.record_txb_choice(plane, &choice, &analysis);
    match choice {
        Av2LossyTxbChoice::Exact => {
            let (context, _) = if mode.use_fsc {
                write_luma_palette_fsc_txb(writer, &coefficients)
            } else if tx4x4_residual_is_zero(&analysis.residual) {
                write_y_txb_all_zero(writer, skip_ctx);
                (0, false)
            } else {
                write_luma_palette_residual_txb(writer, skip_ctx, dc_sign_ctx, &coefficients)
            };
            lossy.copy_source_to_recon_txb(plane, x0, y0, &analysis);
            context
        }
        Av2LossyTxbChoice::QuantizedResidual(quantized_residual) => {
            let (context, _) = if mode.use_fsc {
                write_luma_palette_fsc_txb(writer, &quantized_residual.coefficients)
            } else if tx4x4_residual_is_zero(&quantized_residual.residual) {
                write_y_txb_all_zero(writer, skip_ctx);
                (0, false)
            } else {
                write_luma_palette_residual_txb(
                    writer,
                    skip_ctx,
                    dc_sign_ctx,
                    &quantized_residual.coefficients,
                )
            };
            lossy.fill_residual_recon_txb(
                plane,
                x0,
                y0,
                &analysis,
                &quantized_residual.residual,
            );
            context
        }
        Av2LossyTxbChoice::DcDelta(delta) => {
            let context = write_y_dc_delta_txb(writer, skip_ctx, dc_sign_ctx, delta);
            lossy.fill_quantized_recon_txb(plane, x0, y0, &analysis);
            context
        }
    }
}

fn write_lossy_luma_dpcm_txb(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    dc_sign_ctx: u8,
    lossy: &mut Av2LossySubsampledTileState<'_>,
    x0: usize,
    y0: usize,
    horz: bool,
) -> u8 {
    let plane = Av2LossyPlane::Y;
    let analysis = lossy.analyze_dpcm_txb(plane, x0, y0, horz);
    let coefficients = tx4x4_coefficients_from_residual(&analysis.residual, false);
    let quantized_candidate = Some(lossy.quantized_dpcm_residual_candidate(
        &analysis,
        lossy.quant_step(),
        horz,
    ));
    let refined_quantized_candidate = quantized_candidate
        .as_ref()
        .filter(|candidate| lossy_should_try_refined_ac_quantized(candidate.sse, lossy.quant_step()))
        .map(|_| {
            lossy.quantized_dpcm_residual_candidate(
                &analysis,
                refined_lossy_quant_step(lossy.quant_step()),
                horz,
            )
        });
    let transform_quantized_candidate =
        Some(lossy.transform_quantized_dpcm_residual_candidate(&analysis, horz));
    let choice = choose_lossy_txb(
        analysis.delta,
        &analysis.residual,
        &coefficients,
        Av2CoefficientProxyKind::LumaTransform,
        lossy_quality_rd_quant_step(lossy.quant_step()),
        lossy_max_txb_sse(lossy.quant_step()),
        analysis.dc_sse,
        analysis.dc_variance_loss,
        quantized_candidate,
        refined_quantized_candidate,
        transform_quantized_candidate,
        false,
    );
    lossy.record_txb_choice(plane, &choice, &analysis);
    match choice {
        Av2LossyTxbChoice::Exact => {
            let (context, _) = if tx4x4_residual_is_zero(&analysis.residual) {
                write_y_txb_all_zero(writer, skip_ctx);
                (0, false)
            } else {
                write_luma_palette_residual_txb(writer, skip_ctx, dc_sign_ctx, &coefficients)
            };
            lossy.copy_source_to_recon_txb(plane, x0, y0, &analysis);
            context
        }
        Av2LossyTxbChoice::QuantizedResidual(quantized_residual) => {
            let (context, _) = if tx4x4_residual_is_zero(&quantized_residual.residual) {
                write_y_txb_all_zero(writer, skip_ctx);
                (0, false)
            } else {
                write_luma_palette_residual_txb(
                    writer,
                    skip_ctx,
                    dc_sign_ctx,
                    &quantized_residual.coefficients,
                )
            };
            lossy.fill_dpcm_residual_recon_txb(
                plane,
                x0,
                y0,
                &analysis,
                &quantized_residual.residual,
                horz,
            );
            context
        }
        Av2LossyTxbChoice::DcDelta(_) => {
            unreachable!("lossy DPCM TXBs do not use DC-delta candidates")
        }
    }
}

fn write_lossy_chroma_txb(
    writer: &mut Av2EntropyWriter,
    chroma_plane: Av2ChromaPlane,
    skip_ctx: u8,
    lossy: &mut Av2LossySubsampledTileState<'_>,
    x0: usize,
    y0: usize,
    mode: Av2LossySubsampledModeDecision,
    context: Av2LossyLeafPredictorContext<'_>,
) -> (u8, bool) {
    let plane = match chroma_plane {
        Av2ChromaPlane::U => Av2LossyPlane::U,
        Av2ChromaPlane::V => Av2LossyPlane::V,
    };
    if mode.chroma_use_bdpcm {
        return write_lossy_chroma_dpcm_txb(
            writer,
            chroma_plane,
            skip_ctx,
            lossy,
            x0,
            y0,
            mode.chroma_intra_mode.is_horizontal(),
        );
    }
    let analysis = lossy.analyze_txb(plane, x0, y0, mode, context);
    if !mode.use_fsc {
        let candidate = choose_regular_q_lossy_txb(
            lossy.regular_dct_quantized_residual_candidates(&analysis),
            Av2CoefficientProxyKind::ChromaTransform,
            lossy.quant_step(),
        );
        let result = if regular_quantized_txb_is_zero(&candidate) {
            match chroma_plane {
                Av2ChromaPlane::U => write_u_txb_all_zero(writer, skip_ctx, false),
                Av2ChromaPlane::V => write_v_txb_all_zero(writer, skip_ctx),
            }
            (0, false)
        } else {
            write_chroma_bdpcm_txb(writer, chroma_plane, skip_ctx, &candidate.coefficients, false)
        };
        lossy.fill_residual_recon_txb(plane, x0, y0, &analysis, &candidate.residual);
        lossy.record_txb_choice(plane, &Av2LossyTxbChoice::QuantizedResidual(candidate), &analysis);
        return result;
    }
    let coefficients = tx4x4_coefficients_from_residual(&analysis.residual, mode.use_fsc);
    let quantized_candidate =
        if lossy_should_try_ac_quantized(analysis.dc_sse, lossy.quant_step()) {
            Some(lossy.quantized_residual_candidate(&analysis, mode.use_fsc))
        } else {
            None
        };
    let refined_quantized_candidate =
        quantized_candidate
            .as_ref()
            .filter(|candidate| {
                lossy_should_try_refined_ac_quantized(candidate.sse, lossy.quant_step())
            })
            .map(|_| lossy.refined_quantized_residual_candidate(&analysis, mode.use_fsc));
    let transform_quantized_candidate = (!mode.use_fsc).then(|| ()).and(quantized_candidate
        .as_ref()
        .map(|_| lossy.transform_quantized_residual_candidate(&analysis)));
    let choice = choose_lossy_txb(
        analysis.delta,
        &analysis.residual,
        &coefficients,
        Av2CoefficientProxyKind::ChromaTransform,
        lossy_quality_rd_quant_step(lossy.quant_step()),
        lossy_max_txb_sse(lossy.quant_step()),
        analysis.dc_sse,
        analysis.dc_variance_loss,
        quantized_candidate,
        refined_quantized_candidate,
        transform_quantized_candidate,
        !mode.use_fsc,
    );
    lossy.record_txb_choice(plane, &choice, &analysis);
    match choice {
        Av2LossyTxbChoice::Exact => {
            let result = if tx4x4_residual_is_zero(&analysis.residual) {
                match chroma_plane {
                    Av2ChromaPlane::U => write_u_txb_all_zero(writer, skip_ctx, mode.use_fsc),
                    Av2ChromaPlane::V => write_v_txb_all_zero(writer, skip_ctx),
                }
                (0, false)
            } else {
                write_chroma_bdpcm_txb(
                    writer,
                    chroma_plane,
                    skip_ctx,
                    &coefficients,
                    mode.use_fsc,
                )
            };
            lossy.copy_source_to_recon_txb(plane, x0, y0, &analysis);
            result
        }
        Av2LossyTxbChoice::QuantizedResidual(quantized_residual) => {
            let result = if tx4x4_residual_is_zero(&quantized_residual.residual) {
                match chroma_plane {
                    Av2ChromaPlane::U => write_u_txb_all_zero(writer, skip_ctx, mode.use_fsc),
                    Av2ChromaPlane::V => write_v_txb_all_zero(writer, skip_ctx),
                }
                (0, false)
            } else {
                write_chroma_bdpcm_txb(
                    writer,
                    chroma_plane,
                    skip_ctx,
                    &quantized_residual.coefficients,
                    mode.use_fsc,
                )
            };
            lossy.fill_residual_recon_txb(
                plane,
                x0,
                y0,
                &analysis,
                &quantized_residual.residual,
            );
            result
        }
        Av2LossyTxbChoice::DcDelta(delta) => {
            let result = write_chroma_dc_delta_txb(writer, chroma_plane, skip_ctx, delta);
            lossy.fill_quantized_recon_txb(plane, x0, y0, &analysis);
            result
        }
    }
}

fn write_lossy_chroma_dpcm_txb(
    writer: &mut Av2EntropyWriter,
    chroma_plane: Av2ChromaPlane,
    skip_ctx: u8,
    lossy: &mut Av2LossySubsampledTileState<'_>,
    x0: usize,
    y0: usize,
    horz: bool,
) -> (u8, bool) {
    let plane = match chroma_plane {
        Av2ChromaPlane::U => Av2LossyPlane::U,
        Av2ChromaPlane::V => Av2LossyPlane::V,
    };
    let analysis = lossy.analyze_dpcm_txb(plane, x0, y0, horz);
    let coefficients = tx4x4_coefficients_from_residual(&analysis.residual, false);
    let quantized_candidate = Some(lossy.quantized_dpcm_residual_candidate(
        &analysis,
        lossy.quant_step(),
        horz,
    ));
    let refined_quantized_candidate = quantized_candidate
        .as_ref()
        .filter(|candidate| lossy_should_try_refined_ac_quantized(candidate.sse, lossy.quant_step()))
        .map(|_| {
            lossy.quantized_dpcm_residual_candidate(
                &analysis,
                refined_lossy_quant_step(lossy.quant_step()),
                horz,
            )
        });
    let transform_quantized_candidate =
        Some(lossy.transform_quantized_dpcm_residual_candidate(&analysis, horz));
    let choice = choose_lossy_txb(
        analysis.delta,
        &analysis.residual,
        &coefficients,
        Av2CoefficientProxyKind::ChromaTransform,
        lossy_quality_rd_quant_step(lossy.quant_step()),
        lossy_max_txb_sse(lossy.quant_step()),
        analysis.dc_sse,
        analysis.dc_variance_loss,
        quantized_candidate,
        refined_quantized_candidate,
        transform_quantized_candidate,
        false,
    );
    lossy.record_txb_choice(plane, &choice, &analysis);
    match choice {
        Av2LossyTxbChoice::Exact => {
            let result = if tx4x4_residual_is_zero(&analysis.residual) {
                match chroma_plane {
                    Av2ChromaPlane::U => write_u_txb_all_zero(writer, skip_ctx, false),
                    Av2ChromaPlane::V => write_v_txb_all_zero(writer, skip_ctx),
                }
                (0, false)
            } else {
                write_chroma_bdpcm_txb(writer, chroma_plane, skip_ctx, &coefficients, false)
            };
            lossy.copy_source_to_recon_txb(plane, x0, y0, &analysis);
            result
        }
        Av2LossyTxbChoice::QuantizedResidual(quantized_residual) => {
            let result = if tx4x4_residual_is_zero(&quantized_residual.residual) {
                match chroma_plane {
                    Av2ChromaPlane::U => write_u_txb_all_zero(writer, skip_ctx, false),
                    Av2ChromaPlane::V => write_v_txb_all_zero(writer, skip_ctx),
                }
                (0, false)
            } else {
                write_chroma_bdpcm_txb(
                    writer,
                    chroma_plane,
                    skip_ctx,
                    &quantized_residual.coefficients,
                    false,
                )
            };
            lossy.fill_dpcm_residual_recon_txb(
                plane,
                x0,
                y0,
                &analysis,
                &quantized_residual.residual,
                horz,
            );
            result
        }
        Av2LossyTxbChoice::DcDelta(_) => {
            unreachable!("lossy DPCM TXBs do not use DC-delta candidates")
        }
    }
}

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

fn write_lossy_inter_residual_coefficients(
    writer: &mut Av2EntropyWriter,
    decision: Av2TileDecision,
    visible_rows_mi: usize,
    visible_cols_mi: usize,
    contexts: &mut Av2TxbEntropyContexts,
    lossy: &mut Av2LossySubsampledTileState<'_>,
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
            let (x0, y0) = lossy.txb_origin(Av2LossyPlane::Y, abs_col, abs_row);
            let analysis = lossy.analyze_inter_txb(
                reference,
                Av2LossyPlane::Y,
                x0,
                y0,
                mv_row_px,
                mv_col_px,
            );
            if tx4x4_residual_is_zero(&analysis.residual) {
                trace_inter_txb(
                    decision,
                    "y",
                    x0,
                    y0,
                    row,
                    col,
                    skip_ctx,
                    Some(dc_sign_ctx),
                    true,
                    true,
                    0,
                    None,
                );
                if use_regular_inter_txb_contexts {
                    write_y_inter_txb_all_zero(writer, skip_ctx);
                } else {
                    write_y_txb_all_zero(writer, skip_ctx);
                }
                let zero_residual = [0i32; TX4X4_SAMPLES];
                lossy.fill_residual_recon_txb(
                    Av2LossyPlane::Y,
                    x0,
                    y0,
                    &analysis,
                    &zero_residual,
                );
                lossy.record_txb_choice(Av2LossyPlane::Y, &Av2LossyTxbChoice::Exact, &analysis);
                contexts.y_above[abs_col] = 0;
                contexts.y_left[abs_row] = 0;
                continue;
            }
            let candidate = choose_regular_q_lossy_txb(
                lossy.regular_dct_quantized_residual_candidates(&analysis),
                Av2CoefficientProxyKind::LumaTransform,
                lossy.quant_step(),
            );
            let quantized_zero = regular_quantized_txb_is_zero(&candidate);
            trace_inter_txb(
                decision,
                "y",
                x0,
                y0,
                row,
                col,
                skip_ctx,
                Some(dc_sign_ctx),
                false,
                quantized_zero,
                quantized_txb_eob(&candidate.coefficients),
                Some(&candidate.coefficients),
            );
            trace_inter_candidate_residual("y", x0, y0, lossy.base_qindex(), &candidate);
            let (context, _) = if quantized_zero {
                if use_regular_inter_txb_contexts {
                    write_y_inter_txb_all_zero(writer, skip_ctx);
                } else {
                    write_y_txb_all_zero(writer, skip_ctx);
                }
                (0, false)
            } else if use_regular_inter_txb_contexts {
                write_luma_inter_residual_txb(writer, skip_ctx, dc_sign_ctx, &candidate.coefficients)
            } else {
                write_luma_palette_residual_txb(writer, skip_ctx, dc_sign_ctx, &candidate.coefficients)
            };
            lossy.fill_residual_recon_txb(
                Av2LossyPlane::Y,
                x0,
                y0,
                &analysis,
                &candidate.residual,
            );
            lossy.record_txb_choice(
                Av2LossyPlane::Y,
                &Av2LossyTxbChoice::QuantizedResidual(candidate),
                &analysis,
            );
            contexts.y_above[abs_col] = context;
            contexts.y_left[abs_row] = context;
        }
    }

    let chroma_span = chroma_tx4x4_span(
        decision,
        visible_rows_mi,
        visible_cols_mi,
        lossy.chroma_format,
    );
    if lossy.chroma_format == Av2ChromaFormat::Yuv444 {
        write_lossy_444_inter_chroma_txb(
            writer,
            decision,
            contexts,
            lossy,
            reference,
            mv_row_px,
            mv_col_px,
            chroma_span,
            use_regular_inter_txb_contexts,
        );
        return;
    }
    if lossy.chroma_format == Av2ChromaFormat::Yuv422 {
        write_lossy_422_inter_chroma_tx4x8(
            writer,
            contexts,
            lossy,
            reference,
            mv_row_px,
            mv_col_px,
            chroma_span,
            use_regular_inter_txb_contexts,
        );
        return;
    }
    let mut last_u_txb_nonzero = false;
    for row in 0..chroma_span.height {
        let abs_row = chroma_span.row + row;
        for col in 0..chroma_span.width {
            let abs_col = chroma_span.col + col;
            let skip_ctx =
                chroma_txb_skip_base_context(contexts.u_above[abs_col], contexts.u_left[abs_row])
                    + 6;
            let (x0, y0) = lossy.txb_origin(Av2LossyPlane::U, abs_col, abs_row);
            let analysis = lossy.analyze_inter_txb(
                reference,
                Av2LossyPlane::U,
                x0,
                y0,
                mv_row_px,
                mv_col_px,
            );
            if tx4x4_residual_is_zero(&analysis.residual) {
                trace_inter_txb(
                    decision, "u", x0, y0, row, col, skip_ctx, None, true, true, 0, None,
                );
                write_u_txb_all_zero(writer, skip_ctx, use_regular_inter_txb_contexts);
                let zero_residual = [0i32; TX4X4_SAMPLES];
                lossy.fill_residual_recon_txb(
                    Av2LossyPlane::U,
                    x0,
                    y0,
                    &analysis,
                    &zero_residual,
                );
                lossy.record_txb_choice(Av2LossyPlane::U, &Av2LossyTxbChoice::Exact, &analysis);
                contexts.u_above[abs_col] = 0;
                contexts.u_left[abs_row] = 0;
                last_u_txb_nonzero = false;
                continue;
            }
            let candidate = choose_regular_q_lossy_txb(
                lossy.regular_dct_quantized_residual_candidates(&analysis),
                Av2CoefficientProxyKind::ChromaTransform,
                lossy.quant_step(),
            );
            let quantized_zero = regular_quantized_txb_is_zero(&candidate);
            trace_inter_txb(
                decision,
                "u",
                x0,
                y0,
                row,
                col,
                skip_ctx,
                None,
                false,
                quantized_zero,
                quantized_txb_eob(&candidate.coefficients),
                Some(&candidate.coefficients),
            );
            let (context, nonzero) = if quantized_zero {
                write_u_txb_all_zero(writer, skip_ctx, use_regular_inter_txb_contexts);
                (0, false)
            } else {
                write_chroma_bdpcm_txb(
                    writer,
                    Av2ChromaPlane::U,
                    skip_ctx,
                    &candidate.coefficients,
                    use_regular_inter_txb_contexts,
                )
            };
            lossy.fill_residual_recon_txb(
                Av2LossyPlane::U,
                x0,
                y0,
                &analysis,
                &candidate.residual,
            );
            lossy.record_txb_choice(
                Av2LossyPlane::U,
                &Av2LossyTxbChoice::QuantizedResidual(candidate),
                &analysis,
            );
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
                lossy.chroma_format,
                decision.block_size,
            );
            let (x0, y0) = lossy.txb_origin(Av2LossyPlane::V, abs_col, abs_row);
            let analysis = lossy.analyze_inter_txb(
                reference,
                Av2LossyPlane::V,
                x0,
                y0,
                mv_row_px,
                mv_col_px,
            );
            if tx4x4_residual_is_zero(&analysis.residual) {
                trace_inter_txb(
                    decision, "v", x0, y0, row, col, skip_ctx, None, true, true, 0, None,
                );
                write_v_txb_all_zero(writer, skip_ctx);
                let zero_residual = [0i32; TX4X4_SAMPLES];
                lossy.fill_residual_recon_txb(
                    Av2LossyPlane::V,
                    x0,
                    y0,
                    &analysis,
                    &zero_residual,
                );
                lossy.record_txb_choice(Av2LossyPlane::V, &Av2LossyTxbChoice::Exact, &analysis);
                contexts.v_above[abs_col] = 0;
                contexts.v_left[abs_row] = 0;
                continue;
            }
            let candidate = choose_regular_q_lossy_txb(
                lossy.regular_dct_quantized_residual_candidates(&analysis),
                Av2CoefficientProxyKind::ChromaTransform,
                lossy.quant_step(),
            );
            let quantized_zero = regular_quantized_txb_is_zero(&candidate);
            trace_inter_txb(
                decision,
                "v",
                x0,
                y0,
                row,
                col,
                skip_ctx,
                None,
                false,
                quantized_zero,
                quantized_txb_eob(&candidate.coefficients),
                Some(&candidate.coefficients),
            );
            let (context, _) = if quantized_zero {
                write_v_txb_all_zero(writer, skip_ctx);
                (0, false)
            } else {
                write_chroma_bdpcm_txb(
                    writer,
                    Av2ChromaPlane::V,
                    skip_ctx,
                    &candidate.coefficients,
                    false,
                )
            };
            lossy.fill_residual_recon_txb(
                Av2LossyPlane::V,
                x0,
                y0,
                &analysis,
                &candidate.residual,
            );
            lossy.record_txb_choice(
                Av2LossyPlane::V,
                &Av2LossyTxbChoice::QuantizedResidual(candidate),
                &analysis,
            );
            contexts.v_above[abs_col] = context;
            contexts.v_left[abs_row] = context;
        }
    }
}

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
