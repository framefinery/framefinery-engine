pub fn av2_black_64x64_444_reconstruction() -> Vec<u8> {
    av2_black_444_reconstruction_for_geometry(Av2VideoGeometry {
        width: 64,
        height: 64,
    })
}

pub fn av2_black_444_reconstruction(geometry: Av2VideoGeometry) -> Option<Vec<u8>> {
    geometry
        .validate_shape()
        .ok()
        .map(|()| av2_black_444_reconstruction_for_geometry(geometry))
}

fn av2_black_444_reconstruction_for_geometry(geometry: Av2VideoGeometry) -> Vec<u8> {
    av2_black_444_reconstruction_for_geometry_with_depth(
        geometry,
        SampleBitDepth::new(8).expect("8-bit depth is supported"),
    )
}

fn av2_black_444_reconstruction_for_geometry_with_depth(
    geometry: Av2VideoGeometry,
    bit_depth: SampleBitDepth,
) -> Vec<u8> {
    av2_black_reconstruction_for_geometry(
        geometry,
        Av2StreamFormat {
            chroma_format: Av2ChromaFormat::Yuv444,
            bit_depth,
        },
    )
}

fn av2_black_reconstruction_for_geometry(
    geometry: Av2VideoGeometry,
    stream_format: Av2StreamFormat,
) -> Vec<u8> {
    vec![
        0;
        Picture::expected_len(
            geometry.width,
            geometry.height,
            stream_format.pixel_format(),
        )
    ]
}

fn validate_fixed_black_444_request(request: Av2EncodeRequest) -> Result<Av2VideoGeometry, String> {
    let geometry = validate_mvp_444_request(request)?;
    Ok(geometry)
}

fn validate_mvp_444_request(request: Av2EncodeRequest) -> Result<Av2VideoGeometry, String> {
    let geometry = validate_mvp_request(request)?;
    if !matches!(
        Av2StreamFormat::from_pixel_format(request.format),
        Some(Av2StreamFormat {
            chroma_format: Av2ChromaFormat::Yuv444,
            ..
        })
    ) {
        return Err(
            "AV2 4:4:4 MVP path only supports yuv444p8, yuv444p10le, gbrp8, or rgb24".to_string(),
        );
    }
    Ok(geometry)
}

fn validate_mvp_request(request: Av2EncodeRequest) -> Result<Av2VideoGeometry, String> {
    request.validate()?;
    Ok(request.geometry)
}

fn validate_av2_input_format(format: PixelFormat) -> Result<Av2StreamFormat, String> {
    if !format.is_yuv() && !format.is_rgb() {
        return Err(format!(
            "AV2 input expects planar YUV, gbrp8, or rgb24 format; got {format}"
        ));
    }
    Av2StreamFormat::from_pixel_format(format).ok_or_else(|| {
        format!(
            "AV2 MVP encoder only supports yuv420p8/10, yuv422p8/10, yuv444p8/10, gbrp8, and rgb24 streams; got {format}"
        )
    })
}
