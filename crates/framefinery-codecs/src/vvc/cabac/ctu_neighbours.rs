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
