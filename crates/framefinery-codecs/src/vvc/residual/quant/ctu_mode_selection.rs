#[allow(clippy::too_many_arguments)]
pub(in crate::vvc) fn quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes_and_scratch_with_mode_hints(
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    region: VvcCtuRegion,
    policy: VvcResidualCodingPolicy,
    luma_qp: i32,
    chroma_qp: i32,
    luma_mode_search_state: &mut VvcLumaModeSearchState,
    transform_skip_quant_tables: &VvcTransformSkipQuantTables,
    scratch: &mut VvcCtuQuantScratch,
    luma_inter_decisions: Option<&[Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS]>,
    luma_scc_decisions: Option<&[Option<VvcIbcCuDecision>; MAX_VVC_LUMA_TUS]>,
    inter_reference: Option<&VvcReconstructionFrame>,
    selected_luma_inter_decisions: Option<&mut [Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS]>,
    temporal_mode_hints: Option<&VvcQuantizedColor>,
) -> VvcQuantizedColor {
    let VvcCtuQuantScratch {
        luma_nodes,
        chroma_nodes,
        prediction_scratch,
        predicted_luma,
        predicted_cb,
        predicted_cr,
        transform_scratch,
        reconstructed_residual,
        luma_residuals,
        candidate_luma_prediction,
        candidate_luma_residuals,
        luma_rd_cache,
        cb_residuals,
        cr_residuals,
        candidate_cb_prediction,
        candidate_cr_prediction,
        candidate_cb_residuals,
        candidate_cr_residuals,
        chroma_rd_cache,
    } = scratch;
    predicted_luma.clear();
    predicted_cb.clear();
    predicted_cr.clear();
    reconstructed_residual.clear();
    luma_residuals.clear();
    candidate_luma_prediction.clear();
    candidate_luma_residuals.clear();
    cb_residuals.clear();
    cr_residuals.clear();
    candidate_cb_prediction.clear();
    candidate_cr_prediction.clear();
    candidate_cb_residuals.clear();
    candidate_cr_residuals.clear();
    #[cfg(feature = "vvc-stats")]
    let mut intra_search_stats = VvcIntraSearchStats::default();
    #[cfg(not(feature = "vvc-stats"))]
    let mut intra_search_stats = VvcIntraSearchStats;
    #[cfg(feature = "vvc-stats")]
    let mut residual_energy_stats = VvcResidualEnergyStats::default();
    #[cfg(feature = "vvc-stats")]
    let mut tu_trace_sink = vvc_tu_trace_sink();

    let score_metric = policy.score_metric();
    let chroma_syntax_tie_breaker = policy.chroma_syntax_tie_breaker();
    let luma_ts_quant = transform_skip_quant_tables.luma();
    let chroma_ts_quant = transform_skip_quant_tables.chroma();
    let ctu_shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: region.geometry.coded_width() as u16,
        visible_height: region.geometry.coded_height() as u16,
        chroma_sampling: source_frame.format.chroma_sampling,
        dual_tree_intra: policy.dual_tree_intra(),
    };

    let VvcLumaCtuPassResult {
        metadata: luma_tu_metadata,
        applied_inter_decisions: applied_luma_inter_decisions,
        tu_count: luma_tu_count,
    } = VvcLumaCtuPassContext {
        source_frame,
        region,
        policy,
        score_metric,
        luma_qp,
        luma_ts_quant,
        luma_inter_decisions,
        luma_scc_decisions,
        inter_reference,
        temporal_mode_hints,
        ctu_shape,
    }
    .quantize(VvcLumaCtuPassBuffers {
        frame_recon,
        mode_search_state: luma_mode_search_state,
        nodes: luma_nodes,
        prediction_scratch,
        selected_prediction: predicted_luma,
        selected_residuals: luma_residuals,
        candidate_prediction: candidate_luma_prediction,
        candidate_residuals: candidate_luma_residuals,
        rd_cache: luma_rd_cache,
        transform_scratch,
        reconstructed_residual,
        stats: &mut intra_search_stats,
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats: &mut residual_energy_stats,
        #[cfg(feature = "vvc-stats")]
        trace_sink: tu_trace_sink.as_mut(),
    });

    let VvcChromaCtuPassResult {
        metadata: chroma_tu_metadata,
        tu_count: chroma_tu_count,
    } = VvcChromaCtuPassContext {
        source_frame,
        region,
        policy,
        score_metric,
        syntax_tie_breaker_enabled: chroma_syntax_tie_breaker,
        chroma_qp,
        chroma_ts_quant,
        inter_reference,
        temporal_mode_hints,
        ctu_shape,
        luma_nodes,
        luma_metadata: &luma_tu_metadata,
        applied_luma_inter_decisions: &applied_luma_inter_decisions,
        luma_mode_search_state,
    }
    .quantize(VvcChromaCtuPassBuffers {
        frame_recon,
        nodes: chroma_nodes,
        prediction_scratch,
        selected: VvcChromaCandidateBuffers {
            prediction: VvcChromaPredictionBuffers {
                cb: predicted_cb,
                cr: predicted_cr,
            },
            residuals: VvcChromaResidualBuffers {
                cb: cb_residuals,
                cr: cr_residuals,
            },
        },
        candidate: VvcChromaCandidateBuffers {
            prediction: VvcChromaPredictionBuffers {
                cb: candidate_cb_prediction,
                cr: candidate_cr_prediction,
            },
            residuals: VvcChromaResidualBuffers {
                cb: candidate_cb_residuals,
                cr: candidate_cr_residuals,
            },
        },
        rd_cache: chroma_rd_cache,
        transform_scratch,
        reconstructed_residual,
        stats: &mut intra_search_stats,
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats: &mut residual_energy_stats,
        #[cfg(feature = "vvc-stats")]
        trace_sink: tu_trace_sink.as_mut(),
    });

    let quantized = VvcCtuQuantizationResult {
        luma_metadata: luma_tu_metadata,
        chroma_metadata: chroma_tu_metadata,
        luma_tu_count,
        chroma_tu_count,
        #[cfg(feature = "vvc-stats")]
        intra_search_stats,
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats,
    }
    .into_quantized_color(source_frame);
    if let Some(selected) = selected_luma_inter_decisions {
        *selected = applied_luma_inter_decisions;
    }
    quantized
}
