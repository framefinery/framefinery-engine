// Keep 4:4:4 palette FSC gated until its chroma coefficient path is
// reference-clean on larger screen-content frames.
const AV2_ENABLE_LUMA_PALETTE_FSC_444: bool = false;
const AV2_STATIC_CDF_INTER_IS_INTER_BASE: usize = 380;
const AV2_STATIC_CDF_INTER_SKIP_TXFM_BASE: usize = 384;
const AV2_STATIC_CDF_INTER_SINGLE_REF_BASE: usize = 390;
const AV2_STATIC_CDF_INTER_SINGLE_MODE_BASE: usize = 394;
const AV2_STATIC_CDF_INTER_DRL_BASE: usize = 400;
const AV2_STATIC_CDF_INTRABC_USE_BASE: usize = 405;
const AV2_STATIC_CDF_INTRABC_SKIP_TXFM_BASE: usize = 408;
const AV2_STATIC_CDF_INTRABC_MODE: usize = 414;
const AV2_STATIC_CDF_PARTITION_DO_SPLIT_BASE: usize = 415;
const AV2_STATIC_CDF_PARTITION_RECT_TYPE_BASE: usize = 479;
const AV2_STATIC_CDF_INTRA_FSC_MODE_BASE: usize = 543;
const AV2_STATIC_CDF_INTRA_Y_MODE_IDX_OFFSET_BASE: usize = 567;
const AV2_STATIC_CDF_INTRA_Y_MODE_IDX_BASE: usize = 570;
const AV2_STATIC_CDF_INTRA_UV_MODE_IDX_BASE: usize = 573;

#[inline(always)]
fn inter_is_inter_static_cdf_key(ctx: usize) -> usize {
    AV2_STATIC_CDF_INTER_IS_INTER_BASE + ctx
}

#[inline(always)]
fn inter_skip_txfm_static_cdf_key(ctx: usize) -> usize {
    AV2_STATIC_CDF_INTER_SKIP_TXFM_BASE + ctx
}

#[inline(always)]
fn inter_single_ref_static_cdf_key(ctx: usize) -> usize {
    AV2_STATIC_CDF_INTER_SINGLE_REF_BASE + ctx
}

#[inline(always)]
fn inter_single_mode_static_cdf_key(ctx: usize) -> usize {
    AV2_STATIC_CDF_INTER_SINGLE_MODE_BASE + ctx
}

#[inline(always)]
fn inter_drl_static_cdf_key(ctx: usize) -> usize {
    AV2_STATIC_CDF_INTER_DRL_BASE + ctx
}

#[inline(always)]
fn intrabc_use_static_cdf_key(ctx: usize) -> usize {
    AV2_STATIC_CDF_INTRABC_USE_BASE + ctx
}

#[inline(always)]
fn intrabc_skip_txfm_static_cdf_key(ctx: usize) -> usize {
    AV2_STATIC_CDF_INTRABC_SKIP_TXFM_BASE + ctx
}

#[inline(always)]
fn partition_do_split_static_cdf_key(ctx: usize) -> usize {
    AV2_STATIC_CDF_PARTITION_DO_SPLIT_BASE + ctx
}

#[inline(always)]
fn partition_rect_type_static_cdf_key(ctx: usize) -> usize {
    AV2_STATIC_CDF_PARTITION_RECT_TYPE_BASE + ctx
}

#[inline(always)]
fn intra_fsc_mode_static_cdf_key(context: usize, size_group: usize) -> usize {
    AV2_STATIC_CDF_INTRA_FSC_MODE_BASE + context * 6 + size_group
}

#[inline(always)]
fn intra_y_mode_idx_offset_static_cdf_key(mode_context: u8) -> usize {
    AV2_STATIC_CDF_INTRA_Y_MODE_IDX_OFFSET_BASE + usize::from(mode_context)
}

#[inline(always)]
fn intra_y_mode_idx_static_cdf_key(mode_context: u8) -> usize {
    AV2_STATIC_CDF_INTRA_Y_MODE_IDX_BASE + usize::from(mode_context)
}

#[inline(always)]
fn intra_uv_mode_idx_static_cdf_key(mode_context: usize) -> usize {
    AV2_STATIC_CDF_INTRA_UV_MODE_IDX_BASE + mode_context
}

#[cfg(test)]
pub(crate) fn av2_black_444_tile_entropy_payload(
    geometry: Av2VideoGeometry,
    profile: Av2Black444MvpProfile,
) -> Av2EntropyPayload {
    av2_black_444_tile_entropy_payload_for_region_with_fields(
        Av2TileRegion::root(geometry),
        profile,
        true,
    )
}

pub(crate) fn av2_black_444_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    record_fields: bool,
) -> Av2EntropyPayload {
    av2_black_444_tile_entropy_payload_for_region_with_intrabc_and_fields(
        region,
        profile,
        false,
        record_fields,
    )
}

