fn write_y_black_dc_txb(writer: &mut Av2EntropyWriter, skip_ctx: u8, dc_sign_ctx: u8) {
    write_y_txb_nonzero(writer, skip_ctx);
    write_eob_one_y(writer);
    write_y_dc_level(writer, BLACK_LOSSLESS_DC_LEVEL);
    write_y_negative_dc_sign(writer, dc_sign_ctx);
    write_y_dc_high_range(writer, BLACK_LOSSLESS_DC_LEVEL);
}

fn write_y_dc_delta_txb(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    dc_sign_ctx: u8,
    delta: i16,
) -> u8 {
    let coefficients = dc_delta_coefficients(delta);
    let (context, _) = write_luma_palette_residual_txb(writer, skip_ctx, dc_sign_ctx, &coefficients);
    context
}

fn write_u_black_dc_txb(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    let context = write_u_lossless_dc_txb(writer, skip_ctx, 0);
    assert_eq!(context, NONZERO_NEGATIVE_DC_ENTROPY_CONTEXT);
}

fn write_v_black_dc_txb(writer: &mut Av2EntropyWriter, skip_ctx: u8) {
    let context = write_v_lossless_dc_txb(writer, skip_ctx, 0);
    assert_eq!(context, NONZERO_NEGATIVE_DC_ENTROPY_CONTEXT);
}

fn write_u_lossless_dc_txb(writer: &mut Av2EntropyWriter, skip_ctx: u8, sample: u8) -> u8 {
    if sample == LOSSLESS_DC_PREDICTOR {
        write_u_txb_all_zero(writer, skip_ctx, false);
        return 0;
    }
    let (level, negative) = lossless_dc_level_for_sample(sample);
    write_u_txb_nonzero(writer, skip_ctx, false);
    write_eob_one_uv(writer);
    write_uv_dc_level(writer, level);
    writer.write_literal_bit("tile.coeff.u.dc_sign_negative", negative);
    write_uv_dc_high_range(writer, level);
    nonzero_dc_entropy_context(negative)
}

fn write_v_lossless_dc_txb(writer: &mut Av2EntropyWriter, skip_ctx: u8, sample: u8) -> u8 {
    if sample == LOSSLESS_DC_PREDICTOR {
        write_v_txb_all_zero(writer, skip_ctx);
        return 0;
    }
    let (level, negative) = lossless_dc_level_for_sample(sample);
    write_v_txb_nonzero(writer, skip_ctx);
    write_eob_one_uv(writer);
    write_uv_dc_level(writer, level);
    writer.write_literal_bit("tile.coeff.v.dc_sign_negative", negative);
    write_uv_dc_high_range(writer, level);
    nonzero_dc_entropy_context(negative)
}

fn write_chroma_dc_delta_txb(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    skip_ctx: u8,
    delta: i16,
) -> (u8, bool) {
    let coefficients = dc_delta_coefficients(delta);
    write_chroma_bdpcm_txb(writer, plane, skip_ctx, &coefficients, false)
}

fn dc_delta_coefficients(delta: i16) -> [i32; TX4X4_SAMPLES] {
    let mut coefficients = [0i32; TX4X4_SAMPLES];
    coefficients[0] = i32::from(delta) * 32;
    coefficients
}

fn luma_palette_tx4x4_coefficients(
    palette: &Av2LumaPalette444,
    region: &Av2LumaPaletteRegion,
    x0: usize,
    y0: usize,
) -> [i32; TX4X4_SAMPLES] {
    let mut residual = [0i32; TX4X4_SAMPLES];
    for local_y in 0..TX4X4_SIZE {
        let y = y0 + local_y;
        for local_x in 0..TX4X4_SIZE {
            let x = x0 + local_x;
            let original = i32::from(palette.y_sample(x, y));
            let predicted = i32::from(palette.region_prediction_sample(region, x, y));
            residual[local_y * TX4X4_SIZE + local_x] = original - predicted;
        }
    }

    av2_fwht4x4(&residual)
}

fn luma_palette_idtx4x4_coefficients(
    palette: &Av2LumaPalette444,
    x0: usize,
    y0: usize,
) -> [i32; TX4X4_SAMPLES] {
    let mut residual = [0i32; TX4X4_SAMPLES];
    for local_y in 0..TX4X4_SIZE {
        let y = y0 + local_y;
        for local_x in 0..TX4X4_SIZE {
            let x = x0 + local_x;
            let original = i32::from(palette.y_sample(x, y));
            let predicted = i32::from(palette.luma_prediction_sample(x, y));
            residual[local_y * TX4X4_SIZE + local_x] = original - predicted;
        }
    }

    idtx4x4_coefficients(&residual)
}

