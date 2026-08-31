#[derive(Debug, Clone, Copy)]
struct VvcSelectedChromaResidual {
    cb: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    cr: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
}

#[derive(Debug, Clone, Copy)]
struct VvcScoredSelectedChromaResidual {
    residual: VvcSelectedChromaResidual,
    score: VvcResidualBlockScore,
}

impl VvcScoredSelectedChromaResidual {
    pub(super) fn from_scored_blocks(
        residual: VvcSelectedChromaResidual,
        cb_score: VvcResidualBlockScore,
        cr_score: VvcResidualBlockScore,
        width: usize,
        height: usize,
    ) -> Self {
        Self {
            score: VvcResidualBlockScore {
                distortion: cb_score.distortion.saturating_add(cr_score.distortion),
                rate_cost: chroma_coeff_syntax_cost_estimate(width, height, residual.cb)
                    .saturating_add(chroma_coeff_syntax_cost_estimate(
                        width,
                        height,
                        residual.cr,
                    )),
            },
            residual,
        }
    }

    fn new(
        cb_residuals: &[i16],
        cr_residuals: &[i16],
        width: usize,
        height: usize,
        bit_depth: SampleBitDepth,
        chroma_qp: i32,
        chroma_ts_quant: &VvcTransformSkipQuantTable,
        residual: VvcSelectedChromaResidual,
        transform_scratch: &mut VvcInverseTransformScratch,
        reconstructed_residual: &mut Vec<i16>,
    ) -> Self {
        let cb_score = vvc_chroma_residual_block_score(
            cb_residuals,
            width,
            height,
            bit_depth,
            chroma_qp,
            chroma_ts_quant,
            residual.cb,
            transform_scratch,
            reconstructed_residual,
        );
        let cr_score = vvc_chroma_residual_block_score(
            cr_residuals,
            width,
            height,
            bit_depth,
            chroma_qp,
            chroma_ts_quant,
            residual.cr,
            transform_scratch,
            reconstructed_residual,
        );
        Self::from_scored_blocks(residual, cb_score, cr_score, width, height)
    }
}

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

