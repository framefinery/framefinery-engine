use super::*;
use crate::vvc::VvcLumaIbcDecision;

fn sampled_luma_frame(width: usize, height: usize, luma: Vec<VvcSample>) -> VvcSampledFrame {
    assert_eq!(luma.len(), width * height);
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let chroma_len = (width / 2) * (height / 2);
    VvcSampledFrame {
        geometry: VvcVideoGeometry { width, height },
        format,
        luma,
        cb: vec![128; chroma_len],
        cr: vec![128; chroma_len],
        chroma_len,
    }
}

fn shifted_444_frame_and_reference() -> (VvcSampledFrame, VvcReconstructionFrame) {
    let geometry = VvcVideoGeometry {
        width: 16,
        height: 8,
    };
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let plane_len = geometry.luma_samples();
    let previous_luma: Vec<_> = (0..plane_len)
        .map(|index| ((index * 13 + 7) & 0xff) as VvcSample)
        .collect();
    let previous_cb: Vec<_> = (0..plane_len)
        .map(|index| ((index * 17 + 19) & 0xff) as VvcSample)
        .collect();
    let previous_cr: Vec<_> = (0..plane_len)
        .map(|index| ((index * 23 + 31) & 0xff) as VvcSample)
        .collect();
    let shifted_left_half = |previous: &[VvcSample]| {
        let mut current = vec![0; plane_len];
        for row in 0..geometry.height {
            let previous_start = row * geometry.width + 8;
            let current_start = row * geometry.width;
            current[current_start..current_start + 8]
                .copy_from_slice(&previous[previous_start..previous_start + 8]);
        }
        current
    };
    let source_frame = VvcSampledFrame {
        geometry,
        format,
        luma: shifted_left_half(&previous_luma),
        cb: shifted_left_half(&previous_cb),
        cr: shifted_left_half(&previous_cr),
        chroma_len: plane_len,
    };
    let mut reference = VvcReconstructionFrame::new_neutral(geometry, format);
    reference.luma = previous_luma;
    reference.cb = previous_cb;
    reference.cr = previous_cr;
    (source_frame, reference)
}

fn test_intra_search_stats() -> VvcIntraSearchStats {
    #[cfg(feature = "vvc-stats")]
    {
        VvcIntraSearchStats::default()
    }
    #[cfg(not(feature = "vvc-stats"))]
    {
        VvcIntraSearchStats
    }
}

#[test]
fn vvc_luma_residual_materializer_matches_scalar_residuals() {
    let frame = sampled_luma_frame(
        4,
        4,
        vec![0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60],
    );
    let node = VvcCodingTreeNode::root(4, 4, VvcTreeType::DualTreeLuma);
    let prediction = vec![3; 16];
    let mut residuals = Vec::new();
    let mut stats = test_intra_search_stats();

    materialize_vvc_luma_tu_residuals(&mut residuals, &frame, node, &prediction, &mut stats);

    assert_eq!(
        residuals,
        vec![-3, 1, 5, 9, 13, 17, 21, 25, 29, 33, 37, 41, 45, 49, 53, 57]
    );
}

#[test]
fn vvc_luma_rd_cache_transfers_only_present_candidate_residuals() {
    let frame = sampled_luma_frame(8, 8, vec![128; 64]);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    let policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy);
    let cached_mode = VvcIntraPredictionMode::Planar;
    let cached_residuals = vec![1, -2, 3, -4];
    let mut cache = VvcLumaModeRdCache::new();
    cache.reset(policy, node);
    cache.consider(cached_mode, 10, &cached_residuals);
    let mut selected_residuals = vec![99];

    assert!(cache.take_residuals_if_present(cached_mode, &mut selected_residuals));
    assert_eq!(selected_residuals, cached_residuals);

    let unchanged = selected_residuals.clone();
    assert!(!cache
        .take_residuals_if_present(VvcIntraPredictionMode::Horizontal, &mut selected_residuals,));
    assert_eq!(selected_residuals, unchanged);
}

#[test]
fn vvc_chroma_rd_cache_transfers_only_present_candidate_residuals() {
    let frame = sampled_luma_frame(8, 8, vec![128; 64]);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeChroma);
    let policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy);
    let cached_mode = VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Planar);
    let cached_cb_residuals = vec![1, -2, 3, -4];
    let cached_cr_residuals = vec![-5, 6, -7, 8];
    let mut cache = VvcChromaModeRdCache::new();
    cache.reset(policy, node);
    cache.consider(cached_mode, 10, &cached_cb_residuals, &cached_cr_residuals);
    let mut selected_cb_residuals = vec![99];
    let mut selected_cr_residuals = vec![100];

    assert!(cache.take_residuals_if_present(
        cached_mode,
        &mut selected_cb_residuals,
        &mut selected_cr_residuals,
    ));
    assert_eq!(selected_cb_residuals, cached_cb_residuals);
    assert_eq!(selected_cr_residuals, cached_cr_residuals);

    let unchanged_cb = selected_cb_residuals.clone();
    let unchanged_cr = selected_cr_residuals.clone();
    assert!(!cache.take_residuals_if_present(
        VvcChromaIntraPredictionMode::Derived,
        &mut selected_cb_residuals,
        &mut selected_cr_residuals,
    ));
    assert_eq!(selected_cb_residuals, unchanged_cb);
    assert_eq!(selected_cr_residuals, unchanged_cr);
}

#[test]
fn vvc_luma_prediction_score_matches_materialized_residual_score() {
    let frame = sampled_luma_frame(4, 4, (0..16).map(|idx| (idx * 7) as VvcSample).collect());
    let node = VvcCodingTreeNode::root(4, 4, VvcTreeType::DualTreeLuma);
    let predicted: Vec<_> = (0..16).map(|idx| (idx * 5 + 3) as VvcSample).collect();
    let mut residuals = Vec::new();
    residual_luma_tu_at_into(&mut residuals, &frame, 0, 0, 4, 4, &predicted);

    assert_eq!(
        luma_prediction_residual_score(VvcResidualScoreMetric::Sad, &frame, node, &predicted),
        residual_mode_selection_score(VvcResidualScoreMetric::Sad, &residuals)
    );
    assert_eq!(
        luma_prediction_residual_score(VvcResidualScoreMetric::Sse, &frame, node, &predicted),
        residual_mode_selection_score(VvcResidualScoreMetric::Sse, &residuals)
    );
}

#[test]
fn vvc_luma_exact_inter_candidate_prepares_shared_zero_residual_state() {
    let (source_frame, reference) = shifted_444_frame_and_reference();
    let geometry = source_frame.geometry;
    let format = source_frame.format;
    let frame_recon = VvcReconstructionFrame::new_neutral(geometry, format);
    let mode_search_state = VvcLumaModeSearchState::new_for_geometry(geometry);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy);
    let luma_qp = super::super::VVC_DEFAULT_LOSSY_LUMA_QP;
    let luma_ts_quant = VvcTransformSkipQuantTable::new(format.bit_depth, luma_qp);
    let context = VvcLumaTuSelectionContext {
        policy,
        metric: policy.score_metric(),
        source_frame: &source_frame,
        frame_recon: &frame_recon,
        mode_search_state: &mode_search_state,
        node,
        left: None,
        above: None,
        luma_qp,
        luma_ts_quant: &luma_ts_quant,
        temporal_hint: None,
        inter_decision: None,
        inter_reference: None,
    };
    let decision = VvcLumaInterDecision { mv_x: 8, mv_y: 0 };
    let mut prediction = Vec::new();
    let mut residuals = Vec::new();

    let selected = context
        .select_exact_inter_candidate(decision, &reference, &mut prediction, &mut residuals)
        .expect("exact 4:4:4 inter prediction should be selected");

    assert_eq!(selected.mode, VvcIntraPredictionMode::Dc);
    assert_eq!(selected.inter_decision, Some(decision));
    let expected_prediction: Vec<_> = source_frame
        .luma
        .chunks_exact(geometry.width)
        .flat_map(|row| row[..8].iter().copied())
        .collect();
    assert_eq!(prediction, expected_prediction);
    assert_eq!(residuals, vec![0; 64]);
    let selected_residual = selected.residual.expect("zero residual is preselected");
    assert_eq!(selected_residual.score.distortion, 0);
    assert_eq!(selected_residual.score.rate_cost, 0);
    assert!(selected_residual.residual.block.transform_skip);
    assert_eq!(selected_residual.residual.block.dc_level, 0);
    assert!(!selected_residual.residual.block.has_ac);
    assert!(selected_residual
        .residual
        .block
        .ac_levels
        .iter()
        .all(|level| *level == 0));
}

#[test]
fn vvc_chroma_inter_candidate_prepares_shared_zero_residual_state() {
    let (source_frame, reference) = shifted_444_frame_and_reference();
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeChroma);
    let policy = VvcResidualCodingPolicy::new(source_frame.format, VvcResidualCodingMode::Lossy);
    let context = VvcChromaInterCandidateContext {
        policy,
        source_frame: &source_frame,
        node,
        chroma_x: 0,
        chroma_y: 0,
        chroma_width: 8,
        chroma_height: 8,
    };
    let decision = VvcLumaInterDecision { mv_x: 8, mv_y: 0 };
    let mut cb_prediction = Vec::new();
    let mut cr_prediction = Vec::new();
    let mut cb_residuals = Vec::new();
    let mut cr_residuals = Vec::new();
    #[cfg(feature = "vvc-stats")]
    let mut stats = VvcIntraSearchStats::default();
    #[cfg(not(feature = "vvc-stats"))]
    let mut stats = VvcIntraSearchStats;

    let selected = context
        .select_candidate(
            decision,
            &reference,
            &mut VvcChromaCandidateBuffers {
                prediction: VvcChromaPredictionBuffers {
                    cb: &mut cb_prediction,
                    cr: &mut cr_prediction,
                },
                residuals: VvcChromaResidualBuffers {
                    cb: &mut cb_residuals,
                    cr: &mut cr_residuals,
                },
            },
            &mut stats,
        )
        .expect("valid inter motion should prepare a chroma candidate");

    assert_eq!(selected.mode, VvcChromaIntraPredictionMode::Derived);
    let expected_cb: Vec<_> = source_frame
        .cb
        .chunks_exact(source_frame.geometry.width)
        .flat_map(|row| row[..8].iter().copied())
        .collect();
    let expected_cr: Vec<_> = source_frame
        .cr
        .chunks_exact(source_frame.geometry.width)
        .flat_map(|row| row[..8].iter().copied())
        .collect();
    assert_eq!(cb_prediction, expected_cb);
    assert_eq!(cr_prediction, expected_cr);
    assert_eq!(cb_residuals, vec![0; 64]);
    assert_eq!(cr_residuals, vec![0; 64]);
    let selected_residual = selected.residual.expect("zero residual is preselected");
    assert_eq!(selected_residual.score.distortion, 0);
    assert_eq!(selected_residual.score.rate_cost, 0);
    for block in [selected_residual.residual.cb, selected_residual.residual.cr] {
        assert!(block.transform_skip);
        assert_eq!(block.dc_level, 0);
        assert!(!block.has_ac);
        assert!(block.ac_levels.iter().all(|level| *level == 0));
    }
}