fn luma_palette_fsc_is_rate_worthy(
    palette: &Av2LumaPalette444,
    leaf_x0: usize,
    leaf_y0: usize,
    tile_origin_x: usize,
    tile_origin_y: usize,
    chroma_use_bdpcm: bool,
    chroma_intra_mode: Av2ChromaIntraMode,
) -> bool {
    let coded_mi_context = Av2CodedMiContext::new(PARTITION_CONTEXT_DIM, PARTITION_CONTEXT_DIM);
    let region = palette.syntax_region_palette(
        leaf_x0,
        leaf_y0,
        AV2_LUMA_PALETTE_BLOCK_SIZE,
        AV2_LUMA_PALETTE_BLOCK_SIZE,
    );
    let mut fsc_score = 96usize;
    let mut transform_score = 0usize;

    for row in 0..(AV2_LUMA_PALETTE_BLOCK_SIZE / TX4X4_SIZE) {
        for col in 0..(AV2_LUMA_PALETTE_BLOCK_SIZE / TX4X4_SIZE) {
            let txb_x0 = leaf_x0 + col * TX4X4_SIZE;
            let txb_y0 = leaf_y0 + row * TX4X4_SIZE;

            fsc_score += coefficient_proxy_score(
                &luma_palette_idtx4x4_coefficients(palette, txb_x0, txb_y0),
                Av2CoefficientProxyKind::LumaIdtx,
            );
            transform_score += coefficient_proxy_score(
                &luma_palette_tx4x4_coefficients(palette, &region, txb_x0, txb_y0),
                Av2CoefficientProxyKind::LumaTransform,
            );

            for plane in [Av2ChromaPlane::U, Av2ChromaPlane::V] {
                fsc_score += coefficient_proxy_score(
                    &chroma_idtx4x4_coefficients(
                        palette,
                        plane,
                        txb_x0,
                        txb_y0,
                        tile_origin_x,
                        tile_origin_y,
                        leaf_x0,
                        leaf_y0,
                        AV2_LUMA_PALETTE_BLOCK_SIZE,
                        AV2_LUMA_PALETTE_BLOCK_SIZE,
                        &coded_mi_context,
                        chroma_use_bdpcm,
                        chroma_intra_mode,
                    ),
                    Av2CoefficientProxyKind::ChromaTransform,
                );
                let transform_coefficients = if chroma_use_bdpcm {
                    chroma_bdpcm_tx4x4_coefficients(
                        palette,
                        plane,
                        txb_x0,
                        txb_y0,
                        tile_origin_x,
                        tile_origin_y,
                        chroma_intra_mode.is_horizontal(),
                    )
                } else {
                    chroma_intra_tx4x4_coefficients(
                        palette,
                        plane,
                        txb_x0,
                        txb_y0,
                        tile_origin_x,
                        tile_origin_y,
                        leaf_x0,
                        leaf_y0,
                        AV2_LUMA_PALETTE_BLOCK_SIZE,
                        AV2_LUMA_PALETTE_BLOCK_SIZE,
                        &coded_mi_context,
                        chroma_intra_mode,
                    )
                };
                transform_score += coefficient_proxy_score(
                    &transform_coefficients,
                    Av2CoefficientProxyKind::ChromaTransform,
                );
            }
        }
    }

    fsc_score < transform_score
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Av2CoefficientProxyKind {
    LumaTransform,
    LumaIdtx,
    ChromaTransform,
}

fn coefficient_proxy_score(
    coefficients: &[i32; TX4X4_SAMPLES],
    kind: Av2CoefficientProxyKind,
) -> usize {
    let (levels, bounds) = lossless_coefficient_levels_and_bounds(coefficients);
    let Some((first, eob)) = bounds else {
        return 16;
    };

    let range = match kind {
        Av2CoefficientProxyKind::LumaIdtx => first..TX4X4_SAMPLES,
        Av2CoefficientProxyKind::LumaTransform | Av2CoefficientProxyKind::ChromaTransform => 0..eob,
    };
    let mut score = 96 + range.len() * 10;
    for scan_index in range {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos] as usize;
        if level == 0 {
            continue;
        }
        score += 80 + level.min(12) * 14;
    }

    let mut high_range_avg = 0u32;
    match kind {
        Av2CoefficientProxyKind::LumaIdtx => {
            for scan_index in 0..TX4X4_SAMPLES {
                score += coefficient_high_range_proxy_score(
                    &levels,
                    kind,
                    TX4X4_SCAN[scan_index],
                    &mut high_range_avg,
                );
            }
        }
        Av2CoefficientProxyKind::LumaTransform | Av2CoefficientProxyKind::ChromaTransform => {
            for scan_index in (0..eob).rev() {
                score += coefficient_high_range_proxy_score(
                    &levels,
                    kind,
                    TX4X4_SCAN[scan_index],
                    &mut high_range_avg,
                );
            }
        }
    }

    score
}

