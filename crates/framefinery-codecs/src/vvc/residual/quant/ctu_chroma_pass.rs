struct VvcChromaCtuPassContext<'a> {
    shared: VvcCtuSharedPassContext<'a>,
    syntax_tie_breaker_enabled: bool,
    chroma_qp: i32,
    chroma_ts_quant: &'a VvcTransformSkipQuantTable,
    luma_nodes: &'a [VvcCodingTreeNode],
    luma_metadata: &'a VvcLumaTuMetadata,
    applied_luma_inter_decisions: &'a [Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS],
    luma_mode_search_state: &'a VvcLumaModeSearchState,
}

struct VvcChromaCtuPassBuffers<'a> {
    frame_recon: &'a mut VvcReconstructionFrame,
    nodes: &'a mut Vec<VvcCodingTreeNode>,
    prediction_scratch: &'a mut VvcDcPredictionScratch,
    selected: VvcChromaCandidateBuffers<'a>,
    candidate: VvcChromaCandidateBuffers<'a>,
    rd_cache: &'a mut VvcChromaModeRdCache,
    transform_scratch: &'a mut VvcInverseTransformScratch,
    reconstructed_residual: &'a mut Vec<i16>,
    stats: &'a mut VvcIntraSearchStats,
    #[cfg(feature = "vvc-stats")]
    residual_energy_stats: &'a mut VvcResidualEnergyStats,
    #[cfg(feature = "vvc-stats")]
    trace_sink: Option<&'a mut JsonlInstrumentationSink>,
}

struct VvcChromaCtuPassResult {
    metadata: VvcChromaTuMetadata,
    tu_count: usize,
}

