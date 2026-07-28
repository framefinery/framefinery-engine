pub fn vvc_palette_444_cabac_dump_json(
    input: &[u8],
    geometry: VvcVideoGeometry,
    format: PixelFormat,
) -> Result<String, String> {
    let params = VvcEncodeParams { frames: 1 };
    let frame = sample_vvc_yuv_frame(input, params, geometry, format)?;
    if frame.format.chroma_sampling != ChromaSampling::Cs444 {
        return Err(format!(
            "palette CABAC dump expects 4:4:4 input; got {format}"
        ));
    }

    let cabac = vvc_palette_444_cabac_encoder_with_dump(&frame);
    let semantic_symbols = cabac.semantic_symbols.clone();
    let cabac_bits = cabac.finish();
    let cabac_bytes = bits_to_padded_bytes(&cabac_bits);
    let mut json = String::new();
    json.push_str("{\n");
    json.push_str("  \"kind\": \"framefinery.palette444_cabac.v1\",\n");
    json.push_str(&format!("  \"width\": {},\n", geometry.width));
    json.push_str(&format!("  \"height\": {},\n", geometry.height));
    json.push_str("  \"tile_size\": 8,\n");
    json.push_str("  \"entries\": [\n");
    let entries = vvc_palette_444_tile_entries(&frame);
    for (idx, entry) in entries.iter().enumerate() {
        let comma = if idx + 1 == entries.len() { "" } else { "," };
        json.push_str(&format!(
            "    {{\"x\": {}, \"y\": {}, \"value_y\": {}, \"value_cb\": {}, \"value_cr\": {}}}{}\n",
            entry.x, entry.y, entry.color.y, entry.color.u, entry.color.v, comma
        ));
    }
    json.push_str("  ],\n");
    json.push_str(&format!("  \"cabac_bit_len\": {},\n", cabac_bits.len()));
    json.push_str(&format!(
        "  \"cabac_hex\": \"{}\",\n",
        bytes_to_lower_hex(&cabac_bytes)
    ));
    json.push_str("  \"semantic_symbols\": [\n");
    for (idx, symbol) in semantic_symbols.iter().enumerate() {
        let comma = if idx + 1 == semantic_symbols.len() {
            ""
        } else {
            ","
        };
        json.push_str(&format!(
            "    {{\"kind\": {}, \"data\": {}}}{}\n",
            symbol.kind, symbol.data, comma
        ));
    }
    json.push_str("  ]\n");
    json.push_str("}\n");
    Ok(json)
}

fn vvc_palette_444_tile_entries(frame: &VvcSampledFrame) -> Vec<VvcPalette444TileEntry> {
    let mut entries = Vec::new();
    for y in (0..frame.geometry.height).step_by(8) {
        for x in (0..frame.geometry.width).step_by(8) {
            entries.push(VvcPalette444TileEntry {
                x,
                y,
                color: vvc_palette_444_sample_at(frame, x, y),
            });
        }
    }
    entries
}

fn bits_to_padded_bytes(bits: &[bool]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(bits.len().div_ceil(8));
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for bit in chunk {
            byte = (byte << 1) | u8::from(*bit);
        }
        byte <<= 8 - chunk.len();
        bytes.push(byte);
    }
    bytes
}

fn bytes_to_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
