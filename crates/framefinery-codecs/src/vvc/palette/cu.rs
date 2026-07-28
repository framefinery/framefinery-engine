fn append_vvc_palette_444_8x8_cu_with_events(
    cabac: &mut VvcCabacEncoder,
    ctx: &mut VvcCabacContexts,
    frame: &VvcSampledFrame,
    slice_config: VvcSliceSyntaxConfig,
    ibc_search: &mut VvcIbcHashSearch,
    request: VvcPaletteCuEmitRequest,
) -> bool {
    if !vvc_palette_cu_origin_is_visible(frame.geometry, request.origin_x, request.origin_y) {
        return false;
    }
    if request.write_split_flag {
        ctx.encode(cabac, VvcCabacContext::SplitFlag(request.split_ctx), false);
    }
    // H.266 7.3.11.4/7.4.12.4: with sps_ibc_enabled_flag set for this
    // 4:4:4 screen-content subset, cu_skip_flag and pred_mode_ibc_flag are
    // present before either IBC payload or pred_mode_plt_flag. The first IBC
    // subset never uses skip/merge because the encoder-side hash table can
    // choose candidates outside the decoder's merge list.
    ctx.encode(cabac, VvcCabacContext::CuSkipFlag(0), false);
    let origin_x = request.origin_x as usize;
    let origin_y = request.origin_y as usize;
    if vvc_exact_hash_ibc_444_enabled(frame) {
        if let Some(decision) = ibc_search.decide_8x8(frame, origin_x, origin_y) {
            ctx.encode(
                cabac,
                VvcCabacContext::PredModeIbcFlag(decision.pred_mode_ibc_ctx),
                true,
            );
            append_vvc_ibc_444_8x8_cu(cabac, ctx, decision);
            ibc_search.record_ibc_8x8(frame, decision);
            return false;
        }
    }
    if let Some(residual) = vvc_transform_skip_residual_444_left_8x8(
        frame,
        ibc_search,
        origin_x,
        origin_y,
        slice_config.slice_qp,
    ) {
        ctx.encode(
            cabac,
            VvcCabacContext::PredModeIbcFlag(residual.decision.pred_mode_ibc_ctx),
            true,
        );
        append_vvc_ibc_444_8x8_cu_residual(
            cabac,
            ctx,
            slice_config,
            frame.format.bit_depth,
            &residual,
        );
        ibc_search.record_ibc_8x8(frame, residual.decision);
        return false;
    }
    if let Some(bdpcm) =
        vvc_bdpcm_horizontal_444_8x8(frame, origin_x, origin_y, slice_config.slice_qp)
    {
        ctx.encode(
            cabac,
            VvcCabacContext::PredModeIbcFlag(ibc_search.pred_mode_ibc_ctx(origin_x, origin_y)),
            false,
        );
        ctx.encode(cabac, VvcCabacContext::PredModePltFlag, false);
        append_vvc_bdpcm_444_8x8_cu(cabac, ctx, slice_config, frame.format.bit_depth, &bdpcm);
        ibc_search.record_palette_8x8(frame, origin_x, origin_y);
        return false;
    }
    ctx.encode(
        cabac,
        VvcCabacContext::PredModeIbcFlag(ibc_search.pred_mode_ibc_ctx(origin_x, origin_y)),
        false,
    );
    ctx.encode(cabac, VvcCabacContext::PredModePltFlag, true);
    let syntax = vvc_palette_444_cu_syntax_with_config(
        frame,
        request.origin_x as usize,
        request.origin_y as usize,
        slice_config,
    );
    let palette_index_map = syntax.palette_indices.clone();
    let palette_escape_values = syntax.palette_escape_values.clone();
    let max_palette_index = syntax.max_palette_index;
    let palette_escape_val_present_flag = syntax.palette_escape_val_present_flag;
    for token in vvc_palette_444_syntax_tokens(syntax, request.predictor_mode) {
        append_palette_syntax_token_cabac(cabac, token);
    }
    append_vvc_palette_444_index_map(
        cabac,
        ctx,
        max_palette_index,
        palette_escape_val_present_flag,
        &palette_index_map,
        &palette_escape_values,
    );
    ibc_search.record_palette_8x8(frame, origin_x, origin_y);
    true
}

fn vvc_exact_hash_ibc_444_enabled(frame: &VvcSampledFrame) -> bool {
    frame.format.chroma_sampling == ChromaSampling::Cs444
}

fn append_vvc_ibc_444_8x8_cu(
    cabac: &mut VvcCabacEncoder,
    ctx: &mut VvcCabacContexts,
    decision: VvcIbcCuDecision,
) {
    append_vvc_ibc_444_8x8_prediction(cabac, ctx, decision);
    // H.266 7.3.11.4/7.4.12.4: cu_coded_flag=0 means no transform_tree()
    // follows; the exact-match IBC CU reconstructs entirely from prediction.
    ctx.encode(cabac, VvcCabacContext::CuCodedFlag(0), false);
}

