impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
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
}
