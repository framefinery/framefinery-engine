#[cfg(test)]
pub fn eos_annex_b() -> Vec<u8> {
    write_annex_b(&[VvcNalUnit::eos()]).expect("hard-coded EOS NAL should be valid")
}

const VVC_PREDICTIVE_SINGLE_SLICE_PPS_ID: u8 = 1;
#[cfg(test)]
const VVC_PREDICTIVE_FRAME_SKIP_PPS_ID: u8 = VVC_PREDICTIVE_SINGLE_SLICE_PPS_ID;
const VVC_LOSSY_PREDICTIVE_SKIP_MAX_ABS_8BIT: u16 = 2;
// Production SCC uses the shared, required-reference-tested IBC path.
// Format, profile and mode eligibility remain in VvcStreamConfig.
const VVC_SCC_IBC_PRODUCTION_ENABLED: bool = true;

#[cfg(test)]
pub fn vvc_black_yuv420p8_annex_b(params: VvcEncodeParams) -> Result<Vec<u8>, String> {
    validate_vvc_exact_frame_count(params)?;
    let input = vec![0; Picture::expected_len(8, 8, PixelFormat::Yuv420p8) * params.frames];
    vvc_yuv420p8_annex_b_from_input(&input, params)
}

#[cfg(test)]
pub fn vvc_yuv420p8_annex_b_from_input(
    input: &[u8],
    params: VvcEncodeParams,
) -> Result<Vec<u8>, String> {
    vvc_yuv_annex_b_from_input(
        input,
        params,
        VvcVideoGeometry {
            width: 8,
            height: 8,
        },
        PixelFormat::Yuv420p8,
    )
}

#[cfg(test)]
pub fn vvc_yuv420p_annex_b_from_input(
    input: &[u8],
    params: VvcEncodeParams,
    format: PixelFormat,
) -> Result<Vec<u8>, String> {
    vvc_yuv_annex_b_from_input(
        input,
        params,
        VvcVideoGeometry {
            width: 8,
            height: 8,
        },
        format,
    )
}

#[cfg(test)]
pub fn vvc_default_yuv_annex_b_from_input(
    input: &[u8],
    params: VvcEncodeParams,
    format: PixelFormat,
) -> Result<Vec<u8>, String> {
    vvc_yuv_annex_b_from_input(
        input,
        params,
        VvcVideoGeometry {
            width: 8,
            height: 8,
        },
        format,
    )
}

#[cfg(test)]
pub fn vvc_yuv_annex_b_from_input(
    input: &[u8],
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    format: PixelFormat,
) -> Result<Vec<u8>, String> {
    vvc_yuv_annex_b_from_input_with_limits(
        input,
        params,
        geometry,
        VvcVideoLimits::unbounded(),
        format,
    )
}

#[cfg(test)]
pub fn vvc_yuv_annex_b_from_input_with_limits(
    input: &[u8],
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
) -> Result<Vec<u8>, String> {
    Ok(
        vvc_yuv_encode_artifacts_from_input_with_limits(input, params, geometry, limits, format)?
            .bitstream,
    )
}

#[cfg(test)]
pub fn vvc_yuv_encode_artifacts_from_input_with_limits(
    input: &[u8],
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
) -> Result<VvcEncodeArtifacts, String> {
    let mut reader = Cursor::new(input);
    let mut bitstream = Vec::new();
    let mut reconstruction = Vec::new();
    vvc_yuv_encode_stream_with_limits(
        &mut reader,
        &mut bitstream,
        Some(&mut reconstruction),
        params,
        geometry,
        limits,
        format,
    )?;
    Ok(VvcEncodeArtifacts {
        bitstream,
        reconstruction,
    })
}

#[cfg(test)]
pub fn vvc_yuv_encode_stream_with_limits<R: Read, W: Write>(
    input: &mut R,
    bitstream: &mut W,
    reconstruction: Option<&mut dyn Write>,
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
) -> Result<(), String> {
    vvc_yuv_encode_stream_with_limits_and_options_and_frame_metrics(
        input,
        bitstream,
        reconstruction,
        params,
        geometry,
        limits,
        format,
        VvcEncodeOptions::default(),
        None,
    )
}

