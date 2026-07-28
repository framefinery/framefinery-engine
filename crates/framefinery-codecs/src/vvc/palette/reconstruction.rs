#[cfg(test)]
pub(super) fn vvc_palette_444_reconstruction_yuv(frame: &VvcSampledFrame) -> Vec<VvcSample> {
    vvc_palette_444_reconstruction_yuv_with_config(frame, VvcSliceSyntaxConfig::palette_444())
}

pub(super) fn vvc_palette_444_reconstruction_yuv_with_config(
    frame: &VvcSampledFrame,
    slice_config: VvcSliceSyntaxConfig,
) -> Vec<VvcSample> {
    debug_assert_eq!(frame.format.chroma_sampling, ChromaSampling::Cs444);
    let samples = frame.geometry.luma_samples();
    let mut luma = vec![0; samples];
    let mut cb = vec![0; samples];
    let mut cr = vec![0; samples];
    let mut ibc_search = VvcIbcHashSearch::new();
    let partition_shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: frame.geometry.coded_width() as u16,
        visible_height: frame.geometry.coded_height() as u16,
        chroma_sampling: frame.format.chroma_sampling,
        dual_tree_intra: false,
    };

    for op in VvcCtuCabacOp::intra_ctu_partition(partition_shape, VVC_PALETTE_CU_SIZE) {
        if let VvcCtuCabacOp::LumaLeafWithSplitCtx { node, .. } = op {
            let origin_x = node.x as usize;
            let origin_y = node.y as usize;
            if !vvc_palette_cu_origin_is_visible(frame.geometry, node.x, node.y) {
                continue;
            }
            if vvc_exact_hash_ibc_444_enabled(frame) {
                if let Some(decision) = ibc_search.decide_8x8(frame, origin_x, origin_y) {
                    copy_vvc_ibc_444_8x8_reconstruction(
                        &mut luma,
                        &mut cb,
                        &mut cr,
                        frame.geometry.width,
                        decision,
                    );
                    ibc_search.record_ibc_8x8(frame, decision);
                    continue;
                }
            }
            if let Some(residual) = vvc_transform_skip_residual_444_left_8x8(
                frame,
                &ibc_search,
                origin_x,
                origin_y,
                slice_config.slice_qp,
            ) {
                copy_vvc_ibc_444_8x8_reconstruction(
                    &mut luma,
                    &mut cb,
                    &mut cr,
                    frame.geometry.width,
                    residual.decision,
                );
                add_vvc_transform_skip_residual_444_8x8_reconstruction(
                    &mut luma,
                    &mut cb,
                    &mut cr,
                    frame.geometry.width,
                    frame.format.bit_depth,
                    &residual,
                );
                ibc_search.record_ibc_8x8(frame, residual.decision);
                continue;
            }
            if let Some(bdpcm) =
                vvc_bdpcm_horizontal_444_8x8(frame, origin_x, origin_y, slice_config.slice_qp)
            {
                add_vvc_bdpcm_horizontal_444_8x8_reconstruction(
                    &mut luma,
                    &mut cb,
                    &mut cr,
                    frame.geometry.width,
                    frame.format.bit_depth,
                    origin_x,
                    origin_y,
                    &bdpcm,
                );
                ibc_search.record_palette_8x8(frame, origin_x, origin_y);
                continue;
            }
            let syntax =
                vvc_palette_444_cu_syntax_with_config(frame, origin_x, origin_y, slice_config);
            let width = syntax.cb_width;
            let height = syntax.cb_height;
            for y_off in 0..height {
                for x_off in 0..width {
                    let local = y_off * width + x_off;
                    let palette_index = syntax.palette_indices.get(local).copied().unwrap_or(0);
                    let color = if syntax.palette_escape_val_present_flag
                        && palette_index == syntax.max_palette_index
                    {
                        let escape_level = syntax.palette_escape_values[local].expect(
                            "escape-coded palette sample must carry coded component levels",
                        );
                        vvc_palette_reconstruct_escape_color(
                            escape_level,
                            syntax.bit_depth,
                            syntax.slice_qp,
                        )
                    } else {
                        syntax.new_palette_entries[palette_index as usize]
                    };
                    let dst = (origin_y + y_off) * frame.geometry.width + origin_x + x_off;
                    luma[dst] = color.y;
                    cb[dst] = color.u;
                    cr[dst] = color.v;
                }
            }
            ibc_search.record_palette_8x8(frame, origin_x, origin_y);
        }
    }

    [luma, cb, cr].concat()
}

