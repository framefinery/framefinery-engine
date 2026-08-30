fn quantize_vvc_chroma_4x4_dc_level_from_sum(residual_sum: i64) -> i16 {
    // H.266 8.7.3 inverse coefficient scaling plus 8.7.4 inverse transform
    // reduces to reconstructed_residual = 8 * level for a 4x4 chroma TB at
    // chroma QP 34. The older exhaustive SSE search initialized at level 0 and
    // replaced only on strict improvement, so exact half-step ties keep the
    // earlier level.
    let level = if (-64..=64).contains(&residual_sum) {
        0
    } else if residual_sum > 64 {
        (residual_sum + 63) / 128
    } else {
        -(((-residual_sum) + 64) / 128)
    };
    level.clamp(
        i64::from(-VVC_CHROMA_DC_LEVEL_LIMIT),
        i64::from(VVC_CHROMA_DC_LEVEL_LIMIT),
    ) as i16
}

fn quantize_vvc_chroma_residual_dc_by_search(
    residual_sum: i64,
    original_sse: i64,
    sample_count: i64,
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    chroma_qp: i32,
) -> i16 {
    let dequant_params = vvc_transform_dequant_params(width, height, chroma_qp);
    if !chroma_dc_fast_search_allowed(width, height, bit_depth, dequant_params) {
        return quantize_vvc_chroma_residual_dc_by_exhaustive_search(
            residual_sum,
            original_sse,
            sample_count,
            width,
            height,
            bit_depth,
            dequant_params,
        );
    }

    let target_level = first_chroma_dc_level_at_or_above_target(
        residual_sum,
        sample_count,
        width,
        height,
        bit_depth,
        dequant_params,
    );
    let mut candidates = [0i16; 3];
    let mut candidate_count = 0usize;
    push_chroma_dc_candidate(&mut candidates, &mut candidate_count, 0);
    if target_level <= i32::from(VVC_CHROMA_DC_LEVEL_LIMIT) {
        let level = target_level as i16;
        push_chroma_dc_candidate(
            &mut candidates,
            &mut candidate_count,
            first_chroma_dc_level_with_reconstructed_residual(
                dc_only_residual_from_level_with_params(
                    level,
                    width,
                    height,
                    bit_depth,
                    dequant_params,
                ),
                width,
                height,
                bit_depth,
                dequant_params,
            ),
        );
    }
    if target_level > i32::from(-VVC_CHROMA_DC_LEVEL_LIMIT) {
        let level = (target_level - 1).min(i32::from(VVC_CHROMA_DC_LEVEL_LIMIT)) as i16;
        push_chroma_dc_candidate(
            &mut candidates,
            &mut candidate_count,
            first_chroma_dc_level_with_reconstructed_residual(
                dc_only_residual_from_level_with_params(
                    level,
                    width,
                    height,
                    bit_depth,
                    dequant_params,
                ),
                width,
                height,
                bit_depth,
                dequant_params,
            ),
        );
    }
    candidates[..candidate_count].sort_unstable();

    let mut best_level = 0;
    let mut best_sse = original_sse;

    for level in candidates.into_iter().take(candidate_count) {
        let sse = chroma_dc_sse_for_level(
            level,
            residual_sum,
            original_sse,
            sample_count,
            width,
            height,
            bit_depth,
            dequant_params,
        );
        if sse < best_sse {
            best_sse = sse;
            best_level = level;
        }
    }
    best_level
}

fn chroma_dc_fast_search_allowed(
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    dequant_params: VvcTransformDequantParams,
) -> bool {
    let min_reconstructed = dc_only_residual_from_level_with_params(
        -VVC_CHROMA_DC_LEVEL_LIMIT,
        width,
        height,
        bit_depth,
        dequant_params,
    );
    let max_reconstructed = dc_only_residual_from_level_with_params(
        VVC_CHROMA_DC_LEVEL_LIMIT,
        width,
        height,
        bit_depth,
        dequant_params,
    );
    min_reconstructed >= i64::from(i16::MIN) && max_reconstructed <= i64::from(i16::MAX)
}

fn first_chroma_dc_level_at_or_above_target(
    residual_sum: i64,
    sample_count: i64,
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    dequant_params: VvcTransformDequantParams,
) -> i32 {
    debug_assert!(sample_count > 0);
    let mut lo = i32::from(-VVC_CHROMA_DC_LEVEL_LIMIT);
    let mut hi = i32::from(VVC_CHROMA_DC_LEVEL_LIMIT) + 1;
    while lo < hi {
        let mid = lo + ((hi - lo) >> 1);
        let reconstructed = dc_only_residual_from_level_with_params(
            mid as i16,
            width,
            height,
            bit_depth,
            dequant_params,
        );
        if sample_count * reconstructed >= residual_sum {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo
}

fn first_chroma_dc_level_with_reconstructed_residual(
    target_residual: i64,
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    dequant_params: VvcTransformDequantParams,
) -> i16 {
    let mut lo = i32::from(-VVC_CHROMA_DC_LEVEL_LIMIT);
    let mut hi = i32::from(VVC_CHROMA_DC_LEVEL_LIMIT);
    while lo < hi {
        let mid = lo + ((hi - lo) >> 1);
        let reconstructed = dc_only_residual_from_level_with_params(
            mid as i16,
            width,
            height,
            bit_depth,
            dequant_params,
        );
        if reconstructed >= target_residual {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    lo as i16
}

fn push_chroma_dc_candidate(candidates: &mut [i16; 3], count: &mut usize, level: i16) {
    if candidates
        .iter()
        .take(*count)
        .any(|candidate| *candidate == level)
    {
        return;
    }
    debug_assert!(*count < candidates.len());
    candidates[*count] = level;
    *count += 1;
}

fn chroma_dc_sse_for_level(
    level: i16,
    residual_sum: i64,
    original_sse: i64,
    sample_count: i64,
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    dequant_params: VvcTransformDequantParams,
) -> i64 {
    let reconstructed =
        dc_only_residual_from_level_with_params(level, width, height, bit_depth, dequant_params);
    original_sse + (sample_count * reconstructed * reconstructed)
        - (2 * reconstructed * residual_sum)
}

fn quantize_vvc_chroma_residual_dc_by_exhaustive_search(
    residual_sum: i64,
    original_sse: i64,
    sample_count: i64,
    width: u16,
    height: u16,
    bit_depth: SampleBitDepth,
    dequant_params: VvcTransformDequantParams,
) -> i16 {
    let mut best_level = 0;
    let mut best_sse = original_sse;

    for level in -VVC_CHROMA_DC_LEVEL_LIMIT..=VVC_CHROMA_DC_LEVEL_LIMIT {
        let sse = chroma_dc_sse_for_level(
            level,
            residual_sum,
            original_sse,
            sample_count,
            width,
            height,
            bit_depth,
            dequant_params,
        );
        if sse < best_sse {
            best_sse = sse;
            best_level = level;
        }
    }
    best_level
}
