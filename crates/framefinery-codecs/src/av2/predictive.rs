fn av2_lossy_subsampled_zero_mv_inter_tiles_bitstream_and_reconstruction_for_frame(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    reference_source: &[u8],
    reference_reconstruction: &[u8],
    qp: u8,
    order_hint: u16,
) -> Option<(Vec<u8>, Vec<u8>)> {
    let expected_len = Picture::expected_len(
        geometry.width,
        geometry.height,
        stream_format.pixel_format(),
    );
    if frame.len() != expected_len
        || reference_source.len() != expected_len
        || reference_reconstruction.len() != expected_len
    {
        return None;
    }
    let layout = planar::Av2PlanarYuvLayout::new(
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .ok()?;
    let tile_layout = Av2TileLayout::lossy_subsampled_for_geometry(geometry);
    if tile_layout.is_single_tile() {
        return None;
    }

    let mut zero_mv_tiles = Vec::with_capacity(tile_layout.tile_count());
    let mut motion_search_regions = Vec::new();
    for region in &tile_layout.regions {
        let zero_mv = layout.regions_equal_between(
            frame,
            region.origin_x,
            region.origin_y,
            reference_source,
            region.origin_x,
            region.origin_y,
            region.width,
            region.height,
        );
        zero_mv_tiles.push(zero_mv);
        if !zero_mv {
            motion_search_regions.push(Av2MotionSearchRegion {
                x0: region.origin_x,
                y0: region.origin_y,
                width: region.width,
                height: region.height,
            });
        }
    }
    if motion_search_regions.is_empty() {
        return None;
    }

    let use_exact_motion_residuals = stream_format.bit_depth.bits() <= 8;
    let motion_map = use_exact_motion_residuals
        .then(|| {
            motion::build_lossless_motion_map_for_regions(
                frame,
                reference_source,
                geometry,
                stream_format.chroma_format,
                stream_format.bit_depth,
                &motion_search_regions,
            )
            .ok()
        })
        .flatten();
    let tile_modes: Vec<_> = tile_layout
        .regions
        .iter()
        .zip(zero_mv_tiles.iter())
        .map(|(region, zero_mv)| {
            if *zero_mv {
                Av2PredictiveTileMode::ZeroMv
            } else if let Some(blocks) = motion_map
                .as_ref()
                .and_then(|map| lossy_tile_inter_residual_block_modes(map, *region))
            {
                Av2PredictiveTileMode::Residual(blocks)
            } else {
                Av2PredictiveTileMode::Intra
            }
        })
        .collect();
    let has_zero_mv_tile = tile_modes
        .iter()
        .any(|mode| matches!(mode, Av2PredictiveTileMode::ZeroMv));
    let has_residual_tile = tile_modes.iter().any(|mode| {
        matches!(
            mode,
            Av2PredictiveTileMode::Intra | Av2PredictiveTileMode::Residual(_)
        )
    });
    let has_newmv_residual_tile = tile_modes
        .iter()
        .any(|mode| matches!(mode, Av2PredictiveTileMode::Residual(_)));
    if !has_residual_tile || (!has_zero_mv_tile && !has_newmv_residual_tile) {
        return None;
    }

    let profile = Av2Black444MvpProfile::current();
    let inter_qp = av2_predictive_inter_qp_for_qp(qp, stream_format.bit_depth);
    let quantization = Av2QuantizationParams::regular_qp(inter_qp, stream_format.bit_depth);
    let palette = palette::build_luma_palette_lossless(
        frame,
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .ok();
    let palette_ref = palette.as_ref();
    let mut reconstruction = vec![0; expected_len];
    let mut tile_payloads = Vec::with_capacity(tile_layout.tile_count());
    let mut zero_mv_payload_cache: Vec<(usize, usize, entropy::Av2EntropyPayload)> = Vec::new();
    for (&region, tile_mode) in tile_layout.regions.iter().zip(tile_modes.iter()) {
        match tile_mode {
            Av2PredictiveTileMode::ZeroMv => {
                if !layout.copy_region_between(
                    &mut reconstruction,
                    region.origin_x,
                    region.origin_y,
                    reference_reconstruction,
                    region.origin_x,
                    region.origin_y,
                    region.width,
                    region.height,
                ) {
                    return None;
                }
                let payload = if let Some((_, _, payload)) = zero_mv_payload_cache
                    .iter()
                    .find(|(width, height, _)| *width == region.width && *height == region.height)
                {
                    payload.clone()
                } else {
                    let payload = av2_lossless_predictive_tile_payload_for_mode(
                        region,
                        tile_mode,
                        profile,
                        geometry,
                        stream_format,
                        frame,
                        reference_source,
                        palette_ref,
                    );
                    zero_mv_payload_cache.push((region.width, region.height, payload.clone()));
                    payload
                };
                tile_payloads.push(payload);
            }
            Av2PredictiveTileMode::Residual(residual_blocks) => {
                tile_payloads.push(
                    av2_lossy_fixed_inter_intra_tile_entropy_payload_for_region_with_fields(
                        region,
                        profile,
                        geometry,
                        stream_format.chroma_format,
                        stream_format.bit_depth,
                        frame,
                        reference_reconstruction,
                        &mut reconstruction,
                        residual_blocks,
                        inter_qp,
                        quantization.base_qindex,
                        false,
                    ),
                );
            }
            Av2PredictiveTileMode::Intra => {
                let residual_blocks = lossless_tile_zero_mv_residual_block_modes(region)?;
                tile_payloads.push(
                    av2_lossy_fixed_inter_intra_tile_entropy_payload_for_region_with_fields(
                        region,
                        profile,
                        geometry,
                        stream_format.chroma_format,
                        stream_format.bit_depth,
                        frame,
                        reference_reconstruction,
                        &mut reconstruction,
                        &residual_blocks,
                        inter_qp,
                        quantization.base_qindex,
                        false,
                    ),
                );
            }
            Av2PredictiveTileMode::NewMv(_)
            | Av2PredictiveTileMode::Mixed(_)
            | Av2PredictiveTileMode::MixedInterIntraOrResidual { .. } => return None,
        }
    }

    let mut payload =
        av2_mvp_regular_inter_header_payload(&tile_layout, stream_format, quantization, order_hint);
    let tile_payload = tile_group_payload_from_entropy(&tile_payloads);
    let bit_offset = payload.bytes.len() * 8;
    payload.fields.push(syntax::Av2SyntaxField {
        name: "tile_group.tile_entropy_payload",
        code: syntax::Av2SyntaxCode::TileEntropyPayload,
        bit_offset,
        bit_count: tile_payload.len() * 8,
    });
    payload.bytes.extend_from_slice(&tile_payload);

    let mut out = Vec::new();
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    append_obu(&mut out, Av2ObuType::RegularTileGroup, &payload);
    Some((out, reconstruction))
}

fn av2_lossless_subsampled_regular_inter_tiles_bitstream_and_reconstruction_for_frame(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    reference: &[u8],
    order_hint: u16,
) -> Option<(Vec<u8>, Vec<u8>)> {
    let expected_len = Picture::expected_len(
        geometry.width,
        geometry.height,
        stream_format.pixel_format(),
    );
    if frame.len() != expected_len || reference.len() != expected_len {
        return None;
    }
    let layout = planar::Av2PlanarYuvLayout::new(
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .ok()?;
    let tile_layout = Av2TileLayout::lossless_subsampled_regular_inter_for_geometry(geometry);
    if tile_layout.is_single_tile() {
        return None;
    }
    if tile_layout.has_four_or_more_tile_columns() {
        // Four-column 1920-wide lossless regular-inter layouts currently
        // desynchronize AVM's tile entropy reader on mixed inter frames.
        // Fallback to a predictive closed-loop key frame until that entropy
        // context issue is fixed directly.
        return None;
    }
    if tile_layout.has_lossless_regular_inter_wide_tile() {
        // Uneven two-column layouts with a tile wider than 512 pixels hit the
        // same lossless inter entropy mismatch in AVM. Keep those streams
        // predictive by falling back to closed-loop keys for now.
        return None;
    }

    let mut zero_mv_tiles = Vec::with_capacity(tile_layout.tile_count());
    let mut motion_search_regions = Vec::new();
    for region in &tile_layout.regions {
        let zero_mv = layout.regions_equal_between(
            frame,
            region.origin_x,
            region.origin_y,
            reference,
            region.origin_x,
            region.origin_y,
            region.width,
            region.height,
        );
        zero_mv_tiles.push(zero_mv);
        if !zero_mv {
            motion_search_regions.push(Av2MotionSearchRegion {
                x0: region.origin_x,
                y0: region.origin_y,
                width: region.width,
                height: region.height,
            });
        }
    }
    if motion_search_regions.is_empty() {
        return None;
    }

    let motion_map = motion::build_lossless_motion_map_for_regions(
        frame,
        reference,
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
        &motion_search_regions,
    )
    .ok();
    let tile_modes: Vec<_> = tile_layout
        .regions
        .iter()
        .zip(zero_mv_tiles.iter())
        .map(|(region, zero_mv)| {
            if *zero_mv {
                Av2PredictiveTileMode::ZeroMv
            } else if let Some(mv) = motion_map
                .as_ref()
                .and_then(|map| uniform_lossless_tile_motion(map, *region))
            {
                Av2PredictiveTileMode::NewMv(mv)
            } else if let Some(blocks) = motion_map
                .as_ref()
                .and_then(|map| lossless_tile_inter_block_modes(map, *region))
            {
                Av2PredictiveTileMode::Mixed(blocks)
            } else if let Some(intra_blocks) = motion_map
                .as_ref()
                .and_then(|map| lossless_tile_inter_intra_block_modes(map, *region))
            {
                let residual_blocks = motion_map
                    .as_ref()
                    .and_then(|map| lossless_tile_inter_residual_block_modes(map, *region))
                    .expect("mixed inter/intra blocks should also form residual candidates");
                Av2PredictiveTileMode::MixedInterIntraOrResidual {
                    intra_blocks,
                    residual_blocks,
                }
            } else {
                Av2PredictiveTileMode::Intra
            }
        })
        .collect();
    let inter_tile_count = tile_modes
        .iter()
        .filter(|mode| !matches!(mode, Av2PredictiveTileMode::Intra))
        .count();
    let all_zero_mv = tile_modes
        .iter()
        .all(|mode| matches!(mode, Av2PredictiveTileMode::ZeroMv));
    if inter_tile_count == 0 || all_zero_mv {
        return None;
    }

    let profile = Av2Black444MvpProfile::current();
    let palette = palette::build_luma_palette_lossless(
        frame,
        geometry,
        stream_format.chroma_format,
        stream_format.bit_depth,
    )
    .ok();
    let palette_ref = palette.as_ref();
    let tile_payloads: Vec<_> = if tile_layout.tile_count() > 1 {
        std::thread::scope(|scope| {
            let handles: Vec<_> = tile_layout
                .regions
                .iter()
                .zip(tile_modes.iter())
                .map(|(&region, tile_mode)| {
                    scope.spawn(move || {
                        av2_lossless_predictive_tile_payload_for_mode(
                            region,
                            tile_mode,
                            profile,
                            geometry,
                            stream_format,
                            frame,
                            reference,
                            palette_ref,
                        )
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("AV2 predictive tile worker panicked"))
                .collect()
        })
    } else {
        tile_layout
            .regions
            .iter()
            .zip(tile_modes.iter())
            .map(|(&region, tile_mode)| {
                av2_lossless_predictive_tile_payload_for_mode(
                    region,
                    tile_mode,
                    profile,
                    geometry,
                    stream_format,
                    frame,
                    reference,
                    palette_ref,
                )
            })
            .collect()
    };

    let mut payload = av2_mvp_regular_inter_header_payload(
        &tile_layout,
        stream_format,
        Av2QuantizationParams::lossless(),
        order_hint,
    );
    let tile_payload = tile_group_payload_from_entropy(&tile_payloads);
    let bit_offset = payload.bytes.len() * 8;
    payload.fields.push(syntax::Av2SyntaxField {
        name: "tile_group.tile_entropy_payload",
        code: syntax::Av2SyntaxCode::TileEntropyPayload,
        bit_offset,
        bit_count: tile_payload.len() * 8,
    });
    payload.bytes.extend_from_slice(&tile_payload);

    let mut out = Vec::new();
    append_obu(
        &mut out,
        Av2ObuType::TemporalDelimiter,
        &Av2SyntaxPayload::default(),
    );
    append_obu(&mut out, Av2ObuType::RegularTileGroup, &payload);
    Some((out, frame.to_vec()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Av2PredictiveTileMode {
    ZeroMv,
    NewMv(Av2MotionVector),
    Mixed(Av2LosslessInterTileBlockModes),
    Residual(Av2LosslessInterTileBlockModes),
    MixedInterIntraOrResidual {
        intra_blocks: Av2LosslessInterTileBlockModes,
        residual_blocks: Av2LosslessInterTileBlockModes,
    },
    Intra,
}

fn av2_lossless_predictive_tile_payload_for_mode(
    region: Av2TileRegion,
    tile_mode: &Av2PredictiveTileMode,
    profile: Av2Black444MvpProfile,
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    reference: &[u8],
    palette: Option<&Av2LumaPalette444>,
) -> entropy::Av2EntropyPayload {
    match tile_mode {
        Av2PredictiveTileMode::ZeroMv => {
            av2_lossless_zero_mv_inter_tile_entropy_payload_for_region_with_fields(
                region,
                profile,
                stream_format.chroma_format,
                false,
            )
        }
        Av2PredictiveTileMode::NewMv(mv) => {
            av2_lossless_new_mv_inter_tile_entropy_payload_for_region_with_fields(
                region,
                profile,
                stream_format.chroma_format,
                mv.row_px,
                mv.col_px,
                false,
            )
        }
        Av2PredictiveTileMode::Mixed(blocks) => {
            av2_lossless_mixed_inter_tile_entropy_payload_for_region_with_fields(
                region,
                profile,
                stream_format.chroma_format,
                blocks,
                false,
            )
        }
        Av2PredictiveTileMode::Residual(_) => {
            unreachable!("lossless predictive path does not emit lossy residual tile modes")
        }
        Av2PredictiveTileMode::MixedInterIntraOrResidual {
            intra_blocks,
            residual_blocks,
        } => {
            let mut scratch_reconstruction = Vec::new();
            let residual_payload =
                av2_lossless_mixed_inter_intra_tile_entropy_payload_for_region_with_fields(
                    region,
                    profile,
                    geometry,
                    stream_format.chroma_format,
                    stream_format.bit_depth,
                    frame,
                    reference,
                    &mut scratch_reconstruction,
                    palette,
                    residual_blocks,
                    false,
                );
            if av2_lossless_residual_payload_is_decisive(
                &residual_payload,
                region,
                stream_format.chroma_format,
                stream_format.bit_depth,
            ) {
                return residual_payload;
            }

            scratch_reconstruction.clear();
            let intra_payload =
                av2_lossless_mixed_inter_intra_tile_entropy_payload_for_region_with_fields(
                    region,
                    profile,
                    geometry,
                    stream_format.chroma_format,
                    stream_format.bit_depth,
                    frame,
                    reference,
                    &mut scratch_reconstruction,
                    palette,
                    intra_blocks,
                    false,
                );
            if av2_entropy_payload_rate_key(&residual_payload)
                < av2_entropy_payload_rate_key(&intra_payload)
            {
                residual_payload
            } else {
                intra_payload
            }
        }
        Av2PredictiveTileMode::Intra => {
            let mut scratch_reconstruction = Vec::new();
            av2_lossless_subsampled_regular_inter_intra_tile_entropy_payload_for_region_with_fields(
                region,
                profile,
                geometry,
                stream_format.chroma_format,
                stream_format.bit_depth,
                frame,
                &mut scratch_reconstruction,
                palette,
                false,
            )
        }
    }
}

fn av2_entropy_payload_rate_key(payload: &entropy::Av2EntropyPayload) -> (usize, usize) {
    (payload.bytes.len(), payload.symbol_bits)
}

const AV2_LOSSLESS_RESIDUAL_SHORTCUT_SOURCE_DENOMINATOR: usize = 32;

fn av2_lossless_residual_payload_is_decisive(
    payload: &entropy::Av2EntropyPayload,
    region: Av2TileRegion,
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
) -> bool {
    let chroma_samples = match chroma_format {
        Av2ChromaFormat::Yuv420 => region.width * region.height / 2,
        Av2ChromaFormat::Yuv422 => region.width * region.height,
        Av2ChromaFormat::Yuv444 => region.width * region.height * 2,
    };
    let source_bytes =
        (region.width * region.height + chroma_samples) * bit_depth.bytes_per_sample();
    payload.bytes.len() * AV2_LOSSLESS_RESIDUAL_SHORTCUT_SOURCE_DENOMINATOR <= source_bytes
}

fn uniform_lossless_tile_motion(
    motion_map: &Av2LosslessMotionMap,
    region: Av2TileRegion,
) -> Option<Av2MotionVector> {
    if region.origin_x % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.origin_y % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.width % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.height % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
    {
        return None;
    }

    let mut selected = None;
    for y in (region.origin_y..region.origin_y + region.height).step_by(AV2_LOSSLESS_ME_BLOCK_SIZE)
    {
        for x in
            (region.origin_x..region.origin_x + region.width).step_by(AV2_LOSSLESS_ME_BLOCK_SIZE)
        {
            let block = motion_map.candidate_at(x, y)?;
            if block.mv.row_px == 0 && block.mv.col_px == 0 {
                return None;
            }
            match selected {
                Some(mv) if mv != block.mv => return None,
                Some(_) => {}
                None => selected = Some(block.mv),
            }
        }
    }
    selected
}

fn lossless_tile_inter_block_modes(
    motion_map: &Av2LosslessMotionMap,
    region: Av2TileRegion,
) -> Option<Av2LosslessInterTileBlockModes> {
    if region.origin_x % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.origin_y % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.width % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.height % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
    {
        return None;
    }

    let blocks_wide = region.width / AV2_LOSSLESS_ME_BLOCK_SIZE;
    let blocks_high = region.height / AV2_LOSSLESS_ME_BLOCK_SIZE;
    let mut blocks = Vec::with_capacity(blocks_wide * blocks_high);
    let mut has_nonzero = false;
    for y in (region.origin_y..region.origin_y + region.height).step_by(AV2_LOSSLESS_ME_BLOCK_SIZE)
    {
        for x in
            (region.origin_x..region.origin_x + region.width).step_by(AV2_LOSSLESS_ME_BLOCK_SIZE)
        {
            let block = motion_map.candidate_at(x, y)?;
            if block.mv.row_px == 0 && block.mv.col_px == 0 {
                blocks.push(Av2LosslessInterBlockMode::ZeroMv);
            } else {
                has_nonzero = true;
                blocks.push(Av2LosslessInterBlockMode::NewMv {
                    row_px: block.mv.row_px,
                    col_px: block.mv.col_px,
                });
            }
        }
    }
    has_nonzero.then(|| Av2LosslessInterTileBlockModes::new(blocks_wide, blocks_high, blocks))
}

fn lossless_tile_inter_intra_block_modes(
    motion_map: &Av2LosslessMotionMap,
    region: Av2TileRegion,
) -> Option<Av2LosslessInterTileBlockModes> {
    lossless_tile_mixed_inter_block_modes(motion_map, region, Av2LosslessInterBlockMode::Intra)
}

fn lossless_tile_inter_residual_block_modes(
    motion_map: &Av2LosslessMotionMap,
    region: Av2TileRegion,
) -> Option<Av2LosslessInterTileBlockModes> {
    lossless_tile_mixed_inter_block_modes(
        motion_map,
        region,
        Av2LosslessInterBlockMode::ZeroMvResidual,
    )
}

fn lossy_tile_inter_residual_block_modes(
    motion_map: &Av2LosslessMotionMap,
    region: Av2TileRegion,
) -> Option<Av2LosslessInterTileBlockModes> {
    if region.origin_x % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.origin_y % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.width % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.height % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
    {
        return None;
    }

    let blocks_wide = region.width / AV2_LOSSLESS_ME_BLOCK_SIZE;
    let blocks_high = region.height / AV2_LOSSLESS_ME_BLOCK_SIZE;
    let mut blocks = Vec::with_capacity(blocks_wide * blocks_high);
    let mut has_newmv_residual = false;
    for y in (region.origin_y..region.origin_y + region.height).step_by(AV2_LOSSLESS_ME_BLOCK_SIZE)
    {
        for x in
            (region.origin_x..region.origin_x + region.width).step_by(AV2_LOSSLESS_ME_BLOCK_SIZE)
        {
            if let Some(block) = motion_map.candidate_at(x, y) {
                if block.mv.row_px != 0 || block.mv.col_px != 0 {
                    has_newmv_residual = true;
                    blocks.push(Av2LosslessInterBlockMode::NewMvResidual {
                        row_px: block.mv.row_px,
                        col_px: block.mv.col_px,
                    });
                    continue;
                }
            }
            blocks.push(Av2LosslessInterBlockMode::ZeroMvResidual);
        }
    }

    has_newmv_residual
        .then(|| Av2LosslessInterTileBlockModes::new(blocks_wide, blocks_high, blocks))
}

fn lossless_tile_zero_mv_residual_block_modes(
    region: Av2TileRegion,
) -> Option<Av2LosslessInterTileBlockModes> {
    if region.origin_x % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.origin_y % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.width % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.height % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
    {
        return None;
    }

    let blocks_wide = region.width / AV2_LOSSLESS_ME_BLOCK_SIZE;
    let blocks_high = region.height / AV2_LOSSLESS_ME_BLOCK_SIZE;
    Some(Av2LosslessInterTileBlockModes::new(
        blocks_wide,
        blocks_high,
        vec![Av2LosslessInterBlockMode::ZeroMvResidual; blocks_wide * blocks_high],
    ))
}

fn lossless_tile_mixed_inter_block_modes(
    motion_map: &Av2LosslessMotionMap,
    region: Av2TileRegion,
    missing_block_mode: Av2LosslessInterBlockMode,
) -> Option<Av2LosslessInterTileBlockModes> {
    if region.origin_x % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.origin_y % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.width % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
        || region.height % AV2_LOSSLESS_ME_BLOCK_SIZE != 0
    {
        return None;
    }

    let blocks_wide = region.width / AV2_LOSSLESS_ME_BLOCK_SIZE;
    let blocks_high = region.height / AV2_LOSSLESS_ME_BLOCK_SIZE;
    let mut blocks = Vec::with_capacity(blocks_wide * blocks_high);
    let mut exact_inter_blocks = 0usize;
    for y in (region.origin_y..region.origin_y + region.height).step_by(AV2_LOSSLESS_ME_BLOCK_SIZE)
    {
        for x in
            (region.origin_x..region.origin_x + region.width).step_by(AV2_LOSSLESS_ME_BLOCK_SIZE)
        {
            if let Some(block) = motion_map.candidate_at(x, y) {
                if block.mv.row_px == 0 && block.mv.col_px == 0 {
                    blocks.push(Av2LosslessInterBlockMode::ZeroMv);
                } else {
                    blocks.push(Av2LosslessInterBlockMode::NewMv {
                        row_px: block.mv.row_px,
                        col_px: block.mv.col_px,
                    });
                }
                exact_inter_blocks += 1;
            } else {
                blocks.push(missing_block_mode);
            }
        }
    }
    if exact_inter_blocks == 0 || exact_inter_blocks == blocks.len() {
        return None;
    }
    Some(Av2LosslessInterTileBlockModes::new(
        blocks_wide,
        blocks_high,
        blocks,
    ))
}

fn av2_order_hint_for_frame(frame_index: usize) -> u16 {
    let mask = (1u16 << AV2_PREDICTIVE_ORDER_HINT_BITS) - 1;
    (frame_index as u16) & mask
}
