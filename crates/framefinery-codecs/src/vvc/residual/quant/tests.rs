use super::*;

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
fn vvc_chroma_prediction_score_matches_materialized_residual_score() {
    let mut frame = sampled_luma_frame(4, 4, vec![0; 16]);
    frame.cb = vec![100, 112, 124, 136];
    frame.cr = vec![130, 118, 106, 94];
    let predicted_cb = vec![96, 116, 120, 140];
    let predicted_cr = vec![128, 120, 108, 96];
    let mut cb_residuals = Vec::new();
    let mut cr_residuals = Vec::new();
    residual_chroma_tu_at_into(
        &mut cb_residuals,
        &frame.cb,
        frame.geometry,
        frame.format,
        0,
        0,
        2,
        2,
        &predicted_cb,
    );
    residual_chroma_tu_at_into(
        &mut cr_residuals,
        &frame.cr,
        frame.geometry,
        frame.format,
        0,
        0,
        2,
        2,
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
fn vvc_lossless_speed_luma_planar_search_keeps_lossy_candidate() {
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
    assert!(vvc_luma_lossless_speed_evaluates_planar(
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
fn vvc_lossless_speed_chroma_rd_shortlist_keeps_top_two_lossy_winners() {
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
    let best = VvcScoredLumaResidualBlock {
        block: VvcFinalizedResidualBlock {
            dc_level: 0,
            ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
            has_ac: false,
            transform_skip: false,
            bdpcm_mode: VvcBdpcmMode::None,
        },
        mts_index: 0,
        score: VvcResidualBlockScore {
            distortion: 1_000,
            rate_cost: 0,
        },
    };
    let candidate = VvcScoredLumaResidualBlock {
        block: VvcFinalizedResidualBlock {
            dc_level: 1,
            ac_levels: [0; VVC_LUMA_AC_COEFFS_PER_TU],
            has_ac: false,
            transform_skip: false,
            bdpcm_mode: VvcBdpcmMode::None,
        },
        mts_index: 2,
        score: VvcResidualBlockScore {
            distortion: 700,
            rate_cost: 1_000,
        },
    };

    assert!(!candidate.selects_over(best));
    assert!(VvcScoredLumaResidualBlock {
        score: VvcResidualBlockScore {
            distortion: 700,
            rate_cost: 20,
        },
        ..candidate
    }
    .selects_over(best));
    assert!(VvcScoredLumaResidualBlock {
        score: VvcResidualBlockScore {
            distortion: 0,
            rate_cost: 1_000,
        },
        ..candidate
    }
    .selects_over(best));
    assert!(!VvcScoredLumaResidualBlock {
        score: VvcResidualBlockScore {
            distortion: 1,
            rate_cost: 0,
        },
        ..candidate
    }
    .selects_over(VvcScoredLumaResidualBlock {
        score: VvcResidualBlockScore {
            distortion: 0,
            rate_cost: 1_000,
        },
        ..best
    }));
    assert!(!VvcScoredLumaResidualBlock {
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
    assert_eq!(selected.mts_index, 0);
}

#[test]
fn vvc_luma_mts_search_is_gated_to_supported_lossy_blocks() {
    assert!(vvc_luma_mts_selection_allowed(
        VvcTuResidualCodingMode::Transformed,
        0,
        8,
        8,
        super::super::VVC_DEFAULT_LOSSY_LUMA_QP,
        true,
    ));
    assert!(vvc_luma_mts_selection_allowed(
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