pub(crate) fn av2_black_444_tile_entropy_payload_for_region_with_intrabc_and_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    allow_intrabc: bool,
    record_fields: bool,
) -> Av2EntropyPayload {
    let plan = Av2Black444TilePlan::for_region(
        region,
        profile,
        Av2ChromaFormat::Yuv444,
        false,
        allow_intrabc,
        None,
        None,
    );
    let mut writer =
        Av2EntropyWriter::with_cdf_updates_and_fields(!profile.disable_cdf_update, record_fields);
    plan.write_entropy(&mut writer, None, None);
    writer.finish()
}

#[cfg(any(test, feature = "bench-internals"))]
pub(crate) fn av2_black_tile_entropy_payload_for_region(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    chroma_format: Av2ChromaFormat,
) -> Av2EntropyPayload {
    av2_black_tile_entropy_payload_for_region_with_fields(region, profile, chroma_format, true)
}

#[cfg(any(test, feature = "bench-internals"))]
pub(crate) fn av2_black_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    chroma_format: Av2ChromaFormat,
    record_fields: bool,
) -> Av2EntropyPayload {
    let plan =
        Av2Black444TilePlan::for_region(region, profile, chroma_format, false, false, None, None);
    let mut writer =
        Av2EntropyWriter::with_cdf_updates_and_fields(!profile.disable_cdf_update, record_fields);
    plan.write_entropy(&mut writer, None, None);
    writer.finish()
}

pub(crate) fn av2_luma_palette_444_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    allow_intrabc: bool,
    palette: &Av2LumaPalette444,
    ibc: Option<&Av2LocalIbc444>,
    record_fields: bool,
) -> Av2EntropyPayload {
    let mut best: Option<Av2EntropyPayload> = None;
    // Larger merged palette leaves currently add mode-search cost without a
    // measured size win on 1080p screen-content baselines.
    for partition_policy in [Av2PartitionPolicy::Fixed8x8Leaves] {
        let payload = av2_luma_palette_444_tile_entropy_payload_for_region_with_policy(
            region,
            profile,
            allow_intrabc,
            palette,
            ibc,
            partition_policy,
            record_fields,
        );
        let replace = best.as_ref().is_none_or(|best_payload| {
            (payload.bytes.len(), payload.symbol_bits)
                < (best_payload.bytes.len(), best_payload.symbol_bits)
        });
        if replace {
            best = Some(payload);
        }
    }

    best.expect("AV2 4:4:4 palette has fixed partition candidates")
}

fn av2_luma_palette_444_tile_entropy_payload_for_region_with_policy(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    allow_intrabc: bool,
    palette: &Av2LumaPalette444,
    ibc: Option<&Av2LocalIbc444>,
    partition_policy: Av2PartitionPolicy,
    record_fields: bool,
) -> Av2EntropyPayload {
    let plan = Av2Black444TilePlan::for_region_with_partition_policy(
        region,
        profile,
        Av2ChromaFormat::Yuv444,
        partition_policy,
        true,
        allow_intrabc,
        ibc,
        Some(palette),
    );
    let mut writer =
        Av2EntropyWriter::with_cdf_updates_and_fields(!profile.disable_cdf_update, record_fields);
    plan.write_entropy(&mut writer, Some(palette), ibc);
    writer.finish()
}

#[cfg(any(test, feature = "bench-internals"))]
pub(crate) fn av2_lossy_subsampled_tile_entropy_payload_for_region(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    geometry: Av2VideoGeometry,
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
    source: &[u8],
    recon: &mut [u8],
    qp: u8,
    base_qindex: u16,
) -> Av2EntropyPayload {
    av2_lossy_subsampled_tile_entropy_payload_for_region_with_fields(
        region,
        profile,
        geometry,
        chroma_format,
        bit_depth,
        source,
        recon,
        qp,
        base_qindex,
        true,
    )
}

pub(crate) fn av2_lossy_subsampled_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    geometry: Av2VideoGeometry,
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
    source: &[u8],
    recon: &mut [u8],
    qp: u8,
    base_qindex: u16,
    record_fields: bool,
) -> Av2EntropyPayload {
    let adaptive_partition_features =
        adaptive_partition_features_for_source(region, geometry, bit_depth, source, false, None);
    let plan = Av2Black444TilePlan::for_region_with_partition_policy_and_features(
        region,
        profile,
        chroma_format,
        Av2PartitionPolicy::Fixed8x8Leaves,
        false,
        false,
        None,
        None,
        Some(adaptive_partition_features),
        None,
    );
    let mut writer =
        Av2EntropyWriter::with_cdf_updates_and_fields(!profile.disable_cdf_update, record_fields);
    let mut lossy = Av2LossySubsampledTileState::new(
        geometry,
        region,
        chroma_format,
        bit_depth,
        source,
        recon,
        qp,
        base_qindex,
    );
    plan.write_lossy_subsampled_entropy(&mut writer, &mut lossy);
    writer.finish()
}

