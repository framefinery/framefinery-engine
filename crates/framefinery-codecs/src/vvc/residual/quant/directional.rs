const VVC_LUMA_DIRECTIONAL_SEARCH_CANDIDATE_CAPACITY: usize = 65;
const VVC_LUMA_DEFAULT_DIRECTIONAL_SEEDS: [u8; 9] = [18, 50, 34, 10, 26, 42, 58, 2, 66];
const VVC_LUMA_LOSSY_FALLBACK_DIRECTIONAL_SEEDS: [u8; 5] = [18, 50, 34, 2, 66];
const VVC_LUMA_CARDINAL_DIRECTIONAL_SEEDS: [u8; 3] = [18, 50, 34];
const VVC_LUMA_NEARBY_DIRECTIONAL_OFFSETS: [i16; 7] = [0, -1, 1, -2, 2, -4, 4];
const VVC_LUMA_FAST_DIRECTIONAL_OFFSETS: [i16; 5] = [0, -1, 1, -2, 2];
const VVC_LUMA_AGGRESSIVE_DIRECTIONAL_OFFSETS: [i16; 3] = [0, -1, 1];
const VVC_LUMA_MODE_CELL_SIZE: usize = 4;

#[derive(Debug, Clone, Copy)]
struct VvcLumaDirectionalSearchCandidates {
    modes: [VvcIntraPredictionMode; VVC_LUMA_DIRECTIONAL_SEARCH_CANDIDATE_CAPACITY],
    seen: [bool; 67],
    count: usize,
}

impl VvcLumaDirectionalSearchCandidates {
    fn new() -> Self {
        Self {
            modes: [VvcIntraPredictionMode::Horizontal;
                VVC_LUMA_DIRECTIONAL_SEARCH_CANDIDATE_CAPACITY],
            seen: [false; 67],
            count: 0,
        }
    }

    fn add_mode(&mut self, mode: VvcIntraPredictionMode) {
        let index = usize::from(mode.luma_mode_index());
        debug_assert!((2..=66).contains(&index));
        if self.seen[index] {
            return;
        }
        assert!(self.count < self.modes.len());
        self.modes[self.count] = mode;
        self.seen[index] = true;
        self.count += 1;
    }

    fn add_index(&mut self, index: u8) {
        if (2..=66).contains(&index) {
            self.add_mode(vvc_luma_intra_mode_from_index(index));
        }
    }

    fn add_family(&mut self, center: u8) {
        self.add_family_with_offsets(center, &VVC_LUMA_NEARBY_DIRECTIONAL_OFFSETS);
    }

    fn add_compact_family(&mut self, center: u8) {
        self.add_family_with_offsets(center, &VVC_LUMA_FAST_DIRECTIONAL_OFFSETS);
    }

    fn add_aggressive_family(&mut self, center: u8) {
        self.add_family_with_offsets(center, &VVC_LUMA_AGGRESSIVE_DIRECTIONAL_OFFSETS);
    }

    fn add_family_with_offsets(&mut self, center: u8, offsets: &[i16]) {
        for offset in offsets {
            let index = i16::from(center) + offset;
            if (2..=66).contains(&index) {
                self.add_index(index as u8);
            }
        }
    }

    fn add_refinement(&mut self, center: u8, fast_search: VvcFastSearch) {
        match fast_search {
            VvcFastSearch::Off | VvcFastSearch::Conservative => self.add_family(center),
            VvcFastSearch::Moderate | VvcFastSearch::LosslessSpeed => {
                self.add_compact_family(center);
            }
            VvcFastSearch::Aggressive => self.add_aggressive_family(center),
        }
    }

    fn count(&self) -> usize {
        self.count
    }

    fn iter(&self) -> impl Iterator<Item = VvcIntraPredictionMode> + '_ {
        self.modes[..self.count].iter().copied()
    }

    fn iter_from(&self, start: usize) -> impl Iterator<Item = VvcIntraPredictionMode> + '_ {
        self.modes[start..self.count].iter().copied()
    }
}

