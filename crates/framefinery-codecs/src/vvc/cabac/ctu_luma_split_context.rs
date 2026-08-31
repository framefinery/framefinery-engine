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
