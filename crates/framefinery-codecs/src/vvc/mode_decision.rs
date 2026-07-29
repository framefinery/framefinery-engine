#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcResidualCodingMode {
    Lossy,
    Lossless,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcTuResidualCodingMode {
    Transformed,
    TransformSkip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcBdpcmMode {
    None,
    Horizontal,
    Vertical,
}

impl VvcBdpcmMode {
    pub(in crate::vvc) const fn is_enabled(self) -> bool {
        !matches!(self, Self::None)
    }

    pub(in crate::vvc) const fn inferred_intra_mode(self) -> Option<VvcIntraPredictionMode> {
        match self {
            Self::None => None,
            Self::Horizontal => Some(VvcIntraPredictionMode::Horizontal),
            Self::Vertical => Some(VvcIntraPredictionMode::Vertical),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcResidualScoreMetric {
    Sad,
    Sse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcLumaTuCodingDecision {
    pub(in crate::vvc) residual_coding: VvcTuResidualCodingMode,
    pub(in crate::vvc) mrl_index: u8,
    pub(in crate::vvc) mts_index: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcChromaTuCodingDecision {
    pub(in crate::vvc) residual_coding: VvcTuResidualCodingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcResidualCodingPolicy {
    context: VvcResidualModeDecisionContext,
    luma_max_leaf_size: u16,
    score_metric: VvcResidualScoreMetric,
    chroma_syntax_tie_breaker: bool,
    fast_search: VvcFastSearch,
}

impl VvcResidualCodingMode {
    const fn for_encode_options(options: VvcEncodeOptions) -> Self {
        match options.lossless {
            true => Self::Lossless,
            false => Self::Lossy,
        }
    }

    fn slice_config(
        self,
        stream_format: VvcPictureFormat,
        qp: Option<u8>,
        fast_search: VvcFastSearch,
    ) -> VvcSliceSyntaxConfig {
        let mut config = VvcSliceSyntaxConfig::residual(stream_format.chroma_sampling, self);
        if self.is_lossless() {
            config.slice_qp = vvc_lossless_slice_qp(stream_format.bit_depth);
        } else {
            config.slice_qp = vvc_lossy_slice_qp(stream_format, qp, fast_search);
        }
        config
    }

    const fn picture_partitioning(self) -> VvcPicturePartitioning {
        match self {
            Self::Lossy | Self::Lossless => VvcPicturePartitioning::SingleSlice,
        }
    }

    const fn is_lossless(self) -> bool {
        matches!(self, Self::Lossless)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcIntraPredictionMode {
    Planar,
    Dc,
    Horizontal,
    Vertical,
    #[allow(dead_code)]
    Angular(u8),
}

impl VvcIntraPredictionMode {
    const fn luma_mode_index(self) -> u8 {
        match self {
            Self::Planar => 0,
            Self::Dc => 1,
            Self::Horizontal => 18,
            Self::Vertical => 50,
            Self::Angular(index) => index,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcChromaIntraPredictionMode {
    Derived,
    Explicit(VvcIntraPredictionMode),
    Cclm(VvcChromaCclmMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcChromaCclmMode {
    Linear,
    MdlmLeft,
    MdlmTop,
}

pub(in crate::vvc) const fn vvc_luma_intra_mode_from_index(index: u8) -> VvcIntraPredictionMode {
    match index {
        18 => VvcIntraPredictionMode::Horizontal,
        50 => VvcIntraPredictionMode::Vertical,
        _ => VvcIntraPredictionMode::Angular(index),
    }
}

const VVC_CHROMA_EXPLICIT_MODE_COUNT: usize = 4;
const VVC_CHROMA_VDIA_REPLACEMENT_MODE: VvcIntraPredictionMode =
    VvcIntraPredictionMode::Angular(66);

pub(in crate::vvc) fn vvc_chroma_explicit_candidates(
    co_located_luma_mode: VvcIntraPredictionMode,
) -> [VvcIntraPredictionMode; VVC_CHROMA_EXPLICIT_MODE_COUNT] {
    let mut modes = [
        VvcIntraPredictionMode::Planar,
        VvcIntraPredictionMode::Vertical,
        VvcIntraPredictionMode::Horizontal,
        VvcIntraPredictionMode::Dc,
    ];
    let luma_mode_index = co_located_luma_mode.luma_mode_index();
    let mut idx = 0;
    while idx < modes.len() {
        if modes[idx].luma_mode_index() == luma_mode_index {
            modes[idx] = VVC_CHROMA_VDIA_REPLACEMENT_MODE;
            break;
        }
        idx += 1;
    }
    modes
}

pub(in crate::vvc) fn vvc_chroma_explicit_candidate_index(
    mode: VvcIntraPredictionMode,
    co_located_luma_mode: VvcIntraPredictionMode,
) -> Option<u8> {
    let modes = vvc_chroma_explicit_candidates(co_located_luma_mode);
    modes
        .iter()
        .position(|candidate| candidate.luma_mode_index() == mode.luma_mode_index())
        .map(|index| index as u8)
}

pub(in crate::vvc) fn vvc_residual_chroma_explicit_candidate_allowed(
    mode: VvcIntraPredictionMode,
) -> bool {
    match mode {
        VvcIntraPredictionMode::Planar
        | VvcIntraPredictionMode::Horizontal
        | VvcIntraPredictionMode::Vertical => true,
        VvcIntraPredictionMode::Dc => true,
        VvcIntraPredictionMode::Angular(index) => (2..=66).contains(&index),
    }
}

pub(in crate::vvc) fn vvc_chroma_cclm_node_allowed(node: VvcCodingTreeNode) -> bool {
    // H.266 CodingUnit::checkCCLMAllowed allows CCLM on this dual-tree,
    // CTU-size subset for unsplit 64x64 chroma nodes, nodes below a root QT
    // split, root HBT 64x32 nodes, and root HBT followed by VBT.
    (node.width == 64 && node.height == 64 && node.cqt_depth == 0 && node.mtt_depth == 0)
        || node.cqt_depth > 0
        || (node.split_history[0] == VvcPartSplit::HorizontalBinary
            && node.width == 64
            && node.height == 32)
        || (node.split_history[0] == VvcPartSplit::HorizontalBinary
            && node.split_history[1] == VvcPartSplit::VerticalBinary)
}

pub(in crate::vvc) fn vvc_residual_chroma_cclm_candidate_allowed(
    context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
    geometry: VvcVideoGeometry,
) -> bool {
    let _chroma_sampling = context.chroma_sampling();
    if !vvc_chroma_cclm_node_allowed(node) {
        return false;
    }
    node.fits_visible(
        geometry.coded_width() as u16,
        geometry.coded_height() as u16,
    )
}

const VVC_LUMA_INTRA_CANDIDATE_CAPACITY: usize = 67;
const VVC_CHROMA_INTRA_CANDIDATE_CAPACITY: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcLumaIntraCandidateCost {
    mode: VvcIntraPredictionMode,
    score: u64,
}

impl VvcLumaIntraCandidateCost {
    pub(in crate::vvc) const fn new(mode: VvcIntraPredictionMode, score: u64) -> Self {
        Self { mode, score }
    }

    pub(in crate::vvc) const fn mode(self) -> VvcIntraPredictionMode {
        self.mode
    }

    pub(in crate::vvc) const fn score(self) -> u64 {
        self.score
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcLumaIntraCandidateCosts {
    candidates: [VvcLumaIntraCandidateCost; VVC_LUMA_INTRA_CANDIDATE_CAPACITY],
    count: usize,
}

impl VvcLumaIntraCandidateCosts {
    pub(in crate::vvc) const fn new(dc_score: u64) -> Self {
        Self {
            candidates: [VvcLumaIntraCandidateCost::new(VvcIntraPredictionMode::Dc, 0);
                VVC_LUMA_INTRA_CANDIDATE_CAPACITY],
            count: 1,
        }
        .with_required_candidate(VvcIntraPredictionMode::Dc, dc_score)
    }

    const fn with_required_candidate(mut self, mode: VvcIntraPredictionMode, score: u64) -> Self {
        self.candidates[self.count - 1] = VvcLumaIntraCandidateCost::new(mode, score);
        self
    }

    pub(in crate::vvc) fn with_candidate(
        mut self,
        mode: VvcIntraPredictionMode,
        score: Option<u64>,
    ) -> Self {
        if let Some(score) = score {
            assert!(self.count < self.candidates.len());
            self.candidates[self.count] = VvcLumaIntraCandidateCost::new(mode, score);
            self.count += 1;
        }
        self
    }

    pub(in crate::vvc) fn iter(self) -> impl Iterator<Item = VvcLumaIntraCandidateCost> {
        self.candidates.into_iter().take(self.count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcChromaIntraCandidateCost {
    mode: VvcChromaIntraPredictionMode,
    score: u64,
}

impl VvcChromaIntraCandidateCost {
    pub(in crate::vvc) const fn new(mode: VvcChromaIntraPredictionMode, score: u64) -> Self {
        Self { mode, score }
    }

    pub(in crate::vvc) const fn mode(self) -> VvcChromaIntraPredictionMode {
        self.mode
    }

    pub(in crate::vvc) const fn score(self) -> u64 {
        self.score
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcChromaIntraCandidateCosts {
    candidates: [VvcChromaIntraCandidateCost; VVC_CHROMA_INTRA_CANDIDATE_CAPACITY],
    count: usize,
}

impl VvcChromaIntraCandidateCosts {
    pub(in crate::vvc) const fn new(derived_score: u64) -> Self {
        Self {
            candidates: [VvcChromaIntraCandidateCost::new(VvcChromaIntraPredictionMode::Derived, 0);
                VVC_CHROMA_INTRA_CANDIDATE_CAPACITY],
            count: 1,
        }
        .with_required_candidate(VvcChromaIntraPredictionMode::Derived, derived_score)
    }

    const fn with_required_candidate(
        mut self,
        mode: VvcChromaIntraPredictionMode,
        score: u64,
    ) -> Self {
        self.candidates[self.count - 1] = VvcChromaIntraCandidateCost::new(mode, score);
        self
    }

    pub(in crate::vvc) fn with_candidate(
        mut self,
        mode: VvcChromaIntraPredictionMode,
        score: Option<u64>,
    ) -> Self {
        if let Some(score) = score {
            assert!(self.count < self.candidates.len());
            self.candidates[self.count] = VvcChromaIntraCandidateCost::new(mode, score);
            self.count += 1;
        }
        self
    }

    pub(in crate::vvc) fn iter(self) -> impl Iterator<Item = VvcChromaIntraCandidateCost> {
        self.candidates.into_iter().take(self.count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcResidualModeDecisionContext {
    format: VvcPictureFormat,
    residual_mode: VvcResidualCodingMode,
}

impl VvcResidualModeDecisionContext {
    pub(in crate::vvc) const fn new(
        format: VvcPictureFormat,
        residual_mode: VvcResidualCodingMode,
    ) -> Self {
        Self {
            format,
            residual_mode,
        }
    }

    const fn chroma_sampling(self) -> ChromaSampling {
        self.format.chroma_sampling
    }

    const fn bit_depth(self) -> SampleBitDepth {
        self.format.bit_depth
    }

    const fn is_lossless(self) -> bool {
        self.residual_mode.is_lossless()
    }

    pub(in crate::vvc) const fn residual_mode(self) -> VvcResidualCodingMode {
        self.residual_mode
    }
}

impl VvcResidualCodingPolicy {
    pub(in crate::vvc) fn new(
        format: VvcPictureFormat,
        residual_mode: VvcResidualCodingMode,
    ) -> Self {
        let context = VvcResidualModeDecisionContext::new(format, residual_mode);
        Self {
            context,
            luma_max_leaf_size: select_vvc_luma_max_leaf_size(context),
            score_metric: select_vvc_residual_score_metric(context),
            chroma_syntax_tie_breaker: select_vvc_chroma_mode_syntax_tie_breaker(context),
            fast_search: VvcFastSearch::Off,
        }
    }

    pub(in crate::vvc) const fn luma_max_leaf_size(self) -> u16 {
        self.luma_max_leaf_size
    }

    const fn with_luma_max_leaf_size(mut self, luma_max_leaf_size: u16) -> Self {
        self.luma_max_leaf_size = luma_max_leaf_size;
        self
    }

    pub(in crate::vvc) const fn with_fast_search(mut self, fast_search: VvcFastSearch) -> Self {
        self.fast_search = fast_search;
        self
    }

    pub(in crate::vvc) const fn fast_search(self) -> VvcFastSearch {
        self.fast_search
    }

    pub(in crate::vvc) const fn score_metric(self) -> VvcResidualScoreMetric {
        self.score_metric
    }

    pub(in crate::vvc) const fn chroma_syntax_tie_breaker(self) -> bool {
        self.chroma_syntax_tie_breaker
    }

    pub(in crate::vvc) const fn residual_mode(self) -> VvcResidualCodingMode {
        self.context.residual_mode()
    }

    pub(in crate::vvc) fn select_luma_intra_mode(
        self,
        node: VvcCodingTreeNode,
        costs: VvcLumaIntraCandidateCosts,
    ) -> VvcIntraPredictionMode {
        select_vvc_residual_luma_intra_mode(self.context, node, costs)
    }

    pub(in crate::vvc) fn select_chroma_intra_mode(
        self,
        node: VvcCodingTreeNode,
        costs: VvcChromaIntraCandidateCosts,
    ) -> VvcChromaIntraPredictionMode {
        select_vvc_residual_chroma_intra_mode_from_costs(self.context, node, costs)
    }

    pub(in crate::vvc) fn select_luma_tu_coding_decision(
        self,
        node: VvcCodingTreeNode,
        mode: VvcIntraPredictionMode,
    ) -> VvcLumaTuCodingDecision {
        select_vvc_luma_tu_coding_decision(self.context, node, mode)
    }

    pub(in crate::vvc) fn select_chroma_tu_coding_decision(
        self,
        node: VvcCodingTreeNode,
        mode: VvcChromaIntraPredictionMode,
    ) -> VvcChromaTuCodingDecision {
        select_vvc_chroma_tu_coding_decision(self.context, node, mode)
    }

    pub(in crate::vvc) fn luma_planar_candidate_allowed(self, node: VvcCodingTreeNode) -> bool {
        vvc_residual_luma_planar_candidate_allowed(self.context, node)
    }

    pub(in crate::vvc) fn luma_directional_candidate_allowed(
        self,
        node: VvcCodingTreeNode,
    ) -> bool {
        vvc_residual_luma_directional_candidate_allowed(self.context, node)
    }

    pub(in crate::vvc) fn luma_mrl_candidate_allowed(
        self,
        node: VvcCodingTreeNode,
        mode: VvcIntraPredictionMode,
    ) -> bool {
        vvc_residual_luma_mrl_candidate_allowed(self.context, node, mode)
    }

    pub(in crate::vvc) fn chroma_cclm_candidate_allowed(
        self,
        node: VvcCodingTreeNode,
        geometry: VvcVideoGeometry,
    ) -> bool {
        vvc_residual_chroma_cclm_candidate_allowed(self.context, node, geometry)
    }
}

pub(in crate::vvc) fn select_vvc_residual_luma_intra_mode(
    context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
    costs: VvcLumaIntraCandidateCosts,
) -> VvcIntraPredictionMode {
    let _selector_scope = (
        context.chroma_sampling(),
        context.bit_depth(),
        node.width,
        node.height,
    );
    let mut best_mode = VvcIntraPredictionMode::Dc;
    let mut best_score = u64::MAX;
    for candidate in costs.iter() {
        if candidate.score < best_score {
            best_score = candidate.score;
            best_mode = candidate.mode;
        }
    }
    best_mode
}

pub(in crate::vvc) fn select_vvc_luma_max_leaf_size(
    context: VvcResidualModeDecisionContext,
) -> u16 {
    match context.residual_mode() {
        VvcResidualCodingMode::Lossy => VVC_CURRENT_MAX_LUMA_LEAF_SIZE,
        VvcResidualCodingMode::Lossless => VVC_LOSSLESS_LUMA_LEAF_SIZE,
    }
}

pub(in crate::vvc) fn select_vvc_luma_tu_residual_coding(
    context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
    _mode: VvcIntraPredictionMode,
) -> VvcTuResidualCodingMode {
    let _selector_scope = (
        context.chroma_sampling(),
        context.bit_depth(),
        node.width,
        node.height,
    );
    match context.residual_mode() {
        VvcResidualCodingMode::Lossless => VvcTuResidualCodingMode::TransformSkip,
        VvcResidualCodingMode::Lossy => VvcTuResidualCodingMode::Transformed,
    }
}

pub(in crate::vvc) fn select_vvc_luma_tu_coding_decision(
    context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
) -> VvcLumaTuCodingDecision {
    let residual_coding = select_vvc_luma_tu_residual_coding(context, node, mode);
    let mrl_index = select_vvc_luma_tu_mrl_index(context, node, mode, residual_coding);
    let mts_index = select_vvc_luma_tu_mts_index(context, node, mode, residual_coding, mrl_index);
    VvcLumaTuCodingDecision {
        residual_coding,
        mrl_index,
        mts_index,
    }
}

pub(in crate::vvc) fn select_vvc_luma_tu_mrl_index(
    context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
    residual_coding: VvcTuResidualCodingMode,
) -> u8 {
    let _selector_scope = (
        context.chroma_sampling(),
        context.bit_depth(),
        node.width,
        node.height,
        mode.luma_mode_index(),
        residual_coding,
    );
    0
}

pub(in crate::vvc) fn select_vvc_luma_tu_mts_index(
    context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
    residual_coding: VvcTuResidualCodingMode,
    mrl_index: u8,
) -> u8 {
    let _selector_scope = (
        context.chroma_sampling(),
        context.bit_depth(),
        node.width,
        node.height,
        mode.luma_mode_index(),
        residual_coding,
        mrl_index,
    );
    // TODO(vvc): choose nonzero MTS once the production selector is rate-safe
    // and cheap enough for 8x8 luma TUs. The transform and syntax plumbing is
    // wired, but the current exhaustive selector regresses bitrate and FPS.
    0
}

pub(in crate::vvc) fn select_vvc_residual_score_metric(
    context: VvcResidualModeDecisionContext,
) -> VvcResidualScoreMetric {
    let _selector_scope = (context.chroma_sampling(), context.bit_depth());
    match context.residual_mode() {
        VvcResidualCodingMode::Lossless => VvcResidualScoreMetric::Sad,
        VvcResidualCodingMode::Lossy => VvcResidualScoreMetric::Sse,
    }
}

pub(in crate::vvc) fn select_vvc_chroma_mode_syntax_tie_breaker(
    context: VvcResidualModeDecisionContext,
) -> bool {
    let _selector_scope = (context.chroma_sampling(), context.bit_depth());
    context.is_lossless()
}

pub(in crate::vvc) fn vvc_residual_luma_planar_candidate_allowed(
    _context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
) -> bool {
    node.width >= 4
        && node.height >= 4
        && node.width.is_power_of_two()
        && node.height.is_power_of_two()
}

pub(in crate::vvc) fn vvc_residual_luma_directional_candidate_allowed(
    _context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
) -> bool {
    node.width >= 4
        && node.height >= 4
        && node.width.is_power_of_two()
        && node.height.is_power_of_two()
}

pub(in crate::vvc) fn vvc_residual_luma_mrl_candidate_allowed(
    context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
    mode: VvcIntraPredictionMode,
) -> bool {
    let size_allowed = match context.residual_mode() {
        VvcResidualCodingMode::Lossless => node.width >= 4 && node.height >= 4,
        VvcResidualCodingMode::Lossy => node.width >= 8 && node.height >= 8,
    };
    node.y % VVC_CTU_SIZE as u16 != 0
        && !matches!(mode, VvcIntraPredictionMode::Planar)
        && size_allowed
        && node.width.is_power_of_two()
        && node.height.is_power_of_two()
}

#[cfg(test)]
pub(in crate::vvc) fn select_vvc_residual_chroma_intra_mode(
    context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
) -> VvcChromaIntraPredictionMode {
    select_vvc_residual_chroma_intra_mode_from_costs(
        context,
        node,
        VvcChromaIntraCandidateCosts::new(0),
    )
}

pub(in crate::vvc) fn select_vvc_residual_chroma_intra_mode_from_costs(
    context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
    costs: VvcChromaIntraCandidateCosts,
) -> VvcChromaIntraPredictionMode {
    let _candidate_scope = (
        context.chroma_sampling(),
        context.bit_depth(),
        context.is_lossless(),
        node.width,
        node.height,
    );
    let mut best_mode = VvcChromaIntraPredictionMode::Derived;
    let mut best_score = u64::MAX;
    for candidate in costs.iter() {
        if candidate.score < best_score {
            best_score = candidate.score;
            best_mode = candidate.mode;
        }
    }
    best_mode
}

pub(in crate::vvc) fn select_vvc_chroma_tu_residual_coding(
    context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
    _mode: VvcChromaIntraPredictionMode,
) -> VvcTuResidualCodingMode {
    let _selector_scope = (
        context.chroma_sampling(),
        context.bit_depth(),
        context.is_lossless(),
        node.width,
        node.height,
    );
    match context.residual_mode() {
        VvcResidualCodingMode::Lossless => VvcTuResidualCodingMode::TransformSkip,
        VvcResidualCodingMode::Lossy => VvcTuResidualCodingMode::Transformed,
    }
}

pub(in crate::vvc) fn select_vvc_chroma_tu_coding_decision(
    context: VvcResidualModeDecisionContext,
    node: VvcCodingTreeNode,
    mode: VvcChromaIntraPredictionMode,
) -> VvcChromaTuCodingDecision {
    VvcChromaTuCodingDecision {
        residual_coding: select_vvc_chroma_tu_residual_coding(context, node, mode),
    }
}

fn vvc_lossless_slice_qp(bit_depth: SampleBitDepth) -> i32 {
    -((i32::from(bit_depth.bits()) - 8) * 6)
}

fn vvc_lossy_slice_qp(
    stream_format: VvcPictureFormat,
    qp: Option<u8>,
    fast_search: VvcFastSearch,
) -> i32 {
    let requested_qp = qp.map_or(VVC_DEFAULT_LOSSY_LUMA_QP, |qp| i32::from(qp).clamp(1, 63));
    if fast_search != VvcFastSearch::LosslessSpeed {
        return requested_qp;
    }
    let tuned_qp = match (
        stream_format.chroma_sampling,
        stream_format.bit_depth.bits() > 8,
    ) {
        (ChromaSampling::Cs444, true) => requested_qp.saturating_sub(7),
        (ChromaSampling::Cs420 | ChromaSampling::Cs422, true) => {
            requested_qp.saturating_sub(6)
        }
        (ChromaSampling::Cs444, false) => requested_qp.saturating_sub(1),
        _ => requested_qp,
    };
    tuned_qp.clamp(1, 63)
}

fn vvc_lossy_chroma_qp_for_slice_qp(slice_qp: i32) -> i32 {
    vvc_mapped_chroma_qp_for_slice_qp(slice_qp.clamp(0, 63))
}

fn vvc_mapped_chroma_qp_for_slice_qp(slice_qp: i32) -> i32 {
    // Mirrors the SPS chroma QP mapping table written in header.rs:
    // start=17 and points (17->17), (27->29), (32->34), (44->41).
    const POINTS: [(i32, i32); 4] = [(17, 17), (27, 29), (32, 34), (44, 41)];
    if slice_qp <= POINTS[0].0 {
        return (POINTS[0].1 - (POINTS[0].0 - slice_qp)).clamp(0, 63);
    }
    for window in POINTS.windows(2) {
        let (in0, out0) = window[0];
        let (in1, out1) = window[1];
        if slice_qp <= in1 {
            let den = in1 - in0;
            let num = out1 - out0;
            let m = slice_qp - in0;
            let sh = den >> 1;
            return (out0 + ((num * m + sh) / den)).clamp(0, 63);
        }
    }
    let (last_in, last_out) = POINTS[POINTS.len() - 1];
    (last_out + (slice_qp - last_in)).clamp(0, 63)
}

#[cfg(test)]
const fn vvc_palette_lossless_slice_qp(bit_depth: SampleBitDepth) -> i32 {
    4 - ((bit_depth.bits() as i32 - 8) * 6)
}