#[derive(Debug, Clone)]
pub(in crate::vvc) struct VvcLumaModeSearchState {
    width: usize,
    height: usize,
    cell_cols: usize,
    valid: Vec<bool>,
    modes: Vec<VvcIntraPredictionMode>,
}

impl VvcLumaModeSearchState {
    pub(in crate::vvc) fn new_for_geometry(geometry: VvcVideoGeometry) -> Self {
        let width = geometry.coded_width();
        let height = geometry.coded_height();
        let cell_cols = width.div_ceil(VVC_LUMA_MODE_CELL_SIZE);
        let cell_rows = height.div_ceil(VVC_LUMA_MODE_CELL_SIZE);
        let cell_count = cell_cols.saturating_mul(cell_rows);
        Self {
            width,
            height,
            cell_cols,
            valid: vec![false; cell_count],
            modes: vec![VvcIntraPredictionMode::Planar; cell_count],
        }
    }

    pub(in crate::vvc) fn clear(&mut self) {
        self.valid.fill(false);
    }

    fn left_of(&self, node: VvcCodingTreeNode) -> Option<VvcIntraPredictionMode> {
        let x = node.x.checked_sub(1)?;
        let y = node.y.saturating_add(node.height).saturating_sub(1);
        self.mode_at(x, y)
    }

    fn above_of(&self, node: VvcCodingTreeNode) -> Option<VvcIntraPredictionMode> {
        let y = node.y.checked_sub(1)?;
        if node.y % VVC_CTU_SIZE as u16 == 0 {
            return None;
        }
        let x = node.x.saturating_add(node.width).saturating_sub(1);
        self.mode_at(x, y)
    }

    fn mode_at(&self, x: u16, y: u16) -> Option<VvcIntraPredictionMode> {
        let x = usize::from(x);
        let y = usize::from(y);
        if x >= self.width || y >= self.height {
            return None;
        }
        let cell_x = x / VVC_LUMA_MODE_CELL_SIZE;
        let cell_y = y / VVC_LUMA_MODE_CELL_SIZE;
        let idx = cell_y * self.cell_cols + cell_x;
        self.valid[idx].then_some(self.modes[idx])
    }

    fn mark_node(&mut self, node: VvcCodingTreeNode, mode: VvcIntraPredictionMode) {
        let start_x = usize::from(node.x).min(self.width);
        let start_y = usize::from(node.y).min(self.height);
        let end_x = usize::from(node.x)
            .saturating_add(usize::from(node.width))
            .min(self.width);
        let end_y = usize::from(node.y)
            .saturating_add(usize::from(node.height))
            .min(self.height);
        if end_x <= start_x || end_y <= start_y {
            return;
        }
        let start_cell_x = usize::from(node.x) / VVC_LUMA_MODE_CELL_SIZE;
        let start_cell_y = usize::from(node.y) / VVC_LUMA_MODE_CELL_SIZE;
        let end_cell_x = end_x.div_ceil(VVC_LUMA_MODE_CELL_SIZE);
        let end_cell_y = end_y.div_ceil(VVC_LUMA_MODE_CELL_SIZE);
        for cell_y in start_cell_y..end_cell_y {
            for cell_x in start_cell_x..end_cell_x {
                let idx = cell_y * self.cell_cols + cell_x;
                self.valid[idx] = true;
                self.modes[idx] = mode;
            }
        }
    }

    fn co_located_mode_for_chroma_node(
        &self,
        chroma_node: VvcCodingTreeNode,
    ) -> VvcIntraPredictionMode {
        let max_x = self.width.saturating_sub(1).min(usize::from(u16::MAX)) as u16;
        let max_y = self.height.saturating_sub(1).min(usize::from(u16::MAX)) as u16;
        let ref_x = chroma_node
            .x
            .saturating_add(chroma_node.width >> 1)
            .min(max_x);
        let ref_y = chroma_node
            .y
            .saturating_add(chroma_node.height >> 1)
            .min(max_y);
        self.mode_at(ref_x, ref_y)
            .unwrap_or(VvcIntraPredictionMode::Dc)
    }
}

