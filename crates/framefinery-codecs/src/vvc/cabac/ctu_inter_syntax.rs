impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
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
        self.contexts.encode_qt_root_cbf(cabac, residual);
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
        self.contexts.encode(
            cabac,
            VvcCabacContext::PredModeFlag(pred_mode_ctx),
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
}
