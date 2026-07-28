pub fn eos_annex_b() -> Vec<u8> {
    write_annex_b(&[VvcNalUnit::eos()]).expect("hard-coded EOS NAL should be valid")
}

pub fn vvc_black_yuv420p8_annex_b(params: VvcEncodeParams) -> Result<Vec<u8>, String> {
    validate_vvc_exact_frame_count(params)?;
    let input = vec![0; Picture::expected_len(8, 8, PixelFormat::Yuv420p8) * params.frames];
    vvc_yuv420p8_annex_b_from_input(&input, params)
}

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

pub fn vvc_yuv_encode_stream_with_limits<R: Read, W: Write>(
    input: &mut R,
    bitstream: &mut W,
    reconstruction: Option<&mut dyn Write>,
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
) -> Result<(), String> {
    vvc_yuv_encode_stream_with_limits_and_progress_and_frame_metrics(
        input,
        bitstream,
        reconstruction,
        params,
        geometry,
        limits,
        format,
        VvcEncodeOptions::default(),
        None,
        None,
    )
}

pub fn vvc_yuv_encode_stream_with_limits_and_progress<R: Read, W: Write>(
    input: &mut R,
    bitstream: &mut W,
    reconstruction: Option<&mut dyn Write>,
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
    progress: Option<&mut dyn FnMut(VvcEncodeProgress)>,
) -> Result<(), String> {
    vvc_yuv_encode_stream_with_limits_and_progress_and_frame_metrics(
        input,
        bitstream,
        reconstruction,
        params,
        geometry,
        limits,
        format,
        VvcEncodeOptions::default(),
        progress,
        None,
    )
}

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

pub fn vvc_yuv_encode_stream_with_limits_and_options_and_frame_metrics<R: Read, W: Write>(
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
    vvc_yuv_encode_stream_with_limits_and_progress_and_frame_metrics(
        input,
        bitstream,
        reconstruction,
        params,
        geometry,
        limits,
        format,
        options,
        None,
        frame_metrics,
    )
}

