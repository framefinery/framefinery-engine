fn cclm_top_template_available(
    availability: Option<VvcPlaneAvailability<'_>>,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
) -> bool {
    if start_y == 0 || start_x >= plane_width {
        return false;
    }
    let checked_width = width.min(plane_width - start_x);
    checked_width > 0
        && (0..checked_width)
            .all(|x| reference_sample_available(availability, start_x + x, start_y - 1))
}

fn cclm_left_template_available(
    availability: Option<VvcPlaneAvailability<'_>>,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    height: usize,
) -> bool {
    if start_x == 0 || start_y >= plane_height {
        return false;
    }
    let checked_height = height.min(plane_height - start_y);
    checked_height > 0
        && (0..checked_height)
            .all(|y| reference_sample_available(availability, start_x - 1, start_y + y))
}

fn cclm_top_right_template_available_count(
    availability: Option<VvcPlaneAvailability<'_>>,
    plane_width: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    max_extra: usize,
    unit_width: usize,
) -> usize {
    if start_y == 0 || start_x.saturating_add(width) >= plane_width || max_extra == 0 {
        return 0;
    }
    let available_extra = max_extra.min(plane_width - start_x - width);
    let mut count = 0usize;
    while count < available_extra {
        if !reference_sample_available(availability, start_x + width + count, start_y - 1) {
            break;
        }
        count += 1;
    }
    (count / unit_width) * unit_width
}

fn cclm_below_left_template_available_count(
    availability: Option<VvcPlaneAvailability<'_>>,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    height: usize,
    max_extra: usize,
    unit_height: usize,
) -> usize {
    if start_x == 0 || start_y.saturating_add(height) >= plane_height || max_extra == 0 {
        return 0;
    }
    let available_extra = max_extra.min(plane_height - start_y - height);
    let mut count = 0usize;
    while count < available_extra {
        if !reference_sample_available(availability, start_x - 1, start_y + height + count) {
            break;
        }
        count += 1;
    }
    (count / unit_height) * unit_height
}

fn cclm_chroma_unit_width(chroma_sampling: ChromaSampling) -> usize {
    (4 / chroma_subsample_x(chroma_sampling)).max(1)
}

fn cclm_chroma_unit_height(chroma_sampling: ChromaSampling) -> usize {
    (4 / chroma_subsample_y(chroma_sampling)).max(1)
}

fn cclm_chroma_sample(
    chroma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
    rel_x: isize,
    rel_y: isize,
) -> VvcSample {
    let subsample_x = chroma_subsample_x(chroma_sampling);
    let subsample_y = chroma_subsample_y(chroma_sampling);
    let plane_width = geometry.width / subsample_x;
    let plane_height = geometry.height / subsample_y;
    let start_x = usize::from(node.x) / subsample_x;
    let start_y = usize::from(node.y) / subsample_y;
    let Some((x, y)) =
        clamp_relative_sample_position(start_x, start_y, rel_x, rel_y, plane_width, plane_height)
    else {
        return vvc_neutral_sample(bit_depth);
    };
    if !reference_sample_available(availability, x, y) {
        return vvc_neutral_sample(bit_depth);
    }
    chroma[y * plane_width + x]
}

fn cclm_downsample_inner_luma(
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
    rel_x: usize,
    rel_y: usize,
    left_available: bool,
) -> i32 {
    let luma_x = node.x as isize + (rel_x * chroma_subsample_x(chroma_sampling)) as isize;
    let luma_y = node.y as isize + (rel_y * chroma_subsample_y(chroma_sampling)) as isize;
    cclm_downsample_luma_at(
        luma,
        geometry,
        chroma_sampling,
        bit_depth,
        availability,
        luma_x,
        luma_y,
        rel_x == 0 && !left_available,
    )
}

fn cclm_downsample_top_luma(
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
    rel_x: usize,
    left_available: bool,
) -> i32 {
    let luma_x = node.x as isize + (rel_x * chroma_subsample_x(chroma_sampling)) as isize;
    match chroma_sampling {
        ChromaSampling::Cs444 => cclm_luma_sample(
            luma,
            geometry,
            luma_x,
            node.y as isize - 1,
            bit_depth,
            availability,
        ),
        ChromaSampling::Cs422 => cclm_downsample_luma_at(
            luma,
            geometry,
            chroma_sampling,
            bit_depth,
            availability,
            luma_x,
            node.y as isize - 1,
            rel_x == 0 && !left_available,
        ),
        ChromaSampling::Cs420 => {
            let first_row_of_ctu = usize::from(node.y) % VVC_CTU_SIZE == 0;
            let luma_y = if first_row_of_ctu {
                node.y as isize - 1
            } else {
                node.y as isize - 2
            };
            cclm_downsample_luma_at(
                luma,
                geometry,
                if first_row_of_ctu {
                    ChromaSampling::Cs422
                } else {
                    ChromaSampling::Cs420
                },
                bit_depth,
                availability,
                luma_x,
                luma_y,
                rel_x == 0 && !left_available,
            )
        }
        ChromaSampling::Monochrome => i32::from(vvc_neutral_sample(bit_depth)),
    }
}