pub(crate) fn av2_lossless_subsampled_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    geometry: Av2VideoGeometry,
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
    source: &[u8],
    recon: &mut [u8],
    palette: Option<&Av2LumaPalette444>,
    ibc: Option<&Av2LocalIbc444>,
    record_fields: bool,
) -> Av2EntropyPayload {
    debug_assert!(matches!(
        chroma_format,
        Av2ChromaFormat::Yuv420 | Av2ChromaFormat::Yuv422 | Av2ChromaFormat::Yuv444
    ));
    if use_fast_lossless_subsampled_path(region) {
        return av2_lossless_subsampled_tile_entropy_payload_for_region_with_policy(
            region,
            profile,
            geometry,
            chroma_format,
            bit_depth,
            source,
            recon,
            palette,
            if ibc.is_some() {
                Av2PartitionPolicy::Fixed8x8Leaves
            } else {
                Av2PartitionPolicy::AdaptiveScreenContent
            },
            Av2LosslessSubsampledModeSearch::FastScreenContent,
            ibc,
            record_fields,
            true,
            false,
        );
    }

    let mut best: Option<(Av2EntropyPayload, Vec<u8>)> = None;
    // Larger subsampled lossless leaves can emit AVM-rejected edge-block
    // syntax while still matching the internal reconstruction.
    for partition_policy in [Av2PartitionPolicy::Fixed8x8Leaves] {
        let mut candidate_recon = recon.to_vec();
        let payload = av2_lossless_subsampled_tile_entropy_payload_for_region_with_policy(
            region,
            profile,
            geometry,
            chroma_format,
            bit_depth,
            source,
            &mut candidate_recon,
            palette,
            partition_policy,
            Av2LosslessSubsampledModeSearch::Exhaustive,
            ibc,
            record_fields,
            true,
            false,
        );
        let replace = best.as_ref().is_none_or(|(best_payload, _)| {
            (payload.bytes.len(), payload.symbol_bits)
                < (best_payload.bytes.len(), best_payload.symbol_bits)
        });
        if replace {
            best = Some((payload, candidate_recon));
        }
    }

    let (payload, candidate_recon) =
        best.expect("AV2 planar lossless has fixed partition candidates");
    recon.copy_from_slice(&candidate_recon);
    payload
}

pub(crate) fn av2_lossless_subsampled_fast_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    geometry: Av2VideoGeometry,
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
    source: &[u8],
    palette: Option<&Av2LumaPalette444>,
    record_fields: bool,
) -> Av2EntropyPayload {
    debug_assert!(matches!(
        chroma_format,
        Av2ChromaFormat::Yuv420 | Av2ChromaFormat::Yuv422 | Av2ChromaFormat::Yuv444
    ));
    debug_assert!(use_fast_lossless_subsampled_path(region));
    let mut scratch_recon = [];
    av2_lossless_subsampled_tile_entropy_payload_for_region_with_policy(
        region,
        profile,
        geometry,
        chroma_format,
        bit_depth,
        source,
        &mut scratch_recon,
        palette,
        Av2PartitionPolicy::AdaptiveScreenContent,
        Av2LosslessSubsampledModeSearch::FastScreenContent,
        None,
        record_fields,
        false,
        false,
    )
}

pub(crate) fn av2_lossless_zero_mv_inter_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    chroma_format: Av2ChromaFormat,
    record_fields: bool,
) -> Av2EntropyPayload {
    let plan = Av2Black444TilePlan::for_region_with_partition_policy(
        region,
        profile,
        chroma_format,
        Av2PartitionPolicy::Fixed8x8Leaves,
        false,
        false,
        None,
        None,
    );
    let mut writer =
        Av2EntropyWriter::with_cdf_updates_and_fields(!profile.disable_cdf_update, record_fields);
    plan.write_lossless_zero_mv_inter_entropy(&mut writer, 1);
    writer.finish()
}

pub(crate) fn av2_lossless_mixed_inter_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    chroma_format: Av2ChromaFormat,
    block_modes: &Av2LosslessInterTileBlockModes,
    record_fields: bool,
) -> Av2EntropyPayload {
    let plan = Av2Black444TilePlan::for_region_with_inter_partition_modes(
        region,
        profile,
        chroma_format,
        block_modes,
    );
    let mut writer =
        Av2EntropyWriter::with_cdf_updates_and_fields(!profile.disable_cdf_update, record_fields);
    plan.write_lossless_mixed_inter_entropy(&mut writer, 1, block_modes);
    writer.finish()
}