fn vvc_yuv_encode_stream_with_limits_and_progress_and_frame_metrics<R: Read, W: Write>(
    input: &mut R,
    bitstream: &mut W,
    mut reconstruction: Option<&mut dyn Write>,
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
    options: VvcEncodeOptions,
    mut progress: Option<&mut dyn FnMut(VvcEncodeProgress)>,
    mut frame_metrics: Option<&mut dyn for<'a> FnMut(VvcEncodeFrameMetrics<'a>)>,
) -> Result<(), String> {
    let request = VvcEncodeRequest {
        params,
        geometry,
        limits,
        format,
    }
    .validate()?;
    let geometry = request.geometry;
    let frame_limit = request.frame_limit;
    let stream_format = request.format;
    let stream_layout = PlanarYuvGeometry::new(
        geometry.width,
        geometry.height,
        stream_format.chroma_sampling,
        stream_format.bit_depth,
    )?;
    let frame_len = stream_layout.frame_len();
    let residual_mode = VvcResidualCodingMode::for_encode_options(options);
    let residual_policy =
        VvcResidualCodingPolicy::new(stream_format, residual_mode).with_fast_search(options.fast_search);
    let slice_config = vvc_slice_config_for_input_format(
        residual_mode.slice_config(stream_format, options.qp),
        format,
    );
    let luma_qp = slice_config.slice_qp;
    let chroma_qp = if residual_mode.is_lossless() {
        slice_config.slice_qp
    } else {
        vvc_lossy_chroma_qp_for_slice_qp(luma_qp)
    };
    let transform_skip_quant_tables =
        VvcTransformSkipQuantTables::new(format.bit_depth(), luma_qp, chroma_qp);
    let picture_partitioning = residual_mode.picture_partitioning();
    write_annex_b_to(
        bitstream,
        &[
            vvc_sps_unit(geometry, slice_config, stream_format.bit_depth),
            vvc_pps_unit_with_partitioning(geometry, picture_partitioning),
        ],
    )?;

    #[cfg(feature = "vvc-stats")]
    let mut vvc_stats = VvcStatsSink::from_env()?;
    #[cfg(feature = "vvc-stats")]
    let mut vvc_ctu_bits = VvcCtuBitSink::from_env()?;

    let mut frame_buf = vec![0; frame_len];
    let mut frame_idx = 0usize;
    while frame_limit.should_read(frame_idx) {
        #[cfg(feature = "vvc-stats")]
        let mut frame_stats = VvcFrameStats::new(
            frame_idx,
            geometry,
            stream_format,
            options.lossless,
            slice_config.slice_qp,
            chroma_qp,
        );
        #[cfg(feature = "vvc-stats")]
        {
            frame_stats.add_counter(
                "slice_count",
                match picture_partitioning {
                    VvcPicturePartitioning::SingleSlice => 1,
                    VvcPicturePartitioning::OneSlicePerCtu => vvc_picture_ctu_count(geometry),
                } as u64,
            );
            frame_stats.add_counter(
                "single_slice_frame",
                u64::from(picture_partitioning == VvcPicturePartitioning::SingleSlice),
            );
        }
        #[cfg(feature = "vvc-stats")]
        let stage_start = Instant::now();
        let frame_available =
            read_input_frame(input, &mut frame_buf, frame_idx, frame_limit, "VVC input")?;
        #[cfg(feature = "vvc-stats")]
        frame_stats.add_elapsed("read_frame", stage_start);
        if !frame_available {
            break;
        }
        if let Some(progress) = progress.as_deref_mut() {
            progress(VvcEncodeProgress {
                frame_idx,
                frame_count: frame_limit.metric_count(),
            });
        }
        #[cfg(feature = "vvc-stats")]
        let stage_start = Instant::now();
        let source_frame =
            sample_vvc_yuv_frame(&frame_buf, VvcEncodeParams { frames: 1 }, geometry, format)?;
        #[cfg(feature = "vvc-stats")]
        frame_stats.add_elapsed("sample_frame", stage_start);
        let (frame_recon_yuv, frame_bitstream_bytes) = {
            let mut frame_bitstream = CountingWriter::new(bitstream);
            let frame_recon_yuv = {
                let mut frame_recon =
                    VvcReconstructionFrame::new_neutral(geometry, source_frame.format);
                let mut frame_ctus = Vec::with_capacity(vvc_picture_ctu_count(geometry));
                let mut luma_mode_search_state = VvcLumaModeSearchState::new_for_geometry(geometry);
                for region in vvc_ctu_regions(geometry) {
                    #[cfg(feature = "vvc-stats")]
                    let stage_start = Instant::now();
                    let VvcQuantizedCtuLeafDecision {
                        quantized,
                        luma_max_leaf_size,
                    } = quantize_vvc_ctu_with_luma_leaf_selection(
                        &source_frame,
                        &mut frame_recon,
                        region,
                        residual_policy,
                        luma_qp,
                        chroma_qp,
                        &mut luma_mode_search_state,
                        &transform_skip_quant_tables,
                    );
                    #[cfg(feature = "vvc-stats")]
                    {
                        add_vvc_quantized_ctu_counters(&mut frame_stats, &quantized);
                        match luma_max_leaf_size {
                            VVC_LOSSLESS_LUMA_LEAF_SIZE => {
                                frame_stats.add_counter("luma_ctu_leaf4_count", 1);
                            }
                            VVC_CURRENT_MAX_LUMA_LEAF_SIZE => {
                                frame_stats.add_counter("luma_ctu_leaf8_count", 1);
                            }
                            _ => {}
                        }
                    }
                    #[cfg(feature = "vvc-stats")]
                    if vvc_ctu_bits.is_enabled() {
                        vvc_ctu_bits.write_ctu(
                            frame_idx,
                            geometry,
                            region,
                            stream_format,
                            options.lossless,
                            slice_config.slice_qp,
                            chroma_qp,
                            &quantized,
                            luma_max_leaf_size,
                            slice_config,
                        )?;
                    }
                    #[cfg(feature = "vvc-stats")]
                    frame_stats.add_elapsed("ctu_quantize", stage_start);
                    frame_ctus.push(VvcQuantizedCtu {
                        slice_address: region.slice_address,
                        geometry: region.geometry,
                        color: quantized,
                        luma_max_leaf_size,
                    });
                }
                #[cfg(feature = "vvc-stats")]
                let stage_start = Instant::now();
                #[cfg(feature = "vvc-stats")]
                let entropy_build_start = Instant::now();
                let frame_slice_unit =
                    vvc_frame_slice_unit(frame_idx, geometry, &frame_ctus, slice_config)?;
                #[cfg(feature = "vvc-stats")]
                frame_stats.add_counter(
                    "frame_entropy_build_nanos",
                    entropy_build_start.elapsed().as_nanos() as u64,
                );
                #[cfg(feature = "vvc-stats")]
                let annexb_write_start = Instant::now();
                write_annex_b_to(&mut frame_bitstream, &[frame_slice_unit])?;
                #[cfg(feature = "vvc-stats")]
                frame_stats.add_counter(
                    "frame_annexb_write_nanos",
                    annexb_write_start.elapsed().as_nanos() as u64,
                );
                #[cfg(feature = "vvc-stats")]
                frame_stats.add_elapsed("frame_entropy_write", stage_start);
                #[cfg(feature = "vvc-stats")]
                let stage_start = Instant::now();
                let yuv = frame_recon.into_yuv();
                #[cfg(feature = "vvc-stats")]
                frame_stats.add_elapsed("frame_recon_finalize", stage_start);
                yuv
            };
            (frame_recon_yuv, frame_bitstream.bytes_written())
        };
        #[cfg(feature = "vvc-stats")]
        frame_stats.set_bitstream_bytes(frame_bitstream_bytes);
        if let Some(writer) = reconstruction.as_deref_mut() {
            #[cfg(feature = "vvc-stats")]
            let stage_start = Instant::now();
            writer.write_all(&frame_recon_yuv).map_err(|err| {
                format!("failed to write VVC reconstruction frame {frame_idx}: {err}")
            })?;
            #[cfg(feature = "vvc-stats")]
            frame_stats.add_elapsed("write_reconstruction", stage_start);
        }
        if let Some(frame_metrics) = frame_metrics.as_deref_mut() {
            #[cfg(feature = "vvc-stats")]
            let stage_start = Instant::now();
            frame_metrics(VvcEncodeFrameMetrics {
                frame_idx,
                frame_count: frame_limit.metric_count(),
                bitstream_bytes: frame_bitstream_bytes,
                source: &frame_buf,
                reconstruction: &frame_recon_yuv,
            });
            #[cfg(feature = "vvc-stats")]
            frame_stats.add_elapsed("frame_metrics", stage_start);
        }
        #[cfg(feature = "vvc-stats")]
        vvc_stats.write_frame(&frame_stats)?;
        frame_idx += 1;
    }

    if let FrameLimit::Exact(frames) = frame_limit {
        let mut extra = [0; 1];
        match input.read(&mut extra) {
            Ok(0) => Ok(()),
            Ok(_) => Err(format!(
                "VVC input contains trailing bytes after {} frame(s)",
                frames
            )),
            Err(err) => Err(format!("failed to check VVC input length: {err}")),
        }
    } else {
        Ok(())
    }
}

fn write_annex_b_to<W: Write>(output: &mut W, units: &[VvcNalUnit]) -> Result<(), String> {
    let bytes = write_annex_b(units)?;
    output
        .write_all(&bytes)
        .map_err(|err| format!("failed to write VVC Annex-B stream: {err}"))
}
