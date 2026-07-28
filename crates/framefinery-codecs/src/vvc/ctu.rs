#[derive(Debug, Clone)]
struct VvcQuantizedCtuLeafDecision {
    quantized: VvcQuantizedColor,
    luma_max_leaf_size: u16,
}

fn quantize_vvc_ctu_with_luma_leaf_selection(
    source_frame: &VvcSampledFrame,
    frame_recon: &mut VvcReconstructionFrame,
    region: VvcCtuRegion,
    policy: VvcResidualCodingPolicy,
    luma_qp: i32,
    chroma_qp: i32,
    luma_mode_search_state: &mut VvcLumaModeSearchState,
    transform_skip_quant_tables: &VvcTransformSkipQuantTables,
) -> VvcQuantizedCtuLeafDecision {
    let luma_max_leaf_size =
        select_vvc_luma_max_leaf_size_for_ctu(policy, source_frame, region, luma_qp);
    let policy = policy.with_luma_max_leaf_size(luma_max_leaf_size);
    let quantized = quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes(
        source_frame,
        frame_recon,
        region,
        policy,
        luma_qp,
        chroma_qp,
        luma_mode_search_state,
        transform_skip_quant_tables,
    );
    VvcQuantizedCtuLeafDecision {
        quantized,
        luma_max_leaf_size,
    }
}

fn vvc_ctu_regions(geometry: VvcVideoGeometry) -> impl Iterator<Item = VvcCtuRegion> {
    let cols = vvc_picture_ctu_cols(geometry);
    let rows = vvc_picture_ctu_rows(geometry);
    (0..rows).flat_map(move |ctu_y| {
        (0..cols).map(move |ctu_x| {
            let origin_x = ctu_x * VVC_CTU_SIZE;
            let origin_y = ctu_y * VVC_CTU_SIZE;
            let width = VVC_CTU_SIZE.min(geometry.width.saturating_sub(origin_x).max(1));
            let height = VVC_CTU_SIZE.min(geometry.height.saturating_sub(origin_y).max(1));
            VvcCtuRegion {
                slice_address: ctu_y * cols + ctu_x,
                origin_x,
                origin_y,
                geometry: VvcVideoGeometry { width, height },
            }
        })
    })
}

fn select_vvc_luma_max_leaf_size_for_ctu(
    policy: VvcResidualCodingPolicy,
    source_frame: &VvcSampledFrame,
    region: VvcCtuRegion,
    luma_qp: i32,
) -> u16 {
    let default_leaf_size = policy.luma_max_leaf_size();
    if policy.residual_mode() == VvcResidualCodingMode::Lossless
        && policy.fast_search() == VvcFastSearch::LosslessSpeed
    {
        return VVC_CURRENT_MAX_LUMA_LEAF_SIZE;
    }
    if policy.residual_mode() != VvcResidualCodingMode::Lossy
        || default_leaf_size <= VVC_LOSSLESS_LUMA_LEAF_SIZE
    {
        return default_leaf_size;
    }

    let split_gain = vvc_luma_split_dc_sse_gain(
        source_frame,
        region,
        VVC_CURRENT_MAX_LUMA_LEAF_SIZE,
        VVC_LOSSLESS_LUMA_LEAF_SIZE,
    );
    if split_gain.block_count == 0 {
        return default_leaf_size;
    }

    let lambda = vvc_luma_leaf_split_lambda(luma_qp, source_frame.format.bit_depth);
    let split_rate_penalty = lambda
        .saturating_mul(split_gain.block_count as u64)
        .saturating_mul(512);
    let meaningful_distortion_gain = split_gain.total_sse / 8;
    if split_gain.sse_gain > split_rate_penalty && split_gain.sse_gain > meaningful_distortion_gain
    {
        return VVC_LOSSLESS_LUMA_LEAF_SIZE;
    }

    default_leaf_size
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct VvcLumaSplitSseGain {
    block_count: usize,
    total_sse: u64,
    sse_gain: u64,
}

fn vvc_luma_split_dc_sse_gain(
    source_frame: &VvcSampledFrame,
    region: VvcCtuRegion,
    parent_size: u16,
    child_size: u16,
) -> VvcLumaSplitSseGain {
    debug_assert!(parent_size > child_size);
    debug_assert_eq!(parent_size % child_size, 0);
    let x_end = region
        .origin_x
        .saturating_add(region.geometry.width)
        .min(source_frame.geometry.width);
    let y_end = region
        .origin_y
        .saturating_add(region.geometry.height)
        .min(source_frame.geometry.height);
    if x_end <= region.origin_x || y_end <= region.origin_y {
        return VvcLumaSplitSseGain::default();
    }

    let mut gain = VvcLumaSplitSseGain::default();
    let parent_size = usize::from(parent_size);
    let child_size = usize::from(child_size);
    for y in (region.origin_y..y_end).step_by(parent_size) {
        let block_height = (y_end - y).min(parent_size);
        if block_height < parent_size {
            continue;
        }
        for x in (region.origin_x..x_end).step_by(parent_size) {
            let block_width = (x_end - x).min(parent_size);
            if block_width < parent_size {
                continue;
            }
            let parent_sse = vvc_luma_block_mean_sse(source_frame, x, y, parent_size, parent_size);
            let mut child_sse_sum = 0u64;
            for child_y in (y..y + parent_size).step_by(child_size) {
                for child_x in (x..x + parent_size).step_by(child_size) {
                    child_sse_sum = child_sse_sum.saturating_add(vvc_luma_block_mean_sse(
                        source_frame,
                        child_x,
                        child_y,
                        child_size,
                        child_size,
                    ));
                }
            }
            gain.block_count += 1;
            gain.total_sse = gain.total_sse.saturating_add(parent_sse);
            gain.sse_gain = gain
                .sse_gain
                .saturating_add(parent_sse.saturating_sub(child_sse_sum));
        }
    }
    gain
}

fn vvc_luma_block_mean_sse(
    source_frame: &VvcSampledFrame,
    origin_x: usize,
    origin_y: usize,
    width: usize,
    height: usize,
) -> u64 {
    debug_assert!(origin_x + width <= source_frame.geometry.width);
    debug_assert!(origin_y + height <= source_frame.geometry.height);
    let stride = source_frame.geometry.width;
    let sample_count = width * height;
    let mut sum = 0u64;
    for y in origin_y..origin_y + height {
        let row = y * stride;
        for x in origin_x..origin_x + width {
            sum += u64::from(source_frame.luma[row + x]);
        }
    }
    let mean = ((sum + (sample_count / 2) as u64) / sample_count as u64) as i64;
    let mut sse = 0u64;
    for y in origin_y..origin_y + height {
        let row = y * stride;
        for x in origin_x..origin_x + width {
            let diff = i64::from(source_frame.luma[row + x]) - mean;
            sse = sse.saturating_add((diff * diff) as u64);
        }
    }
    sse
}

fn vvc_luma_leaf_split_lambda(qp: i32, bit_depth: SampleBitDepth) -> u64 {
    let qp_scale = 1u64 << (qp.clamp(0, 63) / 6);
    let bit_depth_delta = u32::from(bit_depth.bits().saturating_sub(8));
    let bit_depth_scale = 1u64 << (bit_depth_delta * 2);
    qp_scale.saturating_mul(bit_depth_scale)
}
