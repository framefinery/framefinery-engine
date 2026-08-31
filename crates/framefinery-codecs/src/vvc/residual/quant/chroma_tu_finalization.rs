fn finalize_vvc_chroma_tu(
    coding_decision: VvcChromaTuCodingDecision,
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
    cb_residuals: &[i16],
    cr_residuals: &[i16],
    chroma_width: usize,
    chroma_height: usize,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    exact_transform_skip_qp: bool,
    preselected_residual: Option<VvcScoredSelectedChromaResidual>,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedChromaTu {
    #[cfg(feature = "vvc-stats")]
    let score_start = StageStart::now();
    let selected_residual = preselected_residual
        .map(|residual| residual.residual)
        .unwrap_or_else(|| VvcSelectedChromaResidual {
            cb: finalize_vvc_chroma_residual_block(
                coding_decision.residual_coding,
                cb_residuals,
                chroma_width,
                chroma_height,
                source_frame.format.bit_depth,
                chroma_qp,
                chroma_ts_quant,
                stats,
                transform_scratch,
                reconstructed_residual,
            ),
            cr: finalize_vvc_chroma_residual_block(
                coding_decision.residual_coding,
                cr_residuals,
                chroma_width,
                chroma_height,
                source_frame.format.bit_depth,
                chroma_qp,
                chroma_ts_quant,
                stats,
                transform_scratch,
                reconstructed_residual,
            ),
        });
    #[cfg(feature = "vvc-stats")]
    stats.add_chroma_rd_scoring_nanos(vvc_elapsed_nanos(score_start));
    let cb_residual = selected_residual.cb;
    let cr_residual = selected_residual.cr;
    let reconstruction = VvcChromaPlaneReconstructionContext {
        source_geometry: source_frame.geometry,
        coded_geometry: frame_recon.coded_geometry(),
        format: source_frame.format,
        node,
        chroma_width,
        chroma_height,
        chroma_qp,
        chroma_ts_quant,
        exact_transform_skip_qp,
    };
    let mut reconstruction_scratch = VvcChromaPlaneReconstructionScratch {
        #[cfg(feature = "vvc-stats")]
        stats,
        transform_scratch,
        reconstructed_residual,
    };
    reconstruction.reconstruct_plane(
        &mut frame_recon.cb,
        &source_frame.cb,
        predicted_cb,
        cb_residual,
        &mut reconstruction_scratch,
    );
    reconstruction.reconstruct_plane(
        &mut frame_recon.cr,
        &source_frame.cr,
        predicted_cr,
        cr_residual,
        &mut reconstruction_scratch,
    );
    let finalized = VvcFinalizedChromaTu {
        cb_dc_level: cb_residual.dc_level,
        cr_dc_level: cr_residual.dc_level,
        cb_ac_levels: cb_residual.ac_levels,
        cr_ac_levels: cr_residual.ac_levels,
        cb_has_ac: cb_residual.has_ac,
        cr_has_ac: cr_residual.has_ac,
        cb_transform_skip: cb_residual.transform_skip,
        cr_transform_skip: cr_residual.transform_skip,
        bdpcm_mode: cb_residual
            .bdpcm_mode
            .is_enabled()
            .then_some(cb_residual.bdpcm_mode)
            .unwrap_or(cr_residual.bdpcm_mode),
    };
    frame_recon.mark_chroma_node_available(node);
    finalized
}

#[derive(Debug, Clone, Copy)]
struct VvcChromaPlaneReconstructionContext<'a> {
    source_geometry: VvcVideoGeometry,
    coded_geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    node: VvcCodingTreeNode,
    chroma_width: usize,
    chroma_height: usize,
    chroma_qp: i32,
    chroma_ts_quant: &'a VvcTransformSkipQuantTable,
    exact_transform_skip_qp: bool,
}

struct VvcChromaPlaneReconstructionScratch<'a> {
    #[cfg(feature = "vvc-stats")]
    stats: &'a mut VvcIntraSearchStats,
    transform_scratch: &'a mut VvcInverseTransformScratch,
    reconstructed_residual: &'a mut Vec<i16>,
}

