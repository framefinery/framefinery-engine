#[cfg(any(test, feature = "bench-internals", feature = "vvc-stats"))]
fn encode_inter_skip_ctu_body_with_contexts(
    cabac: &mut VvcCabacEncoder,
    contexts: &mut VvcCabacContexts,
    ctu_geometry: VvcVideoGeometry,
    slice_config: VvcSliceSyntaxConfig,
) {
    let mut split_neighbours = VvcLumaNeighbourState::new(
        ctu_geometry.coded_width() as u16,
        ctu_geometry.coded_height() as u16,
    );
    let mut skip_neighbours = VvcInterSkipNeighbourState::new(
        ctu_geometry.coded_width() as u16,
        ctu_geometry.coded_height() as u16,
    );
    encode_inter_skip_ctu_body_with_frame_contexts(
        cabac,
        contexts,
        ctu_geometry,
        slice_config,
        &mut split_neighbours,
        &mut skip_neighbours,
        &mut VvcInterMotionNeighbourState::new(
            ctu_geometry.coded_width() as u16,
            ctu_geometry.coded_height() as u16,
        ),
        0,
        0,
        ctu_geometry.coded_width() as u16,
        ctu_geometry.coded_height() as u16,
    );
}

fn encode_inter_skip_ctu_body_with_frame_contexts(
    cabac: &mut VvcCabacEncoder,
    contexts: &mut VvcCabacContexts,
    ctu_geometry: VvcVideoGeometry,
    slice_config: VvcSliceSyntaxConfig,
    split_neighbours: &mut VvcLumaNeighbourState,
    skip_neighbours: &mut VvcInterSkipNeighbourState,
    motion_neighbours: &mut VvcInterMotionNeighbourState,
    origin_x: u16,
    origin_y: u16,
    picture_width: u16,
    picture_height: u16,
) {
    let shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: ctu_geometry.coded_width() as u16,
        visible_height: ctu_geometry.coded_height() as u16,
        chroma_sampling: slice_config.coding_tree.chroma_sampling,
        dual_tree_intra: false,
    };
    VvcCtuCabacOp::visit_inter_skip_ctu_partition_with_luma_neighbours(
        split_neighbours,
        shape,
        origin_x,
        origin_y,
        picture_width,
        picture_height,
        VVC_CTU_SIZE as u16,
        |op| emit_inter_skip_ctu_op(cabac, contexts, op, skip_neighbours, motion_neighbours),
    );
}

fn emit_inter_skip_ctu_op(
    cabac: &mut VvcCabacEncoder,
    contexts: &mut VvcCabacContexts,
    op: VvcCtuCabacOp,
    skip_neighbours: &mut VvcInterSkipNeighbourState,
    motion_neighbours: &mut VvcInterMotionNeighbourState,
) {
    match op {
        VvcCtuCabacOp::QtSplit {
            split_ctx,
            write_split_flag,
            write_qt_flag,
            qt_ctx,
            ..
        } => {
            if write_split_flag {
                contexts.encode_split_flag(cabac, split_ctx, true);
            }
            if write_qt_flag {
                contexts.encode_split_qt_flag(cabac, qt_ctx, true);
            }
        }
        VvcCtuCabacOp::BtSplit {
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
            ..
        } => {
            if write_split_flag {
                contexts.encode_split_flag(cabac, split_ctx, true);
            }
            if write_qt_flag {
                contexts.encode_split_qt_flag(cabac, qt_ctx, false);
            }
            if write_mtt_vertical_flag {
                contexts.encode_mtt_split_cu_vertical_flag(cabac, mtt_vertical_ctx, vertical);
            }
            if write_binary_flag {
                contexts.encode_mtt_split_cu_binary_flag(cabac, mtt_binary_ctx, mtt_binary_value);
            }
        }
        VvcCtuCabacOp::LumaLeafWithSplitCtx {
            node,
            write_split_flag,
            split_ctx,
        } => {
            if write_split_flag {
                contexts.encode_split_flag(cabac, split_ctx, false);
            }
            let skip_ctx = skip_neighbours.skip_ctx(node);
            contexts.encode_cu_skip_flag(cabac, skip_ctx, true);
            skip_neighbours.mark_leaf(node);
            motion_neighbours.mark_leaf(node, VvcInterMotionInfo::default());
        }
        VvcCtuCabacOp::ChromaTree { .. } => {}
    }
}
