impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    fn chroma_leaf_allowed(&self, node: VvcCodingTreeNode) -> bool {
        let chroma_width = vvc_chroma_width(node, self.params.chroma_sampling);
        let chroma_height = vvc_chroma_height(node, self.params.chroma_sampling);
        // H.266 7.3.11.10 transform_unit() is reached after the encoder's
        // chosen legal coding-tree split. The spec maximum for this SPS remains
        // MaxTbSizeY/SubWidthC by MaxTbSizeY/SubHeightC, but this hardware
        // residual subset chooses 8x8 luma-coordinate leaves so each 4:2:0
        // chroma TU is 4x4 samples and shares the luma TU cadence.
        chroma_width <= VVC_CURRENT_ENCODER_CHROMA_420_TB_SIZE
            && chroma_height <= VVC_CURRENT_ENCODER_CHROMA_420_TB_SIZE
    }

    fn chroma_cclm_enabled(&self, node: VvcCodingTreeNode) -> bool {
        if !self.slice_config.tools.cclm_enabled {
            return false;
        }
        vvc_chroma_cclm_node_allowed(node)
    }

    fn chroma_bdpcm_allowed(&self, node: VvcCodingTreeNode) -> bool {
        if !self.slice_config.tools.bdpcm_enabled {
            return false;
        }
        let chroma_width = vvc_chroma_width(node, self.params.chroma_sampling);
        let chroma_height = vvc_chroma_height(node, self.params.chroma_sampling);
        chroma_width <= 8 && chroma_height <= 8 && chroma_width >= 4 && chroma_height >= 4
    }

    fn chroma_split_ctx(
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        neighbours: &VvcChromaNeighbourState,
    ) -> u8 {
        // H.266 9.3.4.2.2 Table 133 derives chroma split_cu_flag condL from
        // the left chroma CU height being smaller than the current chroma CU
        // height, and condA from the above chroma CU width being smaller.
        let left = neighbours.left_of(node);
        let above = neighbours.above_of(node);
        VvcSplitCtxInput {
            available_left: left.is_some(),
            available_above: above.is_some(),
            condition_left: left.is_some_and(|info| info.cb_height < neighbours.node_height(node)),
            condition_above: above.is_some_and(|info| info.cb_width < neighbours.node_width(node)),
            allow_bt_vertical: split.allow_bt_vertical,
            allow_bt_horizontal: split.allow_bt_horizontal,
            allow_tt_vertical: split.allow_tt_vertical,
            allow_tt_horizontal: split.allow_tt_horizontal,
            allow_qt: split.allow_qt,
        }
        .split_cu_flag_ctx()
    }

    fn chroma_qt_split_ctx(node: VvcCodingTreeNode, neighbours: &VvcChromaNeighbourState) -> u8 {
        // H.266 9.3.4.2.2 Table 133 derives split_qt_flag condL/condA from
        // neighbouring chroma CqtDepth being greater than the current depth.
        let left = neighbours.left_of(node);
        let above = neighbours.above_of(node);
        VvcQtSplitCtxInput {
            available_left: left.is_some(),
            available_above: above.is_some(),
            left_deeper_qt: left.is_some_and(|info| info.cqt_depth > node.cqt_depth),
            above_deeper_qt: above.is_some_and(|info| info.cqt_depth > node.cqt_depth),
            cqt_depth: node.cqt_depth,
        }
        .split_qt_flag_ctx()
    }

    fn chroma_mtt_vertical_ctx(
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        neighbours: &VvcChromaNeighbourState,
    ) -> u8 {
        // H.266 9.3.4.2.3 first compares vertical-vs-horizontal BT/TT choices.
        // If tied, it uses the above chroma CU width and left chroma CU height.
        let vertical_choices =
            u8::from(split.allow_bt_vertical) + u8::from(split.allow_tt_vertical);
        let horizontal_choices =
            u8::from(split.allow_bt_horizontal) + u8::from(split.allow_tt_horizontal);
        if vertical_choices > horizontal_choices {
            return 4;
        }
        if vertical_choices < horizontal_choices {
            return 3;
        }
        let Some(above) = neighbours.above_of(node) else {
            return 0;
        };
        let Some(left) = neighbours.left_of(node) else {
            return 0;
        };
        let d_a = neighbours.node_width(node) / above.cb_width.max(1);
        let d_l = neighbours.node_height(node) / left.cb_height.max(1);
        if d_a == d_l {
            0
        } else if d_a < d_l {
            1
        } else {
            2
        }
    }

    fn chroma_prefer_vertical_bt(
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
    ) -> bool {
        if !split.allow_bt_vertical {
            return false;
        }
        if !split.allow_bt_horizontal {
            return true;
        }
        node.width >= node.height
    }
}