#[test]
fn vvc_ctu_quant_scratch_reuse_is_bit_exact_and_retains_allocations() {
    let geometry = VvcVideoGeometry {
        width: 16,
        height: 16,
    };
    let luma = (0..geometry.luma_samples())
        .map(|index| ((index * 37 + 11) & 0xff) as VvcSample)
        .collect();
    let frame = sampled_luma_frame(geometry.width, geometry.height, luma);
    let policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy);
    let luma_qp = super::super::VVC_DEFAULT_LOSSY_LUMA_QP;
    let chroma_qp = super::super::VVC_DEFAULT_LOSSY_CHROMA_QP;
    let quant_tables = VvcTransformSkipQuantTables::new(frame.format.bit_depth, luma_qp, chroma_qp);
    let region = VvcCtuRegion {
        slice_address: 0,
        origin_x: 0,
        origin_y: 0,
        geometry,
    };
    let mut scratch = VvcCtuQuantScratch::default();
    let encode_once = |scratch: &mut VvcCtuQuantScratch| {
        let mut reconstruction = VvcReconstructionFrame::new_neutral(geometry, frame.format);
        let quantized = quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes_and_scratch_with_mode_hints(
            &frame,
            &mut reconstruction,
            region,
            policy,
            luma_qp,
            chroma_qp,
            &mut VvcLumaModeSearchState::new_for_geometry(geometry),
            &quant_tables,
            scratch,
            None,
            None,
            None,
            None,
            None,
        );
        (quantized, reconstruction)
    };
    let capacities = |scratch: &VvcCtuQuantScratch| {
        [
            scratch.luma_nodes.capacity(),
            scratch.chroma_nodes.capacity(),
            scratch.predicted_luma.capacity(),
            scratch.predicted_cb.capacity(),
            scratch.predicted_cr.capacity(),
            scratch.reconstructed_residual.capacity(),
            scratch.luma_residuals.capacity(),
            scratch.candidate_luma_prediction.capacity(),
            scratch.candidate_luma_residuals.capacity(),
            scratch.cb_residuals.capacity(),
            scratch.cr_residuals.capacity(),
            scratch.candidate_cb_prediction.capacity(),
            scratch.candidate_cr_prediction.capacity(),
            scratch.candidate_cb_residuals.capacity(),
            scratch.candidate_cr_residuals.capacity(),
        ]
    };

    let (mut first_quantized, first_reconstruction) = encode_once(&mut scratch);
    let first_capacities = capacities(&scratch);
    assert!(first_capacities.iter().any(|capacity| *capacity > 0));
    let (mut second_quantized, second_reconstruction) = encode_once(&mut scratch);
    #[cfg(feature = "vvc-stats")]
    {
        first_quantized.intra_search_stats = VvcIntraSearchStats::default();
        second_quantized.intra_search_stats = VvcIntraSearchStats::default();
    }

    assert_eq!(second_quantized, first_quantized);
    assert_eq!(second_reconstruction, first_reconstruction);
    assert_eq!(capacities(&scratch), first_capacities);
}

#[test]
fn vvc_ctu_quantization_result_uses_shared_metadata_and_chroma_finalization() {
    let mut frame = sampled_luma_frame(8, 8, vec![200; 64]);
    frame.cb.fill(201);
    frame.cr.fill(33);
    let mut luma_metadata = VvcLumaTuMetadata::new();
    luma_metadata.record_finalized(
        0,
        VvcIntraPredictionMode::Horizontal,
        VvcFinalizedLumaTu {
            abs_remainder: 0,
            negative: false,
            dc_level: 0,
            ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
            has_ac: false,
            transform_skip: false,
            bdpcm_mode: VvcBdpcmMode::None,
            mrl_index: 0,
            mts_index: 0,
        },
    );
    let mut chroma_metadata = VvcChromaTuMetadata::new();
    chroma_metadata.record_finalized(
        0,
        VvcChromaIntraPredictionMode::Derived,
        VvcFinalizedChromaTu {
            cb_dc_level: 0,
            cr_dc_level: 0,
            cb_ac_levels: [0; VVC_CHROMA_AC_COEFFS_PER_TU],
            cr_ac_levels: [0; VVC_CHROMA_AC_COEFFS_PER_TU],
            cb_has_ac: false,
            cr_has_ac: false,
            cb_transform_skip: true,
            cr_transform_skip: false,
            bdpcm_mode: VvcBdpcmMode::None,
        },
    );
    let result = VvcCtuQuantizationResult {
        luma_metadata,
        chroma_metadata,
        luma_tu_count: 1,
        chroma_tu_count: 1,
        #[cfg(feature = "vvc-stats")]
        intra_search_stats: VvcIntraSearchStats::default(),
        #[cfg(feature = "vvc-stats")]
        residual_energy_stats: VvcResidualEnergyStats::default(),
    }
    .into_quantized_color(&frame);

    assert_eq!(result.y, 200);
    assert_eq!(result.u, 201);
    assert_eq!(
        result.v,
        reconstruct_vvc_chroma(quantize_vvc_chroma_sample(33))
    );
    assert_eq!(result.luma_tu_count, 1);
    assert_eq!(result.chroma_tu_count, 1);
    assert_eq!(
        result.luma_tu_intra_modes[0],
        VvcIntraPredictionMode::Horizontal
    );
    assert!(result.cb_tu_transform_skip[0]);
    assert!(!result.cr_tu_transform_skip[0]);
}

#[test]
fn vvc_source_plane_copy_clips_destination_and_extends_source_edges() {
    let source_geometry = VvcVideoGeometry {
        width: 3,
        height: 2,
    };
    let destination_geometry = VvcVideoGeometry {
        width: 5,
        height: 3,
    };
    let source = vec![1, 2, 3, 4, 5, 6];
    let mut destination = vec![99; destination_geometry.luma_samples()];
    assert!(copy_vvc_source_plane_region_with_edge_extension(
        &mut destination,
        destination_geometry,
        &source,
        source_geometry,
        VvcPlaneRegion {
            origin_x: 1,
            origin_y: 0,
            geometry: VvcVideoGeometry {
                width: 6,
                height: 4,
            },
        },
    ));
    assert_eq!(
        destination,
        vec![99, 2, 3, 3, 3, 99, 5, 6, 6, 6, 99, 5, 6, 6, 6]
    );
}

#[test]
fn vvc_source_plane_copy_rejects_inconsistent_plane_lengths() {
    let geometry = VvcVideoGeometry {
        width: 2,
        height: 2,
    };
    let region = VvcPlaneRegion {
        origin_x: 0,
        origin_y: 0,
        geometry,
    };
    assert!(!copy_vvc_source_plane_region_with_edge_extension(
        &mut [0; 3],
        geometry,
        &[1; 4],
        geometry,
        region,
    ));
    assert!(!copy_vvc_source_plane_region_with_edge_extension(
        &mut [0; 4],
        geometry,
        &[1; 3],
        geometry,
        region,
    ));
}

#[test]
fn vvc_chroma_plane_reconstruction_shares_exact_and_transform_skip_paths() {
    let source_geometry = VvcVideoGeometry {
        width: 4,
        height: 4,
    };
    let coded_geometry = VvcVideoGeometry {
        width: 8,
        height: 8,
    };
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let chroma_qp = 0;
    let chroma_ts_quant = VvcTransformSkipQuantTable::new(format.bit_depth, chroma_qp);
    let context = VvcChromaPlaneReconstructionContext {
        source_geometry,
        coded_geometry,
        format,
        node: VvcCodingTreeNode::root(8, 8, VvcTreeType::SingleTree),
        chroma_width: 4,
        chroma_height: 4,
        chroma_qp,
        chroma_ts_quant: &chroma_ts_quant,
        exact_transform_skip_qp: true,
    };
    let transform_skip = VvcFinalizedResidualBlock {
        dc_level: 1,
        ac_levels: [0; VVC_CHROMA_AC_COEFFS_PER_TU],
        has_ac: false,
        transform_skip: true,
        bdpcm_mode: VvcBdpcmMode::None,
    };
    #[cfg(feature = "vvc-stats")]
    let mut stats = VvcIntraSearchStats::default();
    let mut transform_scratch = VvcInverseTransformScratch::default();
    let mut reconstructed_residual = Vec::new();
    let mut scratch = VvcChromaPlaneReconstructionScratch {
        #[cfg(feature = "vvc-stats")]
        stats: &mut stats,
        transform_scratch: &mut transform_scratch,
        reconstructed_residual: &mut reconstructed_residual,
    };

    let mut exact_destination = vec![99; 16];
    context.reconstruct_plane(
        &mut exact_destination,
        &[1, 2, 3, 4],
        &[10; 16],
        transform_skip,
        &mut scratch,
    );
    assert_eq!(
        exact_destination,
        vec![1, 2, 2, 2, 3, 4, 4, 4, 3, 4, 4, 4, 3, 4, 4, 4]
    );

    let mut transform_skip_destination = vec![99; 16];
    VvcChromaPlaneReconstructionContext {
        exact_transform_skip_qp: false,
        ..context
    }
    .reconstruct_plane(
        &mut transform_skip_destination,
        &[5; 4],
        &[10; 16],
        transform_skip,
        &mut scratch,
    );
    let reconstructed_dc = (10 + i32::from(chroma_ts_quant.reconstructed(1))) as VvcSample;
    let mut expected = vec![10; 16];
    expected[0] = reconstructed_dc;
    assert_eq!(transform_skip_destination, expected);
}

#[test]
fn vvc_luma_temporal_hint_candidate_preserves_cheap_residual_gate() {
    let exact_frame = sampled_luma_frame(8, 8, vec![128; 64]);
    let expensive_frame = sampled_luma_frame(8, 8, vec![200; 64]);
    let frame_recon = VvcReconstructionFrame::new_neutral(exact_frame.geometry, exact_frame.format);
    let mode_search_state = VvcLumaModeSearchState::new_for_geometry(exact_frame.geometry);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    let policy = VvcResidualCodingPolicy::new(exact_frame.format, VvcResidualCodingMode::Lossless)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let luma_qp = 0;
    let luma_ts_quant = VvcTransformSkipQuantTable::new(exact_frame.format.bit_depth, luma_qp);
    let hint = VvcLumaTemporalModeHint {
        mode: VvcIntraPredictionMode::Dc,
        bdpcm_mode: VvcBdpcmMode::None,
    };
    let mut prediction_scratch = VvcDcPredictionScratch::default();
    let mut prediction = Vec::new();
    let mut residuals = Vec::new();
    let mut stats = test_intra_search_stats();
    let exact_context = VvcLumaTuSelectionContext {
        policy,
        metric: policy.score_metric(),
        source_frame: &exact_frame,
        frame_recon: &frame_recon,
        mode_search_state: &mode_search_state,
        node,
        left: None,
        above: None,
        luma_qp,
        luma_ts_quant: &luma_ts_quant,
        temporal_hint: Some(hint),
        inter_decision: None,
        inter_reference: None,
    };

    let selected = exact_context
        .select_temporal_hint_candidate(
            hint,
            &mut prediction_scratch,
            &mut prediction,
            &mut residuals,
            &mut stats,
        )
        .expect("zero-residual luma temporal hint should be accepted");
    assert_eq!(selected.mode, hint.mode);
    assert!(selected.residual.is_none());
    assert_eq!(residuals, vec![0; 64]);

    let expensive_context = VvcLumaTuSelectionContext {
        source_frame: &expensive_frame,
        ..exact_context
    };
    assert!(expensive_context
        .select_temporal_hint_candidate(
            hint,
            &mut prediction_scratch,
            &mut prediction,
            &mut residuals,
            &mut stats,
        )
        .is_none());
    assert!(residuals
        .iter()
        .all(|residual| residual.unsigned_abs() > 16));
}

#[test]
fn vvc_chroma_temporal_hint_candidate_preserves_cheap_residual_gate() {
    let exact_frame = sampled_luma_frame(8, 8, vec![128; 64]);
    let mut expensive_frame = sampled_luma_frame(8, 8, vec![128; 64]);
    expensive_frame.cb.fill(200);
    expensive_frame.cr.fill(200);
    let frame_recon = VvcReconstructionFrame::new_neutral(exact_frame.geometry, exact_frame.format);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeChroma);
    let policy = VvcResidualCodingPolicy::new(exact_frame.format, VvcResidualCodingMode::Lossless)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let chroma_qp = 0;
    let chroma_ts_quant = VvcTransformSkipQuantTable::new(exact_frame.format.bit_depth, chroma_qp);
    let hint = VvcChromaTemporalModeHint {
        mode: VvcChromaIntraPredictionMode::Derived,
        bdpcm_mode: VvcBdpcmMode::None,
    };
    let mut prediction_scratch = VvcDcPredictionScratch::default();
    let mut predicted_cb = Vec::new();
    let mut predicted_cr = Vec::new();
    let mut cb_residuals = Vec::new();
    let mut cr_residuals = Vec::new();
    let mut stats = test_intra_search_stats();
    let exact_context = VvcChromaTuSelectionContext {
        policy,
        metric: policy.score_metric(),
        source_frame: &exact_frame,
        frame_recon: &frame_recon,
        node,
        co_located_luma_mode: VvcIntraPredictionMode::Dc,
        chroma_x: 0,
        chroma_y: 0,
        chroma_width: 4,
        chroma_height: 4,
        cclm_enabled: false,
        syntax_tie_breaker_enabled: false,
        chroma_qp,
        chroma_ts_quant: &chroma_ts_quant,
        temporal_hint: Some(hint),
    };

    let selected = {
        let mut buffers = VvcChromaCandidateBuffers {
            prediction: VvcChromaPredictionBuffers {
                cb: &mut predicted_cb,
                cr: &mut predicted_cr,
            },
            residuals: VvcChromaResidualBuffers {
                cb: &mut cb_residuals,
                cr: &mut cr_residuals,
            },
        };
        exact_context.select_temporal_hint_candidate(
            hint,
            &mut prediction_scratch,
            &mut buffers,
            &mut stats,
        )
    }
    .expect("zero-residual chroma temporal hint should be accepted");
    assert_eq!(selected.mode, hint.mode);
    assert!(selected.residual.is_none());
    assert_eq!(cb_residuals, vec![0; 16]);
    assert_eq!(cr_residuals, vec![0; 16]);

    let expensive_context = VvcChromaTuSelectionContext {
        source_frame: &expensive_frame,
        ..exact_context
    };
    let expensive_candidate = {
        let mut buffers = VvcChromaCandidateBuffers {
            prediction: VvcChromaPredictionBuffers {
                cb: &mut predicted_cb,
                cr: &mut predicted_cr,
            },
            residuals: VvcChromaResidualBuffers {
                cb: &mut cb_residuals,
                cr: &mut cr_residuals,
            },
        };
        expensive_context.select_temporal_hint_candidate(
            hint,
            &mut prediction_scratch,
            &mut buffers,
            &mut stats,
        )
    };
    assert!(expensive_candidate.is_none());
    assert!(cb_residuals
        .iter()
        .chain(&cr_residuals)
        .all(|residual| residual.unsigned_abs() > 16));
}

