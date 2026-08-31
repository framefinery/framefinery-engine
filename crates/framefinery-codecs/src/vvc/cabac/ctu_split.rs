use crate::picture::ChromaSampling;
use crate::vvc::{
    chroma_subsample_x, chroma_subsample_y, VvcBdpcmMode, VvcChromaIntraPredictionMode,
    VvcIntraPredictionMode, VvcLumaInterDecision, VvcLumaSccDecision, MAX_VVC_CHROMA_TUS,
    MAX_VVC_LUMA_TUS, VVC_CHROMA_AC_COEFFS_PER_TU, VVC_CTU_SIZE,
    VVC_CURRENT_ENCODER_CHROMA_420_TB_SIZE, VVC_CURRENT_MAX_CHROMA_420_BT_SIZE,
    VVC_CURRENT_MAX_CHROMA_420_MTT_DEPTH, VVC_CURRENT_MAX_CHROMA_420_TT_SIZE,
    VVC_CURRENT_MAX_LUMA_BT_SIZE, VVC_CURRENT_MAX_LUMA_MTT_DEPTH, VVC_CURRENT_MAX_LUMA_TT_SIZE,
    VVC_CURRENT_MIN_CHROMA_420_QT_SIZE, VVC_CURRENT_MIN_LUMA_CB_SIZE, VVC_CURRENT_MIN_LUMA_QT_SIZE,
    VVC_LUMA_AC_COEFFS_PER_TU,
};

