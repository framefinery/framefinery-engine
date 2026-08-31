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