fn vvc_luma_directional_search_candidates(
    policy: VvcResidualCodingPolicy,
    source_frame: &VvcSampledFrame,
    mode_state: &VvcLumaModeSearchState,
    global_node: VvcCodingTreeNode,
) -> VvcLumaDirectionalSearchCandidates {
    if policy.fast_search() != VvcFastSearch::Off {
        return vvc_luma_fast_directional_search_candidates(
            policy,
            source_frame,
            mode_state,
            global_node,
        );
    }
    vvc_luma_default_directional_search_candidates(policy, source_frame, mode_state, global_node)
}

fn vvc_luma_default_directional_search_candidates(
    policy: VvcResidualCodingPolicy,
    source_frame: &VvcSampledFrame,
    mode_state: &VvcLumaModeSearchState,
    global_node: VvcCodingTreeNode,
) -> VvcLumaDirectionalSearchCandidates {
    let mut candidates = VvcLumaDirectionalSearchCandidates::new();
    if policy.residual_mode() == VvcResidualCodingMode::Lossy {
        if vvc_source_luma_directional_seed_allowed(policy, global_node) {
            if let Some(index) = vvc_source_luma_directional_seed(source_frame, global_node) {
                candidates.add_family(index);
            }
        }
        for mode in [
            mode_state.left_of(global_node),
            mode_state.above_of(global_node),
        ]
        .into_iter()
        .flatten()
        {
            candidates.add_index(mode.luma_mode_index());
        }
        for index in VVC_LUMA_LOSSY_FALLBACK_DIRECTIONAL_SEEDS {
            candidates.add_index(index);
        }
    } else {
        for index in VVC_LUMA_DEFAULT_DIRECTIONAL_SEEDS {
            candidates.add_index(index);
        }
        for mode in [
            mode_state.left_of(global_node),
            mode_state.above_of(global_node),
        ]
        .into_iter()
        .flatten()
        {
            candidates.add_family(mode.luma_mode_index());
        }
        if let Some(index) = vvc_source_luma_directional_seed(source_frame, global_node) {
            candidates.add_family(index);
        }
    }
    candidates
}

fn vvc_luma_fast_directional_search_candidates(
    policy: VvcResidualCodingPolicy,
    source_frame: &VvcSampledFrame,
    mode_state: &VvcLumaModeSearchState,
    global_node: VvcCodingTreeNode,
) -> VvcLumaDirectionalSearchCandidates {
    let mut candidates = VvcLumaDirectionalSearchCandidates::new();
    let fast_search = policy.fast_search();
    let left = mode_state.left_of(global_node);
    let above = mode_state.above_of(global_node);
    let source_seed = vvc_source_luma_directional_seed_allowed(policy, global_node)
        .then(|| vvc_source_luma_directional_seed(source_frame, global_node))
        .flatten();

    match fast_search {
        VvcFastSearch::Off => unreachable!("off fast-search uses the default candidate path"),
        VvcFastSearch::Conservative => {
            add_vvc_luma_spatial_directional_candidates(
                &mut candidates,
                left,
                above,
                fast_search,
                policy.residual_mode(),
            );
            if let Some(index) = source_seed {
                candidates.add_compact_family(index);
            }
            for index in VVC_LUMA_LOSSY_FALLBACK_DIRECTIONAL_SEEDS {
                candidates.add_index(index);
            }
        }
        VvcFastSearch::Moderate | VvcFastSearch::LosslessSpeed => {
            if let Some(index) = source_seed {
                if fast_search == VvcFastSearch::LosslessSpeed {
                    candidates.add_index(index);
                } else {
                    candidates.add_compact_family(index);
                }
            }
            add_vvc_luma_spatial_directional_candidates(
                &mut candidates,
                left,
                above,
                fast_search,
                policy.residual_mode(),
            );
            if fast_search != VvcFastSearch::LosslessSpeed || candidates.count() == 0 {
                for index in VVC_LUMA_CARDINAL_DIRECTIONAL_SEEDS {
                    candidates.add_index(index);
                }
            }
        }
        VvcFastSearch::Aggressive => {
            if let Some(index) = source_seed {
                candidates.add_aggressive_family(index);
            }
            add_vvc_luma_spatial_directional_candidates(
                &mut candidates,
                left,
                above,
                fast_search,
                policy.residual_mode(),
            );
            if candidates.count() == 0 {
                for index in VVC_LUMA_CARDINAL_DIRECTIONAL_SEEDS {
                    candidates.add_index(index);
                }
            }
        }
    }

    candidates
}

