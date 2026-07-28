#[cfg(test)]
fn av2_black_444_closed_loop_key_payload(geometry: Av2VideoGeometry) -> Av2SyntaxPayload {
    av2_mvp_444_closed_loop_key_payload(
        geometry,
        SampleBitDepth::new(8).expect("8-bit depth is supported"),
        &Av2Mvp444FrameMode::Black,
    )
}

#[cfg(test)]
fn av2_black_closed_loop_key_payload(
    geometry: Av2VideoGeometry,
    chroma_format: Av2ChromaFormat,
) -> Av2SyntaxPayload {
    let tile_layout = Av2TileLayout::for_geometry(geometry);
    let allow_screen_content_tools = chroma_format == Av2ChromaFormat::Yuv444;
    let allow_intrabc = false;
    let profile = Av2Black444MvpProfile::current();
    let mut payload = av2_mvp_444_closed_loop_key_header_payload(
        allow_screen_content_tools,
        allow_intrabc,
        &tile_layout,
        Av2StreamFormat {
            chroma_format,
            bit_depth: SampleBitDepth::new(8).expect("8-bit depth is supported"),
        },
        Av2QuantizationParams::lossless(),
    );
    let tile_payloads: Vec<_> = tile_layout
        .regions
        .iter()
        .map(|&region| {
            if allow_intrabc {
                av2_black_444_tile_entropy_payload_for_region_with_intrabc_and_fields(
                    region, profile, true, false,
                )
            } else {
                tile::av2_black_tile_entropy_payload_for_region_with_fields(
                    region,
                    profile,
                    chroma_format,
                    false,
                )
            }
        })
        .collect();
    let tile_payload = tile_group_payload_from_entropy(&tile_payloads);
    let bit_offset = payload.bytes.len() * 8;
    payload.fields.push(syntax::Av2SyntaxField {
        name: "tile_group.tile_entropy_payload",
        code: syntax::Av2SyntaxCode::TileEntropyPayload,
        bit_offset,
        bit_count: tile_payload.len() * 8,
    });
    payload.bytes.extend_from_slice(&tile_payload);
    payload
}

fn av2_lossy_subsampled_closed_loop_key_payload(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    reconstruction: &mut [u8],
    qp: u8,
) -> Av2SyntaxPayload {
    av2_lossy_subsampled_closed_loop_key_payload_with_mode(
        geometry,
        stream_format,
        frame,
        reconstruction,
        qp,
        true,
        0,
    )
}

fn av2_lossy_subsampled_predictive_closed_loop_key_payload(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    reconstruction: &mut [u8],
    qp: u8,
    order_hint: u16,
) -> Av2SyntaxPayload {
    av2_lossy_subsampled_closed_loop_key_payload_with_mode(
        geometry,
        stream_format,
        frame,
        reconstruction,
        qp,
        false,
        order_hint,
    )
}

fn av2_lossy_subsampled_closed_loop_key_payload_with_mode(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    reconstruction: &mut [u8],
    qp: u8,
    single_picture_header: bool,
    order_hint: u16,
) -> Av2SyntaxPayload {
    let tile_layout = Av2TileLayout::lossy_subsampled_for_geometry(geometry);
    let profile = Av2Black444MvpProfile::current();
    let quantization = Av2QuantizationParams::regular_qp(qp, stream_format.bit_depth);
    let mut payload = if single_picture_header {
        av2_mvp_444_closed_loop_key_header_payload(
            false,
            false,
            &tile_layout,
            stream_format,
            quantization,
        )
    } else {
        av2_mvp_444_predictive_closed_loop_key_header_payload(
            false,
            false,
            &tile_layout,
            stream_format,
            quantization,
            order_hint,
        )
    };
    let tile_payloads: Vec<_> = tile_layout
        .regions
        .iter()
        .map(|&region| {
            av2_lossy_subsampled_tile_entropy_payload_for_region_with_fields(
                region,
                profile,
                geometry,
                stream_format.chroma_format,
                stream_format.bit_depth,
                frame,
                reconstruction,
                qp,
                quantization.base_qindex,
                false,
            )
        })
        .collect();
    let tile_payload = tile_group_payload_from_entropy(&tile_payloads);
    let bit_offset = payload.bytes.len() * 8;
    payload.fields.push(syntax::Av2SyntaxField {
        name: "tile_group.tile_entropy_payload",
        code: syntax::Av2SyntaxCode::TileEntropyPayload,
        bit_offset,
        bit_count: tile_payload.len() * 8,
    });
    payload.bytes.extend_from_slice(&tile_payload);
    payload
}