#[test]
fn vvc_chroma_prediction_score_matches_materialized_residual_score() {
    let mut frame = sampled_luma_frame(4, 4, vec![0; 16]);
    frame.cb = vec![100, 112, 124, 136];
    frame.cr = vec![130, 118, 106, 94];
    let predicted_cb = vec![96, 116, 120, 140];
    let predicted_cr = vec![128, 120, 108, 96];
    let mut cb_residuals = Vec::new();
    let mut cr_residuals = Vec::new();
    residual_chroma_pair_tu_at_into(
        &mut cb_residuals,
        &mut cr_residuals,
        &frame.cb,
        &frame.cr,
        frame.geometry,
        frame.format,
        0,
        0,
        2,
        2,
        &predicted_cb,
        &predicted_cr,
    );

    let direct = chroma_prediction_residual_score(
        VvcResidualScoreMetric::Sse,
        &frame,
        0,
        0,
        2,
        2,
        &predicted_cb,
        &predicted_cr,
    );
    let materialized =
        residual_mode_selection_score(VvcResidualScoreMetric::Sse, &cb_residuals).saturating_add(
            residual_mode_selection_score(VvcResidualScoreMetric::Sse, &cr_residuals),
        );
    assert_eq!(direct, materialized);

    let mut detected_cb = Vec::new();
    let mut detected_cr = Vec::new();
    let nonzero_flags = residual_chroma_pair_tu_at_into_and_detect_zero(
        &mut detected_cb,
        &mut detected_cr,
        &frame.cb,
        &frame.cr,
        frame.geometry,
        frame.format,
        0,
        0,
        2,
        2,
        &predicted_cb,
        &predicted_cr,
    );
    assert_eq!(detected_cb, cb_residuals);
    assert_eq!(detected_cr, cr_residuals);
    assert_eq!(nonzero_flags, (false, false));
}

#[test]
fn vvc_chroma_intra_search_promotes_only_strict_winners_and_records_ties() {
    let explicit = VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal);
    let cclm = VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear);
    let mut search = VvcChromaIntraSearch::new(100);
    let mut selected_cb: Vec<VvcSample> = vec![1];
    let mut selected_cr: Vec<VvcSample> = vec![10];
    let mut candidate_cb: Vec<VvcSample> = vec![2];
    let mut candidate_cr: Vec<VvcSample> = vec![20];
    let mut selected_prediction = VvcChromaPredictionBuffers {
        cb: &mut selected_cb,
        cr: &mut selected_cr,
    };
    let mut candidate_prediction = VvcChromaPredictionBuffers {
        cb: &mut candidate_cb,
        cr: &mut candidate_cr,
    };

    search.consider_candidate(
        explicit,
        80,
        &mut selected_prediction,
        &mut candidate_prediction,
    );
    assert_eq!(search.best_mode(), explicit);
    assert_eq!(search.best_score(), 80);
    assert_eq!(*selected_prediction.cb, vec![2]);
    assert_eq!(*selected_prediction.cr, vec![20]);
    assert_eq!(*candidate_prediction.cb, vec![1]);
    assert_eq!(*candidate_prediction.cr, vec![10]);

    candidate_prediction.cb[0] = 3;
    candidate_prediction.cr[0] = 30;
    search.consider_candidate(
        cclm,
        80,
        &mut selected_prediction,
        &mut candidate_prediction,
    );
    assert_eq!(search.best_mode(), explicit);
    assert_eq!(*selected_prediction.cb, vec![2]);
    assert_eq!(*selected_prediction.cr, vec![20]);
    assert_eq!(*candidate_prediction.cb, vec![3]);
    assert_eq!(*candidate_prediction.cr, vec![30]);

    let costs: Vec<_> = search
        .candidate_costs()
        .iter()
        .map(|candidate| (candidate.mode(), candidate.score()))
        .collect();
    assert_eq!(
        costs,
        vec![
            (VvcChromaIntraPredictionMode::Derived, 100),
            (explicit, 80),
            (cclm, 80),
        ]
    );
}

#[test]
fn vvc_chroma_candidate_evaluator_shares_derived_explicit_and_cclm_scoring() {
    let mut frame = sampled_luma_frame(
        8,
        8,
        (0..8)
            .flat_map(|y| (0..8).map(move |x| ((x * 13 + y * 5) & 0xff) as VvcSample))
            .collect(),
    );
    frame.cb = (0..16).map(|index| (index * 9) as VvcSample).collect();
    frame.cr = (0..16)
        .map(|index| (255 - index * 7) as VvcSample)
        .collect();
    let frame_recon = VvcReconstructionFrame::new_neutral(frame.geometry, frame.format);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeChroma);
    let policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy);
    let context = VvcChromaModeSearchContext {
        policy,
        metric: policy.score_metric(),
        source_frame: &frame,
        frame_recon: &frame_recon,
        node,
        co_located_luma_mode: VvcIntraPredictionMode::Horizontal,
        chroma_x: 0,
        chroma_y: 0,
        chroma_width: 4,
        chroma_height: 4,
        cclm_enabled: true,
        syntax_tie_breaker_enabled: policy.chroma_syntax_tie_breaker(),
    };

    for mode in [
        VvcChromaIntraPredictionMode::Derived,
        VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Planar),
        VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
    ] {
        let mut cache = VvcChromaModeRdCache::new();
        cache.reset(policy, node);
        let mut prediction_scratch = VvcDcPredictionScratch::default();
        let mut predicted_cb = Vec::new();
        let mut predicted_cr = Vec::new();
        let mut cb_residuals = Vec::new();
        let mut cr_residuals = Vec::new();
        #[cfg(feature = "vvc-stats")]
        let mut stats = VvcIntraSearchStats::default();
        #[cfg(not(feature = "vvc-stats"))]
        let mut stats = VvcIntraSearchStats;

        let score = {
            let mut prediction = VvcChromaPredictionBuffers {
                cb: &mut predicted_cb,
                cr: &mut predicted_cr,
            };
            let mut residuals = VvcChromaResidualBuffers {
                cb: &mut cb_residuals,
                cr: &mut cr_residuals,
            };
            context.predict_and_score_candidate(
                &mut cache,
                mode,
                &mut prediction_scratch,
                &mut prediction,
                &mut residuals,
                &mut stats,
            )
        };
        assert_eq!(
            score,
            chroma_prediction_mode_selection_score(
                policy.score_metric(),
                &frame,
                0,
                0,
                4,
                4,
                &predicted_cb,
                &predicted_cr,
                mode,
                true,
                policy.chroma_syntax_tie_breaker(),
            ),
            "mode={mode:?}",
        );
        assert_eq!(predicted_cb.len(), 16, "mode={mode:?}");
        assert_eq!(predicted_cr.len(), 16, "mode={mode:?}");
        assert_eq!(cb_residuals.len(), 16, "mode={mode:?}");
        assert_eq!(cr_residuals.len(), 16, "mode={mode:?}");
    }
}

#[test]
fn vvc_chroma_search_preserves_unscored_derived_only_fast_path() {
    let mut frame = sampled_luma_frame(8, 8, vec![32; 64]);
    frame.cb = vec![0; 16];
    frame.cr = vec![255; 16];
    let frame_recon = VvcReconstructionFrame::new_neutral(frame.geometry, frame.format);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeChroma);
    let policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossless)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let context = VvcChromaModeSearchContext {
        policy,
        metric: policy.score_metric(),
        source_frame: &frame,
        frame_recon: &frame_recon,
        node,
        co_located_luma_mode: VvcIntraPredictionMode::Horizontal,
        chroma_x: 0,
        chroma_y: 0,
        chroma_width: 4,
        chroma_height: 4,
        cclm_enabled: true,
        syntax_tie_breaker_enabled: policy.chroma_syntax_tie_breaker(),
    };
    let mut cache = VvcChromaModeRdCache::new();
    cache.reset(policy, node);
    let mut prediction_scratch = VvcDcPredictionScratch::default();
    let mut selected_cb = Vec::new();
    let mut selected_cr = Vec::new();
    let mut candidate_cb = Vec::new();
    let mut candidate_cr = Vec::new();
    let mut candidate_cb_residuals = Vec::new();
    let mut candidate_cr_residuals = Vec::new();
    #[cfg(feature = "vvc-stats")]
    let mut stats = VvcIntraSearchStats::default();
    #[cfg(not(feature = "vvc-stats"))]
    let mut stats = VvcIntraSearchStats;

    let result = context.select_intra_mode(VvcChromaModeSearchBuffers {
        cache: &mut cache,
        prediction_scratch: &mut prediction_scratch,
        selected_prediction: VvcChromaPredictionBuffers {
            cb: &mut selected_cb,
            cr: &mut selected_cr,
        },
        candidate_prediction: VvcChromaPredictionBuffers {
            cb: &mut candidate_cb,
            cr: &mut candidate_cr,
        },
        candidate_residuals: VvcChromaResidualBuffers {
            cb: &mut candidate_cb_residuals,
            cr: &mut candidate_cr_residuals,
        },
        stats: &mut stats,
    });

    assert_eq!(result.mode, VvcChromaIntraPredictionMode::Derived);
    assert_eq!(
        result
            .candidate_costs
            .iter()
            .map(|candidate| (candidate.mode(), candidate.score()))
            .collect::<Vec<_>>(),
        vec![(VvcChromaIntraPredictionMode::Derived, 0)],
    );
    assert_eq!(selected_cb.len(), 16);
    assert_eq!(selected_cr.len(), 16);
    assert!(candidate_cb.is_empty());
    assert!(candidate_cr.is_empty());
    assert!(candidate_cb_residuals.is_empty());
    assert!(candidate_cr_residuals.is_empty());
}

#[test]
fn vvc_chroma_refinement_promotes_prediction_and_residual_pair_as_one_unit() {
    let mut selected_cb_prediction = vec![1];
    let mut selected_cr_prediction = vec![2];
    let mut selected_cb_residuals = vec![3];
    let mut selected_cr_residuals = vec![4];
    let mut candidate_cb_prediction = vec![11];
    let mut candidate_cr_prediction = vec![12];
    let mut candidate_cb_residuals = vec![13];
    let mut candidate_cr_residuals = vec![14];
    let mut prediction_scratch = VvcDcPredictionScratch::default();
    #[cfg(feature = "vvc-stats")]
    let mut stats = VvcIntraSearchStats::default();
    #[cfg(not(feature = "vvc-stats"))]
    let mut stats = VvcIntraSearchStats;
    let mut transform_scratch = VvcInverseTransformScratch::default();
    let mut reconstructed_residual = Vec::new();

    {
        let mut buffers = VvcChromaRefinementBuffers {
            prediction_scratch: &mut prediction_scratch,
            selected: VvcChromaCandidateBuffers {
                prediction: VvcChromaPredictionBuffers {
                    cb: &mut selected_cb_prediction,
                    cr: &mut selected_cr_prediction,
                },
                residuals: VvcChromaResidualBuffers {
                    cb: &mut selected_cb_residuals,
                    cr: &mut selected_cr_residuals,
                },
            },
            candidate: VvcChromaCandidateBuffers {
                prediction: VvcChromaPredictionBuffers {
                    cb: &mut candidate_cb_prediction,
                    cr: &mut candidate_cr_prediction,
                },
                residuals: VvcChromaResidualBuffers {
                    cb: &mut candidate_cb_residuals,
                    cr: &mut candidate_cr_residuals,
                },
            },
            stats: &mut stats,
            transform_scratch: &mut transform_scratch,
            reconstructed_residual: &mut reconstructed_residual,
        };

        buffers.promote_candidate();
    }

    assert_eq!(selected_cb_prediction, vec![11]);
    assert_eq!(selected_cr_prediction, vec![12]);
    assert_eq!(selected_cb_residuals, vec![13]);
    assert_eq!(selected_cr_residuals, vec![14]);
    assert_eq!(candidate_cb_prediction, vec![1]);
    assert_eq!(candidate_cr_prediction, vec![2]);
    assert_eq!(candidate_cb_residuals, vec![3]);
    assert_eq!(candidate_cr_residuals, vec![4]);
}

