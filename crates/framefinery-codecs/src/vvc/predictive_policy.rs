fn vvc_lossy_mixed_single_p_slice_supported(
    format: VvcPictureFormat,
    geometry: VvcVideoGeometry,
) -> bool {
    // The mixed InterSkip/Intra P-slice experiment is kept behind this single
    // policy gate until its complete partition and CABAC contract is covered
    // by required-reference multi-CTU tests. This preserves the shared
    // quantize/emit path and falls back to ordinary intra predictive pictures
    // rather than emitting partially modelled mixed syntax.
    format.chroma_sampling == ChromaSampling::Cs420
        && geometry.width % VVC_CTU_SIZE == 0
        && geometry.height % VVC_CTU_SIZE == 0
}

fn vvc_predictive_ctu_dependencies_reused(
    region: VvcCtuRegion,
    ctu_cols: usize,
    reused_ctus: &[bool],
) -> bool {
    let left_reused = region.origin_x == 0
        || region
            .slice_address
            .checked_sub(1)
            .and_then(|idx| reused_ctus.get(idx))
            .copied()
            .unwrap_or(false);
    let above_reused = region.origin_y == 0
        || region
            .slice_address
            .checked_sub(ctu_cols)
            .and_then(|idx| reused_ctus.get(idx))
            .copied()
            .unwrap_or(false);
    left_reused && above_reused
}

fn vvc_lossless_speed_luma_leaf_inter_skip_allowed(format: VvcPictureFormat) -> bool {
    let _format = format;
    // Leaf-level predictive skip needs legal inter-slice local-separate-tree
    // handling for small 4:2:0 intra leaves before it can share the mixed
    // P-slice CTU path. Keep the release path reference-clean by limiting VVC
    // predictive reuse to complete CTUs.
    false
}

fn vvc_predictive_intra_ctu_reuse_enabled_for_mode(mode: VvcResidualCodingMode) -> bool {
    // Exact-source CTU reuse is reference-clean for lossless streams because
    // the reconstructed CTU equals the source CTU. For lossy streams, reusing
    // a previous frame's quantized intra decisions can drift from VTM once
    // the same source CTU is coded in a later predictive picture; keep lossy
    // predictive GOP syntax enabled but re-quantize each CTU until that path
    // is modelled and covered.
    mode.is_lossless()
}

fn vvc_predictive_frame_inter_skip_enabled_for_reference_clean_release() -> bool {
    // Full-frame repeated P pictures use a single all-skip slice and
    // initialize the CABAC state as a P slice before emitting split/skip
    // syntax.
    true
}

fn vvc_predictive_ctu_inter_skip_enabled_for_reference_clean_release() -> bool {
    // Lossy CTU-level InterSkip now emits skipped and non-skipped CTUs in one
    // P-slice, using inter-slice split constraints plus single-tree intra
    // transform_unit syntax for the non-skipped CTUs. Lossless mixed CTU skip
    // still uses CTU slices because the 4x4 4:2:0 local-separate-tree branch is
    // not wired yet.
    true
}

fn vvc_lossy_predictive_ctu_inter_skip_enabled_for_reference_clean_release() -> bool {
    // Lossy CTU InterSkip is only enabled after normal CTU quantization and a
    // conservative RD gate. The pre-scan still requires enough skip candidates
    // to justify switching the frame to mixed P-slice output.
    true
}

fn vvc_lossy_predictive_inter_skip_selects_over_intra(
    source_frame: &VvcSampledFrame,
    intra_reconstruction: &VvcReconstructionFrame,
    region: VvcCtuRegion,
    skip_distortion: u64,
) -> bool {
    let intra_distortion =
        vvc_region_sse_against_reconstruction(source_frame, intra_reconstruction, region);
    skip_distortion <= intra_distortion
}

fn vvc_lossy_predictive_inter_skip_preselected(
    skip_distortion: u64,
    region: VvcCtuRegion,
    format: VvcPictureFormat,
) -> bool {
    skip_distortion <= vvc_lossy_predictive_preskip_max_sse(region, format)
}

const VVC_LOSSY_PREDICTIVE_PRESKIP_AVG_SSE_8BIT: u64 = 8;

fn vvc_lossy_predictive_preskip_max_sse(
    region: VvcCtuRegion,
    format: VvcPictureFormat,
) -> u64 {
    let sample_count =
        vvc_region_visible_luma_sample_count(region) + vvc_region_visible_chroma_sample_count(region, format);
    let scale = 1u64 << u32::from(format.bit_depth.bits().saturating_sub(8));
    sample_count.saturating_mul(
        scale
            .saturating_mul(scale)
            .saturating_mul(VVC_LOSSY_PREDICTIVE_PRESKIP_AVG_SSE_8BIT),
    )
}