fn av2_lossless_subsampled_closed_loop_key_payload(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    reconstruction: &mut [u8],
    profile: Av2Black444MvpProfile,
    palette: Option<&Av2LumaPalette444>,
    ibc: Option<&Av2LocalIbc444>,
) -> Av2SyntaxPayload {
    let tile_layout = if ibc.is_none() {
        Av2TileLayout::lossless_subsampled_fast_for_geometry(geometry)
    } else {
        Av2TileLayout::lossless_subsampled_ibc_for_geometry(geometry)
    };
    let allow_intrabc = ibc.is_some();
    let allow_screen_content_tools = allow_intrabc || palette.is_some();
    let mut payload = av2_mvp_444_closed_loop_key_header_payload(
        allow_screen_content_tools,
        allow_intrabc,
        &tile_layout,
        stream_format,
        Av2QuantizationParams::lossless(),
    );
    let tile_payloads: Vec<_> = if ibc.is_none() && tile_layout.tile_count() > 1 {
        std::thread::scope(|scope| {
            let handles: Vec<_> = tile_layout
                .regions
                .iter()
                .map(|&region| {
                    scope.spawn(move || {
                        av2_lossless_subsampled_fast_tile_entropy_payload_for_region_with_fields(
                            region,
                            profile,
                            geometry,
                            stream_format.chroma_format,
                            stream_format.bit_depth,
                            frame,
                            palette,
                            false,
                        )
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("AV2 tile entropy worker panicked"))
                .collect()
        })
    } else {
        tile_layout
            .regions
            .iter()
            .map(|&region| {
                av2_lossless_subsampled_tile_entropy_payload_for_region_with_fields(
                    region,
                    profile,
                    geometry,
                    stream_format.chroma_format,
                    stream_format.bit_depth,
                    frame,
                    reconstruction,
                    palette,
                    ibc,
                    false,
                )
            })
            .collect()
    };
    if ibc.is_none() && tile_layout.tile_count() > 1 {
        reconstruction.copy_from_slice(frame);
    }
    let tile_payload = tile_group_payload_from_entropy(&tile_payloads);
    let bit_offset = payload.bytes.len() * 8;
    payload.fields.push(syntax::Av2SyntaxField {
        name: "tile_group.tile_entropy_payload",
        code: syntax::Av2SyntaxCode::TileEntropyPayload,
        bit_offset,
        bit_count: tile_payload.len() * 8,
    });
    payload.bytes.extend_from_slice(&tile_payload);
    payload
}

fn av2_lossless_subsampled_predictive_closed_loop_key_payload(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    reconstruction: &mut [u8],
    profile: Av2Black444MvpProfile,
    palette: Option<&Av2LumaPalette444>,
    ibc: Option<&Av2LocalIbc444>,
    order_hint: u16,
) -> Av2SyntaxPayload {
    let tile_layout = if ibc.is_none() {
        Av2TileLayout::lossless_subsampled_fast_for_geometry(geometry)
    } else {
        Av2TileLayout::lossless_subsampled_ibc_for_geometry(geometry)
    };
    let allow_intrabc = ibc.is_some();
    let allow_screen_content_tools = allow_intrabc || palette.is_some();
    let mut payload = av2_mvp_444_predictive_closed_loop_key_header_payload(
        allow_screen_content_tools,
        allow_intrabc,
        &tile_layout,
        stream_format,
        Av2QuantizationParams::lossless(),
        order_hint,
    );
    let tile_payloads: Vec<_> = if ibc.is_none() && tile_layout.tile_count() > 1 {
        std::thread::scope(|scope| {
            let handles: Vec<_> = tile_layout
                .regions
                .iter()
                .map(|&region| {
                    scope.spawn(move || {
                        av2_lossless_subsampled_fast_tile_entropy_payload_for_region_with_fields(
                            region,
                            profile,
                            geometry,
                            stream_format.chroma_format,
                            stream_format.bit_depth,
                            frame,
                            palette,
                            false,
                        )
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("AV2 tile entropy worker panicked"))
                .collect()
        })
    } else {
        tile_layout
            .regions
            .iter()
            .map(|&region| {
                av2_lossless_subsampled_tile_entropy_payload_for_region_with_fields(
                    region,
                    profile,
                    geometry,
                    stream_format.chroma_format,
                    stream_format.bit_depth,
                    frame,
                    reconstruction,
                    palette,
                    ibc,
                    false,
                )
            })
            .collect()
    };
    if ibc.is_none() && tile_layout.tile_count() > 1 {
        reconstruction.copy_from_slice(frame);
    }
    let tile_payload = tile_group_payload_from_entropy(&tile_payloads);
    let bit_offset = payload.bytes.len() * 8;
    payload.fields.push(syntax::Av2SyntaxField {
        name: "tile_group.tile_entropy_payload",
        code: syntax::Av2SyntaxCode::TileEntropyPayload,
        bit_offset,
        bit_count: tile_payload.len() * 8,
    });
    payload.bytes.extend_from_slice(&tile_payload);
    payload
}

fn av2_regular_sef_payload(order_hint: u16) -> Av2SyntaxPayload {
    let mut writer = Av2SyntaxWriter::new();
    writer.write_uvlc("uncompressed_header.cur_mfh_id", 0);
    writer.write_uvlc("uncompressed_header.seq_header_id", 0);
    writer.write_literal("show_existing_frame.existing_frame_idx", 0, 1);
    writer.write_flag("show_existing_frame.derive_sef_order_hint", false);
    writer.write_literal(
        "show_existing_frame.order_hint",
        u64::from(order_hint),
        AV2_PREDICTIVE_ORDER_HINT_BITS,
    );
    writer.trailing_bits();
    writer.finish()
}

#[cfg(test)]
fn av2_lossless_zero_mv_regular_inter_payload(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    order_hint: u16,
) -> Av2SyntaxPayload {
    let tile_layout = Av2TileLayout::lossless_subsampled_fast_for_geometry(geometry);
    let profile = Av2Black444MvpProfile::current();
    let mut payload = av2_mvp_regular_inter_header_payload(
        &tile_layout,
        stream_format,
        Av2QuantizationParams::lossless(),
        order_hint,
    );
    let tile_payloads: Vec<_> = tile_layout
        .regions
        .iter()
        .map(|&region| {
            av2_lossless_zero_mv_inter_tile_entropy_payload_for_region_with_fields(
                region,
                profile,
                stream_format.chroma_format,
                false,
            )
        })
        .collect();
    let tile_payload = tile_group_payload_from_entropy(&tile_payloads);
    let bit_offset = payload.bytes.len() * 8;
    payload.fields.push(syntax::Av2SyntaxField {
        name: "tile_group.tile_entropy_payload",
        code: syntax::Av2SyntaxCode::TileEntropyPayload,
        bit_offset,
        bit_count: tile_payload.len() * 8,
    });
    payload.bytes.extend_from_slice(&tile_payload);
    payload
}



fn av2_mvp_444_closed_loop_key_payload(
    geometry: Av2VideoGeometry,
    bit_depth: SampleBitDepth,
    frame_mode: &Av2Mvp444FrameMode,
) -> Av2SyntaxPayload {
    let tile_layout = av2_tile_layout_for_frame_mode(geometry, frame_mode);
    let mut payload = av2_mvp_444_closed_loop_key_header_payload(
        frame_mode.allow_screen_content_tools(),
        frame_mode.allow_intrabc(),
        &tile_layout,
        Av2StreamFormat {
            chroma_format: Av2ChromaFormat::Yuv444,
            bit_depth,
        },
        Av2QuantizationParams::lossless(),
    );
    let tile_payload = tile_group_payload_from_entropy(&av2_tile_entropy_payloads_for_mode(
        &tile_layout,
        frame_mode,
        false,
    ));
    let bit_offset = payload.bytes.len() * 8;
    payload.fields.push(syntax::Av2SyntaxField {
        name: "tile_group.tile_entropy_payload",
        code: syntax::Av2SyntaxCode::TileEntropyPayload,
        bit_offset,
        bit_count: tile_payload.len() * 8,
    });
    payload.bytes.extend_from_slice(&tile_payload);
    payload
}
