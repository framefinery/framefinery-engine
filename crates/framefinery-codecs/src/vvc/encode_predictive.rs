#[derive(Debug, Clone)]
struct VvcPredictiveFrameCache {
    source: Vec<u8>,
    reconstruction: VvcReconstructionFrame,
    ctu_decisions: Vec<std::sync::Arc<VvcQuantizedCtuLeafDecision>>,
}

#[derive(Debug, Clone, Copy)]
struct VvcReusableCtuDecision<'a> {
    reconstruction: &'a VvcReconstructionFrame,
    decision: &'a std::sync::Arc<VvcQuantizedCtuLeafDecision>,
}

impl VvcPredictiveFrameCache {
    fn ctu_decision(&self, region: VvcCtuRegion) -> Option<&VvcQuantizedCtuLeafDecision> {
        self.ctu_decisions
            .get(region.slice_address)
            .map(std::sync::Arc::as_ref)
    }

    fn reusable_decision(&self, region: VvcCtuRegion) -> Option<VvcReusableCtuDecision<'_>> {
        let decision = self.ctu_decisions.get(region.slice_address)?;
        Some(VvcReusableCtuDecision {
            reconstruction: &self.reconstruction,
            decision,
        })
    }

    fn matching_decision(
        &self,
        current_source: &[u8],
        layout: PlanarYuvFrameLayout,
        region: VvcCtuRegion,
    ) -> Option<VvcReusableCtuDecision<'_>> {
        let reusable = self.reusable_decision(region)?;
        if !layout.regions_equal_between(
            current_source,
            region.origin_x,
            region.origin_y,
            &self.source,
            region.origin_x,
            region.origin_y,
            region.geometry.width,
            region.geometry.height,
        ) {
            return None;
        }
        Some(reusable)
    }

    fn lossy_inter_skip_candidate_distortions(
        &self,
        current_source: &[u8],
        layout: PlanarYuvFrameLayout,
        current_frame: &VvcSampledFrame,
        geometry: VvcVideoGeometry,
    ) -> Vec<Option<u64>> {
        let max_abs_delta =
            vvc_lossy_predictive_skip_max_abs_delta(current_frame.format.bit_depth);
        vvc_ctu_regions(geometry)
            .map(|region| {
                if !vvc_predictive_inter_skip_region(region)
                    || self.ctu_decisions.get(region.slice_address).is_none()
                {
                    return None;
                }
                if layout.regions_equal_between(
                    current_source,
                    region.origin_x,
                    region.origin_y,
                    &self.source,
                    region.origin_x,
                    region.origin_y,
                    region.geometry.width,
                    region.geometry.height,
                ) {
                    return Some(vvc_region_sse_against_reconstruction(
                        current_frame,
                        &self.reconstruction,
                        region,
                    ));
                }
                vvc_predictive_lossy_region_sse_if_within_reconstruction_delta(
                    current_frame,
                    &self.reconstruction,
                    region,
                    max_abs_delta,
                )
            })
            .collect()
    }
}

fn vvc_predictive_frame_lossless_ctu_inter_skip_candidate_count(
    previous_cache: Option<&VvcPredictiveFrameCache>,
    current_source: &[u8],
    layout: PlanarYuvFrameLayout,
    geometry: VvcVideoGeometry,
) -> usize {
    let Some(cache) = previous_cache else {
        return 0;
    };
    vvc_ctu_regions(geometry)
        .filter(|&region| {
            vvc_predictive_inter_skip_region(region)
                && cache
                    .matching_decision(current_source, layout, region)
                    .is_some()
        })
        .count()
}

fn vvc_lossy_predictive_ctu_skip_candidate_count_allows_frame_reuse(
    candidate_count: usize,
    ctu_count: usize,
) -> bool {
    candidate_count.saturating_mul(2) >= ctu_count.max(1)
}

fn vvc_lossy_predictive_ctu_skip_candidate_count_allows_mixed_p_slice(
    candidate_count: usize,
    ctu_count: usize,
    format: VvcPictureFormat,
) -> bool {
    if format.chroma_sampling == ChromaSampling::Cs444 {
        candidate_count >= ctu_count.max(1)
    } else {
        vvc_lossy_predictive_ctu_skip_candidate_count_allows_frame_reuse(
            candidate_count,
            ctu_count,
        )
    }
}