impl VvcChromaCtuPassContext<'_> {
    fn quantize(self, mut buffers: VvcChromaCtuPassBuffers<'_>) -> VvcChromaCtuPassResult {
        let shared = self.shared;
        #[cfg(feature = "vvc-stats")]
        let mut trace_sink = buffers.trace_sink;
        let mut metadata = VvcChromaTuMetadata::new();
        let mut tu_count = 0usize;
        if shared.ctu_shape.dual_tree_intra {
            vvc_chroma_transform_nodes_into(buffers.nodes, shared.ctu_shape);
        } else {
            buffers.nodes.clear();
            buffers.nodes.extend(self.luma_nodes.iter().copied());
        }
        for local_node in buffers.nodes.iter().copied() {
            if tu_count >= MAX_VVC_CHROMA_TUS {
                break;
            }
            let node = vvc_global_ctu_node(local_node, shared.region);
            if !shared.ctu_shape.dual_tree_intra
                && matches!(
                    self.luma_metadata.scc_decision(tu_count),
                    Some(VvcLumaSccDecision::IbcExact(_))
                )
            {
                tu_count += 1;
                continue;
            }
            buffers.rd_cache.reset(shared.policy, node);
            let subsample_x = chroma_subsample_x(shared.source_frame.format.chroma_sampling);
            let subsample_y = chroma_subsample_y(shared.source_frame.format.chroma_sampling);
            let chroma_x = usize::from(node.x) / subsample_x;
            let chroma_y = usize::from(node.y) / subsample_y;
            let chroma_width = usize::from(node.width) / subsample_x;
            let chroma_height = usize::from(node.height) / subsample_y;
            let co_located_luma_mode = self
                .luma_mode_search_state
                .co_located_mode_for_chroma_node(node);
            let cclm_syntax_enabled = vvc_chroma_cclm_node_allowed(node);
            let applied_inter_decision = self
                .applied_luma_inter_decisions
                .get(tu_count)
                .copied()
                .flatten();
            let selected_inter_chroma_candidate =
                match (applied_inter_decision, shared.inter_reference) {
                    (Some(decision), Some(reference)) => VvcChromaInterCandidateContext {
                        policy: shared.policy,
                        source_frame: shared.source_frame,
                        node,
                        chroma_x,
                        chroma_y,
                        chroma_width,
                        chroma_height,
                    }
                    .select_candidate(
                        decision,
                        reference,
                        &mut buffers.selected,
                        buffers.stats,
                    ),
                    _ => None,
                };
            let temporal_chroma_hint = selected_inter_chroma_candidate
                .is_none()
                .then(|| {
                    vvc_chroma_temporal_mode_hint(
                        shared.temporal_mode_hints,
                        tu_count,
                        buffers.nodes.len(),
                        shared.policy,
                        shared.source_frame.geometry,
                        node,
                        co_located_luma_mode,
                        chroma_width,
                        chroma_height,
                    )
                })
                .flatten();
            let selected_chroma_candidate = if let Some(candidate) = selected_inter_chroma_candidate
            {
                candidate
            } else {
                VvcChromaTuSelectionContext {
                    policy: shared.policy,
                    metric: shared.score_metric,
                    source_frame: shared.source_frame,
                    frame_recon: &*buffers.frame_recon,
                    node,
                    co_located_luma_mode,
                    chroma_x,
                    chroma_y,
                    chroma_width,
                    chroma_height,
                    cclm_enabled: cclm_syntax_enabled,
                    syntax_tie_breaker_enabled: self.syntax_tie_breaker_enabled,
                    chroma_qp: self.chroma_qp,
                    chroma_ts_quant: self.chroma_ts_quant,
                    temporal_hint: temporal_chroma_hint,
                }
                .select_candidate(VvcChromaTuSelectionBuffers {
                    cache: &mut *buffers.rd_cache,
                    prediction_scratch: &mut *buffers.prediction_scratch,
                    selected: buffers.selected.reborrow(),
                    candidate: buffers.candidate.reborrow(),
                    stats: &mut *buffers.stats,
                    transform_scratch: &mut *buffers.transform_scratch,
                    reconstructed_residual: &mut *buffers.reconstructed_residual,
                })
            };
            let VvcSelectedChromaTuCandidate {
                mode: chroma_mode,
                coding_decision: chroma_coding_decision,
                residual: selected_chroma_residual,
            } = selected_chroma_candidate;
            #[cfg(feature = "vvc-stats")]
            {
                buffers.residual_energy_stats.add_chroma_residuals(
                    buffers.selected.residuals.cb,
                    chroma_width,
                    chroma_height,
                );
                buffers.residual_energy_stats.add_chroma_residuals(
                    buffers.selected.residuals.cr,
                    chroma_width,
                    chroma_height,
                );
            }
            #[cfg(feature = "vvc-stats")]
            let chroma_finalize_start = StageStart::now();
            let chroma_tu = finalize_vvc_chroma_tu(
                chroma_coding_decision,
                shared.source_frame,
                buffers.frame_recon,
                node,
                buffers.selected.prediction.cb,
                buffers.selected.prediction.cr,
                buffers.selected.residuals.cb,
                buffers.selected.residuals.cr,
                chroma_width,
                chroma_height,
                self.chroma_qp,
                self.chroma_ts_quant,
                vvc_transform_skip_qp_reconstructs_exact(
                    shared.source_frame.format.bit_depth,
                    self.chroma_qp,
                ),
                selected_chroma_residual,
                buffers.stats,
                buffers.transform_scratch,
                buffers.reconstructed_residual,
            );
            #[cfg(feature = "vvc-stats")]
            buffers
                .stats
                .add_chroma_finalize_nanos(chroma_finalize_start.elapsed().as_nanos() as u64);
            metadata.record_finalized(tu_count, chroma_mode, chroma_tu);
            #[cfg(feature = "vvc-stats")]
            write_vvc_chroma_tu_trace(
                trace_sink.as_deref_mut(),
                shared.region,
                tu_count,
                node,
                chroma_mode,
                co_located_luma_mode,
                chroma_tu,
                chroma_width,
                chroma_height,
                buffers.selected.prediction.cb,
                buffers.selected.prediction.cr,
                buffers.selected.residuals.cb,
                buffers.selected.residuals.cr,
            );
            tu_count += 1;
        }

        VvcChromaCtuPassResult { metadata, tu_count }
    }
}