fn append_vvc_ibc_444_8x8_prediction(
    cabac: &mut VvcCabacEncoder,
    ctx: &mut VvcCabacContexts,
    decision: VvcIbcCuDecision,
) {
    // H.266 7.3.11.4: MODE_IBC with cu_skip_flag=0 signals
    // general_merge_flag. Keep it 0 so the explicit BVD from our 32-bit
    // hash-search decision is coded instead of selecting from merge_idx.
    ctx.encode(cabac, VvcCabacContext::GeneralMergeFlag(0), false);
    append_vvc_ibc_mvd_coding(cabac, ctx, decision.mvd_x, decision.mvd_y);
    // MaxNumIbcMergeCand is fixed to one in the SPS, so mvp_l0_flag is
    // inferred. sps_amvr_enabled_flag is also false, so amvr_precision_idx is
    // absent; H.266 Table 16 then scales the coded integer-sample IBC MVD into
    // the 1/16 luma-sample BVD consumed by H.266 8.6.2.1.
    //
}

fn append_vvc_bdpcm_444_8x8_cu(
    cabac: &mut VvcCabacEncoder,
    ctx: &mut VvcCabacContexts,
    slice_config: VvcSliceSyntaxConfig,
    bit_depth: SampleBitDepth,
    residual: &VvcBdpcm444Cu,
) {
    // H.266 7.3.11.4/7.4.12.5: an intra BDPCM CU first signals
    // intra_bdpcm_luma_flag and intra_bdpcm_luma_dir_flag through
    // bdpcm_mode(); intra_luma_pred_modes() then infers horizontal mode.
    // The current simple subset uses horizontal BDPCM for both luma and chroma.
    ctx.encode(cabac, VvcCabacContext::BdpcmMode(0), true);
    ctx.encode(cabac, VvcCabacContext::BdpcmMode(1), false);
    ctx.encode(cabac, VvcCabacContext::BdpcmMode(2), true);
    ctx.encode(cabac, VvcCabacContext::BdpcmMode(3), false);

    // H.266 7.3.11.10 and VTM CABACWriter::cbf_comp(): BDPCM remaps CBF
    // contexts to 1 for Y/Cb and 2 for Cr, independent of prevCbf.
    ctx.encode(cabac, VvcCabacContext::QtCbfCb(1), residual.cbf_cb);
    ctx.encode(cabac, VvcCabacContext::QtCbfCr(2), residual.cbf_cr);
    ctx.encode(cabac, VvcCabacContext::QtCbfY(1), residual.cbf_y);

    let mut encoder = VvcResidualCabacEncoder::new(ctx, slice_config.residual_options());
    let y_coeffs = vvc_palette_transform_skip_coded_coefficients(
        &residual.y_coeffs,
        bit_depth,
        slice_config.slice_qp,
    );
    let cb_coeffs = vvc_palette_transform_skip_coded_coefficients(
        &residual.cb_coeffs,
        bit_depth,
        slice_config.slice_qp,
    );
    let cr_coeffs = vvc_palette_transform_skip_coded_coefficients(
        &residual.cr_coeffs,
        bit_depth,
        slice_config.slice_qp,
    );
    if residual.cbf_y {
        VvcResidualCabacSymbolStream::emit_luma_bdpcm_transform_skip_coefficients(
            3,
            3,
            &y_coeffs,
            &mut encoder,
            cabac,
        )
    }
    if residual.cbf_cb {
        VvcResidualCabacSymbolStream::emit_chroma_bdpcm_transform_skip_coefficients(
            VvcResidualComponent::ChromaCb,
            3,
            3,
            &cb_coeffs,
            &mut encoder,
            cabac,
        )
    }
    if residual.cbf_cr {
        VvcResidualCabacSymbolStream::emit_chroma_bdpcm_transform_skip_coefficients(
            VvcResidualComponent::ChromaCr,
            3,
            3,
            &cr_coeffs,
            &mut encoder,
            cabac,
        )
    }
}