fn vvc_region_visible_luma_sample_count(region: VvcCtuRegion) -> u64 {
    (region.geometry.width as u64).saturating_mul(region.geometry.height as u64)
}

fn vvc_region_visible_chroma_sample_count(
    region: VvcCtuRegion,
    format: VvcPictureFormat,
) -> u64 {
    let subsample_x = chroma_subsample_x(format.chroma_sampling) as u64;
    let subsample_y = chroma_subsample_y(format.chroma_sampling) as u64;
    let chroma_width = (region.geometry.width as u64) / subsample_x;
    let chroma_height = (region.geometry.height as u64) / subsample_y;
    chroma_width.saturating_mul(chroma_height).saturating_mul(2)
}

fn vvc_region_sse_against_reconstruction(
    source_frame: &VvcSampledFrame,
    reconstruction: &VvcReconstructionFrame,
    region: VvcCtuRegion,
) -> u64 {
    vvc_region_sse_with_limit(source_frame, reconstruction, region, None).unwrap_or(u64::MAX)
}

fn vvc_region_sse_with_limit(
    source_frame: &VvcSampledFrame,
    reconstruction: &VvcReconstructionFrame,
    region: VvcCtuRegion,
    max_abs_delta: Option<u16>,
) -> Option<u64> {
    if source_frame.geometry != reconstruction.geometry || source_frame.format != reconstruction.format {
        return None;
    }
    let width = region
        .geometry
        .width
        .min(source_frame.geometry.width.saturating_sub(region.origin_x));
    let height = region
        .geometry
        .height
        .min(source_frame.geometry.height.saturating_sub(region.origin_y));
    if width == 0 || height == 0 {
        return Some(0);
    }
    let mut sse = vvc_plane_region_sse_with_limit(
        &source_frame.luma,
        source_frame.geometry.width,
        &reconstruction.luma,
        reconstruction.luma_width(),
        region.origin_x,
        region.origin_y,
        width,
        height,
        max_abs_delta,
    )?;
    let subsample_x = chroma_subsample_x(source_frame.format.chroma_sampling);
    let subsample_y = chroma_subsample_y(source_frame.format.chroma_sampling);
    let chroma_x = region.origin_x / subsample_x;
    let chroma_y = region.origin_y / subsample_y;
    let chroma_width = width / subsample_x;
    let chroma_height = height / subsample_y;
    let chroma_stride = source_frame.geometry.width / subsample_x;
    let reconstruction_chroma_stride = reconstruction.chroma_width();
    sse = sse.saturating_add(vvc_plane_region_sse_with_limit(
        &source_frame.cb,
        chroma_stride,
        &reconstruction.cb,
        reconstruction_chroma_stride,
        chroma_x,
        chroma_y,
        chroma_width,
        chroma_height,
        max_abs_delta,
    )?);
    sse = sse.saturating_add(vvc_plane_region_sse_with_limit(
        &source_frame.cr,
        chroma_stride,
        &reconstruction.cr,
        reconstruction_chroma_stride,
        chroma_x,
        chroma_y,
        chroma_width,
        chroma_height,
        max_abs_delta,
    )?);
    Some(sse)
}

fn vvc_plane_region_sse_with_limit(
    source: &[VvcSample],
    source_stride: usize,
    reconstruction: &[VvcSample],
    reconstruction_stride: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    max_abs_delta: Option<u16>,
) -> Option<u64> {
    let mut sse = 0u64;
    for y in 0..height {
        let source_row = (start_y + y) * source_stride + start_x;
        let reconstruction_row = (start_y + y) * reconstruction_stride + start_x;
        for x in 0..width {
            let source_sample = source[source_row + x];
            let reconstruction_sample = reconstruction[reconstruction_row + x];
            if max_abs_delta.is_some_and(|limit| {
                source_sample.abs_diff(reconstruction_sample) > limit
            }) {
                return None;
            }
            let delta = i64::from(source_sample) - i64::from(reconstruction_sample);
            sse = sse.saturating_add((delta * delta) as u64);
        }
    }
    Some(sse)
}

