use super::binarization::vvc_encode_exp_golomb_ep_combined;
use super::context::VvcCabacInitType;
use super::ctu_split::{
    vvc_chroma_height, vvc_chroma_split_availability, vvc_chroma_width, VvcChromaSplitAvailability,
    VvcCodingTreeNode, VvcCtuCabacOp, VvcCtuPartitionParams, VvcCtuPartitionShape,
    VvcLumaNeighbourState, VvcPartSplit, VvcQtSplitCtxInput, VvcSplitCtxInput, VvcTreeType,
};
use super::{VvcCabacContext, VvcCabacContexts, VvcCabacEncoder};
use crate::picture::ChromaSampling;
use crate::vvc::residual::{VvcResidualCabacEncoder, VvcResidualCabacSymbolStream};
use crate::vvc::{
    chroma_subsample_x, chroma_subsample_y, vvc_chroma_cclm_node_allowed,
    vvc_chroma_explicit_candidate_index, VvcBdpcmMode, VvcChromaCclmMode,
    VvcChromaIntraPredictionMode, VvcIntraPredictionMode, VvcLumaIbcDecision, VvcLumaInterDecision,
    VvcLumaSccDecision, VvcResidualComponent, VvcSliceSyntaxConfig, VvcVideoGeometry,
    VVC_CHROMA_AC_COEFFS_PER_TU, VVC_CTU_SIZE, VVC_CURRENT_ENCODER_CHROMA_420_TB_SIZE,
    VVC_CURRENT_MAX_LUMA_MTT_DEPTH,
};

const VVC_LUMA_ANGULAR_BASE: i16 = 2;
const VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE: u16 = 4;
const VVC_CHROMA_NEIGHBOUR_CELL_SIZE: u16 = 2;
const VVC_NUM_LUMA_MODES: u32 = 67;
const VVC_NUM_MOST_PROBABLE_LUMA_MODES: usize = 6;
const VVC_REMAINING_LUMA_MODE_COUNT: u32 =
    VVC_NUM_LUMA_MODES - VVC_NUM_MOST_PROBABLE_LUMA_MODES as u32;
const VVC_NUM_INTRA_ANGULAR_MODES: i16 = 65;
const VVC_NUM_INTRA_ANGULAR_MODE_WRAP: i16 = VVC_NUM_INTRA_ANGULAR_MODES - 1;

include!("ctu_intra_mode_syntax.rs");
include!("ctu_inter_motion.rs");
include!("ctu_inter_skip.rs");
include!("ctu_frame_state.rs");
include!("ctu_dispatch.rs");
include!("ctu_split_syntax.rs");
include!("ctu_inter_syntax.rs");
include!("ctu_scc_syntax.rs");
include!("ctu_single_tree_prediction.rs");
include!("ctu_single_tree_residual.rs");
include!("ctu_chroma_tree_entry.rs");
include!("ctu_neighbours.rs");
include!("ctu_luma_mode_syntax.rs");
include!("ctu_luma_residual_tools.rs");
include!("ctu_chroma_mode_selection.rs");
include!("ctu_chroma_leaf_syntax.rs");
include!("ctu_chroma_residual_syntax.rs");
include!("ctu_luma_residual_syntax.rs");

#[cfg(any(test, feature = "bench-internals", feature = "vvc-stats"))]
pub(in crate::vvc) fn encode_ctu_partition_body(
    cabac: &mut VvcCabacEncoder,
    params: &VvcCtuPartitionParams,
    slice_config: VvcSliceSyntaxConfig,
) {
    let mut contexts = initial_vvc_cabac_contexts(slice_config);
    encode_ctu_partition_body_with_contexts(cabac, &mut contexts, params, slice_config);
}

#[cfg(any(test, feature = "bench-internals", feature = "vvc-stats"))]
pub(in crate::vvc) fn initial_vvc_cabac_contexts(
    slice_config: VvcSliceSyntaxConfig,
) -> VvcCabacContexts {
    initial_vvc_cabac_contexts_for_init_type(slice_config, VvcCabacInitType::I)
}