fn vvc_predictive_luma_inter_decisions_for_ctu(
    current: &VvcSampledFrame,
    previous_source: &VvcSampledFrame,
    previous_reconstruction: &VvcReconstructionFrame,
    region: VvcCtuRegion,
    luma_max_leaf_size: u16,
    format: VvcPictureFormat,
) -> Option<[Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS]> {
    if current.geometry != previous_source.geometry
        || current.format != previous_source.format
        || current.geometry != previous_reconstruction.geometry
        || current.format != previous_reconstruction.format
    {
        return None;
    }
    if luma_max_leaf_size != VVC_CURRENT_MAX_LUMA_LEAF_SIZE {
        return None;
    }
    if !vvc_explicit_inter_chroma_exact_gate_supported(format.chroma_sampling) {
        return None;
    }
    let motion_map = motion::vvc_luma_motion_map_for_region(current, previous_source, region, 16)?;
    let ctu_shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: region.geometry.coded_width() as u16,
        visible_height: region.geometry.coded_height() as u16,
        chroma_sampling: format.chroma_sampling,
        dual_tree_intra: false,
    };
    let mut decisions = [None; MAX_VVC_LUMA_TUS];
    let mut any = false;
    for (tu_idx, local_node) in vvc_luma_transform_nodes_for_kind(
        ctu_shape,
        luma_max_leaf_size,
        VvcLumaSplitAvailabilityKind::Inter,
    )
        .into_iter()
        .take(MAX_VVC_LUMA_TUS)
        .enumerate()
    {
        let node = vvc_global_encode_ctu_node(local_node, region)?;
        if !vvc_inter_node_fits_visible_source(current.geometry, node) {
            continue;
        }
        let Some(candidate) =
            vvc_exact_luma_motion_candidate_for_node(&motion_map, local_node)
        else {
            continue;
        };
        let mv_x = i16::try_from(candidate.mv.x).ok()?;
        let mv_y = i16::try_from(candidate.mv.y).ok()?;
        let decision = VvcLumaInterDecision { mv_x, mv_y };
        if vvc_inter_decision_supported_by_source_and_reconstruction(
            current,
            previous_source,
            previous_reconstruction,
            node,
            decision,
        )
        {
            decisions[tu_idx] = Some(decision);
            any = true;
        }
    }
    any.then_some(decisions)
}

fn vvc_exact_luma_motion_candidate_for_node(
    motion_map: &motion::VvcLumaMotionMap,
    local_node: VvcCodingTreeNode,
) -> Option<motion::VvcLumaMotionAggregateCandidate> {
    let block_size = motion_map.block_size();
    let node_x = usize::from(local_node.x);
    let node_y = usize::from(local_node.y);
    let node_width = usize::from(local_node.width);
    let node_height = usize::from(local_node.height);
    if node_x % block_size != 0
        || node_y % block_size != 0
        || node_width % block_size != 0
        || node_height % block_size != 0
    {
        return None;
    }
    let block_x = node_x / block_size;
    let block_y = node_y / block_size;
    let blocks_w = node_width / block_size;
    let blocks_h = node_height / block_size;
    let candidate =
        motion_map.uniform_aggregate_rect_candidate(block_x, block_y, blocks_w, blocks_h)?;
    (candidate.total_sad == 0).then_some(candidate)
}

fn vvc_global_encode_ctu_node(
    mut node: VvcCodingTreeNode,
    region: VvcCtuRegion,
) -> Option<VvcCodingTreeNode> {
    node.x = node.x.checked_add(u16::try_from(region.origin_x).ok()?)?;
    node.y = node.y.checked_add(u16::try_from(region.origin_y).ok()?)?;
    Some(node)
}

fn vvc_inter_node_fits_visible_source(
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
) -> bool {
    usize::from(node.x)
        .checked_add(usize::from(node.width))
        .is_some_and(|end| end <= geometry.width)
        && usize::from(node.y)
            .checked_add(usize::from(node.height))
            .is_some_and(|end| end <= geometry.height)
}

fn vvc_inter_decision_supported_by_source_and_reconstruction(
    current: &VvcSampledFrame,
    previous_source: &VvcSampledFrame,
    previous_reconstruction: &VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    decision: VvcLumaInterDecision,
) -> bool {
    if !vvc_inter_luma_prediction_fits(previous_reconstruction, node, decision) {
        return false;
    }
    if current.format.chroma_sampling == ChromaSampling::Cs444
        && !vvc_inter_luma_reconstruction_predicts_source_exact(
            current,
            previous_reconstruction,
            node,
            decision,
        )
    {
        return false;
    }
    if current.format.chroma_sampling == ChromaSampling::Monochrome {
        return true;
    }
    if !vvc_explicit_inter_chroma_exact_gate_supported(current.format.chroma_sampling) {
        return false;
    }
    if !vvc_inter_chroma_source_motion_is_exact(current, previous_source, node, decision) {
        return false;
    }
    vvc_inter_chroma_prediction_fits(current, previous_reconstruction, node, decision)
        && vvc_inter_chroma_reconstruction_predicts_source_exact(
            current,
            previous_reconstruction,
            node,
            decision,
        )
}

