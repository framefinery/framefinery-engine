#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VvcLumaPartitionEvent {
    Leaf {
        node: VvcCodingTreeNode,
        split: VvcSplitCtxInput,
        write_split_flag: bool,
    },
    QtSplit {
        node: VvcCodingTreeNode,
        split: VvcSplitCtxInput,
        write_split_flag: bool,
    },
    BtSplit {
        node: VvcCodingTreeNode,
        split: VvcSplitCtxInput,
        vertical: bool,
        write_split_flag: bool,
    },
}

fn visit_visible_luma_partition<F>(
    node: VvcCodingTreeNode,
    visible_width: u16,
    visible_height: u16,
    max_leaf_size: u16,
    split_kind: VvcLumaSplitAvailabilityKind,
    emit_event: &mut F,
) where
    F: FnMut(VvcLumaPartitionEvent),
{
    visit_visible_luma_partition_with_limits(
        node,
        visible_width,
        visible_height,
        max_leaf_size,
        split_kind.limits(),
        emit_event,
    );
}

fn visit_visible_luma_partition_with_limits<F>(
    node: VvcCodingTreeNode,
    visible_width: u16,
    visible_height: u16,
    max_leaf_size: u16,
    split_limits: VvcLumaSplitLimits,
    emit_event: &mut F,
) where
    F: FnMut(VvcLumaPartitionEvent),
{
    if !node.intersects_visible(visible_width, visible_height) {
        return;
    }
    if node.fits_visible(visible_width, visible_height)
        && (VvcCtuCabacOp::luma_leaf_allowed(node, max_leaf_size)
            || VvcCtuCabacOp::luma_square_leaf_at_mtt_limit(node, max_leaf_size, split_limits))
    {
        let split = VvcCtuCabacOp::luma_split_availability(
            node,
            visible_width,
            visible_height,
            split_limits,
        );
        emit_event(VvcLumaPartitionEvent::Leaf {
            node,
            write_split_flag: split.has_mtt() || split.allow_qt,
            split,
        });
        return;
    }

    if !node.fits_visible(visible_width, visible_height) {
        visit_implicit_boundary_luma_partition(
            node,
            visible_width,
            visible_height,
            max_leaf_size,
            split_limits,
            emit_event,
        );
        return;
    }

    let split =
        VvcCtuCabacOp::luma_split_availability(node, visible_width, visible_height, split_limits);
    if !split.has_mtt() && !split.allow_qt {
        emit_event(VvcLumaPartitionEvent::Leaf {
            node,
            split,
            write_split_flag: false,
        });
        return;
    }

    debug_assert!(node.width > max_leaf_size || node.height > max_leaf_size);
    if node.mtt_depth > 0 || !split.allow_qt {
        visit_visible_luma_mtt_partition(
            node,
            visible_width,
            visible_height,
            max_leaf_size,
            split_limits,
            emit_event,
        );
        return;
    }
    emit_event(VvcLumaPartitionEvent::QtSplit {
        node,
        split,
        write_split_flag: true,
    });
    for child_idx in 0..4 {
        visit_visible_luma_partition_with_limits(
            node.qt_child(child_idx),
            visible_width,
            visible_height,
            max_leaf_size,
            split_limits,
            emit_event,
        );
    }
}

fn visit_visible_luma_mtt_partition<F>(
    node: VvcCodingTreeNode,
    visible_width: u16,
    visible_height: u16,
    max_leaf_size: u16,
    split_limits: VvcLumaSplitLimits,
    emit_event: &mut F,
) where
    F: FnMut(VvcLumaPartitionEvent),
{
    let vertical =
        node.width > max_leaf_size && (node.height <= max_leaf_size || node.width >= node.height);
    let split =
        VvcCtuCabacOp::luma_split_availability(node, visible_width, visible_height, split_limits);
    emit_event(VvcLumaPartitionEvent::BtSplit {
        node,
        split,
        vertical,
        write_split_flag: true,
    });
    for child_idx in 0..2 {
        visit_visible_luma_partition_with_limits(
            node.mtt_child_with_boundary_depth_offset(
                vertical,
                child_idx,
                visible_width,
                visible_height,
            ),
            visible_width,
            visible_height,
            max_leaf_size,
            split_limits,
            emit_event,
        );
    }
}

fn visit_implicit_boundary_luma_partition<F>(
    node: VvcCodingTreeNode,
    visible_width: u16,
    visible_height: u16,
    max_leaf_size: u16,
    split_limits: VvcLumaSplitLimits,
    emit_event: &mut F,
) where
    F: FnMut(VvcLumaPartitionEvent),
{
    let bottom_left_in_pic = node.x < visible_width && node.y + node.height - 1 < visible_height;
    let top_right_in_pic = node.x + node.width - 1 < visible_width && node.y < visible_height;
    let split =
        VvcCtuCabacOp::luma_split_availability(node, visible_width, visible_height, split_limits);
    if !bottom_left_in_pic && !top_right_in_pic {
        for child_idx in 0..4 {
            visit_visible_luma_partition_with_limits(
                node.qt_child(child_idx),
                visible_width,
                visible_height,
                max_leaf_size,
                split_limits,
                emit_event,
            );
        }
    } else if !bottom_left_in_pic
        && top_right_in_pic
        && VvcCtuCabacOp::boundary_qt_preferred(node, max_leaf_size)
    {
        emit_event(VvcLumaPartitionEvent::QtSplit {
            node,
            split,
            write_split_flag: false,
        });
        for child_idx in 0..4 {
            visit_visible_luma_partition_with_limits(
                node.qt_child(child_idx),
                visible_width,
                visible_height,
                max_leaf_size,
                split_limits,
                emit_event,
            );
        }
    } else if !bottom_left_in_pic && split.allow_bt_horizontal {
        emit_event(VvcLumaPartitionEvent::BtSplit {
            node,
            split,
            vertical: false,
            write_split_flag: false,
        });
        for child_idx in 0..2 {
            visit_visible_luma_partition_with_limits(
                node.mtt_child_with_boundary_depth_offset(
                    false,
                    child_idx,
                    visible_width,
                    visible_height,
                ),
                visible_width,
                visible_height,
                max_leaf_size,
                split_limits,
                emit_event,
            );
        }
    } else if !top_right_in_pic && split.allow_bt_vertical {
        emit_event(VvcLumaPartitionEvent::BtSplit {
            node,
            split,
            vertical: true,
            write_split_flag: false,
        });
        for child_idx in 0..2 {
            visit_visible_luma_partition_with_limits(
                node.mtt_child_with_boundary_depth_offset(
                    true,
                    child_idx,
                    visible_width,
                    visible_height,
                ),
                visible_width,
                visible_height,
                max_leaf_size,
                split_limits,
                emit_event,
            );
        }
    } else {
        for child_idx in 0..4 {
            visit_visible_luma_partition_with_limits(
                node.qt_child(child_idx),
                visible_width,
                visible_height,
                max_leaf_size,
                split_limits,
                emit_event,
            );
        }
    }
}
