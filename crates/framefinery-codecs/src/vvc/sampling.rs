impl VvcSampledFrame {
    fn solid(color: VvcSampledColor) -> Self {
        let geometry = VvcVideoGeometry {
            width: 8,
            height: 8,
        };
        let format = VvcPictureFormat {
            chroma_sampling: ChromaSampling::Cs420,
            bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
        };
        let layout = PlanarYuvGeometry::for_validated_shape(
            geometry.width,
            geometry.height,
            format.chroma_sampling,
            format.bit_depth,
        );
        Self {
            geometry,
            format,
            luma: vec![color.y; layout.luma_samples()],
            cb: vec![color.u; layout.chroma_samples()],
            cr: vec![color.v; layout.chroma_samples()],
            chroma_len: layout.chroma_samples(),
        }
    }

    fn sampled_color(&self) -> VvcSampledColor {
        VvcSampledColor {
            y: self.luma[0],
            u: self.cb[0],
            v: self.cr[0],
        }
    }
}

pub(in crate::vvc) fn vvc_neutral_sample(bit_depth: SampleBitDepth) -> VvcSample {
    1u16 << u32::from(bit_depth.bits() - 1)
}

pub(in crate::vvc) fn vvc_downshift_sample_to_u8(
    sample: VvcSample,
    bit_depth: SampleBitDepth,
) -> u8 {
    let bits = bit_depth.bits();
    if bits <= 8 {
        sample.min(u8::MAX as u16) as u8
    } else {
        (sample >> u32::from(bits - 8)).min(u8::MAX as u16) as u8
    }
}

fn vvc_bit_depth_is_supported(bit_depth: SampleBitDepth) -> bool {
    (VVC_MIN_BIT_DEPTH..=VVC_MAX_BIT_DEPTH).contains(&bit_depth.bits())
}

pub fn sample_vvc_first_yuv420p8(
    input: &[u8],
    params: VvcEncodeParams,
) -> Result<VvcSampledColor, String> {
    Ok(sample_vvc_yuv_frame(
        input,
        params,
        VvcVideoGeometry {
            width: 8,
            height: 8,
        },
        PixelFormat::Yuv420p8,
    )?
    .sampled_color())
}

fn sample_vvc_yuv_frame(
    input: &[u8],
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    format: PixelFormat,
) -> Result<VvcSampledFrame, String> {
    sample_vvc_yuv_frame_at(input, params, geometry, format, 0)
}

fn sample_vvc_yuv_frame_at(
    input: &[u8],
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    format: PixelFormat,
    frame_idx: usize,
) -> Result<VvcSampledFrame, String> {
    validate_vvc_exact_frame_count(params)?;
    if frame_idx >= params.frames {
        return Err(format!(
            "VVC input requested frame {frame_idx}, but stream has {} frame(s)",
            params.frames
        ));
    }
    geometry.validate_shape()?;
    let stream_format = Picture::validate_format_shape(
        geometry.width,
        geometry.height,
        format,
        validate_vvc_input_format,
    )?;
    let layout = PlanarYuvFrameLayout::new(
        geometry.width,
        geometry.height,
        stream_format.chroma_sampling,
        stream_format.bit_depth,
    )?;
    let frame_len = layout.frame_len();
    let expected_len = frame_len * params.frames;
    if input.len() != expected_len {
        return Err(format!(
            "VVC input size mismatch: got {} bytes, expected {} for {}x{} {format} with {} frame(s)",
            input.len(),
            expected_len,
            geometry.width,
            geometry.height,
            params.frames
        ));
    }
    let frame_base = frame_len * frame_idx;
    let frame = &input[frame_base..frame_base + frame_len];

    let luma_samples = layout.luma_samples();
    let mut luma = vec![0; luma_samples];

    let chroma_plane_samples = layout.chroma_samples();
    let mut cb = vec![0; chroma_plane_samples];
    let mut cr = vec![0; chroma_plane_samples];
    let (y_plane, cb_plane, cr_plane) = layout.plane_slices(frame);
    unpack_planar_samples(y_plane, &mut luma, stream_format.bit_depth);
    unpack_planar_samples(cb_plane, &mut cb, stream_format.bit_depth);
    unpack_planar_samples(cr_plane, &mut cr, stream_format.bit_depth);

    Ok(VvcSampledFrame {
        geometry,
        format: stream_format,
        luma,
        cb,
        cr,
        chroma_len: chroma_plane_samples,
    })
}

fn validate_vvc_exact_frame_count(params: VvcEncodeParams) -> Result<FrameLimit, String> {
    let frame_limit = FrameLimit::from_frame_count(params.frames);
    if matches!(frame_limit, FrameLimit::UntilEof) {
        return Err("VVC encode expects at least one frame".to_string());
    }
    Ok(frame_limit)
}

fn validate_vvc_input_format(format: PixelFormat) -> Result<VvcPictureFormat, String> {
    if format == PixelFormat::Gbrp8 {
        return Ok(VvcPictureFormat {
            chroma_sampling: ChromaSampling::Cs444,
            bit_depth: format.bit_depth(),
        });
    }
    let Some(chroma_sampling) = format.chroma_sampling() else {
        return Err(format!(
            "VVC input expects planar YUV or gbrp8 format; got {format}"
        ));
    };
    match chroma_sampling {
        ChromaSampling::Cs420 | ChromaSampling::Cs422 | ChromaSampling::Cs444
            if vvc_bit_depth_is_supported(format.bit_depth()) =>
        {
            Ok(VvcPictureFormat {
                chroma_sampling,
                bit_depth: format.bit_depth(),
            })
        }
        ChromaSampling::Cs420 => Err(format!(
            "VVC 4:2:0 input currently supports bit depths {VVC_MIN_BIT_DEPTH}..{VVC_MAX_BIT_DEPTH}; got {format}"
        )),
        ChromaSampling::Cs422 => Err(format!(
            "VVC 4:2:2 input currently supports bit depths {VVC_MIN_BIT_DEPTH}..{VVC_MAX_BIT_DEPTH}; got {format}"
        )),
        ChromaSampling::Cs444 => Err(format!(
            "VVC 4:4:4 input currently supports bit depths {VVC_MIN_BIT_DEPTH}..{VVC_MAX_BIT_DEPTH}; got {format}"
        )),
        ChromaSampling::Monochrome => Err(format!(
            "VVC monochrome input is not wired yet; got {format}"
        )),
    }
}

fn vvc_slice_config_for_input_format(
    slice_config: VvcSliceSyntaxConfig,
    format: PixelFormat,
) -> VvcSliceSyntaxConfig {
    if format == PixelFormat::Gbrp8 {
        slice_config.with_vui_signal(VvcVuiSignal::srgb_gbr_compatible())
    } else {
        slice_config
    }
}