fn copy_vvc_ibc_444_8x8_reconstruction(
    luma: &mut [VvcSample],
    cb: &mut [VvcSample],
    cr: &mut [VvcSample],
    stride: usize,
    decision: VvcIbcCuDecision,
) {
    for y_off in 0..8 {
        let dst = (decision.origin_y + y_off) * stride + decision.origin_x;
        let src = (decision.ref_origin_y + y_off) * stride + decision.ref_origin_x;
        luma.copy_within(src..src + 8, dst);
        cb.copy_within(src..src + 8, dst);
        cr.copy_within(src..src + 8, dst);
    }
}

fn add_vvc_transform_skip_residual_444_8x8_reconstruction(
    luma: &mut [VvcSample],
    cb: &mut [VvcSample],
    cr: &mut [VvcSample],
    stride: usize,
    bit_depth: SampleBitDepth,
    residual: &VvcTransformSkipResidual444Cu,
) {
    let origin_x = residual.decision.origin_x;
    let origin_y = residual.decision.origin_y;
    for y_off in 0..4 {
        for x_off in 0..4 {
            let local = y_off * 8 + x_off;
            let dst = (origin_y + y_off) * stride + origin_x + x_off;
            luma[dst] = add_i16_to_sample(luma[dst], residual.y_coeffs[local], bit_depth);
            cb[dst] = add_i16_to_sample(cb[dst], residual.cb_coeffs[local], bit_depth);
            cr[dst] = add_i16_to_sample(cr[dst], residual.cr_coeffs[local], bit_depth);
        }
    }
}

fn vvc_transform_skip_residual_444_left_8x8(
    frame: &VvcSampledFrame,
    ibc_search: &VvcIbcHashSearch,
    origin_x: usize,
    origin_y: usize,
    slice_qp: i32,
) -> Option<VvcTransformSkipResidual444Cu> {
    let decision = ibc_search.decide_left_8x8(frame, origin_x, origin_y)?;
    let mut y_coeffs = vec![0i16; 64];
    let mut cb_coeffs = vec![0i16; 64];
    let mut cr_coeffs = vec![0i16; 64];
    let mut cbf_y = false;
    let mut cbf_cb = false;
    let mut cbf_cr = false;

    for y_off in 0..8 {
        for x_off in 0..8 {
            let cur = (origin_y + y_off) * frame.geometry.width + origin_x + x_off;
            let ref_idx = (decision.ref_origin_y + y_off) * frame.geometry.width
                + decision.ref_origin_x
                + x_off;
            let in_residual_subset = x_off < 4 && y_off < 4;
            let y_diff = vvc_palette_sample_diff_i16(frame.luma[cur], frame.luma[ref_idx]);
            let cb_diff = vvc_palette_sample_diff_i16(frame.cb[cur], frame.cb[ref_idx]);
            let cr_diff = vvc_palette_sample_diff_i16(frame.cr[cur], frame.cr[ref_idx]);
            if !in_residual_subset && (y_diff != 0 || cb_diff != 0 || cr_diff != 0) {
                return None;
            }
            if in_residual_subset {
                if !vvc_palette_transform_skip_coeff_is_exact(
                    y_diff,
                    frame.format.bit_depth,
                    slice_qp,
                ) || !vvc_palette_transform_skip_coeff_is_exact(
                    cb_diff,
                    frame.format.bit_depth,
                    slice_qp,
                ) || !vvc_palette_transform_skip_coeff_is_exact(
                    cr_diff,
                    frame.format.bit_depth,
                    slice_qp,
                ) {
                    return None;
                }
                let local = y_off * 8 + x_off;
                y_coeffs[local] = y_diff;
                cb_coeffs[local] = cb_diff;
                cr_coeffs[local] = cr_diff;
                cbf_y |= y_diff != 0;
                cbf_cb |= cb_diff != 0;
                cbf_cr |= cr_diff != 0;
            }
        }
    }

    // Keep this first transform-skip subset observable and syntax-simple:
    // require at least one chroma residual so QtCbf[Y] is explicitly present,
    // and leave pure-luma IBC residuals for the later full-CU residual path.
    if !cbf_y && !cbf_cb && !cbf_cr {
        return None;
    }
    if !cbf_cb && !cbf_cr {
        return None;
    }
    // H.266 8.6.2.2 derives the IBC predictor list from A1/B1/HMVP/zero.
    // The first RTL transform-skip residual subset hardcodes MVD -8,0, so
    // software only selects this mode while that zero-predictor syntax applies.
    if decision.pred_mode_ibc_ctx != 0 || decision.mvd_x != -8 || decision.mvd_y != 0 {
        return None;
    }

    Some(VvcTransformSkipResidual444Cu {
        decision,
        y_coeffs,
        cb_coeffs,
        cr_coeffs,
        cbf_y,
        cbf_cb,
        cbf_cr,
    })
}