pub(crate) fn av2_lossless_mixed_inter_intra_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    geometry: Av2VideoGeometry,
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
    source: &[u8],
    reference: &[u8],
    recon: &mut [u8],
    palette: Option<&Av2LumaPalette444>,
    block_modes: &Av2LosslessInterTileBlockModes,
    record_fields: bool,
) -> Av2EntropyPayload {
    let plan = Av2Black444TilePlan::for_region_with_inter_partition_modes(
        region,
        profile,
        chroma_format,
        block_modes,
    );
    let mut writer =
        Av2EntropyWriter::with_cdf_updates_and_fields(!profile.disable_cdf_update, record_fields);
    let mut lossless = Av2LosslessSubsampledTileState::new(
        geometry,
        region,
        chroma_format,
        bit_depth,
        Av2LosslessSubsampledModeSearch::FastScreenContent,
        source,
        recon,
    );
    plan.write_lossless_mixed_inter_intra_entropy(
        &mut writer,
        1,
        block_modes,
        &mut lossless,
        reference,
        palette,
        false,
    );
    lossless.copy_source_to_recon_region();
    writer.finish()
}

pub(crate) fn av2_lossy_fixed_inter_intra_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    geometry: Av2VideoGeometry,
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
    source: &[u8],
    reference: &[u8],
    recon: &mut [u8],
    block_modes: &Av2LosslessInterTileBlockModes,
    qp: u8,
    base_qindex: u16,
    record_fields: bool,
) -> Av2EntropyPayload {
    let plan = Av2Black444TilePlan::for_region_with_fixed_inter_partition_modes(
        region,
        profile,
        chroma_format,
        block_modes,
    );
    let mut writer =
        Av2EntropyWriter::with_cdf_updates_and_fields(!profile.disable_cdf_update, record_fields);
    let mut lossy = Av2LossySubsampledTileState::new(
        geometry,
        region,
        chroma_format,
        bit_depth,
        source,
        recon,
        qp,
        base_qindex,
    );
    plan.write_lossy_zero_mv_residual_inter_entropy(&mut writer, 1, block_modes, &mut lossy, reference);
    writer.finish()
}

pub(crate) fn av2_lossless_subsampled_regular_inter_intra_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    geometry: Av2VideoGeometry,
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
    source: &[u8],
    recon: &mut [u8],
    palette: Option<&Av2LumaPalette444>,
    record_fields: bool,
) -> Av2EntropyPayload {
    av2_lossless_subsampled_tile_entropy_payload_for_region_with_policy(
        region,
        profile,
        geometry,
        chroma_format,
        bit_depth,
        source,
        recon,
        palette,
        Av2PartitionPolicy::AdaptiveScreenContent,
        Av2LosslessSubsampledModeSearch::FastScreenContent,
        None,
        record_fields,
        true,
        true,
    )
}