fn dc_delta_coefficient_proxy_score(delta: i16, kind: Av2CoefficientProxyKind) -> usize {
    let level = u32::from(delta.unsigned_abs()) * 4;
    if level == 0 {
        return 16;
    }

    let mut score = 96 + 10 + 80 + level.min(12) as usize * 14;
    let mut high_range_avg = 0u32;
    score += coefficient_high_range_level_proxy_score(level, kind, 0, &mut high_range_avg);
    score
}

fn coefficient_high_range_proxy_score(
    levels: &[u32; TX4X4_SAMPLES],
    kind: Av2CoefficientProxyKind,
    pos: usize,
    high_range_avg: &mut u32,
) -> usize {
    let level = levels[pos];
    coefficient_high_range_level_proxy_score(level, kind, pos, high_range_avg)
}

fn coefficient_high_range_level_proxy_score(
    level: u32,
    kind: Av2CoefficientProxyKind,
    pos: usize,
    high_range_avg: &mut u32,
) -> usize {
    if level == 0 {
        return 0;
    }
    let (threshold, decoded_base) = match kind {
        Av2CoefficientProxyKind::LumaIdtx => (5, 6),
        Av2CoefficientProxyKind::LumaTransform if luma_lf_limits(pos) => (7, 8),
        Av2CoefficientProxyKind::LumaTransform => (5, 6),
        Av2CoefficientProxyKind::ChromaTransform if chroma_lf_limits(pos) => (4, 5),
        Av2CoefficientProxyKind::ChromaTransform => (5, 6),
    };
    if level <= threshold {
        return 0;
    }
    let high_range = level.saturating_sub(decoded_base);
    let score = adaptive_high_range_score_bits(high_range, *high_range_avg) * 64;
    *high_range_avg = (*high_range_avg + high_range) >> 1;
    score
}

fn adaptive_high_range_score_bits(value: u32, context: u32) -> usize {
    let m = adaptive_high_range_rice_parameter(context);
    truncated_rice_score_bits(value, m, m + 1, (m + 4).min(6))
}

fn truncated_rice_score_bits(value: u32, m: u8, k: u8, cmax: u8) -> usize {
    let q = value >> m;
    if q >= u32::from(cmax) {
        usize::from(cmax) + exp_golomb_score_bits(value - (u32::from(cmax) << m), k)
    } else {
        q as usize + 1 + usize::from(m)
    }
}

fn exp_golomb_score_bits(value: u32, k: u8) -> usize {
    let x = value + (1u32 << k);
    let length = (u32::BITS - x.leading_zeros()) as u8;
    usize::from(length - 1 - k + length)
}

fn luma_bdpcm_tx4x4_coefficients(
    palette: &Av2LumaPalette444,
    x0: usize,
    y0: usize,
    tile_origin_x: usize,
    tile_origin_y: usize,
    horz: bool,
) -> [i32; TX4X4_SAMPLES] {
    let mut residual = [0i32; TX4X4_SAMPLES];
    for local_y in 0..TX4X4_SIZE {
        let y = y0 + local_y;
        for local_x in 0..TX4X4_SIZE {
            let x = x0 + local_x;
            let sample = i32::from(palette.y_sample(x, y));
            let predicted_delta = if horz {
                let row_predictor = i32::from(luma_h_predictor(
                    palette,
                    x0,
                    y0,
                    local_y,
                    tile_origin_x,
                    tile_origin_y,
                ));
                if local_x == 0 {
                    sample - row_predictor
                } else {
                    let previous = i32::from(palette.y_sample(x - 1, y));
                    sample - previous
                }
            } else if local_y == 0 {
                let col_predictor = i32::from(luma_v_predictor(
                    palette,
                    x0,
                    y0,
                    local_x,
                    tile_origin_x,
                    tile_origin_y,
                ));
                sample - col_predictor
            } else {
                let previous = i32::from(palette.y_sample(x, y - 1));
                sample - previous
            };
            residual[local_y * TX4X4_SIZE + local_x] = predicted_delta;
        }
    }

    av2_fwht4x4(&residual)
}

