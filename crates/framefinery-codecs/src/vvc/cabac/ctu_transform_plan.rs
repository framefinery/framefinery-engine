pub(in crate::vvc) fn vvc_chroma_transform_nodes(
    shape: VvcCtuPartitionShape,
) -> Vec<VvcCodingTreeNode> {
    let mut nodes = Vec::new();
    vvc_chroma_transform_nodes_into(&mut nodes, shape);
    nodes
}

pub(in crate::vvc) fn vvc_chroma_transform_nodes_into(
    nodes: &mut Vec<VvcCodingTreeNode>,
    shape: VvcCtuPartitionShape,
) {
    nodes.clear();
    visit_visible_chroma_partition(
        VvcCodingTreeNode::root(
            shape.root_width,
            shape.root_height,
            VvcTreeType::DualTreeChroma,
        ),
        shape.visible_width,
        shape.visible_height,
        shape.chroma_sampling,
        &mut |event| {
            if let VvcChromaPartitionEvent::Leaf { node, .. } = event {
                nodes.push(node);
            }
        },
    );
}

#[cfg(any(test, feature = "bench-internals"))]
pub(in crate::vvc) fn vvc_luma_transform_nodes(
    shape: VvcCtuPartitionShape,
    max_leaf_size: u16,
) -> Vec<VvcCodingTreeNode> {
    vvc_luma_transform_nodes_for_kind(shape, max_leaf_size, VvcLumaSplitAvailabilityKind::Intra)
}

pub(in crate::vvc) fn vvc_luma_transform_nodes_for_kind(
    shape: VvcCtuPartitionShape,
    max_leaf_size: u16,
    split_kind: VvcLumaSplitAvailabilityKind,
) -> Vec<VvcCodingTreeNode> {
    let mut nodes = Vec::new();
    vvc_luma_transform_nodes_into_for_kind(&mut nodes, shape, max_leaf_size, split_kind);
    nodes
}

#[cfg(any(test, feature = "bench-internals"))]
pub(in crate::vvc) fn vvc_luma_transform_nodes_into(
    nodes: &mut Vec<VvcCodingTreeNode>,
    shape: VvcCtuPartitionShape,
    max_leaf_size: u16,
) {
    vvc_luma_transform_nodes_into_for_kind(
        nodes,
        shape,
        max_leaf_size,
        VvcLumaSplitAvailabilityKind::Intra,
    );
}

pub(in crate::vvc) fn vvc_luma_transform_nodes_into_for_kind(
    nodes: &mut Vec<VvcCodingTreeNode>,
    shape: VvcCtuPartitionShape,
    max_leaf_size: u16,
    split_kind: VvcLumaSplitAvailabilityKind,
) {
    nodes.clear();
    let tree_type = vvc_luma_tree_type(shape);
    visit_visible_luma_partition(
        VvcCodingTreeNode::root(shape.root_width, shape.root_height, tree_type),
        shape.visible_width,
        shape.visible_height,
        max_leaf_size,
        split_kind,
        &mut |event| {
            if let VvcLumaPartitionEvent::Leaf { node, .. } = event {
                nodes.push(node);
            }
        },
    );
}
