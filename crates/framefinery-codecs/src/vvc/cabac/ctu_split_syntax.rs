impl<'a, 'p> VvcCtuCabacGenerator<'a, 'p> {
    fn emit_bt_split(&mut self, cabac: &mut VvcCabacEncoder, op: VvcCtuCabacOp) {
        let VvcCtuCabacOp::BtSplit {
            node,
            vertical,
            split_ctx,
            write_split_flag,
            write_qt_flag,
            qt_ctx,
            write_mtt_vertical_flag,
            mtt_vertical_ctx,
            write_binary_flag,
            mtt_binary_ctx,
            mtt_binary_value,
        } = op
        else {
            unreachable!("emit_bt_split expects a binary split operation");
        };
        debug_assert!(
            node.cqt_depth >= 1
                || node.mtt_depth > 0
                || (node.x % VVC_CTU_SIZE as u16 == 0 && node.y % VVC_CTU_SIZE as u16 == 0)
        );
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        if write_split_flag {
            self.contexts.encode_split_flag(cabac, split_ctx, true);
        }
        if write_qt_flag {
            self.contexts.encode_split_qt_flag(cabac, qt_ctx, false);
        }
        if write_mtt_vertical_flag {
            self.contexts
                .encode_mtt_split_cu_vertical_flag(cabac, mtt_vertical_ctx, vertical);
        }
        if write_binary_flag {
            self.contexts
                .encode_mtt_split_cu_binary_flag(cabac, mtt_binary_ctx, mtt_binary_value);
        }
    }

    fn emit_qt_split(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        split_ctx: u8,
        write_split_flag: bool,
        write_qt_flag: bool,
        qt_ctx: u8,
    ) {
        debug_assert!(node.cqt_depth <= 3);
        debug_assert_eq!(node.mtt_depth, 0);
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        // VVC 7.3.11.4 coding_tree emits split_cu_flag for QT-split luma
        // nodes. Some root-only geometries infer split_qt_flag, while boundary
        // constrained rectangular CTU views write it explicitly.
        if write_split_flag {
            self.contexts.encode_split_flag(cabac, split_ctx, true);
        }
        if write_qt_flag {
            self.contexts.encode_split_qt_flag(cabac, qt_ctx, true);
        }
    }

    fn emit_luma_leaf_split_with_ctx(
        &mut self,
        cabac: &mut VvcCabacEncoder,
        node: VvcCodingTreeNode,
        write_split_flag: bool,
        split_ctx: u8,
    ) {
        debug_assert!(
            node.cqt_depth >= 1
                || node.mtt_depth > 0
                || (node.x % VVC_CTU_SIZE as u16 == 0 && node.y % VVC_CTU_SIZE as u16 == 0)
        );
        debug_assert!(node.mtt_depth <= VVC_CURRENT_MAX_LUMA_MTT_DEPTH + node.depth_offset);
        debug_assert!(matches!(
            node.tree_type,
            VvcTreeType::SingleTree | VvcTreeType::DualTreeLuma
        ));
        if !write_split_flag {
            return;
        }
        self.contexts.encode_split_flag(cabac, split_ctx, false);
    }
}