fn add_vvc_bdpcm_horizontal_444_8x8_reconstruction(
    luma: &mut [VvcSample],
    cb: &mut [VvcSample],
    cr: &mut [VvcSample],
    stride: usize,
    bit_depth: SampleBitDepth,
    origin_x: usize,
    origin_y: usize,
    residual: &VvcBdpcm444Cu,
) {
    debug_assert!(origin_x > 0);
    for y_off in 0..8 {
        let row = origin_y + y_off;
        let left = row * stride + origin_x - 1;
        let mut y_residual = 0i16;
        let mut cb_residual = 0i16;
        let mut cr_residual = 0i16;
        for x_off in 0..8 {
            let local = y_off * 8 + x_off;
            y_residual += residual.y_coeffs[local];
            cb_residual += residual.cb_coeffs[local];
            cr_residual += residual.cr_coeffs[local];
            let dst = row * stride + origin_x + x_off;
            luma[dst] = add_i16_to_sample(luma[left], y_residual, bit_depth);
            cb[dst] = add_i16_to_sample(cb[left], cb_residual, bit_depth);
            cr[dst] = add_i16_to_sample(cr[left], cr_residual, bit_depth);
        }
    }
}

fn vvc_bdpcm_horizontal_444_8x8(
    frame: &VvcSampledFrame,
    origin_x: usize,
    origin_y: usize,
    slice_qp: i32,
) -> Option<VvcBdpcm444Cu> {
    if origin_x == 0 || origin_x + 8 > frame.geometry.width || origin_y + 8 > frame.geometry.height
    {
        return None;
    }

    let (y_coeffs, cbf_y) = vvc_bdpcm_horizontal_coefficients(
        &frame.luma,
        frame.geometry.width,
        origin_x,
        origin_y,
        frame.format.bit_depth,
        slice_qp,
    )?;
    let (cb_coeffs, cbf_cb) = vvc_bdpcm_horizontal_coefficients(
        &frame.cb,
        frame.geometry.width,
        origin_x,
        origin_y,
        frame.format.bit_depth,
        slice_qp,
    )?;
    let (cr_coeffs, cbf_cr) = vvc_bdpcm_horizontal_coefficients(
        &frame.cr,
        frame.geometry.width,
        origin_x,
        origin_y,
        frame.format.bit_depth,
        slice_qp,
    )?;

    if !cbf_y && !cbf_cb && !cbf_cr {
        return None;
    }

    Some(VvcBdpcm444Cu {
        y_coeffs,
        cb_coeffs,
        cr_coeffs,
        cbf_y,
        cbf_cb,
        cbf_cr,
    })
}

