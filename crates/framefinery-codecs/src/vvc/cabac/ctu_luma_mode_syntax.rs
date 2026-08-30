impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
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
}
