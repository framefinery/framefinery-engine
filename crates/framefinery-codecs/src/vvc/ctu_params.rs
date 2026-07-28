fn vvc_frame_cabac_bits(
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
) -> Vec<bool> {
    debug_assert_eq!(ctus.len(), vvc_picture_ctu_count(picture_geometry));
    let mut cabac = VvcCabacEncoder::new();
    let mut contexts = initial_vvc_cabac_contexts(slice_config);
    let mut params_by_ctu = Vec::with_capacity(ctus.len());
    cabac.start();
    for (expected_slice_address, ctu) in ctus.iter().enumerate() {
        debug_assert_eq!(ctu.slice_address, expected_slice_address);
        let Some(params) = vvc_ctu_partition_params_with_luma_max_leaf_size_and_chroma(
            ctu.geometry,
            ctu.color.clone(),
            ctu.luma_max_leaf_size,
            slice_config.coding_tree.chroma_sampling,
            slice_config.coding_tree.dual_tree_intra,
        ) else {
            debug_assert!(
                false,
                "VVC frame CABAC CTU {} has unsupported coded geometry {}x{}",
                ctu.slice_address,
                ctu.geometry.coded_width(),
                ctu.geometry.coded_height()
            );
            return Vec::new();
        };
        params_by_ctu.push(params);
    }
    encode_frame_partition_body_with_contexts(
        &mut cabac,
        &mut contexts,
        picture_geometry,
        &params_by_ctu,
        slice_config,
    );
    cabac.encode_bin_trm(true);
    cabac.finish()
}

fn vvc_ctu_partition_params_with_luma_max_leaf_size_and_chroma(
    geometry: VvcVideoGeometry,
    color: VvcQuantizedColor,
    luma_max_leaf_size: u16,
    chroma_sampling: ChromaSampling,
    dual_tree_intra: bool,
) -> Option<VvcCtuPartitionParams> {
    let coded = geometry.coded();
    if coded.width > VVC_CTU_SIZE
        || coded.height > VVC_CTU_SIZE
        || coded.width < 8
        || coded.height < 8
    {
        return None;
    }
    let shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: coded.width as u16,
        visible_height: coded.height as u16,
        chroma_sampling,
        dual_tree_intra,
    };
    let chroma_tu_count = if color.chroma_tu_count > 1 {
        color.chroma_tu_count
    } else {
        vvc_chroma_transform_nodes(shape).len()
    };
    let mut chroma_tu_intra_modes = color.chroma_tu_intra_modes;
    if color.chroma_tu_count <= 1 {
        for idx in 0..chroma_tu_count.min(MAX_VVC_CHROMA_TUS) {
            chroma_tu_intra_modes[idx] = color.chroma_tu_intra_modes[0];
        }
    }
    let mut cb_tu_transform_skip = color.cb_tu_transform_skip;
    let mut cr_tu_transform_skip = color.cr_tu_transform_skip;
    let mut chroma_tu_bdpcm_modes = color.chroma_tu_bdpcm_modes;
    if color.chroma_tu_count <= 1 {
        for idx in 0..chroma_tu_count.min(MAX_VVC_CHROMA_TUS) {
            cb_tu_transform_skip[idx] = color.cb_tu_transform_skip[0];
            cr_tu_transform_skip[idx] = color.cr_tu_transform_skip[0];
            chroma_tu_bdpcm_modes[idx] = color.chroma_tu_bdpcm_modes[0];
        }
    }
    let (
        luma_tu_count,
        luma_tu_intra_modes,
        luma_tu_abs_levels,
        luma_tu_negative,
        luma_tu_dc_levels,
        luma_tu_ac_levels,
        luma_tu_has_ac,
        luma_tu_transform_skip,
        luma_tu_bdpcm_modes,
        luma_tu_mrl_index,
        luma_tu_mts_index,
    ) = vvc_luma_residual_arrays_for_geometry(
        coded,
        chroma_sampling,
        dual_tree_intra,
        luma_max_leaf_size,
        color,
    );
    Some(VvcCtuPartitionParams {
        root_width: VVC_CTU_SIZE,
        root_height: VVC_CTU_SIZE,
        visible_width: coded.width,
        visible_height: coded.height,
        chroma_sampling,
        dual_tree_intra,
        luma_max_leaf_size,
        chroma_tu_count,
        luma_tu_count,
        luma_tu_intra_modes,
        luma_tu_abs_levels,
        luma_tu_negative,
        luma_tu_dc_levels,
        luma_tu_ac_levels,
        luma_tu_has_ac,
        luma_tu_transform_skip,
        luma_tu_bdpcm_modes,
        luma_tu_mrl_index,
        luma_tu_mts_index,
        cb_dc_abs_level: color.cb_rem,
        cb_dc_negative: color.u < 128 && color.cb_rem != 0,
        chroma_tu_intra_modes,
        cb_tu_dc_levels: color.cb_tu_dc_levels,
        cr_tu_dc_levels: color.cr_tu_dc_levels,
        cb_tu_ac_levels: color.cb_tu_ac_levels,
        cr_tu_ac_levels: color.cr_tu_ac_levels,
        cb_tu_has_ac: color.cb_tu_has_ac,
        cr_tu_has_ac: color.cr_tu_has_ac,
        cb_tu_transform_skip,
        cr_tu_transform_skip,
        chroma_tu_bdpcm_modes,
    })
}

