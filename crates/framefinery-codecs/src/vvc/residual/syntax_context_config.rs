#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcResidualCtxConfig {
    pub(in crate::vvc) component: VvcResidualComponent,
    pub(in crate::vvc) log2_zo_tb_width: u8,
    pub(in crate::vvc) log2_zo_tb_height: u8,
    pub(in crate::vvc) q_state: u8,
    pub(in crate::vvc) transform_skip: bool,
    pub(in crate::vvc) ts_residual_coding_disabled: bool,
    pub(in crate::vvc) bdpcm: bool,
    pub(in crate::vvc) mts_index: u8,
    pub(in crate::vvc) last_significant_x: u8,
    pub(in crate::vvc) last_significant_y: u8,
}

impl VvcResidualCtxConfig {
    #[cfg(test)]
    pub(in crate::vvc) fn luma_4x4_subset(last_significant_x: u8, last_significant_y: u8) -> Self {
        Self::luma_subset(2, 2, last_significant_x, last_significant_y)
    }

    #[cfg(test)]
    pub(in crate::vvc) fn luma_subset(
        log2_zo_tb_width: u8,
        log2_zo_tb_height: u8,
        last_significant_x: u8,
        last_significant_y: u8,
    ) -> Self {
        Self::subset(
            VvcResidualComponent::Luma,
            log2_zo_tb_width,
            log2_zo_tb_height,
            last_significant_x,
            last_significant_y,
        )
    }

    pub(in crate::vvc) fn subset(
        component: VvcResidualComponent,
        log2_zo_tb_width: u8,
        log2_zo_tb_height: u8,
        last_significant_x: u8,
        last_significant_y: u8,
    ) -> Self {
        debug_assert!((2..=6).contains(&log2_zo_tb_width));
        debug_assert!((2..=6).contains(&log2_zo_tb_height));
        Self {
            component,
            log2_zo_tb_width,
            log2_zo_tb_height,
            q_state: 0,
            transform_skip: false,
            ts_residual_coding_disabled: true,
            bdpcm: false,
            mts_index: 0,
            last_significant_x,
            last_significant_y,
        }
    }

    fn is_luma(self) -> bool {
        self.component == VvcResidualComponent::Luma
    }

    fn transform_skip_residual_enabled(self) -> bool {
        self.transform_skip && !self.ts_residual_coding_disabled
    }

    fn tb_width(self) -> usize {
        1usize << self.log2_zo_tb_width
    }

    fn tb_height(self) -> usize {
        1usize << self.log2_zo_tb_height
    }

    fn log2_sb_width(self) -> u8 {
        let mut log2_sb_width = if self.log2_zo_tb_width.min(self.log2_zo_tb_height) < 2 {
            1
        } else {
            2
        };
        if self.log2_zo_tb_width < 2 && self.is_luma() {
            log2_sb_width = self.log2_zo_tb_width;
        } else if self.log2_zo_tb_height < 2 && self.is_luma() {
            log2_sb_width = 4 - self.log2_zo_tb_height;
        }
        log2_sb_width
    }

    fn log2_sb_height(self) -> u8 {
        let mut log2_sb_height = if self.log2_zo_tb_width.min(self.log2_zo_tb_height) < 2 {
            1
        } else {
            2
        };
        if self.log2_zo_tb_width < 2 && self.is_luma() {
            log2_sb_height = 4 - self.log2_zo_tb_width;
        } else if self.log2_zo_tb_height < 2 && self.is_luma() {
            log2_sb_height = self.log2_zo_tb_height;
        }
        log2_sb_height
    }

    fn subblocks_wide(self) -> usize {
        1usize << (self.log2_zo_tb_width - self.log2_sb_width())
    }

    fn subblocks_high(self) -> usize {
        1usize << (self.log2_zo_tb_height - self.log2_sb_height())
    }

    fn subblock_count(self) -> usize {
        self.subblocks_wide() * self.subblocks_high()
    }

    fn subblock_index(self, x_s: u8, y_s: u8) -> usize {
        assert!((x_s as usize) < self.subblocks_wide());
        assert!((y_s as usize) < self.subblocks_high());
        y_s as usize * self.subblocks_wide() + x_s as usize
    }
}
