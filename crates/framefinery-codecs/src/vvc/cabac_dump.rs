pub fn vvc_cabac_vector_dump_json(
    input: &[u8],
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    format: PixelFormat,
) -> Result<String, String> {
    let source_frame = sample_vvc_yuv_frame(input, params, geometry, format)?;
    let slice_config = VvcSliceSyntaxConfig::for_picture_format(source_frame.format);
    let color = quantize_vvc_frame(&source_frame);
    let params = vvc_ctu_partition_params_with_luma_max_leaf_size_and_chroma(
        source_frame.geometry,
        color,
        VVC_CURRENT_MAX_LUMA_LEAF_SIZE,
        slice_config.coding_tree.chroma_sampling,
        slice_config.coding_tree.dual_tree_intra,
    )
    .ok_or_else(|| {
        format!(
            "VVC CABAC vector dump has no generated CTU path for coded geometry {}x{}",
            source_frame.geometry.coded_width(),
            source_frame.geometry.coded_height()
        )
    })?;
    let dump = vvc_ctu_partition_cabac_dump(&params, slice_config);
    let mapped_context_symbols = dump
        .semantic_symbols
        .iter()
        .filter(|symbol| symbol.kind == 2)
        .count();
    if mapped_context_symbols != dump.context_bin_count {
        return Err(format!(
            "VVC CABAC vector dump used {} context bins but only {} have RTL context IDs; audit VvcCabacContext::rtl_context_id before using this as an RTL reference",
            dump.context_bin_count, mapped_context_symbols
        ));
    }
    Ok(format_vvc_cabac_vector_dump_json(
        source_frame.geometry,
        format,
        &params,
        &dump.symbols,
        &dump.semantic_symbols,
        &dump.context_events,
        &dump.bin_engine_events,
        &dump.bits,
    ))
}

struct VvcCtuCabacDump {
    symbols: Vec<VvcCabacDumpSymbol>,
    semantic_symbols: Vec<VvcCabacDumpSymbol>,
    context_events: Vec<VvcCabacDumpContextEvent>,
    context_bin_count: usize,
    bin_engine_events: Vec<cabac::VvcCabacDumpBinEngineEvent>,
    bits: Vec<bool>,
}

fn vvc_ctu_partition_cabac_dump(
    params: &VvcCtuPartitionParams,
    slice_config: VvcSliceSyntaxConfig,
) -> VvcCtuCabacDump {
    debug_assert!((8..=64).contains(&params.root_width));
    debug_assert!((8..=64).contains(&params.root_height));
    debug_assert!(params.visible_width >= 8 && params.visible_height >= 8);

    let mut cabac = VvcCabacEncoder::new_with_dump();
    cabac.start();
    encode_ctu_partition_body(&mut cabac, params, slice_config);
    cabac.encode_bin_trm(true);
    let semantic_symbols = cabac.semantic_symbols.clone();
    let context_events = cabac.context_events.clone();
    let context_bin_count = cabac.context_bin_count;
    let bin_engine_events = cabac.bin_engine_events.clone();
    let symbols = cabac.dump_symbols.clone();
    let bits = cabac.finish();
    VvcCtuCabacDump {
        symbols,
        semantic_symbols,
        context_events,
        context_bin_count,
        bin_engine_events,
        bits,
    }
}

#[cfg(feature = "vvc-stats")]
fn vvc_ctu_partition_cabac_dump_with_frame_state(
    frame_state: &mut VvcFrameCtuCabacState,
    slice_address: usize,
    params: &VvcCtuPartitionParams,
    slice_config: VvcSliceSyntaxConfig,
) -> VvcCtuCabacDump {
    debug_assert!((8..=64).contains(&params.root_width));
    debug_assert!((8..=64).contains(&params.root_height));
    debug_assert!(params.visible_width >= 8 && params.visible_height >= 8);

    let mut cabac = VvcCabacEncoder::new_with_dump();
    cabac.start();
    frame_state.encode_ctu(&mut cabac, slice_address, params, slice_config);
    cabac.encode_bin_trm(true);
    let semantic_symbols = cabac.semantic_symbols.clone();
    let context_events = cabac.context_events.clone();
    let context_bin_count = cabac.context_bin_count;
    let bin_engine_events = cabac.bin_engine_events.clone();
    let symbols = cabac.dump_symbols.clone();
    let bits = cabac.finish();
    VvcCtuCabacDump {
        symbols,
        semantic_symbols,
        context_events,
        context_bin_count,
        bin_engine_events,
        bits,
    }
}

