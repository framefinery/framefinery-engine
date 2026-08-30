impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    fn emit_single_tree_chroma_prediction(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        luma_mode_neighbours: &VvcLumaModeNeighbourState,
    ) {
        if node.tree_type != VvcTreeType::SingleTree
            || self.params.chroma_sampling == ChromaSampling::Monochrome
        {
            return;
        }
        let tu_idx = self.chroma_tu_index;
        assert!(
            tu_idx < self.params.chroma_tu_count,
            "missing chroma TU prediction data for single-tree leaf {tu_idx}"
        );
        let chroma_bdpcm_mode = self.params.chroma_tu_bdpcm_modes[tu_idx];
        if !self.emit_chroma_bdpcm_mode(cabac, node, chroma_bdpcm_mode) {
            self.emit_chroma_intra_prediction_mode(cabac, node, tu_idx, luma_mode_neighbours);
        }
    }
}
