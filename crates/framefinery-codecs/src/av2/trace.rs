pub fn av2_mvp_444_trace_jsonl_for_frame(
    frame: &[u8],
    request: Av2EncodeRequest,
) -> Result<String, String> {
    let geometry = validate_mvp_request(request)?;
    let stream_format = Av2StreamFormat::from_pixel_format(request.format)
        .expect("validate_mvp_request accepts only supported AV2 stream formats");
    let coded_frame: Vec<u8>;
    let frame = if request.format == PixelFormat::Rgb24 {
        coded_frame = rgb24_to_planar_gbr(frame, geometry);
        coded_frame.as_slice()
    } else {
        frame
    };
    if stream_format.chroma_format == Av2ChromaFormat::Yuv420 {
        let black = av2_black_reconstruction_for_geometry(geometry, stream_format);
        if frame != black {
            return av2_lossy_subsampled_trace_jsonl_for_frame(
                geometry,
                stream_format,
                frame,
                AV2_LOSSY_DEFAULT_QP,
            );
        }
        return av2_black_trace_jsonl_for_format(geometry, stream_format);
    }
    let frame_mode = Av2Mvp444FrameMode::from_frame(frame, geometry, stream_format.bit_depth)?;
    av2_mvp_444_trace_jsonl_for_mode(geometry, stream_format.bit_depth, &frame_mode)
}

pub fn av2_mvp_444_ibc_stats_json_for_frame(
    frame: &[u8],
    request: Av2EncodeRequest,
) -> Result<String, String> {
    let geometry = validate_mvp_request(request)?;
    let stream_format = Av2StreamFormat::from_pixel_format(request.format)
        .expect("validate_mvp_request accepts only supported AV2 stream formats");
    if stream_format.chroma_format != Av2ChromaFormat::Yuv444 {
        return Err(format!(
            "AV2 IBC stats expect yuv444p8, yuv444p10le, gbrp8, or rgb24 input; got {}",
            request.format
        ));
    }

    let coded_frame: Vec<u8>;
    let frame = if request.format == PixelFormat::Rgb24 {
        coded_frame = rgb24_to_planar_gbr(frame, geometry);
        coded_frame.as_slice()
    } else {
        frame
    };
    let frame_mode = Av2Mvp444FrameMode::from_frame(frame, geometry, stream_format.bit_depth)?;
    let (black_mode, stats) = match &frame_mode {
        Av2Mvp444FrameMode::Black => (true, Av2LocalIbcStats::default()),
        Av2Mvp444FrameMode::LumaPalette { ibc, .. } => (
            false,
            ibc.as_ref().map(Av2LocalIbc444::stats).unwrap_or_default(),
        ),
    };

    Ok(format!(
        concat!(
            "{{\n",
            "  \"codec\": \"av2\",\n",
            "  \"tool\": \"local_hash_ibc\",\n",
            "  \"width\": {},\n",
            "  \"height\": {},\n",
            "  \"format\": \"{}\",\n",
            "  \"black_mode\": {},\n",
            "  \"allow_intrabc\": {},\n",
            "  \"total_blocks\": {},\n",
            "  \"blocks_with_above_in_tile\": {},\n",
            "  \"blocks_with_left_in_tile\": {},\n",
            "  \"fixed_drl_supported_blocks\": {},\n",
            "  \"raw_above_hash_matches\": {},\n",
            "  \"raw_left_hash_matches\": {},\n",
            "  \"direct_above_hash_matches\": {},\n",
            "  \"direct_left_hash_matches\": {},\n",
            "  \"above_hash_matches_blocked_by_fixed_drl_guard\": {},\n",
            "  \"left_hash_matches_blocked_by_fixed_drl_guard\": {},\n",
            "  \"above_hash_matches_blocked_by_copied_candidate\": {},\n",
            "  \"left_hash_matches_blocked_by_copied_candidate\": {},\n",
            "  \"selected_above_copy_blocks\": {},\n",
            "  \"selected_left_copy_blocks\": {},\n",
            "  \"selected_copy_blocks\": {}\n",
            "}}\n"
        ),
        geometry.width,
        geometry.height,
        request.format,
        black_mode,
        frame_mode.allow_intrabc(),
        stats.total_blocks,
        stats.blocks_with_above_in_tile,
        stats.blocks_with_left_in_tile,
        stats.fixed_drl_supported_blocks,
        stats.raw_above_hash_matches,
        stats.raw_left_hash_matches,
        stats.direct_above_hash_matches,
        stats.direct_left_hash_matches,
        stats.above_hash_matches_blocked_by_fixed_drl_guard,
        stats.left_hash_matches_blocked_by_fixed_drl_guard,
        stats.above_hash_matches_blocked_by_copied_candidate,
        stats.left_hash_matches_blocked_by_copied_candidate,
        stats.selected_above_copy_blocks,
        stats.selected_left_copy_blocks,
        stats.selected_copy_blocks(),
    ))
}

