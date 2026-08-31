impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    #[cfg(test)]
    pub(in crate::vvc) fn emit(&mut self, cabac: &mut VvcCabacEncoder, op: VvcCtuCabacOp) {
        let mut luma_mode_neighbours = VvcLumaModeNeighbourState::new(
            self.params.visible_width as u16,
            self.params.visible_height as u16,
        );
        self.emit_with_luma_mode_neighbours(cabac, op, &mut luma_mode_neighbours);
    }

    fn emit_with_luma_mode_neighbours(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        op: VvcCtuCabacOp,
        luma_mode_neighbours: &mut VvcLumaModeNeighbourState,
    ) {
        if vvc_cabac_op_trace_enabled() {
            eprintln!("FF_CABAC_OP {op:?}");
        }
        match op {
            VvcCtuCabacOp::QtSplit {
                node,
                split_ctx,
                write_split_flag,
                write_qt_flag,
                qt_ctx,
            } => self.emit_qt_split(
                cabac,
                node,
                split_ctx,
                write_split_flag,
                write_qt_flag,
                qt_ctx,
            ),
            op @ VvcCtuCabacOp::BtSplit { .. } => self.emit_bt_split(cabac, op),
            VvcCtuCabacOp::LumaLeafWithSplitCtx {
                node,
                write_split_flag,
                split_ctx,
            } => {
                self.emit_luma_leaf_split_with_ctx(cabac, node, write_split_flag, split_ctx);
                match self.emit_luma_explicit_inter_leaf(cabac, node, luma_mode_neighbours) {
                    VvcExplicitInterLeafSyntax::NotInter => {}
                    VvcExplicitInterLeafSyntax::NoResidual => return,
                    VvcExplicitInterLeafSyntax::Residual => {
                        self.emit_transform_unit_residual(cabac, node);
                        return;
                    }
                }
                if self.emit_luma_scc_selected_leaf(cabac, node) {
                    return;
                }
                self.emit_luma_inter_slice_intra_prefix(cabac, node, luma_mode_neighbours);
                self.emit_luma_scc_regular_intra_prefix(cabac, node);
                if !self.emit_luma_bdpcm_mode(cabac, node, luma_mode_neighbours) {
                    if !self.emit_luma_mip_mode(cabac, node, luma_mode_neighbours) {
                        self.emit_luma_multi_ref_line(cabac, node);
                        self.emit_luma_isp_mode(cabac, node);
                        self.emit_luma_intra_prediction_mode(cabac, node, luma_mode_neighbours);
                    }
                }
                self.emit_single_tree_chroma_prediction(cabac, node, luma_mode_neighbours);
                self.emit_transform_unit_residual(cabac, node);
            }
            VvcCtuCabacOp::ChromaTree {
                node,
                visible_width,
                visible_height,
            } => self.emit_chroma_tree(
                cabac,
                node,
                visible_width,
                visible_height,
                luma_mode_neighbours,
            ),
        }
    }

    fn emit_with_frame_neighbours(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        op: VvcCtuCabacOp,
        luma_mode_neighbours: &mut VvcLumaModeNeighbourState,
        chroma_neighbours: &mut VvcChromaNeighbourState,
    ) {
        if vvc_cabac_op_trace_enabled() {
            eprintln!("FF_CABAC_OP {op:?}");
        }
        match op {
            VvcCtuCabacOp::ChromaTree {
                node,
                visible_width,
                visible_height,
            } => self.emit_chroma_tree_with_neighbours(
                cabac,
                node,
                visible_width,
                visible_height,
                luma_mode_neighbours,
                chroma_neighbours,
            ),
            other => self.emit_with_luma_mode_neighbours(cabac, other, luma_mode_neighbours),
        }
    }
}
