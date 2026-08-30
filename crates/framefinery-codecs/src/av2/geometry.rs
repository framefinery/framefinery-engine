fn pad_av2_frame_to_geometry(
    frame: &[u8],
    visible_geometry: Av2VideoGeometry,
    coded_geometry: Av2VideoGeometry,
    format: PixelFormat,
) -> Vec<u8> {
    debug_assert_eq!(visible_geometry.coded(), coded_geometry);
    debug_assert_eq!(
        frame.len(),
        Picture::expected_len(visible_geometry.width, visible_geometry.height, format)
    );
    if visible_geometry == coded_geometry {
        return frame.to_vec();
    }
    match format {
        PixelFormat::PlanarYuv {
            chroma_sampling,
            bit_depth,
        } => {
            debug_assert_ne!(chroma_sampling, ChromaSampling::Monochrome);
            pad_planar_av2_frame_to_geometry(
                frame,
                visible_geometry,
                coded_geometry,
                chroma_sampling,
                bit_depth,
            )
        }
        PixelFormat::Gbrp8 => {
            pad_three_plane_av2_frame_to_geometry(frame, visible_geometry, coded_geometry, 1)
        }
        PixelFormat::Rgb24 => {
            pad_single_plane_av2_frame_to_geometry(frame, visible_geometry, coded_geometry, 3)
        }
        PixelFormat::Gray { .. } => unreachable!("AV2 does not accept monochrome input"),
    }
}

fn crop_av2_frame_to_geometry(
    frame: &[u8],
    coded_geometry: Av2VideoGeometry,
    visible_geometry: Av2VideoGeometry,
    format: PixelFormat,
) -> Vec<u8> {
    debug_assert_eq!(visible_geometry.coded(), coded_geometry);
    debug_assert_eq!(
        frame.len(),
        Picture::expected_len(coded_geometry.width, coded_geometry.height, format)
    );
    if visible_geometry == coded_geometry {
        return frame.to_vec();
    }
    match format {
        PixelFormat::PlanarYuv {
            chroma_sampling,
            bit_depth,
        } => {
            debug_assert_ne!(chroma_sampling, ChromaSampling::Monochrome);
            crop_planar_av2_frame_to_geometry(
                frame,
                coded_geometry,
                visible_geometry,
                chroma_sampling,
                bit_depth,
            )
        }
        PixelFormat::Gbrp8 => {
            crop_three_plane_av2_frame_to_geometry(frame, coded_geometry, visible_geometry, 1)
        }
        PixelFormat::Rgb24 => {
            crop_single_plane_av2_frame_to_geometry(frame, coded_geometry, visible_geometry, 3)
        }
        PixelFormat::Gray { .. } => unreachable!("AV2 does not accept monochrome input"),
    }
}

fn pad_planar_av2_frame_to_geometry(
    frame: &[u8],
    visible_geometry: Av2VideoGeometry,
    coded_geometry: Av2VideoGeometry,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
) -> Vec<u8> {
    let visible_layout = PlanarYuvFrameLayout::for_validated_shape(
        visible_geometry.width,
        visible_geometry.height,
        chroma_sampling,
        bit_depth,
    );
    let coded_layout = PlanarYuvFrameLayout::for_validated_shape(
        coded_geometry.width,
        coded_geometry.height,
        chroma_sampling,
        bit_depth,
    );
    let mut out = vec![0; coded_layout.frame_len()];
    let (src_y, src_u, src_v) = visible_layout.plane_slices(frame);
    let (dst_y, dst_u, dst_v) = coded_layout.plane_slices_mut(&mut out);
    pad_plane_by_edge(
        src_y,
        visible_layout.plane_stride(PlanarYuvPlane::Y),
        visible_layout.plane_dimensions(PlanarYuvPlane::Y).1,
        dst_y,
        coded_layout.plane_stride(PlanarYuvPlane::Y),
        coded_layout.plane_dimensions(PlanarYuvPlane::Y).1,
        visible_layout.bytes_per_sample(),
    );
    pad_plane_by_edge(
        src_u,
        visible_layout.plane_stride(PlanarYuvPlane::U),
        visible_layout.plane_dimensions(PlanarYuvPlane::U).1,
        dst_u,
        coded_layout.plane_stride(PlanarYuvPlane::U),
        coded_layout.plane_dimensions(PlanarYuvPlane::U).1,
        visible_layout.bytes_per_sample(),
    );
    pad_plane_by_edge(
        src_v,
        visible_layout.plane_stride(PlanarYuvPlane::V),
        visible_layout.plane_dimensions(PlanarYuvPlane::V).1,
        dst_v,
        coded_layout.plane_stride(PlanarYuvPlane::V),
        coded_layout.plane_dimensions(PlanarYuvPlane::V).1,
        visible_layout.bytes_per_sample(),
    );
    out
}