fn add_vvc_luma_spatial_directional_candidates(
    candidates: &mut VvcLumaDirectionalSearchCandidates,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    fast_search: VvcFastSearch,
    residual_mode: VvcResidualCodingMode,
) {
    if let Some(consensus) = vvc_luma_spatial_consensus_directional_seed(left, above) {
        if fast_search == VvcFastSearch::LosslessSpeed {
            candidates.add_index(consensus);
        } else {
            add_vvc_luma_policy_directional_family(candidates, consensus, fast_search);
        }
        return;
    }
    for mode in [left, above].into_iter().flatten() {
        let Some(index) = vvc_luma_directional_index(mode) else {
            continue;
        };
        match fast_search {
            VvcFastSearch::Off => candidates.add_family(index),
            VvcFastSearch::Conservative if residual_mode == VvcResidualCodingMode::Lossless => {
                candidates.add_family(index);
            }
            VvcFastSearch::LosslessSpeed => {
                candidates.add_index(index);
            }
            VvcFastSearch::Conservative | VvcFastSearch::Moderate => {
                candidates.add_compact_family(index);
            }
            VvcFastSearch::Aggressive => candidates.add_index(index),
        }
    }
}

fn add_vvc_luma_policy_directional_family(
    candidates: &mut VvcLumaDirectionalSearchCandidates,
    index: u8,
    fast_search: VvcFastSearch,
) {
    match fast_search {
        VvcFastSearch::Off | VvcFastSearch::Conservative => candidates.add_family(index),
        VvcFastSearch::Moderate | VvcFastSearch::LosslessSpeed => {
            candidates.add_compact_family(index);
        }
        VvcFastSearch::Aggressive => candidates.add_aggressive_family(index),
    }
}

