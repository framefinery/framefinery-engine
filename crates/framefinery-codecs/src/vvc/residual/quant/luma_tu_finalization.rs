fn finalize_vvc_luma_tu(
    coding_decision: VvcLumaTuCodingDecision,
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    predicted_luma: &[VvcSample],
    residuals: &[i16],
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    exact_transform_skip_qp: bool,
    preselected_residual: Option<VvcScoredSelectedLumaResidual>,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedLumaTu {
    #[cfg(feature = "vvc-stats")]
    let score_start = StageStart::now();
    let selected_residual = match preselected_residual {
        Some(residual) => refine_vvc_luma_final_mts_residual(
            residual.residual,
            coding_decision,
            residuals,
            node,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
            stats,
            transform_scratch,
            reconstructed_residual,
        ),
        None => {
            let (block, mts_index) = select_vvc_luma_residual_block_with_mts(
                coding_decision.residual_coding,
                coding_decision.mts_index,
                residuals,
                node.width,
                node.height,
                source_frame.format.bit_depth,
                luma_qp,
                luma_ts_quant,
                true,
                VvcLumaResidualQuantizationSearch::Full,
                stats,
                transform_scratch,
                reconstructed_residual,
            );
            VvcSelectedLumaResidual { block, mts_index }
        }
    };
    #[cfg(feature = "vvc-stats")]
    stats.add_luma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
    let residual = selected_residual.block;
    let mts_index = selected_residual.mts_index;
    let coded_geometry = frame_recon.coded_geometry();
    if exact_transform_skip_qp
        && vvc_luma_transform_skip_score_is_exact(
            residual,
            node.width,
            node.height,
            source_frame.format.bit_depth,
            luma_qp,
        )
    {
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        copy_source_luma_node_into_reconstruction(frame_recon, source_frame, node);
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_fill_nanos(vvc_elapsed_nanos(fill_start));
    } else if residual.transform_skip {
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        fill_visible_luma_transform_skip_node(
            &mut frame_recon.luma,
            coded_geometry,
            node,
            predicted_luma,
            residual,
            source_frame.format.bit_depth,
            luma_ts_quant,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_fill_nanos(vvc_elapsed_nanos(fill_start));
    } else {
        #[cfg(feature = "vvc-stats")]
        let recon_start = StageStart::now();
        reconstruct_vvc_luma_residual_block_into(
            residual,
            mts_index,
            reconstructed_residual,
            transform_scratch,
            node.width,
            node.height,
            source_frame.format.bit_depth,
            luma_qp,
            luma_ts_quant,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_residual_recon_nanos(vvc_elapsed_nanos(recon_start));
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        fill_visible_luma_node(
            &mut frame_recon.luma,
            coded_geometry,
            node,
            predicted_luma,
            reconstructed_residual,
            source_frame.format.bit_depth,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_luma_fill_nanos(vvc_elapsed_nanos(fill_start));
    }
    let finalized = VvcFinalizedLumaTu {
        abs_remainder: residual.abs_remainder(),
        negative: residual.negative(),
        dc_level: residual.dc_level,
        ac_levels: residual.ac_levels,
        has_ac: residual.has_ac,
        transform_skip: residual.transform_skip,
        bdpcm_mode: residual.bdpcm_mode,
        mrl_index: coding_decision.mrl_index,
        mts_index,
    };
    frame_recon.mark_luma_node_available(node);
    finalized
}

fn fill_visible_luma_transform_skip_node(
    luma: &mut [VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    predicted: &[VvcSample],
    residual: VvcFinalizedResidualBlock<VVC_LUMA_AC_COEFFS_PER_TU>,
    bit_depth: SampleBitDepth,
    quant_table: &VvcTransformSkipQuantTable,
) {
    let node_width = usize::from(node.width);
    let node_height = usize::from(node.height);
    let start_x = usize::from(node.x);
    let start_y = usize::from(node.y);
    let visible_width = node_width.min(geometry.width.saturating_sub(start_x));
    let visible_height = node_height.min(geometry.height.saturating_sub(start_y));
    if visible_width == 0 || visible_height == 0 {
        return;
    }
    let (active_width, active_height) =
        vvc_luma_transform_skip_active_extent(node_width, node_height);
    fill_visible_transform_skip_samples(
        luma,
        geometry.width,
        start_x,
        start_y,
        visible_width,
        visible_height,
        node_width,
        active_width,
        active_height,
        predicted,
        residual.dc_level,
        &residual.ac_levels,
        residual.bdpcm_mode,
        bit_depth,
        quant_table,
    );
}

fn copy_source_luma_node_into_reconstruction(
    frame_recon: &mut VvcReconstructionFrame,
    source_frame: &VvcSampledFrame,
    node: VvcCodingTreeNode,
) {
    let destination_geometry = VvcVideoGeometry {
        width: frame_recon.luma_width(),
        height: frame_recon.luma_height(),
    };
    let region = VvcPlaneRegion {
        origin_x: usize::from(node.x),
        origin_y: usize::from(node.y),
        geometry: VvcVideoGeometry {
            width: usize::from(node.width),
            height: usize::from(node.height),
        },
    };
    copy_vvc_source_plane_region_with_edge_extension(
        &mut frame_recon.luma,
        destination_geometry,
        &source_frame.luma,
        source_frame.geometry,
        region,
    );
}

fn refine_vvc_luma_final_mts_residual(
    selected: VvcSelectedLumaResidual,
    coding_decision: VvcLumaTuCodingDecision,
    residuals: &[i16],
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    luma_qp: i32,
    luma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcSelectedLumaResidual {
    if selected.block.transform_skip
        || !vvc_luma_mts_selection_allowed(
            coding_decision.residual_coding,
            coding_decision.mts_index,
            node.width,
            node.height,
            luma_qp,
            selected.block.has_ac,
        )
    {
        return selected;
    }
    let (block, mts_index) = select_vvc_luma_residual_block_with_mts(
        coding_decision.residual_coding,
        coding_decision.mts_index,
        residuals,
        node.width,
        node.height,
        bit_depth,
        luma_qp,
        luma_ts_quant,
        true,
        VvcLumaResidualQuantizationSearch::Full,
        stats,
        transform_scratch,
        reconstructed_residual,
    );
    VvcSelectedLumaResidual { block, mts_index }
}
