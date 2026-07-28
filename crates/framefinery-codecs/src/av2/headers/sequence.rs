#[cfg(test)]
fn av2_black_444_sequence_header_payload(geometry: Av2VideoGeometry) -> Av2SyntaxPayload {
    av2_mvp_444_sequence_header_payload(
        geometry,
        SampleBitDepth::new(8).expect("8-bit depth is supported"),
        Av2Black444MvpProfile::current(),
    )
}

fn av2_mvp_444_sequence_header_payload(
    geometry: Av2VideoGeometry,
    bit_depth: SampleBitDepth,
    profile: Av2Black444MvpProfile,
) -> Av2SyntaxPayload {
    av2_mvp_sequence_header_payload(
        geometry,
        profile,
        Av2StreamFormat {
            chroma_format: Av2ChromaFormat::Yuv444,
            bit_depth,
        },
    )
}

fn av2_mvp_sequence_header_payload(
    geometry: Av2VideoGeometry,
    profile: Av2Black444MvpProfile,
    stream_format: Av2StreamFormat,
) -> Av2SyntaxPayload {
    av2_mvp_sequence_header_payload_with_mode(geometry, profile, stream_format, true)
}

fn av2_mvp_predictive_sequence_header_payload(
    geometry: Av2VideoGeometry,
    profile: Av2Black444MvpProfile,
    stream_format: Av2StreamFormat,
) -> Av2SyntaxPayload {
    av2_mvp_sequence_header_payload_with_mode(geometry, profile, stream_format, false)
}

fn append_rgb_content_interpretation_if_needed(out: &mut Vec<u8>, rgb_identity: bool) {
    if !rgb_identity {
        return;
    }
    append_obu(
        out,
        Av2ObuType::ContentInterpretation,
        &av2_rgb_identity_content_interpretation_payload(),
    );
}

fn av2_rgb_identity_content_interpretation_payload() -> Av2SyntaxPayload {
    let mut writer = Av2SyntaxWriter::new();
    writer.write_literal("content_interpretation.ci_scan_type_idc", 0, 2);
    writer.write_flag(
        "content_interpretation.ci_color_description_present_flag",
        true,
    );
    writer.write_flag(
        "content_interpretation.ci_chroma_sample_position_present_flag",
        false,
    );
    writer.write_flag(
        "content_interpretation.ci_aspect_ratio_info_present_flag",
        false,
    );
    writer.write_flag("content_interpretation.ci_timing_info_present_flag", false);
    writer.write_literal("content_interpretation.ci_reserved_zero_2bits", 0, 2);
    writer.write_rice_golomb(
        "content_interpretation.color_description_idc",
        AV2_COLOR_DESCRIPTION_IDC_SRGB,
        2,
    );
    writer.write_flag("content_interpretation.full_range_flag", true);
    writer.write_flag("content_interpretation.ci_extension_present_flag", false);
    writer.trailing_bits();
    writer.finish()
}

fn av2_mvp_sequence_header_payload_with_mode(
    geometry: Av2VideoGeometry,
    profile: Av2Black444MvpProfile,
    stream_format: Av2StreamFormat,
    single_picture_header: bool,
) -> Av2SyntaxPayload {
    let mut writer = Av2SyntaxWriter::new();
    let width_bits = av2_frame_dimension_bits(geometry.width);
    let height_bits = av2_frame_dimension_bits(geometry.height);

    // AV2 v1.0.0 sequence_header_obu(), mirrored from AVM
    // av2_write_sequence_header_obu().
    writer.write_uvlc("sequence_header.seq_header_id", 0);
    writer.write_literal(
        "sequence_header.seq_profile_idc",
        u64::from(stream_format.sequence_profile_idc()),
        AV2_PROFILE_BITS,
    );
    writer.write_flag(
        "sequence_header.single_picture_header_flag",
        single_picture_header,
    );
    writer.write_literal(
        "sequence_header.seq_max_level_idx",
        u64::from(av2_sequence_level_for_geometry(geometry)),
        AV2_LEVEL_BITS,
    );
    if av2_sequence_level_for_geometry(geometry) >= 4 && !single_picture_header {
        writer.write_flag("sequence_header.seq_tier", false);
    }
    writer.write_uvlc(
        "sequence_header.seq_chroma_format_idc",
        stream_format.chroma_format.sequence_header_idc(),
    );
    writer.write_uvlc(
        "sequence_header.bitdepth_lut_idx",
        stream_format.bitdepth_lut_index(),
    );
    if !single_picture_header {
        writer.write_literal("sequence_header.seq_lcr_id", 0, 3);
        writer.write_flag("sequence_header.still_picture", false);
        writer.write_literal("sequence_header.max_tlayer_id", 0, 2);
        writer.write_literal("sequence_header.max_mlayer_id", 0, 3);
        writer.write_flag("sequence_header.monotonic_output_order_flag", true);
    }
    writer.write_literal(
        "sequence_header.num_bits_width_minus_1",
        (width_bits - 1) as u64,
        4,
    );
    writer.write_literal(
        "sequence_header.num_bits_height_minus_1",
        (height_bits - 1) as u64,
        4,
    );
    writer.write_literal(
        "sequence_header.max_frame_width_minus_1",
        (geometry.width - 1) as u64,
        width_bits,
    );
    writer.write_literal(
        "sequence_header.max_frame_height_minus_1",
        (geometry.height - 1) as u64,
        height_bits,
    );
    writer.write_flag("sequence_header.conf_win_enabled_flag", false);

    if !single_picture_header {
        writer.write_flag(
            "sequence_header.seq_max_display_model_info_present_flag",
            false,
        );
        writer.write_flag("sequence_header.decoder_model_info_present_flag", false);
    }

    write_fixed_black_444_sequence_tools(&mut writer, profile, single_picture_header);

    writer.write_flag("sequence_header.film_grain_params_present", false);
    writer.write_flag("sequence_header.seq_extension_present_flag", false);
    writer.trailing_bits();
    writer.finish()
}