pub fn av2_black_444_trace_jsonl(request: Av2EncodeRequest) -> Result<String, String> {
    let geometry = validate_fixed_black_444_request(request)?;
    av2_mvp_444_trace_jsonl_for_mode(
        geometry,
        request.format.bit_depth(),
        &Av2Mvp444FrameMode::Black,
    )
}

fn av2_mvp_444_trace_jsonl_for_mode(
    geometry: Av2VideoGeometry,
    bit_depth: SampleBitDepth,
    frame_mode: &Av2Mvp444FrameMode,
) -> Result<String, String> {
    let tile_layout = av2_tile_layout_for_frame_mode(geometry, frame_mode);
    let sequence = av2_mvp_444_sequence_header_payload(geometry, bit_depth, frame_mode.profile());
    let closed_loop_header = av2_mvp_444_closed_loop_key_header_payload(
        frame_mode.allow_screen_content_tools(),
        frame_mode.allow_intrabc(),
        &tile_layout,
        Av2StreamFormat {
            chroma_format: Av2ChromaFormat::Yuv444,
            bit_depth,
        },
        Av2QuantizationParams::lossless(),
    );
    let entropy = av2_tile_entropy_payloads_for_mode(&tile_layout, frame_mode, true);
    let mut lines = String::new();

    push_av2_trace_line(
        &mut lines,
        "obu",
        "obu.temporal_delimiter",
        "AV2 v1.0.0 Section 5.4 OBU syntax",
        "header+payload",
        0,
        16,
    );
    for field in &sequence.fields {
        push_av2_trace_line(
            &mut lines,
            "sequence_header",
            field.name,
            av2_spec_section_for_syntax_field(field.name),
            &format!("{:?}", field.code),
            field.bit_offset,
            field.bit_count,
        );
    }
    push_av2_trace_line(
        &mut lines,
        "obu",
        "obu.closed_loop_key",
        "AV2 v1.0.0 Sections 5.19 and 5.20.1 tile group syntax",
        "header",
        0,
        8,
    );
    for field in &closed_loop_header.fields {
        push_av2_trace_line(
            &mut lines,
            "closed_loop_key_header",
            field.name,
            av2_spec_section_for_syntax_field(field.name),
            &format!("{:?}", field.code),
            field.bit_offset,
            field.bit_count,
        );
    }
    for (tile_index, entropy) in entropy.iter().enumerate() {
        for field in &entropy.fields {
            push_av2_entropy_trace_line(&mut lines, tile_index, field);
        }
    }
    Ok(lines)
}