fn format_vvc_cabac_vector_dump_json(
    geometry: VvcVideoGeometry,
    format: PixelFormat,
    params: &VvcCtuPartitionParams,
    symbols: &[VvcCabacDumpSymbol],
    semantic_symbols: &[VvcCabacDumpSymbol],
    context_events: &[VvcCabacDumpContextEvent],
    bin_engine_events: &[cabac::VvcCabacDumpBinEngineEvent],
    bits: &[bool],
) -> String {
    let mut json = String::new();
    json.push_str("{\"kind\":\"framefinery.vvc.cabac_vector.v1\"");
    json.push_str(&format!(",\"width\":{}", geometry.width));
    json.push_str(&format!(",\"height\":{}", geometry.height));
    json.push_str(&format!(",\"format\":\"{format}\""));
    json.push_str(&format!(
        ",\"luma_dc_abs_level\":{}",
        params.luma_tu_abs_levels[0]
    ));
    json.push_str(&format!(
        ",\"luma_dc_negative\":{}",
        if params.luma_tu_negative[0] {
            "true"
        } else {
            "false"
        }
    ));
    json.push_str(",\"luma_ac_levels\":[");
    for (idx, level) in params.luma_tu_ac_levels[0].iter().enumerate() {
        if idx != 0 {
            json.push(',');
        }
        json.push_str(&level.to_string());
    }
    json.push(']');
    json.push_str(&format!(",\"luma_tu_count\":{}", params.luma_tu_count));
    json.push_str(",\"luma_tu_abs_levels_all\":[");
    for idx in 0..params.luma_tu_count {
        if idx != 0 {
            json.push(',');
        }
        json.push_str(&params.luma_tu_abs_levels[idx].to_string());
    }
    json.push(']');
    json.push_str(",\"luma_tu_negative_all\":[");
    for idx in 0..params.luma_tu_count {
        if idx != 0 {
            json.push(',');
        }
        json.push_str(if params.luma_tu_negative[idx] {
            "true"
        } else {
            "false"
        });
    }
    json.push(']');
    json.push_str(",\"luma_tu_ac_levels_all\":[");
    for tu_idx in 0..params.luma_tu_count {
        if tu_idx != 0 {
            json.push(',');
        }
        json.push('[');
        for (idx, level) in params.luma_tu_ac_levels[tu_idx].iter().enumerate() {
            if idx != 0 {
                json.push(',');
            }
            json.push_str(&level.to_string());
        }
        json.push(']');
    }
    json.push(']');
    json.push_str(&format!(",\"cb_dc_abs_level\":{}", params.cb_dc_abs_level));
    json.push_str(&format!(
        ",\"cb_dc_negative\":{}",
        if params.cb_dc_negative {
            "true"
        } else {
            "false"
        }
    ));
    json.push_str(&format!(
        ",\"cb_tu_dc_level\":{}",
        params.cb_tu_dc_levels[0]
    ));
    json.push_str(&format!(
        ",\"cr_tu_dc_level\":{}",
        params.cr_tu_dc_levels[0]
    ));
    json.push_str(",\"cb_tu_ac_levels\":[");
    for (idx, level) in params.cb_tu_ac_levels[0].iter().enumerate() {
        if idx != 0 {
            json.push(',');
        }
        json.push_str(&level.to_string());
    }
    json.push(']');
    json.push_str(",\"cr_tu_ac_levels\":[");
    for (idx, level) in params.cr_tu_ac_levels[0].iter().enumerate() {
        if idx != 0 {
            json.push(',');
        }
        json.push_str(&level.to_string());
    }
    json.push(']');
    json.push_str(&format!(",\"chroma_tu_count\":{}", params.chroma_tu_count));
    json.push_str(",\"cb_tu_dc_levels_all\":[");
    for idx in 0..params.chroma_tu_count {
        if idx != 0 {
            json.push(',');
        }
        json.push_str(&params.cb_tu_dc_levels[idx].to_string());
    }
    json.push(']');
    json.push_str(",\"cr_tu_dc_levels_all\":[");
    for idx in 0..params.chroma_tu_count {
        if idx != 0 {
            json.push(',');
        }
        json.push_str(&params.cr_tu_dc_levels[idx].to_string());
    }
    json.push(']');
    json.push_str(",\"cb_tu_ac_levels_all\":[");
    for tu_idx in 0..params.chroma_tu_count {
        if tu_idx != 0 {
            json.push(',');
        }
        json.push('[');
        for (idx, level) in params.cb_tu_ac_levels[tu_idx].iter().enumerate() {
            if idx != 0 {
                json.push(',');
            }
            json.push_str(&level.to_string());
        }
        json.push(']');
    }
    json.push(']');
    json.push_str(",\"cr_tu_ac_levels_all\":[");
    for tu_idx in 0..params.chroma_tu_count {
        if tu_idx != 0 {
            json.push(',');
        }
        json.push('[');
        for (idx, level) in params.cr_tu_ac_levels[tu_idx].iter().enumerate() {
            if idx != 0 {
                json.push(',');
            }
            json.push_str(&level.to_string());
        }
        json.push(']');
    }
    json.push(']');
    json.push_str(",\"symbol_record_bytes\":5");
    json.push_str(",\"context_id_bits\":10");
    json.push_str(",\"symbol_encoding\":\"kind_u8_data_u32be_hex\"");
    json.push_str(&format!(
        ",\"mapped_context_bin_count\":{}",
        context_events.len()
    ));
    json.push_str(&format!(",\"cabac_bit_len\":{}", bits.len()));
    json.push_str(",\"cabac_bytes_hex\":\"");
    append_hex_bytes(&mut json, bits);
    json.push_str("\",\"symbols_hex\":\"");
    append_symbol_records_hex(&mut json, symbols);
    json.push_str("\",\"semantic_symbols_hex\":\"");
    append_symbol_records_hex(&mut json, semantic_symbols);
    json.push_str("\",\"context_event_record_bytes\":8");
    json.push_str(
        ",\"context_event_encoding\":\"ctx_id_u16be_bin_u8_range_u16be_lps_u16be_mps_u8_hex\"",
    );
    json.push_str(",\"context_events_hex\":\"");
    append_context_event_records_hex(&mut json, context_events);
    json.push_str("\",\"bin_engine_event_record_bytes\":20");
    json.push_str(",\"bin_engine_event_encoding\":\"kind_u8_bin_u8_lps_u16be_mps_u8_low_in_u32be_range_in_u16be_bits_left_in_u8_low_out_u32be_range_out_u16be_bits_left_out_u8_write_out_u8_hex\"");
    json.push_str(",\"bin_engine_events_hex\":\"");
    append_bin_engine_event_records_hex(&mut json, bin_engine_events);
    json.push_str("\"}\n");
    json
}

