#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Av2VideoGeometry {
    pub width: usize,
    pub height: usize,
}

impl Av2VideoGeometry {
    fn validate_shape(self) -> Result<(), String> {
        if self.width < 8 || self.height < 8 {
            return Err(format!(
                "AV2 geometry expects at least 8x8 visible pictures; got {}x{}",
                self.width, self.height
            ));
        }
        if !self.width.is_multiple_of(8) || !self.height.is_multiple_of(8) {
            return Err(format!(
                "AV2 geometry currently requires dimensions in 8-pixel steps; got {}x{}",
                self.width, self.height
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Av2TileLayout {
    regions: Vec<Av2TileRegion>,
    cols: usize,
    rows: usize,
    log2_cols: u8,
    log2_rows: u8,
    min_log2_cols: u8,
    min_log2_rows: u8,
    max_log2_cols: u8,
    max_log2_rows: u8,
}

impl Av2TileLayout {
    fn for_geometry(geometry: Av2VideoGeometry) -> Self {
        let cols = geometry.width.div_ceil(AV2_MVP_SUPERBLOCK_SIZE);
        let rows = geometry.height.div_ceil(AV2_MVP_SUPERBLOCK_SIZE);
        let mut regions = Vec::with_capacity(cols * rows);
        for tile_row in 0..rows {
            let origin_y = tile_row * AV2_MVP_SUPERBLOCK_SIZE;
            let height = (geometry.height - origin_y).min(AV2_MVP_SUPERBLOCK_SIZE);
            for tile_col in 0..cols {
                let origin_x = tile_col * AV2_MVP_SUPERBLOCK_SIZE;
                let width = (geometry.width - origin_x).min(AV2_MVP_SUPERBLOCK_SIZE);
                regions.push(Av2TileRegion {
                    origin_x,
                    origin_y,
                    width,
                    height,
                });
            }
        }
        let limits = Av2TileLimits::for_geometry(geometry);
        let log2_cols = ceil_log2_usize(cols).max(limits.min_log2_cols);
        let min_log2_rows = limits.min_log2.saturating_sub(log2_cols);
        let log2_rows = ceil_log2_usize(rows).max(min_log2_rows);
        assert!(
            log2_cols <= limits.max_log2_cols,
            "AV2 MVP tile columns exceed the Level 2.0 tile limit"
        );
        assert!(
            log2_rows <= limits.max_log2_rows,
            "AV2 MVP tile rows exceed the Level 2.0 tile limit"
        );
        Self {
            regions,
            cols,
            rows,
            log2_cols,
            log2_rows,
            min_log2_cols: limits.min_log2_cols,
            min_log2_rows,
            max_log2_cols: limits.max_log2_cols,
            max_log2_rows: limits.max_log2_rows,
        }
    }

    fn single_for_geometry(geometry: Av2VideoGeometry) -> Self {
        Self::try_single_for_geometry(geometry)
            .expect("AV2 MVP single-tile layout exceeds the configured tile limits")
    }

    fn try_single_for_geometry(geometry: Av2VideoGeometry) -> Option<Self> {
        let limits = Av2TileLimits::for_geometry(geometry);
        if limits.min_log2_cols != 0 || limits.min_log2 != 0 {
            return None;
        }
        Some(Self {
            regions: vec![Av2TileRegion {
                origin_x: 0,
                origin_y: 0,
                width: geometry.width,
                height: geometry.height,
            }],
            cols: 1,
            rows: 1,
            log2_cols: 0,
            log2_rows: 0,
            min_log2_cols: limits.min_log2_cols,
            min_log2_rows: limits.min_log2,
            max_log2_cols: limits.max_log2_cols,
            max_log2_rows: limits.max_log2_rows,
        })
    }

    fn uniform_for_geometry(geometry: Av2VideoGeometry, log2_cols: u8, log2_rows: u8) -> Self {
        let limits = Av2TileLimits::for_geometry(geometry);
        assert!(log2_cols >= limits.min_log2_cols);
        assert!(log2_cols <= limits.max_log2_cols);
        assert!(log2_rows <= limits.max_log2_rows);
        assert!(log2_cols + log2_rows >= limits.min_log2);
        let mi_cols = align_power_of_two(geometry.width, 3) / AV2_MI_SIZE;
        let mi_rows = align_power_of_two(geometry.height, 3) / AV2_MI_SIZE;
        let col_starts_sb = uniform_tile_starts_sb(mi_cols, log2_cols);
        let row_starts_sb = uniform_tile_starts_sb(mi_rows, log2_rows);
        let cols = col_starts_sb.len() - 1;
        let rows = row_starts_sb.len() - 1;
        let mut regions = Vec::with_capacity(cols * rows);
        for tile_row in 0..rows {
            let origin_sb_y = row_starts_sb[tile_row];
            let end_sb_y = row_starts_sb[tile_row + 1];
            let origin_y = origin_sb_y * AV2_MVP_SUPERBLOCK_SIZE;
            let end_y = (end_sb_y * AV2_MVP_SUPERBLOCK_SIZE).min(geometry.height);
            for tile_col in 0..cols {
                let origin_sb_x = col_starts_sb[tile_col];
                let end_sb_x = col_starts_sb[tile_col + 1];
                let origin_x = origin_sb_x * AV2_MVP_SUPERBLOCK_SIZE;
                let end_x = (end_sb_x * AV2_MVP_SUPERBLOCK_SIZE).min(geometry.width);
                regions.push(Av2TileRegion {
                    origin_x,
                    origin_y,
                    width: end_x - origin_x,
                    height: end_y - origin_y,
                });
            }
        }
        let min_log2_rows = limits.min_log2.saturating_sub(log2_cols);
        Self {
            regions,
            cols,
            rows,
            log2_cols: ceil_log2_usize(cols),
            log2_rows: ceil_log2_usize(rows),
            min_log2_cols: limits.min_log2_cols,
            min_log2_rows,
            max_log2_cols: limits.max_log2_cols,
            max_log2_rows: limits.max_log2_rows,
        }
    }

    fn lossless_subsampled_fast_for_geometry(geometry: Av2VideoGeometry) -> Self {
        let limits = Av2TileLimits::for_geometry(geometry);
        let target_log2_cols = if geometry.width >= 1920 {
            2
        } else if geometry.width >= 1024 {
            1
        } else {
            0
        };
        let log2_cols = target_log2_cols
            .max(limits.min_log2_cols)
            .min(limits.max_log2_cols);
        let target_log2_rows = if geometry.height >= 1080 { 1 } else { 0 };
        let min_log2_rows = limits.min_log2.saturating_sub(log2_cols);
        let log2_rows = target_log2_rows
            .max(min_log2_rows)
            .min(limits.max_log2_rows);
        if log2_cols == 0 && log2_rows == 0 {
            Self::single_for_geometry(geometry)
        } else {
            Self::uniform_for_geometry(geometry, log2_cols, log2_rows)
        }
    }

    fn lossy_subsampled_for_geometry(geometry: Av2VideoGeometry) -> Self {
        Self::lossless_subsampled_fast_for_geometry(geometry)
    }

    fn tile_count(&self) -> usize {
        self.regions.len()
    }

    fn local_ibc_tile_bounds(&self) -> Vec<Av2LocalIbcTileBounds> {
        self.regions
            .iter()
            .map(|region| Av2LocalIbcTileBounds {
                origin_x: region.origin_x,
                origin_y: region.origin_y,
                width: region.width,
                height: region.height,
            })
            .collect()
    }

    fn lossless_subsampled_ibc_for_geometry(geometry: Av2VideoGeometry) -> Self {
        Self::try_single_for_geometry(geometry).unwrap_or_else(|| Self::for_geometry(geometry))
    }

    fn is_single_tile(&self) -> bool {
        self.tile_count() == 1
    }
}

fn av2_tile_layout_for_frame_mode(
    geometry: Av2VideoGeometry,
    frame_mode: &Av2Mvp444FrameMode,
) -> Av2TileLayout {
    match frame_mode {
        Av2Mvp444FrameMode::Black => Av2TileLayout::for_geometry(geometry),
        Av2Mvp444FrameMode::LumaPalette { .. } => Av2TileLayout::single_for_geometry(geometry),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2TileLimits {
    min_log2_cols: u8,
    max_log2_cols: u8,
    max_log2_rows: u8,
    min_log2: u8,
}

impl Av2TileLimits {
    fn for_geometry(geometry: Av2VideoGeometry) -> Self {
        assert!(
            AV2_SEQ_MIB_SIZE_LOG2_64X64 >= AV2_MIB_SIZE_LOG2_64X64
                && AV2_SEQ_MIB_SIZE_LOG2_64X64 - AV2_MIB_SIZE_LOG2_64X64 <= 1,
            "AV2 MVP only supports the AVM tile-limit scale used by 64x64 sequence superblocks"
        );
        let mi_cols = align_power_of_two(geometry.width, 3) / AV2_MI_SIZE;
        let mi_rows = align_power_of_two(geometry.height, 3) / AV2_MI_SIZE;
        let aligned_mi_cols = align_power_of_two(mi_cols, AV2_MIB_SIZE_LOG2_64X64 as usize);
        let aligned_mi_rows = align_power_of_two(mi_rows, AV2_MIB_SIZE_LOG2_64X64 as usize);
        let sb_cols = aligned_mi_cols >> AV2_MIB_SIZE_LOG2_64X64;
        let sb_rows = aligned_mi_rows >> AV2_MIB_SIZE_LOG2_64X64;
        let sb_size_log2 = AV2_MIB_SIZE_LOG2_64X64 + 2;
        let max_width_sb =
            (AV2_TILE_WIDTH_SCALING_LEVEL_2_0_TIER_0 * AV2_MAX_TILE_WIDTH) >> (sb_size_log2 + 2);
        let max_area_sb = (AV2_TILE_AREA_SCALING_LEVEL_2_0_TIER_0 * AV2_MAX_TILE_AREA)
            >> ((2 * sb_size_log2) + 2);
        let min_log2_cols = tile_log2(max_width_sb, sb_cols);
        let max_log2_cols = tile_log2(1, sb_cols.min(AV2_MAX_TILE_COLS));
        let max_log2_rows = tile_log2(1, sb_rows.min(AV2_MAX_TILE_ROWS));
        let min_log2 = tile_log2(max_area_sb, sb_cols * sb_rows).max(min_log2_cols);
        Self {
            min_log2_cols,
            max_log2_cols,
            max_log2_rows,
            min_log2,
        }
    }
}

fn uniform_tile_starts_sb(mi_size: usize, log2_tiles: u8) -> Vec<usize> {
    let aligned_mi = align_power_of_two(mi_size, AV2_MIB_SIZE_LOG2_64X64 as usize);
    let sb_count = aligned_mi >> AV2_MIB_SIZE_LOG2_64X64;
    let seq_mib_size_log2 = AV2_SEQ_MIB_SIZE_LOG2_64X64 as usize;
    let seq_sb_count = align_power_of_two(mi_size, seq_mib_size_log2) >> seq_mib_size_log2;
    let full_sb_count = mi_size >> seq_mib_size_log2;
    let target_tiles = 1usize << log2_tiles;
    let base_size_sb = full_sb_count >> log2_tiles;
    let mut extra_sbs = full_sb_count - (base_size_sb << log2_tiles);
    if base_size_sb == 0 {
        extra_sbs += seq_sb_count - full_sb_count;
    }
    let mut starts = Vec::with_capacity(target_tiles + 1);
    let mut start_sb = 0usize;
    while start_sb < seq_sb_count && starts.len() < target_tiles {
        starts.push(start_sb);
        start_sb += base_size_sb + usize::from(extra_sbs > 0);
        extra_sbs = extra_sbs.saturating_sub(1);
    }
    starts.push(sb_count);
    starts
}

fn align_power_of_two(value: usize, power: usize) -> usize {
    let alignment = 1usize << power;
    (value + alignment - 1) & !(alignment - 1)
}

fn tile_log2(block_size: usize, target: usize) -> u8 {
    assert!(block_size > 0);
    assert!(target > 0);
    let mut log2 = 0u8;
    while (block_size << log2) < target {
        log2 += 1;
    }
    log2
}

fn ceil_log2_usize(value: usize) -> u8 {
    assert!(value > 0);
    let mut bits = 0u8;
    let mut threshold = 1usize;
    while threshold < value {
        threshold <<= 1;
        bits += 1;
    }
    bits
}
