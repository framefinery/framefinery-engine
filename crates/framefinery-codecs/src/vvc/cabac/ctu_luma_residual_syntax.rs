impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    fn emit_luma_cbf(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        cbf: bool,
        bdpcm: bool,
    ) {
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        // VVC 7.3.11.10 transform_unit emits tu_y_coded_flag / cbf_comp
        // through QtCbf[Y].
        self.contexts.encode_qt_cbf_y(cabac, u8::from(bdpcm), cbf);
    }

    fn emit_transform_unit_residual(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
    ) {
        if node.tree_type == VvcTreeType::SingleTree
            && self.params.chroma_sampling != ChromaSampling::Monochrome
        {
            self.emit_single_tree_residual(cabac, node);
        } else {
            self.emit_luma_residual(cabac, node);
        }
    }

    fn emit_luma_residual(&mut self, cabac: &mut VvcCabacEncoder, node: VvcCodingTreeNode) {
        let tu_idx = self.luma_tu_index;
        self.luma_tu_index += 1;
        assert!(
            tu_idx < self.params.luma_tu_count,
            "missing luma TU coefficient data for coding-tree leaf {tu_idx} (available {}, node {}x{} at {},{}, split {:?}, max leaf {})",
            self.params.luma_tu_count,
            node.width,
            node.height,
            node.x,
            node.y,
            self.params.luma_split_kind,
            self.params.luma_max_leaf_size,
        );
        let dc_level = self.params.luma_tu_dc_levels[tu_idx];
        let cbf = dc_level != 0 || self.params.luma_tu_has_ac[tu_idx];
        let bdpcm_mode = self.params.luma_tu_bdpcm_modes[tu_idx];
        self.emit_luma_cbf(cabac, node, cbf, bdpcm_mode.is_enabled());
        if !cbf {
            return;
        }

        let log2_width = node.width.ilog2() as u8;
        let log2_height = node.height.ilog2() as u8;
        let ac_levels = &self.params.luma_tu_ac_levels[tu_idx];
        let has_ac = self.params.luma_tu_has_ac[tu_idx];
        let transform_skip = self.params.luma_tu_transform_skip[tu_idx];
        let mts_index = self.params.luma_tu_mts_index[tu_idx];
        let mut residual =
            VvcResidualCabacEncoder::new(&mut *self.contexts, self.slice_config.residual_options());
        VvcResidualCabacSymbolStream::emit_luma_stored_coefficients(
            log2_width,
            log2_height,
            dc_level,
            ac_levels,
            has_ac,
            transform_skip,
            bdpcm_mode.is_enabled(),
            mts_index,
            &mut residual,
            cabac,
        );
        self.emit_luma_post_residual_tools(cabac, node, has_ac, transform_skip, mts_index);
    }
}
