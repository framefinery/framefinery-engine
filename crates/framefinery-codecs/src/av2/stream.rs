// One persistent AV2 stream state, shared by source and owned-frame adapters.
// Coding decisions and reconstruction remain in the same per-frame operation.
struct Av2StreamEncoder {
    request: Av2EncodeRequest,
    options: Av2EncodeOptions,
    visible_geometry: Av2VideoGeometry,
    coded_geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
    source_expected_len: usize,
    coded_expected_len: usize,
    predictive_headers_written: bool,
    predictive_reference: Option<Vec<u8>>,
    predictive_reconstruction: Option<Vec<u8>>,
    frame_index: usize,
    total_bitstream_bytes: usize,
    av2_stats: stats::Av2StatsSink,
    status: Av2StreamStatus,
}

enum Av2StreamStatus {
    Active,
    Finished,
    Failed(String),
}

struct Av2FrameSinks<'output, 'recon, 'metrics> {
    output: &'output mut dyn Write,
    recon: Option<&'recon mut dyn Write>,
    frame_metrics: Option<&'metrics mut dyn for<'frame> FnMut(Av2EncodeFrameMetrics<'frame>)>,
}

struct Av2EncodedFrame {
    frame_index: usize,
    keyframe: bool,
}

impl Av2StreamEncoder {
    fn new(request: Av2EncodeRequest, options: Av2EncodeOptions) -> Result<Self, String> {
        let visible_geometry = validate_mvp_request(request)?;
        let coded_geometry = visible_geometry.coded();
        let stream_format = Av2StreamFormat::from_pixel_format(request.format)
            .expect("validate_mvp_request accepts supported AV2 stream formats");
        if !options.lossless
            && stream_format.chroma_format == Av2ChromaFormat::Yuv422
            && options.qp.is_none()
        {
            return Err(format!(
                "AV2 non-lossless encode is not implemented for {}; pass --set qp=<1..255> to use the experimental lossy residual path",
                request.format
            ));
        }
        if !options.lossless
            && options.gop.is_predictive()
            && options.qp.is_none()
            && stream_format.chroma_format != Av2ChromaFormat::Yuv420
        {
            return Err(format!(
                "AV2 predictive GOP encode for {} requires --set qp=<1..255> to use the lossy residual path; use --set gop=0 for the intra-only legacy path",
                request.format
            ));
        }
        Ok(Self {
            request,
            options,
            visible_geometry,
            coded_geometry,
            stream_format,
            source_expected_len: Picture::expected_len(
                visible_geometry.width,
                visible_geometry.height,
                request.format,
            ),
            coded_expected_len: Picture::expected_len(
                coded_geometry.width,
                coded_geometry.height,
                stream_format.pixel_format(),
            ),
            predictive_headers_written: false,
            predictive_reference: None,
            predictive_reconstruction: None,
            frame_index: 0,
            total_bitstream_bytes: 0,
            av2_stats: stats::Av2StatsSink::from_env()?,
            status: Av2StreamStatus::Active,
        })
    }

    fn ensure_active(&self) -> Result<(), String> {
        match &self.status {
            Av2StreamStatus::Active => Ok(()),
            Av2StreamStatus::Finished => Err("AV2 encoder is already finalized".into()),
            Av2StreamStatus::Failed(error) => Err(error.clone()),
        }
    }

    fn fail(&mut self, error: String) {
        self.predictive_reference = None;
        self.predictive_reconstruction = None;
        self.status = Av2StreamStatus::Failed(error);
    }

    fn finish(&mut self) -> Result<(), String> {
        if let Av2StreamStatus::Failed(error) = &self.status {
            return Err(error.clone());
        }
        self.predictive_reference = None;
        self.predictive_reconstruction = None;
        self.status = Av2StreamStatus::Finished;
        Ok(())
    }

    fn frame_stats(&self) -> stats::Av2FrameStats {
        stats::Av2FrameStats::new(
            self.frame_index,
            self.visible_geometry,
            self.request.format,
            self.stream_format,
            self.options.lossless,
            self.options.qp,
            self.options.gop.as_i32(),
        )
    }

    fn encode_frame(
        &mut self,
        source_frame: &[u8],
        sinks: &mut Av2FrameSinks<'_, '_, '_>,
        frame_stats: stats::Av2FrameStats,
    ) -> Result<Av2EncodedFrame, String> {
        self.ensure_active()?;
        if source_frame.len() != self.source_expected_len {
            return Err(format!(
                "AV2 frame length mismatch: expected {}, got {}",
                self.source_expected_len,
                source_frame.len()
            ));
        }
        let result = self.encode_frame_inner(source_frame, sinks, frame_stats);
        if let Err(error) = &result {
            self.fail(error.clone());
        }
        result
    }

