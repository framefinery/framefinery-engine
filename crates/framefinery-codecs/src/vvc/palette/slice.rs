#[cfg(test)]
pub(super) fn vvc_palette_444_ctu_slice_unit(
    frame_idx: usize,
    picture_geometry: VvcVideoGeometry,
    slice_address: usize,
    frame: &VvcSampledFrame,
    slice_config: VvcSliceSyntaxConfig,
) -> Result<VvcNalUnit, String> {
    let picture_kind = VvcPictureKind::for_frame_idx(frame_idx);
    let poc_lsb = vvc_poc_lsb_for_frame_idx(frame_idx);
    let slice_count = vvc_picture_ctu_count(picture_geometry);
    if slice_address >= slice_count {
        return Err(format!(
            "VVC palette slice address {slice_address} is outside the picture CTU/slice count {slice_count}"
        ));
    }

    Ok(VvcNalUnit {
        nal_unit_type: picture_kind.nal_unit_type(),
        layer_id: 0,
        temporal_id: 0,
        rbsp_payload: vvc_palette_444_slice_payload(
            picture_kind,
            poc_lsb,
            picture_geometry,
            slice_address,
            frame,
            slice_config,
        ),
    })
}

#[cfg(test)]
fn vvc_palette_444_slice_payload(
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    picture_geometry: VvcVideoGeometry,
    slice_address: usize,
    frame: &VvcSampledFrame,
    slice_config: VvcSliceSyntaxConfig,
) -> Vec<u8> {
    let mut writer = VvcSyntaxWriter::new();
    let tool_flags = slice_config.tools;
    let slice_count = vvc_picture_ctu_count(picture_geometry);
    let include_picture_header = slice_count == 1;
    writer.write_flag(
        "sh_picture_header_in_slice_header_flag",
        include_picture_header,
    );
    if include_picture_header {
        super::header::write_vvc_picture_header(&mut writer, picture_kind, poc_lsb, slice_config);
    }
    if slice_count > 1 {
        writer.write_u(
            "sh_slice_address",
            slice_address as u64,
            vvc_slice_address_bits(picture_geometry),
        );
    }
    writer.write_flag("sh_no_output_of_prior_pics_flag", false);
    super::header::write_vvc_slice_header_ref_pic_lists(&mut writer, picture_kind);
    writer.write_se(
        "sh_qp_delta",
        slice_config.slice_qp - super::header::VVC_PPS_INIT_QP,
    );
    if tool_flags.dependent_quantization_enabled {
        writer.write_flag("sh_dep_quant_used_flag", true);
    }
    if tool_flags.sign_data_hiding_enabled && !tool_flags.dependent_quantization_enabled {
        writer.write_flag("sh_sign_data_hiding_used_flag", true);
    }
    if tool_flags.transform_skip_enabled
        && !tool_flags.dependent_quantization_enabled
        && !tool_flags.sign_data_hiding_enabled
    {
        // H.266 7.3.7.1: when this flag is 1, transform-skipped TUs still
        // use residual_coding() rather than residual_codingTS(). The first
        // 4:4:4 residual subset uses transform skip for reconstruction while
        // deliberately reusing the existing regular residual CABAC path.
        writer.write_flag("sh_ts_residual_coding_disabled_flag", true);
    }
    super::header::write_vvc_slice_header_byte_alignment(&mut writer);
    write_vvc_palette_444_entropy(&mut writer, frame, slice_config);
    writer.rbsp_trailing_bits();
    debug_assert!(writer.is_byte_aligned());
    writer.into_bytes()
}

#[cfg(test)]
fn write_vvc_palette_444_entropy(
    writer: &mut VvcSyntaxWriter,
    frame: &VvcSampledFrame,
    slice_config: VvcSliceSyntaxConfig,
) {
    writer.write_cabac_bits(
        "cabac_vvc_palette_444_tile_entry_bits",
        &vvc_palette_444_cabac_bits(frame, slice_config),
    );
}

#[cfg(test)]
fn vvc_palette_444_cabac_bits(
    frame: &VvcSampledFrame,
    slice_config: VvcSliceSyntaxConfig,
) -> Vec<bool> {
    vvc_palette_444_cabac_encoder(frame, slice_config).finish()
}

#[cfg(test)]
fn vvc_palette_444_cabac_encoder(
    frame: &VvcSampledFrame,
    slice_config: VvcSliceSyntaxConfig,
) -> VvcCabacEncoder {
    vvc_palette_444_cabac_encoder_with_dump_recording(frame, false, slice_config)
}

fn vvc_palette_444_cabac_encoder_with_dump(frame: &VvcSampledFrame) -> VvcCabacEncoder {
    vvc_palette_444_cabac_encoder_with_dump_recording(
        frame,
        true,
        VvcSliceSyntaxConfig::palette_444(),
    )
}

