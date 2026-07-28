fn rgb24_to_planar_gbr(frame: &[u8], geometry: Av2VideoGeometry) -> Vec<u8> {
    let pixels = geometry.width * geometry.height;
    debug_assert_eq!(frame.len(), pixels * 3);
    let mut out = vec![0; pixels * 3];
    let (g_plane, chroma) = out.split_at_mut(pixels);
    let (b_plane, r_plane) = chroma.split_at_mut(pixels);
    for (pixel, source) in frame.chunks_exact(3).enumerate() {
        r_plane[pixel] = source[0];
        g_plane[pixel] = source[1];
        b_plane[pixel] = source[2];
    }
    out
}

fn planar_gbr_to_rgb24(frame: &[u8], geometry: Av2VideoGeometry) -> Vec<u8> {
    let pixels = geometry.width * geometry.height;
    debug_assert_eq!(frame.len(), pixels * 3);
    let (g_plane, chroma) = frame.split_at(pixels);
    let (b_plane, r_plane) = chroma.split_at(pixels);
    let mut out = vec![0; pixels * 3];
    for (pixel, ((&r, &g), &b)) in out
        .chunks_exact_mut(3)
        .zip(r_plane.iter().zip(g_plane.iter()).zip(b_plane.iter()))
    {
        pixel[0] = r;
        pixel[1] = g;
        pixel[2] = b;
    }
    out
}
