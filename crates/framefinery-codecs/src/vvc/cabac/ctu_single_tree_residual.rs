impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    fn emit_single_tree_residual(&mut self, cabac: &mut VvcCabacEncoder, node: VvcCodingTreeNode) {
        let luma_tu_idx = self.luma_tu_index;
        self.luma_tu_index += 1;
        assert!(
            luma_tu_idx < self.params.luma_tu_count,
            "missing luma TU coefficient data for single-tree leaf {luma_tu_idx}"
        );
        let chroma_tu_idx = self.chroma_tu_index;
        self.chroma_tu_index += 1;
        assert!(
            chroma_tu_idx < self.params.chroma_tu_count,
            "missing chroma TU coefficient data for single-tree leaf {chroma_tu_idx}"
        );

        let chroma_bdpcm_mode = self.params.chroma_tu_bdpcm_modes[chroma_tu_idx];
        let cb_dc_level = self.params.cb_tu_dc_levels[chroma_tu_idx];
        let cr_dc_level = self.params.cr_tu_dc_levels[chroma_tu_idx];
        let cbf_cb = cb_dc_level != 0 || self.params.cb_tu_has_ac[chroma_tu_idx];
        let cbf_cr = cr_dc_level != 0 || self.params.cr_tu_has_ac[chroma_tu_idx];
        let cbf_cb_ctx = u8::from(chroma_bdpcm_mode.is_enabled());
        let cbf_cr_ctx = if chroma_bdpcm_mode.is_enabled() {
            2
        } else {
            u8::from(cbf_cb)
        };
        self.contexts.encode_qt_cbf_cb(cabac, cbf_cb_ctx, cbf_cb);
        self.contexts.encode_qt_cbf_cr(cabac, cbf_cr_ctx, cbf_cr);

        let luma_dc_level = self.params.luma_tu_dc_levels[luma_tu_idx];
        let cbf_luma = luma_dc_level != 0 || self.params.luma_tu_has_ac[luma_tu_idx];
        let luma_bdpcm_mode = self.params.luma_tu_bdpcm_modes[luma_tu_idx];
        // H.266 7.3.11.10: for an inter CU at transform depth zero, when
        // neither chroma component has residual, luma CBF is inferred true
        // from root_cbf and is not signalled. Keep this inference here at the
        // shared transform-tree syntax boundary so intra and inter use the
        // same residual path.
        let infer_inter_luma_cbf =
            self.params.luma_tu_inter_decisions[luma_tu_idx].is_some() && !cbf_cb && !cbf_cr;
        if infer_inter_luma_cbf {
            assert!(cbf_luma, "inter root CBF requires an inferred luma CBF");
        } else {
            self.emit_luma_cbf(cabac, node, cbf_luma, luma_bdpcm_mode.is_enabled());
        }

        if cbf_luma {
            let log2_width = node.width.ilog2() as u8;
            let log2_height = node.height.ilog2() as u8;
            let luma_has_ac = self.params.luma_tu_has_ac[luma_tu_idx];
            let luma_transform_skip = self.params.luma_tu_transform_skip[luma_tu_idx];
            let luma_mts_index = self.params.luma_tu_mts_index[luma_tu_idx];
            let mut residual = VvcResidualCabacEncoder::new(
                &mut *self.contexts,
                self.slice_config.residual_options(),
            );
            VvcResidualCabacSymbolStream::emit_luma_stored_coefficients(
                log2_width,
                log2_height,
                luma_dc_level,
                &self.params.luma_tu_ac_levels[luma_tu_idx],
                luma_has_ac,
                luma_transform_skip,
                luma_bdpcm_mode.is_enabled(),
                luma_mts_index,
                &mut residual,
                cabac,
            );
        }
        if cbf_cb {
            Self::emit_chroma_residual(
                &mut *self.contexts,
                self.slice_config,
                self.params.chroma_sampling,
                cabac,
                VvcResidualComponent::ChromaCb,
                node,
                cb_dc_level,
                &self.params.cb_tu_ac_levels[chroma_tu_idx],
                self.params.cb_tu_has_ac[chroma_tu_idx],
                self.params.cb_tu_transform_skip[chroma_tu_idx],
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
                &self.params.cr_tu_ac_levels[chroma_tu_idx],
                self.params.cr_tu_has_ac[chroma_tu_idx],
                self.params.cr_tu_transform_skip[chroma_tu_idx],
                chroma_bdpcm_mode.is_enabled(),
            );
        }
        if cbf_luma {
            self.emit_luma_post_residual_tools(
                cabac,
                node,
                self.params.luma_tu_has_ac[luma_tu_idx],
                self.params.luma_tu_transform_skip[luma_tu_idx],
                self.params.luma_tu_mts_index[luma_tu_idx],
            );
        }
    }
}