fn append_vvc_ibc_444_8x8_cu_residual(
    cabac: &mut VvcCabacEncoder,
    ctx: &mut VvcCabacContexts,
    slice_config: VvcSliceSyntaxConfig,
    bit_depth: SampleBitDepth,
    residual: &VvcTransformSkipResidual444Cu,
) {
    append_vvc_ibc_444_8x8_prediction(cabac, ctx, residual.decision);
    ctx.encode(cabac, VvcCabacContext::CuCodedFlag(0), true);

    // H.266 7.3.11.10 transform_unit(), non-separate 4:4:4 tree:
    // chroma cbf flags are coded before luma. For inter/IBC CUs with no
    // chroma CBF, the luma CBF at transform depth 0 is inferred true by VTM
    // CABACWriter::transform_unit(); otherwise QtCbf[Y][0] is signalled.
    ctx.encode(cabac, VvcCabacContext::QtCbfCb(0), residual.cbf_cb);
    ctx.encode(
        cabac,
        VvcCabacContext::QtCbfCr(u8::from(residual.cbf_cb)),
        residual.cbf_cr,
    );
    if residual.cbf_cb || residual.cbf_cr {
        ctx.encode(cabac, VvcCabacContext::QtCbfY(0), residual.cbf_y);
    } else {
        debug_assert!(residual.cbf_y);
    }

    let mut encoder = VvcResidualCabacEncoder::new(ctx, slice_config.residual_options());
    let y_coeffs = vvc_palette_transform_skip_coded_coefficients(
        &residual.y_coeffs,
        bit_depth,
        slice_config.slice_qp,
    );
    let cb_coeffs = vvc_palette_transform_skip_coded_coefficients(
        &residual.cb_coeffs,
        bit_depth,
        slice_config.slice_qp,
    );
    let cr_coeffs = vvc_palette_transform_skip_coded_coefficients(
        &residual.cr_coeffs,
        bit_depth,
        slice_config.slice_qp,
    );
    if residual.cbf_y {
        VvcResidualCabacSymbolStream::emit_luma_transform_skip_coefficients(
            3,
            3,
            &y_coeffs,
            &mut encoder,
            cabac,
        );
    }
    if residual.cbf_cb {
        VvcResidualCabacSymbolStream::emit_chroma_transform_skip_coefficients(
            VvcResidualComponent::ChromaCb,
            3,
            3,
            &cb_coeffs,
            &mut encoder,
            cabac,
        )
    }
    if residual.cbf_cr {
        VvcResidualCabacSymbolStream::emit_chroma_transform_skip_coefficients(
            VvcResidualComponent::ChromaCr,
            3,
            3,
            &cr_coeffs,
            &mut encoder,
            cabac,
        )
    }
}

fn append_vvc_ibc_mvd_coding(
    cabac: &mut VvcCabacEncoder,
    ctx: &mut VvcCabacContexts,
    mvd_x: i16,
    mvd_y: i16,
) {
    let abs_x = i32::from(mvd_x).unsigned_abs();
    let abs_y = i32::from(mvd_y).unsigned_abs();
    ctx.encode(cabac, VvcCabacContext::AbsMvdGreater0Flag(0), abs_x > 0);
    ctx.encode(cabac, VvcCabacContext::AbsMvdGreater0Flag(0), abs_y > 0);
    if abs_x > 0 {
        ctx.encode(cabac, VvcCabacContext::AbsMvdGreater1Flag(0), abs_x > 1);
    }
    if abs_y > 0 {
        ctx.encode(cabac, VvcCabacContext::AbsMvdGreater1Flag(0), abs_y > 1);
    }
    if abs_x > 0 {
        if abs_x > 1 {
            encode_exp_golomb_ep_combined(cabac, abs_x - 2, 1);
        }
        cabac.encode_bin_ep(mvd_x < 0);
    }
    if abs_y > 0 {
        if abs_y > 1 {
            encode_exp_golomb_ep_combined(cabac, abs_y - 2, 1);
        }
        cabac.encode_bin_ep(mvd_y < 0);
    }
}

