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
    let mut luma_nodes = std::mem::take(&mut scratch.luma_nodes);
    let mut chroma_nodes = std::mem::take(&mut scratch.chroma_nodes);
    let mut prediction_scratch = std::mem::take(&mut scratch.prediction_scratch);
    let mut predicted_luma = std::mem::take(&mut scratch.predicted_luma);
    let mut predicted_cb = std::mem::take(&mut scratch.predicted_cb);
    let mut predicted_cr = std::mem::take(&mut scratch.predicted_cr);
    let mut transform_scratch = std::mem::take(&mut scratch.transform_scratch);
    let mut reconstructed_residual = std::mem::take(&mut scratch.reconstructed_residual);
    let mut luma_residuals = std::mem::take(&mut scratch.luma_residuals);
    let mut candidate_luma_prediction = std::mem::take(&mut scratch.candidate_luma_prediction);
    let mut candidate_luma_residuals = std::mem::take(&mut scratch.candidate_luma_residuals);
    let mut luma_rd_cache =
        std::mem::replace(&mut scratch.luma_rd_cache, VvcLumaModeRdCache::new());
    let mut cb_residuals = std::mem::take(&mut scratch.cb_residuals);
    let mut cr_residuals = std::mem::take(&mut scratch.cr_residuals);
    let mut candidate_cb_prediction = std::mem::take(&mut scratch.candidate_cb_prediction);
    let mut candidate_cr_prediction = std::mem::take(&mut scratch.candidate_cr_prediction);
    let mut candidate_cb_residuals = std::mem::take(&mut scratch.candidate_cb_residuals);
    let mut candidate_cr_residuals = std::mem::take(&mut scratch.candidate_cr_residuals);
    let mut chroma_rd_cache =
        std::mem::replace(&mut scratch.chroma_rd_cache, VvcChromaModeRdCache::new());
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
        &mut luma_nodes,
        ctu_shape,
        luma_max_leaf_size,
        policy.luma_split_kind(),
    );
    for local_node in luma_nodes.iter().copied() {
        if luma_tu_count >= MAX_VVC_LUMA_TUS {
            break;
        }
        let node = vvc_global_ctu_node(local_node, region);
        let scc_candidate = luma_scc_decisions
            .and_then(|decisions| {
                decisions.iter().copied().flatten().find(|decision| {
                    decision.origin_x == usize::from(node.x)
                        && decision.origin_y == usize::from(node.y)
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
                luma_tu_metadata.record_mode_hint(
                    luma_tu_count,
                    hint.mode,
                    hint.bdpcm_mode,
                );
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
        if let Some(hint) = vvc_luma_temporal_mode_hint(
            temporal_mode_hints,
            luma_tu_count,
            luma_nodes.len(),
            policy,
            node,
        ) {
            let luma_mode = hint.mode;
            #[cfg(feature = "vvc-stats")]
            let luma_finalize_start = StageStart::now();
            if let Some(luma_tu) = finalize_vvc_luma_tu_with_temporal_mode_hint(
                hint,
                policy,
                source_frame,
                frame_recon,
                node,
                luma_qp,
                luma_ts_quant,
                &mut prediction_scratch,
                &mut predicted_luma,
                &mut luma_residuals,
                &mut intra_search_stats,
                &mut transform_scratch,
                &mut reconstructed_residual,
            ) {
                #[cfg(feature = "vvc-stats")]
                intra_search_stats
                    .add_luma_finalize_nanos(luma_finalize_start.elapsed().as_nanos() as u64);
                luma_mode_search_state.mark_node(node, luma_mode);
                #[cfg(feature = "vvc-stats")]
                residual_energy_stats.add_luma_residuals(
                    &luma_residuals,
                    usize::from(node.width),
                    usize::from(node.height),
                );
                luma_tu_metadata.record_finalized(luma_tu_count, luma_mode, luma_tu);
                #[cfg(feature = "vvc-stats")]
                write_vvc_luma_tu_trace(
                    tu_trace_sink.as_mut(),
                    region,
                    luma_tu_count,
                    node,
                    luma_mode,
                    luma_tu,
                    &predicted_luma,
                    &luma_residuals,
                );
                luma_tu_count += 1;
                continue;
            }
        }
        if let Some(decision) = luma_inter_decisions
            .and_then(|decisions| decisions.get(luma_tu_count))
            .copied()
            .flatten()
        {
            if let Some(reference) = inter_reference {
                #[cfg(feature = "vvc-stats")]
                let luma_finalize_start = StageStart::now();
                if let Some(luma_tu) = finalize_vvc_luma_exact_explicit_inter_candidate(
                    decision,
                    policy,
                    source_frame,
                    reference,
                    frame_recon,
                    node,
                    luma_qp,
                    luma_ts_quant,
                    &mut candidate_luma_prediction,
                    &mut candidate_luma_residuals,
                    &mut intra_search_stats,
                    &mut transform_scratch,
                    &mut reconstructed_residual,
                ) {
                    #[cfg(feature = "vvc-stats")]
                    intra_search_stats
                        .add_luma_finalize_nanos(luma_finalize_start.elapsed().as_nanos() as u64);
                    let luma_mode = VvcIntraPredictionMode::Dc;
                    #[cfg(feature = "vvc-stats")]
                    residual_energy_stats.add_luma_residuals(
                        &candidate_luma_residuals,
                        usize::from(node.width),
                        usize::from(node.height),
                    );
                    luma_tu_metadata.record_finalized(luma_tu_count, luma_mode, luma_tu);
                    applied_luma_inter_decisions[luma_tu_count] = Some(decision);
                    #[cfg(feature = "vvc-stats")]
                    write_vvc_luma_tu_trace(
                        tu_trace_sink.as_mut(),
                        region,
                        luma_tu_count,
                        node,
                        luma_mode,
                        luma_tu,
                        &candidate_luma_prediction,
                        &candidate_luma_residuals,
                    );
                    luma_tu_count += 1;
                    continue;
                }
            }
        }
        #[cfg(feature = "vvc-stats")]
        let luma_mode_search_start = StageStart::now();
        let VvcLumaModeSearchResult {
            mode: raw_luma_mode,
            candidate_costs: luma_candidate_costs,
        } = {
            let search_context = VvcLumaModeSearchContext {
                policy,
                metric: score_metric,
                source_frame,
                frame_recon,
                mode_search_state: luma_mode_search_state,
                node,
                left: left_luma_mode,
                above: above_luma_mode,
            };
            search_context.select_intra_mode(VvcLumaModeSearchBuffers {
                cache: &mut luma_rd_cache,
                prediction_scratch: &mut prediction_scratch,
                selected_prediction: &mut predicted_luma,
                candidate_prediction: &mut candidate_luma_prediction,
                candidate_residuals: &mut candidate_luma_residuals,
                stats: &mut intra_search_stats,
            })
        };
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_luma_mode_search_nanos(luma_mode_search_start.elapsed().as_nanos() as u64);
        if luma_rd_cache.get(raw_luma_mode).is_some() {
            luma_rd_cache.take_residuals(raw_luma_mode, &mut luma_residuals);
        } else {
            #[cfg(feature = "vvc-stats")]
            let residual_start = StageStart::now();
            residual_luma_tu_at_into(
                &mut luma_residuals,
                source_frame,
                usize::from(node.x),
                usize::from(node.y),
                usize::from(node.width),
                usize::from(node.height),
                &predicted_luma,
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_luma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        }
        #[cfg(feature = "vvc-stats")]
        let luma_rd_start = StageStart::now();
        let selected_luma_mode = select_vvc_luma_mode_with_rd_refinement(
            policy,
            node,
            raw_luma_mode,
            luma_candidate_costs,
            &mut luma_rd_cache,
            &mut intra_search_stats,
            left_luma_mode,
            above_luma_mode,
            source_frame,
            frame_recon,
            luma_qp,
            luma_ts_quant,
            &mut prediction_scratch,
            &mut predicted_luma,
            &mut luma_residuals,
            &mut candidate_luma_prediction,
            &mut candidate_luma_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_rd_refinement_nanos(luma_rd_start.elapsed().as_nanos() as u64);
        #[cfg(feature = "vvc-stats")]
        if selected_luma_mode.residual.is_some() {
            intra_search_stats.add_luma_rd_refinement_attempt();
            if selected_luma_mode.mode != raw_luma_mode {
                intra_search_stats.add_luma_rd_refinement_switch();
            }
        }
        let mut luma_mode = selected_luma_mode.mode;
        let mut luma_coding_decision = policy.select_luma_tu_coding_decision(node, luma_mode);
        #[cfg(feature = "vvc-stats")]
        let luma_mrl_start = StageStart::now();
        let selected_luma_mrl = select_vvc_luma_mrl_prediction(
            policy,
            luma_coding_decision.residual_coding,
            luma_coding_decision.mts_index,
            node,
            luma_mode,
            left_luma_mode,
            above_luma_mode,
            luma_qp,
            luma_ts_quant,
            selected_luma_mode.residual,
            &mut intra_search_stats,
            frame_recon,
            source_frame,
            &mut prediction_scratch,
            &mut predicted_luma,
            &mut luma_residuals,
            &mut candidate_luma_prediction,
            &mut candidate_luma_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        );
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_mrl_nanos(luma_mrl_start.elapsed().as_nanos() as u64);
        luma_coding_decision.mrl_index = selected_luma_mrl.mrl_index;
        let mut selected_luma_residual = selected_luma_mrl.residual;
        #[cfg(feature = "vvc-stats")]
        let luma_bdpcm_start = StageStart::now();
        if let Some(selected_bdpcm) = select_vvc_luma_bdpcm_prediction(
            policy,
            node,
            luma_mode,
            luma_coding_decision,
            left_luma_mode,
            above_luma_mode,
            luma_qp,
            luma_ts_quant,
            selected_luma_residual,
            &mut intra_search_stats,
            frame_recon,
            source_frame,
            &mut prediction_scratch,
            &mut predicted_luma,
            &mut luma_residuals,
            &mut candidate_luma_prediction,
            &mut candidate_luma_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        ) {
            luma_mode = selected_bdpcm.mode;
            luma_coding_decision = selected_bdpcm.coding_decision;
            selected_luma_residual = Some(selected_bdpcm.residual);
        }
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_luma_bdpcm_nanos(luma_bdpcm_start.elapsed().as_nanos() as u64);
        let mut selected_luma_inter_decision = None;
        if let Some(decision) = luma_inter_decisions
            .and_then(|decisions| decisions.get(luma_tu_count))
            .copied()
            .flatten()
        {
            if let Some(reference) = inter_reference {
                if let Some(inter_residual) = select_vvc_luma_explicit_inter_candidate(
                    decision,
                    luma_mode,
                    luma_coding_decision,
                    selected_luma_residual,
                    left_luma_mode,
                    above_luma_mode,
                    policy,
                    source_frame,
                    reference,
                    node,
                    luma_qp,
                    luma_ts_quant,
                    &mut candidate_luma_prediction,
                    &mut candidate_luma_residuals,
                    &mut intra_search_stats,
                    &mut transform_scratch,
                    &mut reconstructed_residual,
                ) {
                    luma_mode = VvcIntraPredictionMode::Dc;
                    luma_coding_decision =
                        policy.select_luma_tu_coding_decision(node, luma_mode);
                    selected_luma_residual = Some(inter_residual);
                    selected_luma_inter_decision = Some(decision);
                    std::mem::swap(&mut predicted_luma, &mut candidate_luma_prediction);
                    std::mem::swap(&mut luma_residuals, &mut candidate_luma_residuals);
                }
            }
        }
        if selected_luma_inter_decision.is_none() {
            luma_mode_search_state.mark_node(node, luma_mode);
        }
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats.add_luma_residuals(
            &luma_residuals,
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
            &predicted_luma,
            &luma_residuals,
            luma_qp,
            luma_ts_quant,
            vvc_transform_skip_qp_reconstructs_exact(source_frame.format.bit_depth, luma_qp),
            selected_luma_residual,
            &mut intra_search_stats,
            &mut transform_scratch,
            &mut reconstructed_residual,
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
            &predicted_luma,
            &luma_residuals,
        );
        luma_tu_count += 1;
    }

    let mut chroma_tu_count = 0usize;
    if ctu_shape.dual_tree_intra {
        vvc_chroma_transform_nodes_into(&mut chroma_nodes, ctu_shape);
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
        if let Some(decision) = applied_luma_inter_decisions
            .get(chroma_tu_count)
            .copied()
            .flatten()
        {
            if let Some(reference) = inter_reference {
                if VvcReconstructionFrame::predict_chroma_node_from_inter_motion_into(
                    reference,
                    &mut predicted_cb,
                    &mut predicted_cr,
                    node,
                    decision,
                ) {
                    #[cfg(feature = "vvc-stats")]
                    let residual_start = StageStart::now();
                    let (cb_residuals_all_zero, cr_residuals_all_zero) =
                        residual_chroma_pair_tu_at_into_and_detect_zero(
                        &mut cb_residuals,
                        &mut cr_residuals,
                        &source_frame.cb,
                        &source_frame.cr,
                        source_frame.geometry,
                        source_frame.format,
                        chroma_x,
                        chroma_y,
                        chroma_width,
                        chroma_height,
                        &predicted_cb,
                        &predicted_cr,
                    );
                    #[cfg(feature = "vvc-stats")]
                    intra_search_stats
                        .add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
                    let preselected_residual = if cb_residuals_all_zero && cr_residuals_all_zero {
                        Some(vvc_zero_chroma_preselected_residual())
                    } else {
                        None
                    };
                    let chroma_coding_decision = policy.select_chroma_tu_coding_decision(
                        node,
                        VvcChromaIntraPredictionMode::Derived,
                    );
                    #[cfg(feature = "vvc-stats")]
                    residual_energy_stats.add_chroma_residuals(
                        &cb_residuals,
                        chroma_width,
                        chroma_height,
                    );
                    #[cfg(feature = "vvc-stats")]
                    residual_energy_stats.add_chroma_residuals(
                        &cr_residuals,
                        chroma_width,
                        chroma_height,
                    );
                    #[cfg(feature = "vvc-stats")]
                    let chroma_finalize_start = StageStart::now();
                    let chroma_tu = finalize_vvc_chroma_tu(
                        chroma_coding_decision,
                        source_frame,
                        frame_recon,
                        node,
                        &predicted_cb,
                        &predicted_cr,
                        &cb_residuals,
                        &cr_residuals,
                        chroma_width,
                        chroma_height,
                        chroma_qp,
                        chroma_ts_quant,
                        vvc_transform_skip_qp_reconstructs_exact(
                            source_frame.format.bit_depth,
                            chroma_qp,
                        ),
                        preselected_residual,
                        &mut intra_search_stats,
                        &mut transform_scratch,
                        &mut reconstructed_residual,
                    );
                    #[cfg(feature = "vvc-stats")]
                    intra_search_stats.add_chroma_finalize_nanos(
                        chroma_finalize_start.elapsed().as_nanos() as u64,
                    );
                    chroma_tu_metadata.record_finalized(
                        chroma_tu_count,
                        VvcChromaIntraPredictionMode::Derived,
                        chroma_tu,
                    );
                    chroma_tu_count += 1;
                    continue;
                }
            }
        }
        if chroma_inter_skip
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
                chroma_tu_metadata.record_mode_hint(
                    chroma_tu_count,
                    hint.mode,
                    hint.bdpcm_mode,
                );
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
            #[cfg(feature = "vvc-stats")]
            let chroma_finalize_start = StageStart::now();
            if let Some(chroma_tu) = finalize_vvc_chroma_tu_with_temporal_mode_hint(
                hint,
                policy,
                source_frame,
                frame_recon,
                node,
                co_located_luma_mode,
                chroma_width,
                chroma_height,
                chroma_qp,
                chroma_ts_quant,
                &mut prediction_scratch,
                &mut predicted_cb,
                &mut predicted_cr,
                &mut cb_residuals,
                &mut cr_residuals,
                &mut intra_search_stats,
                &mut transform_scratch,
                &mut reconstructed_residual,
            ) {
                #[cfg(feature = "vvc-stats")]
                intra_search_stats
                    .add_chroma_finalize_nanos(chroma_finalize_start.elapsed().as_nanos() as u64);
                #[cfg(feature = "vvc-stats")]
                {
                    residual_energy_stats.add_chroma_residuals(
                        &cb_residuals,
                        chroma_width,
                        chroma_height,
                    );
                    residual_energy_stats.add_chroma_residuals(
                        &cr_residuals,
                        chroma_width,
                        chroma_height,
                    );
                }
                chroma_tu_metadata.record_finalized(chroma_tu_count, hint.mode, chroma_tu);
                #[cfg(feature = "vvc-stats")]
                write_vvc_chroma_tu_trace(
                    tu_trace_sink.as_mut(),
                    region,
                    chroma_tu_count,
                    node,
                    hint.mode,
                    co_located_luma_mode,
                    chroma_tu,
                    chroma_width,
                    chroma_height,
                    &predicted_cb,
                    &predicted_cr,
                    &cb_residuals,
                    &cr_residuals,
                );
                chroma_tu_count += 1;
                continue;
            }
        }
        let chroma_mode_search_context = VvcChromaModeSearchContext {
            policy,
            metric: score_metric,
            source_frame,
            frame_recon,
            node,
            co_located_luma_mode,
            chroma_x,
            chroma_y,
            chroma_width,
            chroma_height,
            cclm_enabled: cclm_syntax_enabled,
            syntax_tie_breaker_enabled: chroma_syntax_tie_breaker,
        };
        #[cfg(feature = "vvc-stats")]
        let chroma_mode_search_start = StageStart::now();
        let VvcChromaModeSearchResult {
            mode: raw_chroma_mode,
            candidate_costs: chroma_candidate_costs,
        } = chroma_mode_search_context.select_intra_mode(VvcChromaModeSearchBuffers {
            cache: &mut chroma_rd_cache,
            prediction_scratch: &mut prediction_scratch,
            selected_cb_prediction: &mut predicted_cb,
            selected_cr_prediction: &mut predicted_cr,
            candidate_cb_prediction: &mut candidate_cb_prediction,
            candidate_cr_prediction: &mut candidate_cr_prediction,
            candidate_cb_residuals: &mut candidate_cb_residuals,
            candidate_cr_residuals: &mut candidate_cr_residuals,
            stats: &mut intra_search_stats,
        });
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_chroma_mode_search_nanos(chroma_mode_search_start.elapsed().as_nanos() as u64);
        if chroma_rd_cache.get(raw_chroma_mode).is_some() {
            chroma_rd_cache.take_residuals(
                raw_chroma_mode,
                &mut cb_residuals,
                &mut cr_residuals,
            );
        } else {
            #[cfg(feature = "vvc-stats")]
            let residual_start = StageStart::now();
            residual_chroma_tu_at_into(
                &mut cb_residuals,
                &source_frame.cb,
                source_frame.geometry,
                source_frame.format,
                chroma_x,
                chroma_y,
                chroma_width,
                chroma_height,
                &predicted_cb,
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
            #[cfg(feature = "vvc-stats")]
            let residual_start = StageStart::now();
            residual_chroma_tu_at_into(
                &mut cr_residuals,
                &source_frame.cr,
                source_frame.geometry,
                source_frame.format,
                chroma_x,
                chroma_y,
                chroma_width,
                chroma_height,
                &predicted_cr,
            );
            #[cfg(feature = "vvc-stats")]
            intra_search_stats.add_chroma_residual_build_nanos(vvc_elapsed_nanos(residual_start));
        }
        #[cfg(feature = "vvc-stats")]
        let chroma_rd_start = StageStart::now();
        let selected_chroma_mode = if vvc_chroma_lossy_speed_direct_bdpcm_candidates_allowed(
            policy,
            source_frame.format.chroma_sampling,
            source_frame.format.bit_depth,
            raw_chroma_mode,
        )
        {
            VvcSelectedChromaMode {
                mode: raw_chroma_mode,
                residual: None,
            }
        } else {
            select_vvc_chroma_mode_with_rd_refinement(
                policy,
                node,
                raw_chroma_mode,
                chroma_candidate_costs,
                &mut chroma_rd_cache,
                &mut intra_search_stats,
                co_located_luma_mode,
                cclm_syntax_enabled,
                source_frame,
                frame_recon,
                chroma_width,
                chroma_height,
                chroma_qp,
                chroma_ts_quant,
                &mut prediction_scratch,
                &mut predicted_cb,
                &mut predicted_cr,
                &mut cb_residuals,
                &mut cr_residuals,
                &mut candidate_cb_prediction,
                &mut candidate_cr_prediction,
                &mut candidate_cb_residuals,
                &mut candidate_cr_residuals,
                &mut transform_scratch,
                &mut reconstructed_residual,
            )
        };
        #[cfg(feature = "vvc-stats")]
        intra_search_stats
            .add_chroma_rd_refinement_nanos(chroma_rd_start.elapsed().as_nanos() as u64);
        #[cfg(feature = "vvc-stats")]
        if selected_chroma_mode.residual.is_some() {
            intra_search_stats.add_chroma_rd_refinement_attempt();
            if selected_chroma_mode.mode != raw_chroma_mode {
                intra_search_stats.add_chroma_rd_refinement_switch();
            }
        }
        let mut chroma_mode = selected_chroma_mode.mode;
        let mut selected_chroma_residual = selected_chroma_mode.residual;
        #[cfg(feature = "vvc-stats")]
        let chroma_bdpcm_start = StageStart::now();
        if let Some(selected_bdpcm) = select_vvc_chroma_bdpcm_prediction(
            policy,
            node,
            chroma_mode,
            co_located_luma_mode,
            cclm_syntax_enabled,
            source_frame,
            frame_recon,
            chroma_width,
            chroma_height,
            chroma_qp,
            chroma_ts_quant,
            selected_chroma_residual,
            &mut intra_search_stats,
            &mut prediction_scratch,
            &mut predicted_cb,
            &mut predicted_cr,
            &mut cb_residuals,
            &mut cr_residuals,
            &mut candidate_cb_prediction,
            &mut candidate_cr_prediction,
            &mut candidate_cb_residuals,
            &mut candidate_cr_residuals,
            &mut transform_scratch,
            &mut reconstructed_residual,
        ) {
            chroma_mode = selected_bdpcm.mode;
            selected_chroma_residual = Some(selected_bdpcm.residual);
        }
        #[cfg(feature = "vvc-stats")]
        intra_search_stats.add_chroma_bdpcm_nanos(chroma_bdpcm_start.elapsed().as_nanos() as u64);
        let chroma_coding_decision = policy.select_chroma_tu_coding_decision(node, chroma_mode);
        #[cfg(feature = "vvc-stats")]
        {
            residual_energy_stats.add_chroma_residuals(&cb_residuals, chroma_width, chroma_height);
            residual_energy_stats.add_chroma_residuals(&cr_residuals, chroma_width, chroma_height);
        }
        #[cfg(feature = "vvc-stats")]
        let chroma_finalize_start = StageStart::now();
        let chroma_tu = finalize_vvc_chroma_tu(
            chroma_coding_decision,
            source_frame,
            frame_recon,
            node,
            &predicted_cb,
            &predicted_cr,
            &cb_residuals,
            &cr_residuals,
            chroma_width,
            chroma_height,
            chroma_qp,
            chroma_ts_quant,
            vvc_transform_skip_qp_reconstructs_exact(source_frame.format.bit_depth, chroma_qp),
            selected_chroma_residual,
            &mut intra_search_stats,
            &mut transform_scratch,
            &mut reconstructed_residual,
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
            &predicted_cb,
            &predicted_cr,
            &cb_residuals,
            &cr_residuals,
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
    scratch.prediction_scratch = prediction_scratch;
    scratch.predicted_luma = predicted_luma;
    scratch.predicted_cb = predicted_cb;
    scratch.predicted_cr = predicted_cr;
    scratch.transform_scratch = transform_scratch;
    scratch.reconstructed_residual = reconstructed_residual;
    scratch.luma_residuals = luma_residuals;
    scratch.candidate_luma_prediction = candidate_luma_prediction;
    scratch.candidate_luma_residuals = candidate_luma_residuals;
    scratch.luma_rd_cache = luma_rd_cache;
    scratch.cb_residuals = cb_residuals;
    scratch.cr_residuals = cr_residuals;
    scratch.candidate_cb_prediction = candidate_cb_prediction;
    scratch.candidate_cr_prediction = candidate_cr_prediction;
    scratch.candidate_cb_residuals = candidate_cb_residuals;
    scratch.candidate_cr_residuals = candidate_cr_residuals;
    scratch.chroma_rd_cache = chroma_rd_cache;
    scratch.luma_nodes = luma_nodes;
    scratch.chroma_nodes = chroma_nodes;
    if let Some(selected) = selected_luma_inter_decisions {
        *selected = applied_luma_inter_decisions;
    }
    quantized
}

use crate::vvc::cabac::vvc_luma_transform_nodes_into_for_kind;