pub(crate) fn av2_lossless_new_mv_inter_tile_entropy_payload_for_region_with_fields(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    chroma_format: Av2ChromaFormat,
    mv_row_px: i16,
    mv_col_px: i16,
    record_fields: bool,
) -> Av2EntropyPayload {
    let plan = Av2Black444TilePlan::for_region_with_partition_policy(
        region,
        profile,
        chroma_format,
        Av2PartitionPolicy::Fixed8x8Leaves,
        false,
        false,
        None,
        None,
    );
    let mut writer =
        Av2EntropyWriter::with_cdf_updates_and_fields(!profile.disable_cdf_update, record_fields);
    plan.write_lossless_new_mv_inter_entropy(&mut writer, 1, mv_row_px, mv_col_px);
    writer.finish()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Av2LosslessInterBlockMode {
    Intra,
    ZeroMv,
    ZeroMvResidual,
    NewMv { row_px: i16, col_px: i16 },
    NewMvResidual { row_px: i16, col_px: i16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Av2LosslessInterTileBlockModes {
    blocks_wide: usize,
    blocks_high: usize,
    blocks: Vec<Av2LosslessInterBlockMode>,
}

impl Av2LosslessInterTileBlockModes {
    pub(crate) fn new(
        blocks_wide: usize,
        blocks_high: usize,
        blocks: Vec<Av2LosslessInterBlockMode>,
    ) -> Self {
        assert_eq!(
            blocks.len(),
            blocks_wide * blocks_high,
            "AV2 inter tile mode map must cover every 8x8 block"
        );
        Self {
            blocks_wide,
            blocks_high,
            blocks,
        }
    }

    #[cfg(test)]
    pub(crate) fn block_mode_at(
        &self,
        block_x: usize,
        block_y: usize,
    ) -> Option<Av2LosslessInterBlockMode> {
        (block_x < self.blocks_wide && block_y < self.blocks_high)
            .then_some(self.blocks[block_y * self.blocks_wide + block_x])
    }

    fn mode_for_decision(&self, decision: Av2TileDecision) -> Av2LosslessInterBlockMode {
        self.homogeneous_mode_for_leaf(decision.row, decision.col, decision.block_size)
            .expect("AV2 inter partition leaf must cover one homogeneous 8x8 mode region")
    }

    fn homogeneous_mode_for_leaf(
        &self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
    ) -> Option<Av2LosslessInterBlockMode> {
        debug_assert_eq!(row_mi % 2, 0);
        debug_assert_eq!(col_mi % 2, 0);
        debug_assert_eq!(block_size.width % MVP_LEAF_BLOCK_SIZE, 0);
        debug_assert_eq!(block_size.height % MVP_LEAF_BLOCK_SIZE, 0);
        let row0 = row_mi / 2;
        let col0 = col_mi / 2;
        let rows = block_size.height / MVP_LEAF_BLOCK_SIZE;
        let cols = block_size.width / MVP_LEAF_BLOCK_SIZE;
        if row0 + rows > self.blocks_high || col0 + cols > self.blocks_wide {
            return None;
        }

        let first = self.blocks[row0 * self.blocks_wide + col0];
        for row in row0..row0 + rows {
            for col in col0..col0 + cols {
                if self.blocks[row * self.blocks_wide + col] != first {
                    return None;
                }
            }
        }
        Some(first)
    }

    fn homogeneous_skip_mode_for_leaf(
        &self,
        row_mi: usize,
        col_mi: usize,
        block_size: Av2MvpBlockSize,
    ) -> Option<Av2LosslessInterBlockMode> {
        let mode = self.homogeneous_mode_for_leaf(row_mi, col_mi, block_size)?;
        matches!(
            mode,
            Av2LosslessInterBlockMode::ZeroMv | Av2LosslessInterBlockMode::NewMv { .. }
        )
        .then_some(mode)
    }
}

const AV2_FAST_LOSSLESS_SUBSAMPLED_MIN_PIXELS: usize = 128 * 128;
const AV2_SCREEN_ADAPTIVE_LEAF_SIZE: usize = 64;
const AV2_SCREEN_ADAPTIVE_BASE_LEAF_SIZE: usize = 16;
const AV2_SCREEN_ADAPTIVE_SAMPLE_STEP: usize = 4;
const AV2_SCREEN_ADAPTIVE_UNIQUE_LIMIT: usize = 8;
const AV2_SCREEN_ADAPTIVE_GRADIENT_UNIQUE_LIMIT: usize = 16;
const AV2_SCREEN_ADAPTIVE_GRADIENT_Q8_LIMIT: u64 = 192;
const AV2_SCREEN_ADAPTIVE_RANGE_LIMIT: u16 = 8;
const AV2_LOSSLESS_PALETTE_PARTITION_UNIQUE_LIMIT: usize = 4;

fn use_fast_lossless_subsampled_path(region: Av2TileRegion) -> bool {
    region.width * region.height >= AV2_FAST_LOSSLESS_SUBSAMPLED_MIN_PIXELS
}

fn adaptive_partition_features_for_source(
    region: Av2TileRegion,
    geometry: Av2VideoGeometry,
    bit_depth: SampleBitDepth,
    source: &[u8],
    palette_enabled: bool,
    ibc: Option<&Av2LocalIbc444>,
) -> Av2AdaptivePartitionFeatures {
    let cols = region.width.div_ceil(AV2_SCREEN_ADAPTIVE_LEAF_SIZE);
    let rows = region.height.div_ceil(AV2_SCREEN_ADAPTIVE_LEAF_SIZE);
    let mut simple_leaves = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        let y0 = region.origin_y + row * AV2_SCREEN_ADAPTIVE_LEAF_SIZE;
        let height = (region.origin_y + region.height - y0).min(AV2_SCREEN_ADAPTIVE_LEAF_SIZE);
        for col in 0..cols {
            let x0 = region.origin_x + col * AV2_SCREEN_ADAPTIVE_LEAF_SIZE;
            let width = (region.origin_x + region.width - x0).min(AV2_SCREEN_ADAPTIVE_LEAF_SIZE);
            simple_leaves.push(luma_region_is_simple_for_adaptive_leaf(
                geometry, bit_depth, source, x0, y0, width, height,
            ));
        }
    }
    let forced_micro_cols = region.width / MVP_LEAF_BLOCK_SIZE;
    let forced_micro_rows = region.height / MVP_LEAF_BLOCK_SIZE;
    let mut forced_micro_blocks = Vec::new();
    if palette_enabled || ibc.is_some() {
        forced_micro_blocks.reserve(forced_micro_cols * forced_micro_rows);
        for row in 0..forced_micro_rows {
            let y0 = region.origin_y + row * MVP_LEAF_BLOCK_SIZE;
            for col in 0..forced_micro_cols {
                let x0 = region.origin_x + col * MVP_LEAF_BLOCK_SIZE;
                let palette_forced = palette_enabled
                    && lossless_luma_8x8_is_palette_worthy(
                        geometry, bit_depth, source, x0, y0,
                    );
                let ibc_forced = ibc
                    .and_then(|ibc| ibc.candidate_copy(x0, y0))
                    .is_some();
                forced_micro_blocks.push(palette_forced || ibc_forced);
            }
        }
    }
    Av2AdaptivePartitionFeatures {
        simple_leaves,
        cols,
        leaf_size: AV2_SCREEN_ADAPTIVE_LEAF_SIZE,
        forced_micro_blocks,
        forced_micro_cols,
    }
}

fn lossless_luma_8x8_is_palette_worthy(
    geometry: Av2VideoGeometry,
    bit_depth: SampleBitDepth,
    source: &[u8],
    x0: usize,
    y0: usize,
) -> bool {
    let mut values = [0u16; AV2_LUMA_PALETTE_MAX_COLORS + 1];
    let mut unique = 0usize;
    for y in y0..(y0 + MVP_LEAF_BLOCK_SIZE) {
        for x in x0..(x0 + MVP_LEAF_BLOCK_SIZE) {
            let sample = read_planar_sample(source, y * geometry.width + x, bit_depth);
            if values[..unique].contains(&sample) {
                continue;
            }
            if unique == values.len() {
                return false;
            }
            values[unique] = sample;
            unique += 1;
        }
    }
    (2..=AV2_LOSSLESS_PALETTE_PARTITION_UNIQUE_LIMIT).contains(&unique)
}

fn luma_region_is_simple_for_adaptive_leaf(
    geometry: Av2VideoGeometry,
    bit_depth: SampleBitDepth,
    source: &[u8],
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
) -> bool {
    debug_assert!(width <= AV2_SCREEN_ADAPTIVE_LEAF_SIZE);
    debug_assert!(height <= AV2_SCREEN_ADAPTIVE_LEAF_SIZE);
    let normalize_shift = bit_depth.bits().saturating_sub(8);
    let mut seen = [false; 256];
    let mut unique = 0usize;
    let mut min_sample = u16::MAX;
    let mut max_sample = 0u16;
    let mut gradient_sum = 0u64;
    let mut gradient_edges = 0u64;
    let mut above = [0u16; AV2_SCREEN_ADAPTIVE_LEAF_SIZE / AV2_SCREEN_ADAPTIVE_SAMPLE_STEP];
    for (sample_row, y) in (y0..(y0 + height))
        .step_by(AV2_SCREEN_ADAPTIVE_SAMPLE_STEP)
        .enumerate()
    {
        let mut prev = None;
        for (sample_col, x) in (x0..(x0 + width))
            .step_by(AV2_SCREEN_ADAPTIVE_SAMPLE_STEP)
            .enumerate()
        {
            let sample =
                read_planar_sample(source, y * geometry.width + x, bit_depth) >> normalize_shift;
            let sample_index = usize::from(sample);
            if !seen[sample_index] {
                seen[sample_index] = true;
                unique += 1;
            }
            min_sample = min_sample.min(sample);
            max_sample = max_sample.max(sample);
            if let Some(left) = prev {
                gradient_sum += u64::from(sample.abs_diff(left));
                gradient_edges += 1;
            }
            if sample_row > 0 {
                gradient_sum += u64::from(sample.abs_diff(above[sample_col]));
                gradient_edges += 1;
            }
            above[sample_col] = sample;
            prev = Some(sample);
        }
    }

    let range = max_sample - min_sample;
    let gradient_q8 = if gradient_edges == 0 {
        0
    } else {
        (gradient_sum * 256) / gradient_edges
    };
    unique <= AV2_SCREEN_ADAPTIVE_UNIQUE_LIMIT
        || range <= AV2_SCREEN_ADAPTIVE_RANGE_LIMIT
        || (unique <= AV2_SCREEN_ADAPTIVE_GRADIENT_UNIQUE_LIMIT
            && gradient_q8 <= AV2_SCREEN_ADAPTIVE_GRADIENT_Q8_LIMIT)
}

fn choose_adaptive_screen_content_partition(
    row_mi: usize,
    col_mi: usize,
    block_size: Av2MvpBlockSize,
    visible_rows_mi: usize,
    visible_cols_mi: usize,
    features: &Av2AdaptivePartitionFeatures,
) -> Av2MvpPartition {
    if !block_size.is_partition_point() {
        return Av2MvpPartition::None;
    }

    let allowed = allowed_partitions(row_mi, col_mi, block_size, visible_rows_mi, visible_cols_mi);
    if let Some(forced) =
        forced_boundary_partition(row_mi, col_mi, block_size, visible_rows_mi, visible_cols_mi)
    {
        if allowed.contains(forced) {
            return forced;
        }
    }
    if let Some(only_allowed) = allowed.only() {
        return only_allowed;
    }
    if allowed.none
        && block_size.width <= AV2_SCREEN_ADAPTIVE_LEAF_SIZE
        && block_size.height <= AV2_SCREEN_ADAPTIVE_LEAF_SIZE
        && features.allows_larger_leaf(row_mi, col_mi, block_size)
    {
        return Av2MvpPartition::None;
    }
    let base_leaf_size = features.base_leaf_size(row_mi, col_mi, block_size);
    if allowed.none
        && block_size.width <= base_leaf_size
        && block_size.height <= base_leaf_size
    {
        return Av2MvpPartition::None;
    }

    let max_size = if block_size.width <= AV2_SCREEN_ADAPTIVE_LEAF_SIZE
        && block_size.height <= AV2_SCREEN_ADAPTIVE_LEAF_SIZE
    {
        base_leaf_size
    } else {
        AV2_SCREEN_ADAPTIVE_LEAF_SIZE
    };

    if block_size.width == block_size.height {
        if block_size.height > max_size && allowed.horz {
            return Av2MvpPartition::Horz;
        }
        if block_size.width > max_size && allowed.vert {
            return Av2MvpPartition::Vert;
        }
    } else if block_size.width > block_size.height {
        if block_size.width > max_size && allowed.vert {
            return Av2MvpPartition::Vert;
        }
        if block_size.height > max_size && allowed.horz {
            return Av2MvpPartition::Horz;
        }
    } else {
        if block_size.height > max_size && allowed.horz {
            return Av2MvpPartition::Horz;
        }
        if block_size.width > max_size && allowed.vert {
            return Av2MvpPartition::Vert;
        }
    }

    if allowed.none {
        Av2MvpPartition::None
    } else if allowed.horz {
        Av2MvpPartition::Horz
    } else if allowed.vert {
        Av2MvpPartition::Vert
    } else {
        Av2MvpPartition::None
    }
}

fn choose_lossless_inter_partition(
    row_mi: usize,
    col_mi: usize,
    block_size: Av2MvpBlockSize,
    visible_rows_mi: usize,
    visible_cols_mi: usize,
    block_modes: &Av2LosslessInterTileBlockModes,
) -> Av2MvpPartition {
    if !block_size.is_partition_point() {
        return Av2MvpPartition::None;
    }

    let allowed = allowed_partitions(row_mi, col_mi, block_size, visible_rows_mi, visible_cols_mi);
    if let Some(forced) =
        forced_boundary_partition(row_mi, col_mi, block_size, visible_rows_mi, visible_cols_mi)
    {
        if allowed.contains(forced) {
            return forced;
        }
    }
    if let Some(only_allowed) = allowed.only() {
        return only_allowed;
    }
    if allowed.none
        && block_modes
            .homogeneous_skip_mode_for_leaf(row_mi, col_mi, block_size)
            .is_some()
    {
        return Av2MvpPartition::None;
    }

    let mut best = None;
    for partition in [Av2MvpPartition::Horz, Av2MvpPartition::Vert] {
        if !allowed.contains(partition) || block_size.subsize(partition).is_none() {
            continue;
        }
        let score =
            lossless_inter_partition_homogeneous_area(row_mi, col_mi, block_size, partition, block_modes);
        let tie_break = match partition {
            Av2MvpPartition::Horz => usize::from(block_size.height >= block_size.width),
            Av2MvpPartition::Vert => usize::from(block_size.width >= block_size.height),
            Av2MvpPartition::None => 0,
        };
        let candidate_key = (score, tie_break);
        if best.is_none_or(|(best_key, _)| candidate_key > best_key) {
            best = Some((candidate_key, partition));
        }
    }

    best.map(|(_, partition)| partition)
        .unwrap_or_else(|| choose_8x8_leaf_partition(row_mi, col_mi, block_size, visible_rows_mi, visible_cols_mi))
}

fn lossless_inter_partition_homogeneous_area(
    row_mi: usize,
    col_mi: usize,
    block_size: Av2MvpBlockSize,
    partition: Av2MvpPartition,
    block_modes: &Av2LosslessInterTileBlockModes,
) -> usize {
    let Some(subsize) = block_size.subsize(partition) else {
        return 0;
    };
    let mut score = 0usize;
    let children = match partition {
        Av2MvpPartition::Horz => [
            (row_mi, col_mi),
            (row_mi + block_size.mi_height() / 2, col_mi),
        ],
        Av2MvpPartition::Vert => [
            (row_mi, col_mi),
            (row_mi, col_mi + block_size.mi_width() / 2),
        ],
        Av2MvpPartition::None => return 0,
    };
    for (child_row_mi, child_col_mi) in children {
        if block_modes
            .homogeneous_skip_mode_for_leaf(child_row_mi, child_col_mi, subsize)
            .is_some()
        {
            score += subsize.width * subsize.height;
        }
    }
    score
}

fn cached_lossless_subsampled_mode(
    cache: &mut Option<(Av2TileDecision, Av2LosslessSubsampledModeDecision)>,
    lossless: &Av2LosslessSubsampledTileState<'_>,
    decision: Av2TileDecision,
    visible_rows_mi: usize,
    visible_cols_mi: usize,
    coded_mi_context: &Av2CodedMiContext,
    palette: Option<&Av2LumaPalette444>,
) -> Av2LosslessSubsampledModeDecision {
    if let Some((cached_decision, cached_mode)) = cache {
        if *cached_decision == decision {
            return *cached_mode;
        }
    }
    let mode = lossless.mode_decision_for_leaf(
        decision,
        visible_rows_mi,
        visible_cols_mi,
        coded_mi_context,
        palette,
    );
    *cache = Some((decision, mode));
    mode
}

fn cached_lossy_subsampled_mode_with_syntax(
    cache: &mut Option<(usize, usize, Av2MvpBlockSize, Av2LossySubsampledModeDecision)>,
    lossy: &Av2LossySubsampledTileState<'_>,
    decision: Av2TileDecision,
    visible_rows_mi: usize,
    visible_cols_mi: usize,
    coded_mi_context: &Av2CodedMiContext,
    luma_mode_syntax: Av2LumaModeSyntax,
) -> Av2LossySubsampledModeDecision {
    if let Some((row, col, block_size, mode)) = cache {
        if *row == decision.row && *col == decision.col && *block_size == decision.block_size {
            return *mode;
        }
    }
    let mode =
        lossy.mode_decision_for_leaf(
            decision,
            visible_rows_mi,
            visible_cols_mi,
            coded_mi_context,
            luma_mode_syntax,
        );
    *cache = Some((decision.row, decision.col, decision.block_size, mode));
    mode
}

fn cached_lossy_subsampled_mode(
    cache: &mut Option<(usize, usize, Av2MvpBlockSize, Av2LossySubsampledModeDecision)>,
    lossy: &Av2LossySubsampledTileState<'_>,
    decision: Av2TileDecision,
    visible_rows_mi: usize,
    visible_cols_mi: usize,
    coded_mi_context: &Av2CodedMiContext,
    luma_mode_context: &Av2LumaModeContext,
) -> Av2LossySubsampledModeDecision {
    if let Some((row, col, block_size, mode)) = cache {
        if *row == decision.row && *col == decision.col && *block_size == decision.block_size {
            return *mode;
        }
    }
    let luma_mode_syntax =
        luma_mode_context.syntax_for_leaf(decision.row, decision.col, decision.block_size);
    cached_lossy_subsampled_mode_with_syntax(
        cache,
        lossy,
        decision,
        visible_rows_mi,
        visible_cols_mi,
        coded_mi_context,
        luma_mode_syntax,
    )
}

fn av2_lossless_subsampled_tile_entropy_payload_for_region_with_policy(
    region: Av2TileRegion,
    profile: Av2Black444MvpProfile,
    geometry: Av2VideoGeometry,
    chroma_format: Av2ChromaFormat,
    bit_depth: SampleBitDepth,
    source: &[u8],
    recon: &mut [u8],
    palette: Option<&Av2LumaPalette444>,
    partition_policy: Av2PartitionPolicy,
    mode_search: Av2LosslessSubsampledModeSearch,
    ibc: Option<&Av2LocalIbc444>,
    record_fields: bool,
    copy_fast_recon: bool,
    regular_inter_frame: bool,
) -> Av2EntropyPayload {
    let adaptive_partition_features = (partition_policy == Av2PartitionPolicy::AdaptiveScreenContent)
        .then(|| {
            adaptive_partition_features_for_source(
                region,
                geometry,
                bit_depth,
                source,
                palette.is_some(),
                ibc,
            )
        });
    let plan = Av2Black444TilePlan::for_region_with_partition_policy_and_features(
        region,
        profile,
        chroma_format,
        partition_policy,
        false,
        ibc.is_some(),
        ibc,
        None,
        adaptive_partition_features,
        None,
    );
    let mut writer =
        Av2EntropyWriter::with_cdf_updates_and_fields(!profile.disable_cdf_update, record_fields);
    let mut lossless = Av2LosslessSubsampledTileState::new(
        geometry,
        region,
        chroma_format,
        bit_depth,
        mode_search,
        source,
        recon,
    );
    plan.write_lossless_subsampled_entropy(&mut writer, &mut lossless, palette, regular_inter_frame);
    if copy_fast_recon && mode_search == Av2LosslessSubsampledModeSearch::FastScreenContent {
        lossless.copy_source_to_recon_region();
    }
    writer.finish()
}
