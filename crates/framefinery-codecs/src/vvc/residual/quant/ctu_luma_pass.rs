struct VvcLumaCtuPassContext<'a> {
    shared: VvcCtuSharedPassContext<'a>,
    luma_qp: i32,
    luma_ts_quant: &'a VvcTransformSkipQuantTable,
    luma_inter_decisions: Option<&'a [Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS]>,
    luma_scc_decisions: Option<&'a [Option<VvcIbcCuDecision>; MAX_VVC_LUMA_TUS]>,
}

struct VvcLumaCtuPassBuffers<'a> {
    frame_recon: &'a mut VvcReconstructionFrame,
    mode_search_state: &'a mut VvcLumaModeSearchState,
    nodes: &'a mut Vec<VvcCodingTreeNode>,
    prediction_scratch: &'a mut VvcIntraPredictionScratch,
    selected_prediction: &'a mut Vec<VvcSample>,
    selected_residuals: &'a mut Vec<i16>,
    candidate_prediction: &'a mut Vec<VvcSample>,
    candidate_residuals: &'a mut Vec<i16>,
    rd_cache: &'a mut VvcLumaModeRdCache,
    transform_scratch: &'a mut VvcInverseTransformScratch,
    reconstructed_residual: &'a mut Vec<i16>,
    stats: &'a mut VvcIntraSearchStats,
    #[cfg(feature = "vvc-stats")]
    residual_energy_stats: &'a mut VvcResidualEnergyStats,
    #[cfg(feature = "vvc-stats")]
    trace_sink: Option<&'a mut JsonlInstrumentationSink>,
}

struct VvcLumaCtuPassResult {
    metadata: VvcLumaTuMetadata,
    applied_inter_decisions: [Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS],
    tu_count: usize,
}

impl VvcLumaCtuPassContext<'_> {
    fn quantize(self, buffers: VvcLumaCtuPassBuffers<'_>) -> VvcLumaCtuPassResult {
        let shared = self.shared;
        #[cfg(feature = "vvc-stats")]
        let mut trace_sink = buffers.trace_sink;
        let mut metadata = VvcLumaTuMetadata::new();
        let mut applied_inter_decisions = [None; MAX_VVC_LUMA_TUS];
        let mut tu_count = 0usize;
        vvc_luma_transform_nodes_into_for_kind(
            buffers.nodes,
            shared.ctu_shape,
            shared.policy.luma_max_leaf_size(),
            shared.policy.luma_split_kind(),
        );
        for local_node in buffers.nodes.iter().copied() {
            if tu_count >= MAX_VVC_LUMA_TUS {
                break;
            }
            let node = vvc_global_ctu_node(local_node, shared.region);
            let scc_candidate = self.luma_scc_decisions.and_then(|decisions| {
                decisions.iter().copied().flatten().find(|decision| {
                    decision.origin_x == usize::from(node.x)
                        && decision.origin_y == usize::from(node.y)
                })
            });
            if let Some(decision) = scc_candidate {
                if !shared.ctu_shape.dual_tree_intra
                    && shared.source_frame.format.chroma_sampling == ChromaSampling::Cs444
                    && node.width == 8
                    && node.height == 8
                    && buffers.frame_recon.copy_ibc_444_8x8(decision)
                {
                    metadata.record_scc_decision(tu_count, decision.into_luma_scc_decision());
                    tu_count += 1;
                    continue;
                }
            }
            buffers.rd_cache.reset(shared.policy, node);
            let left_luma_mode = buffers.mode_search_state.left_of(node);
            let above_luma_mode = buffers.mode_search_state.above_of(node);
            let temporal_luma_hint = vvc_luma_temporal_mode_hint(
                shared.temporal_mode_hints,
                tu_count,
                buffers.nodes.len(),
                shared.policy,
                node,
            );
            let inter_decision = self
                .luma_inter_decisions
                .and_then(|decisions| decisions.get(tu_count))
                .copied()
                .flatten();
            let selected_luma_candidate = VvcLumaTuSelectionContext {
                policy: shared.policy,
                metric: shared.score_metric,
                source_frame: shared.source_frame,
                frame_recon: &*buffers.frame_recon,
                mode_search_state: &*buffers.mode_search_state,
                node,
                left: left_luma_mode,
                above: above_luma_mode,
                luma_qp: self.luma_qp,
                luma_ts_quant: self.luma_ts_quant,
                temporal_hint: temporal_luma_hint,
                inter_decision,
                inter_reference: shared.inter_reference,
            }
            .select_candidate(VvcLumaTuSelectionBuffers {
                cache: &mut *buffers.rd_cache,
                prediction_scratch: &mut *buffers.prediction_scratch,
                selected_prediction: &mut *buffers.selected_prediction,
                selected_residuals: &mut *buffers.selected_residuals,
                candidate_prediction: &mut *buffers.candidate_prediction,
                candidate_residuals: &mut *buffers.candidate_residuals,
                stats: &mut *buffers.stats,
                transform_scratch: &mut *buffers.transform_scratch,
                reconstructed_residual: &mut *buffers.reconstructed_residual,
            });
            let VvcSelectedLumaTuCandidate {
                mode: luma_mode,
                coding_decision: luma_coding_decision,
                residual: selected_luma_residual,
                inter_decision: selected_luma_inter_decision,
            } = selected_luma_candidate;
            if selected_luma_inter_decision.is_none() {
                buffers.mode_search_state.mark_node(node, luma_mode);
            }
            #[cfg(feature = "vvc-stats")]
            buffers.residual_energy_stats.add_luma_residuals(
                buffers.selected_residuals,
                usize::from(node.width),
                usize::from(node.height),
            );
            #[cfg(feature = "vvc-stats")]
            let luma_finalize_start = StageStart::now();
            let luma_tu = finalize_vvc_luma_tu(
                luma_coding_decision,
                shared.source_frame,
                buffers.frame_recon,
                node,
                buffers.selected_prediction,
                buffers.selected_residuals,
                self.luma_qp,
                self.luma_ts_quant,
                vvc_transform_skip_qp_reconstructs_exact(
                    shared.source_frame.format.bit_depth,
                    self.luma_qp,
                ),
                selected_luma_residual,
                buffers.stats,
                buffers.transform_scratch,
                buffers.reconstructed_residual,
            );
            #[cfg(feature = "vvc-stats")]
            buffers
                .stats
                .add_luma_finalize_nanos(luma_finalize_start.elapsed().as_nanos() as u64);
            metadata.record_finalized(tu_count, luma_mode, luma_tu);
            applied_inter_decisions[tu_count] = selected_luma_inter_decision;
            #[cfg(feature = "vvc-stats")]
            write_vvc_luma_tu_trace(
                trace_sink.as_deref_mut(),
                shared.region,
                tu_count,
                node,
                luma_mode,
                luma_tu,
                buffers.selected_prediction,
                buffers.selected_residuals,
            );
            tu_count += 1;
        }

        VvcLumaCtuPassResult {
            metadata,
            applied_inter_decisions,
            tu_count,
        }
    }
}

use crate::vvc::cabac::vvc_luma_transform_nodes_into_for_kind;
