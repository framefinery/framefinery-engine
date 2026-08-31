impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    fn emit_chroma_qt_split(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        write_split_flag: bool,
        neighbours: &VvcChromaNeighbourState,
    ) {
        let qt_ctx = Self::chroma_qt_split_ctx(node, neighbours);
        if write_split_flag && split.can_no {
            self.contexts.encode_split_flag(
                cabac,
                Self::chroma_split_ctx(node, split, neighbours),
                true,
            );
        }
        if split.allow_btt() {
            self.contexts.encode_split_qt_flag(cabac, qt_ctx, true);
        }
    }

    fn emit_chroma_bt_split(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        vertical: bool,
        write_split_flag: bool,
        neighbours: &VvcChromaNeighbourState,
    ) {
        if !write_split_flag {
            // H.266 7.3.11.4 still signals split_qt_flag for an implicit
            // boundary BT when both QT and BTT are available; split_cu_flag,
            // direction, and binary/ternary choice are inferred.
            if split.allow_qt && split.allow_btt() {
                self.contexts.encode_split_qt_flag(
                    cabac,
                    Self::chroma_qt_split_ctx(node, neighbours),
                    false,
                );
            }
            return;
        }
        debug_assert!(!split.allow_qt || split.allow_btt());
        if split.can_no {
            self.contexts.encode_split_flag(
                cabac,
                Self::chroma_split_ctx(node, split, neighbours),
                true,
            );
        }
        if split.allow_qt {
            let qt_ctx = Self::chroma_qt_split_ctx(node, neighbours);
            self.contexts.encode_split_qt_flag(cabac, qt_ctx, false);
        }

        let can_hor = split.allow_bt_horizontal || split.allow_tt_horizontal;
        let can_ver = split.allow_bt_vertical || split.allow_tt_vertical;
        if can_ver && can_hor {
            self.contexts.encode_mtt_split_cu_vertical_flag(
                cabac,
                Self::chroma_mtt_vertical_ctx(node, split, neighbours),
                vertical,
            );
        }

        let can_binary = if vertical {
            split.allow_bt_vertical
        } else {
            split.allow_bt_horizontal
        };
        let can_ternary = if vertical {
            split.allow_tt_vertical
        } else {
            split.allow_tt_horizontal
        };
        if can_binary && can_ternary {
            self.contexts.encode_mtt_split_cu_binary_flag(
                cabac,
                VvcCtuCabacOp::mtt_binary_ctx(vertical, node.mtt_depth),
                true,
            );
        }
    }
}
