#[derive(Debug, Clone, Copy)]
struct VvcFinalizedChromaTu {
    cb_dc_level: i16,
    cr_dc_level: i16,
    cb_ac_levels: [i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    cr_ac_levels: [i16; VVC_CHROMA_AC_COEFFS_PER_TU],
    cb_has_ac: bool,
    cr_has_ac: bool,
    cb_transform_skip: bool,
    cr_transform_skip: bool,
    bdpcm_mode: VvcBdpcmMode,
}

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
        Self {
            residual,
            score: VvcResidualBlockScore {
                distortion: cb_score.distortion.saturating_add(cr_score.distortion),
                rate_cost: chroma_coeff_syntax_cost_estimate(width, height, residual.cb)
                    .saturating_add(chroma_coeff_syntax_cost_estimate(
                        width,
                        height,
                        residual.cr,
                    )),
            },
        }
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
    let coded_geometry = frame_recon.coded_geometry();
    if exact_transform_skip_qp
        && vvc_chroma_transform_skip_score_is_exact(
            cb_residual,
            chroma_width,
            chroma_height,
            source_frame.format.bit_depth,
            chroma_qp,
        )
    {
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        copy_source_chroma_node_into_reconstruction(
            &mut frame_recon.cb,
            &source_frame.cb,
            source_frame.geometry,
            coded_geometry,
            source_frame.format,
            node,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_fill_nanos(vvc_elapsed_nanos(fill_start));
    } else if cb_residual.transform_skip {
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        fill_visible_chroma_transform_skip_node(
            &mut frame_recon.cb,
            coded_geometry,
            node,
            source_frame.format.chroma_sampling,
            predicted_cb,
            cb_residual,
            source_frame.format.bit_depth,
            chroma_ts_quant,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_fill_nanos(vvc_elapsed_nanos(fill_start));
    } else {
        #[cfg(feature = "vvc-stats")]
        let recon_start = StageStart::now();
        reconstruct_vvc_chroma_residual_block_into(
            cb_residual,
            reconstructed_residual,
            transform_scratch,
            chroma_width,
            chroma_height,
            source_frame.format.bit_depth,
            chroma_qp,
            chroma_ts_quant,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_recon_nanos(vvc_elapsed_nanos(recon_start));
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        fill_visible_chroma_node(
            &mut frame_recon.cb,
            coded_geometry,
            node,
            source_frame.format.chroma_sampling,
            predicted_cb,
            reconstructed_residual,
            source_frame.format.bit_depth,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_fill_nanos(vvc_elapsed_nanos(fill_start));
    }
    if exact_transform_skip_qp
        && vvc_chroma_transform_skip_score_is_exact(
            cr_residual,
            chroma_width,
            chroma_height,
            source_frame.format.bit_depth,
            chroma_qp,
        )
    {
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        copy_source_chroma_node_into_reconstruction(
            &mut frame_recon.cr,
            &source_frame.cr,
            source_frame.geometry,
            coded_geometry,
            source_frame.format,
            node,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_fill_nanos(vvc_elapsed_nanos(fill_start));
    } else if cr_residual.transform_skip {
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        fill_visible_chroma_transform_skip_node(
            &mut frame_recon.cr,
            coded_geometry,
            node,
            source_frame.format.chroma_sampling,
            predicted_cr,
            cr_residual,
            source_frame.format.bit_depth,
            chroma_ts_quant,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_fill_nanos(vvc_elapsed_nanos(fill_start));
    } else {
        #[cfg(feature = "vvc-stats")]
        let recon_start = StageStart::now();
        reconstruct_vvc_chroma_residual_block_into(
            cr_residual,
            reconstructed_residual,
            transform_scratch,
            chroma_width,
            chroma_height,
            source_frame.format.bit_depth,
            chroma_qp,
            chroma_ts_quant,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_residual_recon_nanos(vvc_elapsed_nanos(recon_start));
        #[cfg(feature = "vvc-stats")]
        let fill_start = StageStart::now();
        fill_visible_chroma_node(
            &mut frame_recon.cr,
            coded_geometry,
            node,
            source_frame.format.chroma_sampling,
            predicted_cr,
            reconstructed_residual,
            source_frame.format.bit_depth,
        );
        #[cfg(feature = "vvc-stats")]
        stats.add_chroma_fill_nanos(vvc_elapsed_nanos(fill_start));
    }
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
    let active_width = node_width.min(4);
    let active_height = node_height.min(4);
    if residual.bdpcm_mode.is_enabled() {
        fill_visible_chroma_bdpcm_transform_skip_node(
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
            residual,
            bit_depth,
            quant_table,
        );
        return;
    }

    let max_sample = i32::from(bit_depth.max_sample());
    for local_y in 0..visible_height {
        let row = (start_y + local_y) * chroma_width + start_x;
        let predicted_row = local_y * node_width;
        for local_x in 0..visible_width {
            let reconstructed_residual =
                if local_y < active_height && local_x < active_width {
                    let level = if local_x == 0 && local_y == 0 {
                        residual.dc_level
                    } else {
                        residual.ac_levels[local_y * 4 + local_x - 1]
                    };
                    quant_table.reconstructed(level)
                } else {
                    0
                };
            let idx = predicted_row + local_x;
            chroma[row + local_x] =
                (i32::from(predicted[idx]) + i32::from(reconstructed_residual))
                    .clamp(0, max_sample) as VvcSample;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_visible_chroma_bdpcm_transform_skip_node(
    chroma: &mut [VvcSample],
    chroma_stride: usize,
    start_x: usize,
    start_y: usize,
    visible_width: usize,
    visible_height: usize,
    node_width: usize,
    active_width: usize,
    active_height: usize,
    predicted: &[VvcSample],
    residual: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    bit_depth: SampleBitDepth,
    quant_table: &VvcTransformSkipQuantTable,
) {
    let max_sample = i32::from(bit_depth.max_sample());
    let active_rows = visible_height.min(active_height);
    let mut vertical_predictors = [0i16; 4];
    for local_y in 0..active_rows {
        let row = (start_y + local_y) * chroma_stride + start_x;
        let predicted_row = local_y * node_width;
        let mut horizontal_predictor = 0i16;
        for local_x in 0..active_width {
            let delta = if local_x == 0 && local_y == 0 {
                residual.dc_level
            } else {
                residual.ac_levels[local_y * 4 + local_x - 1]
            };
            let level = match residual.bdpcm_mode {
                VvcBdpcmMode::None => unreachable!("BDPCM fill requires a direction"),
                VvcBdpcmMode::Horizontal if local_x > 0 => {
                    add_bdpcm_quantized_levels(delta, horizontal_predictor)
                }
                VvcBdpcmMode::Vertical if local_y > 0 => {
                    add_bdpcm_quantized_levels(delta, vertical_predictors[local_x])
                }
                VvcBdpcmMode::Horizontal | VvcBdpcmMode::Vertical => delta,
            };
            horizontal_predictor = level;
            vertical_predictors[local_x] = level;
            if local_x < visible_width {
                let idx = predicted_row + local_x;
                chroma[row + local_x] =
                    (i32::from(predicted[idx]) + i32::from(quant_table.reconstructed(level)))
                        .clamp(0, max_sample) as VvcSample;
            }
        }
        for local_x in active_width..visible_width {
            let idx = predicted_row + local_x;
            chroma[row + local_x] = predicted[idx];
        }
    }
    for local_y in active_rows..visible_height {
        let row = (start_y + local_y) * chroma_stride + start_x;
        let predicted_row = local_y * node_width;
        chroma[row..row + visible_width]
            .copy_from_slice(&predicted[predicted_row..predicted_row + visible_width]);
    }
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
    let source_chroma_width = source_geometry.width / subsample_x;
    let source_chroma_height = source_geometry.height / subsample_y;
    if source_chroma_width == 0 || source_chroma_height == 0 {
        return;
    }
    let chroma_width = dst_geometry.width / subsample_x;
    let chroma_height = dst_geometry.height / subsample_y;
    let start_x = usize::from(node.x) / subsample_x;
    let start_y = usize::from(node.y) / subsample_y;
    if start_x >= chroma_width || start_y >= chroma_height {
        return;
    }
    let end_x = start_x
        .saturating_add(usize::from(node.width) / subsample_x)
        .min(chroma_width);
    let end_y = start_y
        .saturating_add(usize::from(node.height) / subsample_y)
        .min(chroma_height);
    for y in start_y..end_y {
        let dst_row = y * chroma_width;
        let src_y = y.min(source_chroma_height - 1);
        let src_row = src_y * source_chroma_width;
        for x in start_x..end_x {
            let src_x = x.min(source_chroma_width - 1);
            chroma[dst_row + x] = source[src_row + src_x];
        }
    }
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
    match residual_coding {
        VvcTuResidualCodingMode::TransformSkip => {
            #[cfg(feature = "vvc-stats")]
            let quant_start = StageStart::now();
            let block =
                finalize_vvc_chroma_transform_skip_residual_block(residuals, width, height, chroma_ts_quant);
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
        self.score.selects_quality_over(best.score)
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

fn vvc_chroma_fast_search_uses_transform_skip_candidate(policy: VvcResidualCodingPolicy) -> bool {
    // Chroma transform skip remains available as a candidate, but lossy fast
    // search must compare it against transformed residual coding for 8-bit
    // 4:4:4 screen content. For flat 4:4:4/RGB blocks, forcing transform skip
    // emits many AC coefficients where transformed coding can collapse the
    // block to DC-only. Other formats keep the prior shortcut for throughput.
    policy.residual_mode() == VvcResidualCodingMode::Lossy
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
        && (policy.chroma_sampling() != ChromaSampling::Cs444 || policy.bit_depth().bits() != 8)
}

fn select_vvc_chroma_residual_block_with_transform_skip(
    residual_coding: VvcTuResidualCodingMode,
    residuals: &[i16],
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
    chroma_ts_quant: &VvcTransformSkipQuantTable,
    transformed: VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU>,
    stats: &mut VvcIntraSearchStats,
    transform_scratch: &mut VvcInverseTransformScratch,
    reconstructed_residual: &mut Vec<i16>,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    #[cfg(not(feature = "vvc-stats"))]
    let _ = stats;
    if !vvc_chroma_lossy_transform_skip_selection_allowed(residual_coding, width, height, chroma_qp)
    {
        return transformed;
    }

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
    if !transform_skip.has_ac && transform_skip.dc_level == 0 {
        return transformed;
    }

    let transformed_score = vvc_chroma_residual_block_score(
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
    let transform_skip_score = vvc_chroma_residual_block_score(
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
    if transform_skip_score.selects_quality_over(transformed_score) {
        transform_skip
    } else {
        transformed
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
        && width <= usize::from(VVC_TRANSFORM_SKIP_MAX_SIZE)
        && height <= usize::from(VVC_TRANSFORM_SKIP_MAX_SIZE)
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
    let distortion = if vvc_chroma_transform_skip_score_is_exact(
        residual, width, height, bit_depth, qp,
    ) {
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
        && width <= 4
        && height <= 4
        && vvc_transform_skip_qp_reconstructs_exact(bit_depth, qp)
}

fn finalize_vvc_chroma_transform_skip_residual_block(
    residuals: &[i16],
    width: usize,
    height: usize,
    quant_table: &VvcTransformSkipQuantTable,
) -> VvcFinalizedResidualBlock<VVC_CHROMA_AC_COEFFS_PER_TU> {
    debug_assert_eq!(residuals.len(), width * height);
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