fn chroma_bdpcm_tx4x4_coefficients(
    palette: &Av2LumaPalette444,
    plane: Av2ChromaPlane,
    x0: usize,
    y0: usize,
    tile_origin_x: usize,
    tile_origin_y: usize,
    horz: bool,
) -> [i32; TX4X4_SAMPLES] {
    let residual =
        chroma_bdpcm_residual4x4(palette, plane, x0, y0, tile_origin_x, tile_origin_y, horz);
    av2_fwht4x4(&residual)
}

fn chroma_bdpcm_residual4x4(
    palette: &Av2LumaPalette444,
    plane: Av2ChromaPlane,
    x0: usize,
    y0: usize,
    tile_origin_x: usize,
    tile_origin_y: usize,
    horz: bool,
) -> [i32; TX4X4_SAMPLES] {
    let mut residual = [0i32; TX4X4_SAMPLES];
    for local_y in 0..TX4X4_SIZE {
        let y = y0 + local_y;
        for local_x in 0..TX4X4_SIZE {
            let x = x0 + local_x;
            let sample = i32::from(chroma_sample(palette, plane, x, y));
            let predicted_delta = if horz {
                let row_predictor = i32::from(chroma_h_predictor(
                    palette,
                    plane,
                    x0,
                    y0,
                    local_y,
                    tile_origin_x,
                    tile_origin_y,
                ));
                if local_x == 0 {
                    sample - row_predictor
                } else {
                    let previous = i32::from(chroma_sample(palette, plane, x - 1, y));
                    sample - previous
                }
            } else if local_y == 0 {
                let col_predictor = i32::from(chroma_v_predictor(
                    palette,
                    plane,
                    x0,
                    y0,
                    local_x,
                    tile_origin_x,
                    tile_origin_y,
                ));
                sample - col_predictor
            } else {
                let previous = i32::from(chroma_sample(palette, plane, x, y - 1));
                sample - previous
            };
            residual[local_y * TX4X4_SIZE + local_x] = predicted_delta;
        }
    }

    residual
}

fn chroma_intra_tx4x4_coefficients(
    palette: &Av2LumaPalette444,
    plane: Av2ChromaPlane,
    x0: usize,
    y0: usize,
    tile_origin_x: usize,
    tile_origin_y: usize,
    leaf_x0: usize,
    leaf_y0: usize,
    leaf_width: usize,
    leaf_height: usize,
    coded_mi_context: &Av2CodedMiContext,
    mode: Av2ChromaIntraMode,
) -> [i32; TX4X4_SAMPLES] {
    let residual = chroma_intra_residual4x4(
        palette,
        plane,
        x0,
        y0,
        tile_origin_x,
        tile_origin_y,
        leaf_x0,
        leaf_y0,
        leaf_width,
        leaf_height,
        coded_mi_context,
        mode,
    );
    av2_fwht4x4(&residual)
}

fn chroma_intra_residual4x4(
    palette: &Av2LumaPalette444,
    plane: Av2ChromaPlane,
    x0: usize,
    y0: usize,
    tile_origin_x: usize,
    tile_origin_y: usize,
    leaf_x0: usize,
    leaf_y0: usize,
    leaf_width: usize,
    leaf_height: usize,
    coded_mi_context: &Av2CodedMiContext,
    mode: Av2ChromaIntraMode,
) -> [i32; TX4X4_SAMPLES] {
    av2_intra_residual4x4(
        mode,
        None,
        palette.bit_depth(),
        |local_x, local_y| chroma_sample(palette, plane, x0 + local_x, y0 + local_y),
        || chroma_dc_predictor(palette, plane, x0, y0),
        |local_y| chroma_h_predictor(palette, plane, x0, y0, local_y, tile_origin_x, tile_origin_y),
        |local_x| chroma_v_predictor(palette, plane, x0, y0, local_x, tile_origin_x, tile_origin_y),
        || chroma_above_left_predictor(palette, plane, x0, y0, tile_origin_x, tile_origin_y),
        |_angle, local_x, local_y| match mode {
            Av2ChromaIntraMode::Directional45 => {
                let above = chroma_d45_above_edge(
                    palette,
                    plane,
                    x0,
                    y0,
                    tile_origin_x,
                    tile_origin_y,
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    coded_mi_context,
                );
                above[local_y + local_x + 1]
            }
            Av2ChromaIntraMode::Directional67 => {
                let above = chroma_d45_above_edge(
                    palette,
                    plane,
                    x0,
                    y0,
                    tile_origin_x,
                    tile_origin_y,
                    leaf_x0,
                    leaf_y0,
                    leaf_width,
                    coded_mi_context,
                );
                directional_interpolate(above, local_x, local_y)
            }
            Av2ChromaIntraMode::Directional135 => {
                let edges = chroma_d135_edges(palette, plane, x0, y0, tile_origin_x, tile_origin_y);
                if local_x >= local_y {
                    let offset = local_x - local_y;
                    if offset == 0 {
                        edges.above_left
                    } else {
                        edges.above[offset - 1]
                    }
                } else {
                    edges.left[local_y - local_x - 1]
                }
            }
            Av2ChromaIntraMode::Directional113 => {
                let edges = chroma_d135_edges(palette, plane, x0, y0, tile_origin_x, tile_origin_y);
                zone2_directional_predictor(edges, 24, 170, local_x, local_y)
            }
            Av2ChromaIntraMode::Directional157 => {
                let edges = chroma_d135_edges(palette, plane, x0, y0, tile_origin_x, tile_origin_y);
                zone2_directional_predictor(edges, 170, 24, local_x, local_y)
            }
            Av2ChromaIntraMode::Directional203 => {
                let left = chroma_d203_left_edge(
                    palette,
                    plane,
                    x0,
                    y0,
                    tile_origin_x,
                    tile_origin_y,
                    leaf_x0,
                    leaf_y0,
                    leaf_height,
                    coded_mi_context,
                );
                directional_interpolate(left, local_y, local_x)
            }
            _ => unreachable!("4:4:4 chroma path only dispatches directional modes here"),
        },
        || {
            chroma_smooth_edges(
                palette,
                plane,
                x0,
                y0,
                tile_origin_x,
                tile_origin_y,
                leaf_x0,
                leaf_y0,
                leaf_width,
                leaf_height,
                coded_mi_context,
            )
        },
    )
}