fn vvc_luma_residual_arrays_for_geometry(
    coded: VvcCodedGeometry,
    chroma_sampling: ChromaSampling,
    dual_tree_intra: bool,
    luma_max_leaf_size: u16,
    color: VvcQuantizedColor,
) -> (
    usize,
    [VvcIntraPredictionMode; MAX_VVC_LUMA_TUS],
    [u8; MAX_VVC_LUMA_TUS],
    [bool; MAX_VVC_LUMA_TUS],
    [i16; MAX_VVC_LUMA_TUS],
    [[i16; VVC_LUMA_AC_COEFFS_PER_TU]; MAX_VVC_LUMA_TUS],
    [bool; MAX_VVC_LUMA_TUS],
    [bool; MAX_VVC_LUMA_TUS],
    [VvcBdpcmMode; MAX_VVC_LUMA_TUS],
    [u8; MAX_VVC_LUMA_TUS],
    [u8; MAX_VVC_LUMA_TUS],
) {
    let mut luma_tu_count = color.luma_tu_count;
    let mut luma_tu_intra_modes = color.luma_tu_intra_modes;
    let mut luma_tu_abs_levels = color.luma_tu_remainders;
    let mut luma_tu_negative = color.luma_tu_negative;
    let mut luma_tu_dc_levels = color.luma_tu_dc_levels;
    let mut luma_tu_ac_levels = color.luma_tu_ac_levels;
    let mut luma_tu_has_ac = color.luma_tu_has_ac;
    let mut luma_tu_transform_skip = color.luma_tu_transform_skip;
    let mut luma_tu_bdpcm_modes = color.luma_tu_bdpcm_modes;
    let mut luma_tu_mrl_index = color.luma_tu_mrl_index;
    let mut luma_tu_mts_index = color.luma_tu_mts_index;
    if color.luma_tu_count > 1 {
        return (
            luma_tu_count,
            luma_tu_intra_modes,
            luma_tu_abs_levels,
            luma_tu_negative,
            luma_tu_dc_levels,
            luma_tu_ac_levels,
            luma_tu_has_ac,
            luma_tu_transform_skip,
            luma_tu_bdpcm_modes,
            luma_tu_mrl_index,
            luma_tu_mts_index,
        );
    }

    let leaf_count =
        vvc_luma_leaf_count(coded, chroma_sampling, dual_tree_intra, luma_max_leaf_size);
    luma_tu_count = leaf_count;
    for idx in 0..leaf_count.min(MAX_VVC_LUMA_TUS) {
        luma_tu_intra_modes[idx] = color.luma_tu_intra_modes[0];
        luma_tu_abs_levels[idx] = color.luma_tu_remainders[0];
        luma_tu_negative[idx] = color.luma_tu_negative[0];
        luma_tu_dc_levels[idx] = color.luma_tu_dc_levels[0];
        luma_tu_ac_levels[idx] = color.luma_tu_ac_levels[0];
        luma_tu_has_ac[idx] = color.luma_tu_has_ac[0];
        luma_tu_transform_skip[idx] = color.luma_tu_transform_skip[0];
        luma_tu_bdpcm_modes[idx] = color.luma_tu_bdpcm_modes[0];
        luma_tu_mrl_index[idx] = color.luma_tu_mrl_index[0];
        luma_tu_mts_index[idx] = color.luma_tu_mts_index[0];
    }
    (
        luma_tu_count,
        luma_tu_intra_modes,
        luma_tu_abs_levels,
        luma_tu_negative,
        luma_tu_dc_levels,
        luma_tu_ac_levels,
        luma_tu_has_ac,
        luma_tu_transform_skip,
        luma_tu_bdpcm_modes,
        luma_tu_mrl_index,
        luma_tu_mts_index,
    )
}

fn vvc_luma_leaf_count(
    coded: VvcCodedGeometry,
    chroma_sampling: ChromaSampling,
    dual_tree_intra: bool,
    luma_max_leaf_size: u16,
) -> usize {
    let shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: coded.width as u16,
        visible_height: coded.height as u16,
        chroma_sampling,
        dual_tree_intra,
    };
    vvc_luma_transform_nodes(shape, luma_max_leaf_size).len()
}