fn vvc_explicit_inter_chroma_exact_gate_supported(chroma_sampling: ChromaSampling) -> bool {
    matches!(
        chroma_sampling,
        ChromaSampling::Cs420 | ChromaSampling::Cs444
    )
}

fn vvc_inter_luma_reconstruction_predicts_source_exact(
    current: &VvcSampledFrame,
    previous_reconstruction: &VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    decision: VvcLumaInterDecision,
) -> bool {
    let dst_x = usize::from(node.x);
    let dst_y = usize::from(node.y);
    let Some(src_x) = offset_vvc_encode_origin(dst_x, decision.mv_x) else {
        return false;
    };
    let Some(src_y) = offset_vvc_encode_origin(dst_y, decision.mv_y) else {
        return false;
    };
    let width = usize::from(node.width);
    let height = usize::from(node.height);
    vvc_plane_regions_equal(
        &current.luma,
        current.geometry.width,
        dst_x,
        dst_y,
        &previous_reconstruction.luma,
        previous_reconstruction.luma_width(),
        src_x,
        src_y,
        width,
        height,
    )
}

fn vvc_inter_chroma_source_motion_is_exact(
    current: &VvcSampledFrame,
    previous_source: &VvcSampledFrame,
    node: VvcCodingTreeNode,
    decision: VvcLumaInterDecision,
) -> bool {
    let Some(region) = vvc_chroma_motion_region(current.format, node, decision) else {
        return false;
    };
    let stride = current.geometry.width / region.subsample_x;
    vvc_plane_regions_equal(
        &current.cb,
        stride,
        region.dst_x,
        region.dst_y,
        &previous_source.cb,
        stride,
        region.src_x,
        region.src_y,
        region.width,
        region.height,
    ) && vvc_plane_regions_equal(
        &current.cr,
        stride,
        region.dst_x,
        region.dst_y,
        &previous_source.cr,
        stride,
        region.src_x,
        region.src_y,
        region.width,
        region.height,
    )
}

fn vvc_inter_chroma_reconstruction_predicts_source_exact(
    current: &VvcSampledFrame,
    previous_reconstruction: &VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    decision: VvcLumaInterDecision,
) -> bool {
    let Some(region) = vvc_chroma_motion_region(current.format, node, decision) else {
        return false;
    };
    vvc_plane_regions_equal(
        &current.cb,
        current.geometry.width / region.subsample_x,
        region.dst_x,
        region.dst_y,
        &previous_reconstruction.cb,
        previous_reconstruction.chroma_width(),
        region.src_x,
        region.src_y,
        region.width,
        region.height,
    ) && vvc_plane_regions_equal(
        &current.cr,
        current.geometry.width / region.subsample_x,
        region.dst_x,
        region.dst_y,
        &previous_reconstruction.cr,
        previous_reconstruction.chroma_width(),
        region.src_x,
        region.src_y,
        region.width,
        region.height,
    )
}

fn vvc_inter_luma_prediction_fits(
    previous_reconstruction: &VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    decision: VvcLumaInterDecision,
) -> bool {
    let Some(src_x) = offset_vvc_encode_origin(usize::from(node.x), decision.mv_x) else {
        return false;
    };
    let Some(src_y) = offset_vvc_encode_origin(usize::from(node.y), decision.mv_y) else {
        return false;
    };
    vvc_plane_region_fits(
        previous_reconstruction.luma_width(),
        previous_reconstruction.luma_height(),
        src_x,
        src_y,
        usize::from(node.width),
        usize::from(node.height),
    )
}

fn vvc_inter_chroma_prediction_fits(
    current: &VvcSampledFrame,
    previous_reconstruction: &VvcReconstructionFrame,
    node: VvcCodingTreeNode,
    decision: VvcLumaInterDecision,
) -> bool {
    let Some(region) = vvc_chroma_motion_region(current.format, node, decision) else {
        return false;
    };
    vvc_plane_region_fits(
        previous_reconstruction.chroma_width(),
        previous_reconstruction.chroma_height(),
        region.src_x,
        region.src_y,
        region.width,
        region.height,
    ) && vvc_plane_region_fits(
        current.geometry.width / region.subsample_x,
        current.geometry.height / region.subsample_y,
        region.dst_x,
        region.dst_y,
        region.width,
        region.height,
    )
}