fn finalize_vvc_chroma_residual_block(
    residual_coding: VvcTuResidualCodingMode,
    residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    // Keep this defensive gate beside finalization as well as mode selection:
    // callers that provide a preselected decision must still take a legal,
    // exactly reconstructable path.
    let residual_coding = if matches!(residual_coding, VvcTuResidualCodingMode::TransformSkip)
        && (width > 8 || height > 8)
    {
        VvcTuResidualCodingMode::Transformed
    } else {
        residual_coding
    };
    match residual_coding {
        VvcTuResidualCodingMode::TransformSkip => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = StageStart::now();
            let block = finalize_vvc_chroma_transform_skip_residual_block(
                residuals,
                width,
                height,
                chroma_ts_quant,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
            block
        }
        VvcTuResidualCodingMode::Transformed => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = StageStart::now();
            let quantized = quantize_vvc_chroma_residual_greedy_with_qp(
                residuals,
                width as u16,
                height as u16,
                bit_depth,
                chroma_qp,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transformed_quant_nanos(vvc_elapsed_nanos(quant_start));
            let transformed = VvcFinalizedResidualBlock {
                dc_level: quantized.reconstructed_dc_coeff,
                ac_levels: quantized.reconstructed_ac_coeffs,
                has_ac: quantized.has_ac,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            };
            select_vvc_chroma_residual_block_with_transform_skip(
                residual_coding,
                residuals,
                width,
                height,
                bit_depth,
                chroma_qp,
                chroma_ts_quant,
                transformed,
                stats,
                transform_scratch,
                reconstructed_residual,
            )
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct VvcScoredChromaResidualBlock {
    block: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    score: VvcResidualBlockScore,
}

impl VvcScoredChromaResidualBlock {
    fn new(
        residuals: &[i16],
        width: usize,
        height: usize,
        bit_depth: SampleBitDepth,
        chroma_qp: i32,
        chroma_ts_quant: &VvcTransformSkipQuantTable,
        block: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
        transform_scratch: &mut VvcInverseTransformScratch,
        reconstructed_residual: &mut Vec<i16>,
    ) -> Self {
        let score = vvc_chroma_residual_block_score(
            residuals,
            width,
            height,
            bit_depth,
            chroma_qp,
            chroma_ts_quant,
            block,
            transform_scratch,
            reconstructed_residual,
        );
        Self { block, score }
    }

    fn selects_over(self, best: Self) -> bool {
        self.score.selects_over(best.score)
    }
}

fn select_vvc_scored_chroma_residual_block_with_transform_skip(
    policy: VvcResidualCodingPolicy,
    residual_coding: VvcTuResidualCodingMode,
    residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcScoredChromaResidualBlock {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    match residual_coding {
        VvcTuResidualCodingMode::TransformSkip => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = StageStart::now();
            let block = finalize_vvc_chroma_transform_skip_residual_block(
                residuals,
                width,
                height,
                chroma_ts_quant,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
            VvcScoredChromaResidualBlock::new(
                residuals,
                width,
                height,
                bit_depth,
                chroma_qp,
                chroma_ts_quant,
                block,
                transform_scratch,
                reconstructed_residual,
            )
        }
        VvcTuResidualCodingMode::Transformed => {
            let mut transform_skip_candidate = None;
            if vvc_chroma_lossy_transform_skip_selection_allowed(
                residual_coding,
                width,
                height,
                chroma_qp,
            ) {
                #[cfg(feature = "vvc-stats")]
                let quant_start = StageStart::now();
                let transform_skip = finalize_vvc_chroma_transform_skip_residual_block(
                    residuals,
                    width,
                    height,
                    chroma_ts_quant,
                );
                #[cfg(feature = "vvc-stats")]
                stats.add_chroma_transform_skip_candidate_nanos(vvc_elapsed_nanos(quant_start));
                if transform_skip.has_ac || transform_skip.dc_level != 0 {
                    let transform_skip = VvcScoredChromaResidualBlock::new(
                        residuals,
                        width,
                        height,
                        bit_depth,
                        chroma_qp,
                        chroma_ts_quant,
                        transform_skip,
                        transform_scratch,
                        reconstructed_residual,
                    );
                    if vvc_chroma_fast_search_uses_transform_skip_candidate(policy) {
                        return transform_skip;
                    }
                    transform_skip_candidate = Some(transform_skip);
                }
            }
            #[cfg(feature = "vvc-stats")]
            let quant_start = StageStart::now();
            let quantized = quantize_vvc_chroma_residual_greedy_with_qp(
                residuals,
                width as u16,
                height as u16,
                bit_depth,
                chroma_qp,
            );
            #[cfg(feature = "vvc-stats")]
            stats.add_chroma_transformed_quant_nanos(vvc_elapsed_nanos(quant_start));
            let transformed = VvcFinalizedResidualBlock {
                dc_level: quantized.reconstructed_dc_coeff,
                ac_levels: quantized.reconstructed_ac_coeffs,
                has_ac: quantized.has_ac,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            };
            let mut best = VvcScoredChromaResidualBlock::new(
                residuals,
                width,
                height,
                bit_depth,
                chroma_qp,
                chroma_ts_quant,
                transformed,
                transform_scratch,
                reconstructed_residual,
            );
            if let Some(transform_skip) = transform_skip_candidate {
                if transform_skip.selects_over(best) {
                    best = transform_skip;
                }
            }
            best
        }
    }
}

fn vvc_chroma_lossy_transform_skip_selection_allowed(
    residual_coding: VvcTuResidualCodingMode,
    width: usize,
    height: usize,
    chroma_qp: i32,
) -> bool {
    VVC_ENABLE_LOSSY_TRANSFORM_SKIP_SELECTION
        && matches!(residual_coding, VvcTuResidualCodingMode::Transformed)
        && chroma_qp > 0
        && width <= 8
        && height <= 8
}

fn vvc_chroma_residual_block_score(
    source_residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcResidualBlockScore {
    let distortion =
        if vvc_chroma_transform_skip_score_is_exact(residual, width, height, bit_depth, qp) {
            0
        } else {
            chroma_reconstructed_residual_sse(
                source_residuals,
                width,
                height,
                bit_depth,
                qp,
                chroma_ts_quant,
                residual,
                transform_scratch,
                reconstructed_residual,
            )
        };
    let rate_cost = u64::from(residual.dc_level != 0)
        .saturating_mul(8)
        .saturating_add(chroma_coeff_syntax_cost_estimate(width, height, residual))
        .saturating_add(u64::from(residual.transform_skip));
    VvcResidualBlockScore {
        distortion,
        rate_cost,
    }
}

fn vvc_chroma_transform_skip_score_is_exact(
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    qp: i32,
) -> bool {
    residual.transform_skip
        && width <= 8
        && height <= 8
        && vvc_transform_skip_qp_reconstructs_exact(bit_depth, qp)
}

fn finalize_vvc_chroma_transform_skip_residual_block(
    residuals: &[i16],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    debug_assert_eq!(residuals.len(), width * height);
    if residuals.iter().all(|&residual| residual == 0) {
        return VvcFinalizedResidualBlock {
            dc_level: 0,
            ac_levels: [0; VVC_CHROMA_AC_COEFFS_PER_TU],
            has_ac: false,
            transform_skip: true,
            bdpcm_mode: VvcBdpcmMode::None,
        };
    }
    let dc_level = residuals
        .first()
        .copied()
        .map(|level| quant_table.level(level))
        .unwrap_or(0);
    let (ac_levels, has_ac) =
        transform_skip_chroma_ac_levels_and_flag_with_table(residuals, width, quant_table);
    VvcFinalizedResidualBlock {
        dc_level,
        ac_levels,
        has_ac,
        transform_skip: true,
        bdpcm_mode: VvcBdpcmMode::None,
    }
}

fn finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
    residuals: &[i16],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
    bdpcm_mode: VvcBdpcmMode,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    debug_assert!(bdpcm_mode.is_enabled());
    debug_assert_eq!(residuals.len(), width * height);
    if residuals.iter().all(|&residual| residual == 0) {
        return VvcFinalizedResidualBlock {
            dc_level: 0,
            ac_levels: [0; VVC_CHROMA_AC_COEFFS_PER_TU],
            has_ac: false,
            transform_skip: true,
            bdpcm_mode,
        };
    }
    let active_width = width.min(4);
    let active_height = height.min(4);
    let mut quantized_levels = [0i16; 16];
    let mut ac_levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
    let mut dc_level = 0i16;
    let mut has_ac = false;
    for y in 0..active_height {
        for x in 0..active_width {
            let level = quant_table.level(residuals[y * width + x]);
            quantized_levels[y * 4 + x] = level;
            let predictor = match bdpcm_mode {
                VvcBdpcmMode::None => unreachable!("BDPCM block requires a direction"),
                VvcBdpcmMode::Horizontal if x > 0 => quantized_levels[y * 4 + x - 1],
                VvcBdpcmMode::Vertical if y > 0 => quantized_levels[(y - 1) * 4 + x],
                VvcBdpcmMode::Horizontal | VvcBdpcmMode::Vertical => 0,
            };
            let coeff = (i32::from(level) - i32::from(predictor))
                .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
            if x == 0 && y == 0 {
                dc_level = coeff;
            } else {
                let slot = y * 4 + x - 1;
                ac_levels[slot] = coeff;
                has_ac |= coeff != 0;
            }
        }
    }
    VvcFinalizedResidualBlock {
        dc_level,
        ac_levels,
        has_ac,
        transform_skip: true,
        bdpcm_mode,
    }
}

fn reconstruct_vvc_chroma_residual_block_into(
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    reconstructed_residual: &mut Vec<i16>,
    transform_scratch: &mut VvcInverseTransformScratch,
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
) {
    if residual.transform_skip {
        if residual.bdpcm_mode.is_enabled() {
            reconstruct_vvc_chroma_bdpcm_transform_skip_residuals_into_with_table(
                reconstructed_residual,
                residual.dc_level,
                &residual.ac_levels,
                width,
                height,
                chroma_ts_quant,
                residual.bdpcm_mode,
            );
        } else {
            reconstruct_vvc_chroma_transform_skip_residuals_into_with_table(
                reconstructed_residual,
                residual.dc_level,
                &residual.ac_levels,
                width,
                height,
                chroma_ts_quant,
            );
        }
    } else {
        inverse_transform_vvc_chroma_quantized_block_into_with_qp(
            reconstructed_residual,
            transform_scratch,
            width as u16,
            height as u16,
            residual.dc_level,
            &residual.ac_levels,
            bit_depth,
            chroma_qp,
        );
    }
}
