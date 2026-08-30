impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    fn emit_luma_post_residual_tools(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        has_ac: bool,
        transform_skip: bool,
        mts_index: u8,
    ) {
        self.emit_luma_lfnst_idx(cabac, node, has_ac, transform_skip, mts_index);
        self.emit_luma_mts_idx(cabac, node, has_ac, transform_skip, mts_index);
    }

    fn emit_luma_lfnst_idx(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        has_ac: bool,
        transform_skip: bool,
        mts_index: u8,
    ) {
        if !self.slice_config.tools.lfnst_enabled
            || transform_skip
            || mts_index != 0
            || !has_ac
            || node.width > 64
            || node.height > 64
        {
            return;
        }
        // Active LFNST needs transform-domain candidate ownership and
        // coefficient-group constraints. Keep the syntax site wired and emit
        // lfnst_idx=0 while production selection is absent.
        self.contexts.encode_lfnst_idx(cabac, 0, false);
    }

    fn emit_luma_mts_idx(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        has_ac: bool,
        transform_skip: bool,
        mts_index: u8,
    ) {
        if !self.slice_config.tools.explicit_mts_intra_enabled
            || transform_skip
            || !has_ac
            || node.width > 32
            || node.height > 32
        {
            return;
        }
        assert!(
            matches!(mts_index, 0 | 2..=5),
            "VVC MTS index must be DCT2_DCT2 or one of the explicit MTS transform types"
        );

        // H.266 cu_residual() writes mts_idx after the transform tree. The
        // current selector still chooses DCT2_DCT2, but keep the VTM-shaped
        // syntax ready for later non-default transform candidates.
        self.contexts.encode_mts_idx(cabac, 0, mts_index != 0);
        if mts_index != 0 {
            for offset in 0..3 {
                let bin = mts_index > 2 + offset;
                self.contexts.encode_mts_idx(cabac, 1 + offset, bin);
                if !bin {
                    break;
                }
            }
        }
    }
}