#[test]
fn vvc_luma_intra_search_promotes_only_strict_winners_and_records_ties() {
    let directional = VvcIntraPredictionMode::Horizontal;
    let mut search = VvcLumaIntraSearch::new(100);
    let mut selected: Vec<VvcSample> = vec![1];
    let mut candidate: Vec<VvcSample> = vec![2];

    search.consider_candidate(
        VvcIntraPredictionMode::Planar,
        80,
        &mut selected,
        &mut candidate,
    );
    assert_eq!(search.best_mode(), VvcIntraPredictionMode::Planar);
    assert_eq!(search.best_score(), 80);
    assert_eq!(selected, vec![2]);
    assert_eq!(candidate, vec![1]);

    candidate[0] = 3;
    search.consider_candidate(directional, 80, &mut selected, &mut candidate);
    assert_eq!(search.best_mode(), VvcIntraPredictionMode::Planar);
    assert_eq!(selected, vec![2]);
    assert_eq!(candidate, vec![3]);

    let costs: Vec<_> = search
        .candidate_costs()
        .iter()
        .map(|candidate| (candidate.mode(), candidate.score()))
        .collect();
    assert_eq!(
        costs,
        vec![
            (VvcIntraPredictionMode::Dc, 100),
            (VvcIntraPredictionMode::Planar, 80),
            (directional, 80),
        ]
    );
}

#[test]
fn vvc_luma_candidate_evaluator_shares_dc_planar_and_directional_scoring() {
    let frame = sampled_luma_frame(
        8,
        8,
        (0..8)
            .flat_map(|y| (0..8).map(move |x| ((x * 11 + y * 7) & 0xff) as VvcSample))
            .collect(),
    );
    let frame_recon = VvcReconstructionFrame::new_neutral(frame.geometry, frame.format);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    let policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy);
    let mode_search_state = VvcLumaModeSearchState::new_for_geometry(frame.geometry);
    let context = VvcLumaModeSearchContext {
        policy,
        metric: policy.score_metric(),
        source_frame: &frame,
        frame_recon: &frame_recon,
        mode_search_state: &mode_search_state,
        node,
        left: None,
        above: None,
    };

    for mode in [
        VvcIntraPredictionMode::Dc,
        VvcIntraPredictionMode::Planar,
        VvcIntraPredictionMode::Angular(42),
    ] {
        let mut cache = VvcLumaModeRdCache::new();
        cache.reset(policy, node);
        let mut prediction_scratch = VvcDcPredictionScratch::default();
        let mut predicted = Vec::new();
        let mut residuals = Vec::new();
        #[cfg(feature = "vvc-stats")]
        let mut stats = VvcIntraSearchStats::default();
        #[cfg(not(feature = "vvc-stats"))]
        let mut stats = VvcIntraSearchStats;

        let score = context.predict_and_score_candidate(
            &mut cache,
            mode,
            &mut prediction_scratch,
            &mut predicted,
            &mut residuals,
            &mut stats,
        );
        assert_eq!(
            score,
            luma_prediction_mode_selection_score(
                policy.score_metric(),
                &frame,
                node,
                &predicted,
                None,
                None,
                mode,
            ),
            "mode={mode:?}",
        );
        assert_eq!(predicted.len(), 64, "mode={mode:?}");
        assert_eq!(residuals.len(), 64, "mode={mode:?}");
    }
}

#[test]
fn vvc_luma_tu_metadata_records_scc_and_finalized_without_cross_talk() {
    let mut metadata = VvcLumaTuMetadata::new();
    let scc_decision = VvcLumaSccDecision::IbcExact(VvcLumaIbcDecision {
        mvd_x: -8,
        mvd_y: 4,
        pred_mode_ibc_ctx: 2,
    });
    metadata.record_scc_decision(0, scc_decision);

    let mut ac_levels = [0; VVC_LUMA_AC_COEFFS_PER_TU];
    ac_levels[0] = -7;
    ac_levels[VVC_LUMA_AC_COEFFS_PER_TU - 1] = 11;
    let finalized = VvcFinalizedLumaTu {
        abs_remainder: 4,
        negative: true,
        dc_level: -4,
        ac_levels,
        has_ac: true,
        transform_skip: true,
        bdpcm_mode: VvcBdpcmMode::Horizontal,
        mrl_index: 2,
        mts_index: 3,
    };
    metadata.record_finalized(2, VvcIntraPredictionMode::Planar, finalized);

    assert_eq!(metadata.scc_decision(0), Some(scc_decision));
    assert_eq!(
        metadata.luma_tu_scc_decisions[1],
        VvcLumaSccDecision::RegularIntra
    );
    assert_eq!(metadata.luma_tu_intra_modes[1], VvcIntraPredictionMode::Dc);
    assert_eq!(metadata.luma_tu_bdpcm_modes[1], VvcBdpcmMode::None);
    assert_eq!(metadata.luma_tu_remainders[1], 0);
    assert_eq!(metadata.luma_tu_dc_levels[1], 0);
    assert!(!metadata.luma_tu_has_ac[1]);

    assert_eq!(
        metadata.luma_tu_intra_modes[2],
        VvcIntraPredictionMode::Planar
    );
    assert_eq!(metadata.luma_tu_remainders[2], finalized.abs_remainder);
    assert_eq!(metadata.luma_tu_negative[2], finalized.negative);
    assert_eq!(metadata.luma_tu_dc_levels[2], finalized.dc_level);
    assert_eq!(metadata.luma_tu_ac_levels[2], finalized.ac_levels);
    assert_eq!(metadata.luma_tu_has_ac[2], finalized.has_ac);
    assert_eq!(metadata.luma_tu_transform_skip[2], finalized.transform_skip);
    assert_eq!(metadata.luma_tu_bdpcm_modes[2], finalized.bdpcm_mode);
    assert_eq!(metadata.luma_tu_mrl_index[2], finalized.mrl_index);
    assert_eq!(metadata.luma_tu_mts_index[2], finalized.mts_index);

    assert_eq!(metadata.luma_tu_intra_modes[3], VvcIntraPredictionMode::Dc);
    assert_eq!(metadata.luma_tu_remainders[3], 0);
    assert_eq!(metadata.luma_tu_bdpcm_modes[3], VvcBdpcmMode::None);
}

#[test]
fn vvc_chroma_tu_metadata_records_finalized_without_cross_talk() {
    let mut metadata = VvcChromaTuMetadata::new();

    let mut cb_ac_levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
    let mut cr_ac_levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
    cb_ac_levels[0] = -5;
    cr_ac_levels[VVC_CHROMA_AC_COEFFS_PER_TU - 1] = 9;
    let finalized = VvcFinalizedChromaTu {
        cb_dc_level: -3,
        cr_dc_level: 4,
        cb_ac_levels,
        cr_ac_levels,
        cb_has_ac: true,
        cr_has_ac: true,
        cb_transform_skip: true,
        cr_transform_skip: false,
        bdpcm_mode: VvcBdpcmMode::Horizontal,
    };
    let finalized_mode = VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmTop);
    metadata.record_finalized(2, finalized_mode, finalized);

    assert_eq!(
        metadata.chroma_tu_intra_modes[0],
        VvcChromaIntraPredictionMode::Derived
    );
    assert_eq!(metadata.cb_tu_dc_levels[0], 0);
    assert_eq!(metadata.chroma_tu_bdpcm_modes[0], VvcBdpcmMode::None);

    assert_eq!(
        metadata.chroma_tu_intra_modes[1],
        VvcChromaIntraPredictionMode::Derived
    );
    assert_eq!(metadata.chroma_tu_bdpcm_modes[1], VvcBdpcmMode::None);
    assert_eq!(metadata.cb_tu_dc_levels[1], 0);
    assert_eq!(metadata.cr_tu_dc_levels[1], 0);
    assert!(!metadata.cb_tu_has_ac[1]);
    assert!(!metadata.cr_tu_has_ac[1]);

    assert_eq!(metadata.chroma_tu_intra_modes[2], finalized_mode);
    assert_eq!(metadata.cb_tu_dc_levels[2], finalized.cb_dc_level);
    assert_eq!(metadata.cr_tu_dc_levels[2], finalized.cr_dc_level);
    assert_eq!(metadata.cb_tu_ac_levels[2], finalized.cb_ac_levels);
    assert_eq!(metadata.cr_tu_ac_levels[2], finalized.cr_ac_levels);
    assert_eq!(metadata.cb_tu_has_ac[2], finalized.cb_has_ac);
    assert_eq!(metadata.cr_tu_has_ac[2], finalized.cr_has_ac);
    assert_eq!(
        metadata.cb_tu_transform_skip[2],
        finalized.cb_transform_skip
    );
    assert_eq!(
        metadata.cr_tu_transform_skip[2],
        finalized.cr_transform_skip
    );
    assert_eq!(metadata.chroma_tu_bdpcm_modes[2], finalized.bdpcm_mode);

    assert_eq!(
        metadata.chroma_tu_intra_modes[3],
        VvcChromaIntraPredictionMode::Derived
    );
    assert_eq!(metadata.cb_tu_dc_levels[3], 0);
    assert_eq!(metadata.cr_tu_dc_levels[3], 0);
}

#[test]
fn vvc_source_luma_directional_seed_maps_integer_gradients() {
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    let flat = sampled_luma_frame(8, 8, vec![64; 64]);
    assert_eq!(vvc_source_luma_directional_seed(&flat, node), None);

    let horizontal_ramp = sampled_luma_frame(
        8,
        8,
        (0..8)
            .flat_map(|_| (0..8).map(|x| (x * 16) as VvcSample))
            .collect(),
    );
    assert_eq!(
        vvc_source_luma_directional_seed(&horizontal_ramp, node),
        Some(50)
    );

    let vertical_ramp = sampled_luma_frame(
        8,
        8,
        (0..8)
            .flat_map(|y| (0..8).map(move |_| (y * 16) as VvcSample))
            .collect(),
    );
    assert_eq!(
        vvc_source_luma_directional_seed(&vertical_ramp, node),
        Some(18)
    );

    let diagonal_ramp = sampled_luma_frame(
        8,
        8,
        (0..8)
            .flat_map(|y| (0..8).map(move |x| ((x + y) * 8) as VvcSample))
            .collect(),
    );
    assert_eq!(
        vvc_source_luma_directional_seed(&diagonal_ramp, node),
        Some(34)
    );
}

#[test]
fn vvc_lossy_luma_directional_search_uses_focused_gradient_family() {
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    let frame = sampled_luma_frame(
        8,
        8,
        (0..8)
            .flat_map(|_| (0..8).map(|x| (x * 16) as VvcSample))
            .collect(),
    );
    let state = VvcLumaModeSearchState::new_for_geometry(frame.geometry);
    let lossy_policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy);
    let lossless_policy =
        VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossless);

    let lossy_candidates =
        vvc_luma_directional_search_candidates(lossy_policy, &frame, &state, node);
    let lossy_indexes: Vec<_> = lossy_candidates
        .iter()
        .map(|mode| mode.luma_mode_index())
        .collect();
    let lossless_candidates =
        vvc_luma_directional_search_candidates(lossless_policy, &frame, &state, node);
    let lossless_indexes: Vec<_> = lossless_candidates
        .iter()
        .map(|mode| mode.luma_mode_index())
        .collect();
    assert!(lossy_candidates.count() > VVC_LUMA_NEARBY_DIRECTIONAL_OFFSETS.len());
    assert!(lossy_candidates.count() < lossless_candidates.count());
    for index in [2, 18, 34, 46, 48, 49, 50, 51, 52, 54, 66] {
        assert!(lossy_indexes.contains(&index));
    }
    for index in [18, 34, 50] {
        assert!(lossless_indexes.contains(&index));
    }
}

