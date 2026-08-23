use crate::picture::{ChromaSampling, SampleBitDepth};

#[cfg(test)]
use super::vvc_cabac_bits_with_luma_max_leaf_size;
use super::{
    vvc_ctu_cabac_payload, vvc_frame_cabac_payload, vvc_frame_inter_skip_cabac_payload,
    VvcCabacPayload, VvcCodingTreeConfig, VvcNalUnit, VvcNalUnitType, VvcProfile, VvcQuantizedCtu,
    VvcQuantizedCtuPayload, VvcSliceSyntaxConfig, VvcSyntaxRbsp, VvcSyntaxWriter, VvcVideoGeometry,
    VvcVuiSignal, VVC_CURRENT_MAX_LUMA_MTT_DEPTH,
};
#[cfg(test)]
use super::{VvcQuantizedColor, VVC_CURRENT_MAX_LUMA_LEAF_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcPictureKind {
    Idr,
    Cra,
    Trail,
}

const VVC_SPS_LOG2_MAX_POC_LSB_MINUS4: u8 = 12;
pub(in crate::vvc) const VVC_PPS_INIT_QP: i32 = 32;
pub(in crate::vvc) const VVC_POC_LSB_BITS: u8 = VVC_SPS_LOG2_MAX_POC_LSB_MINUS4 + 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcPicturePartitioning {
    SingleSlice,
    OneSlicePerCtu,
}

const VVC_MAX_EXPLICIT_TILE_COLUMNS: usize = 30;
const VVC_MAX_TILES_PER_PICTURE: usize = 990;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcSliceType {
    I,
    P,
}

impl VvcPictureKind {
    pub(in crate::vvc) fn for_frame_idx(frame_idx: usize) -> Self {
        if frame_idx == 0 {
            Self::Idr
        } else {
            Self::Cra
        }
    }

    pub(in crate::vvc) fn nal_unit_type(self) -> VvcNalUnitType {
        match self {
            Self::Idr => VvcNalUnitType::IdrNLp,
            Self::Cra => VvcNalUnitType::Cra,
            Self::Trail => VvcNalUnitType::Trail,
        }
    }

    const fn carries_slice_header_ref_pic_lists(self) -> bool {
        !matches!(self, Self::Idr)
    }

    const fn is_irap(self) -> bool {
        matches!(self, Self::Idr | Self::Cra)
    }
}

impl VvcSliceType {
    pub(in crate::vvc) const fn code(self) -> u32 {
        match self {
            Self::I => 2,
            Self::P => 1,
        }
    }

    const fn is_inter(self) -> bool {
        matches!(self, Self::P)
    }
}

pub(in crate::vvc) fn vvc_poc_lsb_for_frame_idx(frame_idx: usize) -> u32 {
    // H.266/VTM constrain sps_log2_max_pic_order_cnt_lsb_minus4 to 0..=12;
    // value 12 is the largest allowed value and gives a 16-bit POC LSB. Longer
    // sequences wrap the LSB; future reference-list work must add the matching
    // POC MSB/reference logic.
    let modulus = 1usize << VVC_POC_LSB_BITS;
    (frame_idx % modulus) as u32
}

pub(in crate::vvc) fn vvc_sps_unit(
    geometry: VvcVideoGeometry,
    slice_config: VvcSliceSyntaxConfig,
    bit_depth: SampleBitDepth,
) -> VvcNalUnit {
    VvcNalUnit {
        nal_unit_type: VvcNalUnitType::Sps,
        layer_id: 0,
        temporal_id: 0,
        rbsp_payload: vvc_configured_sps_payload(geometry, slice_config, bit_depth),
    }
}

pub(in crate::vvc) fn vvc_pps_unit_with_partitioning_and_config(
    geometry: VvcVideoGeometry,
    partitioning: VvcPicturePartitioning,
    slice_config: VvcSliceSyntaxConfig,
) -> VvcNalUnit {
    VvcNalUnit {
        nal_unit_type: VvcNalUnitType::Pps,
        layer_id: 0,
        temporal_id: 0,
        rbsp_payload: vvc_pps_payload_with_partitioning(geometry, partitioning, slice_config),
    }
}

pub(in crate::vvc) fn vvc_frame_slice_unit(
    frame_idx: usize,
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
) -> Result<VvcNalUnit, String> {
    let picture_kind = VvcPictureKind::for_frame_idx(frame_idx);
    let poc_lsb = vvc_poc_lsb_for_frame_idx(frame_idx);
    let expected_ctus = vvc_picture_ctu_count(picture_geometry);
    if ctus.len() != expected_ctus {
        return Err(format!(
            "VVC frame slice expected {expected_ctus} CTU payload(s), got {}",
            ctus.len()
        ));
    }

    Ok(VvcNalUnit {
        nal_unit_type: picture_kind.nal_unit_type(),
        layer_id: 0,
        temporal_id: 0,
        rbsp_payload: vvc_frame_slice_payload_with_poc(
            picture_kind,
            poc_lsb,
            picture_geometry,
            ctus,
            slice_config,
        ),
    })
}

pub(in crate::vvc) fn vvc_predictive_frame_slice_unit(
    frame_idx: usize,
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
) -> Result<VvcNalUnit, String> {
    let picture_kind = vvc_picture_kind_for_frame_idx(frame_idx, true);
    let poc_lsb = vvc_poc_lsb_for_frame_idx(frame_idx);
    let expected_ctus = vvc_picture_ctu_count(picture_geometry);
    if ctus.len() != expected_ctus {
        return Err(format!(
            "VVC predictive frame slice expected {expected_ctus} CTU payload(s), got {}",
            ctus.len()
        ));
    }

    Ok(VvcNalUnit {
        nal_unit_type: picture_kind.nal_unit_type(),
        layer_id: 0,
        temporal_id: 0,
        rbsp_payload: vvc_frame_slice_payload_with_poc(
            picture_kind,
            poc_lsb,
            picture_geometry,
            ctus,
            slice_config,
        ),
    })
}

pub(in crate::vvc) fn vvc_predictive_ctu_slice_units(
    frame_idx: usize,
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
) -> Result<Vec<VvcNalUnit>, String> {
    let mut inter_skip_cache = VvcCtuInterSkipSlicePayloadCache::default();
    vvc_predictive_ctu_slice_units_with_inter_skip_cache(
        frame_idx,
        picture_geometry,
        ctus,
        slice_config,
        &mut inter_skip_cache,
    )
}

pub(in crate::vvc) fn vvc_predictive_ctu_slice_units_with_inter_skip_cache(
    frame_idx: usize,
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
    inter_skip_cache: &mut VvcCtuInterSkipSlicePayloadCache,
) -> Result<Vec<VvcNalUnit>, String> {
    let picture_kind = vvc_picture_kind_for_frame_idx(frame_idx, true);
    if picture_kind.is_irap() {
        return Err("VVC predictive CTU-slice path requires a non-IRAP picture".to_string());
    }
    let poc_lsb = vvc_poc_lsb_for_frame_idx(frame_idx);
    let expected_ctus = vvc_picture_ctu_count(picture_geometry);
    if ctus.len() != expected_ctus {
        return Err(format!(
            "VVC predictive CTU-slice frame expected {expected_ctus} CTU payload(s), got {}",
            ctus.len()
        ));
    }

    let mut units = Vec::with_capacity(ctus.len() + 1);
    units.push(vvc_picture_header_unit_with_poc(
        picture_kind,
        poc_lsb,
        slice_config,
    ));
    for ctu in ctus {
        if matches!(ctu.payload, VvcQuantizedCtuPayload::InterSkip) {
            units.push(inter_skip_cache.slice_unit_for(
                picture_kind,
                poc_lsb,
                picture_geometry,
                ctu,
                slice_config,
            ));
        } else {
            units.push(vvc_ctu_slice_unit_with_poc(
                picture_kind,
                poc_lsb,
                picture_geometry,
                ctu,
                slice_config,
            ));
        }
    }
    Ok(units)
}

#[cfg(test)]
pub(in crate::vvc) fn vvc_predictive_ctu_slice_units_uncached_for_test(
    frame_idx: usize,
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
) -> Result<Vec<VvcNalUnit>, String> {
    let picture_kind = vvc_picture_kind_for_frame_idx(frame_idx, true);
    if picture_kind.is_irap() {
        return Err("VVC predictive CTU-slice path requires a non-IRAP picture".to_string());
    }
    let poc_lsb = vvc_poc_lsb_for_frame_idx(frame_idx);
    let expected_ctus = vvc_picture_ctu_count(picture_geometry);
    if ctus.len() != expected_ctus {
        return Err(format!(
            "VVC predictive CTU-slice frame expected {expected_ctus} CTU payload(s), got {}",
            ctus.len()
        ));
    }

    let mut units = Vec::with_capacity(ctus.len() + 1);
    units.push(vvc_picture_header_unit_with_poc(
        picture_kind,
        poc_lsb,
        slice_config,
    ));
    for ctu in ctus {
        units.push(vvc_ctu_slice_unit_with_poc(
            picture_kind,
            poc_lsb,
            picture_geometry,
            ctu,
            slice_config,
        ));
    }
    Ok(units)
}

#[cfg(test)]
pub(in crate::vvc) fn vvc_predictive_frame_skip_slice_unit(
    frame_idx: usize,
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
) -> Result<VvcNalUnit, String> {
    let picture_kind = vvc_picture_kind_for_frame_idx(frame_idx, true);
    let poc_lsb = vvc_poc_lsb_for_frame_idx(frame_idx);
    let expected_ctus = vvc_picture_ctu_count(picture_geometry);
    if ctus.len() != expected_ctus {
        return Err(format!(
            "VVC predictive skip frame expected {expected_ctus} CTU payload(s), got {}",
            ctus.len()
        ));
    }
    if picture_kind.is_irap()
        || !ctus
            .iter()
            .all(|ctu| matches!(ctu.payload, VvcQuantizedCtuPayload::InterSkip))
    {
        return Err(
            "VVC predictive frame-slice skip path requires a non-IRAP all-skip picture".to_string(),
        );
    }

    Ok(VvcNalUnit {
        nal_unit_type: picture_kind.nal_unit_type(),
        layer_id: 0,
        temporal_id: 0,
        rbsp_payload: vvc_frame_slice_payload_with_poc(
            picture_kind,
            poc_lsb,
            picture_geometry,
            ctus,
            slice_config,
        ),
    })
}

pub(in crate::vvc) fn vvc_predictive_frame_skip_slice_unit_with_payload(
    frame_idx: usize,
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
    inter_skip_payload: &VvcCabacPayload,
) -> Result<VvcNalUnit, String> {
    let picture_kind = vvc_picture_kind_for_frame_idx(frame_idx, true);
    let expected_ctus = vvc_picture_ctu_count(picture_geometry);
    if ctus.len() != expected_ctus {
        return Err(format!(
            "VVC predictive skip frame expected {expected_ctus} CTU payload(s), got {}",
            ctus.len()
        ));
    }
    if picture_kind.is_irap()
        || !ctus
            .iter()
            .all(|ctu| matches!(ctu.payload, VvcQuantizedCtuPayload::InterSkip))
    {
        return Err(
            "VVC predictive frame-slice skip path requires a non-IRAP all-skip picture".to_string(),
        );
    }

    vvc_predictive_frame_skip_slice_unit_with_cached_payload(
        frame_idx,
        picture_geometry,
        slice_config,
        inter_skip_payload,
    )
}

pub(in crate::vvc) fn vvc_predictive_frame_skip_slice_unit_with_cached_payload(
    frame_idx: usize,
    picture_geometry: VvcVideoGeometry,
    slice_config: VvcSliceSyntaxConfig,
    inter_skip_payload: &VvcCabacPayload,
) -> Result<VvcNalUnit, String> {
    let picture_kind = vvc_picture_kind_for_frame_idx(frame_idx, true);
    if picture_kind.is_irap() {
        return Err(
            "VVC predictive frame-slice skip path requires a non-IRAP all-skip picture".to_string(),
        );
    }
    let poc_lsb = vvc_poc_lsb_for_frame_idx(frame_idx);
    Ok(VvcNalUnit {
        nal_unit_type: picture_kind.nal_unit_type(),
        layer_id: 0,
        temporal_id: 0,
        rbsp_payload: vvc_frame_slice_payload_with_poc_and_cabac_payload(
            picture_kind,
            poc_lsb,
            picture_geometry,
            slice_config,
            inter_skip_payload,
        ),
    })
}

#[derive(Default)]
pub(in crate::vvc) struct VvcFrameSkipPayloadCache {
    entries: Vec<(VvcVideoGeometry, VvcSliceSyntaxConfig, VvcCabacPayload)>,
}

impl VvcFrameSkipPayloadCache {
    pub(in crate::vvc) fn payload_for(
        &mut self,
        picture_geometry: VvcVideoGeometry,
        slice_config: VvcSliceSyntaxConfig,
    ) -> &VvcCabacPayload {
        if let Some(index) = self.entries.iter().position(|(geometry, config, _)| {
            *geometry == picture_geometry && *config == slice_config
        }) {
            return &self.entries[index].2;
        }
        self.entries.push((
            picture_geometry,
            slice_config,
            vvc_frame_inter_skip_cabac_payload(picture_geometry, slice_config),
        ));
        &self
            .entries
            .last()
            .expect("just pushed frame-skip payload")
            .2
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcCtuInterSkipSlicePayloadCacheKey {
    picture_kind: VvcPictureKind,
    picture_geometry: VvcVideoGeometry,
    ctu_geometry: VvcVideoGeometry,
    slice_address: usize,
    slice_config: VvcSliceSyntaxConfig,
}

#[derive(Default)]
pub(in crate::vvc) struct VvcCtuInterSkipSlicePayloadCache {
    entries: Vec<(VvcCtuInterSkipSlicePayloadCacheKey, Vec<u8>)>,
}

impl VvcCtuInterSkipSlicePayloadCache {
    fn slice_unit_for(
        &mut self,
        picture_kind: VvcPictureKind,
        poc_lsb: u32,
        picture_geometry: VvcVideoGeometry,
        ctu: &VvcQuantizedCtu,
        slice_config: VvcSliceSyntaxConfig,
    ) -> VvcNalUnit {
        if vvc_picture_ctu_count(picture_geometry) <= 1 {
            return VvcNalUnit {
                nal_unit_type: picture_kind.nal_unit_type(),
                layer_id: 0,
                temporal_id: 0,
                rbsp_payload: vvc_ctu_slice_payload_with_poc(
                    picture_kind,
                    poc_lsb,
                    picture_geometry,
                    ctu,
                    slice_config,
                ),
            };
        }
        VvcNalUnit {
            nal_unit_type: picture_kind.nal_unit_type(),
            layer_id: 0,
            temporal_id: 0,
            rbsp_payload: self
                .payload_for(picture_kind, poc_lsb, picture_geometry, ctu, slice_config)
                .to_vec(),
        }
    }

    fn payload_for(
        &mut self,
        picture_kind: VvcPictureKind,
        poc_lsb: u32,
        picture_geometry: VvcVideoGeometry,
        ctu: &VvcQuantizedCtu,
        slice_config: VvcSliceSyntaxConfig,
    ) -> &[u8] {
        debug_assert!(matches!(ctu.payload, VvcQuantizedCtuPayload::InterSkip));
        debug_assert!(vvc_picture_ctu_count(picture_geometry) > 1);
        let key = VvcCtuInterSkipSlicePayloadCacheKey {
            picture_kind,
            picture_geometry,
            ctu_geometry: ctu.geometry,
            slice_address: ctu.slice_address,
            slice_config,
        };
        if let Some(index) = self
            .entries
            .iter()
            .position(|(entry_key, _)| *entry_key == key)
        {
            return &self.entries[index].1;
        }
        self.entries.push((
            key,
            vvc_ctu_slice_payload_with_poc(
                picture_kind,
                poc_lsb,
                picture_geometry,
                ctu,
                slice_config,
            ),
        ));
        &self
            .entries
            .last()
            .expect("just pushed CTU InterSkip payload")
            .1
    }
}

fn vvc_picture_kind_for_frame_idx(frame_idx: usize, predictive: bool) -> VvcPictureKind {
    if predictive && frame_idx > 0 {
        VvcPictureKind::Trail
    } else {
        VvcPictureKind::for_frame_idx(frame_idx)
    }
}

#[cfg(test)]
pub(in crate::vvc) fn vvc_sps_payload(geometry: VvcVideoGeometry) -> Vec<u8> {
    vvc_configured_sps_payload(
        geometry,
        VvcSliceSyntaxConfig::yuv420_residual(),
        SampleBitDepth::new(8).expect("valid bit depth"),
    )
}

fn vvc_configured_sps_payload(
    geometry: VvcVideoGeometry,
    slice_config: VvcSliceSyntaxConfig,
    bit_depth: SampleBitDepth,
) -> Vec<u8> {
    vvc_sps_rbsp(geometry, slice_config, bit_depth).bytes
}

pub(in crate::vvc) fn vvc_sps_rbsp(
    geometry: VvcVideoGeometry,
    slice_config: VvcSliceSyntaxConfig,
    bit_depth: SampleBitDepth,
) -> VvcSyntaxRbsp {
    let mut writer = VvcSyntaxWriter::new();
    let config = slice_config.coding_tree;
    let tool_flags = slice_config.tools;
    let palette_enabled = tool_flags.palette_enabled;
    writer.write_u("sps_seq_parameter_set_id", 0, 4);
    writer.write_u("sps_video_parameter_set_id", 0, 4);
    writer.write_u("sps_max_sub_layers_minus1", 0, 3);
    writer.write_u(
        "sps_chroma_format_idc",
        chroma_format_idc(config.chroma_sampling) as u64,
        2,
    );
    let sps_log2_ctu_size_minus5: u32 = 1;
    let ctu_log2_size = sps_log2_ctu_size_minus5 + 5;
    writer.write_u(
        "sps_log2_ctu_size_minus5",
        u64::from(sps_log2_ctu_size_minus5),
        2,
    );
    writer.write_flag("sps_ptl_dpb_hrd_params_present_flag", true);
    writer.write_u(
        "general_profile_idc",
        vvc_general_profile_idc(slice_config.profile, bit_depth) as u64,
        7,
    );
    writer.write_flag("general_tier_flag", false);
    writer.write_u("general_level_idc", 0, 8);
    writer.write_flag("ptl_frame_only_constraint_flag", true);
    writer.write_flag("ptl_multilayer_enabled_flag", false);
    writer.write_flag("gci_present_flag", false);
    for _ in 0..5 {
        writer.write_flag("gci_alignment_zero_bit", false);
    }
    writer.write_u("ptl_num_sub_profiles", 0, 8);
    writer.write_flag("sps_gdr_enabled_flag", false);
    writer.write_flag(
        "sps_ref_pic_resampling_enabled_flag",
        slice_config.ref_pic_resampling_enabled,
    );
    if slice_config.ref_pic_resampling_enabled {
        writer.write_flag("sps_res_change_in_clvs_allowed_flag", false);
    }
    writer.write_ue(
        "sps_pic_width_max_in_luma_samples",
        geometry.coded_width() as u32,
    );
    writer.write_ue(
        "sps_pic_height_max_in_luma_samples",
        geometry.coded_height() as u32,
    );
    writer.write_flag("sps_conformance_window_flag", true);
    writer.write_ue("sps_conf_win_left_offset", 0);
    writer.write_ue(
        "sps_conf_win_right_offset",
        geometry.crop_right(config.chroma_sampling),
    );
    writer.write_ue("sps_conf_win_top_offset", 0);
    writer.write_ue(
        "sps_conf_win_bottom_offset",
        geometry.crop_bottom(config.chroma_sampling),
    );
    writer.write_flag("sps_subpic_info_present_flag", false);
    writer.write_ue(
        "sps_bitdepth_minus8",
        u32::from(bit_depth.bits().saturating_sub(8)),
    );
    writer.write_flag("sps_entropy_coding_sync_enabled_flag", false);
    writer.write_flag(
        "sps_entry_point_offsets_present_flag",
        slice_config.entry_point_offsets_present,
    );
    writer.write_u(
        "sps_log2_max_pic_order_cnt_lsb_minus4",
        u64::from(VVC_SPS_LOG2_MAX_POC_LSB_MINUS4),
        4,
    );
    writer.write_flag("sps_poc_msb_cycle_flag", false);
    writer.write_u("sps_num_extra_ph_bytes", 0, 2);
    writer.write_u("sps_num_extra_sh_bytes", 0, 2);
    writer.write_ue("dpb_max_dec_pic_buffering_minus1[i]", 0);
    writer.write_ue("dpb_max_num_reorder_pics[i]", 0);
    writer.write_ue("dpb_max_latency_increase_plus1[i]", 0);
    writer.write_ue("sps_log2_min_luma_coding_block_size_minus2", 0);
    writer.write_flag("sps_partition_constraints_override_enabled_flag", true);
    writer.write_ue("sps_log2_diff_min_qt_min_cb_intra_slice_luma", 1);
    writer.write_ue(
        "sps_max_mtt_hierarchy_depth_intra_slice_luma",
        u32::from(VVC_CURRENT_MAX_LUMA_MTT_DEPTH),
    );
    writer.write_ue("sps_log2_diff_max_bt_min_qt_intra_slice_luma", 2);
    writer.write_ue("sps_log2_diff_max_tt_min_qt_intra_slice_luma", 2);
    // sps_qtbtt_dual_tree_intra_flag is a coding-tree policy choice. The
    // residual path uses dual-tree partitioning across chroma formats; palette
    // remains a single-tree 4:4:4 tool.
    let dual_tree_intra =
        config.dual_tree_intra && config.chroma_sampling != ChromaSampling::Monochrome;
    writer.write_flag("sps_qtbtt_dual_tree_intra_flag", dual_tree_intra);
    if dual_tree_intra {
        writer.write_ue("sps_log2_diff_min_qt_min_cb_intra_slice_chroma", 1);
        writer.write_ue("sps_max_mtt_hierarchy_depth_intra_slice_chroma", 3);
        writer.write_ue(
            "sps_log2_diff_max_bt_min_qt_intra_slice_chroma",
            (ctu_log2_size - 3).min(3),
        );
        writer.write_ue("sps_log2_diff_max_tt_min_qt_intra_slice_chroma", 2);
    }
    writer.write_ue("sps_log2_diff_min_qt_min_cb_inter_slice", 1);
    writer.write_ue("sps_max_mtt_hierarchy_depth_inter_slice", 3);
    writer.write_ue(
        "sps_log2_diff_max_bt_min_qt_inter_slice",
        (ctu_log2_size - 3).min(3),
    );
    writer.write_ue(
        "sps_log2_diff_max_tt_min_qt_inter_slice",
        (ctu_log2_size - 3).min(3),
    );
    writer.write_flag("sps_max_luma_transform_size_64_flag", true);
    writer.write_flag(
        "sps_transform_skip_enabled_flag",
        tool_flags.transform_skip_enabled,
    );
    if tool_flags.transform_skip_enabled {
        writer.write_ue("sps_log2_transform_skip_max_size_minus2", 1);
        writer.write_flag("sps_bdpcm_enabled_flag", tool_flags.bdpcm_enabled);
    }
    let mts_enabled = tool_flags.mts_enabled();
    writer.write_flag("sps_mts_enabled_flag", mts_enabled);
    if mts_enabled {
        writer.write_flag(
            "sps_explicit_mts_intra_enabled_flag",
            tool_flags.explicit_mts_intra_enabled,
        );
        writer.write_flag("sps_explicit_mts_inter_enabled_flag", false);
    }
    writer.write_flag("sps_lfnst_enabled_flag", tool_flags.lfnst_enabled);
    writer.write_flag("sps_joint_cbcr_enabled_flag", tool_flags.joint_cbcr_enabled);
    writer.write_flag("sps_same_qp_table_for_chroma_flag", true);
    writer.write_se("sps_qp_table_starts_minus26", -9);
    writer.write_ue("sps_num_points_in_qp_table_minus1", 2);
    writer.write_ue("sps_delta_qp_in_val_minus1", 9);
    writer.write_ue("sps_delta_qp_diff_val", 5);
    writer.write_ue("sps_delta_qp_in_val_minus1", 4);
    writer.write_ue("sps_delta_qp_diff_val", 1);
    writer.write_ue("sps_delta_qp_in_val_minus1", 11);
    writer.write_ue("sps_delta_qp_diff_val", 12);
    writer.write_flag("sps_sao_enabled_flag", false);
    writer.write_flag("sps_alf_enabled_flag", false);
    writer.write_flag("sps_lmcs_enable_flag", false);
    writer.write_flag("sps_weighted_pred_flag", false);
    writer.write_flag("sps_weighted_bipred_flag", false);
    writer.write_flag("sps_long_term_ref_pics_flag", false);
    writer.write_flag("sps_idr_rpl_present_flag", false);
    writer.write_flag("sps_rpl1_same_as_rpl0_flag", true);
    writer.write_ue("sps_num_ref_pic_lists[0]", 1);
    if slice_config.inter_enabled {
        writer.write_ue("num_ref_entries[listIdx][rplsIdx]", 1);
        // One short-term reference to the previous POC:
        // DeltaPocValSt = -(abs_delta_poc_st + 1) = -1.
        writer.write_ue("abs_delta_poc_st[i]", 0);
        writer.write_flag("strp_entry_sign_flag[i]", true);
    } else {
        writer.write_ue("num_ref_entries[listIdx][rplsIdx]", 0);
    }
    writer.write_flag("sps_ref_wraparound_enabled_flag", false);
    let temporal_mvp_enabled = false;
    writer.write_flag("sps_temporal_mvp_enabled_flag", temporal_mvp_enabled);
    if temporal_mvp_enabled {
        writer.write_flag("sps_sbtmvp_enabled_flag", false);
    }
    let amvr_enabled = false;
    writer.write_flag("sps_amvr_enabled_flag", amvr_enabled);
    writer.write_flag("sps_bdof_enabled_flag", false);
    writer.write_flag("sps_smvd_enabled_flag", false);
    writer.write_flag("sps_dmvr_enabled_flag", false);
    let mmvd_enabled = false;
    writer.write_flag("sps_mmvd_enabled_flag", mmvd_enabled);
    if mmvd_enabled {
        writer.write_flag("sps_mmvd_fullpel_only_flag", false);
    }
    let sps_six_minus_max_num_merge_cand = if slice_config.inter_enabled { 5 } else { 0 };
    writer.write_ue(
        "sps_six_minus_max_num_merge_cand",
        sps_six_minus_max_num_merge_cand,
    );
    writer.write_flag("sps_sbt_enabled_flag", false);
    let affine_enabled = false;
    writer.write_flag("sps_affine_enabled_flag", affine_enabled);
    if affine_enabled {
        writer.write_ue("sps_five_minus_max_num_subblock_merge_cand", 0);
        writer.write_flag("sps_affine_type_flag", false);
        if amvr_enabled {
            writer.write_flag("sps_affine_amvr_enabled_flag", false);
        }
        writer.write_flag("sps_affine_prof_enabled_flag", false);
    }
    writer.write_flag("sps_bcw_enabled_flag", false);
    writer.write_flag("sps_ciip_enabled_flag", false);
    let max_num_merge_cand = 6 - sps_six_minus_max_num_merge_cand;
    if max_num_merge_cand >= 2 {
        writer.write_flag("sps_gpm_enabled_flag", false);
    }
    writer.write_ue("sps_log2_parallel_merge_level_minus2", 0);
    writer.write_flag("sps_isp_enabled_flag", tool_flags.isp_enabled);
    writer.write_flag("sps_mrl_enabled_flag", tool_flags.mrl_enabled);
    writer.write_flag("sps_mip_enabled_flag", tool_flags.mip_enabled);
    if config.chroma_sampling != ChromaSampling::Monochrome {
        writer.write_flag("sps_cclm_enabled_flag", tool_flags.cclm_enabled);
    }
    if config.chroma_sampling == ChromaSampling::Cs420 {
        writer.write_flag("sps_chroma_horizontal_collocated_flag", true);
        writer.write_flag("sps_chroma_vertical_collocated_flag", false);
    }
    writer.write_flag("sps_palette_enabled_flag", palette_enabled);
    if palette_enabled || tool_flags.transform_skip_enabled {
        writer.write_ue("sps_internal_bit_depth_minus_input_bit_depth", 0);
    }
    writer.write_flag("sps_ibc_enabled_flag", tool_flags.ibc_enabled);
    if tool_flags.ibc_enabled {
        // H.266 7.3.2.3: sps_six_minus_max_num_ibc_merge_cand sets
        // MaxNumIbcMergeCand. Keep it at one candidate for the first
        // CTU-local hash-search subset so mvp_l0_flag is inferred and the
        // explicit BVD remains the only candidate-selection signal.
        writer.write_ue("sps_six_minus_max_num_ibc_merge_cand", 5);
    }
    writer.write_flag("sps_ladf_enabled_flag", false);
    writer.write_flag("sps_explicit_scaling_list_enabled_flag", false);
    writer.write_flag(
        "sps_dep_quant_enabled_flag",
        tool_flags.dependent_quantization_enabled,
    );
    writer.write_flag(
        "sps_sign_data_hiding_enabled_flag",
        tool_flags.sign_data_hiding_enabled,
    );
    writer.write_flag("sps_virtual_boundaries_enabled_flag", false);
    writer.write_flag("sps_timing_hrd_params_present_flag", false);
    writer.write_flag("sps_field_seq_flag", false);
    writer.write_flag(
        "sps_vui_parameters_present_flag",
        slice_config.vui_signal.is_some(),
    );
    if let Some(vui_signal) = slice_config.vui_signal {
        let vui_payload_size = vvc_vui_payload_size_bytes(vui_signal);
        writer.write_ue("sps_vui_payload_size_minus1", (vui_payload_size - 1) as u32);
        writer.byte_align_zero("sps_vui_alignment_zero_bit");
        debug_assert!(writer.is_byte_aligned());
        write_vvc_vui_parameters(&mut writer, vui_signal);
    }
    writer.write_flag("sps_extension_present_flag", false);
    writer.rbsp_trailing_bits();
    debug_assert!(writer.is_byte_aligned());
    writer.finish()
}

fn vvc_vui_payload_size_bytes(vui_signal: VvcVuiSignal) -> usize {
    let mut writer = VvcSyntaxWriter::new();
    write_vvc_vui_parameters(&mut writer, vui_signal);
    debug_assert!(writer.is_byte_aligned());
    writer.into_bytes().len()
}

fn write_vvc_vui_parameters(writer: &mut VvcSyntaxWriter, vui_signal: VvcVuiSignal) {
    writer.write_flag("vui_progressive_source_flag", vui_signal.progressive_source);
    writer.write_flag("vui_interlaced_source_flag", vui_signal.interlaced_source);
    writer.write_flag("vui_non_packed_constraint_flag", vui_signal.non_packed);
    writer.write_flag(
        "vui_non_projected_constraint_flag",
        vui_signal.non_projected,
    );
    writer.write_flag("vui_aspect_ratio_info_present_flag", false);
    writer.write_flag("vui_overscan_info_present_flag", false);
    writer.write_flag("vui_colour_description_present_flag", true);
    writer.write_u(
        "vui_colour_primaries",
        u64::from(vui_signal.colour_primaries),
        8,
    );
    writer.write_u(
        "vui_transfer_characteristics",
        u64::from(vui_signal.transfer_characteristics),
        8,
    );
    writer.write_u("vui_matrix_coeffs", u64::from(vui_signal.matrix_coeffs), 8);
    writer.write_flag("vui_full_range_flag", vui_signal.full_range);
    writer.write_flag("vui_chroma_loc_info_present_flag", false);
    if !writer.is_byte_aligned() {
        writer.write_flag("vui_payload_bit_equal_to_one", true);
        writer.byte_align_zero("vui_payload_bit_equal_to_zero");
    }
}

pub(in crate::vvc) fn write_vvc_picture_header(
    writer: &mut VvcSyntaxWriter,
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    slice_config: VvcSliceSyntaxConfig,
) {
    let inter_slice_allowed = slice_config.inter_enabled && !picture_kind.is_irap();
    writer.write_flag("ph_gdr_or_irap_pic_flag", picture_kind.is_irap());
    writer.write_flag("ph_non_ref_pic_flag", false);
    if picture_kind.is_irap() {
        writer.write_flag("ph_gdr_pic_flag", false);
    }
    writer.write_flag("ph_inter_slice_allowed_flag", inter_slice_allowed);
    if inter_slice_allowed {
        writer.write_flag("ph_intra_slice_allowed_flag", true);
    }
    writer.write_ue(
        "ph_pic_parameter_set_id",
        u32::from(slice_config.picture_parameter_set_id),
    );
    writer.write_u("ph_pic_order_cnt_lsb", u64::from(poc_lsb), VVC_POC_LSB_BITS);
    writer.write_flag("ph_partition_constraints_override_flag", false);
    if inter_slice_allowed {
        writer.write_flag("ph_mvd_l1_zero_flag", true);
    }
    if vvc_picture_header_carries_slice_state(slice_config) {
        write_vvc_picture_header_ref_pic_lists(writer, picture_kind);
        writer.write_se("ph_qp_delta", slice_config.slice_qp - VVC_PPS_INIT_QP);
    }
    if slice_config.tools.joint_cbcr_enabled {
        writer.write_flag("ph_joint_cbcr_sign_flag", false);
    }
}

pub(in crate::vvc) fn write_vvc_slice_header_byte_alignment(writer: &mut VvcSyntaxWriter) {
    // H.266 7.3.7 slice_header(): the header terminates with exactly one
    // byte_alignment() syntax structure before CABAC-coded slice data.
    writer.write_flag("cabac_alignment_one_bit", true);
    writer.byte_align_zero("cabac_alignment_zero_bit");
}

pub(in crate::vvc) fn write_vvc_slice_header_ref_pic_lists(
    writer: &mut VvcSyntaxWriter,
    picture_kind: VvcPictureKind,
) {
    write_vvc_ref_pic_lists(writer, picture_kind);
}

fn write_vvc_picture_header_ref_pic_lists(
    writer: &mut VvcSyntaxWriter,
    picture_kind: VvcPictureKind,
) {
    write_vvc_ref_pic_lists(writer, picture_kind);
}

fn write_vvc_ref_pic_lists(writer: &mut VvcSyntaxWriter, picture_kind: VvcPictureKind) {
    if picture_kind.carries_slice_header_ref_pic_lists() {
        // The current SPS/PPS subset needs only rpl_sps_flag[0] at whichever
        // syntax site the active PPS selects. The SPS carries one short-term
        // RPL0, sps_rpl1_same_as_rpl0_flag=1, and pps_rpl1_idx_present_flag=0.
        writer.write_flag("rpl_sps_flag[0]", true);
    }
}

const fn vvc_picture_header_carries_slice_state(slice_config: VvcSliceSyntaxConfig) -> bool {
    slice_config.picture_header_slice_state_enabled
}

fn chroma_format_idc(chroma_sampling: ChromaSampling) -> u32 {
    match chroma_sampling {
        ChromaSampling::Monochrome => 0,
        ChromaSampling::Cs420 => 1,
        ChromaSampling::Cs422 => 2,
        ChromaSampling::Cs444 => 3,
    }
}

fn vvc_general_profile_idc(profile: VvcProfile, bit_depth: SampleBitDepth) -> u32 {
    profile
        .effective_for_bit_depth(bit_depth)
        .expect("slice config profile should be valid for SPS bit depth")
        .general_profile_idc()
}

fn vvc_pps_payload_with_partitioning(
    geometry: VvcVideoGeometry,
    partitioning: VvcPicturePartitioning,
    slice_config: VvcSliceSyntaxConfig,
) -> Vec<u8> {
    vvc_pps_rbsp_with_partitioning_and_config(geometry, partitioning, slice_config).bytes
}

#[cfg(test)]
pub(in crate::vvc) fn vvc_pps_rbsp(geometry: VvcVideoGeometry) -> VvcSyntaxRbsp {
    vvc_pps_rbsp_with_partitioning_and_config(
        geometry,
        VvcPicturePartitioning::OneSlicePerCtu,
        VvcSliceSyntaxConfig::yuv420_residual(),
    )
}

pub(in crate::vvc) fn vvc_pps_rbsp_with_partitioning_and_config(
    geometry: VvcVideoGeometry,
    partitioning: VvcPicturePartitioning,
    slice_config: VvcSliceSyntaxConfig,
) -> VvcSyntaxRbsp {
    let mut writer = VvcSyntaxWriter::new();
    let ctu_cols = vvc_picture_ctu_cols(geometry);
    let ctu_rows = vvc_picture_ctu_rows(geometry);
    let has_multiple_ctus = ctu_cols * ctu_rows > 1;
    let use_picture_partitioning = has_multiple_ctus
        && partitioning == VvcPicturePartitioning::OneSlicePerCtu
        && vvc_one_slice_per_ctu_partitioning_supported(geometry);
    let picture_header_carries_slice_state =
        use_picture_partitioning && vvc_picture_header_carries_slice_state(slice_config);
    writer.write_u(
        "pps_pic_parameter_set_id",
        u64::from(slice_config.picture_parameter_set_id),
        6,
    );
    writer.write_u("pps_seq_parameter_set_id", 0, 4);
    writer.write_flag("pps_mixed_nalu_types_in_pic_flag", false);
    writer.write_ue(
        "pps_pic_width_in_luma_samples",
        geometry.coded_width() as u32,
    );
    writer.write_ue(
        "pps_pic_height_in_luma_samples",
        geometry.coded_height() as u32,
    );
    writer.write_flag("pps_conformance_window_flag", false);
    writer.write_flag("pps_scaling_window_explicit_signalling_flag", false);
    writer.write_flag("pps_output_flag_present_flag", false);
    writer.write_flag("pps_no_pic_partition_flag", !use_picture_partitioning);
    writer.write_flag("pps_subpic_id_mapping_present_flag", false);
    if use_picture_partitioning {
        // One tile per 64x64 CTU, one rectangular slice per tile. This keeps
        // each CABAC body local to a CTU while avoiding any full-picture
        // working buffer in the current software encoder.
        let slice_count = ctu_cols * ctu_rows;
        writer.write_u("pps_log2_ctu_size_minus5", 1, 2);
        writer.write_ue("pps_num_exp_tile_columns_minus1", ctu_cols as u32 - 1);
        writer.write_ue("pps_num_exp_tile_rows_minus1", ctu_rows as u32 - 1);
        for _ in 0..ctu_cols {
            writer.write_ue("pps_tile_column_width_minus1[i]", 0);
        }
        for _ in 0..ctu_rows {
            writer.write_ue("pps_tile_row_height_minus1[i]", 0);
        }
        writer.write_flag("pps_loop_filter_across_tiles_enabled_flag", false);
        writer.write_flag("pps_rect_slice_flag", true);
        writer.write_flag("pps_single_slice_per_subpic_flag", false);
        writer.write_ue("pps_num_slices_in_pic_minus1", slice_count as u32 - 1);
        if slice_count - 1 > 1 {
            writer.write_flag("pps_tile_idx_delta_present_flag", false);
        }
        let mut tile_idx = 0usize;
        for _slice_idx in 0..slice_count - 1 {
            if tile_idx % ctu_cols != ctu_cols - 1 {
                writer.write_ue("pps_slice_width_in_tiles_minus1[i]", 0);
            }
            if tile_idx / ctu_cols != ctu_rows - 1 && tile_idx % ctu_cols == 0 {
                writer.write_ue("pps_slice_height_in_tiles_minus1[i]", 0);
            }
            tile_idx += 1;
        }
        writer.write_flag("pps_loop_filter_across_slices_enabled_flag", false);
    }
    writer.write_flag("pps_cabac_init_present_flag", false);
    writer.write_ue(
        "pps_num_ref_idx_default_active_minus1[0]",
        if slice_config.inter_enabled { 0 } else { 3 },
    );
    writer.write_ue(
        "pps_num_ref_idx_default_active_minus1[1]",
        if slice_config.inter_enabled { 0 } else { 3 },
    );
    writer.write_flag("pps_rpl1_idx_present_flag", false);
    writer.write_flag("pps_weighted_pred_flag", false);
    writer.write_flag("pps_weighted_bipred_flag", false);
    writer.write_flag("pps_ref_wraparound_enabled_flag", false);
    writer.write_se("pps_init_qp_minus26", 6);
    writer.write_flag("pps_cu_qp_delta_enabled_flag", false);
    writer.write_flag("pps_chroma_tool_offsets_present_flag", true);
    writer.write_se("pps_cb_qp_offset", 0);
    writer.write_se("pps_cr_qp_offset", 0);
    writer.write_flag("pps_joint_cbcr_qp_offset_present_flag", true);
    writer.write_se("pps_joint_cbcr_qp_offset_value", -1);
    writer.write_flag("pps_slice_chroma_qp_offsets_present_flag", false);
    writer.write_flag("pps_cu_chroma_qp_offset_list_enabled_flag", false);
    writer.write_flag("pps_deblocking_filter_control_present_flag", true);
    writer.write_flag("pps_deblocking_filter_override_enabled_flag", false);
    writer.write_flag("pps_deblocking_filter_disabled_flag", true);
    if use_picture_partitioning {
        // H.266 picture_parameter_set_rbsp(): when picture partitioning is
        // present, these flags declare whether later syntax is carried in the
        // picture header. Predictive CTU-sliced pictures already emit a
        // separate picture header, so carry RPL/QP once there instead of
        // repeating it in every CTU slice header.
        writer.write_flag(
            "pps_rpl_info_in_ph_flag",
            picture_header_carries_slice_state,
        );
        writer.write_flag("pps_sao_info_in_ph_flag", false);
        writer.write_flag("pps_alf_info_in_ph_flag", false);
        writer.write_flag(
            "pps_qp_delta_info_in_ph_flag",
            picture_header_carries_slice_state,
        );
    }
    writer.write_flag("pps_picture_header_extension_present_flag", false);
    writer.write_flag("pps_slice_header_extension_present_flag", false);
    writer.write_flag("pps_extension_flag", false);
    writer.rbsp_trailing_bits();
    debug_assert!(writer.is_byte_aligned());
    writer.finish()
}

#[cfg(test)]
pub(in crate::vvc) fn vvc_slice_payload(
    picture_kind: VvcPictureKind,
    geometry: VvcVideoGeometry,
    color: VvcQuantizedColor,
    slice_config: VvcSliceSyntaxConfig,
) -> Vec<u8> {
    vvc_slice_payload_with_poc(
        picture_kind,
        vvc_test_poc_lsb(picture_kind),
        geometry,
        0,
        geometry,
        color,
        slice_config,
        VVC_CURRENT_MAX_LUMA_LEAF_SIZE,
    )
}

#[cfg(test)]
pub(in crate::vvc) fn vvc_slice_payload_with_poc(
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    picture_geometry: VvcVideoGeometry,
    slice_address: usize,
    ctu_geometry: VvcVideoGeometry,
    color: VvcQuantizedColor,
    slice_config: VvcSliceSyntaxConfig,
    luma_max_leaf_size: u16,
) -> Vec<u8> {
    vvc_slice_rbsp_with_poc(
        picture_kind,
        poc_lsb,
        picture_geometry,
        slice_address,
        ctu_geometry,
        color,
        slice_config,
        luma_max_leaf_size,
    )
    .bytes
}

fn vvc_frame_slice_payload_with_poc(
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
) -> Vec<u8> {
    vvc_frame_slice_rbsp_with_poc(picture_kind, poc_lsb, picture_geometry, ctus, slice_config).bytes
}

fn vvc_picture_header_unit_with_poc(
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    slice_config: VvcSliceSyntaxConfig,
) -> VvcNalUnit {
    VvcNalUnit {
        nal_unit_type: VvcNalUnitType::PictureHeader,
        layer_id: 0,
        temporal_id: 0,
        rbsp_payload: vvc_picture_header_payload_with_poc(picture_kind, poc_lsb, slice_config),
    }
}

fn vvc_picture_header_payload_with_poc(
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    slice_config: VvcSliceSyntaxConfig,
) -> Vec<u8> {
    let mut writer = VvcSyntaxWriter::without_fields();
    write_vvc_picture_header(&mut writer, picture_kind, poc_lsb, slice_config);
    writer.rbsp_trailing_bits();
    debug_assert!(writer.is_byte_aligned());
    writer.into_bytes()
}

fn vvc_ctu_slice_unit_with_poc(
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    picture_geometry: VvcVideoGeometry,
    ctu: &VvcQuantizedCtu,
    slice_config: VvcSliceSyntaxConfig,
) -> VvcNalUnit {
    VvcNalUnit {
        nal_unit_type: picture_kind.nal_unit_type(),
        layer_id: 0,
        temporal_id: 0,
        rbsp_payload: vvc_ctu_slice_payload_with_poc(
            picture_kind,
            poc_lsb,
            picture_geometry,
            ctu,
            slice_config,
        ),
    }
}

fn vvc_ctu_slice_payload_with_poc(
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    picture_geometry: VvcVideoGeometry,
    ctu: &VvcQuantizedCtu,
    slice_config: VvcSliceSyntaxConfig,
) -> Vec<u8> {
    let mut writer = VvcSyntaxWriter::without_fields();
    let slice_type = vvc_slice_type_for_ctu(picture_kind, ctu);
    let tool_flags = slice_config.tools;
    let slice_count = vvc_picture_ctu_count(picture_geometry);
    let include_picture_header = slice_count == 1;
    writer.write_flag(
        "sh_picture_header_in_slice_header_flag",
        include_picture_header,
    );
    if include_picture_header {
        write_vvc_picture_header(&mut writer, picture_kind, poc_lsb, slice_config);
    }
    if slice_count > 1 {
        writer.write_u(
            "sh_slice_address",
            ctu.slice_address as u64,
            vvc_slice_address_bits(picture_geometry),
        );
    }
    if slice_config.inter_enabled && !picture_kind.is_irap() {
        writer.write_ue("sh_slice_type", slice_type.code());
    }
    if picture_kind.is_irap() {
        writer.write_flag("sh_no_output_of_prior_pics_flag", false);
    }
    if !vvc_picture_header_carries_slice_state(slice_config) {
        write_vvc_slice_header_ref_pic_lists(&mut writer, picture_kind);
        writer.write_se("sh_qp_delta", slice_config.slice_qp - VVC_PPS_INIT_QP);
    }
    if tool_flags.dependent_quantization_enabled {
        writer.write_flag("sh_dep_quant_used_flag", true);
    }
    if tool_flags.sign_data_hiding_enabled && !tool_flags.dependent_quantization_enabled {
        writer.write_flag("sh_sign_data_hiding_used_flag", true);
    }
    if tool_flags.transform_skip_enabled
        && !tool_flags.dependent_quantization_enabled
        && !tool_flags.sign_data_hiding_enabled
    {
        writer.write_flag("sh_ts_residual_coding_disabled_flag", true);
    }
    write_vvc_slice_header_byte_alignment(&mut writer);
    let inter_slice =
        slice_config.inter_enabled && !picture_kind.is_irap() && slice_type.is_inter();
    let payload = vvc_ctu_cabac_payload(picture_geometry, ctu, slice_config, inter_slice);
    writer.write_packed_cabac_bits(
        "cabac_vvc_ctu_quantized_residual_bits",
        &payload.bytes,
        payload.bit_len,
    );
    writer.rbsp_trailing_bits();
    debug_assert!(writer.is_byte_aligned());
    writer.into_bytes()
}

#[cfg(test)]
pub(in crate::vvc) fn vvc_slice_rbsp(
    picture_kind: VvcPictureKind,
    geometry: VvcVideoGeometry,
    color: VvcQuantizedColor,
    slice_config: VvcSliceSyntaxConfig,
) -> VvcSyntaxRbsp {
    vvc_slice_rbsp_with_poc(
        picture_kind,
        vvc_test_poc_lsb(picture_kind),
        geometry,
        0,
        geometry,
        color,
        slice_config,
        VVC_CURRENT_MAX_LUMA_LEAF_SIZE,
    )
}

#[cfg(test)]
fn vvc_test_poc_lsb(picture_kind: VvcPictureKind) -> u32 {
    match picture_kind {
        VvcPictureKind::Idr => 0,
        VvcPictureKind::Cra => 1,
        VvcPictureKind::Trail => 2,
    }
}

fn vvc_frame_slice_rbsp_with_poc(
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
) -> VvcSyntaxRbsp {
    let mut writer = VvcSyntaxWriter::without_fields();
    let slice_type = vvc_slice_type_for_ctus(picture_kind, ctus);
    write_vvc_frame_slice_header(
        &mut writer,
        picture_kind,
        poc_lsb,
        picture_geometry,
        slice_config,
        slice_type,
    );
    write_vvc_frame_coding_tree_entropy(
        &mut writer,
        picture_kind,
        picture_geometry,
        ctus,
        slice_config,
        slice_type,
    );
    writer.rbsp_trailing_bits();
    debug_assert!(writer.is_byte_aligned());
    writer.finish()
}

fn vvc_frame_slice_payload_with_poc_and_cabac_payload(
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    picture_geometry: VvcVideoGeometry,
    slice_config: VvcSliceSyntaxConfig,
    payload: &VvcCabacPayload,
) -> Vec<u8> {
    let mut writer = VvcSyntaxWriter::without_fields();
    write_vvc_frame_slice_header(
        &mut writer,
        picture_kind,
        poc_lsb,
        picture_geometry,
        slice_config,
        VvcSliceType::P,
    );
    writer.write_packed_cabac_bits(
        "cabac_vvc_frame_quantized_residual_bits",
        &payload.bytes,
        payload.bit_len,
    );
    writer.rbsp_trailing_bits();
    debug_assert!(writer.is_byte_aligned());
    writer.into_bytes()
}

fn write_vvc_frame_slice_header(
    writer: &mut VvcSyntaxWriter,
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    _picture_geometry: VvcVideoGeometry,
    slice_config: VvcSliceSyntaxConfig,
    slice_type: VvcSliceType,
) {
    let tool_flags = slice_config.tools;
    writer.write_flag("sh_picture_header_in_slice_header_flag", true);
    write_vvc_picture_header(writer, picture_kind, poc_lsb, slice_config);
    if picture_kind.is_irap() {
        writer.write_flag("sh_no_output_of_prior_pics_flag", false);
    }
    if slice_config.inter_enabled && !picture_kind.is_irap() {
        writer.write_ue("sh_slice_type", slice_type.code());
    }
    if !vvc_picture_header_carries_slice_state(slice_config) {
        write_vvc_slice_header_ref_pic_lists(writer, picture_kind);
        writer.write_se("sh_qp_delta", slice_config.slice_qp - VVC_PPS_INIT_QP);
    }
    if tool_flags.dependent_quantization_enabled {
        writer.write_flag("sh_dep_quant_used_flag", true);
    }
    if tool_flags.sign_data_hiding_enabled && !tool_flags.dependent_quantization_enabled {
        writer.write_flag("sh_sign_data_hiding_used_flag", true);
    }
    if tool_flags.transform_skip_enabled
        && !tool_flags.dependent_quantization_enabled
        && !tool_flags.sign_data_hiding_enabled
    {
        writer.write_flag("sh_ts_residual_coding_disabled_flag", true);
    }
    write_vvc_slice_header_byte_alignment(writer);
}

#[cfg(test)]
pub(in crate::vvc) fn vvc_slice_rbsp_with_poc(
    picture_kind: VvcPictureKind,
    poc_lsb: u32,
    picture_geometry: VvcVideoGeometry,
    slice_address: usize,
    ctu_geometry: VvcVideoGeometry,
    color: VvcQuantizedColor,
    slice_config: VvcSliceSyntaxConfig,
    luma_max_leaf_size: u16,
) -> VvcSyntaxRbsp {
    let mut writer = VvcSyntaxWriter::new();
    let tool_flags = slice_config.tools;
    let slice_count = vvc_picture_ctu_count(picture_geometry);
    let include_picture_header = slice_count == 1;
    writer.write_flag(
        "sh_picture_header_in_slice_header_flag",
        include_picture_header,
    );
    if include_picture_header {
        write_vvc_picture_header(&mut writer, picture_kind, poc_lsb, slice_config);
    }
    if slice_count > 1 {
        writer.write_u(
            "sh_slice_address",
            slice_address as u64,
            vvc_slice_address_bits(picture_geometry),
        );
    }
    if slice_config.inter_enabled && !picture_kind.is_irap() {
        writer.write_ue("sh_slice_type", VvcSliceType::I.code());
    }
    if picture_kind.is_irap() {
        writer.write_flag("sh_no_output_of_prior_pics_flag", false);
    }
    if !vvc_picture_header_carries_slice_state(slice_config) {
        write_vvc_slice_header_ref_pic_lists(&mut writer, picture_kind);
        writer.write_se("sh_qp_delta", slice_config.slice_qp - VVC_PPS_INIT_QP);
    }
    if tool_flags.dependent_quantization_enabled {
        writer.write_flag("sh_dep_quant_used_flag", true);
    }
    if tool_flags.sign_data_hiding_enabled && !tool_flags.dependent_quantization_enabled {
        writer.write_flag("sh_sign_data_hiding_used_flag", true);
    }
    if tool_flags.transform_skip_enabled
        && !tool_flags.dependent_quantization_enabled
        && !tool_flags.sign_data_hiding_enabled
    {
        writer.write_flag("sh_ts_residual_coding_disabled_flag", true);
    }
    write_vvc_slice_header_byte_alignment(&mut writer);
    write_vvc_coding_tree_entropy_with_luma_max_leaf_size(
        &mut writer,
        ctu_geometry,
        color,
        slice_config,
        luma_max_leaf_size,
    );
    writer.rbsp_trailing_bits();
    debug_assert!(writer.is_byte_aligned());
    writer.finish()
}

#[cfg(test)]
pub(in crate::vvc) fn write_vvc_coding_tree_entropy(
    writer: &mut VvcSyntaxWriter,
    geometry: VvcVideoGeometry,
    color: VvcQuantizedColor,
    slice_config: VvcSliceSyntaxConfig,
) {
    write_vvc_coding_tree_entropy_with_luma_max_leaf_size(
        writer,
        geometry,
        color,
        slice_config,
        VVC_CURRENT_MAX_LUMA_LEAF_SIZE,
    );
}

#[cfg(test)]
pub(in crate::vvc) fn write_vvc_coding_tree_entropy_with_luma_max_leaf_size(
    writer: &mut VvcSyntaxWriter,
    geometry: VvcVideoGeometry,
    color: VvcQuantizedColor,
    slice_config: VvcSliceSyntaxConfig,
    luma_max_leaf_size: u16,
) {
    let bits =
        vvc_cabac_bits_with_luma_max_leaf_size(geometry, &color, slice_config, luma_max_leaf_size);
    writer.write_cabac_bits("cabac_vvc_quantized_residual_bits", &bits);
}

fn write_vvc_frame_coding_tree_entropy(
    writer: &mut VvcSyntaxWriter,
    picture_kind: VvcPictureKind,
    picture_geometry: VvcVideoGeometry,
    ctus: &[VvcQuantizedCtu],
    slice_config: VvcSliceSyntaxConfig,
    slice_type: VvcSliceType,
) {
    let inter_slice =
        slice_config.inter_enabled && !picture_kind.is_irap() && slice_type.is_inter();
    let payload = vvc_frame_cabac_payload(picture_geometry, ctus, slice_config, inter_slice);
    writer.write_packed_cabac_bits(
        "cabac_vvc_frame_quantized_residual_bits",
        &payload.bytes,
        payload.bit_len,
    );
}

pub(in crate::vvc) fn vvc_slice_type_for_ctus(
    picture_kind: VvcPictureKind,
    ctus: &[VvcQuantizedCtu],
) -> VvcSliceType {
    if !picture_kind.is_irap() && ctus.iter().any(|ctu| ctu.payload.is_inter_coded()) {
        VvcSliceType::P
    } else {
        VvcSliceType::I
    }
}

fn vvc_slice_type_for_ctu(picture_kind: VvcPictureKind, ctu: &VvcQuantizedCtu) -> VvcSliceType {
    if !picture_kind.is_irap() && ctu.payload.is_inter_coded() {
        VvcSliceType::P
    } else {
        VvcSliceType::I
    }
}

pub(in crate::vvc) fn vvc_picture_ctu_cols(geometry: VvcVideoGeometry) -> usize {
    geometry.coded_width().div_ceil(crate::vvc::VVC_CTU_SIZE)
}

pub(in crate::vvc) fn vvc_picture_ctu_rows(geometry: VvcVideoGeometry) -> usize {
    geometry.coded_height().div_ceil(crate::vvc::VVC_CTU_SIZE)
}

pub(in crate::vvc) fn vvc_picture_ctu_count(geometry: VvcVideoGeometry) -> usize {
    vvc_picture_ctu_cols(geometry) * vvc_picture_ctu_rows(geometry)
}

pub(in crate::vvc) fn vvc_one_slice_per_ctu_partitioning_supported(
    geometry: VvcVideoGeometry,
) -> bool {
    vvc_picture_ctu_cols(geometry) <= VVC_MAX_EXPLICIT_TILE_COLUMNS
        && vvc_picture_ctu_count(geometry) <= VVC_MAX_TILES_PER_PICTURE
}

pub(in crate::vvc) fn vvc_slice_address_bits(geometry: VvcVideoGeometry) -> u8 {
    ceil_log2_usize(vvc_picture_ctu_count(geometry).max(1))
}

fn ceil_log2_usize(value: usize) -> u8 {
    if value <= 1 {
        0
    } else {
        (usize::BITS - (value - 1).leading_zeros()) as u8
    }
}
