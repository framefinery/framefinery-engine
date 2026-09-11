impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    fn emit_luma_scc_regular_intra_prefix(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        ibc_ctx: u8,
    ) {
        if self.inter_slice {
            // Mixed P-slice SCC syntax also has to order pred_mode_flag,
            // pred_mode_ibc_flag, and pred_mode_plt_flag with inter-mode
            // eligibility. Keep this preparatory hook scoped to intra slices
            // until real IBC/palette candidates are introduced there.
            return;
        }
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        if self.slice_config.tools.ibc_enabled && vvc_scc_ibc_luma_node_allowed(node) {
            self.contexts.encode_cu_skip_flag(cabac, 0, false);
            self.contexts
                .encode(cabac, VvcCabacContext::PredModeIbcFlag(ibc_ctx), false);
        }
        if self.slice_config.tools.palette_enabled && vvc_scc_palette_luma_node_allowed(node) {
            self.contexts
                .encode(cabac, VvcCabacContext::PredModePltFlag, false);
        }
    }

    fn emit_luma_scc_selected_leaf(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
    ) -> bool {
        let decision = self
            .params
            .luma_tu_scc_decisions
            .get(self.luma_tu_index)
            .copied()
            .unwrap_or(VvcLumaSccDecision::RegularIntra);
        match decision {
            VvcLumaSccDecision::RegularIntra => false,
            VvcLumaSccDecision::IbcExact(decision) => {
                self.emit_luma_exact_ibc_leaf(cabac, node, decision);
                true
            }
        }
    }

    fn emit_luma_exact_ibc_leaf(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        decision: VvcLumaIbcDecision,
    ) {
        assert!(
            !self.inter_slice,
            "P-slice exact IBC leaf ordering is not wired into pred_mode_flag yet"
        );
        assert!(
            self.slice_config.tools.ibc_enabled,
            "exact IBC leaf selected without SCC IBC syntax enabled"
        );
        assert!(
            vvc_scc_ibc_luma_node_allowed(node),
            "exact IBC leaf selected for unsupported luma CU size {}x{}",
            node.width,
            node.height
        );
        assert_eq!(
            node.tree_type,
            VvcTreeType::SingleTree,
            "exact IBC leaf currently requires single-tree 4:4:4 syntax"
        );
        assert_eq!(
            self.params.chroma_sampling,
            ChromaSampling::Cs444,
            "exact IBC leaf currently requires 4:4:4 syntax"
        );
        assert!(
            self.luma_tu_index < self.params.luma_tu_count,
            "missing luma TU slot for exact IBC leaf {}",
            self.luma_tu_index
        );

        self.contexts.encode_cu_skip_flag(cabac, 0, false);
        self.contexts.encode(
            cabac,
            VvcCabacContext::PredModeIbcFlag(decision.pred_mode_ibc_ctx),
            true,
        );
        self.emit_luma_exact_ibc_prediction(cabac, decision);
        // IBC is an inter-coded CU in VTM's coding_unit() flow. With no
        // residual it therefore signals rqt_root_cbf=0 and returns before the
        // transform tree; cu_coded_flag belongs to the intra residual path.
        self.contexts.encode_qt_root_cbf(cabac, false);
        self.luma_tu_index += 1;
        if self.params.chroma_sampling != ChromaSampling::Monochrome {
            assert!(
                self.chroma_tu_index < self.params.chroma_tu_count,
                "missing chroma TU slot for exact IBC leaf {}",
                self.chroma_tu_index
            );
            self.chroma_tu_index += 1;
        }
    }

    fn emit_luma_exact_ibc_prediction(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        decision: VvcLumaIbcDecision,
    ) {
        // prediction_unit() first signals merge_flag. The IBC non-merge branch
        // then consumes the explicit BVD; general_merge_flag belongs to the
        // regular inter prediction branch and is not present here.
        self.contexts.encode_merge_flag(cabac, false);
        self.emit_luma_ibc_mvd_coding(cabac, decision.mvd_x, decision.mvd_y);
        // MaxNumIbcMergeCand is fixed to one in this SPS and AMVR is disabled,
        // so mvp_l0_flag/amvr_precision_idx are inferred in the same way as the
        // existing palette/SCC scaffold.
    }

    fn emit_luma_ibc_mvd_coding(&mut self, cabac: &mut VvcCabacEncoder, mvd_x: i16, mvd_y: i16) {
        self.emit_luma_mvd_coding(cabac, i32::from(mvd_x), i32::from(mvd_y));
    }
}