fn crop_planar_av2_frame_to_geometry(
    frame: &[u8],
    coded_geometry: Av2VideoGeometry,
    visible_geometry: Av2VideoGeometry,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
) -> Vec<u8> {
    let coded_layout = PlanarYuvFrameLayout::for_validated_shape(
        coded_geometry.width,
        coded_geometry.height,
        chroma_sampling,
        bit_depth,
    );
    let visible_layout = PlanarYuvFrameLayout::for_validated_shape(
        visible_geometry.width,
        visible_geometry.height,
        chroma_sampling,
        bit_depth,
    );
    let mut out = vec![0; visible_layout.frame_len()];
    let (src_y, src_u, src_v) = coded_layout.plane_slices(frame);
    let (dst_y, dst_u, dst_v) = visible_layout.plane_slices_mut(&mut out);
    crop_plane_from_origin(
        src_y,
        coded_layout.plane_stride(PlanarYuvPlane::Y),
        dst_y,
        visible_layout.plane_stride(PlanarYuvPlane::Y),
        visible_layout.plane_dimensions(PlanarYuvPlane::Y).1,
        visible_layout.bytes_per_sample(),
    );
    crop_plane_from_origin(
        src_u,
        coded_layout.plane_stride(PlanarYuvPlane::U),
        dst_u,
        visible_layout.plane_stride(PlanarYuvPlane::U),
        visible_layout.plane_dimensions(PlanarYuvPlane::U).1,
        visible_layout.bytes_per_sample(),
    );
    crop_plane_from_origin(
        src_v,
        coded_layout.plane_stride(PlanarYuvPlane::V),
        dst_v,
        visible_layout.plane_stride(PlanarYuvPlane::V),
        visible_layout.plane_dimensions(PlanarYuvPlane::V).1,
        visible_layout.bytes_per_sample(),
    );
    out
}

fn pad_three_plane_av2_frame_to_geometry(
    frame: &[u8],
    visible_geometry: Av2VideoGeometry,
    coded_geometry: Av2VideoGeometry,
    bytes_per_sample: usize,
) -> Vec<u8> {
    let visible_plane_len = visible_geometry.width * visible_geometry.height * bytes_per_sample;
    let coded_plane_len = coded_geometry.width * coded_geometry.height * bytes_per_sample;
    let mut out = vec![0; coded_plane_len * 3];
    for plane in 0..3 {
        let src_start = plane * visible_plane_len;
        let dst_start = plane * coded_plane_len;
        pad_plane_by_edge(
            &frame[src_start..src_start + visible_plane_len],
            visible_geometry.width,
            visible_geometry.height,
            &mut out[dst_start..dst_start + coded_plane_len],
            coded_geometry.width,
            coded_geometry.height,
            bytes_per_sample,
        );
    }
    out
}

