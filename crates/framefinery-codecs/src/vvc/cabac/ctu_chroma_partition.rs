#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) enum VvcChromaPartitionEvent {
    Leaf {
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
    },
    QtSplit {
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        write_split_flag: bool,
    },
    BtSplit {
        node: VvcCodingTreeNode,
        split: VvcChromaSplitAvailability,
        vertical: bool,
        write_split_flag: bool,
    },
}

pub(in crate::vvc) fn visit_visible_chroma_partition<F>(
    node: VvcCodingTreeNode,
    visible_width: u16,
    visible_height: u16,
    chroma_sampling: ChromaSampling,
    emit_event: &mut F,
) where
    F: FnMut(VvcChromaPartitionEvent),
{
    debug_assert_eq!(node.tree_type, VvcTreeType::DualTreeChroma);
    if !node.intersects_visible(visible_width, visible_height) {
        return;
    }
    if node.fits_visible(visible_width, visible_height)
        && chroma_leaf_allowed(node, chroma_sampling)
    {
        emit_event(VvcChromaPartitionEvent::Leaf {
            node,
            split: vvc_chroma_split_availability(
                node,
                visible_width,
                visible_height,
                chroma_sampling,
            ),
        });
        return;
    }

    if !node.fits_visible(visible_width, visible_height) {
        visit_implicit_boundary_chroma_partition(
            node,
            visible_width,
            visible_height,
            chroma_sampling,
            emit_event,
        );
        return;
    }

    let split = vvc_chroma_split_availability(node, visible_width, visible_height, chroma_sampling);
    if split.allow_qt {
        emit_event(VvcChromaPartitionEvent::QtSplit {
            node,
            split,
            write_split_flag: true,
        });
        for child_idx in 0..4 {
            visit_visible_chroma_partition(
                node.qt_child(child_idx),
                visible_width,
                visible_height,
                chroma_sampling,
                emit_event,
            );
        }
    } else {
        let vertical = chroma_prefer_vertical_bt(node, split);
        emit_event(VvcChromaPartitionEvent::BtSplit {
            node,
            split,
            vertical,
            write_split_flag: true,
        });
        for child_idx in 0..2 {
            visit_visible_chroma_partition(
                node.mtt_child(vertical, child_idx),
                visible_width,
                visible_height,
                chroma_sampling,
                emit_event,
            );
        }
    }
}

fn visit_implicit_boundary_chroma_partition<F>(
    node: VvcCodingTreeNode,
    visible_width: u16,
    visible_height: u16,
    chroma_sampling: ChromaSampling,
    emit_event: &mut F,
) where
    F: FnMut(VvcChromaPartitionEvent),
{
    let split = vvc_chroma_split_availability(node, visible_width, visible_height, chroma_sampling);
    if split.allow_qt {
        emit_event(VvcChromaPartitionEvent::QtSplit {
            node,
            split,
            write_split_flag: false,
        });
        for child_idx in 0..4 {
            visit_visible_chroma_partition(
                node.qt_child(child_idx),
                visible_width,
                visible_height,
                chroma_sampling,
                emit_event,
            );
        }
        return;
    }
    match split.implicit_split {
        VvcPartSplit::Quad => {
            for child_idx in 0..4 {
                visit_visible_chroma_partition(
                    node.qt_child(child_idx),
                    visible_width,
                    visible_height,
                    chroma_sampling,
                    emit_event,
                );
            }
        }
        VvcPartSplit::HorizontalBinary | VvcPartSplit::VerticalBinary => {
            let vertical = split.implicit_split == VvcPartSplit::VerticalBinary;
            emit_event(VvcChromaPartitionEvent::BtSplit {
                node,
                split,
                vertical,
                write_split_flag: false,
            });
            for child_idx in 0..2 {
                visit_visible_chroma_partition(
                    node.mtt_child_with_boundary_depth_offset(
                        vertical,
                        child_idx,
                        visible_width,
                        visible_height,
                    ),
                    visible_width,
                    visible_height,
                    chroma_sampling,
                    emit_event,
                );
            }
        }
        VvcPartSplit::None => {
            debug_assert!(
                !node.intersects_visible(visible_width, visible_height),
                "boundary chroma node must have an implicit split"
            );
        }
    }
}
