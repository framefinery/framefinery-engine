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
    let mut chroma_tu_metadata = VvcChromaTuMetadata::new();
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

    let mut chroma_tu_count = 0usize;
    if ctu_shape.dual_tree_intra {
        vvc_chroma_transform_nodes_into(chroma_nodes, ctu_shape);
    } else {
        chroma_nodes.clear();
        chroma_nodes.extend(luma_nodes.iter().copied());
    }
    for local_node in chroma_nodes.iter().copied() {
        if chroma_tu_count >= MAX_VVC_CHROMA_TUS {
            break;
        }
        let node = vvc_global_ctu_node(local_node, region);
        if !ctu_shape.dual_tree_intra
            && matches!(
                luma_tu_metadata.scc_decision(chroma_tu_count),
                Some(VvcLumaSccDecision::IbcExact(_))
            )
        {
            chroma_tu_count += 1;
            continue;
        }
        chroma_rd_cache.reset(policy, node);
        let subsample_x = chroma_subsample_x(source_frame.format.chroma_sampling);
        let subsample_y = chroma_subsample_y(source_frame.format.chroma_sampling);
        let chroma_x = usize::from(node.x) / subsample_x;
        let chroma_y = usize::from(node.y) / subsample_y;
        let chroma_width = usize::from(node.width) / subsample_x;
        let chroma_height = usize::from(node.height) / subsample_y;
        let co_located_luma_mode = luma_mode_search_state.co_located_mode_for_chroma_node(node);
        let cclm_syntax_enabled = vvc_chroma_cclm_node_allowed(node);
        let applied_inter_decision = applied_luma_inter_decisions
            .get(chroma_tu_count)
            .copied()
            .flatten();
        let selected_inter_chroma_candidate = match (applied_inter_decision, inter_reference) {
            (Some(decision), Some(reference)) => VvcChromaInterCandidateContext {
                policy,
                source_frame,
                node,
                chroma_x,
                chroma_y,
                chroma_width,
                chroma_height,
            }
            .select_candidate(
                decision,
                reference,
                &mut VvcChromaCandidateBuffers {
                    prediction: VvcChromaPredictionBuffers {
                        cb: &mut *predicted_cb,
                        cr: &mut *predicted_cr,
                    },
                    residuals: VvcChromaResidualBuffers {
                        cb: &mut *cb_residuals,
                        cr: &mut *cr_residuals,
                    },
                },
                &mut intra_search_stats,
            ),
            _ => None,
        };
        let temporal_chroma_hint = selected_inter_chroma_candidate
            .is_none()
            .then(|| {
                vvc_chroma_temporal_mode_hint(
                    temporal_mode_hints,
                    chroma_tu_count,
                    chroma_nodes.len(),
                    policy,
                    source_frame.geometry,
                    node,
                    co_located_luma_mode,
                    chroma_width,
                    chroma_height,
                )
            })
            .flatten();
        let selected_chroma_candidate = if let Some(candidate) = selected_inter_chroma_candidate {
            candidate
        } else {
            VvcChromaTuSelectionContext {
                policy,
                metric: score_metric,
                source_frame,
                frame_recon: &*frame_recon,
                node,
                co_located_luma_mode,
                chroma_x,
                chroma_y,
                chroma_width,
                chroma_height,
                cclm_enabled: cclm_syntax_enabled,
                syntax_tie_breaker_enabled: chroma_syntax_tie_breaker,
                chroma_qp,
                chroma_ts_quant,
                temporal_hint: temporal_chroma_hint,
            }
            .select_candidate(VvcChromaTuSelectionBuffers {
                cache: &mut *chroma_rd_cache,
                prediction_scratch: &mut *prediction_scratch,
                selected: VvcChromaCandidateBuffers {
                    prediction: VvcChromaPredictionBuffers {
                        cb: &mut *predicted_cb,
                        cr: &mut *predicted_cr,
                    },
                    residuals: VvcChromaResidualBuffers {
                        cb: &mut *cb_residuals,
                        cr: &mut *cr_residuals,
                    },
                },
                candidate: VvcChromaCandidateBuffers {
                    prediction: VvcChromaPredictionBuffers {
                        cb: &mut *candidate_cb_prediction,
                        cr: &mut *candidate_cr_prediction,
                    },
                    residuals: VvcChromaResidualBuffers {
                        cb: &mut *candidate_cb_residuals,
                        cr: &mut *candidate_cr_residuals,
                    },
                },
                stats: &mut intra_search_stats,
                transform_scratch: &mut *transform_scratch,
                reconstructed_residual: &mut *reconstructed_residual,
            })
        };
        let VvcSelectedChromaTuCandidate {
            mode: chroma_mode,
            coding_decision: chroma_coding_decision,
            residual: selected_chroma_residual,
        } = selected_chroma_candidate;
        #[cfg(feature = "vvc-stats")]
        {
            residual_energy_stats.add_chroma_residuals(cb_residuals, chroma_width, chroma_height);
            residual_energy_stats.add_chroma_residuals(cr_residuals, chroma_width, chroma_height);
        }
        #[cfg(feature = "vvc-stats")]
        let chroma_finalize_start = StageStart::now();
        let chroma_tu = finalize_vvc_chroma_tu(
            chroma_coding_decision,
            source_frame,
            frame_recon,
            node,
            predicted_cb,
            predicted_cr,
            cb_residuals,
            cr_residuals,
            chroma_width,
            chroma_height,
            chroma_qp,
            chroma_ts_quant,
            vvc_transform_skip_qp_reconstructs_exact(source_frame.format.bit_depth, chroma_qp),
            selected_chroma_residual,
            &mut intra_search_stats,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_chroma_finalize_nanos(chroma_finalize_start.elapsed().as_nanos() as u64);
        chroma_tu_metadata.record_finalized(chroma_tu_count, chroma_mode, chroma_tu);
        #[cfg(feature = "vvc-stats")]
        write_vvc_chroma_tu_trace(
            tu_trace_sink.as_mut(),
            region,
            chroma_tu_count,
            node,
            chroma_mode,
            co_located_luma_mode,
            chroma_tu,
            chroma_width,
            chroma_height,
            predicted_cb,
            predicted_cr,
            cb_residuals,
            cr_residuals,
        );
        chroma_tu_count += 1;
    }

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
