impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
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
            "missing chroma TU coefficient data for coding-tree leaf {tu_idx} (available {}, node {}x{} at {},{}, sampling {:?}, dual_tree {}, inter {})",
            self.params.chroma_tu_count,
            node.width,
            node.height,
            node.x,
            node.y,
            self.params.chroma_sampling,
            self.params.dual_tree_intra,
            self.inter_slice,
        );
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
}