fn vvc_predictive_luma_leaf_inter_skip_mask(
    current_source: &[u8],
    previous_source: &[u8],
    layout: PlanarYuvFrameLayout,
    region: VvcCtuRegion,
    luma_max_leaf_size: u16,
    chroma_sampling: ChromaSampling,
    dual_tree_intra: bool,
) -> Option<[bool; MAX_VVC_LUMA_TUS]> {
    if luma_max_leaf_size < VVC_CURRENT_MAX_LUMA_LEAF_SIZE {
        return None;
    }

    let shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: region.geometry.coded_width() as u16,
        visible_height: region.geometry.coded_height() as u16,
        chroma_sampling,
        dual_tree_intra,
    };
    let nodes = vvc_luma_transform_nodes_for_kind(
        shape,
        luma_max_leaf_size,
        VvcLumaSplitAvailabilityKind::Inter,
    );
    if nodes.is_empty() || nodes.len() > MAX_VVC_LUMA_TUS {
        return None;
    }

    let mut mask = [false; MAX_VVC_LUMA_TUS];
    let mut skipped = 0usize;
    for (idx, node) in nodes.into_iter().enumerate() {
        let origin_x = region.origin_x + usize::from(node.x);
        let origin_y = region.origin_y + usize::from(node.y);
        let width = usize::from(node.width).min(region.geometry.width.saturating_sub(node.x as usize));
        let height =
            usize::from(node.height).min(region.geometry.height.saturating_sub(node.y as usize));
        if width != 0
            && height != 0
            && layout.luma_regions_equal_between(
                current_source,
                origin_x,
                origin_y,
                previous_source,
                origin_x,
                origin_y,
                width,
                height,
            )
        {
            mask[idx] = true;
            skipped += 1;
        }
    }
    (skipped > 0).then_some(mask)
}

fn vvc_predictive_chroma_leaf_inter_skip_mask(
    current_source: &[u8],
    previous_source: &[u8],
    layout: PlanarYuvFrameLayout,
    region: VvcCtuRegion,
    chroma_sampling: ChromaSampling,
    dual_tree_intra: bool,
) -> Option<[bool; MAX_VVC_CHROMA_TUS]> {
    let shape = VvcCtuPartitionShape {
        root_width: VVC_CTU_SIZE as u16,
        root_height: VVC_CTU_SIZE as u16,
        visible_width: region.geometry.coded_width() as u16,
        visible_height: region.geometry.coded_height() as u16,
        chroma_sampling,
        dual_tree_intra,
    };
    let nodes = vvc_chroma_transform_nodes(shape);
    if nodes.is_empty() || nodes.len() > MAX_VVC_CHROMA_TUS {
        return None;
    }

    let mut mask = [false; MAX_VVC_CHROMA_TUS];
    let mut skipped = 0usize;
    for (idx, node) in nodes.into_iter().enumerate() {
        let origin_x = region.origin_x + usize::from(node.x);
        let origin_y = region.origin_y + usize::from(node.y);
        let width =
            usize::from(node.width).min(region.geometry.width.saturating_sub(node.x as usize));
        let height =
            usize::from(node.height).min(region.geometry.height.saturating_sub(node.y as usize));
        if width != 0
            && height != 0
            && layout.chroma_regions_equal_between(
                current_source,
                origin_x,
                origin_y,
                previous_source,
                origin_x,
                origin_y,
                width,
                height,
            )
        {
            mask[idx] = true;
            skipped += 1;
        }
    }
    (skipped > 0).then_some(mask)
}

fn vvc_predictive_inter_skip_region(region: VvcCtuRegion) -> bool {
    let coded_width = region.geometry.coded_width();
    let coded_height = region.geometry.coded_height();
    (1..=VVC_CTU_SIZE).contains(&coded_width) && (1..=VVC_CTU_SIZE).contains(&coded_height)
}

fn vvc_lossy_predictive_skip_max_abs_delta(bit_depth: SampleBitDepth) -> u16 {
    VVC_LOSSY_PREDICTIVE_SKIP_MAX_ABS_8BIT << u32::from(bit_depth.bits().saturating_sub(8))
}

#[cfg(any(test, feature = "bench-internals"))]
fn vvc_predictive_lossy_region_within_reconstruction_delta(
    current_source: &VvcSampledFrame,
    previous_reconstruction: &VvcReconstructionFrame,
    region: VvcCtuRegion,
    max_abs_delta: u16,
) -> bool {
    vvc_predictive_lossy_region_sse_if_within_reconstruction_delta(
        current_source,
        previous_reconstruction,
        region,
        max_abs_delta,
    )
    .is_some()
}

fn vvc_predictive_lossy_region_sse_if_within_reconstruction_delta(
    current_source: &VvcSampledFrame,
    previous_reconstruction: &VvcReconstructionFrame,
    region: VvcCtuRegion,
    max_abs_delta: u16,
) -> Option<u64> {
    let width = region
        .geometry
        .width
        .min(current_source.geometry.width.saturating_sub(region.origin_x));
    let height = region
        .geometry
        .height
        .min(current_source.geometry.height.saturating_sub(region.origin_y));
    if width == 0 || height == 0 {
        return None;
    }
    let sse = vvc_region_sse_with_limit(
        current_source,
        previous_reconstruction,
        region,
        Some(max_abs_delta),
    )?;
    Some(sse)
}