fn vvc_luma_spatial_consensus_directional_seed(
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> Option<u8> {
    let left = vvc_luma_directional_index(left?)?;
    let above = vvc_luma_directional_index(above?)?;
    (left.abs_diff(above) <= 4).then_some(((u16::from(left) + u16::from(above)) / 2) as u8)
}

fn vvc_luma_directional_index(mode: VvcIntraPredictionMode) -> Option<u8> {
    let index = mode.luma_mode_index();
    (2..=66).contains(&index).then_some(index)
}

fn vvc_source_luma_directional_seed_allowed(
    policy: VvcResidualCodingPolicy,
    node: VvcCodingTreeNode,
) -> bool {
    match policy.fast_search() {
        VvcFastSearch::Off | VvcFastSearch::Conservative => {
            policy.residual_mode() == VvcResidualCodingMode::Lossless
                || (node.width >= 8 && node.height >= 8)
        }
        VvcFastSearch::Moderate | VvcFastSearch::Aggressive => {
            node.width >= 8 && node.height >= 8
        }
        VvcFastSearch::LosslessSpeed => {
            policy.residual_mode() == VvcResidualCodingMode::Lossy
                && node.width >= 8
                && node.height >= 8
        }
    }
}

fn vvc_source_luma_directional_seed(
    source_frame: &VvcSampledFrame,
    node: VvcCodingTreeNode,
) -> Option<u8> {
    let x0 = usize::from(node.x);
    let y0 = usize::from(node.y);
    let x1 = x0
        .saturating_add(usize::from(node.width))
        .min(source_frame.geometry.width);
    let y1 = y0
        .saturating_add(usize::from(node.height))
        .min(source_frame.geometry.height);
    if x1 <= x0 + 1 || y1 <= y0 + 1 {
        return None;
    }

    let stride = source_frame.geometry.width;
    let mut gxx = 0i64;
    let mut gyy = 0i64;
    let mut gxy = 0i64;
    for y in (y0 + 1)..y1 {
        for x in (x0 + 1)..x1 {
            let sample = i64::from(source_frame.luma[y * stride + x]);
            let dx = sample - i64::from(source_frame.luma[y * stride + x - 1]);
            let dy = sample - i64::from(source_frame.luma[(y - 1) * stride + x]);
            gxx += dx * dx;
            gyy += dy * dy;
            gxy += dx * dy;
        }
    }
    if gxx == 0 && gyy == 0 {
        return None;
    }

    let gradient_angle = 0.5 * (2.0 * gxy as f64).atan2((gxx - gyy) as f64);
    let mut edge_angle = gradient_angle + std::f64::consts::FRAC_PI_2;
    while edge_angle < 0.0 {
        edge_angle += std::f64::consts::PI;
    }
    while edge_angle >= std::f64::consts::PI {
        edge_angle -= std::f64::consts::PI;
    }
    let folded_edge_angle = if edge_angle > std::f64::consts::FRAC_PI_2 {
        std::f64::consts::PI - edge_angle
    } else {
        edge_angle
    };
    let mode_offset = (folded_edge_angle / std::f64::consts::FRAC_PI_2 * 32.0).round() as i16;
    Some((18 + mode_offset).clamp(2, 66) as u8)
}

fn luma_prediction_mode_selection_score(
    metric: VvcResidualScoreMetric,
    source_frame: &VvcSampledFrame,
    node: VvcCodingTreeNode,
    predicted: &[VvcSample],
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    mode: VvcIntraPredictionMode,
) -> u64 {
    const SYNTAX_TIE_BREAKER_SCALE: u64 = 64;
    luma_prediction_residual_score(metric, source_frame, node, predicted)
        .saturating_mul(SYNTAX_TIE_BREAKER_SCALE)
        .saturating_add(u64::from(vvc_luma_intra_mode_syntax_bin_count(
            mode, left, above,
        )))
}

fn luma_residual_mode_selection_score(
    metric: VvcResidualScoreMetric,
    residuals: &[i16],
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
    mode: VvcIntraPredictionMode,
) -> u64 {
    const SYNTAX_TIE_BREAKER_SCALE: u64 = 64;
    residual_mode_selection_score(metric, residuals)
        .saturating_mul(SYNTAX_TIE_BREAKER_SCALE)
        .saturating_add(u64::from(vvc_luma_intra_mode_syntax_bin_count(
            mode, left, above,
        )))
}

fn luma_prediction_residual_score(
    metric: VvcResidualScoreMetric,
    frame: &VvcSampledFrame,
    node: VvcCodingTreeNode,
    predicted: &[VvcSample],
) -> u64 {
    let origin_x = usize::from(node.x);
    let origin_y = usize::from(node.y);
    let width = usize::from(node.width);
    let height = usize::from(node.height);
    debug_assert_eq!(predicted.len(), width * height);
    let copy_width = width.min(frame.geometry.width.saturating_sub(origin_x));
    let copy_height = height.min(frame.geometry.height.saturating_sub(origin_y));
    prediction_plane_residual_score(
        metric,
        &frame.luma,
        frame.geometry.width,
        origin_x,
        origin_y,
        width,
        height,
        copy_width,
        copy_height,
        0,
        predicted,
    )
}

fn chroma_prediction_mode_selection_score(
    metric: VvcResidualScoreMetric,
    source_frame: &VvcSampledFrame,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
    mode: VvcChromaIntraPredictionMode,
    cclm_enabled: bool,
    syntax_tie_breaker_enabled: bool,
) -> u64 {
    const SYNTAX_TIE_BREAKER_SCALE: u64 = 64;
    let residual_score = chroma_prediction_residual_score(
        metric,
        source_frame,
        origin_x,
        origin_y,
        width,
        height,
        predicted_cb,
        predicted_cr,
    );
    let syntax_tie_breaker = if syntax_tie_breaker_enabled {
        vvc_chroma_intra_mode_syntax_bin_count(mode, cclm_enabled)
    } else {
        0
    };
    residual_score
        .saturating_mul(SYNTAX_TIE_BREAKER_SCALE)
        .saturating_add(u64::from(syntax_tie_breaker))
}

fn chroma_residual_mode_selection_score(
    metric: VvcResidualScoreMetric,
    cb_residuals: &[i16],
    cr_residuals: &[i16],
    mode: VvcChromaIntraPredictionMode,
    cclm_enabled: bool,
    syntax_tie_breaker_enabled: bool,
) -> u64 {
    const SYNTAX_TIE_BREAKER_SCALE: u64 = 64;
    let residual_score = residual_mode_selection_score(metric, cb_residuals)
        .saturating_add(residual_mode_selection_score(metric, cr_residuals));
    let syntax_tie_breaker = if syntax_tie_breaker_enabled {
        vvc_chroma_intra_mode_syntax_bin_count(mode, cclm_enabled)
    } else {
        0
    };
    residual_score
        .saturating_mul(SYNTAX_TIE_BREAKER_SCALE)
        .saturating_add(u64::from(syntax_tie_breaker))
}

fn chroma_prediction_residual_score(
    metric: VvcResidualScoreMetric,
    frame: &VvcSampledFrame,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted_cb: &[VvcSample],
    predicted_cr: &[VvcSample],
) -> u64 {
    chroma_plane_prediction_residual_score(
        metric,
        &frame.cb,
        frame.geometry,
        frame.format,
        origin_x,
        origin_y,
        width,
        height,
        predicted_cb,
    )
    .saturating_add(chroma_plane_prediction_residual_score(
        metric,
        &frame.cr,
        frame.geometry,
        frame.format,
        origin_x,
        origin_y,
        width,
        height,
        predicted_cr,
    ))
}

fn chroma_plane_prediction_residual_score(
    metric: VvcResidualScoreMetric,
    samples: &[VvcSample],
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    predicted: &[VvcSample],
) -> u64 {
    debug_assert_eq!(predicted.len(), width * height);
    let chroma_width = geometry.width / chroma_subsample_x(format.chroma_sampling);
    let chroma_height = geometry.height / chroma_subsample_y(format.chroma_sampling);
    let neutral = vvc_neutral_sample(format.bit_depth);
    let copy_width = width.min(chroma_width.saturating_sub(origin_x));
    let copy_height = height.min(chroma_height.saturating_sub(origin_y));
    prediction_plane_residual_score(
        metric,
        samples,
        chroma_width,
        origin_x,
        origin_y,
        width,
        height,
        copy_width,
        copy_height,
        neutral,
        predicted,
    )
}

fn prediction_plane_residual_score(
    metric: VvcResidualScoreMetric,
    samples: &[VvcSample],
    stride: usize,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    copy_width: usize,
    copy_height: usize,
    padding_sample: VvcSample,
    predicted: &[VvcSample],
) -> u64 {
    match metric {
        VvcResidualScoreMetric::Sad => prediction_plane_residual_sad(
            samples,
            stride,
            origin_x,
            origin_y,
            width,
            height,
            copy_width,
            copy_height,
            padding_sample,
            predicted,
        ),
        VvcResidualScoreMetric::Sse => prediction_plane_residual_sse(
            samples,
            stride,
            origin_x,
            origin_y,
            width,
            height,
            copy_width,
            copy_height,
            padding_sample,
            predicted,
        ),
    }
}

fn prediction_plane_residual_sad(
    samples: &[VvcSample],
    stride: usize,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    copy_width: usize,
    copy_height: usize,
    padding_sample: VvcSample,
    predicted: &[VvcSample],
) -> u64 {
    let mut score = 0u64;
    for y in 0..copy_height {
        let dst = y * width;
        if copy_width > 0 {
            let src = (origin_y + y) * stride + origin_x;
            for (sample, predicted) in samples[src..src + copy_width]
                .iter()
                .zip(&predicted[dst..dst + copy_width])
            {
                score += vvc_sample_delta_sad_score(*sample, *predicted);
            }
        }
        for predicted in &predicted[dst + copy_width..dst + width] {
            score += vvc_sample_delta_sad_score(padding_sample, *predicted);
        }
    }
    for y in copy_height..height {
        let dst = y * width;
        for predicted in &predicted[dst..dst + width] {
            score += vvc_sample_delta_sad_score(padding_sample, *predicted);
        }
    }
    score
}

fn prediction_plane_residual_sse(
    samples: &[VvcSample],
    stride: usize,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
    copy_width: usize,
    copy_height: usize,
    padding_sample: VvcSample,
    predicted: &[VvcSample],
) -> u64 {
    let mut score = 0u64;
    for y in 0..copy_height {
        let dst = y * width;
        if copy_width > 0 {
            let src = (origin_y + y) * stride + origin_x;
            for (sample, predicted) in samples[src..src + copy_width]
                .iter()
                .zip(&predicted[dst..dst + copy_width])
            {
                score += vvc_sample_delta_sse_score(*sample, *predicted);
            }
        }
        for predicted in &predicted[dst + copy_width..dst + width] {
            score += vvc_sample_delta_sse_score(padding_sample, *predicted);
        }
    }
    for y in copy_height..height {
        let dst = y * width;
        for predicted in &predicted[dst..dst + width] {
            score += vvc_sample_delta_sse_score(padding_sample, *predicted);
        }
    }
    score
}

fn vvc_sample_delta_sad_score(sample: VvcSample, predicted: VvcSample) -> u64 {
    let residual = vvc_sample_delta_i16(sample, predicted);
    u64::from(residual.unsigned_abs())
}

fn vvc_sample_delta_sse_score(sample: VvcSample, predicted: VvcSample) -> u64 {
    let residual = i64::from(vvc_sample_delta_i16(sample, predicted));
    (residual * residual) as u64
}

fn residual_sad(residuals: &[i16]) -> u64 {
    residuals
        .iter()
        .map(|residual| u64::from(residual.unsigned_abs()))
        .sum()
}

fn vvc_luma_exact_min_syntax_mode_search_done(best_score: u64) -> bool {
    best_score <= u64::from(VVC_LUMA_MIN_INTRA_MODE_SYNTAX_BINS)
}

fn vvc_chroma_lossy_exact_mode_search_done(
    syntax_tie_breaker_enabled: bool,
    best_score: u64,
) -> bool {
    !syntax_tie_breaker_enabled && best_score == 0
}

fn residual_mode_selection_score(metric: VvcResidualScoreMetric, residuals: &[i16]) -> u64 {
    match metric {
        VvcResidualScoreMetric::Sad => residual_sad(residuals),
        VvcResidualScoreMetric::Sse => residual_sse(residuals),
    }
}

fn residual_sse(residuals: &[i16]) -> u64 {
    residuals
        .iter()
        .map(|residual| {
            let residual = i64::from(*residual);
            (residual * residual) as u64
        })
        .sum()
}