#[test]
fn vvc_lossy_luma_directional_search_skips_source_seed_for_4x4() {
    let node = VvcCodingTreeNode::root(4, 4, VvcTreeType::DualTreeLuma);
    let frame = sampled_luma_frame(
        4,
        4,
        (0..4)
            .flat_map(|_| (0..4).map(|x| (x * 16) as VvcSample))
            .collect(),
    );
    let state = VvcLumaModeSearchState::new_for_geometry(frame.geometry);
    let lossy_policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy);
    let lossless_policy =
        VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossless);

    let lossy_candidates =
        vvc_luma_directional_search_candidates(lossy_policy, &frame, &state, node);
    let lossy_indexes: Vec<_> = lossy_candidates
        .iter()
        .map(|mode| mode.luma_mode_index())
        .collect();
    assert_eq!(lossy_indexes, vec![18, 50, 34, 2, 66]);

    let lossless_candidates =
        vvc_luma_directional_search_candidates(lossless_policy, &frame, &state, node);
    let lossless_indexes: Vec<_> = lossless_candidates
        .iter()
        .map(|mode| mode.luma_mode_index())
        .collect();
    assert!(lossless_indexes.contains(&48));
    assert!(lossless_indexes.contains(&52));
}

#[test]
fn vvc_lossy_luma_directional_search_keeps_exact_neighbor_modes() {
    let mut target = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    target.x = 8;
    let frame = sampled_luma_frame(16, 8, vec![64; 128]);
    let mut state = VvcLumaModeSearchState::new_for_geometry(frame.geometry);
    state.mark_node(
        VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma),
        VvcIntraPredictionMode::Angular(42),
    );
    let lossy_policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy);

    let candidates = vvc_luma_directional_search_candidates(lossy_policy, &frame, &state, target);
    let indexes: Vec<_> = candidates
        .iter()
        .map(|mode| mode.luma_mode_index())
        .collect();
    assert_eq!(indexes, vec![42, 18, 50, 34, 2, 66]);
}

#[test]
fn vvc_lossless_speed_luma_directional_search_uses_source_seed_index() {
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    let frame = sampled_luma_frame(
        8,
        8,
        (0..8)
            .flat_map(|_| (0..8).map(|x| (x * 16) as VvcSample))
            .collect(),
    );
    let state = VvcLumaModeSearchState::new_for_geometry(frame.geometry);
    let policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);

    let candidates = vvc_luma_directional_search_candidates(policy, &frame, &state, node);
    let indexes: Vec<_> = candidates
        .iter()
        .map(|mode| mode.luma_mode_index())
        .collect();

    assert_eq!(indexes, vec![50]);
}

#[test]
fn vvc_lossless_speed_luma_directional_search_uses_neighbor_index() {
    let mut target = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    target.x = 8;
    let frame = sampled_luma_frame(16, 8, vec![64; 128]);
    let mut state = VvcLumaModeSearchState::new_for_geometry(frame.geometry);
    state.mark_node(
        VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma),
        VvcIntraPredictionMode::Angular(42),
    );
    let policy = VvcResidualCodingPolicy::new(frame.format, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);

    let candidates = vvc_luma_directional_search_candidates(policy, &frame, &state, target);
    let indexes: Vec<_> = candidates
        .iter()
        .map(|mode| mode.luma_mode_index())
        .collect();

    assert_eq!(indexes, vec![42]);
}

#[test]
fn vvc_lossless_speed_luma_search_keeps_dc_for_lossy() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let default_lossy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy);
    let fast_lossy = default_lossy.with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossless = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossless)
        .with_fast_search(VvcFastSearch::LosslessSpeed);

    assert!(!vvc_luma_lossless_speed_skips_dc(default_lossy));
    assert!(!vvc_luma_lossless_speed_skips_dc(fast_lossy));
    assert!(vvc_luma_lossless_speed_skips_dc(fast_lossless));
}

#[test]
fn vvc_lossless_speed_luma_planar_search_uses_neighbor_gate() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let default_lossy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy);
    let fast_lossy = default_lossy.with_fast_search(VvcFastSearch::LosslessSpeed);

    assert!(vvc_luma_lossless_speed_evaluates_planar(
        default_lossy,
        Some(VvcIntraPredictionMode::Horizontal),
        Some(VvcIntraPredictionMode::Vertical),
    ));
    assert!(!vvc_luma_lossless_speed_evaluates_planar(
        fast_lossy,
        Some(VvcIntraPredictionMode::Horizontal),
        Some(VvcIntraPredictionMode::Vertical),
    ));
    assert!(vvc_luma_lossless_speed_evaluates_planar(
        fast_lossy,
        Some(VvcIntraPredictionMode::Planar),
        Some(VvcIntraPredictionMode::Horizontal),
    ));
    assert!(vvc_luma_lossless_speed_evaluates_planar(
        fast_lossy, None, None,
    ));
    let fast_lossless = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossless)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    assert!(!vvc_luma_lossless_speed_evaluates_planar(
        fast_lossless,
        Some(VvcIntraPredictionMode::Horizontal),
        Some(VvcIntraPredictionMode::Vertical),
    ));
}

#[test]
fn vvc_lossless_speed_chroma_keeps_lossy_candidates() {
    let yuv444 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let yuv420 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let yuv444_10 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(10).expect("valid bit depth"),
    };
    let default_lossy = VvcResidualCodingPolicy::new(yuv444, VvcResidualCodingMode::Lossy);
    let fast_lossy = default_lossy.with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossy_420 = VvcResidualCodingPolicy::new(yuv420, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossy_444_10 = VvcResidualCodingPolicy::new(yuv444_10, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossless = VvcResidualCodingPolicy::new(yuv444, VvcResidualCodingMode::Lossless)
        .with_fast_search(VvcFastSearch::LosslessSpeed);

    assert!(!vvc_chroma_fast_search_uses_derived_only(default_lossy));
    assert!(!vvc_chroma_fast_search_uses_derived_only(fast_lossy));
    assert!(!vvc_chroma_fast_search_uses_derived_only(fast_lossy_420));
    assert!(!vvc_chroma_fast_search_uses_derived_only(fast_lossy_444_10));
    assert!(vvc_chroma_fast_search_uses_derived_only(fast_lossless));
}

#[test]
fn vvc_lossless_speed_chroma_prunes_lossy_explicit_dc_search() {
    let yuv444 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let default_lossy = VvcResidualCodingPolicy::new(yuv444, VvcResidualCodingMode::Lossy);
    let fast_lossy = default_lossy.with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossless = VvcResidualCodingPolicy::new(yuv444, VvcResidualCodingMode::Lossless)
        .with_fast_search(VvcFastSearch::LosslessSpeed);

    assert!(vvc_chroma_explicit_candidate_allowed_for_search(
        default_lossy,
        VvcIntraPredictionMode::Dc,
    ));
    assert!(!vvc_chroma_explicit_candidate_allowed_for_search(
        fast_lossy,
        VvcIntraPredictionMode::Dc,
    ));
    assert!(vvc_chroma_explicit_candidate_allowed_for_search(
        fast_lossy,
        VvcIntraPredictionMode::Planar,
    ));
    assert!(!vvc_chroma_explicit_candidate_allowed_for_search(
        fast_lossless,
        VvcIntraPredictionMode::Planar,
    ));
}

#[test]
fn vvc_lossless_speed_cclm_search_is_444_only_for_lossy() {
    let yuv420 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let yuv444 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let off_lossy_420 = VvcResidualCodingPolicy::new(yuv420, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::Off);
    let fast_lossy_420 = VvcResidualCodingPolicy::new(yuv420, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossy_444 = VvcResidualCodingPolicy::new(yuv444, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);

    assert!(vvc_chroma_cclm_fast_search_allowed(off_lossy_420, 0, 8, 8));
    assert!(!vvc_chroma_cclm_fast_search_allowed(
        fast_lossy_420,
        u64::MAX,
        8,
        8,
    ));
    assert!(vvc_chroma_cclm_fast_search_allowed(fast_lossy_444, 0, 8, 8));
}

#[test]
fn vvc_lossless_speed_luma_rd_prefers_transform_skip_first_for_lossy() {
    let yuv420 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let yuv444 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let yuv444_10 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(10).expect("valid bit depth"),
    };
    let default_lossy = VvcResidualCodingPolicy::new(yuv420, VvcResidualCodingMode::Lossy);
    let fast_lossy = default_lossy.with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossy_444 = VvcResidualCodingPolicy::new(yuv444, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossy_444_10 = VvcResidualCodingPolicy::new(yuv444_10, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossless = VvcResidualCodingPolicy::new(yuv420, VvcResidualCodingMode::Lossless)
        .with_fast_search(VvcFastSearch::LosslessSpeed);

    assert!(!vvc_luma_fast_search_prefers_transform_skip_candidate(
        default_lossy
    ));
    assert!(vvc_luma_fast_search_prefers_transform_skip_candidate(
        fast_lossy
    ));
    assert!(vvc_luma_fast_search_prefers_transform_skip_candidate(
        fast_lossy_444
    ));
    assert!(vvc_luma_fast_search_prefers_transform_skip_candidate(
        fast_lossy_444_10
    ));
    assert!(!vvc_luma_fast_search_prefers_transform_skip_candidate(
        fast_lossless
    ));
}

#[test]
fn vvc_lossless_speed_chroma_rd_compares_transform_skip_for_lossy() {
    let yuv444 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let yuv420 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let yuv422 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs422,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let yuv444_10 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(10).expect("valid bit depth"),
    };
    let default_lossy = VvcResidualCodingPolicy::new(yuv444, VvcResidualCodingMode::Lossy);
    let fast_lossy = default_lossy.with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossy_420 = VvcResidualCodingPolicy::new(yuv420, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossy_422 = VvcResidualCodingPolicy::new(yuv422, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let fast_lossy_444_10 = VvcResidualCodingPolicy::new(yuv444_10, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);

    assert!(!vvc_chroma_fast_search_uses_transform_skip_candidate(
        default_lossy
    ));
    assert!(!vvc_chroma_fast_search_uses_transform_skip_candidate(
        fast_lossy
    ));
    assert!(vvc_chroma_fast_search_uses_transform_skip_candidate(
        fast_lossy_420
    ));
    assert!(vvc_chroma_fast_search_uses_transform_skip_candidate(
        fast_lossy_422
    ));
    assert!(vvc_chroma_fast_search_uses_transform_skip_candidate(
        fast_lossy_444_10
    ));
}

#[test]
fn vvc_lossy_luma_rd_shortlist_keeps_best_winners() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    let costs = VvcLumaIntraCandidateCosts::new(1_000_000)
        .with_candidate(VvcIntraPredictionMode::Planar, Some(200_000))
        .with_candidate(VvcIntraPredictionMode::Horizontal, Some(50_000))
        .with_candidate(VvcIntraPredictionMode::Vertical, Some(75_000))
        .with_candidate(VvcIntraPredictionMode::Angular(34), Some(25_000))
        .with_candidate(VvcIntraPredictionMode::Angular(66), Some(10_000))
        .with_candidate(VvcIntraPredictionMode::Angular(2), Some(125_000))
        .with_candidate(VvcIntraPredictionMode::Angular(10), Some(150_000))
        .with_candidate(VvcIntraPredictionMode::Angular(20), Some(175_000))
        .with_candidate(VvcIntraPredictionMode::Angular(30), Some(180_000))
        .with_candidate(VvcIntraPredictionMode::Angular(34), Some(5_000));

    let shortlist = VvcLumaModeRdShortlist::from_candidate_costs(policy, node, costs);
    let indexes: Vec<_> = shortlist
        .iter()
        .map(|candidate| candidate.mode().luma_mode_index())
        .collect();

    assert_eq!(indexes.len(), VVC_LOSSY_LUMA_RD_WINNER_CANDIDATES);
    assert_eq!(indexes, vec![34, 66, 18, 50, 2]);
    assert!(!indexes.contains(&0));
    assert!(!indexes.contains(&1));
}

#[test]
fn vvc_lossless_speed_luma_rd_shortlist_keeps_top_two_lossy_winners() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    let costs = VvcLumaIntraCandidateCosts::new(1_000_000)
        .with_candidate(VvcIntraPredictionMode::Planar, Some(200_000))
        .with_candidate(VvcIntraPredictionMode::Horizontal, Some(50_000))
        .with_candidate(VvcIntraPredictionMode::Vertical, Some(75_000))
        .with_candidate(VvcIntraPredictionMode::Angular(34), Some(25_000))
        .with_candidate(VvcIntraPredictionMode::Angular(66), Some(10_000));

    let shortlist = VvcLumaModeRdShortlist::from_candidate_costs(policy, node, costs);
    let indexes: Vec<_> = shortlist
        .iter()
        .map(|candidate| candidate.mode().luma_mode_index())
        .collect();

    assert_eq!(indexes, vec![66, 34]);
}

#[test]
fn vvc_lossless_speed_luma_rd_shortlist_keeps_top_screen_content_winner() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    let costs = VvcLumaIntraCandidateCosts::new(1_000_000)
        .with_candidate(VvcIntraPredictionMode::Planar, Some(200_000))
        .with_candidate(VvcIntraPredictionMode::Horizontal, Some(50_000))
        .with_candidate(VvcIntraPredictionMode::Vertical, Some(75_000))
        .with_candidate(VvcIntraPredictionMode::Angular(34), Some(25_000))
        .with_candidate(VvcIntraPredictionMode::Angular(66), Some(10_000));

    let shortlist = VvcLumaModeRdShortlist::from_candidate_costs(policy, node, costs);
    let indexes: Vec<_> = shortlist
        .iter()
        .map(|candidate| candidate.mode().luma_mode_index())
        .collect();

    assert_eq!(indexes, vec![66]);
}

#[test]
fn vvc_lossless_luma_rd_shortlist_keeps_all_candidates() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossless);
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeLuma);
    let costs = VvcLumaIntraCandidateCosts::new(1_000)
        .with_candidate(VvcIntraPredictionMode::Planar, Some(200))
        .with_candidate(VvcIntraPredictionMode::Horizontal, Some(50))
        .with_candidate(VvcIntraPredictionMode::Vertical, Some(75))
        .with_candidate(VvcIntraPredictionMode::Angular(34), Some(25))
        .with_candidate(VvcIntraPredictionMode::Angular(66), Some(10));

    let shortlist = VvcLumaModeRdShortlist::from_candidate_costs(policy, node, costs);
    let indexes: Vec<_> = shortlist
        .iter()
        .map(|candidate| candidate.mode().luma_mode_index())
        .collect();

    assert_eq!(indexes, vec![66, 34, 18, 50, 0, 1]);
}