fn chroma_idtx4x4_coefficients(
    palette: &Av2LumaPalette444,
    plane: Av2ChromaPlane,
    x0: usize,
    y0: usize,
    tile_origin_x: usize,
    tile_origin_y: usize,
    leaf_x0: usize,
    leaf_y0: usize,
    leaf_width: usize,
    leaf_height: usize,
    coded_mi_context: &Av2CodedMiContext,
    use_bdpcm: bool,
    mode: Av2ChromaIntraMode,
) -> [i32; TX4X4_SAMPLES] {
    let residual = if use_bdpcm {
        chroma_bdpcm_residual4x4(
            palette,
            plane,
            x0,
            y0,
            tile_origin_x,
            tile_origin_y,
            mode.is_horizontal(),
        )
    } else {
        chroma_intra_residual4x4(
            palette,
            plane,
            x0,
            y0,
            tile_origin_x,
            tile_origin_y,
            leaf_x0,
            leaf_y0,
            leaf_width,
            leaf_height,
            coded_mi_context,
            mode,
        )
    };
    idtx4x4_coefficients(&residual)
}

fn idtx4x4_coefficients(residual: &[i32; TX4X4_SAMPLES]) -> [i32; TX4X4_SAMPLES] {
    let mut coefficients = [0i32; TX4X4_SAMPLES];
    for (coefficient, residual) in coefficients.iter_mut().zip(residual.iter()) {
        *coefficient = *residual * 8;
    }
    coefficients
}

fn chroma_d45_above_edge(
    palette: &Av2LumaPalette444,
    plane: Av2ChromaPlane,
    txb_x0: usize,
    txb_y0: usize,
    tile_origin_x: usize,
    tile_origin_y: usize,
    leaf_x0: usize,
    leaf_y0: usize,
    leaf_width: usize,
    coded_mi_context: &Av2CodedMiContext,
) -> [Av2Sample; 8] {
    let sb_origin_x = (txb_x0 / MVP_SUPERBLOCK_SIZE) * MVP_SUPERBLOCK_SIZE;
    let sb_right = (sb_origin_x + MVP_SUPERBLOCK_SIZE).min(palette.width());
    let have_top = txb_y0 > tile_origin_y;
    let have_left = txb_x0 > tile_origin_x;
    let mut above = [av2_lossless_v_pred_above_edge(palette.bit_depth()); 8];
    if have_top {
        for index in 0..above.len() {
            let x = txb_x0 + index;
            let external_top_right_coded = txb_y0 == leaf_y0
                && x < sb_right
                && coded_mi_context.is_coded((txb_y0 - 1) / MI_SIZE, x / MI_SIZE);
            if x < leaf_x0 + leaf_width || external_top_right_coded {
                above[index] = chroma_sample(palette, plane, x, txb_y0 - 1);
            } else if index > 0 {
                above[index] = above[index - 1];
            }
        }
    } else if have_left {
        above.fill(chroma_sample(palette, plane, txb_x0 - 1, txb_y0));
    }
    above
}

