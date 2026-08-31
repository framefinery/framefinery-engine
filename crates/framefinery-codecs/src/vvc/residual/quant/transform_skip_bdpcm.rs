const VVC_TRANSFORM_SKIP_MAX_SAMPLES: usize =
    VVC_TRANSFORM_SKIP_MAX_SIZE as usize * VVC_TRANSFORM_SKIP_MAX_SIZE as usize;

#[derive(Debug, Clone, Default)]
struct VvcBdpcmLevelState {
    horizontal: i16,
    vertical: [i16; VVC_TRANSFORM_SKIP_MAX_SIZE as usize],
}

impl VvcBdpcmLevelState {
    #[inline]
    fn begin_row(&mut self) {
        self.horizontal = 0;
    }

    #[inline]
    fn difference(&mut self, mode: VvcBdpcmMode, x: usize, y: usize, level: i16) -> i16 {
        let predictor = self.predictor(mode, x, y);
        self.remember(x, level);
        (i32::from(level) - i32::from(predictor)).clamp(i32::from(i16::MIN), i32::from(i16::MAX))
            as i16
    }

    #[inline]
    fn reconstruct(&mut self, mode: VvcBdpcmMode, x: usize, y: usize, delta: i16) -> i16 {
        let predictor = self.predictor(mode, x, y);
        let level = add_bdpcm_quantized_levels(delta, predictor);
        self.remember(x, level);
        level
    }

    #[inline]
    fn predictor(&self, mode: VvcBdpcmMode, x: usize, y: usize) -> i16 {
        debug_assert!(x < self.vertical.len());
        match mode {
            VvcBdpcmMode::None => unreachable!("BDPCM level state requires a direction"),
            VvcBdpcmMode::Horizontal if x > 0 => self.horizontal,
            VvcBdpcmMode::Vertical if y > 0 => self.vertical[x],
            VvcBdpcmMode::Horizontal | VvcBdpcmMode::Vertical => 0,
        }
    }

    #[inline]
    fn remember(&mut self, x: usize, level: i16) {
        self.horizontal = level;
        self.vertical[x] = level;
    }
}

#[inline]
fn add_bdpcm_quantized_levels(delta: i16, predictor: i16) -> i16 {
    (i32::from(delta) + i32::from(predictor)).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}