fn crop_three_plane_av2_frame_to_geometry(
    frame: &[u8],
    coded_geometry: Av2VideoGeometry,
    visible_geometry: Av2VideoGeometry,
    bytes_per_sample: usize,
) -> Vec<u8> {
    let coded_plane_len = coded_geometry.width * coded_geometry.height * bytes_per_sample;
    let visible_plane_len = visible_geometry.width * visible_geometry.height * bytes_per_sample;
    let mut out = vec![0; visible_plane_len * 3];
    for plane in 0..3 {
        let src_start = plane * coded_plane_len;
        let dst_start = plane * visible_plane_len;
        crop_plane_from_origin(
            &frame[src_start..src_start + coded_plane_len],
            coded_geometry.width,
            &mut out[dst_start..dst_start + visible_plane_len],
            visible_geometry.width,
            visible_geometry.height,
            bytes_per_sample,
        );
    }
    out
}

fn pad_single_plane_av2_frame_to_geometry(
    frame: &[u8],
    visible_geometry: Av2VideoGeometry,
    coded_geometry: Av2VideoGeometry,
    bytes_per_sample: usize,
) -> Vec<u8> {
    let mut out = vec![0; coded_geometry.width * coded_geometry.height * bytes_per_sample];
    pad_plane_by_edge(
        frame,
        visible_geometry.width,
        visible_geometry.height,
        &mut out,
        coded_geometry.width,
        coded_geometry.height,
        bytes_per_sample,
    );
    out
}

fn crop_single_plane_av2_frame_to_geometry(
    frame: &[u8],
    coded_geometry: Av2VideoGeometry,
    visible_geometry: Av2VideoGeometry,
    bytes_per_sample: usize,
) -> Vec<u8> {
    let mut out = vec![0; visible_geometry.width * visible_geometry.height * bytes_per_sample];
    crop_plane_from_origin(
        frame,
        coded_geometry.width,
        &mut out,
        visible_geometry.width,
        visible_geometry.height,
        bytes_per_sample,
    );
    out
}

fn pad_plane_by_edge(
    src: &[u8],
    src_width: usize,
    src_height: usize,
    dst: &mut [u8],
    dst_width: usize,
    dst_height: usize,
    bytes_per_sample: usize,
) {
    debug_assert!(src_width > 0);
    debug_assert!(src_height > 0);
    debug_assert!(dst_width >= src_width);
    debug_assert!(dst_height >= src_height);
    debug_assert_eq!(src.len(), src_width * src_height * bytes_per_sample);
    debug_assert_eq!(dst.len(), dst_width * dst_height * bytes_per_sample);

    let src_row_bytes = src_width * bytes_per_sample;
    let dst_row_bytes = dst_width * bytes_per_sample;
    let last_sample_offset = src_row_bytes - bytes_per_sample;
    for dst_y in 0..dst_height {
        let src_y = dst_y.min(src_height - 1);
        let src_row = &src[src_y * src_row_bytes..(src_y + 1) * src_row_bytes];
        let dst_row = &mut dst[dst_y * dst_row_bytes..(dst_y + 1) * dst_row_bytes];
        dst_row[..src_row_bytes].copy_from_slice(src_row);
        let last_sample = &src_row[last_sample_offset..src_row_bytes];
        for dst_x in src_width..dst_width {
            let start = dst_x * bytes_per_sample;
            dst_row[start..start + bytes_per_sample].copy_from_slice(last_sample);
        }
    }
}

fn crop_plane_from_origin(
    src: &[u8],
    src_width: usize,
    dst: &mut [u8],
    dst_width: usize,
    dst_height: usize,
    bytes_per_sample: usize,
) {
    debug_assert!(src_width >= dst_width);
    debug_assert!(dst_width > 0);
    debug_assert!(dst_height > 0);
    debug_assert!(src.len() >= src_width * dst_height * bytes_per_sample);
    debug_assert_eq!(dst.len(), dst_width * dst_height * bytes_per_sample);

    let src_row_bytes = src_width * bytes_per_sample;
    let dst_row_bytes = dst_width * bytes_per_sample;
    for y in 0..dst_height {
        let src_start = y * src_row_bytes;
        let dst_start = y * dst_row_bytes;
        dst[dst_start..dst_start + dst_row_bytes]
            .copy_from_slice(&src[src_start..src_start + dst_row_bytes]);
    }
}