fn av2_sequence_level_for_geometry(geometry: Av2VideoGeometry) -> u8 {
    const LEVELS: &[(u8, usize, usize, usize)] = &[
        (0, 147_456, 640, 640),
        (1, 278_784, 880, 880),
        (2, 665_856, 1360, 1360),
        (3, 1_065_024, 1720, 1720),
        (4, 2_359_296, 2560, 2560),
        (6, 8_912_896, 4975, 4975),
        (10, 35_651_584, 9951, 9951),
        (14, 142_606_336, 19902, 19902),
        (18, 570_425_344, 39804, 39804),
    ];
    let picture_size = geometry.width * geometry.height;
    LEVELS
        .iter()
        .find_map(|&(level, max_picture_size, max_width, max_height)| {
            (picture_size <= max_picture_size
                && geometry.width <= max_width
                && geometry.height <= max_height)
                .then_some(level)
        })
        .unwrap_or(AV2_SEQUENCE_LEVEL_MAX)
}

fn av2_frame_dimension_bits(dimension: usize) -> u8 {
    assert!(dimension > 0, "AV2 frame dimension must be positive");
    let max_index = (dimension - 1) as u64;
    (64 - max_index.leading_zeros()) as u8
}

fn write_fixed_black_444_sequence_tools(
    writer: &mut Av2SyntaxWriter,
    profile: Av2Black444MvpProfile,
    single_picture_header: bool,
) {
    // AV2 v1.0.0 sequence_header() tool groups, mirrored from AVM
    // write_sequence_header(). Values are the fixed AVM choices for one
    // black yuv444p8 still picture in the minimum viable bitstream subset.
    writer.write_flag("sequence_partition.sb_size_is_256", false);
    writer.write_flag("sequence_partition.sb_size_is_128", false);
    writer.write_flag("sequence_partition.enable_sdp", profile.enable_sdp);
    writer.write_flag(
        "sequence_partition.enable_ext_partitions",
        profile.enable_ext_partitions,
    );
    if profile.enable_ext_partitions {
        writer.write_flag(
            "sequence_partition.enable_uneven_4way_partitions",
            profile.enable_uneven_4way_partitions,
        );
    }
    writer.write_flag("sequence_partition.max_pb_aspect_ratio_lt2", false);

    writer.write_flag("sequence_segment.enable_ext_seg", false);
    writer.write_flag("sequence_segment.seq_seg_info_present_flag", false);

    writer.write_flag("sequence_intra.enable_intra_dip", false);
    writer.write_flag(
        "sequence_intra.enable_intra_edge_filter",
        profile.enable_intra_edge_filter,
    );
    writer.write_flag("sequence_intra.enable_mrls", profile.enable_mrls);
    writer.write_flag("sequence_intra.enable_cfl_intra", profile.enable_cfl_intra);
    writer.write_literal("sequence_intra.cfl_ds_filter_index", 0, 2);
    writer.write_flag("sequence_intra.enable_mhccp", profile.enable_mhccp);
    writer.write_flag("sequence_intra.enable_ibp", profile.enable_ibp);

    if !single_picture_header {
        for _ in 1..5 {
            writer.write_flag("sequence_inter.motion_mode_enabled", false);
        }
        writer.write_flag("sequence_inter.enable_masked_compound", false);
        writer.write_flag("sequence_inter.enable_ref_frame_mvs", false);
        writer.write_literal(
            "sequence_inter.order_hint_bits_minus_1",
            u64::from(AV2_PREDICTIVE_ORDER_HINT_BITS - 1),
            4,
        );
    }
    writer.write_flag("sequence_inter.enable_refmvbank", profile.enable_refmvbank);
    writer.write_flag(
        "sequence_inter.is_drl_reorder_disable",
        profile.is_drl_reorder_disable,
    );
    if !profile.is_drl_reorder_disable {
        writer.write_flag("sequence_inter.enable_drl_reorder_constraint", false);
    }
    if !single_picture_header {
        writer.write_flag("sequence_inter.enable_explicit_ref_frame_map", false);
        writer.write_flag("sequence_inter.signal_dpb_explicit", true);
        writer.write_literal("sequence_inter.ref_frames_minus_1", 1, 4);
        writer.write_literal("sequence_inter.number_of_bits_for_lt_frame_id", 0, 3);
        writer.write_quniform(
            "sequence_inter.def_max_drl_bits_minus_min",
            AV2_MAX_MAX_DRL_BITS_MINUS_MIN_PLUS_ONE,
            0,
        );
        writer.write_flag("sequence_inter.allow_frame_max_drl_bits", false);
    }
    writer.write_quniform(
        "sequence_inter.def_max_bvp_drl_bits_minus_min",
        AV2_MAX_MAX_IBC_DRL_BITS_MINUS_MIN_PLUS_ONE,
        profile.def_max_bvp_drl_bits_minus_min,
    );
    writer.write_flag(
        "sequence_inter.allow_frame_max_bvp_drl_bits",
        profile.allow_frame_max_bvp_drl_bits,
    );
    if !single_picture_header {
        writer.write_literal("sequence_inter.num_same_ref_compound", 0, 2);
        writer.write_flag("sequence_inter.enable_tip", false);
        writer.write_flag("sequence_inter.enable_mv_traj", false);
    }
    writer.write_flag("sequence_inter.enable_bawp", profile.enable_bawp);
    if !single_picture_header {
        writer.write_flag("sequence_inter.enable_cwp", false);
        writer.write_flag("sequence_inter.enable_imp_msk_bld", false);
        writer.write_flag("sequence_inter.enable_lf_sub_pu", false);
        writer.write_literal("sequence_inter.enable_opfl_refine", 0, 2);
        writer.write_flag("sequence_inter.enable_refinemv", false);
        writer.write_flag("sequence_inter.enable_bru", false);
        writer.write_flag("sequence_inter.enable_adaptive_mvd", false);
        writer.write_flag("sequence_inter.enable_mvd_sign_derive", false);
        writer.write_flag("sequence_inter.enable_flex_mvres", false);
        writer.write_flag("sequence_inter.enable_global_motion", false);
        writer.write_flag("sequence_inter.enable_short_refresh_frame_flags", false);
    }

    if !single_picture_header {
        writer.write_flag("sequence_scc.force_screen_content_tools_select", true);
        writer.write_flag("sequence_scc.force_integer_mv_select", true);
    }

    writer.write_flag("sequence_transform.enable_fsc", profile.enable_fsc);
    if !profile.enable_fsc {
        writer.write_flag(
            "sequence_transform.enable_idtx_intra",
            profile.enable_idtx_intra,
        );
    }
    writer.write_flag("sequence_transform.enable_ist", false);
    writer.write_flag("sequence_transform.enable_inter_ist", false);
    writer.write_flag(
        "sequence_transform.enable_chroma_dctonly",
        profile.enable_chroma_dctonly,
    );
    if !single_picture_header {
        writer.write_flag("sequence_transform.enable_inter_ddt", false);
    }
    writer.write_flag("sequence_transform.reduced_tx_part_set", false);
    writer.write_flag("sequence_transform.enable_cctx", profile.enable_cctx);
    writer.write_flag("sequence_transform.enable_tcq_nonzero", false);
    writer.write_flag("sequence_transform.enable_parity_hiding", false);
    if !single_picture_header {
        writer.write_flag("sequence_transform.enable_avg_cdf", true);
        writer.write_flag("sequence_transform.avg_cdf_type", true);
    }
    writer.write_flag("sequence_transform.separate_uv_delta_q", false);
    writer.write_flag("sequence_transform.equal_ac_dc_q", true);
    writer.write_literal(
        "sequence_transform.base_uv_ac_delta_q_minus_min",
        (0 - AV2_DELTA_DCQUANT_MIN as i16) as u64,
        5,
    );
    writer.write_flag("sequence_transform.uv_ac_delta_q_enabled", false);

    writer.write_flag("sequence_filter.disable_loopfilters_across_tiles", false);
    writer.write_flag("sequence_filter.enable_cdef", false);
    writer.write_flag("sequence_filter.enable_gdf", false);
    writer.write_flag("sequence_filter.enable_restoration", false);
    writer.write_flag("sequence_filter.enable_ccso", false);
    if !single_picture_header {
        writer.write_flag("sequence_filter.enable_cdef_on_skip_txfm_always_on", false);
        writer.write_flag("sequence_filter.enable_cdef_on_skip_txfm_disabled", true);
    }
    writer.write_literal("sequence_filter.df_par_bits_minus2", 1, 2);

    writer.write_flag("sequence_tile_config.seq_tile_info_present_flag", false);
}
