impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
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
}
