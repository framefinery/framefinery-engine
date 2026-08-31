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