include!("ctu_partition_geometry.rs");

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::vvc) struct VvcCtuPartitionParams {
    pub(in crate::vvc) root_width: usize,
    pub(in crate::vvc) root_height: usize,
    pub(in crate::vvc) visible_width: usize,
    pub(in crate::vvc) visible_height: usize,
    pub(in crate::vvc) chroma_sampling: ChromaSampling,
    pub(in crate::vvc) dual_tree_intra: bool,
    pub(in crate::vvc) luma_split_kind: VvcLumaSplitAvailabilityKind,
    pub(in crate::vvc) luma_max_leaf_size: u16,
    pub(in crate::vvc) chroma_tu_count: usize,
    pub(in crate::vvc) luma_tu_count: usize,
    pub(in crate::vvc) luma_tu_intra_modes: [VvcIntraPredictionMode; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_abs_levels: [u8; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_negative: [bool; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_dc_levels: [i16; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_ac_levels: [[i16; VVC_LUMA_AC_COEFFS_PER_TU]; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_has_ac: [bool; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_inter_decisions: [Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_scc_decisions: [VvcLumaSccDecision; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_transform_skip: [bool; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_bdpcm_modes: [VvcBdpcmMode; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_mrl_index: [u8; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) luma_tu_mts_index: [u8; MAX_VVC_LUMA_TUS],
    pub(in crate::vvc) cb_dc_abs_level: u8,
    pub(in crate::vvc) cb_dc_negative: bool,
    pub(in crate::vvc) chroma_tu_intra_modes: [VvcChromaIntraPredictionMode; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cb_tu_dc_levels: [i16; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cr_tu_dc_levels: [i16; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cb_tu_ac_levels: [[i16; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cr_tu_ac_levels: [[i16; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cb_tu_has_ac: [bool; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cr_tu_has_ac: [bool; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cb_tu_transform_skip: [bool; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) cr_tu_transform_skip: [bool; MAX_VVC_CHROMA_TUS],
    pub(in crate::vvc) chroma_tu_bdpcm_modes: [VvcBdpcmMode; MAX_VVC_CHROMA_TUS],
}

impl VvcCtuPartitionParams {
    pub(in crate::vvc) fn shape(&self) -> VvcCtuPartitionShape {
        VvcCtuPartitionShape {
            root_width: self.root_width as u16,
            root_height: self.root_height as u16,
            visible_width: self.visible_width as u16,
            visible_height: self.visible_height as u16,
            chroma_sampling: self.chroma_sampling,
            dual_tree_intra: self.dual_tree_intra,
        }
    }

    pub(in crate::vvc) fn single_tree_shape(&self) -> VvcCtuPartitionShape {
        let mut shape = self.shape();
        shape.dual_tree_intra = false;
        shape
    }

    #[cfg(test)]
    pub(in crate::vvc) fn visible_chroma_width(&self) -> u16 {
        // coding_tree() uses luma-coordinate cbWidth/cbHeight even for
        // DUAL_TREE_CHROMA. Chroma subsampling is applied by chroma syntax and
        // transform decisions below the tree, not by shrinking the tree root.
        self.visible_width as u16
    }

    #[cfg(test)]
    pub(in crate::vvc) fn visible_chroma_height(&self) -> u16 {
        self.visible_height as u16
    }

    #[cfg(test)]
    pub(in crate::vvc) fn ctu_chroma_root(&self) -> VvcCodingTreeNode {
        VvcCodingTreeNode::root(
            self.root_width as u16,
            self.root_height as u16,
            VvcTreeType::DualTreeChroma,
        )
    }
}

pub(in crate::vvc) fn vvc_chroma_transform_nodes(
    shape: VvcCtuPartitionShape,
) -> Vec<VvcCodingTreeNode> {
    let mut nodes = Vec::new();
    vvc_chroma_transform_nodes_into(&mut nodes, shape);
    nodes
}

pub(in crate::vvc) fn vvc_chroma_transform_nodes_into(
    nodes: &mut Vec<VvcCodingTreeNode>,
    shape: VvcCtuPartitionShape,
) {
    nodes.clear();
    visit_visible_chroma_partition(
        VvcCodingTreeNode::root(
            shape.root_width,
            shape.root_height,
            VvcTreeType::DualTreeChroma,
        ),
        shape.visible_width,
        shape.visible_height,
        shape.chroma_sampling,
        &mut |event| {
            if let VvcChromaPartitionEvent::Leaf { node, .. } = event {
                nodes.push(node);
            }
        },
    );
}

#[cfg(any(test, feature = "bench-internals"))]
pub(in crate::vvc) fn vvc_luma_transform_nodes(
    shape: VvcCtuPartitionShape,
    max_leaf_size: u16,
) -> Vec<VvcCodingTreeNode> {
    vvc_luma_transform_nodes_for_kind(shape, max_leaf_size, VvcLumaSplitAvailabilityKind::Intra)
}

pub(in crate::vvc) fn vvc_luma_transform_nodes_for_kind(
    shape: VvcCtuPartitionShape,
    max_leaf_size: u16,
    split_kind: VvcLumaSplitAvailabilityKind,
) -> Vec<VvcCodingTreeNode> {
    let mut nodes = Vec::new();
    vvc_luma_transform_nodes_into_for_kind(&mut nodes, shape, max_leaf_size, split_kind);
    nodes
}

#[cfg(any(test, feature = "bench-internals"))]
pub(in crate::vvc) fn vvc_luma_transform_nodes_into(
    nodes: &mut Vec<VvcCodingTreeNode>,
    shape: VvcCtuPartitionShape,
    max_leaf_size: u16,
) {
    vvc_luma_transform_nodes_into_for_kind(
        nodes,
        shape,
        max_leaf_size,
        VvcLumaSplitAvailabilityKind::Intra,
    );
}

pub(in crate::vvc) fn vvc_luma_transform_nodes_into_for_kind(
    nodes: &mut Vec<VvcCodingTreeNode>,
    shape: VvcCtuPartitionShape,
    max_leaf_size: u16,
    split_kind: VvcLumaSplitAvailabilityKind,
) {
    nodes.clear();
    let tree_type = vvc_luma_tree_type(shape);
    visit_visible_luma_partition(
        VvcCodingTreeNode::root(shape.root_width, shape.root_height, tree_type),
        shape.visible_width,
        shape.visible_height,
        max_leaf_size,
        split_kind,
        &mut |event| {
            if let VvcLumaPartitionEvent::Leaf { node, .. } = event {
                nodes.push(node);
            }
        },
    );
}

include!("ctu_luma_partition.rs");
include!("ctu_chroma_partition.rs");

fn chroma_leaf_allowed(node: VvcCodingTreeNode, chroma_sampling: ChromaSampling) -> bool {
    let chroma_width = vvc_chroma_width(node, chroma_sampling);
    let chroma_height = vvc_chroma_height(node, chroma_sampling);
    // H.266 7.3.11.10 permits transform_unit() after any legal coding-tree
    // leaf. The current residual hardware deliberately chooses legal splits
    // down to 8x8 luma-coordinate leaves, which are 4x4 chroma TUs in 4:2:0.
    // Split availability below still uses the SPS-derived max chroma TB size.
    chroma_width <= VVC_CURRENT_ENCODER_CHROMA_420_TB_SIZE
        && chroma_height <= VVC_CURRENT_ENCODER_CHROMA_420_TB_SIZE
}

pub(in crate::vvc) fn vvc_chroma_width(
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
) -> u16 {
    node.width / chroma_subsample_x(chroma_sampling) as u16
}

pub(in crate::vvc) fn vvc_chroma_height(
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
) -> u16 {
    node.height / chroma_subsample_y(chroma_sampling) as u16
}

fn chroma_implicit_split(
    node: VvcCodingTreeNode,
    visible_width: u16,
    visible_height: u16,
) -> VvcPartSplit {
    if node.fits_visible(visible_width, visible_height) {
        return VvcPartSplit::None;
    }

    let bottom_left_in_pic = node.x < visible_width && node.y + node.height - 1 < visible_height;
    let top_right_in_pic = node.x + node.width - 1 < visible_width && node.y < visible_height;
    let max_mtt_depth = VVC_CURRENT_MAX_CHROMA_420_MTT_DEPTH + node.depth_offset;
    let bt_allowed = node.width <= VVC_CURRENT_MAX_CHROMA_420_BT_SIZE
        && node.height <= VVC_CURRENT_MAX_CHROMA_420_BT_SIZE
        && node.mtt_depth < max_mtt_depth;
    let qt_allowed = node.width > VVC_CURRENT_MIN_CHROMA_420_QT_SIZE
        && node.height > VVC_CURRENT_MIN_CHROMA_420_QT_SIZE
        && node.mtt_depth == 0;

    // H.266 7.4.12.4 infers split_cu_flag for picture-boundary CUs. VTM's
    // QTBTPartitioner::getImplicitSplit implements the same 6.4.1/6.4.2
    // order: prefer QT when both BL/TR are outside and QT is legal, otherwise
    // use a boundary BT on the out-of-picture axis when available, with QT as
    // the fallback for any remaining boundary node.
    if !bottom_left_in_pic && !top_right_in_pic && qt_allowed {
        VvcPartSplit::Quad
    } else if !bottom_left_in_pic && bt_allowed && node.width <= VVC_CTU_SIZE as u16 {
        VvcPartSplit::HorizontalBinary
    } else if !top_right_in_pic && bt_allowed && node.height <= VVC_CTU_SIZE as u16 {
        VvcPartSplit::VerticalBinary
    } else if node.width > VVC_CTU_SIZE as u16 || node.height > VVC_CTU_SIZE as u16 {
        VvcPartSplit::Quad
    } else if !bottom_left_in_pic || !top_right_in_pic {
        VvcPartSplit::Quad
    } else {
        VvcPartSplit::None
    }
}

pub(in crate::vvc) fn vvc_chroma_split_availability(
    node: VvcCodingTreeNode,
    visible_width: u16,
    visible_height: u16,
    chroma_sampling: ChromaSampling,
) -> VvcChromaSplitAvailability {
    let chroma_width = vvc_chroma_width(node, chroma_sampling);
    let chroma_height = vvc_chroma_height(node, chroma_sampling);
    let chroma_area = chroma_width * chroma_height;
    let implicit_split = chroma_implicit_split(node, visible_width, visible_height);
    let max_mtt_depth = VVC_CURRENT_MAX_CHROMA_420_MTT_DEPTH + node.depth_offset;
    let mut can_no = true;
    let mut allow_qt =
        node.parent_split == VvcPartSplit::None || node.parent_split == VvcPartSplit::Quad;
    allow_qt &= node.width > VVC_CURRENT_MIN_CHROMA_420_QT_SIZE;
    allow_qt &= chroma_width > 4;

    // H.266 6.4.2/6.4.3 derive chroma MTT availability from the SPS chroma
    // MTT depth plus the implicit-boundary BT depthOffset carried by the node.
    // The size checks are in luma coordinates except for the dual-tree 4:2:0
    // chroma area/width guards.
    let mut can_btt = node.mtt_depth < max_mtt_depth;
    if can_btt
        && node.width <= VVC_CURRENT_MIN_LUMA_CB_SIZE
        && node.height <= VVC_CURRENT_MIN_LUMA_CB_SIZE
    {
        can_btt = false;
    }
    if can_btt
        && node.width > VVC_CURRENT_MAX_CHROMA_420_BT_SIZE
        && node.height > VVC_CURRENT_MAX_CHROMA_420_BT_SIZE
        && node.width > VVC_CURRENT_MAX_CHROMA_420_TT_SIZE
        && node.height > VVC_CURRENT_MAX_CHROMA_420_TT_SIZE
    {
        can_btt = false;
    }

    let mut allow_bt_horizontal = true;
    let mut allow_bt_vertical = true;
    let mut allow_tt_horizontal = true;
    let mut allow_tt_vertical = true;

    if implicit_split != VvcPartSplit::None {
        can_no = false;
        allow_tt_horizontal = false;
        allow_tt_vertical = false;
        allow_bt_horizontal = implicit_split == VvcPartSplit::HorizontalBinary;
        allow_bt_vertical = implicit_split == VvcPartSplit::VerticalBinary;
        if chroma_width == 4 {
            allow_bt_vertical = false;
        }
        if !allow_bt_horizontal && !allow_bt_vertical && !allow_qt {
            allow_qt = true;
        }
        return VvcChromaSplitAvailability {
            can_no,
            allow_qt,
            allow_bt_vertical,
            allow_bt_horizontal,
            allow_tt_vertical,
            allow_tt_horizontal,
            implicit_split,
        };
    }

    if !can_btt {
        allow_bt_horizontal = false;
        allow_bt_vertical = false;
        allow_tt_horizontal = false;
        allow_tt_vertical = false;
    }

    if node.width > VVC_CURRENT_MAX_CHROMA_420_BT_SIZE
        || node.height > VVC_CURRENT_MAX_CHROMA_420_BT_SIZE
    {
        allow_bt_horizontal = false;
        allow_bt_vertical = false;
    }
    if node.height <= VVC_CURRENT_MIN_LUMA_CB_SIZE
        || (node.width > VVC_CTU_SIZE as u16 && node.height <= VVC_CTU_SIZE as u16)
        || chroma_area <= 16
    {
        allow_bt_horizontal = false;
    }
    if node.width <= VVC_CURRENT_MIN_LUMA_CB_SIZE
        || (node.width <= VVC_CTU_SIZE as u16 && node.height > VVC_CTU_SIZE as u16)
        || chroma_area <= 16
        || chroma_width == 4
    {
        allow_bt_vertical = false;
    }

    if node.height <= 2 * VVC_CURRENT_MIN_LUMA_CB_SIZE
        || node.height > VVC_CURRENT_MAX_CHROMA_420_TT_SIZE
        || node.width > VVC_CURRENT_MAX_CHROMA_420_TT_SIZE
        || node.width > VVC_CTU_SIZE as u16
        || node.height > VVC_CTU_SIZE as u16
        || chroma_area <= 32
    {
        allow_tt_horizontal = false;
    }
    if node.width <= 2 * VVC_CURRENT_MIN_LUMA_CB_SIZE
        || node.width > VVC_CURRENT_MAX_CHROMA_420_TT_SIZE
        || node.height > VVC_CURRENT_MAX_CHROMA_420_TT_SIZE
        || node.width > VVC_CTU_SIZE as u16
        || node.height > VVC_CTU_SIZE as u16
        || chroma_area <= 32
        || chroma_width == 8
    {
        allow_tt_vertical = false;
    }

    VvcChromaSplitAvailability {
        can_no,
        allow_qt,
        allow_bt_vertical,
        allow_bt_horizontal,
        allow_tt_vertical,
        allow_tt_horizontal,
        implicit_split,
    }
}

fn chroma_prefer_vertical_bt(node: VvcCodingTreeNode, split: VvcChromaSplitAvailability) -> bool {
    if !split.allow_bt_vertical {
        return false;
    }
    if !split.allow_bt_horizontal {
        return true;
    }
    // H.266 6.4.1 supplies the available BT directions; this encoder's
    // residual policy chooses legal BT splits that drive both axes toward the
    // 8x8 luma-coordinate leaf used by the current hardware datapath.
    node.width >= node.height
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcChromaSplitAvailability {
    pub(in crate::vvc) can_no: bool,
    pub(in crate::vvc) allow_qt: bool,
    pub(in crate::vvc) allow_bt_vertical: bool,
    pub(in crate::vvc) allow_bt_horizontal: bool,
    pub(in crate::vvc) allow_tt_vertical: bool,
    pub(in crate::vvc) allow_tt_horizontal: bool,
    pub(in crate::vvc) implicit_split: VvcPartSplit,
}

impl VvcChromaSplitAvailability {
    pub(in crate::vvc) fn can_split(self) -> bool {
        self.allow_qt || self.allow_btt()
    }

    pub(in crate::vvc) fn allow_btt(self) -> bool {
        self.allow_bt_vertical
            || self.allow_bt_horizontal
            || self.allow_tt_vertical
            || self.allow_tt_horizontal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcLumaNeighbourInfo {
    cb_width: u16,
    cb_height: u16,
    cqt_depth: u8,
}

const VVC_LUMA_NEIGHBOUR_CELL_SIZE: u16 = VVC_CURRENT_MIN_LUMA_CB_SIZE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::vvc) struct VvcLumaNeighbourState {
    width: u16,
    height: u16,
    cell_width: usize,
    valid: Vec<bool>,
    cb_width: Vec<u16>,
    cb_height: Vec<u16>,
    cqt_depth: Vec<u8>,
}

impl VvcLumaNeighbourState {
    pub(in crate::vvc) fn new(width: u16, height: u16) -> Self {
        let cell_width = usize::from(width.div_ceil(VVC_LUMA_NEIGHBOUR_CELL_SIZE));
        let cell_height = usize::from(height.div_ceil(VVC_LUMA_NEIGHBOUR_CELL_SIZE));
        let cells = cell_width * cell_height;
        Self {
            width,
            height,
            cell_width,
            valid: vec![false; cells],
            cb_width: vec![0; cells],
            cb_height: vec![0; cells],
            cqt_depth: vec![0; cells],
        }
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let cell_x = usize::from(x / VVC_LUMA_NEIGHBOUR_CELL_SIZE);
        let cell_y = usize::from(y / VVC_LUMA_NEIGHBOUR_CELL_SIZE);
        Some(cell_y * self.cell_width + cell_x)
    }

    fn info_at(&self, x: u16, y: u16) -> Option<VvcLumaNeighbourInfo> {
        let index = self.index(x, y)?;
        self.valid[index].then_some(VvcLumaNeighbourInfo {
            cb_width: self.cb_width[index],
            cb_height: self.cb_height[index],
            cqt_depth: self.cqt_depth[index],
        })
    }

    fn left_of(&self, node: VvcCodingTreeNode) -> Option<VvcLumaNeighbourInfo> {
        node.x.checked_sub(1).and_then(|x| self.info_at(x, node.y))
    }

    fn above_of(&self, node: VvcCodingTreeNode) -> Option<VvcLumaNeighbourInfo> {
        node.y.checked_sub(1).and_then(|y| self.info_at(node.x, y))
    }

    fn mark_leaf(&mut self, node: VvcCodingTreeNode) {
        let end_x = (node.x + node.width).min(self.width);
        let end_y = (node.y + node.height).min(self.height);
        let start_cell_x = node.x / VVC_LUMA_NEIGHBOUR_CELL_SIZE;
        let start_cell_y = node.y / VVC_LUMA_NEIGHBOUR_CELL_SIZE;
        let end_cell_x = end_x.div_ceil(VVC_LUMA_NEIGHBOUR_CELL_SIZE);
        let end_cell_y = end_y.div_ceil(VVC_LUMA_NEIGHBOUR_CELL_SIZE);
        for cell_y in start_cell_y..end_cell_y {
            let start = usize::from(cell_y) * self.cell_width + usize::from(start_cell_x);
            let end = usize::from(cell_y) * self.cell_width + usize::from(end_cell_x);
            self.valid[start..end].fill(true);
            self.cb_width[start..end].fill(node.width);
            self.cb_height[start..end].fill(node.height);
            self.cqt_depth[start..end].fill(node.cqt_depth);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcSplitCtxInput {
    pub(in crate::vvc) available_left: bool,
    pub(in crate::vvc) available_above: bool,
    pub(in crate::vvc) condition_left: bool,
    pub(in crate::vvc) condition_above: bool,
    pub(in crate::vvc) allow_bt_vertical: bool,
    pub(in crate::vvc) allow_bt_horizontal: bool,
    pub(in crate::vvc) allow_tt_vertical: bool,
    pub(in crate::vvc) allow_tt_horizontal: bool,
    pub(in crate::vvc) allow_qt: bool,
}

impl VvcSplitCtxInput {
    fn has_mtt(self) -> bool {
        self.allow_bt_vertical
            || self.allow_bt_horizontal
            || self.allow_tt_vertical
            || self.allow_tt_horizontal
    }

    #[cfg(test)]
    pub(in crate::vvc) fn qt_split_without_neighbours() -> Self {
        Self {
            available_left: false,
            available_above: false,
            condition_left: false,
            condition_above: false,
            allow_bt_vertical: false,
            allow_bt_horizontal: false,
            allow_tt_vertical: false,
            allow_tt_horizontal: false,
            allow_qt: true,
        }
    }

    #[cfg(test)]
    pub(in crate::vvc) fn full_child_without_smaller_neighbours() -> Self {
        Self {
            available_left: false,
            available_above: false,
            condition_left: false,
            condition_above: false,
            allow_bt_vertical: true,
            allow_bt_horizontal: true,
            allow_tt_vertical: true,
            allow_tt_horizontal: true,
            allow_qt: true,
        }
    }

    #[cfg(test)]
    pub(in crate::vvc) fn full_child_with_deeper_neighbours(
        left_deeper: bool,
        above_deeper: bool,
    ) -> Self {
        Self {
            available_left: left_deeper,
            available_above: above_deeper,
            condition_left: left_deeper,
            condition_above: above_deeper,
            allow_bt_vertical: true,
            allow_bt_horizontal: true,
            allow_tt_vertical: true,
            allow_tt_horizontal: true,
            allow_qt: true,
        }
    }

    pub(in crate::vvc) fn split_cu_flag_ctx(self) -> u8 {
        // VVC 9.3.4.2.2 derives ctxInc for split_cu_flag as:
        //   condL + condA + ctxSetIdx * 3
        // with ctxSetIdx =
        //   (allowBtVer + allowBtHor + allowTtVer + allowTtHor
        //    + 2 * allowQt - 1) / 2.
        let split_alternatives = u8::from(self.allow_bt_vertical)
            + u8::from(self.allow_bt_horizontal)
            + u8::from(self.allow_tt_vertical)
            + u8::from(self.allow_tt_horizontal)
            + (2 * u8::from(self.allow_qt));
        debug_assert!(split_alternatives > 0);
        let ctx_set_idx = (split_alternatives - 1) / 2;
        u8::from(self.condition_left && self.available_left)
            + u8::from(self.condition_above && self.available_above)
            + (3 * ctx_set_idx)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcQtSplitCtxInput {
    pub(in crate::vvc) available_left: bool,
    pub(in crate::vvc) available_above: bool,
    pub(in crate::vvc) left_deeper_qt: bool,
    pub(in crate::vvc) above_deeper_qt: bool,
    pub(in crate::vvc) cqt_depth: u8,
}

impl VvcQtSplitCtxInput {
    #[cfg(test)]
    pub(in crate::vvc) fn from_node_without_deeper_neighbours(node: VvcCodingTreeNode) -> Self {
        Self {
            available_left: false,
            available_above: false,
            left_deeper_qt: false,
            above_deeper_qt: false,
            cqt_depth: node.cqt_depth,
        }
    }

    #[cfg(test)]
    pub(in crate::vvc) fn from_node_with_deeper_neighbours(
        node: VvcCodingTreeNode,
        left_deeper_qt: bool,
        above_deeper_qt: bool,
    ) -> Self {
        Self {
            available_left: left_deeper_qt,
            available_above: above_deeper_qt,
            left_deeper_qt,
            above_deeper_qt,
            cqt_depth: node.cqt_depth,
        }
    }

    pub(in crate::vvc) fn split_qt_flag_ctx(self) -> u8 {
        // VVC 9.3.4.2.2 derives ctxInc for split_qt_flag as:
        //   (condL && availableL) + (condA && availableA) + ctxSetIdx * 3
        // where ctxSetIdx is cqtDepth >= 2.
        u8::from(self.left_deeper_qt && self.available_left)
            + u8::from(self.above_deeper_qt && self.available_above)
            + (3 * u8::from(self.cqt_depth >= 2))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcLumaSplitAvailabilityKind {
    Intra,
    Inter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcCtuCabacOp {
    QtSplit {
        node: VvcCodingTreeNode,
        split_ctx: u8,
        write_split_flag: bool,
        write_qt_flag: bool,
        qt_ctx: u8,
    },
    BtSplit {
        node: VvcCodingTreeNode,
        vertical: bool,
        split_ctx: u8,
        write_split_flag: bool,
        write_qt_flag: bool,
        qt_ctx: u8,
        write_mtt_vertical_flag: bool,
        mtt_vertical_ctx: u8,
        write_binary_flag: bool,
        mtt_binary_ctx: u8,
        mtt_binary_value: bool,
    },
    LumaLeafWithSplitCtx {
        node: VvcCodingTreeNode,
        write_split_flag: bool,
        split_ctx: u8,
    },
    ChromaTree {
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
    },
}

impl VvcCtuCabacOp {
    #[cfg(test)]
    pub(in crate::vvc) fn ctu_partition(params: &VvcCtuPartitionParams) -> Vec<Self> {
        Self::intra_ctu_partition(params.shape(), params.luma_max_leaf_size)
    }

    #[cfg(any(test, feature = "bench-internals"))]
    pub(in crate::vvc) fn intra_ctu_partition(
        shape: VvcCtuPartitionShape,
        max_leaf_size: u16,
    ) -> Vec<Self> {
        let mut ops = Vec::new();
        let mut neighbours = VvcLumaNeighbourState::new(shape.visible_width, shape.visible_height);
        Self::append_intra_ctu_partition_with_luma_neighbours(
            &mut ops,
            &mut neighbours,
            shape,
            0,
            0,
            shape.visible_width,
            shape.visible_height,
            max_leaf_size,
        );
        ops
    }

    #[cfg(test)]
    pub(in crate::vvc) fn inter_skip_ctu_partition(
        shape: VvcCtuPartitionShape,
        max_leaf_size: u16,
    ) -> Vec<Self> {
        let mut ops = Vec::new();
        let mut neighbours = VvcLumaNeighbourState::new(shape.visible_width, shape.visible_height);
        Self::visit_inter_skip_ctu_partition_with_luma_neighbours(
            &mut neighbours,
            shape,
            0,
            0,
            shape.visible_width,
            shape.visible_height,
            max_leaf_size,
            |op| ops.push(op),
        );
        ops
    }

    #[cfg(any(test, feature = "bench-internals"))]
    pub(in crate::vvc) fn append_intra_ctu_partition_with_luma_neighbours(
        ops: &mut Vec<Self>,
        neighbours: &mut VvcLumaNeighbourState,
        shape: VvcCtuPartitionShape,
        origin_x: u16,
        origin_y: u16,
        picture_visible_width: u16,
        picture_visible_height: u16,
        max_leaf_size: u16,
    ) {
        Self::visit_intra_ctu_partition_with_luma_neighbours(
            neighbours,
            shape,
            origin_x,
            origin_y,
            picture_visible_width,
            picture_visible_height,
            max_leaf_size,
            |op| ops.push(op),
        );
    }

    pub(in crate::vvc) fn visit_intra_ctu_partition_with_luma_neighbours<F>(
        neighbours: &mut VvcLumaNeighbourState,
        shape: VvcCtuPartitionShape,
        origin_x: u16,
        origin_y: u16,
        picture_visible_width: u16,
        picture_visible_height: u16,
        max_leaf_size: u16,
        mut emit_op: F,
    ) where
        F: FnMut(Self),
    {
        let tree_type = vvc_luma_tree_type(shape);
        let root = VvcCodingTreeNode::root_at(
            origin_x,
            origin_y,
            shape.root_width,
            shape.root_height,
            tree_type,
        );
        visit_visible_luma_partition(
            root,
            picture_visible_width,
            picture_visible_height,
            max_leaf_size,
            VvcLumaSplitAvailabilityKind::Intra,
            &mut |event| {
                Self::emit_luma_partition_event(neighbours, event, &mut emit_op);
            },
        );
        if shape.dual_tree_intra && shape.chroma_sampling != ChromaSampling::Monochrome {
            emit_op(Self::ChromaTree {
                node: VvcCodingTreeNode::root_at(
                    origin_x,
                    origin_y,
                    shape.root_width,
                    shape.root_height,
                    VvcTreeType::DualTreeChroma,
                ),
                visible_width: picture_visible_width,
                visible_height: picture_visible_height,
            });
        }
    }

    pub(in crate::vvc) fn visit_inter_skip_ctu_partition_with_luma_neighbours<F>(
        neighbours: &mut VvcLumaNeighbourState,
        shape: VvcCtuPartitionShape,
        origin_x: u16,
        origin_y: u16,
        picture_visible_width: u16,
        picture_visible_height: u16,
        max_leaf_size: u16,
        mut emit_op: F,
    ) where
        F: FnMut(Self),
    {
        let tree_type = vvc_luma_tree_type(shape);
        let root = VvcCodingTreeNode::root_at(
            origin_x,
            origin_y,
            shape.root_width,
            shape.root_height,
            tree_type,
        );
        visit_visible_luma_partition(
            root,
            picture_visible_width,
            picture_visible_height,
            max_leaf_size,
            VvcLumaSplitAvailabilityKind::Inter,
            &mut |event| {
                Self::emit_luma_partition_event(neighbours, event, &mut emit_op);
            },
        );
    }

    fn emit_luma_partition_event<F>(
        neighbours: &mut VvcLumaNeighbourState,
        event: VvcLumaPartitionEvent,
        emit_op: &mut F,
    ) where
        F: FnMut(Self),
    {
        match event {
            VvcLumaPartitionEvent::Leaf {
                node,
                split,
                write_split_flag,
            } => {
                emit_op(Self::LumaLeafWithSplitCtx {
                    node,
                    write_split_flag,
                    split_ctx: if write_split_flag {
                        Self::luma_split_ctx(node, split, neighbours)
                    } else {
                        0
                    },
                });
                neighbours.mark_leaf(node);
            }
            VvcLumaPartitionEvent::QtSplit {
                node,
                split,
                write_split_flag,
            } => emit_op(Self::QtSplit {
                node,
                split_ctx: Self::luma_split_ctx(node, split, neighbours),
                write_split_flag,
                write_qt_flag: split.allow_qt && split.has_mtt(),
                qt_ctx: Self::luma_qt_split_ctx(node, neighbours),
            }),
            VvcLumaPartitionEvent::BtSplit {
                node,
                split,
                vertical,
                write_split_flag,
            } => {
                let can_hor = split.allow_bt_horizontal || split.allow_tt_horizontal;
                let can_ver = split.allow_bt_vertical || split.allow_tt_vertical;
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
                emit_op(Self::BtSplit {
                    node,
                    vertical,
                    split_ctx: Self::luma_split_ctx(node, split, neighbours),
                    write_split_flag,
                    write_qt_flag: split.allow_qt && split.has_mtt(),
                    qt_ctx: Self::luma_qt_split_ctx(node, neighbours),
                    write_mtt_vertical_flag: can_hor && can_ver,
                    mtt_vertical_ctx: Self::luma_mtt_vertical_ctx(node, split, neighbours),
                    write_binary_flag: can_binary && can_ternary,
                    mtt_binary_ctx: Self::mtt_binary_ctx(vertical, node.mtt_depth),
                    mtt_binary_value: true,
                });
            }
        }
    }

    fn boundary_qt_preferred(node: VvcCodingTreeNode, max_leaf_size: u16) -> bool {
        Self::qt_flag_can_be_signaled(node)
            && node.width > max_leaf_size
            && node.height > max_leaf_size
    }

    fn luma_split_ctx(
        node: VvcCodingTreeNode,
        mut split: VvcSplitCtxInput,
        neighbours: &VvcLumaNeighbourState,
    ) -> u8 {
        // H.266 9.3.4.2.2 Table 133 derives split_cu_flag condL from the
        // left neighbour CbHeight being smaller than the current cbHeight, and
        // condA from the above neighbour CbWidth being smaller than cbWidth.
        let left = neighbours.left_of(node);
        let above = neighbours.above_of(node);
        split.available_left = left.is_some();
        split.available_above = above.is_some();
        split.condition_left = left.is_some_and(|info| info.cb_height < node.height);
        split.condition_above = above.is_some_and(|info| info.cb_width < node.width);
        split.split_cu_flag_ctx()
    }

    fn luma_qt_split_ctx(node: VvcCodingTreeNode, neighbours: &VvcLumaNeighbourState) -> u8 {
        // H.266 9.3.4.2.2 Table 133 derives split_qt_flag condL/condA from
        // neighbouring CqtDepth being greater than the current cqtDepth.
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

    fn luma_mtt_vertical_ctx(
        node: VvcCodingTreeNode,
        split: VvcSplitCtxInput,
        neighbours: &VvcLumaNeighbourState,
    ) -> u8 {
        // H.266 9.3.4.2.3 first compares the number of vertical-vs-horizontal
        // BT/TT choices. If tied, it uses left/above neighbouring block sizes.
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
        let d_a = node.width / above.cb_width.max(1);
        let d_l = node.height / left.cb_height.max(1);
        if d_a == d_l {
            0
        } else if d_a < d_l {
            1
        } else {
            2
        }
    }

    fn luma_split_availability(
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
    ) -> VvcSplitCtxInput {
        // H.266 7.4.12.4 derives allowSplitQt/BT/TT by invoking 6.4.1,
        // 6.4.2 and 6.4.3. This implementation is intentionally written in
        // those terms so future SPS/profile changes update one availability
        // model instead of geometry-specific branches.
        let allow_qt = Self::qt_flag_can_be_signaled(node);
        let max_mtt_depth = VVC_CURRENT_MAX_LUMA_MTT_DEPTH + node.depth_offset;
        let allow_bt_vertical =
            Self::allow_luma_bt_split(node, true, visible_width, visible_height, max_mtt_depth);
        let allow_bt_horizontal =
            Self::allow_luma_bt_split(node, false, visible_width, visible_height, max_mtt_depth);
        let allow_tt_vertical =
            Self::allow_luma_tt_split(node, true, visible_width, visible_height, max_mtt_depth);
        let allow_tt_horizontal =
            Self::allow_luma_tt_split(node, false, visible_width, visible_height, max_mtt_depth);

        VvcSplitCtxInput {
            available_left: false,
            available_above: false,
            condition_left: false,
            condition_above: false,
            allow_bt_vertical,
            allow_bt_horizontal,
            allow_tt_vertical,
            allow_tt_horizontal,
            allow_qt,
        }
    }

    fn luma_inter_split_availability(
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
    ) -> VvcSplitCtxInput {
        // The SPS advertises an inter-slice maximum MTT depth of 3. The
        // intra encoder's smaller luma leaf/TU limits must not leak into
        // P-slice split-context derivation for all-skip CUs.
        let allow_qt = node.mtt_depth == 0
            && node.width > VVC_CURRENT_MIN_LUMA_QT_SIZE
            && node.height > VVC_CURRENT_MIN_LUMA_QT_SIZE
            && matches!(node.parent_split, VvcPartSplit::None | VvcPartSplit::Quad);
        let max_mtt_depth = 3 + node.depth_offset;
        let allow_bt_vertical = Self::allow_luma_bt_split_with_max_size(
            node,
            true,
            visible_width,
            visible_height,
            max_mtt_depth,
            VVC_CTU_SIZE as u16,
        );
        let allow_bt_horizontal = Self::allow_luma_bt_split_with_max_size(
            node,
            false,
            visible_width,
            visible_height,
            max_mtt_depth,
            VVC_CTU_SIZE as u16,
        );
        let allow_tt_vertical = Self::allow_luma_tt_split_with_max_size(
            node,
            true,
            visible_width,
            visible_height,
            max_mtt_depth,
            VVC_CTU_SIZE as u16,
        );
        let allow_tt_horizontal = Self::allow_luma_tt_split_with_max_size(
            node,
            false,
            visible_width,
            visible_height,
            max_mtt_depth,
            VVC_CTU_SIZE as u16,
        );

        VvcSplitCtxInput {
            available_left: false,
            available_above: false,
            condition_left: false,
            condition_above: false,
            allow_bt_vertical,
            allow_bt_horizontal,
            allow_tt_vertical,
            allow_tt_horizontal,
            allow_qt,
        }
    }

    fn luma_split_availability_for_kind(
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
        kind: VvcLumaSplitAvailabilityKind,
    ) -> VvcSplitCtxInput {
        match kind {
            VvcLumaSplitAvailabilityKind::Intra => {
                Self::luma_split_availability(node, visible_width, visible_height)
            }
            VvcLumaSplitAvailabilityKind::Inter => {
                Self::luma_inter_split_availability(node, visible_width, visible_height)
            }
        }
    }

    fn allow_luma_bt_split(
        node: VvcCodingTreeNode,
        vertical: bool,
        visible_width: u16,
        visible_height: u16,
        max_mtt_depth: u8,
    ) -> bool {
        Self::allow_luma_bt_split_with_max_size(
            node,
            vertical,
            visible_width,
            visible_height,
            max_mtt_depth,
            VVC_CURRENT_MAX_LUMA_BT_SIZE,
        )
    }

    fn allow_luma_bt_split_with_max_size(
        node: VvcCodingTreeNode,
        vertical: bool,
        visible_width: u16,
        visible_height: u16,
        max_mtt_depth: u8,
        max_bt_size: u16,
    ) -> bool {
        // H.266 6.4.2, luma intra subset. MinBtSizeY is MinCbSizeY.
        let cb_size = if vertical { node.width } else { node.height };
        if cb_size <= VVC_CURRENT_MIN_LUMA_CB_SIZE {
            return false;
        }
        if node.width > max_bt_size || node.height > max_bt_size || node.mtt_depth >= max_mtt_depth
        {
            return false;
        }
        let crosses_right = node.x + node.width > visible_width;
        let crosses_bottom = node.y + node.height > visible_height;
        if vertical && crosses_bottom {
            return false;
        }
        if !vertical && crosses_right && !crosses_bottom {
            return false;
        }
        if crosses_right && crosses_bottom && node.width > VVC_CURRENT_MIN_LUMA_QT_SIZE {
            return false;
        }
        // The parallel ternary-split exclusion from H.266 6.4.2 is kept
        // explicit for future TT support. The current luma partitioner only
        // selects BT, so the previous split cannot be the parallel TT mode.
        let _part_idx = node.part_idx;
        true
    }

    fn allow_luma_tt_split(
        node: VvcCodingTreeNode,
        vertical: bool,
        visible_width: u16,
        visible_height: u16,
        max_mtt_depth: u8,
    ) -> bool {
        Self::allow_luma_tt_split_with_max_size(
            node,
            vertical,
            visible_width,
            visible_height,
            max_mtt_depth,
            VVC_CURRENT_MAX_LUMA_TT_SIZE.min(64),
        )
    }

    fn allow_luma_tt_split_with_max_size(
        node: VvcCodingTreeNode,
        vertical: bool,
        visible_width: u16,
        visible_height: u16,
        max_mtt_depth: u8,
        max_tt_size: u16,
    ) -> bool {
        // H.266 6.4.3, luma intra subset. TT is not available for boundary
        // nodes, which is why boundary split syntax often infers the binary
        // flag rather than writing it.
        let cb_size = if vertical { node.width } else { node.height };
        if cb_size <= 2 * VVC_CURRENT_MIN_LUMA_CB_SIZE {
            return false;
        }
        if node.width > max_tt_size
            || node.height > max_tt_size
            || node.mtt_depth >= max_mtt_depth
            || node.x + node.width > visible_width
            || node.y + node.height > visible_height
        {
            return false;
        }
        true
    }

    fn luma_leaf_allowed(node: VvcCodingTreeNode, max_leaf_size: u16) -> bool {
        // The current residual path emits one transform_unit() per luma CU
        // leaf. Keep luma leaves within the requested square TB subset until
        // explicit TU partitioning under coding_unit() is implemented.
        node.width <= max_leaf_size && node.height <= max_leaf_size
    }

    fn luma_square_leaf_at_mtt_limit(
        node: VvcCodingTreeNode,
        max_leaf_size: u16,
        split_kind: VvcLumaSplitAvailabilityKind,
    ) -> bool {
        if node.width != node.height || node.width <= max_leaf_size || node.mtt_depth == 0 {
            return false;
        }
        let max_mtt_depth = match split_kind {
            VvcLumaSplitAvailabilityKind::Intra => VVC_CURRENT_MAX_LUMA_MTT_DEPTH,
            VvcLumaSplitAvailabilityKind::Inter => 3,
        } + node.depth_offset;
        node.mtt_depth >= max_mtt_depth
    }

    pub(in crate::vvc) fn mtt_binary_ctx(vertical: bool, mtt_depth: u8) -> u8 {
        // ITU-T H.266 clause 9.3.4.2.1, Table 132:
        // ctxInc = (2 * mtt_split_cu_vertical_flag) + (mttDepth <= 1 ? 1 : 0).
        (2 * u8::from(vertical)) + u8::from(mtt_depth <= 1)
    }

    pub(in crate::vvc) fn qt_flag_can_be_signaled(node: VvcCodingTreeNode) -> bool {
        let min_qt_size = match node.tree_type {
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma => VVC_CURRENT_MIN_LUMA_QT_SIZE,
            VvcTreeType::DualTreeChroma => VVC_CURRENT_MIN_CHROMA_420_QT_SIZE,
        };
        node.mtt_depth == 0 && node.width > min_qt_size && node.height > min_qt_size
    }
}