pub(in crate::vvc) fn initial_vvc_cabac_contexts_for_init_type(
    slice_config: VvcSliceSyntaxConfig,
    init_type: VvcCabacInitType,
) -> VvcCabacContexts {
    // H.266 initializes every active CABAC context set from the slice QP.
    // This must also apply when transform skip is disabled; using the
    // fallback QP there leaves predictive slices out of sync with decoders.
    VvcCabacContexts::with_slice_qp_and_init_type(slice_config.slice_qp, init_type)
}

#[cfg(any(test, feature = "bench-internals", feature = "vvc-stats"))]
pub(in crate::vvc) fn encode_ctu_partition_body_with_contexts(
    cabac: &mut VvcCabacEncoder,
    contexts: &mut VvcCabacContexts,
    params: &VvcCtuPartitionParams,
    slice_config: VvcSliceSyntaxConfig,
) {
    let mut ctu = VvcCtuCabacGenerator::new(contexts, params, slice_config);
    let mut luma_mode_neighbours =
        VvcLumaModeNeighbourState::new(params.visible_width as u16, params.visible_height as u16);
    let mut split_neighbours =
        VvcLumaNeighbourState::new(params.visible_width as u16, params.visible_height as u16);
    let shape = params.shape();
    VvcCtuCabacOp::visit_intra_ctu_partition_with_luma_neighbours(
        &mut split_neighbours,
        shape,
        0,
        0,
        shape.visible_width,
        shape.visible_height,
        params.luma_max_leaf_size,
        |op| {
            ctu.emit_with_luma_mode_neighbours(cabac, op, &mut luma_mode_neighbours);
        },
    );
}

#[derive(Debug)]
pub(in crate::vvc) struct VvcCtuCabacGenerator<'a, 'p> {
    contexts: &'a mut VvcCabacContexts,
    params: &'p VvcCtuPartitionParams,
    luma_tu_index: usize,
    chroma_tu_index: usize,
    chroma_inter_skip_active: bool,
    slice_config: VvcSliceSyntaxConfig,
    inter_slice: bool,
    inter_skip_ctx: u8,
    inter_skip_neighbours: Option<&'a mut VvcInterSkipNeighbourState>,
    inter_motion_neighbours: Option<&'a mut VvcInterMotionNeighbourState>,
}

impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    pub(in crate::vvc) fn new(
        contexts: &'a mut VvcCabacContexts,
        params: &'p VvcCtuPartitionParams,
        slice_config: VvcSliceSyntaxConfig,
    ) -> Self {
        Self {
            contexts,
            params,
            luma_tu_index: 0,
            chroma_tu_index: 0,
            chroma_inter_skip_active: vvc_chroma_inter_skip_active(
                &params.chroma_tu_inter_skip,
                params.chroma_tu_count,
            ),
            slice_config,
            inter_slice: false,
            inter_skip_ctx: 0,
            inter_skip_neighbours: None,
            inter_motion_neighbours: None,
        }
    }

    fn with_inter_slice(
        mut self,
        inter_slice: bool,
        skip_ctx: u8,
        skip_neighbours: Option<&'a mut VvcInterSkipNeighbourState>,
        motion_neighbours: Option<&'a mut VvcInterMotionNeighbourState>,
    ) -> Self {
        self.inter_slice = inter_slice;
        self.inter_skip_ctx = skip_ctx.min(2);
        self.inter_skip_neighbours = skip_neighbours;
        self.inter_motion_neighbours = motion_neighbours;
        self
    }

    fn emit_chroma_visible_qt_subtree(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
        min_leaf_size: u16,
        luma_mode_neighbours: &VvcLumaModeNeighbourState,
        neighbours: &mut VvcChromaNeighbourState,
    ) {
        debug_assert_eq!(node.tree_type, VvcTreeType::DualTreeChroma);
        if !node.intersects_visible(visible_width, visible_height) {
            return;
        }
        if node.fits_visible(visible_width, visible_height) && self.chroma_leaf_allowed(node) {
            self.emit_chroma_transform_only_leaf(
                cabac,
                node,
                vvc_chroma_split_availability(
                    node,
                    visible_width,
                    visible_height,
                    self.params.chroma_sampling,
                ),
                0,
                luma_mode_neighbours,
                neighbours,
            );
            return;
        }

        if !node.fits_visible(visible_width, visible_height) {
            self.emit_chroma_implicit_boundary_children(
                cabac,
                node,
                visible_width,
                visible_height,
                min_leaf_size,
                luma_mode_neighbours,
                neighbours,
            );
            return;
        }

        let split = vvc_chroma_split_availability(
            node,
            visible_width,
            visible_height,
            self.params.chroma_sampling,
        );
        if self.inter_slice
            && self.params.chroma_sampling != ChromaSampling::Cs420
            && self
                .params
                .chroma_tu_inter_skip
                .get(self.chroma_tu_index)
                .copied()
                .unwrap_or(false)
            && self.emit_chroma_inter_skip_subtree_if_all_skipped(
                cabac,
                node,
                visible_width,
                visible_height,
                split,
                neighbours,
            )
        {
            return;
        }
        if split.allow_qt {
            self.emit_chroma_visible_qt_split(cabac, node, split, neighbours);
            for child_idx in 0..4 {
                self.emit_chroma_visible_qt_subtree(
                    cabac,
                    node.qt_child(child_idx),
                    visible_width,
                    visible_height,
                    min_leaf_size,
                    luma_mode_neighbours,
                    neighbours,
                );
            }
        } else {
            // H.266 6.4.1 supplies the available MTT directions after QT is no
            // longer signaled. The current hardware residual subset chooses a
            // legal BT direction that drives the larger remaining axis toward
            // the 8x8 luma-coordinate leaf.
            let vertical = Self::chroma_prefer_vertical_bt(node, split);
            self.emit_chroma_visible_mtt_split(cabac, node, split, vertical, true, neighbours);
            for child_idx in 0..2 {
                self.emit_chroma_visible_qt_subtree(
                    cabac,
                    node.mtt_child(vertical, child_idx),
                    visible_width,
                    visible_height,
                    min_leaf_size,
                    luma_mode_neighbours,
                    neighbours,
                );
            }
        }
    }

    fn emit_chroma_inter_skip_subtree_if_all_skipped(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
        split: VvcChromaSplitAvailability,
        neighbours: &VvcChromaNeighbourState,
    ) -> bool {
        if !node.fits_visible(visible_width, visible_height) || !split.can_no {
            return false;
        }
        let Some(leaf_count) =
            self.chroma_all_inter_skip_subtree_leaf_count(node, visible_width, visible_height)
        else {
            return false;
        };
        if leaf_count < 4 {
            return false;
        }

        if split.can_split() {
            self.contexts.encode_split_flag(
                cabac,
                Self::chroma_split_ctx(node, split, neighbours),
                false,
            );
        }
        let skip_ctx = self.inter_skip_ctx_for_node(node);
        self.contexts.encode_cu_skip_flag(cabac, skip_ctx, true);
        if let Some(neighbours) = self.inter_skip_neighbours.as_mut() {
            neighbours.mark_leaf(node);
        }
        self.chroma_tu_index += leaf_count;
        true
    }

    fn chroma_all_inter_skip_subtree_leaf_count(
        &self,
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
    ) -> Option<usize> {
        let start = self.chroma_tu_index;
        if start >= self.params.chroma_tu_count
            || start >= self.params.chroma_tu_inter_skip.len()
            || !self.params.chroma_tu_inter_skip[start]
        {
            return None;
        }

        let leaf_count = self.chroma_subtree_leaf_count(node, visible_width, visible_height);
        let end = start.checked_add(leaf_count)?;
        if end > self.params.chroma_tu_count || end > self.params.chroma_tu_inter_skip.len() {
            return None;
        }
        self.params.chroma_tu_inter_skip[start..end]
            .iter()
            .all(|&skip| skip)
            .then_some(leaf_count)
    }

    fn chroma_subtree_leaf_count(
        &self,
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
    ) -> usize {
        if !node.intersects_visible(visible_width, visible_height) {
            return 0;
        }
        if node.fits_visible(visible_width, visible_height) && self.chroma_leaf_allowed(node) {
            return 1;
        }

        let split = vvc_chroma_split_availability(
            node,
            visible_width,
            visible_height,
            self.params.chroma_sampling,
        );
        if !node.fits_visible(visible_width, visible_height) {
            if split.allow_qt || split.implicit_split == VvcPartSplit::Quad {
                return (0..4)
                    .map(|child_idx| {
                        self.chroma_subtree_leaf_count(
                            node.qt_child(child_idx),
                            visible_width,
                            visible_height,
                        )
                    })
                    .sum();
            }
            if matches!(
                split.implicit_split,
                VvcPartSplit::HorizontalBinary | VvcPartSplit::VerticalBinary
            ) {
                let vertical = split.implicit_split == VvcPartSplit::VerticalBinary;
                return (0..2)
                    .map(|child_idx| {
                        self.chroma_subtree_leaf_count(
                            node.mtt_child_with_boundary_depth_offset(
                                vertical,
                                child_idx,
                                visible_width,
                                visible_height,
                            ),
                            visible_width,
                            visible_height,
                        )
                    })
                    .sum();
            }
            return 0;
        }

        if split.allow_qt {
            (0..4)
                .map(|child_idx| {
                    self.chroma_subtree_leaf_count(
                        node.qt_child(child_idx),
                        visible_width,
                        visible_height,
                    )
                })
                .sum()
        } else {
            let vertical = Self::chroma_prefer_vertical_bt(node, split);
            (0..2)
                .map(|child_idx| {
                    self.chroma_subtree_leaf_count(
                        node.mtt_child(vertical, child_idx),
                        visible_width,
                        visible_height,
                    )
                })
                .sum()
        }
    }

    fn emit_chroma_implicit_boundary_children(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
        min_leaf_size: u16,
        luma_mode_neighbours: &VvcLumaModeNeighbourState,
        neighbours: &mut VvcChromaNeighbourState,
    ) {
        let split = vvc_chroma_split_availability(
            node,
            visible_width,
            visible_height,
            self.params.chroma_sampling,
        );
        if split.allow_qt {
            if split.allow_btt() {
                self.contexts.encode_split_qt_flag(
                    cabac,
                    Self::chroma_qt_split_ctx(node, neighbours),
                    true,
                );
            }
            for child_idx in 0..4 {
                self.emit_chroma_visible_qt_subtree(
                    cabac,
                    node.qt_child(child_idx),
                    visible_width,
                    visible_height,
                    min_leaf_size,
                    luma_mode_neighbours,
                    neighbours,
                );
            }
            return;
        }
        match split.implicit_split {
            VvcPartSplit::Quad => {
                for child_idx in 0..4 {
                    self.emit_chroma_visible_qt_subtree(
                        cabac,
                        node.qt_child(child_idx),
                        visible_width,
                        visible_height,
                        min_leaf_size,
                        luma_mode_neighbours,
                        neighbours,
                    );
                }
            }
            VvcPartSplit::HorizontalBinary | VvcPartSplit::VerticalBinary => {
                let vertical = split.implicit_split == VvcPartSplit::VerticalBinary;
                self.emit_chroma_boundary_bt_split(cabac, node, split, vertical, neighbours);
                for child_idx in 0..2 {
                    self.emit_chroma_visible_qt_subtree(
                        cabac,
                        node.mtt_child_with_boundary_depth_offset(
                            vertical,
                            child_idx,
                            visible_width,
                            visible_height,
                        ),
                        visible_width,
                        visible_height,
                        min_leaf_size,
                        luma_mode_neighbours,
                        neighbours,
                    );
                }
            }
            VvcPartSplit::None => {
                debug_assert!(
                    !node.intersects_visible(visible_width, visible_height),
                    "boundary chroma node must have an implicit split"
                );
            }
        }
    }

    fn emit_chroma_visible_qt_split(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        neighbours: &VvcChromaNeighbourState,
    ) {
        let qt_ctx = Self::chroma_qt_split_ctx(node, neighbours);
        if split.can_no {
            self.contexts.encode_split_flag(
                cabac,
                Self::chroma_split_ctx(node, split, neighbours),
                true,
            );
        }
        if split.allow_btt() {
            self.contexts.encode_split_qt_flag(cabac, qt_ctx, true);
        }
    }

    fn emit_chroma_visible_mtt_split(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        vertical: bool,
        binary: bool,
        neighbours: &VvcChromaNeighbourState,
    ) {
        debug_assert!(!split.allow_qt || split.allow_btt());
        if split.can_no {
            self.contexts.encode_split_flag(
                cabac,
                Self::chroma_split_ctx(node, split, neighbours),
                true,
            );
        }
        if split.allow_qt {
            let qt_ctx = Self::chroma_qt_split_ctx(node, neighbours);
            self.contexts.encode_split_qt_flag(cabac, qt_ctx, false);
        }

        let can_hor = split.allow_bt_horizontal || split.allow_tt_horizontal;
        let can_ver = split.allow_bt_vertical || split.allow_tt_vertical;
        if can_ver && can_hor {
            self.contexts.encode_mtt_split_cu_vertical_flag(
                cabac,
                Self::chroma_mtt_vertical_ctx(node, split, neighbours),
                vertical,
            );
        }

        let can_binary = if vertical {
            split.allow_bt_vertical
        } else {
            split.allow_bt_horizontal
        };
        let can_ternary = if vertical {
            split.allow_tt_vertical
        } else {
            split.allow_tt_horizontal
        };
        if can_binary && can_ternary {
            self.contexts.encode_mtt_split_cu_binary_flag(
                cabac,
                VvcCtuCabacOp::mtt_binary_ctx(vertical, node.mtt_depth),
                binary,
            );
        }
    }

    fn emit_chroma_boundary_bt_split(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        _vertical: bool,
        neighbours: &VvcChromaNeighbourState,
    ) {
        // H.266 7.3.11.4 still signals split_qt_flag for an implicit
        // boundary BT when both QT and BTT are available; split_cu_flag itself
        // is inferred by 7.4.12.4 and therefore not written.
        if split.allow_qt && split.allow_btt() {
            self.contexts.encode_split_qt_flag(
                cabac,
                Self::chroma_qt_split_ctx(node, neighbours),
                false,
            );
        }
    }
}