fn vvc_bdpcm_horizontal_coefficients(
    plane: &[VvcSample],
    stride: usize,
    origin_x: usize,
    origin_y: usize,
    bit_depth: SampleBitDepth,
    slice_qp: i32,
) -> Option<(Vec<i16>, bool)> {
    let mut coeffs = vec![0i16; 64];
    let mut cbf = false;
    for y_off in 0..8 {
        let row = origin_y + y_off;
        let left = row * stride + origin_x - 1;
        let left_sample = i32::from(plane[left]);
        let mut prev_residual = 0i32;
        for x_off in 0..8 {
            let cur = row * stride + origin_x + x_off;
            let residual = i32::from(plane[cur]) - left_sample;
            let coeff = if x_off == 0 {
                residual
            } else {
                residual - prev_residual
            };
            let in_residual_subset = x_off < 4 && y_off < 4;
            if !in_residual_subset && coeff != 0 {
                return None;
            }
            if in_residual_subset {
                let coeff = coeff.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
                if !vvc_palette_transform_skip_coeff_is_exact(coeff, bit_depth, slice_qp) {
                    return None;
                }
                coeffs[y_off * 8 + x_off] = coeff;
                cbf |= coeff != 0;
            }
            prev_residual = residual;
        }
    }
    Some((coeffs, cbf))
}

fn vvc_palette_sample_diff_i16(sample: VvcSample, reference: VvcSample) -> i16 {
    (i32::from(sample) - i32::from(reference)).clamp(i32::from(i16::MIN), i32::from(i16::MAX))
        as i16
}

fn vvc_palette_escape_level_color(
    color: VvcSampledColor,
    bit_depth: SampleBitDepth,
    slice_qp: i32,
) -> VvcSampledColor {
    VvcSampledColor {
        y: vvc_palette_escape_level(color.y, bit_depth, slice_qp),
        u: vvc_palette_escape_level(color.u, bit_depth, slice_qp),
        v: vvc_palette_escape_level(color.v, bit_depth, slice_qp),
    }
}

fn vvc_palette_reconstruct_escape_color(
    color: VvcSampledColor,
    bit_depth: SampleBitDepth,
    slice_qp: i32,
) -> VvcSampledColor {
    VvcSampledColor {
        y: vvc_palette_reconstruct_escape_level(color.y, bit_depth, slice_qp),
        u: vvc_palette_reconstruct_escape_level(color.u, bit_depth, slice_qp),
        v: vvc_palette_reconstruct_escape_level(color.v, bit_depth, slice_qp),
    }
}

fn vvc_palette_escape_level(
    sample: VvcSample,
    bit_depth: SampleBitDepth,
    slice_qp: i32,
) -> VvcSample {
    let shift = vvc_palette_escape_reconstruction_shift(bit_depth, slice_qp);
    if shift == 0 {
        return sample;
    }
    let rounding = 1u32 << (shift - 1);
    ((u32::from(sample) + rounding) >> shift) as VvcSample
}

fn vvc_palette_reconstruct_escape_level(
    level: VvcSample,
    bit_depth: SampleBitDepth,
    slice_qp: i32,
) -> VvcSample {
    let shift = vvc_palette_escape_reconstruction_shift(bit_depth, slice_qp);
    (u32::from(level) << shift).min(u32::from(bit_depth.max_sample())) as VvcSample
}

fn vvc_palette_escape_reconstruction_shift(bit_depth: SampleBitDepth, slice_qp: i32) -> u32 {
    let qp_bd_offset = (i32::from(bit_depth.bits()) - 8) * 6;
    let decoder_qp = slice_qp + qp_bd_offset;
    debug_assert_eq!(decoder_qp.rem_euclid(6), 4);
    debug_assert!(decoder_qp >= 4);
    ((decoder_qp - 4) / 6) as u32
}

fn vvc_palette_transform_skip_coeff_is_exact(
    coeff: i16,
    bit_depth: SampleBitDepth,
    slice_qp: i32,
) -> bool {
    vvc_palette_transform_skip_coded_coeff(coeff, bit_depth, slice_qp).is_some()
}

