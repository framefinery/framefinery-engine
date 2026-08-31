#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcPlaneRegion {
    origin_x: usize,
    origin_y: usize,
    geometry: VvcVideoGeometry,
}

fn copy_vvc_source_plane_region_with_edge_extension(
    destination: &mut [VvcSample],
    destination_geometry: VvcVideoGeometry,
    source: &[VvcSample],
    source_geometry: VvcVideoGeometry,
    region: VvcPlaneRegion,
) -> bool {
    let Some(destination_len) = destination_geometry
        .width
        .checked_mul(destination_geometry.height)
    else {
        return false;
    };
    let Some(source_len) = source_geometry.width.checked_mul(source_geometry.height) else {
        return false;
    };
    if destination.len() != destination_len
        || source.len() != source_len
        || source_geometry.width == 0
        || source_geometry.height == 0
        || region.geometry.width == 0
        || region.geometry.height == 0
        || region.origin_x >= destination_geometry.width
        || region.origin_y >= destination_geometry.height
    {
        return false;
    }

    let end_x = region
        .origin_x
        .saturating_add(region.geometry.width)
        .min(destination_geometry.width);
    let end_y = region
        .origin_y
        .saturating_add(region.geometry.height)
        .min(destination_geometry.height);
    for y in region.origin_y..end_y {
        let destination_row = y * destination_geometry.width;
        let source_y = y.min(source_geometry.height - 1);
        let source_row = source_y * source_geometry.width;
        for x in region.origin_x..end_x {
            let source_x = x.min(source_geometry.width - 1);
            destination[destination_row + x] = source[source_row + source_x];
        }
    }
    true
}