fn vvc_cabac_op_trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("FRAMEFINERY_CABAC_OP_TRACE").is_some_and(|value| value != "0")
    })
}

fn vvc_scc_ibc_luma_node_allowed(node: VvcCodingTreeNode) -> bool {
    node.width <= 64 && node.height <= 64
}

fn vvc_scc_palette_luma_node_allowed(node: VvcCodingTreeNode) -> bool {
    node.width <= 64 && node.height <= 64 && u32::from(node.width) * u32::from(node.height) > 16
}

fn vvc_chroma_inter_skip_active(chroma_tu_inter_skip: &[bool], chroma_tu_count: usize) -> bool {
    chroma_tu_inter_skip[..chroma_tu_count.min(chroma_tu_inter_skip.len())]
        .iter()
        .any(|&skip| skip)
}

#[cfg(test)]
mod tests {
    use super::{vvc_chroma_inter_skip_active, vvc_explicit_inter_mvp_choice, VvcInterMotionInfo};

    #[test]
    fn chroma_inter_skip_active_ignores_inactive_tail() {
        assert!(!vvc_chroma_inter_skip_active(&[false, true], 1));
        assert!(vvc_chroma_inter_skip_active(&[false, true], 2));
        assert!(!vvc_chroma_inter_skip_active(&[false], 4));
        assert!(!vvc_chroma_inter_skip_active(&[], 4));
    }