#[test]
fn vvc_lossy_chroma_rd_shortlist_keeps_best_winners() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy);
    let costs = VvcChromaIntraCandidateCosts::new(1_000_000)
        .with_candidate(
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Planar),
            Some(200_000),
        )
        .with_candidate(
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
            Some(50_000),
        )
        .with_candidate(
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Vertical),
            Some(75_000),
        )
        .with_candidate(
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
            Some(25_000),
        )
        .with_candidate(
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmLeft),
            Some(10_000),
        )
        .with_candidate(
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmTop),
            Some(125_000),
        );

    let shortlist = VvcChromaModeRdShortlist::from_candidate_costs(policy, costs);
    let modes: Vec<_> = shortlist.iter().map(|candidate| candidate.mode()).collect();

    assert_eq!(modes.len(), VVC_LOSSY_CHROMA_RD_WINNER_CANDIDATES);
    assert_eq!(
        modes,
        vec![
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmLeft),
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Vertical),
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmTop),
        ]
    );
}

#[test]
fn vvc_lossless_speed_chroma_rd_shortlist_keeps_top_three_lossy_winners() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let costs = VvcChromaIntraCandidateCosts::new(1_000_000)
        .with_candidate(
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
            Some(50_000),
        )
        .with_candidate(
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
            Some(25_000),
        );

    let shortlist = VvcChromaModeRdShortlist::from_candidate_costs(policy, costs);
    let modes: Vec<_> = shortlist.iter().map(|candidate| candidate.mode()).collect();

    assert_eq!(
        modes,
        vec![
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
            VvcChromaIntraPredictionMode::Derived,
        ]
    );
}

#[test]
fn vvc_lossless_chroma_rd_shortlist_keeps_all_candidates() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossless);
    let costs = VvcChromaIntraCandidateCosts::new(1_000)
        .with_candidate(
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Planar),
            Some(200),
        )
        .with_candidate(
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
            Some(50),
        )
        .with_candidate(
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Vertical),
            Some(75),
        )
        .with_candidate(
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
            Some(25),
        )
        .with_candidate(
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmLeft),
            Some(10),
        )
        .with_candidate(
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmTop),
            Some(125),
        );

    let shortlist = VvcChromaModeRdShortlist::from_candidate_costs(policy, costs);
    let modes: Vec<_> = shortlist.iter().map(|candidate| candidate.mode()).collect();

    assert_eq!(
        modes,
        vec![
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmLeft),
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Vertical),
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::MdlmTop),
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Planar),
            VvcChromaIntraPredictionMode::Derived,
        ]
    );
}

#[test]
fn vvc_lossless_speed_direct_chroma_bdpcm_is_scoped_to_lossy_444_derived() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);

    assert_eq!(
        vvc_chroma_lossy_speed_direct_bdpcm_candidate_modes(
            policy,
            ChromaSampling::Cs444,
            format.bit_depth,
            VvcChromaIntraPredictionMode::Derived,
            VvcIntraPredictionMode::Horizontal,
        ),
        [Some(VvcBdpcmMode::Horizontal), None]
    );
    assert_eq!(
        vvc_chroma_lossy_speed_direct_bdpcm_candidate_modes(
            policy,
            ChromaSampling::Cs444,
            format.bit_depth,
            VvcChromaIntraPredictionMode::Derived,
            VvcIntraPredictionMode::Vertical,
        ),
        [Some(VvcBdpcmMode::Vertical), None]
    );
    assert_eq!(
        vvc_chroma_lossy_speed_direct_bdpcm_candidate_modes(
            policy,
            ChromaSampling::Cs444,
            format.bit_depth,
            VvcChromaIntraPredictionMode::Derived,
            VvcIntraPredictionMode::Planar,
        ),
        [Some(VvcBdpcmMode::Horizontal), Some(VvcBdpcmMode::Vertical)]
    );
    assert_eq!(
        vvc_chroma_lossy_speed_direct_bdpcm_candidate_modes(
            policy,
            ChromaSampling::Cs444,
            format.bit_depth,
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
            VvcIntraPredictionMode::Horizontal,
        ),
        [None, None]
    );
    assert_eq!(
        vvc_chroma_lossy_speed_direct_bdpcm_candidate_modes(
            policy,
            ChromaSampling::Cs422,
            format.bit_depth,
            VvcChromaIntraPredictionMode::Derived,
            VvcIntraPredictionMode::Horizontal,
        ),
        [None, None]
    );
    assert_eq!(
        vvc_chroma_lossy_speed_direct_bdpcm_candidate_modes(
            policy,
            ChromaSampling::Cs444,
            SampleBitDepth::new(10).expect("valid bit depth"),
            VvcChromaIntraPredictionMode::Derived,
            VvcIntraPredictionMode::Horizontal,
        ),
        [None, None]
    );
    assert_eq!(
        vvc_chroma_lossy_speed_direct_bdpcm_candidate_modes(
            VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy),
            ChromaSampling::Cs444,
            format.bit_depth,
            VvcChromaIntraPredictionMode::Derived,
            VvcIntraPredictionMode::Horizontal,
        ),
        [None, None]
    );
}

#[test]
fn vvc_lossless_speed_chroma_bdpcm_skips_high_depth_420() {
    let yuv420p8 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let yuv420p10 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(10).expect("valid bit depth"),
    };
    let yuv422p10 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs422,
        bit_depth: SampleBitDepth::new(10).expect("valid bit depth"),
    };
    let yuv444p10 = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs444,
        bit_depth: SampleBitDepth::new(10).expect("valid bit depth"),
    };
    let lossless_speed = |format| {
        VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossless)
            .with_fast_search(VvcFastSearch::LosslessSpeed)
    };

    assert!(vvc_chroma_lossless_speed_bdpcm_format_allowed(
        lossless_speed(yuv420p8),
        yuv420p8,
    ));
    assert!(!vvc_chroma_lossless_speed_bdpcm_format_allowed(
        lossless_speed(yuv420p10),
        yuv420p10,
    ));
    assert!(vvc_chroma_lossless_speed_bdpcm_format_allowed(
        lossless_speed(yuv422p10),
        yuv422p10,
    ));
    assert!(vvc_chroma_lossless_speed_bdpcm_format_allowed(
        lossless_speed(yuv444p10),
        yuv444p10,
    ));
    assert!(vvc_chroma_bdpcm_fast_search_allowed(
        lossless_speed(yuv444p10),
        VvcChromaIntraPredictionMode::Derived,
    ));
    assert!(!vvc_chroma_bdpcm_fast_search_allowed(
        lossless_speed(yuv444p10),
        VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Horizontal),
    ));
    assert!(vvc_chroma_lossless_speed_bdpcm_format_allowed(
        VvcResidualCodingPolicy::new(yuv420p10, VvcResidualCodingMode::Lossless),
        yuv420p10,
    ));
}

#[test]
fn vvc_direct_chroma_bdpcm_residual_gate_requires_sse_gain() {
    let selected_cb = [4i16; 16];
    let selected_cr = [4i16; 16];
    let better_cb = [3i16; 16];
    let better_cr = [3i16; 16];
    let equal_cb = selected_cb;
    let equal_cr = selected_cr;
    let worse_cb = [5i16; 16];
    let worse_cr = [5i16; 16];

    assert!(vvc_chroma_direct_bdpcm_residual_is_safe(
        &selected_cb,
        &selected_cr,
        &better_cb,
        &better_cr,
    ));
    assert!(!vvc_chroma_direct_bdpcm_residual_is_safe(
        &selected_cb,
        &selected_cr,
        &equal_cb,
        &equal_cr,
    ));
    assert!(!vvc_chroma_direct_bdpcm_residual_is_safe(
        &selected_cb,
        &selected_cr,
        &worse_cb,
        &worse_cr,
    ));
}

#[test]
fn vvc_lossy_temporal_mode_hints_are_tightly_gated() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let lossy_speed = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let lossy_off = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy);
    let lossless_speed = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossless)
        .with_fast_search(VvcFastSearch::LosslessSpeed);

    assert_eq!(
        vvc_temporal_mode_hint_max_avg_abs_residual_8bit(lossy_speed),
        Some(0)
    );
    assert_eq!(
        vvc_temporal_mode_hint_max_avg_abs_residual_8bit(lossy_off),
        None
    );
    assert_eq!(
        vvc_temporal_mode_hint_max_avg_abs_residual_8bit(lossless_speed),
        Some(16)
    );
    assert!(vvc_temporal_mode_hint_residual_is_cheap(
        &[0; 16],
        16,
        format.bit_depth,
        lossy_speed,
    ));
    assert!(!vvc_temporal_mode_hint_residual_is_cheap(
        &[1; 16],
        16,
        format.bit_depth,
        lossy_speed,
    ));
    assert!(vvc_temporal_mode_hint_residual_is_cheap(
        &[16; 16],
        16,
        format.bit_depth,
        lossless_speed,
    ));
    assert!(!vvc_temporal_mode_hint_residual_is_cheap(
        &[17; 16],
        16,
        format.bit_depth,
        lossless_speed,
    ));
    assert!(!vvc_temporal_mode_hint_residual_is_cheap(
        &[1; 16],
        16,
        format.bit_depth,
        lossy_off,
    ));
}

#[test]
fn vvc_temporal_chroma_explicit_hint_requires_current_candidate_index() {
    let format = VvcPictureFormat {
        chroma_sampling: ChromaSampling::Cs420,
        bit_depth: SampleBitDepth::new(8).expect("valid bit depth"),
    };
    let policy = VvcResidualCodingPolicy::new(format, VvcResidualCodingMode::Lossy)
        .with_fast_search(VvcFastSearch::LosslessSpeed);
    let geometry = VvcVideoGeometry {
        width: 8,
        height: 8,
    };
    let node = VvcCodingTreeNode::root(8, 8, VvcTreeType::DualTreeChroma);

    assert_eq!(
        vvc_supported_temporal_chroma_mode_hint(
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Vertical),
            policy,
            geometry,
            node,
            VvcIntraPredictionMode::Planar,
        ),
        VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Vertical)
    );
    assert_eq!(
        vvc_supported_temporal_chroma_mode_hint(
            VvcChromaIntraPredictionMode::Explicit(VvcIntraPredictionMode::Planar),
            policy,
            geometry,
            node,
            VvcIntraPredictionMode::Planar,
        ),
        VvcChromaIntraPredictionMode::Derived
    );
}

#[test]
fn vvc_luma_quality_gate_can_spend_bits_for_lower_distortion() {
    let best = VvcLumaQuantizedResidualScore {
        distortion: 1_000,
        rate_cost: 0,
    };
    let candidate = VvcLumaQuantizedResidualScore {
        distortion: 700,
        rate_cost: 20,
    };

    assert!(candidate.selects_over(best));
    assert!(!VvcLumaQuantizedResidualScore {
        distortion: 1_100,
        rate_cost: 0,
    }
    .selects_over(best));

    let best = VvcResidualBlockScore {
        distortion: 1_000,
        rate_cost: 0,
    };
    let candidate = VvcResidualBlockScore {
        distortion: 700,
        rate_cost: 20,
    };

    assert!(candidate.selects_over(best));
    assert!(!VvcResidualBlockScore {
        distortion: 1_100,
        rate_cost: 0,
    }
    .selects_over(best));

    assert!(vvc_transform_skip_short_circuits_transformed(
        VvcResidualBlockScore {
            distortion: 0,
            rate_cost: 128,
        }
    ));
    assert!(!vvc_transform_skip_short_circuits_transformed(
        VvcResidualBlockScore {
            distortion: 1,
            rate_cost: 0,
        }
    ));
}

