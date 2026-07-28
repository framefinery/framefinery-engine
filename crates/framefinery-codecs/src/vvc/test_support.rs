#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VvcCodingTreeStep {
    LumaTransformUnit {
        width: usize,
        height: usize,
    },
    ChromaTransformUnit {
        x: usize,
        y: usize,
        cb_coded: bool,
        cr_coded: bool,
    },
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VvcLumaPartitionStep {
    QuadSplit {
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    },
    Leaf {
        x: usize,
        y: usize,
        width: usize,
        height: usize,
    },
}

#[cfg(test)]
fn vvc_coding_tree_plan(geometry: VvcVideoGeometry) -> Vec<VvcCodingTreeStep> {
    vvc_coding_tree_plan_with_config(geometry, VvcCodingTreeConfig::yuv(ChromaSampling::Cs420))
}

#[cfg(test)]
fn vvc_coding_tree_plan_with_config(
    geometry: VvcVideoGeometry,
    config: VvcCodingTreeConfig,
) -> Vec<VvcCodingTreeStep> {
    let mut steps = Vec::new();
    steps.push(VvcCodingTreeStep::LumaTransformUnit {
        width: geometry.coded_width(),
        height: geometry.coded_height(),
    });

    let chroma_width = geometry.coded_width() / chroma_subsample_x(config.chroma_sampling);
    let chroma_height = geometry.coded_height() / chroma_subsample_y(config.chroma_sampling);
    for y in (0..chroma_height).step_by(4) {
        for x in (0..chroma_width).step_by(4) {
            let first = x == 0 && y == 0;
            steps.push(VvcCodingTreeStep::ChromaTransformUnit {
                x,
                y,
                cb_coded: first && geometry.coded_width() <= 8,
                cr_coded: first,
            });
        }
    }

    steps
}

#[cfg(test)]
fn vvc_luma_partition_plan(geometry: VvcVideoGeometry) -> Vec<VvcLumaPartitionStep> {
    let coded = geometry.coded();
    let mut steps = Vec::new();
    append_vvc_luma_partition(
        &mut steps,
        0,
        0,
        coded.width,
        coded.height,
        VvcCodedGeometry {
            width: VVC_CURRENT_MAX_LUMA_LEAF_SIZE as usize,
            height: VVC_CURRENT_MAX_LUMA_LEAF_SIZE as usize,
        },
    );
    steps
}

#[cfg(test)]
fn append_vvc_luma_partition(
    steps: &mut Vec<VvcLumaPartitionStep>,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    max_leaf: VvcCodedGeometry,
) {
    if width > max_leaf.width || height > max_leaf.height {
        steps.push(VvcLumaPartitionStep::QuadSplit {
            x,
            y,
            width,
            height,
        });
        let child_width = width / 2;
        let child_height = height / 2;
        for child_y in [y, y + child_height] {
            for child_x in [x, x + child_width] {
                append_vvc_luma_partition(
                    steps,
                    child_x,
                    child_y,
                    child_width,
                    child_height,
                    max_leaf,
                );
            }
        }
    } else {
        steps.push(VvcLumaPartitionStep::Leaf {
            x,
            y,
            width,
            height,
        });
    }
}

#[cfg(test)]
fn vvc_cabac_bits(
    geometry: VvcVideoGeometry,
    color: &VvcQuantizedColor,
    slice_config: VvcSliceSyntaxConfig,
) -> Vec<bool> {
    vvc_cabac_bits_with_luma_max_leaf_size(
        geometry,
        color,
        slice_config,
        VVC_CURRENT_MAX_LUMA_LEAF_SIZE,
    )
}

#[cfg(test)]
fn vvc_cabac_bits_with_luma_max_leaf_size(
    geometry: VvcVideoGeometry,
    color: &VvcQuantizedColor,
    slice_config: VvcSliceSyntaxConfig,
    luma_max_leaf_size: u16,
) -> Vec<bool> {
    if let Some(params) = vvc_ctu_partition_params_with_luma_max_leaf_size_and_chroma(
        geometry,
        color.clone(),
        luma_max_leaf_size,
        slice_config.coding_tree.chroma_sampling,
        slice_config.coding_tree.dual_tree_intra,
    ) {
        return vvc_ctu_partition_cabac_bits(&params, slice_config);
    }
    debug_assert!(
        false,
        "VVC coding tree for coded geometry {}x{} must be generated from syntax parameters",
        geometry.coded_width(),
        geometry.coded_height()
    );
    Vec::new()
}

#[cfg(test)]
fn vvc_ctu_partition_params(
    geometry: VvcVideoGeometry,
    color: &VvcQuantizedColor,
) -> Option<VvcCtuPartitionParams> {
    vvc_ctu_partition_params_with_luma_max_leaf_size(
        geometry,
        color,
        VVC_CURRENT_MAX_LUMA_LEAF_SIZE,
    )
}

#[cfg(test)]
fn vvc_ctu_partition_params_with_luma_max_leaf_size(
    geometry: VvcVideoGeometry,
    color: &VvcQuantizedColor,
    luma_max_leaf_size: u16,
) -> Option<VvcCtuPartitionParams> {
    let coded = geometry.coded();
    if coded.width > VVC_CTU_SIZE
        || coded.height > VVC_CTU_SIZE
        || coded.width < 8
        || coded.height < 8
    {
        return None;
    }
    vvc_ctu_partition_params_with_luma_max_leaf_size_and_chroma(
        geometry,
        color.clone(),
        luma_max_leaf_size,
        ChromaSampling::Cs420,
        true,
    )
}

#[cfg(test)]
fn vvc_ctu_partition_cabac_bits(
    params: &VvcCtuPartitionParams,
    slice_config: VvcSliceSyntaxConfig,
) -> Vec<bool> {
    debug_assert!((8..=64).contains(&params.root_width));
    debug_assert!((8..=64).contains(&params.root_height));
    debug_assert!(params.visible_width >= 8 && params.visible_height >= 8);

    let mut cabac = VvcCabacEncoder::new();
    cabac.start();
    encode_ctu_partition_body(&mut cabac, params, slice_config);
    cabac.encode_bin_trm(true);
    cabac.finish()
}
