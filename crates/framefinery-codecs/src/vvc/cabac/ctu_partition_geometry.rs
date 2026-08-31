#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcCtuPartitionShape {
    pub(in crate::vvc) root_width: u16,
    pub(in crate::vvc) root_height: u16,
    pub(in crate::vvc) visible_width: u16,
    pub(in crate::vvc) visible_height: u16,
    pub(in crate::vvc) chroma_sampling: ChromaSampling,
    pub(in crate::vvc) dual_tree_intra: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcTreeType {
    SingleTree,
    DualTreeLuma,
    DualTreeChroma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcPartSplit {
    None,
    Quad,
    HorizontalBinary,
    VerticalBinary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcCodingTreeNode {
    pub(in crate::vvc) x: u16,
    pub(in crate::vvc) y: u16,
    pub(in crate::vvc) width: u16,
    pub(in crate::vvc) height: u16,
    pub(in crate::vvc) cqt_depth: u8,
    pub(in crate::vvc) mtt_depth: u8,
    pub(in crate::vvc) depth_offset: u8,
    pub(in crate::vvc) part_idx: u8,
    pub(in crate::vvc) parent_split: VvcPartSplit,
    pub(in crate::vvc) tree_type: VvcTreeType,
    pub(in crate::vvc) split_history: [VvcPartSplit; 2],
}

impl VvcCodingTreeNode {
    pub(in crate::vvc) fn root(width: u16, height: u16, tree_type: VvcTreeType) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
            cqt_depth: 0,
            mtt_depth: 0,
            depth_offset: 0,
            part_idx: 0,
            parent_split: VvcPartSplit::None,
            tree_type,
            split_history: [VvcPartSplit::None; 2],
        }
    }

    fn root_at(x: u16, y: u16, width: u16, height: u16, tree_type: VvcTreeType) -> Self {
        Self {
            x,
            y,
            width,
            height,
            cqt_depth: 0,
            mtt_depth: 0,
            depth_offset: 0,
            part_idx: 0,
            parent_split: VvcPartSplit::None,
            tree_type,
            split_history: [VvcPartSplit::None; 2],
        }
    }

    fn with_split_at_current_depth(self, split: VvcPartSplit) -> [VvcPartSplit; 2] {
        let mut split_history = self.split_history;
        let depth = usize::from(self.cqt_depth + self.mtt_depth);
        if depth < split_history.len() {
            split_history[depth] = split;
        }
        split_history
    }

    pub(in crate::vvc) fn qt_child(self, child_idx: u8) -> Self {
        debug_assert!(child_idx < 4);
        let half_width = self.width / 2;
        let half_height = self.height / 2;
        Self {
            x: self.x + u16::from(child_idx & 1) * half_width,
            y: self.y + u16::from(child_idx >> 1) * half_height,
            width: half_width,
            height: half_height,
            cqt_depth: self.cqt_depth + 1,
            mtt_depth: 0,
            depth_offset: self.depth_offset,
            part_idx: child_idx,
            parent_split: VvcPartSplit::Quad,
            tree_type: self.tree_type,
            split_history: self.with_split_at_current_depth(VvcPartSplit::Quad),
        }
    }

    pub(in crate::vvc) fn mtt_child(self, vertical: bool, child_idx: u8) -> Self {
        debug_assert!(child_idx < 2);
        let width = if vertical { self.width / 2 } else { self.width };
        let height = if vertical {
            self.height
        } else {
            self.height / 2
        };
        Self {
            x: self.x + u16::from(vertical) * u16::from(child_idx) * width,
            y: self.y + u16::from(!vertical) * u16::from(child_idx) * height,
            width,
            height,
            cqt_depth: self.cqt_depth,
            mtt_depth: self.mtt_depth + 1,
            depth_offset: self.depth_offset,
            part_idx: child_idx,
            parent_split: if vertical {
                VvcPartSplit::VerticalBinary
            } else {
                VvcPartSplit::HorizontalBinary
            },
            tree_type: self.tree_type,
            split_history: self.with_split_at_current_depth(if vertical {
                VvcPartSplit::VerticalBinary
            } else {
                VvcPartSplit::HorizontalBinary
            }),
        }
    }

    pub(in crate::vvc) fn mtt_child_with_boundary_depth_offset(
        self,
        vertical: bool,
        child_idx: u8,
        visible_width: u16,
        visible_height: u16,
    ) -> Self {
        let mut child = self.mtt_child(vertical, child_idx);
        // H.266 7.3.11.4 increments depthOffset for boundary BT splits that
        // reduce the out-of-picture axis: vertical BT when the parent extends
        // past the right picture boundary, horizontal BT when it extends past
        // the bottom picture boundary. 7.4.12.4 then adds depthOffset to
        // MaxMttDepth for split availability.
        let crosses_reduced_boundary = if vertical {
            self.x + self.width > visible_width
        } else {
            self.y + self.height > visible_height
        };
        child.depth_offset = self.depth_offset + u8::from(crosses_reduced_boundary);
        child
    }

    pub(in crate::vvc) fn intersects_visible(
        self,
        visible_width: u16,
        visible_height: u16,
    ) -> bool {
        self.x < visible_width && self.y < visible_height
    }

    pub(in crate::vvc) fn fits_visible(self, visible_width: u16, visible_height: u16) -> bool {
        self.x + self.width <= visible_width && self.y + self.height <= visible_height
    }
}

fn vvc_luma_tree_type(shape: VvcCtuPartitionShape) -> VvcTreeType {
    if shape.dual_tree_intra && shape.chroma_sampling != ChromaSampling::Monochrome {
        VvcTreeType::DualTreeLuma
    } else {
        VvcTreeType::SingleTree
    }
}