impl VvcChromaPlaneReconstructionContext<'_> {
    fn reconstruct_plane(
        self,
        destination: &mut [VvcSample],
        source: &[VvcSample],
        predicted: &[VvcSample],
        residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
        scratch: &mut VvcChromaPlaneReconstructionScratch<'_>,
    ) {
        if self.exact_transform_skip_qp
            && vvc_chroma_transform_skip_score_is_exact(
                residual,
                self.chroma_width,
                self.chroma_height,
                self.format.bit_depth,
                self.chroma_qp,
            )
        {
            #[cfg(feature = "vvc-stats")]
            let fill_start = StageStart::now();
            copy_source_chroma_node_into_reconstruction(
                destination,
                source,
                self.source_geometry,
                self.coded_geometry,
                self.format,
                self.node,
            );
            #[cfg(feature = "vvc-stats")]
            scratch
                .stats
                .add_chroma_fill_nanos(vvc_elapsed_nanos(fill_start));
        } else if residual.transform_skip {
            #[cfg(feature = "vvc-stats")]
            let fill_start = StageStart::now();
            fill_visible_chroma_transform_skip_node(
                destination,
                self.coded_geometry,
                self.node,
                self.format.chroma_sampling,
                predicted,
                residual,
                self.format.bit_depth,
                self.chroma_ts_quant,
            );
            #[cfg(feature = "vvc-stats")]
            scratch
                .stats
                .add_chroma_fill_nanos(vvc_elapsed_nanos(fill_start));
        } else {
            #[cfg(feature = "vvc-stats")]
            let recon_start = StageStart::now();
            reconstruct_vvc_chroma_residual_block_into(
                residual,
                scratch.reconstructed_residual,
                scratch.transform_scratch,
                self.chroma_width,
                self.chroma_height,
                self.format.bit_depth,
                self.chroma_qp,
                self.chroma_ts_quant,
            );
            #[cfg(feature = "vvc-stats")]
            scratch
                .stats
                .add_chroma_residual_recon_nanos(vvc_elapsed_nanos(recon_start));
            #[cfg(feature = "vvc-stats")]
            let fill_start = StageStart::now();
            fill_visible_chroma_node(
                destination,
                self.coded_geometry,
                self.node,
                self.format.chroma_sampling,
                predicted,
                scratch.reconstructed_residual,
                self.format.bit_depth,
            );
            #[cfg(feature = "vvc-stats")]
            scratch
                .stats
                .add_chroma_fill_nanos(vvc_elapsed_nanos(fill_start));
        }
    }
}

fn fill_visible_chroma_transform_skip_node(
    chroma: &mut [VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    predicted: &[VvcSample],
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    bit_depth: SampleBitDepth,
    quant_table: &VvcTransformSkipQuantTable,
) {
    let subsample_x = chroma_subsample_x(chroma_sampling);
    let subsample_y = chroma_subsample_y(chroma_sampling);
    let node_width = usize::from(node.width) / subsample_x;
    let node_height = usize::from(node.height) / subsample_y;
    let start_x = usize::from(node.x) / subsample_x;
    let start_y = usize::from(node.y) / subsample_y;
    let chroma_width = geometry.width / subsample_x;
    let chroma_height = geometry.height / subsample_y;
    let visible_width = node_width.min(chroma_width.saturating_sub(start_x));
    let visible_height = node_height.min(chroma_height.saturating_sub(start_y));
    if visible_width == 0 || visible_height == 0 {
        return;
    }
    let active_width = node_width.min(8);
    let active_height = node_height.min(8);
    fill_visible_transform_skip_samples(
        chroma,
        chroma_width,
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

fn copy_source_chroma_node_into_reconstruction(
    chroma: &mut [VvcSample],
    source: &[VvcSample],
    source_geometry: VvcVideoGeometry,
    dst_geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    node: VvcCodingTreeNode,
) {
    let subsample_x = chroma_subsample_x(format.chroma_sampling);
    let subsample_y = chroma_subsample_y(format.chroma_sampling);
    let source_chroma_geometry = VvcVideoGeometry {
        width: source_geometry.width / subsample_x,
        height: source_geometry.height / subsample_y,
    };
    let destination_chroma_geometry = VvcVideoGeometry {
        width: dst_geometry.width / subsample_x,
        height: dst_geometry.height / subsample_y,
    };
    let region = VvcPlaneRegion {
        origin_x: usize::from(node.x) / subsample_x,
        origin_y: usize::from(node.y) / subsample_y,
        geometry: VvcVideoGeometry {
            width: usize::from(node.width) / subsample_x,
            height: usize::from(node.height) / subsample_y,
        },
    };
    copy_vvc_source_plane_region_with_edge_extension(
        chroma,
        destination_chroma_geometry,
        source,
        source_chroma_geometry,
        region,
    );
}