    fn encode_frame_inner(
        &mut self,
        source_frame: &[u8],
        sinks: &mut Av2FrameSinks<'_, '_, '_>,
        mut frame_stats: stats::Av2FrameStats,
    ) -> Result<Av2EncodedFrame, String> {
        let request = self.request;
        let options = self.options;
        let visible_geometry = self.visible_geometry;
        let coded_geometry = self.coded_geometry;
        let stream_format = self.stream_format;
        let coded_expected_len = self.coded_expected_len;
        let rgb_identity = request.format.is_rgb();
        let packed_rgb_identity = request.format == PixelFormat::Rgb24;
        let frame_index = self.frame_index;
        let frame_limit = FrameLimit::from_frame_count(request.params.frames);
        let mut keyframe = true;
        #[cfg(feature = "av2-sb-bit-profile")]
        sb_bits::set_current_frame(frame_index);
        let frame_encode_start = crate::timing::StageStart::now();
        if options.gop.resets_references_before(frame_index) {
            self.predictive_reference = None;
            self.predictive_reconstruction = None;
        }
        let predictive_enabled = options.gop.is_predictive();
        let predictive_frame = options.gop.is_predictive_frame(frame_index);
        let planar_rgb_frame: Vec<u8>;
        let padded_frame: Vec<u8>;
        let frame = if packed_rgb_identity {
            let stage_start = stats::Av2StageStart::now();
            planar_rgb_frame = rgb24_to_planar_gbr(source_frame, visible_geometry);
            frame_stats.add_elapsed("rgb24_to_planar_gbr", stage_start);
            if coded_geometry != visible_geometry {
                let stage_start = stats::Av2StageStart::now();
                padded_frame = pad_av2_frame_to_geometry(
                    &planar_rgb_frame,
                    visible_geometry,
                    coded_geometry,
                    stream_format.pixel_format(),
                );
                frame_stats.add_elapsed("pad_to_coded_geometry", stage_start);
                padded_frame.as_slice()
            } else {
                planar_rgb_frame.as_slice()
            }
        } else if coded_geometry != visible_geometry {
            let stage_start = stats::Av2StageStart::now();
            padded_frame = pad_av2_frame_to_geometry(
                source_frame,
                visible_geometry,
                coded_geometry,
                stream_format.pixel_format(),
            );
            frame_stats.add_elapsed("pad_to_coded_geometry", stage_start);
            padded_frame.as_slice()
        } else {
            source_frame
        };
        debug_assert_eq!(frame.len(), coded_expected_len);
        // Preserve sequence/reference state for predictive coding. Intra-only
        // picture syntax remains selected by the existing GOP policy.
        if options.lossless
            && matches!(
                stream_format.chroma_format,
                Av2ChromaFormat::Yuv420 | Av2ChromaFormat::Yuv422 | Av2ChromaFormat::Yuv444
            )
        {
            let (bitstream, reconstruction) = if predictive_enabled {
                let order_hint = av2_order_hint_for_frame(frame_index);
                if predictive_frame && self.predictive_reference.as_deref() == Some(frame) {
                    let stage_start = stats::Av2StageStart::now();
                    keyframe = false;
                    let result = av2_lossless_regular_sef_frame(frame, order_hint);
                    frame_stats.add_elapsed("lossless_show_existing_frame", stage_start);
                    result
                } else if predictive_frame {
                    if let Some((bitstream, reconstruction)) =
                        self.predictive_reference.as_deref().and_then(|reference| {
                            let stage_start = stats::Av2StageStart::now();
                            let result = av2_lossless_regular_inter_tiles_frame(
                                coded_geometry,
                                stream_format,
                                frame,
                                reference,
                                order_hint,
                            );
                            frame_stats.add_elapsed("lossless_inter_tiles", stage_start);
                            result
                        })
                    {
                        keyframe = false;
                        self.predictive_reference = Some(frame.to_vec());
                        (bitstream, reconstruction)
                    } else {
                        let result =
                            av2_lossless_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                                coded_geometry,
                                visible_geometry,
                                stream_format,
                                frame,
                                !self.predictive_headers_written,
                                order_hint,
                                rgb_identity,
                                &mut frame_stats,
                            );
                        self.predictive_headers_written = true;
                        self.predictive_reference = Some(frame.to_vec());
                        result
                    }
                } else {
                    let result =
                        av2_lossless_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                            coded_geometry,
                            visible_geometry,
                            stream_format,
                            frame,
                            !self.predictive_headers_written,
                            order_hint,
                            rgb_identity,
                            &mut frame_stats,
                        );
                    self.predictive_headers_written = true;
                    self.predictive_reference = Some(frame.to_vec());
                    result
                }
            } else {
                av2_lossless_subsampled_bitstream_and_reconstruction_for_frame(
                    coded_geometry,
                    visible_geometry,
                    stream_format,
                    frame,
                    rgb_identity,
                    &mut frame_stats,
                )
            };
            frame_stats.set_bitstream_bytes(bitstream.len());
            let stage_start = stats::Av2StageStart::now();
            sinks
                .output
                .write_all(&bitstream)
                .map_err(|err| format!("failed to write AV2 bitstream: {err}"))?;
            self.total_bitstream_bytes += bitstream.len();
            frame_stats.add_elapsed("bitstream_write", stage_start);
            let stage_start = stats::Av2StageStart::now();
            let public_reconstruction = av2_public_reconstruction(
                &reconstruction,
                coded_geometry,
                visible_geometry,
                stream_format.pixel_format(),
                packed_rgb_identity,
            );
            frame_stats.add_elapsed("public_reconstruction", stage_start);
            let reconstruction = public_reconstruction.as_ref();
            if let Some(recon) = sinks.recon.as_deref_mut() {
                let stage_start = stats::Av2StageStart::now();
                recon
                    .write_all(reconstruction)
                    .map_err(|err| format!("failed to write AV2 reconstruction: {err}"))?;
                frame_stats.add_elapsed("write_reconstruction", stage_start);
            }
            if let Some(frame_metrics) = sinks.frame_metrics.as_deref_mut() {
                let stage_start = stats::Av2StageStart::now();
                frame_metrics(Av2EncodeFrameMetrics {
                    frame_idx: frame_index,
                    frame_count: frame_limit.metric_count(),
                    bitstream_bytes: bitstream.len(),
                    total_bitstream_bytes: self.total_bitstream_bytes,
                    encode_elapsed: frame_encode_start.elapsed(),
                    source: source_frame,
                    reconstruction,
                });
                frame_stats.add_elapsed("frame_metrics", stage_start);
            }
            self.av2_stats.write_frame(&frame_stats)?;
            self.frame_index += 1;
            return Ok(Av2EncodedFrame {
                frame_index,
                keyframe,
            });
        }
        let use_lossy_residual_path =
            options.qp.is_some() || stream_format.chroma_format == Av2ChromaFormat::Yuv420;
        if use_lossy_residual_path {
            let qp = options.qp.unwrap_or(AV2_LOSSY_DEFAULT_QP);
            let (bitstream, reconstruction) = if predictive_enabled {
                let order_hint = av2_order_hint_for_frame(frame_index);
                if predictive_frame && self.predictive_reference.as_deref() == Some(frame) {
                    if let Some(reference_reconstruction) =
                        self.predictive_reconstruction.as_deref()
                    {
                        let stage_start = stats::Av2StageStart::now();
                        keyframe = false;
                        let result =
                            av2_lossy_regular_sef_frame(reference_reconstruction, order_hint);
                        frame_stats.add_elapsed("lossy_show_existing_frame", stage_start);
                        result
                    } else {
                        av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                            coded_geometry,
                            visible_geometry,
                            stream_format,
                            frame,
                            qp,
                            !self.predictive_headers_written,
                            order_hint,
                            rgb_identity,
                            &mut frame_stats,
                        )
                    }
                } else if predictive_frame {
                    if let (Some(reference), Some(reference_reconstruction)) = (
                        self.predictive_reference.as_deref(),
                        self.predictive_reconstruction.as_deref(),
                    ) {
                        let stage_start = stats::Av2StageStart::now();
                        let inter_result = av2_lossy_zero_mv_inter_tiles_frame(
                            coded_geometry,
                            stream_format,
                            frame,
                            reference,
                            reference_reconstruction,
                            qp,
                            order_hint,
                        );
                        frame_stats.add_elapsed("lossy_zero_mv_inter_tiles", stage_start);
                        keyframe = inter_result.is_none();
                        inter_result.unwrap_or_else(|| {
                            av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                                coded_geometry,
                                visible_geometry,
                                stream_format,
                                frame,
                                qp,
                                !self.predictive_headers_written,
                                order_hint,
                                rgb_identity,
                                &mut frame_stats,
                            )
                        })
                    } else {
                        av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                            coded_geometry,
                            visible_geometry,
                            stream_format,
                            frame,
                            qp,
                            !self.predictive_headers_written,
                            order_hint,
                            rgb_identity,
                            &mut frame_stats,
                        )
                    }
                } else {
                    av2_lossy_subsampled_predictive_key_bitstream_and_reconstruction_for_frame(
                        coded_geometry,
                        visible_geometry,
                        stream_format,
                        frame,
                        qp,
                        !self.predictive_headers_written,
                        order_hint,
                        rgb_identity,
                        &mut frame_stats,
                    )
                }
            } else {
                av2_lossy_subsampled_bitstream_and_reconstruction_for_frame(
                    coded_geometry,
                    visible_geometry,
                    stream_format,
                    frame,
                    qp,
                    rgb_identity,
                    &mut frame_stats,
                )
            };
            if predictive_enabled {
                self.predictive_headers_written = true;
                self.predictive_reference = Some(frame.to_vec());
                self.predictive_reconstruction = Some(reconstruction.clone());
            }
            frame_stats.set_bitstream_bytes(bitstream.len());
            let stage_start = stats::Av2StageStart::now();
            sinks
                .output
                .write_all(&bitstream)
                .map_err(|err| format!("failed to write AV2 bitstream: {err}"))?;
            self.total_bitstream_bytes += bitstream.len();
            frame_stats.add_elapsed("bitstream_write", stage_start);
            let stage_start = stats::Av2StageStart::now();
            let public_reconstruction = av2_public_reconstruction(
                &reconstruction,
                coded_geometry,
                visible_geometry,
                stream_format.pixel_format(),
                packed_rgb_identity,
            );
            frame_stats.add_elapsed("public_reconstruction", stage_start);
            let reconstruction = public_reconstruction.as_ref();
            if let Some(recon) = sinks.recon.as_deref_mut() {
                let stage_start = stats::Av2StageStart::now();
                recon
                    .write_all(reconstruction)
                    .map_err(|err| format!("failed to write AV2 reconstruction: {err}"))?;
                frame_stats.add_elapsed("write_reconstruction", stage_start);
            }
            if let Some(frame_metrics) = sinks.frame_metrics.as_deref_mut() {
                let stage_start = stats::Av2StageStart::now();
                frame_metrics(Av2EncodeFrameMetrics {
                    frame_idx: frame_index,
                    frame_count: frame_limit.metric_count(),
                    bitstream_bytes: bitstream.len(),
                    total_bitstream_bytes: self.total_bitstream_bytes,
                    encode_elapsed: frame_encode_start.elapsed(),
                    source: source_frame,
                    reconstruction,
                });
                frame_stats.add_elapsed("frame_metrics", stage_start);
            }
            self.av2_stats.write_frame(&frame_stats)?;
            self.frame_index += 1;
            return Ok(Av2EncodedFrame {
                frame_index,
                keyframe,
            });
        }

        let stage_start = stats::Av2StageStart::now();
        let frame_mode =
            Av2Mvp444FrameMode::from_frame(frame, coded_geometry, stream_format.bit_depth)?;
        frame_stats.add_elapsed("mvp_444_mode_decision", stage_start);

        let stage_start = stats::Av2StageStart::now();
        let bitstream = av2_mvp_444_bitstream_for_mode(
            coded_geometry,
            visible_geometry,
            stream_format.bit_depth,
            &frame_mode,
            rgb_identity,
        );
        frame_stats.add_elapsed("mvp_444_bitstream", stage_start);
        let stage_start = stats::Av2StageStart::now();
        let reconstruction = frame_mode.reconstruction(coded_geometry, stream_format.bit_depth);
        frame_stats.add_elapsed("mvp_444_reconstruction", stage_start);
        frame_stats.set_bitstream_bytes(bitstream.len());
        let stage_start = stats::Av2StageStart::now();
        sinks
            .output
            .write_all(&bitstream)
            .map_err(|err| format!("failed to write AV2 bitstream: {err}"))?;
        self.total_bitstream_bytes += bitstream.len();
        frame_stats.add_elapsed("bitstream_write", stage_start);
        let stage_start = stats::Av2StageStart::now();
        let public_reconstruction = av2_public_reconstruction(
            &reconstruction,
            coded_geometry,
            visible_geometry,
            stream_format.pixel_format(),
            packed_rgb_identity,
        );
        frame_stats.add_elapsed("public_reconstruction", stage_start);
        let reconstruction = public_reconstruction.as_ref();
        if let Some(recon) = sinks.recon.as_deref_mut() {
            let stage_start = stats::Av2StageStart::now();
            recon
                .write_all(reconstruction)
                .map_err(|err| format!("failed to write AV2 reconstruction: {err}"))?;
            frame_stats.add_elapsed("write_reconstruction", stage_start);
        }
        if let Some(frame_metrics) = sinks.frame_metrics.as_deref_mut() {
            let stage_start = stats::Av2StageStart::now();
            frame_metrics(Av2EncodeFrameMetrics {
                frame_idx: frame_index,
                frame_count: frame_limit.metric_count(),
                bitstream_bytes: bitstream.len(),
                total_bitstream_bytes: self.total_bitstream_bytes,
                encode_elapsed: frame_encode_start.elapsed(),
                source: source_frame,
                reconstruction,
            });
            frame_stats.add_elapsed("frame_metrics", stage_start);
        }
        self.av2_stats.write_frame(&frame_stats)?;
        self.frame_index += 1;
        Ok(Av2EncodedFrame {
            frame_index,
            keyframe,
        })
    }
}