fn append_vvc_palette_444_index_map(
    cabac: &mut VvcCabacEncoder,
    ctx: &mut VvcCabacContexts,
    max_palette_index: u8,
    palette_escape_val_present_flag: bool,
    palette_indices: &[u8],
    palette_escape_values: &[Option<VvcSampledColor>],
) {
    if max_palette_index == 0 {
        return;
    }

    ctx.encode(cabac, VvcCabacContext::PaletteTransposeFlag, false);
    let scan_positions = vvc_palette_horizontal_scan_positions(8, 8);
    let scan_indices: Vec<u8> = scan_positions
        .iter()
        .map(|&(x, y)| palette_indices[y * 8 + x])
        .collect();
    let mut prev_run_pos = 0usize;
    let mut previous_run_type_copy_above = false;
    let mut prev_index = 0u8;
    let mut run_copy_flags = [false; 16];

    for min_sub_pos in (0..scan_indices.len()).step_by(16) {
        let max_sub_pos = (min_sub_pos + 16).min(scan_indices.len());

        for cur_pos in min_sub_pos..max_sub_pos {
            let index = scan_indices[cur_pos];
            let identity = cur_pos > 0 && index == prev_index;
            run_copy_flags[cur_pos - min_sub_pos] = identity;
            if cur_pos > 0 {
                let dist = cur_pos - prev_run_pos - 1;
                ctx.encode(
                    cabac,
                    VvcCabacContext::RunCopyFlag(vvc_palette_run_copy_ctx_id(
                        dist,
                        previous_run_type_copy_above,
                    )),
                    identity,
                );
            }
            if !identity || cur_pos == 0 {
                let (_, y) = scan_positions[cur_pos];
                let run_type_is_inferred_index = y == 0;
                prev_run_pos = cur_pos;
                if cur_pos != 0 && !run_type_is_inferred_index {
                    ctx.encode(cabac, VvcCabacContext::CopyAbovePaletteIndicesFlag, false);
                }
                previous_run_type_copy_above = false;
            };
            prev_index = index;
        }

        for cur_pos in min_sub_pos..max_sub_pos {
            if run_copy_flags[cur_pos - min_sub_pos] {
                continue;
            }
            let index = scan_indices[cur_pos];
            let max_symbol = max_palette_index as u32 + 1 - u32::from(cur_pos > 0);
            if max_symbol <= 1 {
                continue;
            }
            let mut level = index as u32;
            if cur_pos > 0 {
                let previous = scan_indices[cur_pos - 1] as u32;
                debug_assert_ne!(level, previous);
                if level > previous {
                    level -= 1;
                }
            }
            encode_trunc_bin_code_ep(cabac, level, max_symbol);
        }

        if palette_escape_val_present_flag {
            for component in 0..3 {
                for cur_pos in min_sub_pos..max_sub_pos {
                    if scan_indices[cur_pos] != max_palette_index {
                        continue;
                    }
                    let (x, y) = scan_positions[cur_pos];
                    let sample = palette_escape_values[y * 8 + x]
                        .expect("escape-coded palette index must carry coded component levels");
                    let value = match component {
                        0 => sample.y,
                        1 => sample.u,
                        _ => sample.v,
                    };
                    // H.266 7.3.11.6 writes palette_escape_val after each
                    // 16-sample palette-index subset for samples whose
                    // PaletteIndexMap equals MaxPaletteIndex. Per Table 130,
                    // palette_escape_val is bypass-coded; H.266 9.3.3 uses
                    // EG5 binarization for this syntax element.
                    encode_exp_golomb_ep_combined(cabac, value as u32, 5);
                }
            }
        }
    }
}

fn vvc_palette_horizontal_scan_positions(width: usize, height: usize) -> Vec<(usize, usize)> {
    let mut scanned = Vec::with_capacity(width * height);
    for y in 0..height {
        if y % 2 == 0 {
            for x in 0..width {
                scanned.push((x, y));
            }
        } else {
            for x in (0..width).rev() {
                scanned.push((x, y));
            }
        }
    }
    scanned
}

fn vvc_palette_run_copy_ctx_id(dist: usize, previous_run_type_copy_above: bool) -> u8 {
    // H.266 9.3.4.2.11 and Table 134 derive run_copy_flag ctxInc from
    // binDist and PreviousRunType. The current encoder only selects index runs,
    // but keep the copy-above half labelled for the mixed palette path.
    match (previous_run_type_copy_above, dist) {
        (true, 0) => 5,
        (true, 1 | 2) => 6,
        (true, _) => 7,
        (false, 0) => 0,
        (false, 1) => 1,
        (false, 2) => 2,
        (false, 3) => 3,
        (false, _) => 4,
    }
}

#[cfg(test)]
pub(super) fn vvc_palette_run_copy_context_id_for_audit(
    dist: usize,
    previous_run_type_copy_above: bool,
) -> u8 {
    vvc_palette_run_copy_ctx_id(dist, previous_run_type_copy_above)
}

#[cfg(test)]
pub(super) fn vvc_palette_444_context_audit_rows() -> Vec<(&'static str, u8, u8)> {
    let mut rows = vec![
        (
            "pred_mode_plt_flag[0]",
            VvcCabacContext::PredModePltFlag.init_value(),
            VvcCabacContext::PredModePltFlag.log2_window_size(),
        ),
        (
            "palette_transpose_flag[0]",
            VvcCabacContext::PaletteTransposeFlag.init_value(),
            VvcCabacContext::PaletteTransposeFlag.log2_window_size(),
        ),
        (
            "copy_above_palette_indices_flag[0]",
            VvcCabacContext::CopyAbovePaletteIndicesFlag.init_value(),
            VvcCabacContext::CopyAbovePaletteIndicesFlag.log2_window_size(),
        ),
    ];
    for idx in 0..8 {
        let ctx = VvcCabacContext::RunCopyFlag(idx);
        rows.push(("run_copy_flag", ctx.init_value(), ctx.log2_window_size()));
    }
    rows
}

fn vvc_palette_cu_origin_is_visible(
    geometry: VvcVideoGeometry,
    origin_x: u16,
    origin_y: u16,
) -> bool {
    (origin_x as usize) < geometry.width && (origin_y as usize) < geometry.height
}