    #[test]
    fn explicit_inter_mvp_choice_uses_mvd_syntax_cost() {
        let desired = VvcInterMotionInfo {
            mv_internal_x: 0,
            mv_internal_y: 0,
        };
        let candidates = [
            VvcInterMotionInfo {
                mv_internal_x: -8,
                mv_internal_y: -8,
            },
            VvcInterMotionInfo {
                mv_internal_x: -16,
                mv_internal_y: 0,
            },
        ];

        let choice = vvc_explicit_inter_mvp_choice(desired, candidates);

        assert_eq!(choice.index, 1);
        assert_eq!((choice.mvd_x, choice.mvd_y), (4, 0));
        assert_eq!(choice.mvd_syntax_cost, 8);
    }

    #[test]
    fn explicit_inter_mvp_choice_keeps_mvp0_on_syntax_cost_tie() {
        let desired = VvcInterMotionInfo {
            mv_internal_x: 0,
            mv_internal_y: 0,
        };
        let candidates = [
            VvcInterMotionInfo {
                mv_internal_x: -4,
                mv_internal_y: 0,
            },
            VvcInterMotionInfo {
                mv_internal_x: 0,
                mv_internal_y: -4,
            },
        ];

        let choice = vvc_explicit_inter_mvp_choice(desired, candidates);

        assert_eq!(choice.index, 0);
        assert_eq!((choice.mvd_x, choice.mvd_y), (1, 0));
    }
}