#[test]
fn vvc_luma_exact_prediction_bypasses_rd() {
    assert!(vvc_luma_exact_prediction_skips_rd(&[0, 0, 0, 0]));
    assert!(!vvc_luma_exact_prediction_skips_rd(&[0, 1, 0, 0]));

    let zero_residual = VvcSelectedLumaResidual {
        block: VvcFinalizedResidualBlock {
            dc_level: 0,
            ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
            has_ac: false,
            transform_skip: false,
            bdpcm_mode: VvcBdpcmMode::None,
        },
        mts_index: 0,
    };
    let scored_zero_residual = VvcScoredSelectedLumaResidual {
        residual: zero_residual,
        score: VvcResidualBlockScore {
            distortion: 4,
            rate_cost: 0,
        },
    };
    assert!(vvc_luma_zero_coded_residual_skips_rd(
        scored_zero_residual,
        4,
    ));
    assert!(!vvc_luma_zero_coded_residual_skips_rd(
        VvcScoredSelectedLumaResidual {
            score: VvcResidualBlockScore {
                distortion: 5,
                rate_cost: 0,
            },
            ..scored_zero_residual
        },
        4,
    ));

    let nonzero_residual = VvcSelectedLumaResidual {
        block: VvcFinalizedResidualBlock {
            dc_level: 1,
            ..zero_residual.block
        },
        mts_index: 0,
    };
    assert!(!vvc_luma_zero_coded_residual_skips_rd(
        VvcScoredSelectedLumaResidual {
            residual: nonzero_residual,
            score: VvcResidualBlockScore {
                distortion: 0,
                rate_cost: 0,
            },
        },
        4,
    ));
}

#[test]
fn vvc_luma_exact_min_syntax_score_stops_mode_search() {
    assert!(vvc_luma_exact_min_syntax_mode_search_done(u64::from(
        VVC_LUMA_MIN_INTRA_MODE_SYNTAX_BINS,
    )));
    assert!(!vvc_luma_exact_min_syntax_mode_search_done(u64::from(
        VVC_LUMA_MIN_INTRA_MODE_SYNTAX_BINS + 1,
    )));

    for (left, above) in [
        (None, None),
        (Some(VvcIntraPredictionMode::Horizontal), None),
        (
            Some(VvcIntraPredictionMode::Horizontal),
            Some(VvcIntraPredictionMode::Vertical),
        ),
        (
            Some(VvcIntraPredictionMode::Angular(34)),
            Some(VvcIntraPredictionMode::Angular(35)),
        ),
    ] {
        let min_bins = (0..=66)
            .map(|index| match index {
                0 => VvcIntraPredictionMode::Planar,
                1 => VvcIntraPredictionMode::Dc,
                18 => VvcIntraPredictionMode::Horizontal,
                50 => VvcIntraPredictionMode::Vertical,
                _ => VvcIntraPredictionMode::Angular(index),
            })
            .map(|mode| vvc_luma_intra_mode_syntax_bin_count(mode, left, above))
            .min()
            .expect("luma has intra mode candidates");
        assert_eq!(min_bins, VVC_LUMA_MIN_INTRA_MODE_SYNTAX_BINS);
    }
}

#[test]
fn vvc_chroma_exact_prediction_bypasses_rd() {
    assert!(vvc_chroma_exact_prediction_skips_rd(
        &[0, 0, 0, 0],
        &[0, 0, 0, 0],
    ));
    assert!(!vvc_chroma_exact_prediction_skips_rd(
        &[0, 0, 0, 0],
        &[0, -1, 0, 0],
    ));
}

#[test]
fn vvc_chroma_lossy_exact_prediction_stops_mode_search() {
    assert!(vvc_chroma_lossy_exact_mode_search_done(false, 0));
    assert!(!vvc_chroma_lossy_exact_mode_search_done(false, 1));
    assert!(!vvc_chroma_lossy_exact_mode_search_done(true, 0));
}

#[test]
fn vvc_zero_transform_skip_residuals_keep_zero_levels() {
    let bit_depth = SampleBitDepth::new(10).expect("valid bit depth");
    let quant_table = VvcTransformSkipQuantTable::new(bit_depth, 19);

    let luma = finalize_vvc_luma_transform_skip_residual_block(&[0; 64], 8, 8, &quant_table);
    assert_eq!(luma.dc_level, 0);
    assert!(!luma.has_ac);
    assert!(luma.ac_levels.iter().all(|level| *level == 0));

    let luma_bdpcm = finalize_vvc_luma_bdpcm_transform_skip_residual_block(
        &[0; 32],
        4,
        8,
        &quant_table,
        VvcBdpcmMode::Horizontal,
    );
    assert_eq!(luma_bdpcm.dc_level, 0);
    assert!(!luma_bdpcm.has_ac);

    let chroma = finalize_vvc_chroma_transform_skip_residual_block(&[0; 16], 4, 4, &quant_table);
    assert_eq!(chroma.dc_level, 0);
    assert!(!chroma.has_ac);
    assert!(chroma.ac_levels.iter().all(|level| *level == 0));

    let chroma_bdpcm = finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
        &[0; 16],
        4,
        4,
        &quant_table,
        VvcBdpcmMode::Vertical,
    );
    assert_eq!(chroma_bdpcm.dc_level, 0);
    assert!(!chroma_bdpcm.has_ac);
}

#[test]
fn vvc_transform_skip_finalizers_preserve_component_extents_and_bdpcm_layouts() {
    let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
    let qp = crate::vvc::vvc_lossless_slice_qp(bit_depth);
    let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);

    let mut tall_residuals = vec![0; 4 * 8];
    tall_residuals[31] = 7;
    let luma = finalize_vvc_luma_transform_skip_residual_block(&tall_residuals, 4, 8, &quant_table);
    let chroma =
        finalize_vvc_chroma_transform_skip_residual_block(&tall_residuals, 4, 8, &quant_table);
    assert!(!luma.has_ac, "4x8 luma stores only its active 4x4 extent");
    assert!(luma.ac_levels.iter().all(|&level| level == 0));
    assert!(
        chroma.has_ac,
        "4x8 chroma stores its complete active extent"
    );
    assert_eq!(chroma.ac_levels[30], quant_table.level(7));

    let bdpcm_residuals = [10, 13, 20, 27];
    let luma_bdpcm = finalize_vvc_luma_bdpcm_transform_skip_residual_block(
        &bdpcm_residuals,
        2,
        2,
        &quant_table,
        VvcBdpcmMode::Horizontal,
    );
    let chroma_bdpcm = finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
        &bdpcm_residuals,
        2,
        2,
        &quant_table,
        VvcBdpcmMode::Horizontal,
    );
    let levels = bdpcm_residuals.map(|residual| quant_table.level(residual));
    assert_eq!(luma_bdpcm.dc_level, levels[0]);
    assert_eq!(luma_bdpcm.ac_levels[0], levels[1] - levels[0]);
    assert_eq!(luma_bdpcm.ac_levels[1], levels[2]);
    assert_eq!(luma_bdpcm.ac_levels[2], levels[3] - levels[2]);
    assert_eq!(chroma_bdpcm.dc_level, levels[0]);
    assert_eq!(chroma_bdpcm.ac_levels[0], levels[1] - levels[0]);
    assert_eq!(chroma_bdpcm.ac_levels[3], levels[2]);
    assert_eq!(chroma_bdpcm.ac_levels[4], levels[3] - levels[2]);
}

#[test]
fn vvc_transform_skip_reconstruction_shares_component_layouts() {
    let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
    let qp = crate::vvc::vvc_lossless_slice_qp(bit_depth);
    let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);

    let mut ac_levels = [0; VVC_LUMA_AC_COEFFS_PER_TU];
    ac_levels[30] = quant_table.level(7);
    let mut luma = Vec::new();
    reconstruct_vvc_luma_transform_skip_residuals_into_with_qp(
        &mut luma, 0, &ac_levels, 4, 8, bit_depth, qp,
    );
    let mut chroma = Vec::new();
    reconstruct_vvc_chroma_transform_skip_residuals_into_with_qp(
        &mut chroma,
        0,
        &ac_levels,
        4,
        8,
        bit_depth,
        qp,
    );
    assert_eq!(luma.len(), 32);
    assert_eq!(chroma.len(), 32);
    assert_eq!(luma[31], 0, "4x8 luma reconstructs only its 4x4 extent");
    assert_eq!(chroma[31], quant_table.reconstructed(ac_levels[30]));

    let source = [10, 13, 20, 27];
    let luma_block = finalize_vvc_luma_bdpcm_transform_skip_residual_block(
        &source,
        2,
        2,
        &quant_table,
        VvcBdpcmMode::Horizontal,
    );
    let chroma_block = finalize_vvc_chroma_bdpcm_transform_skip_residual_block(
        &source,
        2,
        2,
        &quant_table,
        VvcBdpcmMode::Horizontal,
    );
    reconstruct_vvc_luma_bdpcm_transform_skip_residuals_into_with_qp(
        &mut luma,
        luma_block.dc_level,
        &luma_block.ac_levels,
        2,
        2,
        bit_depth,
        qp,
        luma_block.bdpcm_mode,
    );
    reconstruct_vvc_chroma_bdpcm_transform_skip_residuals_into_with_qp(
        &mut chroma,
        chroma_block.dc_level,
        &chroma_block.ac_levels,
        2,
        2,
        bit_depth,
        qp,
        chroma_block.bdpcm_mode,
    );
    assert_eq!(luma, source);
    assert_eq!(chroma, source);
}

#[test]
fn vvc_transform_skip_table_reconstructs_indexed_quantized_levels() {
    let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
    let quant_table = VvcTransformSkipQuantTable::new(bit_depth, 19);
    let (scale, right_shift) = vvc_transform_skip_dequant_params(bit_depth, 19);

    for level in [-511i16, -100, -17, -1, 0, 1, 17, 100, 511] {
        let expected = reconstruct_vvc_transform_skip_level_with_params(level, scale, right_shift);
        assert_eq!(quant_table.reconstructed(level), expected, "level={level}");
    }
}

#[test]
fn vvc_lossless_8x8_transform_skip_reconstructs_every_sample() {
    let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
    let qp = crate::vvc::vvc_lossless_slice_qp(bit_depth);
    let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);
    let source_residuals = vec![-96; 64];
    let block =
        finalize_vvc_luma_transform_skip_residual_block(&source_residuals, 8, 8, &quant_table);
    let mut reconstructed = Vec::new();
    reconstruct_vvc_luma_transform_skip_residuals_into_with_qp(
        &mut reconstructed,
        block.dc_level,
        &block.ac_levels,
        8,
        8,
        bit_depth,
        qp,
    );
    assert_eq!(reconstructed, source_residuals);
}

#[test]
fn vvc_direct_luma_transform_skip_sse_matches_reconstruction() {
    let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
    let qp = super::super::VVC_DEFAULT_LOSSY_LUMA_QP;
    let width = 8;
    let height = 8;
    let source_residuals: Vec<i16> = (0..width * height)
        .map(|idx| ((idx as i16 * 7) % 31) - 15)
        .collect();
    let mut ac_levels = [0; VVC_LUMA_AC_COEFFS_PER_TU];
    for (idx, level) in ac_levels.iter_mut().enumerate().take(63) {
        *level = ((idx as i16 % 5) - 2).clamp(-2, 2);
    }
    let block = VvcFinalizedResidualBlock {
        dc_level: 3,
        ac_levels,
        has_ac: true,
        transform_skip: true,
        bdpcm_mode: VvcBdpcmMode::None,
    };
    let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);

    let actual =
        luma_transform_skip_residual_sse(&source_residuals, width, height, &quant_table, block);
    let mut reconstructed = Vec::new();
    reconstruct_vvc_luma_transform_skip_residuals_into_with_qp(
        &mut reconstructed,
        block.dc_level,
        &block.ac_levels,
        width,
        height,
        bit_depth,
        qp,
    );
    assert_eq!(
        actual,
        residual_sse_for_test(&source_residuals, &reconstructed)
    );
}