fn append_bin_engine_event_records_hex(
    out: &mut String,
    events: &[cabac::VvcCabacDumpBinEngineEvent],
) {
    for event in events {
        append_byte_hex(out, event.kind);
        append_byte_hex(out, u8::from(event.bin));
        for byte in event.lps.to_be_bytes() {
            append_byte_hex(out, byte);
        }
        append_byte_hex(out, u8::from(event.mps));
        for byte in event.low_in.to_be_bytes() {
            append_byte_hex(out, byte);
        }
        for byte in event.range_in.to_be_bytes() {
            append_byte_hex(out, byte);
        }
        append_byte_hex(out, event.bits_left_in);
        for byte in event.low_out.to_be_bytes() {
            append_byte_hex(out, byte);
        }
        for byte in event.range_out.to_be_bytes() {
            append_byte_hex(out, byte);
        }
        append_byte_hex(out, event.bits_left_out);
        append_byte_hex(out, u8::from(event.write_out));
    }
}

fn append_context_event_records_hex(out: &mut String, events: &[VvcCabacDumpContextEvent]) {
    for event in events {
        for byte in event.ctx_id.to_be_bytes() {
            append_byte_hex(out, byte);
        }
        append_byte_hex(out, u8::from(event.bin));
        for byte in event.range.to_be_bytes() {
            append_byte_hex(out, byte);
        }
        for byte in event.lps.to_be_bytes() {
            append_byte_hex(out, byte);
        }
        append_byte_hex(out, u8::from(event.mps));
    }
}

fn append_hex_bytes(out: &mut String, bits: &[bool]) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for bit in chunk {
            byte = (byte << 1) | u8::from(*bit);
        }
        byte <<= 8 - chunk.len();
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
}

fn append_symbol_records_hex(out: &mut String, symbols: &[VvcCabacDumpSymbol]) {
    for symbol in symbols {
        append_byte_hex(out, symbol.kind);
        for byte in symbol.data.to_be_bytes() {
            append_byte_hex(out, byte);
        }
    }
}

fn append_byte_hex(out: &mut String, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push(HEX[(byte >> 4) as usize] as char);
    out.push(HEX[(byte & 0x0f) as usize] as char);
}