fn av2_black_trace_jsonl_for_format(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
) -> Result<String, String> {
    let tile_layout = Av2TileLayout::for_geometry(geometry);
    let profile = Av2Black444MvpProfile::current();
    let sequence = av2_mvp_sequence_header_payload(geometry, profile, stream_format);
    let closed_loop_header = av2_mvp_444_closed_loop_key_header_payload(
        false,
        false,
        &tile_layout,
        stream_format,
        Av2QuantizationParams::lossless(),
    );
    let entropy: Vec<_> = tile_layout
        .regions
        .iter()
        .map(|&region| {
            av2_black_tile_entropy_payload_for_region(region, profile, stream_format.chroma_format)
        })
        .collect();
    let mut lines = String::new();

    push_av2_trace_line(
        &mut lines,
        "obu",
        "obu.temporal_delimiter",
        "AV2 v1.0.0 Section 5.4 OBU syntax",
        "header+payload",
        0,
        16,
    );
    for field in &sequence.fields {
        push_av2_trace_line(
            &mut lines,
            "sequence_header",
            field.name,
            av2_spec_section_for_syntax_field(field.name),
            &format!("{:?}", field.code),
            field.bit_offset,
            field.bit_count,
        );
    }
    push_av2_trace_line(
        &mut lines,
        "obu",
        "obu.closed_loop_key",
        "AV2 v1.0.0 Sections 5.19 and 5.20.1 tile group syntax",
        "header",
        0,
        8,
    );
    for field in &closed_loop_header.fields {
        push_av2_trace_line(
            &mut lines,
            "closed_loop_key_header",
            field.name,
            av2_spec_section_for_syntax_field(field.name),
            &format!("{:?}", field.code),
            field.bit_offset,
            field.bit_count,
        );
    }
    for (tile_index, entropy) in entropy.iter().enumerate() {
        for field in &entropy.fields {
            push_av2_entropy_trace_line(&mut lines, tile_index, field);
        }
    }
    Ok(lines)
}

fn av2_lossy_subsampled_trace_jsonl_for_frame(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    frame: &[u8],
    qp: u8,
) -> Result<String, String> {
    let expected_len = Picture::expected_len(
        geometry.width,
        geometry.height,
        stream_format.pixel_format(),
    );
    if frame.len() != expected_len {
        return Err(format!(
            "AV2 {} trace input length mismatch: expected {expected_len}, got {}",
            stream_format.pixel_format(),
            frame.len()
        ));
    }
    let tile_layout = Av2TileLayout::lossy_subsampled_for_geometry(geometry);
    let profile = Av2Black444MvpProfile::current();
    let sequence = av2_mvp_sequence_header_payload(geometry, profile, stream_format);
    let closed_loop_header = av2_mvp_444_closed_loop_key_header_payload(
        false,
        false,
        &tile_layout,
        stream_format,
        Av2QuantizationParams::regular_qp(qp, stream_format.bit_depth),
    );
    let mut reconstruction = vec![0; expected_len];
    let entropy: Vec<_> = tile_layout
        .regions
        .iter()
        .map(|&region| {
            av2_lossy_subsampled_tile_entropy_payload_for_region(
                region,
                profile,
                geometry,
                stream_format.chroma_format,
                stream_format.bit_depth,
                frame,
                &mut reconstruction,
                qp,
                Av2QuantizationParams::regular_qp(qp, stream_format.bit_depth).base_qindex,
            )
        })
        .collect();
    let mut lines = String::new();

    push_av2_trace_line(
        &mut lines,
        "obu",
        "obu.temporal_delimiter",
        "AV2 v1.0.0 Section 5.4 OBU syntax",
        "header+payload",
        0,
        16,
    );
    for field in &sequence.fields {
        push_av2_trace_line(
            &mut lines,
            "sequence_header",
            field.name,
            av2_spec_section_for_syntax_field(field.name),
            &format!("{:?}", field.code),
            field.bit_offset,
            field.bit_count,
        );
    }
    push_av2_trace_line(
        &mut lines,
        "obu",
        "obu.closed_loop_key",
        "AV2 v1.0.0 Sections 5.19 and 5.20.1 tile group syntax",
        "header",
        0,
        8,
    );
    for field in &closed_loop_header.fields {
        push_av2_trace_line(
            &mut lines,
            "closed_loop_key_header",
            field.name,
            av2_spec_section_for_syntax_field(field.name),
            &format!("{:?}", field.code),
            field.bit_offset,
            field.bit_count,
        );
    }
    for (tile_index, entropy) in entropy.iter().enumerate() {
        for field in &entropy.fields {
            push_av2_entropy_trace_line(&mut lines, tile_index, field);
        }
    }
    Ok(lines)
}

