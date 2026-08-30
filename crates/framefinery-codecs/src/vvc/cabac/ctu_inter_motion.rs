#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::vvc) struct VvcInterMotionInfo {
    pub(in crate::vvc) mv_internal_x: i32,
    pub(in crate::vvc) mv_internal_y: i32,
}

impl VvcInterMotionInfo {
    pub(in crate::vvc) fn from_full_pel_decision(decision: VvcLumaInterDecision) -> Self {
        Self {
            mv_internal_x: i32::from(decision.mv_x) << 4,
            mv_internal_y: i32::from(decision.mv_y) << 4,
        }
    }

    fn rounded_translational_amvp(self) -> Self {
        // The current encoder keeps AMVR disabled for translational inter CUs.
        // VTM rounds AMVP candidates from internal 1/16 precision to quarter
        // precision and back before duplicate pruning.
        Self {
            mv_internal_x: vvc_round_internal_mv_to_quarter(self.mv_internal_x),
            mv_internal_y: vvc_round_internal_mv_to_quarter(self.mv_internal_y),
        }
    }

    fn signalled_mvd_from(self, predictor: Self) -> (i32, i32) {
        let mvd_internal_x = self.mv_internal_x - predictor.mv_internal_x;
        let mvd_internal_y = self.mv_internal_y - predictor.mv_internal_y;
        debug_assert_eq!(mvd_internal_x & 3, 0);
        debug_assert_eq!(mvd_internal_y & 3, 0);
        (mvd_internal_x >> 2, mvd_internal_y >> 2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcExplicitInterMvpChoice {
    index: usize,
    mvd_x: i32,
    mvd_y: i32,
    mvd_syntax_cost: u64,
}

fn vvc_round_internal_mv_to_quarter(value: i32) -> i32 {
    ((value + 2) >> 2) << 2
}

fn vvc_explicit_inter_mvp_choice_for_decision(
    decision: VvcLumaInterDecision,
    candidates: [VvcInterMotionInfo; 2],
) -> VvcExplicitInterMvpChoice {
    vvc_explicit_inter_mvp_choice(
        VvcInterMotionInfo::from_full_pel_decision(decision),
        candidates,
    )
}

fn vvc_explicit_inter_mvp_choice(
    desired: VvcInterMotionInfo,
    candidates: [VvcInterMotionInfo; 2],
) -> VvcExplicitInterMvpChoice {
    let choice0 = vvc_explicit_inter_mvp_choice_at_index(desired, candidates, 0);
    let choice1 = vvc_explicit_inter_mvp_choice_at_index(desired, candidates, 1);
    if choice1.mvd_syntax_cost < choice0.mvd_syntax_cost {
        choice1
    } else {
        choice0
    }
}

fn vvc_explicit_inter_mvp_choice_at_index(
    desired: VvcInterMotionInfo,
    candidates: [VvcInterMotionInfo; 2],
    index: usize,
) -> VvcExplicitInterMvpChoice {
    let (mvd_x, mvd_y) = desired.signalled_mvd_from(candidates[index]);
    VvcExplicitInterMvpChoice {
        index,
        mvd_x,
        mvd_y,
        mvd_syntax_cost: vvc_explicit_inter_mvd_syntax_cost(mvd_x)
            .saturating_add(vvc_explicit_inter_mvd_syntax_cost(mvd_y)),
    }
}

fn vvc_explicit_inter_mvd_syntax_cost(value: i32) -> u64 {
    let magnitude = u64::from(value.unsigned_abs());
    if magnitude == 0 {
        return 1;
    }
    2 + vvc_unsigned_magnitude_syntax_cost(magnitude)
}

fn vvc_unsigned_magnitude_syntax_cost(mut value: u64) -> u64 {
    let mut bits = 1;
    while value > 1 {
        value >>= 1;
        bits += 2;
    }
    bits
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::vvc) struct VvcInterMotionNeighbourState {
    width: u16,
    height: u16,
    cell_width: usize,
    inter: Vec<Option<VvcInterMotionInfo>>,
    hmvp: Vec<VvcInterMotionInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VvcExplicitInterLeafSyntax {
    NotInter,
    NoResidual,
    Residual,
}

impl VvcInterMotionNeighbourState {
    const MAX_HMVP_CANDIDATES: usize = 5;

    pub(in crate::vvc) fn new(width: u16, height: u16) -> Self {
        let cell_width = usize::from(width.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE));
        let cell_height = usize::from(height.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE));
        Self {
            width,
            height,
            cell_width,
            inter: vec![None; cell_width * cell_height],
            hmvp: Vec::new(),
        }
    }

    pub(in crate::vvc) fn mark_leaf(
        &mut self,
        node: VvcCodingTreeNode,
        motion: VvcInterMotionInfo,
    ) {
        let end_x = (node.x + node.width).min(self.width);
        let end_y = (node.y + node.height).min(self.height);
        let start_cell_x = node.x / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE;
        let start_cell_y = node.y / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE;
        let end_cell_x = end_x.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        let end_cell_y = end_y.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        for cell_y in start_cell_y..end_cell_y {
            let start = usize::from(cell_y) * self.cell_width + usize::from(start_cell_x);
            let end = usize::from(cell_y) * self.cell_width + usize::from(end_cell_x);
            self.inter[start..end].fill(Some(motion));
        }
        if vvc_inter_motion_saved_for_hmvp(node) {
            self.push_hmvp(motion);
        }
    }

    fn push_hmvp(&mut self, motion: VvcInterMotionInfo) {
        if let Some(index) = self.hmvp.iter().position(|candidate| *candidate == motion) {
            self.hmvp.remove(index);
        } else if self.hmvp.len() == Self::MAX_HMVP_CANDIDATES {
            self.hmvp.remove(0);
        }
        self.hmvp.push(motion);
    }

    pub(in crate::vvc) fn mvp_candidates(
        &self,
        node: VvcCodingTreeNode,
    ) -> [VvcInterMotionInfo; 2] {
        let mut candidates = Vec::with_capacity(2);
        if let Some(left) = self.left_spatial_candidate(node) {
            candidates.push(left.rounded_translational_amvp());
        }
        if let Some(above) = self.above_spatial_candidate(node) {
            candidates.push(above.rounded_translational_amvp());
        }
        if candidates.len() == 2 && candidates[0] == candidates[1] {
            candidates.pop();
        }
        for candidate in &self.hmvp {
            if candidates.len() >= 2 {
                break;
            }
            candidates.push(candidate.rounded_translational_amvp());
        }
        while candidates.len() < 2 {
            candidates.push(VvcInterMotionInfo::default());
        }
        [candidates[0], candidates[1]]
    }

    fn left_spatial_candidate(&self, node: VvcCodingTreeNode) -> Option<VvcInterMotionInfo> {
        let x = node.x.checked_sub(1)?;
        self.motion_at(x, node.y.saturating_add(node.height))
            .or_else(|| {
                self.motion_at(
                    x,
                    node.y
                        .saturating_add(node.height)
                        .saturating_sub(1)
                        .min(self.height.saturating_sub(1)),
                )
            })
    }

    fn above_spatial_candidate(&self, node: VvcCodingTreeNode) -> Option<VvcInterMotionInfo> {
        let y = node.y.checked_sub(1)?;
        self.motion_at(node.x.saturating_add(node.width), y)
            .or_else(|| {
                self.motion_at(
                    node.x
                        .saturating_add(node.width)
                        .saturating_sub(1)
                        .min(self.width.saturating_sub(1)),
                    y,
                )
            })
            .or_else(|| self.motion_at(node.x.checked_sub(1)?, y))
    }

    fn motion_at(&self, x: u16, y: u16) -> Option<VvcInterMotionInfo> {
        let index = self.index(x, y)?;
        self.inter[index]
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let cell_x = usize::from(x / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        let cell_y = usize::from(y / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        Some(cell_y * self.cell_width + cell_x)
    }
}

fn vvc_inter_motion_saved_for_hmvp(node: VvcCodingTreeNode) -> bool {
    // SPS log2_parallel_merge_level_minus2 is currently fixed to zero, so the
    // parallel-merge level is 4 luma samples.
    const PARALLEL_MERGE_LEVEL: u16 = 4;
    let mask = !(u32::from(PARALLEL_MERGE_LEVEL) - 1);
    let crosses_x = ((u32::from(node.x + node.width) ^ u32::from(node.x)) & mask) != 0;
    let crosses_y = ((u32::from(node.y + node.height) ^ u32::from(node.y)) & mask) != 0;
    crosses_x && crosses_y
}
