fn vvc_frame_cabac_payload(
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
    inter_slice: bool,
) -> VvcCabacPayload {
    debug_assert_eq!(ctus.len(), vvc_picture_ctu_count(picture_geometry));
    let mut cabac = VvcCabacEncoder::new_with_payload_capacity(
        picture_geometry
            .coded_width()
            .saturating_mul(picture_geometry.coded_height()),
    );
    let mut frame_state = VvcFrameCtuCabacState::new(picture_geometry, slice_config, inter_slice);
    cabac.start();
    for (expected_slice_address, ctu) in ctus.iter().enumerate() {
        debug_assert_eq!(ctu.slice_address, expected_slice_address);
        match &ctu.payload {
            VvcQuantizedCtuPayload::Intra(params) => {
                frame_state.encode_ctu(&mut cabac, ctu.slice_address, params, slice_config);
            }
            VvcQuantizedCtuPayload::InterSkip => {
                frame_state.encode_inter_skip_ctu(
                    &mut cabac,
                    ctu.slice_address,
                    ctu.geometry,
                    slice_config,
                );
            }
        }
    }
    cabac.encode_bin_trm(true);
    cabac.finish_payload()
}

fn vvc_frame_inter_skip_cabac_payload(
    picture_geometry: VvcVideoGeometry,
    slice_config: VvcSliceSyntaxConfig,
) -> VvcCabacPayload {
    debug_assert!(slice_config.inter_enabled);
    let mut cabac = VvcCabacEncoder::new_with_payload_capacity(
        picture_geometry
            .coded_width()
            .saturating_mul(picture_geometry.coded_height()),
    );
    let mut frame_state = VvcFrameCtuCabacState::new(picture_geometry, slice_config, true);
    cabac.start();
    for region in vvc_ctu_regions(picture_geometry) {
        frame_state.encode_inter_skip_ctu(
            &mut cabac,
            region.slice_address,
            region.geometry,
            slice_config,
        );
    }
    cabac.encode_bin_trm(true);
    cabac.finish_payload()
}

fn vvc_ctu_cabac_payload(
    picture_geometry: VvcVideoGeometry,
    ctu: &VvcQuantizedCtu,
    slice_config: VvcSliceSyntaxConfig,
    inter_slice: bool,
) -> VvcCabacPayload {
    let mut cabac = VvcCabacEncoder::new_with_payload_capacity(
        ctu.geometry
            .coded_width()
            .saturating_mul(ctu.geometry.coded_height()),
    );
    let mut frame_state = VvcFrameCtuCabacState::new(picture_geometry, slice_config, inter_slice);
    cabac.start();
    match &ctu.payload {
        VvcQuantizedCtuPayload::Intra(params) => {
            frame_state.encode_ctu(&mut cabac, ctu.slice_address, params, slice_config);
        }
        VvcQuantizedCtuPayload::InterSkip => {
            frame_state.encode_inter_skip_ctu(
                &mut cabac,
                ctu.slice_address,
                ctu.geometry,
                slice_config,
            );
        }
    }
    cabac.encode_bin_trm(true);
    cabac.finish_payload()
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
    let VvcQuantizedColor {
        y: _,
        u,
        v: _,
        mut luma_tu_intra_modes,
        luma_tu_remainders: mut luma_tu_abs_levels,
        mut luma_tu_negative,
        mut luma_tu_dc_levels,
        mut luma_tu_ac_levels,
        mut luma_tu_has_ac,
        mut luma_tu_scc_decisions,
        mut luma_tu_transform_skip,
        mut luma_tu_bdpcm_modes,
        mut luma_tu_mrl_index,
        mut luma_tu_mts_index,
        mut luma_tu_count,
        chroma_tu_count,
        mut chroma_tu_intra_modes,
        cb_tu_dc_levels,
        cr_tu_dc_levels,
        cb_tu_ac_levels,
        cr_tu_ac_levels,
        cb_tu_has_ac,
        cr_tu_has_ac,
        mut cb_tu_transform_skip,
        mut cr_tu_transform_skip,
        mut chroma_tu_bdpcm_modes,
        cb_rem,
        cr_rem: _,
        ..
    } = color;
    let source_chroma_tu_count = chroma_tu_count;
    let chroma_tu_count = if source_chroma_tu_count > 1 {
        source_chroma_tu_count
    } else {
        vvc_chroma_transform_nodes(shape).len()
    };
    if source_chroma_tu_count <= 1 {
        for idx in 0..chroma_tu_count.min(MAX_VVC_CHROMA_TUS) {
            chroma_tu_intra_modes[idx] = chroma_tu_intra_modes[0];
        }
    }
    if source_chroma_tu_count <= 1 {
        for idx in 0..chroma_tu_count.min(MAX_VVC_CHROMA_TUS) {
            cb_tu_transform_skip[idx] = cb_tu_transform_skip[0];
            cr_tu_transform_skip[idx] = cr_tu_transform_skip[0];
            chroma_tu_bdpcm_modes[idx] = chroma_tu_bdpcm_modes[0];
        }
    }
    if luma_tu_count <= 1 {
        let leaf_count =
            vvc_luma_leaf_count(coded, chroma_sampling, dual_tree_intra, luma_max_leaf_size);
        luma_tu_count = leaf_count;
        for idx in 0..leaf_count.min(MAX_VVC_LUMA_TUS) {
            luma_tu_intra_modes[idx] = luma_tu_intra_modes[0];
            luma_tu_abs_levels[idx] = luma_tu_abs_levels[0];
            luma_tu_negative[idx] = luma_tu_negative[0];
            luma_tu_dc_levels[idx] = luma_tu_dc_levels[0];
            luma_tu_ac_levels[idx] = luma_tu_ac_levels[0];
            luma_tu_has_ac[idx] = luma_tu_has_ac[0];
            luma_tu_scc_decisions[idx] = luma_tu_scc_decisions[0];
            luma_tu_transform_skip[idx] = luma_tu_transform_skip[0];
            luma_tu_bdpcm_modes[idx] = luma_tu_bdpcm_modes[0];
            luma_tu_mrl_index[idx] = luma_tu_mrl_index[0];
            luma_tu_mts_index[idx] = luma_tu_mts_index[0];
        }
    }
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
        luma_tu_inter_skip: [false; MAX_VVC_LUMA_TUS],
        luma_tu_inter_decisions: [None; MAX_VVC_LUMA_TUS],
        luma_tu_scc_decisions,
        luma_tu_transform_skip,
        luma_tu_bdpcm_modes,
        luma_tu_mrl_index,
        luma_tu_mts_index,
        cb_dc_abs_level: cb_rem,
        cb_dc_negative: u < 128 && cb_rem != 0,
        chroma_tu_intra_modes,
        cb_tu_dc_levels,
        cr_tu_dc_levels,
        cb_tu_ac_levels,
        cr_tu_ac_levels,
        cb_tu_has_ac,
        cr_tu_has_ac,
        chroma_tu_inter_skip: [false; MAX_VVC_CHROMA_TUS],
        cb_tu_transform_skip,
        cr_tu_transform_skip,
        chroma_tu_bdpcm_modes,
    })
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