fn cclm_downsample_left_luma(
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
    rel_y: usize,
) -> i32 {
    let luma_x = node.x as isize - chroma_subsample_x(chroma_sampling) as isize;
    let luma_y = node.y as isize + (rel_y * chroma_subsample_y(chroma_sampling)) as isize;
    match chroma_sampling {
        ChromaSampling::Cs444 => {
            cclm_luma_sample(luma, geometry, luma_x, luma_y, bit_depth, availability)
        }
        ChromaSampling::Cs422 => {
            let center = cclm_luma_sample(luma, geometry, luma_x, luma_y, bit_depth, availability);
            let left =
                cclm_luma_sample(luma, geometry, luma_x - 1, luma_y, bit_depth, availability);
            let right =
                cclm_luma_sample(luma, geometry, luma_x + 1, luma_y, bit_depth, availability);
            (2 + 2 * center + left + right) >> 2
        }
        ChromaSampling::Cs420 => {
            let top = luma_y;
            let center0 = cclm_luma_sample(luma, geometry, luma_x, top, bit_depth, availability);
            let left0 = cclm_luma_sample(luma, geometry, luma_x - 1, top, bit_depth, availability);
            let right0 = cclm_luma_sample(luma, geometry, luma_x + 1, top, bit_depth, availability);
            let center1 =
                cclm_luma_sample(luma, geometry, luma_x, top + 1, bit_depth, availability);
            let left1 =
                cclm_luma_sample(luma, geometry, luma_x - 1, top + 1, bit_depth, availability);
            let right1 =
                cclm_luma_sample(luma, geometry, luma_x + 1, top + 1, bit_depth, availability);
            (4 + 2 * center0 + left0 + right0 + 2 * center1 + left1 + right1) >> 3
        }
        ChromaSampling::Monochrome => i32::from(vvc_neutral_sample(bit_depth)),
    }
}

fn cclm_downsample_luma_at(
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
    luma_x: isize,
    luma_y: isize,
    left_padding: bool,
) -> i32 {
    match chroma_sampling {
        ChromaSampling::Cs444 => {
            cclm_luma_sample(luma, geometry, luma_x, luma_y, bit_depth, availability)
        }
        ChromaSampling::Cs422 => {
            let left_x = luma_x - isize::from(!left_padding);
            let center = cclm_luma_sample(luma, geometry, luma_x, luma_y, bit_depth, availability);
            let left = cclm_luma_sample(luma, geometry, left_x, luma_y, bit_depth, availability);
            let right =
                cclm_luma_sample(luma, geometry, luma_x + 1, luma_y, bit_depth, availability);
            (2 + 2 * center + left + right) >> 2
        }
        ChromaSampling::Cs420 => {
            let left_x = luma_x - isize::from(!left_padding);
            let center0 = cclm_luma_sample(luma, geometry, luma_x, luma_y, bit_depth, availability);
            let left0 = cclm_luma_sample(luma, geometry, left_x, luma_y, bit_depth, availability);
            let right0 =
                cclm_luma_sample(luma, geometry, luma_x + 1, luma_y, bit_depth, availability);
            let center1 =
                cclm_luma_sample(luma, geometry, luma_x, luma_y + 1, bit_depth, availability);
            let left1 =
                cclm_luma_sample(luma, geometry, left_x, luma_y + 1, bit_depth, availability);
            let right1 = cclm_luma_sample(
                luma,
                geometry,
                luma_x + 1,
                luma_y + 1,
                bit_depth,
                availability,
            );
            (4 + 2 * center0 + left0 + right0 + 2 * center1 + left1 + right1) >> 3
        }
        ChromaSampling::Monochrome => i32::from(vvc_neutral_sample(bit_depth)),
    }
}

fn cclm_luma_sample(
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    x: isize,
    y: isize,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) -> i32 {
    let Some((x, y)) = clamp_sample_position(x, y, geometry.width, geometry.height) else {
        return i32::from(vvc_neutral_sample(bit_depth));
    };
    if !reference_sample_available(availability, x, y) {
        return i32::from(vvc_neutral_sample(bit_depth));
    }
    i32::from(luma[y * geometry.width + x])
}

fn clamp_relative_sample_position(
    start_x: usize,
    start_y: usize,
    rel_x: isize,
    rel_y: isize,
    plane_width: usize,
    plane_height: usize,
) -> Option<(usize, usize)> {
    let x = start_x as isize + rel_x;
    let y = start_y as isize + rel_y;
    clamp_sample_position(x, y, plane_width, plane_height)
}

fn clamp_sample_position(
    x: isize,
    y: isize,
    plane_width: usize,
    plane_height: usize,
) -> Option<(usize, usize)> {
    if plane_width == 0 || plane_height == 0 {
        return None;
    }
    Some((
        x.clamp(0, plane_width.saturating_sub(1) as isize) as usize,
        y.clamp(0, plane_height.saturating_sub(1) as isize) as usize,
    ))
}