fn vvc_palette_444_cabac_encoder_with_dump_recording(
    frame: &VvcSampledFrame,
    record_dump: bool,
    slice_config: VvcSliceSyntaxConfig,
) -> VvcCabacEncoder {
    let mut cabac = VvcCabacEncoder::new();
    if record_dump {
        cabac = VvcCabacEncoder::new_with_dump();
    }
    let mut ctx = VvcCabacContexts::with_slice_qp(slice_config.slice_qp);
    let mut predictor_mode = VvcPalettePredictorMode::SignalNewEntry;
    let mut ibc_search = VvcIbcHashSearch::new();
    cabac.start();
    let partition_shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: frame.geometry.coded_width() as u16,
        visible_height: frame.geometry.coded_height() as u16,
        chroma_sampling: frame.format.chroma_sampling,
        dual_tree_intra: false,
    };
    for op in VvcCtuCabacOp::intra_ctu_partition(partition_shape, VVC_PALETTE_CU_SIZE) {
        append_vvc_palette_444_partition_op(
            &mut cabac,
            &mut ctx,
            frame,
            slice_config,
            &mut predictor_mode,
            &mut ibc_search,
            op,
        );
    }
    cabac.encode_bin_trm(true);
    cabac
}

#[cfg(test)]
pub(super) fn vvc_palette_444_cabac_context_bins(frame: &VvcSampledFrame) -> Vec<(u16, bool)> {
    vvc_palette_444_cabac_encoder_with_dump(frame)
        .context_events
        .into_iter()
        .map(|event| (event.ctx_id, event.bin))
        .collect()
}

fn append_vvc_palette_444_partition_op(
    cabac: &mut VvcCabacEncoder,
    ctx: &mut VvcCabacContexts,
    frame: &VvcSampledFrame,
    slice_config: VvcSliceSyntaxConfig,
    predictor_mode: &mut VvcPalettePredictorMode,
    ibc_search: &mut VvcIbcHashSearch,
    op: VvcCtuCabacOp,
) {
    match op {
        VvcCtuCabacOp::QtSplit {
            split_ctx,
            write_split_flag,
            write_qt_flag,
            qt_ctx,
            ..
        } => {
            // H.266 7.3.11.4 / 7.4.12.4: split_cu_flag and split_qt_flag
            // are only written when the split availability model has more
            // than one legal outcome. Boundary-only QT splits are inferred by
            // the decoder and must not consume CABAC bins.
            if write_split_flag {
                ctx.encode(cabac, VvcCabacContext::SplitFlag(split_ctx), true);
            }
            if write_qt_flag {
                ctx.encode(cabac, VvcCabacContext::SplitQtFlag(qt_ctx), true);
            }
        }
        VvcCtuCabacOp::BtSplit {
            vertical,
            split_ctx,
            write_split_flag,
            write_qt_flag,
            qt_ctx,
            write_mtt_vertical_flag,
            mtt_vertical_ctx,
            write_binary_flag,
            mtt_binary_ctx,
            mtt_binary_value,
            ..
        } => {
            // The palette path uses the same CTU split availability and
            // context derivation as the audited residual path. Only the CU
            // payload below the leaf differs.
            if write_split_flag {
                ctx.encode(cabac, VvcCabacContext::SplitFlag(split_ctx), true);
            }
            if write_qt_flag {
                ctx.encode(cabac, VvcCabacContext::SplitQtFlag(qt_ctx), false);
            }
            if write_mtt_vertical_flag {
                ctx.encode(
                    cabac,
                    VvcCabacContext::MttSplitCuVerticalFlag(mtt_vertical_ctx),
                    vertical,
                );
            }
            if write_binary_flag {
                ctx.encode(
                    cabac,
                    VvcCabacContext::MttSplitCuBinaryFlag(mtt_binary_ctx),
                    mtt_binary_value,
                );
            }
        }
        VvcCtuCabacOp::LumaLeafWithSplitCtx {
            node,
            write_split_flag,
            split_ctx,
        } => {
            if append_vvc_palette_444_8x8_cu_with_events(
                cabac,
                ctx,
                frame,
                slice_config,
                ibc_search,
                VvcPaletteCuEmitRequest {
                    origin_x: node.x,
                    origin_y: node.y,
                    write_split_flag,
                    split_ctx,
                    predictor_mode: *predictor_mode,
                },
            ) {
                *predictor_mode = VvcPalettePredictorMode::SignalNewEntryAfterPredictor;
            }
        }
        VvcCtuCabacOp::ChromaTree { .. } => {
            unreachable!("4:4:4 single-tree partitioning must not emit a chroma tree")
        }
    }
}
