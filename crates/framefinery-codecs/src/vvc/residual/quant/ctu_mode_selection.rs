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
    luma_inter_skip: Option<&[bool; MAX_VVC_LUMA_TUS]>,
    chroma_inter_skip: Option<&[bool; MAX_VVC_CHROMA_TUS]>,
    luma_inter_decisions: Option<&[Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS]>,
    luma_scc_decisions: Option<&[Option<VvcIbcCuDecision>; MAX_VVC_LUMA_TUS]>,
    inter_reference: Option<&VvcReconstructionFrame>,
    selected_luma_inter_decisions: Option<&mut [Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS]>,
    temporal_mode_hints: Option<&VvcQuantizedColor>,
) -> VvcQuantizedColor {
    let mut luma_tu_metadata = VvcLumaTuMetadata::new();
    let mut chroma_tu_metadata = VvcChromaTuMetadata::new();
    let mut applied_luma_inter_decisions = [None; MAX_VVC_LUMA_TUS];
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
    let luma_max_leaf_size = policy.luma_max_leaf_size();
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

    let mut luma_tu_count = 0usize;
    vvc_luma_transform_nodes_into_for_kind(
        luma_nodes,
        ctu_shape,
        luma_max_leaf_size,
        policy.luma_split_kind(),
    );
    for local_node in luma_nodes.iter().copied() {
        if luma_tu_count >= MAX_VVC_LUMA_TUS {
            break;
        }
        let node = vvc_global_ctu_node(local_node, region);
        let scc_candidate = luma_scc_decisions.and_then(|decisions| {
            decisions.iter().copied().flatten().find(|decision| {
                decision.origin_x == usize::from(node.x) && decision.origin_y == usize::from(node.y)
            })
        });
        if let Some(decision) = scc_candidate {
            if !ctu_shape.dual_tree_intra
                && source_frame.format.chroma_sampling == ChromaSampling::Cs444
                && node.width == 8
                && node.height == 8
                && frame_recon.copy_ibc_444_8x8(decision)
            {
                luma_tu_metadata
                    .record_scc_decision(luma_tu_count, decision.into_luma_scc_decision());
                luma_tu_count += 1;
                continue;
            }
        }
        if luma_inter_skip
            .and_then(|mask| mask.get(luma_tu_count))
            .copied()
            .unwrap_or(false)
        {
            if let Some(hint) = vvc_luma_temporal_mode_hint(
                temporal_mode_hints,
                luma_tu_count,
                luma_nodes.len(),
                policy,
                node,
            ) {
                luma_tu_metadata.record_mode_hint(luma_tu_count, hint.mode, hint.bdpcm_mode);
                luma_mode_search_state.mark_node(node, hint.mode);
            }
            copy_source_luma_node_into_reconstruction(frame_recon, source_frame, node);
            frame_recon.mark_luma_node_available(node);
            luma_tu_count += 1;
            continue;
        }
        luma_rd_cache.reset(policy, node);
        let left_luma_mode = luma_mode_search_state.left_of(node);
        let above_luma_mode = luma_mode_search_state.above_of(node);
        let temporal_luma_hint = vvc_luma_temporal_mode_hint(
            temporal_mode_hints,
            luma_tu_count,
            luma_nodes.len(),
            policy,
            node,
        );
        let inter_decision = luma_inter_decisions
            .and_then(|decisions| decisions.get(luma_tu_count))
            .copied()
            .flatten();
        let selected_luma_candidate = VvcLumaTuSelectionContext {
            policy,
            metric: score_metric,
            source_frame,
            frame_recon: &*frame_recon,
            mode_search_state: &*luma_mode_search_state,
            node,
            left: left_luma_mode,
            above: above_luma_mode,
            luma_qp,
            luma_ts_quant,
            temporal_hint: temporal_luma_hint,
            inter_decision,
            inter_reference,
        }
        .select_candidate(VvcLumaTuSelectionBuffers {
            cache: &mut *luma_rd_cache,
            prediction_scratch: &mut *prediction_scratch,
            selected_prediction: &mut *predicted_luma,
            selected_residuals: &mut *luma_residuals,
            candidate_prediction: &mut *candidate_luma_prediction,
            candidate_residuals: &mut *candidate_luma_residuals,
            stats: &mut intra_search_stats,
            transform_scratch: &mut *transform_scratch,
            reconstructed_residual: &mut *reconstructed_residual,
        });
        let VvcSelectedLumaTuCandidate {
            mode: luma_mode,
            coding_decision: luma_coding_decision,
            residual: selected_luma_residual,
            inter_decision: selected_luma_inter_decision,
        } = selected_luma_candidate;
        if selected_luma_inter_decision.is_none() {
            luma_mode_search_state.mark_node(node, luma_mode);
        }
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats.add_luma_residuals(
            luma_residuals,
            usize::from(node.width),
            usize::from(node.height),
        );
        #[cfg(feature = "vvc-stats")]
        let luma_finalize_start = StageStart::now();
        let luma_tu = finalize_vvc_luma_tu(
            luma_coding_decision,
            source_frame,
            frame_recon,
            node,
            predicted_luma,
            luma_residuals,
            luma_qp,
            luma_ts_quant,
            vvc_transform_skip_qp_reconstructs_exact(source_frame.format.bit_depth, luma_qp),
            selected_luma_residual,
            &mut intra_search_stats,
            transform_scratch,
            reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_finalize_nanos(luma_finalize_start.elapsed().as_nanos() as u64);
        luma_tu_metadata.record_finalized(luma_tu_count, luma_mode, luma_tu);
        applied_luma_inter_decisions[luma_tu_count] = selected_luma_inter_decision;
        #[cfg(feature = "vvc-stats")]
        write_vvc_luma_tu_trace(
            tu_trace_sink.as_mut(),
            region,
            luma_tu_count,
            node,
            luma_mode,
            luma_tu,
            predicted_luma,
            luma_residuals,
        );
        luma_tu_count += 1;
    }

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
        if selected_inter_chroma_candidate.is_none()
            && chroma_inter_skip
                .and_then(|mask| mask.get(chroma_tu_count))
                .copied()
                .unwrap_or(false)
        {
            if let Some(hint) = vvc_chroma_temporal_mode_hint(
                temporal_mode_hints,
                chroma_tu_count,
                chroma_nodes.len(),
                policy,
                source_frame.geometry,
                node,
                co_located_luma_mode,
                chroma_width,
                chroma_height,
            ) {
                chroma_tu_metadata.record_mode_hint(chroma_tu_count, hint.mode, hint.bdpcm_mode);
            }
            let coded_geometry = frame_recon.coded_geometry();
            copy_source_chroma_node_into_reconstruction(
                &mut frame_recon.cb,
                &source_frame.cb,
                source_frame.geometry,
                coded_geometry,
                source_frame.format,
                node,
            );
            copy_source_chroma_node_into_reconstruction(
                &mut frame_recon.cr,
                &source_frame.cr,
                source_frame.geometry,
                coded_geometry,
                source_frame.format,
                node,
            );
            frame_recon.mark_chroma_node_available(node);
            chroma_tu_count += 1;
            continue;
        }
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

    let color = source_frame.sampled_color();
    let cb_rem = quantize_vvc_chroma_sample(vvc_downshift_sample_to_u8(
        color.u,
        source_frame.format.bit_depth,
    ));
    let cr_rem = quantize_vvc_chroma_sample(vvc_downshift_sample_to_u8(
        color.v,
        source_frame.format.bit_depth,
    ));
    let VvcLumaTuMetadata {
        luma_tu_intra_modes,
        luma_tu_remainders,
        luma_tu_negative,
        luma_tu_dc_levels,
        luma_tu_ac_levels,
        luma_tu_has_ac,
        luma_tu_scc_decisions,
        luma_tu_transform_skip,
        luma_tu_bdpcm_modes,
        luma_tu_mrl_index,
        luma_tu_mts_index,
    } = luma_tu_metadata;
    let VvcChromaTuMetadata {
        chroma_tu_intra_modes,
        cb_tu_dc_levels,
        cr_tu_dc_levels,
        cb_tu_ac_levels,
        cr_tu_ac_levels,
        cb_tu_has_ac,
        cr_tu_has_ac,
        cb_tu_transform_skip,
        cr_tu_transform_skip,
        chroma_tu_bdpcm_modes,
    } = chroma_tu_metadata;
    let quantized = VvcQuantizedColor {
        y: vvc_downshift_sample_to_u8(color.y, source_frame.format.bit_depth),
        u: finalized_vvc_chroma_sample(
            cb_tu_transform_skip.first().copied().unwrap_or(false),
            color.u,
            cb_rem,
            source_frame.format.bit_depth,
        ),
        v: finalized_vvc_chroma_sample(
            cr_tu_transform_skip.first().copied().unwrap_or(false),
            color.v,
            cr_rem,
            source_frame.format.bit_depth,
        ),
        luma_tu_intra_modes,
        luma_tu_remainders,
        luma_tu_negative,
        luma_tu_dc_levels,
        luma_tu_ac_levels,
        luma_tu_has_ac,
        luma_tu_scc_decisions,
        luma_tu_transform_skip,
        luma_tu_bdpcm_modes,
        luma_tu_mrl_index,
        luma_tu_mts_index,
        luma_tu_count,
        chroma_tu_count,
        chroma_tu_intra_modes,
        cb_tu_dc_levels,
        cr_tu_dc_levels,
        cb_tu_ac_levels,
        cr_tu_ac_levels,
        cb_tu_has_ac,
        cr_tu_has_ac,
        cb_tu_transform_skip,
        cr_tu_transform_skip,
        chroma_tu_bdpcm_modes,
        cb_rem,
        cr_rem,
        #[cfg(feature = "vvc-stats")]
        intra_search_stats,
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats,
    };
    if let Some(selected) = selected_luma_inter_decisions {
        *selected = applied_luma_inter_decisions;
    }
    quantized
}

use crate::vvc::cabac::vvc_luma_transform_nodes_into_for_kind;
