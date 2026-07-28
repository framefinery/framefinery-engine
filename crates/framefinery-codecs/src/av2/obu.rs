fn tile_group_payload_from_entropy(tile_payloads: &[entropy::Av2EntropyPayload]) -> Vec<u8> {
    if tile_payloads.len() == 1 {
        return tile_payloads[0].bytes.clone();
    }

    let mut out = Vec::new();
    for (tile_index, payload) in tile_payloads.iter().enumerate() {
        if tile_index + 1 != tile_payloads.len() {
            write_tile_size_prefix(payload.bytes.len(), &mut out);
        }
        out.extend_from_slice(&payload.bytes);
    }
    out
}

fn av2_tile_entropy_payloads_for_mode(
    tile_layout: &Av2TileLayout,
    frame_mode: &Av2Mvp444FrameMode,
    record_fields: bool,
) -> Vec<entropy::Av2EntropyPayload> {
    tile_layout
        .regions
        .iter()
        .map(|&region| av2_tile_entropy_payload_for_region(region, frame_mode, record_fields))
        .collect()
}

fn av2_tile_entropy_payload_for_region(
    region: Av2TileRegion,
    frame_mode: &Av2Mvp444FrameMode,
    record_fields: bool,
) -> entropy::Av2EntropyPayload {
    match frame_mode {
        Av2Mvp444FrameMode::Black => {
            av2_black_444_tile_entropy_payload_for_region_with_intrabc_and_fields(
                region,
                frame_mode.profile(),
                frame_mode.allow_intrabc(),
                record_fields,
            )
        }
        Av2Mvp444FrameMode::LumaPalette { palette, ibc } => {
            if !frame_mode.allow_intrabc() && av2_luma_palette_region_is_black(palette, region) {
                av2_black_444_tile_entropy_payload_for_region_with_fields(
                    region,
                    frame_mode.profile(),
                    record_fields,
                )
            } else {
                av2_luma_palette_444_tile_entropy_payload_for_region_with_fields(
                    region,
                    frame_mode.profile(),
                    frame_mode.allow_intrabc(),
                    palette,
                    ibc.as_ref(),
                    record_fields,
                )
            }
        }
    }
}

fn av2_luma_palette_region_is_black(palette: &Av2LumaPalette444, region: Av2TileRegion) -> bool {
    for y in region.origin_y..(region.origin_y + region.height) {
        for x in region.origin_x..(region.origin_x + region.width) {
            if palette.y_sample(x, y) != 0
                || palette.u_sample(x, y) != 0
                || palette.v_sample(x, y) != 0
            {
                return false;
            }
        }
    }
    true
}

fn write_tile_size_prefix(tile_size: usize, out: &mut Vec<u8>) {
    let stored = tile_size
        .checked_sub(AV2_MIN_TILE_SIZE_BYTES)
        .expect("AV2 tile payload must not be empty");
    assert!(
        stored <= u32::MAX as usize,
        "AV2 MVP tile payload size prefix is limited to 32 bits"
    );
    out.extend_from_slice(&(stored as u32).to_le_bytes());
}

fn append_obu(out: &mut Vec<u8>, obu_type: Av2ObuType, payload: &Av2SyntaxPayload) {
    let header = av2_obu_header(obu_type);
    let obu_payload_len = (header.len() + payload.bytes.len()) as u32;
    if obu_type == Av2ObuType::ClosedLoopKey {
        // AV2 v1.0.0 Section 5.3 defines OBU lengths as unsigned LEB128.
        // The RTL reserves three bytes for closed-loop frame OBUs so it can
        // stream tile payloads once and patch the final length afterward. Very
        // large software-only high-depth frames can exceed that envelope, so
        // fall back to the normal variable-width LEB128 form when needed.
        if leb128_len(obu_payload_len) <= 3 {
            write_leb128_fixed_width(obu_payload_len, 3, out);
        } else {
            write_leb128(obu_payload_len, out);
        }
    } else {
        write_leb128(obu_payload_len, out);
    }
    out.extend_from_slice(&header);
    out.extend_from_slice(&payload.bytes);
}

fn av2_obu_header(obu_type: Av2ObuType) -> Vec<u8> {
    let mut writer = Av2SyntaxWriter::new();
    writer.write_flag("obu_header.obu_header_extension_flag", false);
    writer.write_literal("obu_header.obu_type", obu_type as u64, 5);
    writer.write_literal("obu_header.obu_tlayer_id", 0, 2);
    writer.finish().bytes
}

fn write_leb128(mut value: u32, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn leb128_len(mut value: u32) -> usize {
    let mut len = 1;
    while value >= 0x80 {
        value >>= 7;
        len += 1;
    }
    len
}

fn write_leb128_fixed_width(mut value: u32, width: usize, out: &mut Vec<u8>) {
    assert!(
        (1..=5).contains(&width),
        "AV2 fixed LEB width must be 1..=5"
    );
    for index in 0..width {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if index + 1 != width {
            byte |= 0x80;
        }
        out.push(byte);
    }
    assert_eq!(value, 0, "AV2 fixed-width LEB is too narrow");
}