fn vvc_palette_transform_skip_coded_coeff(
    coeff: i16,
    bit_depth: SampleBitDepth,
    slice_qp: i32,
) -> Option<i16> {
    let shift = vvc_palette_escape_reconstruction_shift(bit_depth, slice_qp);
    if shift == 0 {
        return Some(coeff);
    }
    let scale = 1i32 << shift;
    let coeff = i32::from(coeff);
    if coeff % scale != 0 {
        return None;
    }
    Some((coeff / scale) as i16)
}

fn vvc_palette_transform_skip_coded_coefficients<'a>(
    coeffs: &'a [i16],
    bit_depth: SampleBitDepth,
    slice_qp: i32,
) -> Cow<'a, [i16]> {
    if vvc_palette_escape_reconstruction_shift(bit_depth, slice_qp) == 0 {
        return Cow::Borrowed(coeffs);
    }
    Cow::Owned(
        coeffs
            .iter()
            .copied()
            .map(|coeff| {
                vvc_palette_transform_skip_coded_coeff(coeff, bit_depth, slice_qp)
                    .expect("selected high-depth transform-skip residual must be exact")
            })
            .collect(),
    )
}

#[cfg(test)]
pub(super) fn vvc_palette_transform_skip_coded_coeff_for_test(
    coeff: i16,
    bit_depth: SampleBitDepth,
) -> Option<i16> {
    vvc_palette_transform_skip_coded_coeff(
        coeff,
        bit_depth,
        VvcSliceSyntaxConfig::palette_444().slice_qp,
    )
}

#[cfg(test)]
pub(super) fn vvc_palette_transform_skip_coded_coeff_with_config_for_test(
    coeff: i16,
    bit_depth: SampleBitDepth,
    slice_config: VvcSliceSyntaxConfig,
) -> Option<i16> {
    vvc_palette_transform_skip_coded_coeff(coeff, bit_depth, slice_config.slice_qp)
}

fn add_i16_to_sample(sample: VvcSample, delta: i16, bit_depth: SampleBitDepth) -> VvcSample {
    let value = i32::from(sample) + i32::from(delta);
    value.clamp(0, i32::from(bit_depth.max_sample())) as VvcSample
}

#[cfg(test)]
pub(super) fn vvc_palette_444_decode_reconstruction(
    geometry: VvcVideoGeometry,
    syntax: VvcPalette444Syntax,
) -> VvcPalette444DecodedPicture {
    // H.266 8.4.5.3, restricted to the current SINGLE_TREE 4:4:4 subset:
    // PaletteIndexMap either selects CurrentPaletteEntries or, when equal to
    // MaxPaletteIndex with palette_escape_val_present_flag set, reconstructs
    // PaletteEscapeVal through equations (441)..(443) using SliceQpY.
    debug_assert_eq!(syntax.tree_type, VvcPaletteTreeType::SingleTree);
    debug_assert_eq!(syntax.start_comp, 0);
    debug_assert_eq!(syntax.num_comps, 3);

    let samples = geometry.luma_samples();
    if syntax.max_palette_index == 0 && !syntax.palette_escape_val_present_flag {
        let entry = syntax.new_palette_entries[0];
        return VvcPalette444DecodedPicture {
            luma: vec![entry.y; samples],
            cb: vec![entry.u; samples],
            cr: vec![entry.v; samples],
        };
    }

    let mut luma = Vec::with_capacity(samples);
    let mut cb = Vec::with_capacity(samples);
    let mut cr = Vec::with_capacity(samples);
    for (sample_idx, index) in syntax.palette_indices.iter().enumerate() {
        let color = if syntax.palette_escape_val_present_flag && *index == syntax.max_palette_index
        {
            let escape_level = syntax.palette_escape_values[sample_idx]
                .expect("escape-coded palette sample must carry coded component levels");
            vvc_palette_reconstruct_escape_color(escape_level, syntax.bit_depth, syntax.slice_qp)
        } else {
            syntax.new_palette_entries[*index as usize]
        };
        luma.push(color.y);
        cb.push(color.u);
        cr.push(color.v);
    }
    VvcPalette444DecodedPicture { luma, cb, cr }
}
