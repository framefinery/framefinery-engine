#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcCclmParameters {
    a: i32,
    b: i32,
    shift: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcCclmLayout {
    width: usize,
    height: usize,
    template: VvcCclmTemplate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcCclmTemplate {
    above_available: bool,
    left_available: bool,
    downsample_left_available: bool,
    actual_top: usize,
    actual_left: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcCclmLumaSelection {
    select_luma: [i32; 4],
    min_group: [usize; 2],
    max_group: [usize; 2],
}

fn vvc_cclm_template(
    mode: VvcChromaCclmMode,
    availability: Option<VvcPlaneAvailability<'_>>,
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    chroma_sampling: ChromaSampling,
) -> VvcCclmTemplate {
    let top_available =
        cclm_top_template_available(availability, plane_width, start_x, start_y, width);
    let left_available =
        cclm_left_template_available(availability, plane_height, start_x, start_y, height);
    match mode {
        VvcChromaCclmMode::Linear => VvcCclmTemplate {
            above_available: top_available,
            left_available,
            downsample_left_available: left_available,
            actual_top: usize::from(top_available) * width,
            actual_left: usize::from(left_available) * height,
        },
        VvcChromaCclmMode::MdlmTop => {
            let unit_width = cclm_chroma_unit_width(chroma_sampling);
            let max_extra = (height / unit_width) * unit_width;
            let extra = if top_available {
                cclm_top_right_template_available_count(
                    availability,
                    plane_width,
                    start_x,
                    start_y,
                    width,
                    max_extra,
                    unit_width,
                )
            } else {
                0
            };
            VvcCclmTemplate {
                above_available: top_available,
                left_available: false,
                downsample_left_available: left_available,
                actual_top: usize::from(top_available) * (width + extra),
                actual_left: 0,
            }
        }
        VvcChromaCclmMode::MdlmLeft => {
            let unit_height = cclm_chroma_unit_height(chroma_sampling);
            let max_extra = (width / unit_height) * unit_height;
            let extra = if left_available {
                cclm_below_left_template_available_count(
                    availability,
                    plane_height,
                    start_x,
                    start_y,
                    height,
                    max_extra,
                    unit_height,
                )
            } else {
                0
            };
            VvcCclmTemplate {
                above_available: false,
                left_available,
                downsample_left_available: left_available,
                actual_top: 0,
                actual_left: usize::from(left_available) * (height + extra),
            }
        }
    }
}

fn derive_vvc_cclm_luma_selection(
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    luma_availability: Option<VvcPlaneAvailability<'_>>,
    layout: VvcCclmLayout,
) -> VvcCclmLumaSelection {
    let template = layout.template;
    let actual_top = template.actual_top;
    let actual_left = template.actual_left;
    let above_is4 = usize::from(!template.left_available);
    let left_is4 = usize::from(!template.above_available);
    let top_start = actual_top >> (2 + above_is4);
    let top_step = (actual_top >> (1 + above_is4)).max(1);
    let left_start = actual_left >> (2 + left_is4);
    let left_step = (actual_left >> (1 + left_is4)).max(1);

    let mut select_luma = [0i32; 4];
    let mut top_count = 0usize;
    let mut total_count = 0usize;
    if template.above_available {
        top_count = actual_top.min((1 + above_is4) << 1);
        let mut pos = top_start;
        for idx in 0..top_count {
            select_luma[idx] = cclm_downsample_top_luma(
                luma,
                geometry,
                node,
                chroma_sampling,
                bit_depth,
                luma_availability,
                pos,
                template.downsample_left_available,
            );
            pos += top_step;
            total_count += 1;
        }
    }
    if template.left_available {
        let left_count = actual_left.min((1 + left_is4) << 1);
        let mut pos = left_start;
        for idx in 0..left_count {
            let dst = top_count + idx;
            select_luma[dst] = cclm_downsample_left_luma(
                luma,
                geometry,
                node,
                chroma_sampling,
                bit_depth,
                luma_availability,
                pos,
            );
            pos += left_step;
            total_count += 1;
        }
    }

    if total_count == 2 {
        select_luma[3] = select_luma[0];
        select_luma[2] = select_luma[1];
        select_luma[0] = select_luma[1];
        select_luma[1] = select_luma[3];
    }

    let mut min_group = [0usize, 2usize];
    let mut max_group = [1usize, 3usize];
    if select_luma[min_group[0]] > select_luma[min_group[1]] {
        min_group.swap(0, 1);
    }
    if select_luma[max_group[0]] > select_luma[max_group[1]] {
        max_group.swap(0, 1);
    }
    if select_luma[min_group[0]] > select_luma[max_group[1]] {
        std::mem::swap(&mut min_group, &mut max_group);
    }
    if select_luma[min_group[1]] > select_luma[max_group[0]] {
        std::mem::swap(&mut min_group[1], &mut max_group[0]);
    }

    VvcCclmLumaSelection {
        select_luma,
        min_group,
        max_group,
    }
}

fn derive_vvc_cclm_parameters_from_selection(
    chroma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    chroma_availability: Option<VvcPlaneAvailability<'_>>,
    layout: VvcCclmLayout,
    luma_selection: VvcCclmLumaSelection,
) -> VvcCclmParameters {
    let template = layout.template;
    if !template.above_available && !template.left_available {
        return VvcCclmParameters {
            a: 0,
            b: i32::from(vvc_neutral_sample(bit_depth)),
            shift: 0,
        };
    }

    let actual_top = template.actual_top;
    let actual_left = template.actual_left;
    let above_is4 = usize::from(!template.left_available);
    let left_is4 = usize::from(!template.above_available);
    let top_start = actual_top >> (2 + above_is4);
    let top_step = (actual_top >> (1 + above_is4)).max(1);
    let left_start = actual_left >> (2 + left_is4);
    let left_step = (actual_left >> (1 + left_is4)).max(1);

    let mut select_chroma = [0i32; 4];
    let mut top_count = 0usize;
    let mut total_count = 0usize;
    if template.above_available {
        top_count = actual_top.min((1 + above_is4) << 1);
        let mut pos = top_start;
        for idx in 0..top_count {
            select_chroma[idx] = i32::from(cclm_chroma_sample(
                chroma,
                geometry,
                node,
                chroma_sampling,
                bit_depth,
                chroma_availability,
                pos as isize,
                -1,
            ));
            pos += top_step;
            total_count += 1;
        }
    }
    if template.left_available {
        let left_count = actual_left.min((1 + left_is4) << 1);
        let mut pos = left_start;
        for idx in 0..left_count {
            let dst = top_count + idx;
            select_chroma[dst] = i32::from(cclm_chroma_sample(
                chroma,
                geometry,
                node,
                chroma_sampling,
                bit_depth,
                chroma_availability,
                -1,
                pos as isize,
            ));
            pos += left_step;
            total_count += 1;
        }
    }

    if total_count == 2 {
        select_chroma[3] = select_chroma[0];
        select_chroma[2] = select_chroma[1];
        select_chroma[0] = select_chroma[1];
        select_chroma[1] = select_chroma[3];
    }

    let select_luma = luma_selection.select_luma;
    let min_group = luma_selection.min_group;
    let max_group = luma_selection.max_group;
    let min_luma = (select_luma[min_group[0]] + select_luma[min_group[1]] + 1) >> 1;
    let min_chroma = (select_chroma[min_group[0]] + select_chroma[min_group[1]] + 1) >> 1;
    let max_luma = (select_luma[max_group[0]] + select_luma[max_group[1]] + 1) >> 1;
    let max_chroma = (select_chroma[max_group[0]] + select_chroma[max_group[1]] + 1) >> 1;
    let diff = max_luma - min_luma;
    if diff <= 0 {
        return VvcCclmParameters {
            a: 0,
            b: min_chroma,
            shift: 0,
        };
    }
    let diff_chroma = max_chroma - min_chroma;
    let mut x = floor_log2_i32(diff);
    const DIV_SIG_TABLE: [i32; 16] = [0, 7, 6, 5, 5, 4, 4, 3, 3, 2, 2, 1, 1, 1, 1, 0];
    let norm_diff = ((diff << 4) >> x) & 15;
    let v = DIV_SIG_TABLE[norm_diff as usize] | 8;
    x += i32::from(norm_diff != 0);
    let y = floor_log2_i32(diff_chroma.abs()) + 1;
    let add = 1 << y >> 1;
    let mut a = (diff_chroma * v + add) >> y;
    let mut shift = 3 + x - y;
    if shift < 1 {
        shift = 1;
        a = if a == 0 {
            0
        } else if a < 0 {
            -15
        } else {
            15
        };
    }
    let b = min_chroma - right_shift_i32(a * min_luma, shift);
    VvcCclmParameters { a, b, shift }
}

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

fn floor_log2_i32(value: i32) -> i32 {
    if value <= 0 {
        -1
    } else {
        31 - value.leading_zeros() as i32
    }
}

fn right_shift_i32(value: i32, shift: i32) -> i32 {
    if shift >= 0 {
        value >> shift
    } else {
        value << (-shift)
    }
}