#[derive(Debug, Clone, Copy)]
struct VvcChromaMotionRegion {
    subsample_x: usize,
    subsample_y: usize,
    dst_x: usize,
    dst_y: usize,
    src_x: usize,
    src_y: usize,
    width: usize,
    height: usize,
}

fn vvc_chroma_motion_region(
    format: VvcPictureFormat,
    node: VvcCodingTreeNode,
    decision: VvcLumaInterDecision,
) -> Option<VvcChromaMotionRegion> {
    let subsample_x = chroma_subsample_x(format.chroma_sampling);
    let subsample_y = chroma_subsample_y(format.chroma_sampling);
    if i32::from(decision.mv_x).rem_euclid(subsample_x as i32) != 0
        || i32::from(decision.mv_y).rem_euclid(subsample_y as i32) != 0
    {
        return None;
    }
    let dst_x = usize::from(node.x) / subsample_x;
    let dst_y = usize::from(node.y) / subsample_y;
    let src_x = offset_vvc_encode_origin(dst_x, decision.mv_x / subsample_x as i16)?;
    let src_y = offset_vvc_encode_origin(dst_y, decision.mv_y / subsample_y as i16)?;
    Some(VvcChromaMotionRegion {
        subsample_x,
        subsample_y,
        dst_x,
        dst_y,
        src_x,
        src_y,
        width: usize::from(node.width) / subsample_x,
        height: usize::from(node.height) / subsample_y,
    })
}

fn vvc_plane_region_fits(
    plane_width: usize,
    plane_height: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> bool {
    x.checked_add(width).is_some_and(|end| end <= plane_width)
        && y.checked_add(height)
            .is_some_and(|end| end <= plane_height)
}

#[allow(clippy::too_many_arguments)]
fn vvc_plane_regions_equal(
    a: &[VvcSample],
    a_stride: usize,
    a_x: usize,
    a_y: usize,
    b: &[VvcSample],
    b_stride: usize,
    b_x: usize,
    b_y: usize,
    width: usize,
    height: usize,
) -> bool {
    if width == 0 || height == 0 {
        return false;
    }
    for row in 0..height {
        let a_start = (a_y + row) * a_stride + a_x;
        let b_start = (b_y + row) * b_stride + b_x;
        let a_end = a_start + width;
        let b_end = b_start + width;
        if a.get(a_start..a_end) != b.get(b_start..b_end) {
            return false;
        }
    }
    true
}

fn offset_vvc_encode_origin(origin: usize, delta: i16) -> Option<usize> {
    if delta >= 0 {
        origin.checked_add(delta as usize)
    } else {
        origin.checked_sub(delta.unsigned_abs() as usize)
    }
}

fn vvc_luma_inter_decision_count(
    decisions: &[Option<VvcLumaInterDecision>; MAX_VVC_LUMA_TUS],
) -> usize {
    decisions.iter().filter(|decision| decision.is_some()).count()
}

fn vvc_explicit_inter_decision_count_allows_mixed_p_slice(
    candidate_count: usize,
    format: VvcPictureFormat,
    eligible_luma_leaf_count: usize,
) -> bool {
    if candidate_count == 0 {
        return false;
    }
    if format.chroma_sampling != ChromaSampling::Cs444 {
        return true;
    }
    eligible_luma_leaf_count > 0 && candidate_count >= eligible_luma_leaf_count
}

fn vvc_explicit_inter_eligible_luma_leaf_count(
    geometry: VvcVideoGeometry,
    chroma_sampling: ChromaSampling,
) -> usize {
    vvc_ctu_regions(geometry)
        .map(|region| {
            let ctu_shape = VvcCtuPartitionShape {
                root_width: VVC_CTU_SIZE as u16,
                root_height: VVC_CTU_SIZE as u16,
                visible_width: region.geometry.coded_width() as u16,
                visible_height: region.geometry.coded_height() as u16,
                chroma_sampling,
                dual_tree_intra: false,
            };
            vvc_luma_transform_nodes_for_kind(
                ctu_shape,
                VVC_CURRENT_MAX_LUMA_LEAF_SIZE,
                VvcLumaSplitAvailabilityKind::Inter,
            )
                .into_iter()
                .take(MAX_VVC_LUMA_TUS)
                .filter_map(|local_node| vvc_global_encode_ctu_node(local_node, region))
                .filter(|&node| vvc_inter_node_fits_visible_source(geometry, node))
                .count()
        })
        .sum()
}
