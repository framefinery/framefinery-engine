impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    fn emit_chroma_visible_qt_subtree(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        visible_width: u16,
        visible_height: u16,
        luma_mode_neighbours: &VvcLumaModeNeighbourState,
        neighbours: &mut VvcChromaNeighbourState,
    ) {
        let chroma_sampling = self.params.chroma_sampling;
        visit_visible_chroma_partition(
            node,
            visible_width,
            visible_height,
            chroma_sampling,
            &mut |event| {
                self.emit_chroma_partition_event(cabac, event, luma_mode_neighbours, neighbours);
            },
        );
    }

    fn emit_chroma_partition_event(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        event: VvcChromaPartitionEvent,
        luma_mode_neighbours: &VvcLumaModeNeighbourState,
        neighbours: &mut VvcChromaNeighbourState,
    ) {
        match event {
            VvcChromaPartitionEvent::Leaf { node, split } => {
                self.emit_chroma_transform_only_leaf(
                    cabac,
                    node,
                    split,
                    0,
                    luma_mode_neighbours,
                    neighbours,
                );
            }
            VvcChromaPartitionEvent::QtSplit {
                node,
                split,
                write_split_flag,
            } => self.emit_chroma_qt_split(cabac, node, split, write_split_flag, neighbours),
            VvcChromaPartitionEvent::BtSplit {
                node,
                split,
                vertical,
                write_split_flag,
            } => self.emit_chroma_bt_split(
                cabac,
                node,
                split,
                vertical,
                write_split_flag,
                neighbours,
            ),
        }
    }
}
