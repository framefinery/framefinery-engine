trait VvcCoeffAccessor {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn level_at(&self, x: usize, y: usize) -> i16;
    fn level_at_scan_position(&self, pos: VvcScanPosition) -> i16 {
        self.level_at(pos.x, pos.y)
    }
}

#[cfg(any(test, feature = "bench-internals"))]
struct VvcRasterCoeffAccessor<'a> {
    coeff_levels: &'a [i16],
    width: usize,
    height: usize,
}

#[cfg(any(test, feature = "bench-internals"))]
impl<'a> VvcRasterCoeffAccessor<'a> {
    fn new(coeff_levels: &'a [i16], width: usize, height: usize) -> Self {
        assert_eq!(coeff_levels.len(), width * height);
        Self {
            coeff_levels,
            width,
            height,
        }
    }
}

#[cfg(any(test, feature = "bench-internals"))]
impl VvcCoeffAccessor for VvcRasterCoeffAccessor<'_> {
    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn level_at(&self, x: usize, y: usize) -> i16 {
        self.coeff_levels[y * self.width + x]
    }

    fn level_at_scan_position(&self, pos: VvcScanPosition) -> i16 {
        match self.width {
            4 => self.coeff_levels[pos.raster_idx_4x4],
            8 => self.coeff_levels[pos.raster_idx_8x8],
            _ => self.level_at(pos.x, pos.y),
        }
    }
}

struct VvcStoredCoeffAccessor<'a, const AC_COEFFS: usize> {
    width: usize,
    height: usize,
    dc_level: i16,
    ac_levels: &'a [i16; AC_COEFFS],
    coeff_stride: usize,
}

impl<'a, const AC_COEFFS: usize> VvcStoredCoeffAccessor<'a, AC_COEFFS> {
    fn new(
        width: usize,
        height: usize,
        dc_level: i16,
        ac_levels: &'a [i16; AC_COEFFS],
        has_ac: bool,
        coeff_stride: usize,
    ) -> Self {
        debug_assert_eq!(has_ac, ac_levels.iter().any(|level| *level != 0));
        debug_assert!(coeff_stride > 0);
        Self {
            width,
            height,
            dc_level,
            ac_levels,
            coeff_stride,
        }
    }
}

impl<const AC_COEFFS: usize> VvcCoeffAccessor for VvcStoredCoeffAccessor<'_, AC_COEFFS> {
    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn level_at(&self, x: usize, y: usize) -> i16 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        if x == 0 && y == 0 {
            self.dc_level
        } else {
            if x >= self.coeff_stride {
                return 0;
            }
            let compact_idx = y * self.coeff_stride + x;
            if compact_idx == 0 || compact_idx > self.ac_levels.len() {
                return 0;
            }
            self.ac_levels[compact_idx - 1]
        }
    }

    fn level_at_scan_position(&self, pos: VvcScanPosition) -> i16 {
        if pos.x >= self.width || pos.y >= self.height {
            return 0;
        }
        let compact_idx = match self.coeff_stride {
            4 => pos.raster_idx_4x4,
            8 => pos.raster_idx_8x8,
            _ => pos.y * self.coeff_stride + pos.x,
        };
        if compact_idx == 0 {
            self.dc_level
        } else if compact_idx > self.ac_levels.len() {
            0
        } else {
            self.ac_levels[compact_idx - 1]
        }
    }
}
