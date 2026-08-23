use super::binarization::vvc_encode_exp_golomb_ep_combined;
use super::context::{VvcCabacInitType, VvcCabacProbModel};
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

pub(in crate::vvc) fn encode_ctu_partition_body(
    cabac: &mut VvcCabacEncoder,
    params: &VvcCtuPartitionParams,
    slice_config: VvcSliceSyntaxConfig,
) {
    let mut contexts = initial_vvc_cabac_contexts(slice_config);
    encode_ctu_partition_body_with_contexts(cabac, &mut contexts, params, slice_config);
}

fn encode_inter_skip_ctu_body_with_contexts(
    cabac: &mut VvcCabacEncoder,
    contexts: &mut VvcCabacContexts,
    ctu_geometry: VvcVideoGeometry,
    slice_config: VvcSliceSyntaxConfig,
) {
    let mut split_neighbours = VvcLumaNeighbourState::new(
        ctu_geometry.coded_width() as u16,
        ctu_geometry.coded_height() as u16,
    );
    let mut skip_neighbours = VvcInterSkipNeighbourState::new(
        ctu_geometry.coded_width() as u16,
        ctu_geometry.coded_height() as u16,
    );
    encode_inter_skip_ctu_body_with_frame_contexts(
        cabac,
        contexts,
        ctu_geometry,
        slice_config,
        &mut split_neighbours,
        &mut skip_neighbours,
        &mut VvcInterMotionNeighbourState::new(
            ctu_geometry.coded_width() as u16,
            ctu_geometry.coded_height() as u16,
        ),
        0,
        0,
        ctu_geometry.coded_width() as u16,
        ctu_geometry.coded_height() as u16,
    );
}

fn encode_inter_skip_ctu_body_with_frame_contexts(
    cabac: &mut VvcCabacEncoder,
    contexts: &mut VvcCabacContexts,
    ctu_geometry: VvcVideoGeometry,
    slice_config: VvcSliceSyntaxConfig,
    split_neighbours: &mut VvcLumaNeighbourState,
    skip_neighbours: &mut VvcInterSkipNeighbourState,
    motion_neighbours: &mut VvcInterMotionNeighbourState,
    origin_x: u16,
    origin_y: u16,
    picture_width: u16,
    picture_height: u16,
) {
    let shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: ctu_geometry.coded_width() as u16,
        visible_height: ctu_geometry.coded_height() as u16,
        chroma_sampling: slice_config.coding_tree.chroma_sampling,
        dual_tree_intra: false,
    };
    VvcCtuCabacOp::visit_inter_skip_ctu_partition_with_luma_neighbours(
        split_neighbours,
        shape,
        origin_x,
        origin_y,
        picture_width,
        picture_height,
        VVC_CTU_SIZE as u16,
        |op| emit_inter_skip_ctu_op(cabac, contexts, op, skip_neighbours, motion_neighbours),
    );
}

fn emit_inter_skip_ctu_op(
    cabac: &mut VvcCabacEncoder,
    contexts: &mut VvcCabacContexts,
    op: VvcCtuCabacOp,
    skip_neighbours: &mut VvcInterSkipNeighbourState,
    motion_neighbours: &mut VvcInterMotionNeighbourState,
) {
    match op {
        VvcCtuCabacOp::QtSplit {
            split_ctx,
            write_split_flag,
            write_qt_flag,
            qt_ctx,
            ..
        } => {
            if write_split_flag {
                contexts.encode_split_flag(cabac, split_ctx, true);
            }
            if write_qt_flag {
                contexts.encode_split_qt_flag(cabac, qt_ctx, true);
            }
        }
        VvcCtuCabacOp::BtSplit {
            vertical,
            split_ctx,
            write_split_flag,
            write_qt_flag,
            qt_ctx,
            write_mtt_vertical_flag,
            mtt_vertical_ctx,
            write_binary_flag,
            mtt_binary_ctx,
            mtt_binary_value,
            ..
        } => {
            if write_split_flag {
                contexts.encode_split_flag(cabac, split_ctx, true);
            }
            if write_qt_flag {
                contexts.encode_split_qt_flag(cabac, qt_ctx, false);
            }
            if write_mtt_vertical_flag {
                contexts.encode_mtt_split_cu_vertical_flag(cabac, mtt_vertical_ctx, vertical);
            }
            if write_binary_flag {
                contexts.encode_mtt_split_cu_binary_flag(cabac, mtt_binary_ctx, mtt_binary_value);
            }
        }
        VvcCtuCabacOp::LumaLeafWithSplitCtx {
            node,
            write_split_flag,
            split_ctx,
        } => {
            if write_split_flag {
                contexts.encode_split_flag(cabac, split_ctx, false);
            }
            let skip_ctx = skip_neighbours.skip_ctx(node);
            contexts.encode_cu_skip_flag(cabac, skip_ctx, true);
            skip_neighbours.mark_leaf(node);
            motion_neighbours.mark_leaf(node, VvcInterMotionInfo::default());
        }
        VvcCtuCabacOp::ChromaTree { .. } => {}
    }
}