#[test]
fn vvc_direct_luma_bdpcm_transform_skip_sse_matches_reconstruction() {
    let bit_depth = SampleBitDepth::new(10).expect("valid bit depth");
    let qp = super::super::VVC_DEFAULT_LOSSY_LUMA_QP;
    let width = 4;
    let height = 8;
    let source_residuals: Vec<i16> = (0..width * height)
        .map(|idx| ((idx as i16 * 11) % 43) - 21)
        .collect();
    let mut ac_levels = [0; VVC_LUMA_AC_COEFFS_PER_TU];
    for (idx, level) in ac_levels.iter_mut().enumerate().take(15) {
        *level = (idx as i16 % 7) - 3;
    }
    let block = VvcFinalizedResidualBlock {
        dc_level: -4,
        ac_levels,
        has_ac: true,
        transform_skip: true,
        bdpcm_mode: VvcBdpcmMode::Horizontal,
    };
    let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);

    let actual =
        luma_transform_skip_residual_sse(&source_residuals, width, height, &quant_table, block);
    let mut reconstructed = Vec::new();
    reconstruct_vvc_luma_bdpcm_transform_skip_residuals_into_with_qp(
        &mut reconstructed,
        block.dc_level,
        &block.ac_levels,
        width,
        height,
        bit_depth,
        qp,
        block.bdpcm_mode,
    );
    assert_eq!(
        actual,
        residual_sse_for_test(&source_residuals, &reconstructed)
    );
}

#[test]
fn vvc_direct_chroma_transform_skip_sse_matches_reconstruction() {
    let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
    let qp = super::super::VVC_DEFAULT_LOSSY_CHROMA_QP;
    let width = 3;
    let height = 4;
    let source_residuals: Vec<i16> = (0..width * height)
        .map(|idx| ((idx as i16 * 5) % 23) - 11)
        .collect();
    let mut ac_levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
    for (idx, level) in ac_levels.iter_mut().enumerate() {
        *level = (idx as i16 % 5) - 2;
    }
    let block = VvcFinalizedResidualBlock {
        dc_level: 2,
        ac_levels,
        has_ac: true,
        transform_skip: true,
        bdpcm_mode: VvcBdpcmMode::None,
    };
    let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);

    let actual =
        chroma_transform_skip_residual_sse(&source_residuals, width, height, &quant_table, block);
    let mut reconstructed = Vec::new();
    reconstruct_vvc_chroma_transform_skip_residuals_into_with_qp(
        &mut reconstructed,
        block.dc_level,
        &block.ac_levels,
        width,
        height,
        bit_depth,
        qp,
    );
    assert_eq!(
        actual,
        residual_sse_for_test(&source_residuals, &reconstructed)
    );
}

#[test]
fn vvc_direct_chroma_bdpcm_transform_skip_sse_matches_reconstruction() {
    let bit_depth = SampleBitDepth::new(10).expect("valid bit depth");
    let qp = super::super::VVC_DEFAULT_LOSSY_CHROMA_QP;
    let width = 4;
    let height = 3;
    let source_residuals: Vec<i16> = (0..width * height)
        .map(|idx| ((idx as i16 * 13) % 37) - 18)
        .collect();
    let mut ac_levels = [0; VVC_CHROMA_AC_COEFFS_PER_TU];
    for (idx, level) in ac_levels.iter_mut().enumerate() {
        *level = (idx as i16 % 7) - 3;
    }
    let block = VvcFinalizedResidualBlock {
        dc_level: -2,
        ac_levels,
        has_ac: true,
        transform_skip: true,
        bdpcm_mode: VvcBdpcmMode::Vertical,
    };
    let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);

    let actual =
        chroma_transform_skip_residual_sse(&source_residuals, width, height, &quant_table, block);
    let mut reconstructed = Vec::new();
    reconstruct_vvc_chroma_bdpcm_transform_skip_residuals_into_with_qp(
        &mut reconstructed,
        block.dc_level,
        &block.ac_levels,
        width,
        height,
        bit_depth,
        qp,
        block.bdpcm_mode,
    );
    assert_eq!(
        actual,
        residual_sse_for_test(&source_residuals, &reconstructed)
    );
}

#[test]
fn vvc_transform_skip_quant_radius_one_matches_previous_wide_search() {
    for bits in [8u8, 10, 12] {
        let bit_depth = SampleBitDepth::new(bits).expect("valid bit depth");
        let max_residual = (1i32 << bits) - 1;
        let sampled_step = 1usize << usize::from(bits.saturating_sub(8));
        for qp in 0..=63 {
            for residual in (-max_residual..=max_residual).step_by(sampled_step) {
                assert_eq!(
                    quantize_vvc_transform_skip_level(residual as i16, bit_depth, qp),
                    quantize_vvc_transform_skip_level_with_radius(
                        residual as i16,
                        bit_depth,
                        qp,
                        2,
                    ),
                    "bits={bits} qp={qp} residual={residual}"
                );
            }
            for residual in -64..=64 {
                assert_eq!(
                    quantize_vvc_transform_skip_level(residual, bit_depth, qp),
                    quantize_vvc_transform_skip_level_with_radius(residual, bit_depth, qp, 2),
                    "bits={bits} qp={qp} residual={residual}"
                );
            }
            for residual in [
                -max_residual,
                -max_residual + 1,
                -1,
                0,
                1,
                max_residual - 1,
                max_residual,
            ] {
                assert_eq!(
                    quantize_vvc_transform_skip_level(residual as i16, bit_depth, qp),
                    quantize_vvc_transform_skip_level_with_radius(
                        residual as i16,
                        bit_depth,
                        qp,
                        2,
                    ),
                    "bits={bits} qp={qp} residual={residual}"
                );
            }
        }
    }
}

fn residual_sse_for_test(source: &[i16], reconstructed: &[i16]) -> u64 {
    source
        .iter()
        .zip(reconstructed.iter())
        .map(|(source, reconstructed)| {
            let diff = i64::from(*source) - i64::from(*reconstructed);
            (diff * diff) as u64
        })
        .sum()
}

#[test]
fn vvc_luma_residual_tool_selection_is_rate_aware() {
    let best = VvcScoredSelectedLumaResidual {
        residual: VvcSelectedLumaResidual {
            block: VvcFinalizedResidualBlock {
                dc_level: 0,
                ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
                has_ac: false,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            },
            mts_index: 0,
        },
        score: VvcResidualBlockScore {
            distortion: 1_000,
            rate_cost: 0,
        },
    };
    let candidate = VvcScoredSelectedLumaResidual {
        residual: VvcSelectedLumaResidual {
            block: VvcFinalizedResidualBlock {
                dc_level: 1,
                ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
                has_ac: false,
                transform_skip: false,
                bdpcm_mode: VvcBdpcmMode::None,
            },
            mts_index: 2,
        },
        score: VvcResidualBlockScore {
            distortion: 700,
            rate_cost: 1_000,
        },
    };

    assert!(!candidate.selects_over(best));
    assert!(VvcScoredSelectedLumaResidual {
        score: VvcResidualBlockScore {
            distortion: 700,
            rate_cost: 20,
        },
        ..candidate
    }
    .selects_over(best));
    assert!(VvcScoredSelectedLumaResidual {
        score: VvcResidualBlockScore {
            distortion: 0,
            rate_cost: 1_000,
        },
        ..candidate
    }
    .selects_over(best));
    assert!(!VvcScoredSelectedLumaResidual {
        score: VvcResidualBlockScore {
            distortion: 1,
            rate_cost: 0,
        },
        ..candidate
    }
    .selects_over(VvcScoredSelectedLumaResidual {
        score: VvcResidualBlockScore {
            distortion: 0,
            rate_cost: 1_000,
        },
        ..best
    }));
    assert!(!VvcScoredSelectedLumaResidual {
        score: VvcResidualBlockScore {
            distortion: 1_100,
            rate_cost: 0,
        },
        ..best
    }
    .selects_over(best));
}

#[test]
fn vvc_luma_mrl_selection_is_rate_aware() {
    let best = VvcLumaMrlCandidate {
        distortion: 1_000,
        rate_cost: 0,
        residual: None,
    };
    let candidate = VvcLumaMrlCandidate {
        distortion: 700,
        rate_cost: 20,
        residual: None,
    };

    assert!(candidate.selects_over(best));
    assert!(!VvcLumaMrlCandidate {
        distortion: 700,
        rate_cost: 1_000,
        residual: None,
    }
    .selects_over(best));
    assert!(!VvcLumaMrlCandidate {
        distortion: 1_100,
        rate_cost: 0,
        residual: None,
    }
    .selects_over(best));
}

#[test]
fn vvc_chroma_quality_gate_can_spend_bits_for_lower_distortion() {
    let best = VvcChromaQuantizedResidualScore {
        distortion: 1_000,
        rate_cost: 0,
    };
    let candidate = VvcChromaQuantizedResidualScore {
        distortion: 700,
        rate_cost: 20,
    };

    assert!(candidate.selects_over(best));
    assert!(!VvcChromaQuantizedResidualScore {
        distortion: 1_100,
        rate_cost: 0,
    }
    .selects_over(best));
}

#[test]
fn vvc_luma_mts_selection_rejects_dc_only_explicit_candidate() {
    let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
    let qp = 19;
    let residuals = [
        0, 0, -1, -1, -1, -1, -1, -1, //
        0, -1, -1, -1, -1, -1, -1, -1, //
        -1, -1, -1, -1, -1, -1, -1, -1, //
        -1, -1, -1, -1, -1, -1, -2, -2, //
        -1, -1, -1, -1, -1, -2, -2, -2, //
        -1, -1, -1, -1, -2, -2, -2, -2, //
        -1, -1, -1, -1, -2, -2, -2, -2, //
        -1, -1, -1, -2, -2, -2, -2, -2,
    ];
    let quant_table = VvcTransformSkipQuantTable::new(bit_depth, qp);
    #[cfg(feature = "vvc-stats")]
    let mut stats = VvcIntraSearchStats::default();
    #[cfg(not(feature = "vvc-stats"))]
    let mut stats = VvcIntraSearchStats;
    let mut scratch = VvcInverseTransformScratch::default();
    let mut reconstructed = Vec::new();

    let base = finalize_vvc_luma_residual_block(
        VvcTuResidualCodingMode::Transformed,
        0,
        &residuals,
        8,
        8,
        bit_depth,
        qp,
        &quant_table,
        VvcLumaResidualQuantizationSearch::Full,
        &mut stats,
        &mut scratch,
        &mut reconstructed,
    );
    let explicit_mts = finalize_vvc_luma_residual_block(
        VvcTuResidualCodingMode::Transformed,
        2,
        &residuals,
        8,
        8,
        bit_depth,
        qp,
        &quant_table,
        VvcLumaResidualQuantizationSearch::Full,
        &mut stats,
        &mut scratch,
        &mut reconstructed,
    );
    assert!(base.has_ac);
    assert_ne!(explicit_mts.dc_level, 0);
    assert!(!explicit_mts.has_ac);
    assert!(!vvc_luma_explicit_mts_candidate_is_signalable(explicit_mts));

    let selected = select_vvc_scored_luma_residual_block_with_mts(
        VvcTuResidualCodingMode::Transformed,
        2,
        &residuals,
        8,
        8,
        bit_depth,
        qp,
        &quant_table,
        true,
        VvcLumaResidualQuantizationSearch::Full,
        &mut stats,
        &mut scratch,
        &mut reconstructed,
    );
    assert_eq!(selected.residual.mts_index, 0);
}

#[test]
fn vvc_luma_mts_search_is_gated_to_supported_lossy_blocks() {
    assert!(!vvc_luma_mts_selection_allowed(
        VvcTuResidualCodingMode::Transformed,
        0,
        8,
        8,
        super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
        true,
    ));
    assert!(!vvc_luma_mts_selection_allowed(
        VvcTuResidualCodingMode::Transformed,
        5,
        4,
        4,
        super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
        true,
    ));
    assert!(!vvc_luma_mts_selection_allowed(
        VvcTuResidualCodingMode::TransformSkip,
        0,
        8,
        8,
        super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
        true,
    ));
    assert!(!vvc_luma_mts_selection_allowed(
        VvcTuResidualCodingMode::Transformed,
        1,
        8,
        8,
        super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
        true,
    ));
    assert!(!vvc_luma_mts_selection_allowed(
        VvcTuResidualCodingMode::Transformed,
        0,
        16,
        8,
        super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
        true,
    ));
    assert!(!vvc_luma_mts_selection_allowed(
        VvcTuResidualCodingMode::Transformed,
        0,
        8,
        8,
        0,
        true,
    ));
    assert!(!vvc_luma_mts_selection_allowed(
        VvcTuResidualCodingMode::Transformed,
        0,
        8,
        8,
        super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
        false,
    ));
}