#[cfg(test)]
pub fn vvc_yuv_encode_stream_with_limits_and_frame_metrics<R: Read, W: Write>(
    input: &mut R,
    bitstream: &mut W,
    reconstruction: Option<&mut dyn Write>,
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
    frame_metrics: Option<&mut dyn for<'a> FnMut(VvcEncodeFrameMetrics<'a>)>,
) -> Result<(), String> {
    vvc_yuv_encode_stream_with_limits_and_options_and_frame_metrics(
        input,
        bitstream,
        reconstruction,
        params,
        geometry,
        limits,
        format,
        VvcEncodeOptions::default(),
        frame_metrics,
    )
}

pub fn vvc_yuv_encode_stream_with_limits_and_options_and_frame_metrics<
    R: Read + ?Sized,
    W: Write + ?Sized,
>(
    input: &mut R,
    bitstream: &mut W,
    reconstruction: Option<&mut dyn Write>,
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
    options: VvcEncodeOptions,
    frame_metrics: Option<&mut dyn for<'a> FnMut(VvcEncodeFrameMetrics<'a>)>,
) -> Result<(), String> {
    let mut encoder = VvcStreamEncoder::new(
        VvcEncodeRequest {
            params,
            geometry,
            limits,
            format,
        },
        options,
    )?;
    let frame_limit = encoder.config.frame_limit;
    let mut frame_buf = vec![0; encoder.config.frame_len];
    // Keep the legacy ?Sized writer signature while sharing a dyn-Write sink.
    let mut bitstream = bitstream;
    let mut sinks = VvcFrameSinks {
        output: &mut bitstream,
        recon: reconstruction,
        frame_metrics,
    };
    while frame_limit.should_read(encoder.frame_index) {
        #[cfg(feature = "vvc-stats")]
        let mut frame_stats = encoder.frame_stats();
        #[cfg(feature = "vvc-stats")]
        let stage_start = StageStart::now();
        let frame_available = read_input_frame(
            input,
            &mut frame_buf,
            encoder.frame_index,
            frame_limit,
            "VVC input",
        )?;
        #[cfg(feature = "vvc-stats")]
        frame_stats.add_elapsed("read_frame", stage_start);
        if !frame_available {
            break;
        }
        encoder.encode_frame(
            &frame_buf,
            &mut sinks,
            #[cfg(feature = "vvc-stats")]
            frame_stats,
        )?;
    }
    if let FrameLimit::Exact(frames) = frame_limit {
        let mut extra = [0; 1];
        match input.read(&mut extra) {
            Ok(0) => {}
            Ok(_) => {
                return Err(format!(
                    "VVC input contains trailing bytes after {} frame(s)",
                    frames
                ))
            }
            Err(err) => return Err(format!("failed to check VVC input length: {err}")),
        }
    }
    encoder.finish()
}

fn vvc_intra_ctu_payload_from_decision(
    region: VvcCtuRegion,
    decision: &VvcQuantizedCtuLeafDecision,
    slice_config: VvcSliceSyntaxConfig,
    inter_slice_partition: bool,
) -> Result<VvcQuantizedCtuPayload, String> {
    let mut payload = vvc_intra_ctu_payload_from_quantized(
        region,
        decision.quantized.clone(),
        decision.luma_max_leaf_size,
        slice_config,
        inter_slice_partition,
    )?;
    if let VvcQuantizedCtuPayload::Intra(params) = &mut payload {
        params.luma_tu_inter_decisions = decision.luma_tu_inter_decisions;
    }
    Ok(payload)
}

fn vvc_intra_ctu_payload_from_quantized(
    region: VvcCtuRegion,
    quantized: VvcQuantizedColor,
    luma_max_leaf_size: u16,
    slice_config: VvcSliceSyntaxConfig,
    inter_slice_partition: bool,
) -> Result<VvcQuantizedCtuPayload, String> {
    let Some(params) = vvc_ctu_partition_params_with_luma_max_leaf_size_and_chroma_for_kind(
        region.geometry,
        quantized,
        luma_max_leaf_size,
        slice_config.coding_tree.chroma_sampling,
        slice_config.coding_tree.dual_tree_intra && !inter_slice_partition,
        if inter_slice_partition {
            VvcLumaSplitAvailabilityKind::Inter
        } else {
            VvcLumaSplitAvailabilityKind::Intra
        },
    ) else {
        return Err(format!(
            "VVC frame CABAC CTU {} has unsupported coded geometry {}x{}",
            region.slice_address,
            region.geometry.coded_width(),
            region.geometry.coded_height()
        ));
    };
    Ok(VvcQuantizedCtuPayload::Intra(Box::new(params)))
}