fn vvc_luma_mpm_list(
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> [u8; VVC_NUM_MOST_PROBABLE_LUMA_MODES] {
    let left = left
        .unwrap_or(VvcIntraPredictionMode::Planar)
        .luma_mode_index();
    let above = above
        .unwrap_or(VvcIntraPredictionMode::Planar)
        .luma_mode_index();
    let min = left.min(above);
    let max = left.max(above);
    let mut mpm = [0; VVC_NUM_MOST_PROBABLE_LUMA_MODES];
    mpm[0] = VvcIntraPredictionMode::Planar.luma_mode_index();
    if max < VVC_LUMA_ANGULAR_BASE as u8 {
        mpm[1] = VvcIntraPredictionMode::Dc.luma_mode_index();
        mpm[2] = VvcIntraPredictionMode::Vertical.luma_mode_index();
        mpm[3] = VvcIntraPredictionMode::Horizontal.luma_mode_index();
        mpm[4] = vvc_wrap_luma_angular_mode(
            i16::from(VvcIntraPredictionMode::Vertical.luma_mode_index()) - 4,
        );
        mpm[5] = vvc_wrap_luma_angular_mode(
            i16::from(VvcIntraPredictionMode::Vertical.luma_mode_index()) + 4,
        );
        return mpm;
    }
    if left == above || min < VVC_LUMA_ANGULAR_BASE as u8 {
        mpm[1] = max;
        mpm[2] = vvc_wrap_luma_angular_mode(i16::from(max) - 1);
        mpm[3] = vvc_wrap_luma_angular_mode(i16::from(max) + 1);
        mpm[4] = vvc_wrap_luma_angular_mode(i16::from(max) - 2);
        mpm[5] = vvc_wrap_luma_angular_mode(i16::from(max) + 2);
        return mpm;
    }

    mpm[1] = left;
    mpm[2] = above;
    let diff = max - min;
    if diff == 1 {
        mpm[3] = vvc_wrap_luma_angular_mode(i16::from(min) - 1);
        mpm[4] = vvc_wrap_luma_angular_mode(i16::from(max) + 1);
        mpm[5] = vvc_wrap_luma_angular_mode(i16::from(min) - 2);
    } else if diff >= VVC_NUM_INTRA_ANGULAR_MODES as u8 - 3 {
        mpm[3] = vvc_wrap_luma_angular_mode(i16::from(min) + 1);
        mpm[4] = vvc_wrap_luma_angular_mode(i16::from(max) - 1);
        mpm[5] = vvc_wrap_luma_angular_mode(i16::from(min) + 2);
    } else if diff == 2 {
        mpm[3] = vvc_wrap_luma_angular_mode(i16::from(min) + 1);
        mpm[4] = vvc_wrap_luma_angular_mode(i16::from(min) - 1);
        mpm[5] = vvc_wrap_luma_angular_mode(i16::from(max) + 1);
    } else {
        mpm[3] = vvc_wrap_luma_angular_mode(i16::from(min) - 1);
        mpm[4] = vvc_wrap_luma_angular_mode(i16::from(min) + 1);
        mpm[5] = vvc_wrap_luma_angular_mode(i16::from(max) - 1);
    }
    mpm
}

#[cfg(test)]
pub(in crate::vvc) fn vvc_luma_mpm_list_for_test(
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> [u8; VVC_NUM_MOST_PROBABLE_LUMA_MODES] {
    vvc_luma_mpm_list(left, above)
}

pub(in crate::vvc) fn vvc_luma_intra_mode_syntax_bin_count(
    mode: VvcIntraPredictionMode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> u8 {
    let mode_index = mode.luma_mode_index();
    let mpm = vvc_luma_mpm_list(left, above);
    if let Some(mpm_idx) = vvc_luma_mpm_index_for_mode_index(mode_index, mpm) {
        let bypass_bins = if mpm_idx == 0 { 0 } else { mpm_idx.min(4) };
        return 2 + bypass_bins as u8;
    }

    1 + vvc_trunc_bin_code_ep_bin_count(
        vvc_luma_remaining_mode_index(mode_index, mpm),
        VVC_REMAINING_LUMA_MODE_COUNT,
    )
}

pub(in crate::vvc) fn vvc_luma_intra_mode_is_mpm(
    mode: VvcIntraPredictionMode,
    left: Option<VvcIntraPredictionMode>,
    above: Option<VvcIntraPredictionMode>,
) -> bool {
    vvc_luma_mpm_index_for_mode_index(mode.luma_mode_index(), vvc_luma_mpm_list(left, above))
        .is_some()
}

fn vvc_luma_mpm_index_for_mode_index(
    mode_index: u8,
    mpm: [u8; VVC_NUM_MOST_PROBABLE_LUMA_MODES],
) -> Option<usize> {
    mpm.iter().position(|candidate| *candidate == mode_index)
}

pub(in crate::vvc) fn vvc_chroma_intra_mode_syntax_bin_count(
    mode: VvcChromaIntraPredictionMode,
    cclm_enabled: bool,
) -> u8 {
    let cclm_flag_bins = u8::from(cclm_enabled);
    match mode {
        VvcChromaIntraPredictionMode::Cclm(cclm_mode) => {
            debug_assert!(cclm_enabled);
            cclm_flag_bins
                + 1
                + match cclm_mode {
                    VvcChromaCclmMode::Linear => 0,
                    VvcChromaCclmMode::MdlmLeft | VvcChromaCclmMode::MdlmTop => 1,
                }
        }
        VvcChromaIntraPredictionMode::Derived => cclm_flag_bins + 1,
        VvcChromaIntraPredictionMode::Explicit(_) => cclm_flag_bins + 3,
    }
}

fn vvc_wrap_luma_angular_mode(mode: i16) -> u8 {
    ((mode - VVC_LUMA_ANGULAR_BASE).rem_euclid(VVC_NUM_INTRA_ANGULAR_MODE_WRAP)
        + VVC_LUMA_ANGULAR_BASE) as u8
}

fn vvc_luma_remaining_mode_index(
    mode_index: u8,
    mut mpm: [u8; VVC_NUM_MOST_PROBABLE_LUMA_MODES],
) -> u32 {
    let mut remaining = u32::from(mode_index);
    mpm.sort_unstable();
    for candidate in mpm.into_iter().rev() {
        if remaining > u32::from(candidate) {
            remaining -= 1;
        }
    }
    debug_assert!(remaining < VVC_REMAINING_LUMA_MODE_COUNT);
    remaining
}

fn vvc_trunc_bin_code_ep_bin_count(symbol: u32, num_symbols: u32) -> u8 {
    debug_assert!(symbol < num_symbols);
    let thresh = 31 - num_symbols.leading_zeros();
    let val = 1 << thresh;
    let b = num_symbols - val;
    if symbol < val - b {
        thresh as u8
    } else {
        (thresh + 1) as u8
    }
}

fn encode_vvc_trunc_bin_code_ep(cabac: &mut VvcCabacEncoder, symbol: u32, num_symbols: u32) {
    debug_assert!(symbol < num_symbols);
    let thresh = 31 - num_symbols.leading_zeros();
    let val = 1 << thresh;
    let b = num_symbols - val;
    if symbol < val - b {
        cabac.encode_bins_ep(symbol, thresh);
    } else {
        cabac.encode_bins_ep(symbol + val - b, thresh + 1);
    }
}

pub(in crate::vvc) fn initial_vvc_cabac_contexts(
    slice_config: VvcSliceSyntaxConfig,
) -> VvcCabacContexts {
    initial_vvc_cabac_contexts_for_init_type(slice_config, VvcCabacInitType::I)
}

pub(in crate::vvc) fn initial_vvc_cabac_contexts_for_init_type(
    slice_config: VvcSliceSyntaxConfig,
    init_type: VvcCabacInitType,
) -> VvcCabacContexts {
    if slice_config.tools.transform_skip_enabled {
        VvcCabacContexts::with_slice_qp_and_init_type(slice_config.slice_qp, init_type)
    } else {
        VvcCabacContexts::with_slice_qp_and_init_type(VvcCabacContexts::DEFAULT_SLICE_QP, init_type)
    }
}

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

pub(in crate::vvc) struct VvcFrameCtuCabacState {
    contexts: VvcCabacContexts,
    luma_neighbours: VvcLumaNeighbourState,
    luma_mode_neighbours: VvcLumaModeNeighbourState,
    chroma_neighbours: VvcChromaNeighbourState,
    inter_skip_neighbours: VvcInterSkipNeighbourState,
    inter_motion_neighbours: VvcInterMotionNeighbourState,
    skip_neighbours: Vec<bool>,
    pred_mode_contexts: Option<[VvcCabacProbModel; 2]>,
    inter_slice: bool,
    picture_width: u16,
    picture_height: u16,
    ctu_cols: usize,
}

impl VvcFrameCtuCabacState {
    pub(in crate::vvc) fn new(
        picture_geometry: VvcVideoGeometry,
        slice_config: VvcSliceSyntaxConfig,
        inter_slice: bool,
    ) -> Self {
        let picture_width = picture_geometry.coded_width() as u16;
        let picture_height = picture_geometry.coded_height() as u16;
        let init_type = if inter_slice {
            VvcCabacInitType::P
        } else {
            VvcCabacInitType::I
        };
        Self {
            contexts: initial_vvc_cabac_contexts_for_init_type(slice_config, init_type),
            luma_neighbours: VvcLumaNeighbourState::new(picture_width, picture_height),
            luma_mode_neighbours: VvcLumaModeNeighbourState::new(picture_width, picture_height),
            chroma_neighbours: VvcChromaNeighbourState::new(
                picture_width,
                picture_height,
                slice_config.coding_tree.chroma_sampling,
            ),
            inter_skip_neighbours: VvcInterSkipNeighbourState::new(picture_width, picture_height),
            inter_motion_neighbours: VvcInterMotionNeighbourState::new(
                picture_width,
                picture_height,
            ),
            skip_neighbours: vec![
                false;
                picture_geometry.coded_width().div_ceil(VVC_CTU_SIZE)
                    * picture_geometry.coded_height().div_ceil(VVC_CTU_SIZE)
            ],
            pred_mode_contexts: inter_slice.then(|| {
                std::array::from_fn(|idx| {
                    VvcCabacProbModel::from_context(
                        VvcCabacContext::PredModeFlag(idx as u8),
                        slice_config.slice_qp,
                    )
                })
            }),
            inter_slice,
            picture_width,
            picture_height,
            ctu_cols: picture_geometry.coded_width().div_ceil(VVC_CTU_SIZE),
        }
    }

    pub(in crate::vvc) fn encode_ctu(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        slice_address: usize,
        params: &VvcCtuPartitionParams,
        slice_config: VvcSliceSyntaxConfig,
    ) {
        let ctu_x = slice_address % self.ctu_cols;
        let ctu_y = slice_address / self.ctu_cols;
        let origin_x = (ctu_x * VVC_CTU_SIZE) as u16;
        let origin_y = (ctu_y * VVC_CTU_SIZE) as u16;
        let skip_ctx = self.skip_ctx(slice_address);
        let pred_mode_contexts = self.pred_mode_contexts.as_mut();
        let luma_neighbours = &mut self.luma_neighbours;
        let luma_mode_neighbours = &mut self.luma_mode_neighbours;
        let chroma_neighbours = &mut self.chroma_neighbours;
        let inter_motion_neighbours = &mut self.inter_motion_neighbours;
        let shape = if self.inter_slice {
            params.single_tree_shape()
        } else {
            params.shape()
        };
        let mut ctu_encoder = VvcCtuCabacGenerator::new(&mut self.contexts, params, slice_config)
            .with_inter_slice(
                self.inter_slice,
                skip_ctx,
                pred_mode_contexts,
                Some(&mut self.inter_skip_neighbours),
                Some(inter_motion_neighbours),
            );
        if self.inter_slice {
            VvcCtuCabacOp::visit_inter_skip_ctu_partition_with_luma_neighbours(
                luma_neighbours,
                shape,
                origin_x,
                origin_y,
                self.picture_width,
                self.picture_height,
                params.luma_max_leaf_size,
                |op| {
                    ctu_encoder.emit_with_frame_neighbours(
                        cabac,
                        op,
                        luma_mode_neighbours,
                        chroma_neighbours,
                    );
                },
            );
        } else {
            VvcCtuCabacOp::visit_intra_ctu_partition_with_luma_neighbours(
                luma_neighbours,
                shape,
                origin_x,
                origin_y,
                self.picture_width,
                self.picture_height,
                params.luma_max_leaf_size,
                |op| {
                    ctu_encoder.emit_with_frame_neighbours(
                        cabac,
                        op,
                        luma_mode_neighbours,
                        chroma_neighbours,
                    );
                },
            );
        }
        if slice_address < self.skip_neighbours.len() {
            self.skip_neighbours[slice_address] = false;
        }
    }

    pub(in crate::vvc) fn encode_inter_skip_ctu(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        slice_address: usize,
        ctu_geometry: VvcVideoGeometry,
        slice_config: VvcSliceSyntaxConfig,
    ) {
        let ctu_x = slice_address % self.ctu_cols;
        let ctu_y = slice_address / self.ctu_cols;
        encode_inter_skip_ctu_body_with_frame_contexts(
            cabac,
            &mut self.contexts,
            ctu_geometry,
            slice_config,
            &mut self.luma_neighbours,
            &mut self.inter_skip_neighbours,
            &mut self.inter_motion_neighbours,
            (ctu_x * VVC_CTU_SIZE) as u16,
            (ctu_y * VVC_CTU_SIZE) as u16,
            self.picture_width,
            self.picture_height,
        );
        if slice_address < self.skip_neighbours.len() {
            self.skip_neighbours[slice_address] = true;
        }
    }

    fn skip_ctx(&self, slice_address: usize) -> u8 {
        let left = slice_address % self.ctu_cols != 0
            && self
                .skip_neighbours
                .get(slice_address - 1)
                .copied()
                .unwrap_or(false);
        let above = slice_address >= self.ctu_cols
            && self
                .skip_neighbours
                .get(slice_address - self.ctu_cols)
                .copied()
                .unwrap_or(false);
        u8::from(left) + u8::from(above)
    }
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
    inter_pred_mode_contexts: Option<&'a mut [VvcCabacProbModel; 2]>,
    inter_skip_neighbours: Option<&'a mut VvcInterSkipNeighbourState>,
    inter_motion_neighbours: Option<&'a mut VvcInterMotionNeighbourState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcChromaNeighbourInfo {
    cb_width: u16,
    cb_height: u16,
    cqt_depth: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VvcLumaModeNeighbourState {
    width: u16,
    height: u16,
    cell_width: usize,
    valid: Vec<bool>,
    modes: Vec<VvcIntraPredictionMode>,
    mip_flags: Vec<bool>,
}

impl VvcLumaModeNeighbourState {
    fn new(width: u16, height: u16) -> Self {
        let cell_width = usize::from(width.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE));
        let cell_height = usize::from(height.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE));
        let samples = cell_width * cell_height;
        Self {
            width,
            height,
            cell_width,
            valid: vec![false; samples],
            modes: vec![VvcIntraPredictionMode::Planar; samples],
            mip_flags: vec![false; samples],
        }
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let cell_x = usize::from(x / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        let cell_y = usize::from(y / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        Some(cell_y * self.cell_width + cell_x)
    }

    fn mode_at(&self, x: u16, y: u16) -> Option<VvcIntraPredictionMode> {
        let index = self.index(x, y)?;
        self.valid[index].then_some(self.modes[index])
    }

    fn mip_flag_at(&self, x: u16, y: u16) -> bool {
        self.index(x, y)
            .filter(|&index| self.valid[index])
            .is_some_and(|index| self.mip_flags[index])
    }

    fn left_of(&self, node: VvcCodingTreeNode) -> Option<VvcIntraPredictionMode> {
        let x = node.x.checked_sub(1)?;
        let y = (node.y + node.height)
            .saturating_sub(1)
            .min(self.height.saturating_sub(1));
        self.mode_at(x, y)
    }

    fn left_mip_flag_of(&self, node: VvcCodingTreeNode) -> bool {
        let Some(x) = node.x.checked_sub(1) else {
            return false;
        };
        let y = (node.y + node.height)
            .saturating_sub(1)
            .min(self.height.saturating_sub(1));
        self.mip_flag_at(x, y)
    }

    fn above_of(&self, node: VvcCodingTreeNode) -> Option<VvcIntraPredictionMode> {
        let y = node.y.checked_sub(1)?;
        if node.y % VVC_CTU_SIZE as u16 == 0 {
            return None;
        }
        let x = (node.x + node.width)
            .saturating_sub(1)
            .min(self.width.saturating_sub(1));
        self.mode_at(x, y)
    }

    fn above_mip_flag_of(&self, node: VvcCodingTreeNode) -> bool {
        let Some(y) = node.y.checked_sub(1) else {
            return false;
        };
        let x = (node.x + node.width)
            .saturating_sub(1)
            .min(self.width.saturating_sub(1));
        self.mip_flag_at(x, y)
    }

    fn co_located_for_chroma(&self, node: VvcCodingTreeNode) -> Option<VvcIntraPredictionMode> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let x = node
            .x
            .saturating_add(node.width >> 1)
            .min(self.width.saturating_sub(1));
        let y = node
            .y
            .saturating_add(node.height >> 1)
            .min(self.height.saturating_sub(1));
        self.mode_at(x, y)
    }

    fn mark_leaf(&mut self, node: VvcCodingTreeNode, mode: VvcIntraPredictionMode) {
        let end_x = (node.x + node.width).min(self.width);
        let end_y = (node.y + node.height).min(self.height);
        let start_cell_x = node.x / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE;
        let start_cell_y = node.y / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE;
        let end_cell_x = end_x.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        let end_cell_y = end_y.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        for cell_y in start_cell_y..end_cell_y {
            let start = usize::from(cell_y) * self.cell_width + usize::from(start_cell_x);
            let end = usize::from(cell_y) * self.cell_width + usize::from(end_cell_x);
            self.valid[start..end].fill(true);
            self.modes[start..end].fill(mode);
            self.mip_flags[start..end].fill(false);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VvcInterSkipNeighbourState {
    width: u16,
    height: u16,
    cell_width: usize,
    skipped: Vec<bool>,
}

impl VvcInterSkipNeighbourState {
    fn new(width: u16, height: u16) -> Self {
        let cell_width = usize::from(width.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE));
        let cell_height = usize::from(height.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE));
        Self {
            width,
            height,
            cell_width,
            skipped: vec![false; cell_width * cell_height],
        }
    }

    fn skip_ctx(&self, node: VvcCodingTreeNode) -> u8 {
        u8::from(self.left_of(node)) + u8::from(self.above_of(node))
    }

    fn left_of(&self, node: VvcCodingTreeNode) -> bool {
        let Some(x) = node.x.checked_sub(1) else {
            return false;
        };
        let y = (node.y + node.height)
            .saturating_sub(1)
            .min(self.height.saturating_sub(1));
        self.skipped_at(x, y)
    }

    fn above_of(&self, node: VvcCodingTreeNode) -> bool {
        let Some(y) = node.y.checked_sub(1) else {
            return false;
        };
        let x = (node.x + node.width)
            .saturating_sub(1)
            .min(self.width.saturating_sub(1));
        self.skipped_at(x, y)
    }

    fn skipped_at(&self, x: u16, y: u16) -> bool {
        self.index(x, y).is_some_and(|index| self.skipped[index])
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let cell_x = usize::from(x / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        let cell_y = usize::from(y / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        Some(cell_y * self.cell_width + cell_x)
    }

    fn mark_leaf(&mut self, node: VvcCodingTreeNode) {
        let end_x = (node.x + node.width).min(self.width);
        let end_y = (node.y + node.height).min(self.height);
        let start_cell_x = node.x / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE;
        let start_cell_y = node.y / VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE;
        let end_cell_x = end_x.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        let end_cell_y = end_y.div_ceil(VVC_LUMA_MODE_NEIGHBOUR_CELL_SIZE);
        for cell_y in start_cell_y..end_cell_y {
            let start = usize::from(cell_y) * self.cell_width + usize::from(start_cell_x);
            let end = usize::from(cell_y) * self.cell_width + usize::from(end_cell_x);
            self.skipped[start..end].fill(true);
        }
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct VvcChromaNeighbourState {
    width: u16,
    height: u16,
    chroma_sampling: ChromaSampling,
    cell_width: usize,
    valid: Vec<bool>,
    cb_width: Vec<u16>,
    cb_height: Vec<u16>,
    cqt_depth: Vec<u8>,
}

impl VvcChromaNeighbourState {
    fn new(visible_width: u16, visible_height: u16, chroma_sampling: ChromaSampling) -> Self {
        let width = visible_width / chroma_subsample_x(chroma_sampling) as u16;
        let height = visible_height / chroma_subsample_y(chroma_sampling) as u16;
        let cell_width = usize::from(width.div_ceil(VVC_CHROMA_NEIGHBOUR_CELL_SIZE));
        let cell_height = usize::from(height.div_ceil(VVC_CHROMA_NEIGHBOUR_CELL_SIZE));
        let cells = cell_width * cell_height;
        Self {
            width,
            height,
            chroma_sampling,
            cell_width,
            valid: vec![false; cells],
            cb_width: vec![0; cells],
            cb_height: vec![0; cells],
            cqt_depth: vec![0; cells],
        }
    }

    fn node_x(&self, node: VvcCodingTreeNode) -> u16 {
        node.x / chroma_subsample_x(self.chroma_sampling) as u16
    }

    fn node_y(&self, node: VvcCodingTreeNode) -> u16 {
        node.y / chroma_subsample_y(self.chroma_sampling) as u16
    }

    fn node_width(&self, node: VvcCodingTreeNode) -> u16 {
        vvc_chroma_width(node, self.chroma_sampling)
    }

    fn node_height(&self, node: VvcCodingTreeNode) -> u16 {
        vvc_chroma_height(node, self.chroma_sampling)
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let cell_x = usize::from(x / VVC_CHROMA_NEIGHBOUR_CELL_SIZE);
        let cell_y = usize::from(y / VVC_CHROMA_NEIGHBOUR_CELL_SIZE);
        Some(cell_y * self.cell_width + cell_x)
    }

    fn info_at(&self, x: u16, y: u16) -> Option<VvcChromaNeighbourInfo> {
        let index = self.index(x, y)?;
        self.valid[index].then_some(VvcChromaNeighbourInfo {
            cb_width: self.cb_width[index],
            cb_height: self.cb_height[index],
            cqt_depth: self.cqt_depth[index],
        })
    }

    fn left_of(&self, node: VvcCodingTreeNode) -> Option<VvcChromaNeighbourInfo> {
        let y = self.node_y(node);
        self.node_x(node)
            .checked_sub(1)
            .and_then(|x| self.info_at(x, y))
    }

    fn above_of(&self, node: VvcCodingTreeNode) -> Option<VvcChromaNeighbourInfo> {
        let x = self.node_x(node);
        self.node_y(node)
            .checked_sub(1)
            .and_then(|y| self.info_at(x, y))
    }

    fn mark_leaf(&mut self, node: VvcCodingTreeNode) {
        let start_x = self.node_x(node);
        let start_y = self.node_y(node);
        let node_width = self.node_width(node);
        let node_height = self.node_height(node);
        let end_x = (start_x + node_width).min(self.width);
        let end_y = (start_y + node_height).min(self.height);
        let start_cell_x = start_x / VVC_CHROMA_NEIGHBOUR_CELL_SIZE;
        let start_cell_y = start_y / VVC_CHROMA_NEIGHBOUR_CELL_SIZE;
        let end_cell_x = end_x.div_ceil(VVC_CHROMA_NEIGHBOUR_CELL_SIZE);
        let end_cell_y = end_y.div_ceil(VVC_CHROMA_NEIGHBOUR_CELL_SIZE);
        for cell_y in start_cell_y..end_cell_y {
            let start = usize::from(cell_y) * self.cell_width + usize::from(start_cell_x);
            let end = usize::from(cell_y) * self.cell_width + usize::from(end_cell_x);
            self.valid[start..end].fill(true);
            self.cb_width[start..end].fill(node_width);
            self.cb_height[start..end].fill(node_height);
            self.cqt_depth[start..end].fill(node.cqt_depth);
        }
    }
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
            inter_pred_mode_contexts: None,
            inter_skip_neighbours: None,
            inter_motion_neighbours: None,
        }
    }

    fn with_inter_slice(
        mut self,
        inter_slice: bool,
        skip_ctx: u8,
        pred_mode_contexts: Option<&'a mut [VvcCabacProbModel; 2]>,
        skip_neighbours: Option<&'a mut VvcInterSkipNeighbourState>,
        motion_neighbours: Option<&'a mut VvcInterMotionNeighbourState>,
    ) -> Self {
        self.inter_slice = inter_slice;
        self.inter_skip_ctx = skip_ctx.min(2);
        self.inter_pred_mode_contexts = pred_mode_contexts;
        self.inter_skip_neighbours = skip_neighbours;
        self.inter_motion_neighbours = motion_neighbours;
        self
    }

    #[cfg(test)]
    pub(in crate::vvc) fn emit(&mut self, cabac: &mut VvcCabacEncoder, op: VvcCtuCabacOp) {
        let mut luma_mode_neighbours = VvcLumaModeNeighbourState::new(
            self.params.visible_width as u16,
            self.params.visible_height as u16,
        );
        self.emit_with_luma_mode_neighbours(cabac, op, &mut luma_mode_neighbours);
    }

    fn emit_with_luma_mode_neighbours(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        op: VvcCtuCabacOp,
        luma_mode_neighbours: &mut VvcLumaModeNeighbourState,
    ) {
        if vvc_cabac_op_trace_enabled() {
            eprintln!("FF_CABAC_OP {op:?}");
        }
        match op {
            VvcCtuCabacOp::QtSplit {
                node,
                split_ctx,
                write_split_flag,
                write_qt_flag,
                qt_ctx,
            } => self.emit_qt_split(
                cabac,
                node,
                split_ctx,
                write_split_flag,
                write_qt_flag,
                qt_ctx,
            ),
            op @ VvcCtuCabacOp::BtSplit { .. } => self.emit_bt_split(cabac, op),
            VvcCtuCabacOp::LumaLeafWithSplitCtx {
                node,
                write_split_flag,
                split_ctx,
            } => {
                self.emit_luma_leaf_split_with_ctx(cabac, node, write_split_flag, split_ctx);
                if self.emit_luma_inter_skip_leaf(cabac, node) {
                    return;
                }
                match self.emit_luma_explicit_inter_leaf(cabac, node, luma_mode_neighbours) {
                    VvcExplicitInterLeafSyntax::NotInter => {}
                    VvcExplicitInterLeafSyntax::NoResidual => return,
                    VvcExplicitInterLeafSyntax::Residual => {
                        self.emit_transform_unit_residual(cabac, node);
                        return;
                    }
                }
                if self.emit_luma_scc_selected_leaf(cabac, node) {
                    return;
                }
                self.emit_luma_inter_slice_intra_prefix(cabac, node, luma_mode_neighbours);
                self.emit_luma_scc_regular_intra_prefix(cabac, node);
                if !self.emit_luma_bdpcm_mode(cabac, node, luma_mode_neighbours) {
                    if !self.emit_luma_mip_mode(cabac, node, luma_mode_neighbours) {
                        self.emit_luma_multi_ref_line(cabac, node);
                        self.emit_luma_isp_mode(cabac, node);
                        self.emit_luma_intra_prediction_mode(cabac, node, luma_mode_neighbours);
                    }
                }
                self.emit_single_tree_chroma_prediction(cabac, node, luma_mode_neighbours);
                self.emit_transform_unit_residual(cabac, node);
            }
            VvcCtuCabacOp::ChromaTree {
                node,
                visible_width,
                visible_height,
            } => self.emit_chroma_tree(
                cabac,
                node,
                visible_width,
                visible_height,
                luma_mode_neighbours,
            ),
        }
    }

    fn emit_with_frame_neighbours(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        op: VvcCtuCabacOp,
        luma_mode_neighbours: &mut VvcLumaModeNeighbourState,
        chroma_neighbours: &mut VvcChromaNeighbourState,
    ) {
        if vvc_cabac_op_trace_enabled() {
            eprintln!("FF_CABAC_OP {op:?}");
        }
        match op {
            VvcCtuCabacOp::ChromaTree {
                node,
                visible_width,
                visible_height,
            } => self.emit_chroma_tree_with_neighbours(
                cabac,
                node,
                visible_width,
                visible_height,
                luma_mode_neighbours,
                chroma_neighbours,
            ),
            other => self.emit_with_luma_mode_neighbours(cabac, other, luma_mode_neighbours),
        }
    }

    fn emit_bt_split(&mut self, cabac: &mut VvcCabacEncoder, op: VvcCtuCabacOp) {
        let VvcCtuCabacOp::BtSplit {
            node,
            vertical,
            split_ctx,
            write_split_flag,
            write_qt_flag,
            qt_ctx,
            write_mtt_vertical_flag,
            mtt_vertical_ctx,
            write_binary_flag,
            mtt_binary_ctx,
            mtt_binary_value,
        } = op
        else {
            unreachable!("emit_bt_split expects a binary split operation");
        };
        debug_assert!(node.cqt_depth >= 1 || node.mtt_depth > 0 || (node.x == 0 && node.y == 0));
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        if write_split_flag {
            self.contexts.encode_split_flag(cabac, split_ctx, true);
        }
        if write_qt_flag {
            self.contexts.encode_split_qt_flag(cabac, qt_ctx, false);
        }
        if write_mtt_vertical_flag {
            self.contexts
                .encode_mtt_split_cu_vertical_flag(cabac, mtt_vertical_ctx, vertical);
        }
        if write_binary_flag {
            self.contexts
                .encode_mtt_split_cu_binary_flag(cabac, mtt_binary_ctx, mtt_binary_value);
        }
    }

    fn emit_qt_split(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        split_ctx: u8,
        write_split_flag: bool,
        write_qt_flag: bool,
        qt_ctx: u8,
    ) {
        debug_assert!(node.cqt_depth <= 3);
        debug_assert_eq!(node.mtt_depth, 0);
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        // VVC 7.3.11.4 coding_tree emits split_cu_flag for QT-split luma
        // nodes. Some root-only geometries infer split_qt_flag, while boundary
        // constrained rectangular CTU views write it explicitly.
        if write_split_flag {
            self.contexts.encode_split_flag(cabac, split_ctx, true);
        }
        if write_qt_flag {
            self.contexts.encode_split_qt_flag(cabac, qt_ctx, true);
        }
    }

    fn emit_luma_leaf_split_with_ctx(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        write_split_flag: bool,
        split_ctx: u8,
    ) {
        debug_assert!(node.cqt_depth >= 1 || node.mtt_depth > 0 || (node.x == 0 && node.y == 0));
        debug_assert!(node.mtt_depth <= VVC_CURRENT_MAX_LUMA_MTT_DEPTH + node.depth_offset);
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        if !write_split_flag {
            return;
        }
        self.contexts.encode_split_flag(cabac, split_ctx, false);
    }

    fn emit_luma_scc_regular_intra_prefix(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
    ) {
        if self.inter_slice {
            // Mixed P-slice SCC syntax also has to order pred_mode_flag,
            // pred_mode_ibc_flag, and pred_mode_plt_flag with inter-mode
            // eligibility. Keep this preparatory hook scoped to intra slices
            // until real IBC/palette candidates are introduced there.
            return;
        }
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        if self.slice_config.tools.ibc_enabled && vvc_scc_ibc_luma_node_allowed(node) {
            self.contexts.encode_cu_skip_flag(cabac, 0, false);
            self.contexts
                .encode(cabac, VvcCabacContext::PredModeIbcFlag(0), false);
        }
        if self.slice_config.tools.palette_enabled && vvc_scc_palette_luma_node_allowed(node) {
            self.contexts
                .encode(cabac, VvcCabacContext::PredModePltFlag, false);
        }
    }

    fn emit_luma_scc_selected_leaf(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
    ) -> bool {
        let decision = self
            .params
            .luma_tu_scc_decisions
            .get(self.luma_tu_index)
            .copied()
            .unwrap_or(VvcLumaSccDecision::RegularIntra);
        match decision {
            VvcLumaSccDecision::RegularIntra => false,
            VvcLumaSccDecision::IbcExact(decision) => {
                self.emit_luma_exact_ibc_leaf(cabac, node, decision);
                true
            }
        }
    }

    fn emit_luma_exact_ibc_leaf(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        decision: VvcLumaIbcDecision,
    ) {
        assert!(
            !self.inter_slice,
            "P-slice exact IBC leaf ordering is not wired into pred_mode_flag yet"
        );
        assert!(
            self.slice_config.tools.ibc_enabled,
            "exact IBC leaf selected without SCC IBC syntax enabled"
        );
        assert!(
            vvc_scc_ibc_luma_node_allowed(node),
            "exact IBC leaf selected for unsupported luma CU size {}x{}",
            node.width,
            node.height
        );
        assert_eq!(
            node.tree_type,
            VvcTreeType::SingleTree,
            "exact IBC leaf currently requires single-tree 4:4:4 syntax"
        );
        assert_eq!(
            self.params.chroma_sampling,
            ChromaSampling::Cs444,
            "exact IBC leaf currently requires 4:4:4 syntax"
        );
        assert!(
            self.luma_tu_index < self.params.luma_tu_count,
            "missing luma TU slot for exact IBC leaf {}",
            self.luma_tu_index
        );

        self.contexts.encode_cu_skip_flag(cabac, 0, false);
        self.contexts.encode(
            cabac,
            VvcCabacContext::PredModeIbcFlag(decision.pred_mode_ibc_ctx),
            true,
        );
        self.emit_luma_exact_ibc_prediction(cabac, decision);
        // H.266 7.3.11.4/7.4.12.4: cu_coded_flag=0 means the exact-match IBC
        // CU has no transform_tree(); reconstruction is fully predicted.
        self.contexts
            .encode(cabac, VvcCabacContext::CuCodedFlag(0), false);
        self.luma_tu_index += 1;
        if self.params.chroma_sampling != ChromaSampling::Monochrome {
            assert!(
                self.chroma_tu_index < self.params.chroma_tu_count,
                "missing chroma TU slot for exact IBC leaf {}",
                self.chroma_tu_index
            );
            self.chroma_tu_index += 1;
        }
    }

    fn emit_luma_exact_ibc_prediction(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        decision: VvcLumaIbcDecision,
    ) {
        // MODE_IBC with cu_skip_flag=0 signals general_merge_flag. Use explicit
        // BVD syntax for hash-search decisions rather than selecting an inferred
        // merge vector.
        self.contexts
            .encode(cabac, VvcCabacContext::GeneralMergeFlag(0), false);
        self.emit_luma_ibc_mvd_coding(cabac, decision.mvd_x, decision.mvd_y);
        // MaxNumIbcMergeCand is fixed to one in this SPS and AMVR is disabled,
        // so mvp_l0_flag/amvr_precision_idx are inferred in the same way as the
        // existing palette/SCC scaffold.
    }

    fn emit_luma_ibc_mvd_coding(&mut self, cabac: &mut VvcCabacEncoder, mvd_x: i16, mvd_y: i16) {
        self.emit_luma_mvd_coding(cabac, i32::from(mvd_x), i32::from(mvd_y));
    }

    fn emit_luma_mvd_coding(&mut self, cabac: &mut VvcCabacEncoder, mvd_x: i32, mvd_y: i32) {
        let abs_x = i32::from(mvd_x).unsigned_abs();
        let abs_y = i32::from(mvd_y).unsigned_abs();
        self.contexts
            .encode(cabac, VvcCabacContext::AbsMvdGreater0Flag(0), abs_x > 0);
        self.contexts
            .encode(cabac, VvcCabacContext::AbsMvdGreater0Flag(0), abs_y > 0);
        if abs_x > 0 {
            self.contexts
                .encode(cabac, VvcCabacContext::AbsMvdGreater1Flag(0), abs_x > 1);
        }
        if abs_y > 0 {
            self.contexts
                .encode(cabac, VvcCabacContext::AbsMvdGreater1Flag(0), abs_y > 1);
        }
        if abs_x > 0 {
            if abs_x > 1 {
                vvc_encode_exp_golomb_ep_combined(cabac, abs_x - 2, 1);
            }
            cabac.encode_bin_ep(mvd_x < 0);
        }
        if abs_y > 0 {
            if abs_y > 1 {
                vvc_encode_exp_golomb_ep_combined(cabac, abs_y - 2, 1);
            }
            cabac.encode_bin_ep(mvd_y < 0);
        }
    }

    fn emit_luma_explicit_inter_leaf(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        neighbours: &VvcLumaModeNeighbourState,
    ) -> VvcExplicitInterLeafSyntax {
        if !self.inter_slice || self.luma_tu_index >= self.params.luma_tu_count {
            return VvcExplicitInterLeafSyntax::NotInter;
        }
        let Some(decision) = self.params.luma_tu_inter_decisions[self.luma_tu_index] else {
            return VvcExplicitInterLeafSyntax::NotInter;
        };
        assert_eq!(
            node.tree_type,
            VvcTreeType::SingleTree,
            "explicit inter leaf requires single-tree P-slice syntax"
        );
        assert!(
            !self.params.luma_tu_inter_skip[self.luma_tu_index],
            "explicit inter and inter-skip are mutually exclusive for one luma TU"
        );
        assert!(
            !self.slice_config.tools.ibc_enabled && !self.slice_config.tools.palette_enabled,
            "P-slice SCC inter-mode ordering is not wired for explicit inter leaves"
        );

        self.emit_luma_inter_slice_prediction_prefix(cabac, node, neighbours, false);
        self.contexts
            .encode(cabac, VvcCabacContext::GeneralMergeFlag(0), false);
        let candidates = self
            .inter_motion_neighbours
            .as_ref()
            .map(|neighbours| neighbours.mvp_candidates(node))
            .unwrap_or([VvcInterMotionInfo::default(); 2]);
        let mvp_choice = vvc_explicit_inter_mvp_choice_for_decision(decision, candidates);
        self.emit_luma_mvd_coding(cabac, mvp_choice.mvd_x, mvp_choice.mvd_y);
        self.contexts
            .encode_mvp_idx_flag(cabac, mvp_choice.index != 0);
        let residual = self.explicit_inter_leaf_has_residual();
        self.contexts
            .encode(cabac, VvcCabacContext::CuCodedFlag(0), residual);
        let desired = VvcInterMotionInfo::from_full_pel_decision(decision);
        if let Some(neighbours) = self.inter_motion_neighbours.as_mut() {
            neighbours.mark_leaf(node, desired);
        }
        if residual {
            return VvcExplicitInterLeafSyntax::Residual;
        }
        self.luma_tu_index += 1;
        if node.tree_type == VvcTreeType::SingleTree
            && self.params.chroma_sampling != ChromaSampling::Monochrome
        {
            self.chroma_tu_index += 1;
        }
        VvcExplicitInterLeafSyntax::NoResidual
    }

    fn explicit_inter_leaf_has_residual(&self) -> bool {
        let luma_tu_idx = self.luma_tu_index;
        let luma_residual = self.params.luma_tu_dc_levels[luma_tu_idx] != 0
            || self.params.luma_tu_has_ac[luma_tu_idx];
        if luma_residual || self.params.chroma_sampling == ChromaSampling::Monochrome {
            return luma_residual;
        }
        let chroma_tu_idx = self.chroma_tu_index;
        if chroma_tu_idx >= self.params.chroma_tu_count {
            return luma_residual;
        }
        self.params.cb_tu_dc_levels[chroma_tu_idx] != 0
            || self.params.cb_tu_has_ac[chroma_tu_idx]
            || self.params.cr_tu_dc_levels[chroma_tu_idx] != 0
            || self.params.cr_tu_has_ac[chroma_tu_idx]
    }

    fn emit_luma_inter_slice_intra_prefix(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        neighbours: &VvcLumaModeNeighbourState,
    ) {
        if !self.inter_slice {
            return;
        }
        self.emit_luma_inter_slice_prediction_prefix(cabac, node, neighbours, true);
    }

    fn emit_luma_inter_slice_prediction_prefix(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        neighbours: &VvcLumaModeNeighbourState,
        pred_mode_intra: bool,
    ) {
        debug_assert_ne!(
            (node.width, node.height),
            (4, 4),
            "4x4 intra leaves in P slices need explicit modeType handling"
        );
        let skip_ctx = self.inter_skip_ctx_for_node(node);
        self.contexts.encode_cu_skip_flag(cabac, skip_ctx, false);
        let pred_mode_ctx =
            u8::from(neighbours.left_of(node).is_some() || neighbours.above_of(node).is_some());
        let pred_mode_contexts = self
            .inter_pred_mode_contexts
            .as_mut()
            .expect("P-slice intra prefix requires pred_mode_flag contexts");
        VvcCabacContexts::encode_model(
            cabac,
            VvcCabacContext::PredModeFlag(pred_mode_ctx),
            &mut pred_mode_contexts[pred_mode_ctx as usize],
            pred_mode_intra,
        );
    }

    fn emit_luma_inter_skip_leaf(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
    ) -> bool {
        if !self.inter_slice
            || self.luma_tu_index >= self.params.luma_tu_count
            || !self.params.luma_tu_inter_skip[self.luma_tu_index]
        {
            return false;
        }
        let skip_ctx = self.inter_skip_ctx_for_node(node);
        self.contexts.encode_cu_skip_flag(cabac, skip_ctx, true);
        if let Some(neighbours) = self.inter_skip_neighbours.as_mut() {
            neighbours.mark_leaf(node);
        }
        self.luma_tu_index += 1;
        true
    }

    fn inter_skip_ctx_for_node(&self, node: VvcCodingTreeNode) -> u8 {
        self.inter_skip_neighbours
            .as_ref()
            .map(|neighbours| neighbours.skip_ctx(node))
            .unwrap_or(self.inter_skip_ctx)
            .min(2)
    }

    fn emit_luma_intra_prediction_mode(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        neighbours: &mut VvcLumaModeNeighbourState,
    ) {
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        let mode = self.params.luma_tu_intra_modes[self.luma_tu_index];
        let mode_index = mode.luma_mode_index();
        let mrl_index = self.params.luma_tu_mrl_index[self.luma_tu_index];
        let mpm = vvc_luma_mpm_list(neighbours.left_of(node), neighbours.above_of(node));
        let mpm_idx = vvc_luma_mpm_index_for_mode_index(mode_index, mpm);
        if mrl_index == 0 {
            self.contexts
                .encode_intra_luma_mpm_flag(cabac, mpm_idx.is_some());
        } else {
            assert!(
                mpm_idx.is_some(),
                "VVC nonzero MRL luma modes must be coded through MPM syntax"
            );
        }
        if let Some(mpm_idx) = mpm_idx {
            if mrl_index == 0 {
                self.contexts
                    .encode_intra_luma_planar_flag(cabac, 1, mpm_idx > 0);
            } else {
                assert_ne!(
                    mpm_idx, 0,
                    "VVC nonzero MRL cannot be combined with planar luma prediction"
                );
            }
            if mpm_idx > 0 {
                cabac.encode_bin_ep(mpm_idx > 1);
            }
            if mpm_idx > 1 {
                cabac.encode_bin_ep(mpm_idx > 2);
            }
            if mpm_idx > 2 {
                cabac.encode_bin_ep(mpm_idx > 3);
            }
            if mpm_idx > 3 {
                cabac.encode_bin_ep(mpm_idx > 4);
            }
        } else {
            assert_eq!(
                mrl_index, 0,
                "VVC remaining-mode syntax is not legal for nonzero MRL"
            );
            encode_vvc_trunc_bin_code_ep(
                cabac,
                vvc_luma_remaining_mode_index(mode_index, mpm),
                VVC_REMAINING_LUMA_MODE_COUNT,
            );
        }
        neighbours.mark_leaf(node, mode);
    }

    fn emit_luma_bdpcm_mode(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        neighbours: &mut VvcLumaModeNeighbourState,
    ) -> bool {
        if !self.luma_bdpcm_allowed(node) {
            return false;
        }
        let mode = self.params.luma_tu_bdpcm_modes[self.luma_tu_index];
        self.contexts.encode_bdpcm_mode(cabac, 0, mode.is_enabled());
        if mode.is_enabled() {
            self.contexts
                .encode_bdpcm_mode(cabac, 1, matches!(mode, VvcBdpcmMode::Vertical));
            neighbours.mark_leaf(
                node,
                mode.inferred_intra_mode()
                    .expect("enabled BDPCM mode has an inferred intra mode"),
            );
        }
        mode.is_enabled()
    }

    fn emit_luma_mip_mode(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        neighbours: &mut VvcLumaModeNeighbourState,
    ) -> bool {
        if !self.slice_config.tools.mip_enabled {
            return false;
        }
        let ctx = self.luma_mip_flag_ctx(node, neighbours);
        // Active MIP prediction still needs the matrix predictor tables and
        // syntax payload. Keep the spec flag site wired and emit no-MIP for
        // now when a gated config enables the SPS capability.
        self.contexts.encode_mip_flag(cabac, ctx, false);
        false
    }

    fn luma_mip_flag_ctx(
        &self,
        node: VvcCodingTreeNode,
        neighbours: &VvcLumaModeNeighbourState,
    ) -> u8 {
        if node.width > node.height.saturating_mul(2) || node.height > node.width.saturating_mul(2)
        {
            return 3;
        }
        u8::from(neighbours.left_mip_flag_of(node)) + u8::from(neighbours.above_mip_flag_of(node))
    }

    fn emit_luma_multi_ref_line(&mut self, cabac: &mut VvcCabacEncoder, node: VvcCodingTreeNode) {
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        // With sps_mrl_enabled_flag set, VVC extend_ref_line emits
        // MultiRefLineIdx for intra luma CUs that are not on the first luma
        // line of the CTU. VTM's MULTI_REF_LINE_IDX table is [0, 1, 2].
        if self.slice_config.tools.mrl_enabled && node.y % VVC_CTU_SIZE as u16 != 0 {
            let mrl_index = self.params.luma_tu_mrl_index[self.luma_tu_index];
            assert_eq!(
                mrl_index.min(2),
                mrl_index,
                "VVC MRL index must be one of 0, 1, or 2"
            );
            self.contexts
                .encode_multi_ref_line_idx(cabac, 0, mrl_index != 0);
            if mrl_index != 0 {
                self.contexts
                    .encode_multi_ref_line_idx(cabac, 1, mrl_index != 1);
            }
        }
    }

    fn emit_luma_isp_mode(&mut self, cabac: &mut VvcCabacEncoder, node: VvcCodingTreeNode) {
        if !self.luma_isp_allowed(node) {
            return;
        }
        // Active ISP needs split transform-tree ownership. Emit the NONE flag
        // at the VTM syntax site while the production selector remains absent.
        self.contexts.encode_isp_mode(cabac, 0, false);
    }

    fn luma_bdpcm_allowed(&self, node: VvcCodingTreeNode) -> bool {
        self.slice_config.tools.bdpcm_enabled
            && node.width <= 8
            && node.height <= 8
            && node.width >= 4
            && node.height >= 4
    }

    fn luma_isp_allowed(&self, node: VvcCodingTreeNode) -> bool {
        self.slice_config.tools.isp_enabled
            && self.params.luma_tu_mrl_index[self.luma_tu_index] == 0
            && node.width >= 4
            && node.height >= 4
            && node.width <= 64
            && node.height <= 64
            && node.width.is_power_of_two()
            && node.height.is_power_of_two()
    }

    fn emit_luma_cbf(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        cbf: bool,
        bdpcm: bool,
    ) {
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        // VVC 7.3.11.10 transform_unit emits tu_y_coded_flag / cbf_comp
        // through QtCbf[Y].
        self.contexts.encode_qt_cbf_y(cabac, u8::from(bdpcm), cbf);
    }

    fn emit_transform_unit_residual(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
    ) {
        if node.tree_type == VvcTreeType::SingleTree
            && self.params.chroma_sampling != ChromaSampling::Monochrome
        {
            self.emit_single_tree_residual(cabac, node);
        } else {
            self.emit_luma_residual(cabac, node);
        }
    }

    fn emit_luma_residual(&mut self, cabac: &mut VvcCabacEncoder, node: VvcCodingTreeNode) {
        let tu_idx = self.luma_tu_index;
        self.luma_tu_index += 1;
        assert!(
            tu_idx < self.params.luma_tu_count,
            "missing luma TU coefficient data for coding-tree leaf {tu_idx}"
        );
        let dc_level = self.params.luma_tu_dc_levels[tu_idx];
        let cbf = dc_level != 0 || self.params.luma_tu_has_ac[tu_idx];
        let bdpcm_mode = self.params.luma_tu_bdpcm_modes[tu_idx];
        self.emit_luma_cbf(cabac, node, cbf, bdpcm_mode.is_enabled());
        if !cbf {
            return;
        }

        let log2_width = node.width.ilog2() as u8;
        let log2_height = node.height.ilog2() as u8;
        let ac_levels = &self.params.luma_tu_ac_levels[tu_idx];
        let has_ac = self.params.luma_tu_has_ac[tu_idx];
        let transform_skip = self.params.luma_tu_transform_skip[tu_idx];
        let mts_index = self.params.luma_tu_mts_index[tu_idx];
        let mut residual =
            VvcResidualCabacEncoder::new(&mut *self.contexts, self.slice_config.residual_options());
        VvcResidualCabacSymbolStream::emit_luma_stored_coefficients(
            log2_width,
            log2_height,
            dc_level,
            ac_levels,
            has_ac,
            transform_skip,
            bdpcm_mode.is_enabled(),
            mts_index,
            &mut residual,
            cabac,
        );
        self.emit_luma_post_residual_tools(cabac, node, has_ac, transform_skip, mts_index);
    }

    fn emit_single_tree_chroma_prediction(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        luma_mode_neighbours: &VvcLumaModeNeighbourState,
    ) {
        if node.tree_type != VvcTreeType::SingleTree
            || self.params.chroma_sampling == ChromaSampling::Monochrome
        {
            return;
        }
        let tu_idx = self.chroma_tu_index;
        assert!(
            tu_idx < self.params.chroma_tu_count,
            "missing chroma TU prediction data for single-tree leaf {tu_idx}"
        );
        let chroma_bdpcm_mode = self.params.chroma_tu_bdpcm_modes[tu_idx];
        if !self.emit_chroma_bdpcm_mode(cabac, node, chroma_bdpcm_mode) {
            self.emit_chroma_intra_prediction_mode(cabac, node, tu_idx, luma_mode_neighbours);
        }
    }

    fn emit_single_tree_residual(&mut self, cabac: &mut VvcCabacEncoder, node: VvcCodingTreeNode) {
        let luma_tu_idx = self.luma_tu_index;
        self.luma_tu_index += 1;
        assert!(
            luma_tu_idx < self.params.luma_tu_count,
            "missing luma TU coefficient data for single-tree leaf {luma_tu_idx}"
        );
        let chroma_tu_idx = self.chroma_tu_index;
        self.chroma_tu_index += 1;
        assert!(
            chroma_tu_idx < self.params.chroma_tu_count,
            "missing chroma TU coefficient data for single-tree leaf {chroma_tu_idx}"
        );

        let chroma_bdpcm_mode = self.params.chroma_tu_bdpcm_modes[chroma_tu_idx];
        let cb_dc_level = self.params.cb_tu_dc_levels[chroma_tu_idx];
        let cr_dc_level = self.params.cr_tu_dc_levels[chroma_tu_idx];
        let cbf_cb = cb_dc_level != 0 || self.params.cb_tu_has_ac[chroma_tu_idx];
        let cbf_cr = cr_dc_level != 0 || self.params.cr_tu_has_ac[chroma_tu_idx];
        let cbf_cb_ctx = u8::from(chroma_bdpcm_mode.is_enabled());
        let cbf_cr_ctx = if chroma_bdpcm_mode.is_enabled() {
            2
        } else {
            u8::from(cbf_cb)
        };
        self.contexts.encode_qt_cbf_cb(cabac, cbf_cb_ctx, cbf_cb);
        self.contexts.encode_qt_cbf_cr(cabac, cbf_cr_ctx, cbf_cr);

        let luma_dc_level = self.params.luma_tu_dc_levels[luma_tu_idx];
        let cbf_luma = luma_dc_level != 0 || self.params.luma_tu_has_ac[luma_tu_idx];
        let luma_bdpcm_mode = self.params.luma_tu_bdpcm_modes[luma_tu_idx];
        self.emit_luma_cbf(cabac, node, cbf_luma, luma_bdpcm_mode.is_enabled());

        if cbf_luma {
            let log2_width = node.width.ilog2() as u8;
            let log2_height = node.height.ilog2() as u8;
            let luma_has_ac = self.params.luma_tu_has_ac[luma_tu_idx];
            let luma_transform_skip = self.params.luma_tu_transform_skip[luma_tu_idx];
            let luma_mts_index = self.params.luma_tu_mts_index[luma_tu_idx];
            let mut residual = VvcResidualCabacEncoder::new(
                &mut *self.contexts,
                self.slice_config.residual_options(),
            );
            VvcResidualCabacSymbolStream::emit_luma_stored_coefficients(
                log2_width,
                log2_height,
                luma_dc_level,
                &self.params.luma_tu_ac_levels[luma_tu_idx],
                luma_has_ac,
                luma_transform_skip,
                luma_bdpcm_mode.is_enabled(),
                luma_mts_index,
                &mut residual,
                cabac,
            );
        }
        if cbf_cb {
            Self::emit_chroma_residual(
                &mut *self.contexts,
                self.slice_config,
                self.params.chroma_sampling,
                cabac,
                VvcResidualComponent::ChromaCb,
                node,
                cb_dc_level,
                &self.params.cb_tu_ac_levels[chroma_tu_idx],
                self.params.cb_tu_has_ac[chroma_tu_idx],
                self.params.cb_tu_transform_skip[chroma_tu_idx],
                chroma_bdpcm_mode.is_enabled(),
            );
        }
        if cbf_cr {
            Self::emit_chroma_residual(
                &mut *self.contexts,
                self.slice_config,
                self.params.chroma_sampling,
                cabac,
                VvcResidualComponent::ChromaCr,
                node,
                cr_dc_level,
                &self.params.cr_tu_ac_levels[chroma_tu_idx],
                self.params.cr_tu_has_ac[chroma_tu_idx],
                self.params.cr_tu_transform_skip[chroma_tu_idx],
                chroma_bdpcm_mode.is_enabled(),
            );
        }
        if cbf_luma {
            self.emit_luma_post_residual_tools(
                cabac,
                node,
                self.params.luma_tu_has_ac[luma_tu_idx],
                self.params.luma_tu_transform_skip[luma_tu_idx],
                self.params.luma_tu_mts_index[luma_tu_idx],
            );
        }
    }

    fn emit_luma_post_residual_tools(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        has_ac: bool,
        transform_skip: bool,
        mts_index: u8,
    ) {
        self.emit_luma_lfnst_idx(cabac, node, has_ac, transform_skip, mts_index);
        self.emit_luma_mts_idx(cabac, node, has_ac, transform_skip, mts_index);
    }

    fn emit_luma_lfnst_idx(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        has_ac: bool,
        transform_skip: bool,
        mts_index: u8,
    ) {
        if !self.slice_config.tools.lfnst_enabled
            || transform_skip
            || mts_index != 0
            || !has_ac
            || node.width > 64
            || node.height > 64
        {
            return;
        }
        // Active LFNST needs transform-domain candidate ownership and
        // coefficient-group constraints. Keep the syntax site wired and emit
        // lfnst_idx=0 while production selection is absent.
        self.contexts.encode_lfnst_idx(cabac, 0, false);
    }

    fn emit_luma_mts_idx(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        has_ac: bool,
        transform_skip: bool,
        mts_index: u8,
    ) {
        if !self.slice_config.tools.explicit_mts_intra_enabled
            || transform_skip
            || !has_ac
            || node.width > 32
            || node.height > 32
        {
            return;
        }
        assert!(
            matches!(mts_index, 0 | 2..=5),
            "VVC MTS index must be DCT2_DCT2 or one of the explicit MTS transform types"
        );

        // H.266 cu_residual() writes mts_idx after the transform tree. The
        // current selector still chooses DCT2_DCT2, but keep the VTM-shaped
        // syntax ready for later non-default transform candidates.
        self.contexts.encode_mts_idx(cabac, 0, mts_index != 0);
        if mts_index != 0 {
            for offset in 0..3 {
                let bin = mts_index > 2 + offset;
                self.contexts.encode_mts_idx(cabac, 1 + offset, bin);
                if !bin {
                    break;
                }
            }
        }
    }

    fn emit_chroma_tree(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
        luma_mode_neighbours: &VvcLumaModeNeighbourState,
    ) {
        debug_assert_eq!(node.tree_type, VvcTreeType::DualTreeChroma);
        let mut neighbours = VvcChromaNeighbourState::new(
            visible_width,
            visible_height,
            self.params.chroma_sampling,
        );
        self.emit_chroma_tree_with_neighbours(
            cabac,
            node,
            visible_width,
            visible_height,
            luma_mode_neighbours,
            &mut neighbours,
        );
    }

    fn emit_chroma_tree_with_neighbours(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
        luma_mode_neighbours: &VvcLumaModeNeighbourState,
        neighbours: &mut VvcChromaNeighbourState,
    ) {
        debug_assert_eq!(node.tree_type, VvcTreeType::DualTreeChroma);
        self.emit_chroma_visible_qt_subtree(
            cabac,
            node,
            visible_width,
            visible_height,
            4,
            luma_mode_neighbours,
            neighbours,
        );
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

    fn emit_chroma_residual(
        contexts: &mut VvcCabacContexts,
        slice_config: VvcSliceSyntaxConfig,
        chroma_sampling: ChromaSampling,
        cabac: &mut VvcCabacEncoder,
        component: VvcResidualComponent,
        node: VvcCodingTreeNode,
        dc_level: i16,
        ac_levels: &[i16; VVC_CHROMA_AC_COEFFS_PER_TU],
        has_ac: bool,
        transform_skip: bool,
        bdpcm: bool,
    ) {
        let width = usize::from(vvc_chroma_width(node, chroma_sampling));
        let height = usize::from(vvc_chroma_height(node, chroma_sampling));
        let log2_width = (width as u16).ilog2() as u8;
        let log2_height = (height as u16).ilog2() as u8;
        let mut residual = VvcResidualCabacEncoder::new(contexts, slice_config.residual_options());
        VvcResidualCabacSymbolStream::emit_chroma_stored_coefficients(
            component,
            log2_width,
            log2_height,
            dc_level,
            ac_levels,
            has_ac,
            transform_skip,
            bdpcm,
            &mut residual,
            cabac,
        );
    }

    fn emit_chroma_transform_only_leaf(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        cbf_cb_ctx: u8,
        luma_mode_neighbours: &VvcLumaModeNeighbourState,
        neighbours: &mut VvcChromaNeighbourState,
    ) {
        debug_assert_eq!(node.tree_type, VvcTreeType::DualTreeChroma);
        if split.can_no && split.can_split() {
            self.contexts.encode_split_flag(
                cabac,
                Self::chroma_split_ctx(node, split, neighbours),
                false,
            );
        }
        let tu_idx = self.chroma_tu_index;
        assert!(
            tu_idx < self.params.chroma_tu_count,
            "missing chroma TU coefficient data for coding-tree leaf {tu_idx}"
        );
        if self.inter_slice && self.chroma_inter_skip_active {
            let chroma_inter_skip = self.params.chroma_tu_inter_skip[tu_idx];
            let skip_ctx = self.inter_skip_ctx_for_node(node);
            self.contexts
                .encode_cu_skip_flag(cabac, skip_ctx, chroma_inter_skip);
            if chroma_inter_skip {
                if let Some(neighbours) = self.inter_skip_neighbours.as_mut() {
                    neighbours.mark_leaf(node);
                }
                self.chroma_tu_index += 1;
                return;
            }
        }
        let chroma_bdpcm_mode = self.params.chroma_tu_bdpcm_modes[tu_idx];
        if !self.emit_chroma_bdpcm_mode(cabac, node, chroma_bdpcm_mode) {
            self.emit_chroma_intra_prediction_mode(cabac, node, tu_idx, luma_mode_neighbours);
        }
        self.chroma_tu_index += 1;
        let cb_dc_level = self.params.cb_tu_dc_levels[tu_idx];
        let cr_dc_level = self.params.cr_tu_dc_levels[tu_idx];
        let cbf_cb = cb_dc_level != 0 || self.params.cb_tu_has_ac[tu_idx];
        let cbf_cr = cr_dc_level != 0 || self.params.cr_tu_has_ac[tu_idx];
        let cbf_cb_ctx = if chroma_bdpcm_mode.is_enabled() {
            1
        } else {
            cbf_cb_ctx
        };
        let cbf_cr_ctx = if chroma_bdpcm_mode.is_enabled() {
            2
        } else {
            u8::from(cbf_cb)
        };
        self.contexts.encode_qt_cbf_cb(cabac, cbf_cb_ctx, cbf_cb);
        self.contexts.encode_qt_cbf_cr(cabac, cbf_cr_ctx, cbf_cr);
        if cbf_cb {
            Self::emit_chroma_residual(
                &mut *self.contexts,
                self.slice_config,
                self.params.chroma_sampling,
                cabac,
                VvcResidualComponent::ChromaCb,
                node,
                cb_dc_level,
                &self.params.cb_tu_ac_levels[tu_idx],
                self.params.cb_tu_has_ac[tu_idx],
                self.params.cb_tu_transform_skip[tu_idx],
                chroma_bdpcm_mode.is_enabled(),
            );
        }
        if cbf_cr {
            Self::emit_chroma_residual(
                &mut *self.contexts,
                self.slice_config,
                self.params.chroma_sampling,
                cabac,
                VvcResidualComponent::ChromaCr,
                node,
                cr_dc_level,
                &self.params.cr_tu_ac_levels[tu_idx],
                self.params.cr_tu_has_ac[tu_idx],
                self.params.cr_tu_transform_skip[tu_idx],
                chroma_bdpcm_mode.is_enabled(),
            );
        }
        neighbours.mark_leaf(node);
    }

    fn emit_chroma_bdpcm_mode(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        mode: VvcBdpcmMode,
    ) -> bool {
        if !self.chroma_bdpcm_allowed(node) {
            return false;
        }
        self.contexts.encode_bdpcm_mode(cabac, 2, mode.is_enabled());
        if mode.is_enabled() {
            self.contexts
                .encode_bdpcm_mode(cabac, 3, matches!(mode, VvcBdpcmMode::Vertical));
        }
        mode.is_enabled()
    }

    fn emit_chroma_intra_prediction_mode(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        tu_idx: usize,
        luma_mode_neighbours: &VvcLumaModeNeighbourState,
    ) {
        let mode = self.params.chroma_tu_intra_modes[tu_idx];
        if self.chroma_cclm_enabled(node) {
            let cclm_mode = match mode {
                VvcChromaIntraPredictionMode::Cclm(cclm_mode) => Some(cclm_mode),
                _ => None,
            };
            self.contexts
                .encode_cclm_mode_flag(cabac, cclm_mode.is_some());
            if let Some(cclm_mode) = cclm_mode {
                let symbol = match cclm_mode {
                    VvcChromaCclmMode::Linear => 0,
                    VvcChromaCclmMode::MdlmLeft => 1,
                    VvcChromaCclmMode::MdlmTop => 2,
                };
                self.contexts.encode_cclm_mode_idx(cabac, symbol != 0);
                if symbol > 0 {
                    cabac.encode_bin_ep(symbol == 2);
                }
                return;
            }
        }
        match mode {
            VvcChromaIntraPredictionMode::Derived => {
                self.contexts.encode_intra_chroma_pred_mode(cabac, 0, false);
            }
            VvcChromaIntraPredictionMode::Explicit(mode) => {
                let co_located_luma_mode = luma_mode_neighbours
                    .co_located_for_chroma(node)
                    .unwrap_or(VvcIntraPredictionMode::Dc);
                let Some(candidate_index) =
                    vvc_chroma_explicit_candidate_index(mode, co_located_luma_mode)
                else {
                    assert_eq!(
                        mode, co_located_luma_mode,
                        "selected VVC chroma explicit mode must be derived or in the candidate table"
                    );
                    self.contexts.encode_intra_chroma_pred_mode(cabac, 0, false);
                    return;
                };
                self.contexts.encode_intra_chroma_pred_mode(cabac, 0, true);
                cabac.encode_bins_ep(u32::from(candidate_index), 2);
            }
            VvcChromaIntraPredictionMode::Cclm(_) => {
                debug_assert!(
                    false,
                    "selected VVC CCLM mode for a node where CCLM is not signaled"
                );
                self.contexts.encode_intra_chroma_pred_mode(cabac, 0, false);
            }
        }
    }

    fn chroma_leaf_allowed(&self, node: VvcCodingTreeNode) -> bool {
        let chroma_width = vvc_chroma_width(node, self.params.chroma_sampling);
        let chroma_height = vvc_chroma_height(node, self.params.chroma_sampling);
        // H.266 7.3.11.10 transform_unit() is reached after the encoder's
        // chosen legal coding-tree split. The spec maximum for this SPS remains
        // MaxTbSizeY/SubWidthC by MaxTbSizeY/SubHeightC, but this hardware
        // residual subset chooses 8x8 luma-coordinate leaves so each 4:2:0
        // chroma TU is 4x4 samples and shares the luma TU cadence.
        chroma_width <= VVC_CURRENT_ENCODER_CHROMA_420_TB_SIZE
            && chroma_height <= VVC_CURRENT_ENCODER_CHROMA_420_TB_SIZE
    }

    fn chroma_cclm_enabled(&self, node: VvcCodingTreeNode) -> bool {
        if !self.slice_config.tools.cclm_enabled {
            return false;
        }
        vvc_chroma_cclm_node_allowed(node)
    }

    fn chroma_bdpcm_allowed(&self, node: VvcCodingTreeNode) -> bool {
        if !self.slice_config.tools.bdpcm_enabled {
            return false;
        }
        let chroma_width = vvc_chroma_width(node, self.params.chroma_sampling);
        let chroma_height = vvc_chroma_height(node, self.params.chroma_sampling);
        chroma_width <= 8 && chroma_height <= 8 && chroma_width >= 4 && chroma_height >= 4
    }

    fn chroma_split_ctx(
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        neighbours: &VvcChromaNeighbourState,
    ) -> u8 {
        // H.266 9.3.4.2.2 Table 133 derives chroma split_cu_flag condL from
        // the left chroma CU height being smaller than the current chroma CU
        // height, and condA from the above chroma CU width being smaller.
        let left = neighbours.left_of(node);
        let above = neighbours.above_of(node);
        VvcSplitCtxInput {
            available_left: left.is_some(),
            available_above: above.is_some(),
            condition_left: left.is_some_and(|info| info.cb_height < neighbours.node_height(node)),
            condition_above: above.is_some_and(|info| info.cb_width < neighbours.node_width(node)),
            allow_bt_vertical: split.allow_bt_vertical,
            allow_bt_horizontal: split.allow_bt_horizontal,
            allow_tt_vertical: split.allow_tt_vertical,
            allow_tt_horizontal: split.allow_tt_horizontal,
            allow_qt: split.allow_qt,
        }
        .split_cu_flag_ctx()
    }

    fn chroma_qt_split_ctx(node: VvcCodingTreeNode, neighbours: &VvcChromaNeighbourState) -> u8 {
        // H.266 9.3.4.2.2 Table 133 derives split_qt_flag condL/condA from
        // neighbouring chroma CqtDepth being greater than the current depth.
        let left = neighbours.left_of(node);
        let above = neighbours.above_of(node);
        VvcQtSplitCtxInput {
            available_left: left.is_some(),
            available_above: above.is_some(),
            left_deeper_qt: left.is_some_and(|info| info.cqt_depth > node.cqt_depth),
            above_deeper_qt: above.is_some_and(|info| info.cqt_depth > node.cqt_depth),
            cqt_depth: node.cqt_depth,
        }
        .split_qt_flag_ctx()
    }

    fn chroma_mtt_vertical_ctx(
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        neighbours: &VvcChromaNeighbourState,
    ) -> u8 {
        // H.266 9.3.4.2.3 first compares vertical-vs-horizontal BT/TT choices.
        // If tied, it uses the above chroma CU width and left chroma CU height.
        let vertical_choices =
            u8::from(split.allow_bt_vertical) + u8::from(split.allow_tt_vertical);
        let horizontal_choices =
            u8::from(split.allow_bt_horizontal) + u8::from(split.allow_tt_horizontal);
        if vertical_choices > horizontal_choices {
            return 4;
        }
        if vertical_choices < horizontal_choices {
            return 3;
        }
        let Some(above) = neighbours.above_of(node) else {
            return 0;
        };
        let Some(left) = neighbours.left_of(node) else {
            return 0;
        };
        let d_a = neighbours.node_width(node) / above.cb_width.max(1);
        let d_l = neighbours.node_height(node) / left.cb_height.max(1);
        if d_a == d_l {
            0
        } else if d_a < d_l {
            1
        } else {
            2
        }
    }

    fn chroma_prefer_vertical_bt(
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
    ) -> bool {
        if !split.allow_bt_vertical {
            return false;
        }
        if !split.allow_bt_horizontal {
            return true;
        }
        node.width >= node.height
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
