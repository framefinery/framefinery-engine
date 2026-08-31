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
include!("ctu_chroma_tree_traversal.rs");
include!("ctu_chroma_boundary.rs");
include!("ctu_chroma_split_syntax.rs");
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

#[cfg(test)]
mod tests {
    use super::{vvc_explicit_inter_mvp_choice, VvcInterMotionInfo};

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