fn push_av2_trace_line(
    out: &mut String,
    phase: &str,
    name: &str,
    spec: &str,
    code: &str,
    bit_offset: usize,
    bit_count: usize,
) {
    out.push_str(&format!(
        "{{\"codec\":\"av2\",\"source\":\"software\",\"phase\":\"{}\",\"name\":\"{}\",\"spec\":\"{}\",\"code\":\"{}\",\"bit_offset\":{},\"bit_count\":{}}}\n",
        escape_json(phase),
        escape_json(name),
        escape_json(spec),
        escape_json(code),
        bit_offset,
        bit_count
    ));
}

fn push_av2_entropy_trace_line(
    out: &mut String,
    tile_index: usize,
    field: &entropy::Av2EntropyField,
) {
    let mut line = format!(
        "{{\"codec\":\"av2\",\"source\":\"software\",\"phase\":\"tile_entropy\",\"tile_index\":{},\"name\":\"{}\",\"spec\":\"{}\",\"code\":\"{}\",\"bit_offset\":{},\"bit_count\":{}",
        tile_index,
        escape_json(field.name),
        escape_json(av2_spec_section_for_entropy_field(field.name)),
        escape_json(&format!("{:?}", field.code)),
        field.symbol_offset,
        field.bit_count
    );
    if let Some(symbol) = field.symbol {
        line.push_str(&format!(",\"symbol\":{symbol}"));
    }
    if let Some(value) = field.literal_value {
        line.push_str(&format!(",\"literal_value\":{value}"));
    }
    if let Some(fl) = field.fl {
        line.push_str(&format!(",\"fl\":{fl}"));
    }
    if let Some(fh) = field.fh {
        line.push_str(&format!(",\"fh\":{fh}"));
    }
    if let Some(fl_inc) = field.fl_inc {
        line.push_str(&format!(",\"fl_inc\":{fl_inc}"));
    }
    if let Some(fh_inc) = field.fh_inc {
        line.push_str(&format!(",\"fh_inc\":{fh_inc}"));
    }
    line.push_str("}\n");
    out.push_str(&line);
}

fn av2_spec_section_for_syntax_field(name: &str) -> &'static str {
    if name.starts_with("sequence_header.") || name.starts_with("sequence_") {
        "AV2 v1.0.0 Section 5.4.1 sequence_header_obu()"
    } else if name.starts_with("tile_group.") || name.starts_with("uncompressed_header.") {
        "AV2 v1.0.0 Sections 5.19 and 5.20.1 tile_group_obu()"
    } else if name.starts_with("tile_info.")
        || name.starts_with("quantization.")
        || name.starts_with("segmentation.")
        || name.starts_with("quantization_matrix.")
    {
        "AV2 v1.0.0 Section 5.20.1 uncompressed header syntax"
    } else if name == "trailing_bits" {
        "AV2 v1.0.0 Section 5.4.1 trailing bits"
    } else {
        "AV2 v1.0.0 syntax"
    }
}

fn av2_spec_section_for_entropy_field(name: &str) -> &'static str {
    if name.starts_with("tile.partition.") {
        "AV2 v1.0.0 Section 5.20.3.2 partition()"
    } else if name.starts_with("tile.intrabc.") {
        "AV2 v1.0.0 Sections 5.20.5.1 and 5.20.5.3 intra block copy syntax"
    } else if name.starts_with("tile.intra.") {
        "AV2 v1.0.0 Sections 5.20.5.5 and 5.20.5.6 intra mode syntax"
    } else if name.starts_with("tile.palette.") {
        "AV2 v1.0.0 Sections 5.20.8.1 and 5.20.8.4 palette syntax"
    } else if name.starts_with("tile.coeff.") {
        "AV2 v1.0.0 Sections 5.20.7.23, 5.20.7.24, and 5.20.7.27 residual coefficient syntax"
    } else {
        "AV2 v1.0.0 tile entropy syntax"
    }
}

fn escape_json(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
