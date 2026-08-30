// Current production residual TUs are at most 8x8 luma and 4x4 chroma. Raise
// this with the transform-size selector when larger emitted TUs are enabled.
const VVC_RESIDUAL_CONTEXT_COEFFS: usize = 64;
const VVC_MAX_RESIDUAL_SUBBLOCKS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::vvc) struct VvcResidualPass1State {
    pub(in crate::vvc) config: VvcResidualCtxConfig,
    pub(in crate::vvc) sig_coeff: [bool; VVC_RESIDUAL_CONTEXT_COEFFS],
    pub(in crate::vvc) abs_level_pass1: [u8; VVC_RESIDUAL_CONTEXT_COEFFS],
    rice_abs_level: [u16; VVC_RESIDUAL_CONTEXT_COEFFS],
    pub(in crate::vvc) sb_coded: [bool; VVC_MAX_RESIDUAL_SUBBLOCKS],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::vvc) struct VvcResidualLocalStats {
    pub(in crate::vvc) loc_num_sig: u8,
    pub(in crate::vvc) loc_sum_abs_pass1: u8,
}

impl VvcResidualPass1State {
    pub(in crate::vvc) fn new(config: VvcResidualCtxConfig) -> Self {
        debug_assert!(config.subblock_count() <= VVC_MAX_RESIDUAL_SUBBLOCKS);
        Self {
            config,
            sig_coeff: [false; VVC_RESIDUAL_CONTEXT_COEFFS],
            abs_level_pass1: [0; VVC_RESIDUAL_CONTEXT_COEFFS],
            rice_abs_level: [0; VVC_RESIDUAL_CONTEXT_COEFFS],
            sb_coded: [false; VVC_MAX_RESIDUAL_SUBBLOCKS],
        }
    }

    pub(in crate::vvc) fn set_pass1_coeff(
        &mut self,
        x: u8,
        y: u8,
        abs_level: u16,
        _negative: bool,
    ) {
        let index = self
            .coefficient_index(x, y)
            .expect("VVC residual pass-1 coefficient is outside the tracked transform block");
        self.sig_coeff[index] = abs_level != 0;
        // VTM CoeffCodingContext::sigCtxIdAbs uses
        // min(4 + (absLevel & 1), absLevel) for the local template sum and
        // then reuses sumAbs - numPos for the par/gt context offset.
        // Keep that exact template magnitude here instead of an artificial
        // pass-1 clip so AC contexts track H.266 9.3.4.2.8/9.
        let pass1_level = template_abs_sum_level(abs_level);
        self.abs_level_pass1[index] = pass1_level;
        self.rice_abs_level[index] = u16::from(pass1_level);
    }

    fn set_rice_abs_level(&mut self, x: u8, y: u8, abs_level: u16) {
        let index = self
            .coefficient_index(x, y)
            .expect("VVC residual rice coefficient is outside the tracked transform block");
        self.rice_abs_level[index] = abs_level;
    }

    pub(in crate::vvc) fn set_sb_coded(&mut self, x_s: u8, y_s: u8, coded: bool) {
        let index = self.config.subblock_index(x_s, y_s);
        debug_assert!(index < VVC_MAX_RESIDUAL_SUBBLOCKS);
        if index < VVC_MAX_RESIDUAL_SUBBLOCKS {
            self.sb_coded[index] = coded;
        }
    }

    pub(in crate::vvc) fn sb_coded_flag_ctx_inc(&self, x_s: u8, y_s: u8) -> u8 {
        // VVC 9.3.4.2.6. Keep transform-skip and regular residual paths
        // separate because future screen-content tools will use both.
        let mut csbf_ctx = 0;
        if self.config.transform_skip_residual_enabled() {
            if x_s > 0 && self.sb_coded_at(x_s - 1, y_s) {
                csbf_ctx += 1;
            }
            if y_s > 0 && self.sb_coded_at(x_s, y_s - 1) {
                csbf_ctx += 1;
            }
            4 + csbf_ctx
        } else {
            if (x_s as usize) + 1 < self.config.subblocks_wide() && self.sb_coded_at(x_s + 1, y_s) {
                csbf_ctx += 1;
            }
            if (y_s as usize) + 1 < self.config.subblocks_high() && self.sb_coded_at(x_s, y_s + 1) {
                csbf_ctx += 1;
            }
            if self.config.is_luma() {
                csbf_ctx.min(1)
            } else {
                2 + csbf_ctx.min(1)
            }
        }
    }

    pub(in crate::vvc) fn sig_coeff_flag_ctx_inc(&self, x: u8, y: u8) -> u8 {
        // VVC 9.3.4.2.8. QState is kept explicit even though the current
        // subset initializes it to zero for the simple residual path.
        let stats = self.local_stats(x, y);
        if self.config.transform_skip_residual_enabled() {
            60 + stats.loc_num_sig
        } else {
            let d = x + y;
            let sum_bucket = ((stats.loc_sum_abs_pass1 + 1) >> 1).min(3);
            let q_bucket = 12 * self.config.q_state.saturating_sub(1);
            if self.config.is_luma() {
                q_bucket
                    + sum_bucket
                    + if d < 2 {
                        8
                    } else if d < 5 {
                        4
                    } else {
                        0
                    }
            } else {
                36 + (8 * self.config.q_state.saturating_sub(1))
                    + sum_bucket
                    + if d < 2 { 4 } else { 0 }
            }
        }
    }

    pub(in crate::vvc) fn par_level_flag_ctx_inc(&self, x: u8, y: u8) -> u8 {
        self.par_or_abs_level_ctx_inc(x, y, false, 0)
    }

    pub(in crate::vvc) fn abs_level_gtx_flag_ctx_inc(&self, x: u8, y: u8, gtx_idx: u8) -> u8 {
        self.par_or_abs_level_ctx_inc(x, y, true, gtx_idx)
    }

    pub(in crate::vvc) fn local_stats(&self, x: u8, y: u8) -> VvcResidualLocalStats {
        // VVC 9.3.4.2.7. The regular transform path looks forward in raster
        // coordinates because coefficients are scanned in reverse order.
        let mut loc_num_sig = 0;
        let mut loc_sum_abs_pass1 = 0;
        if self.config.transform_skip_residual_enabled() {
            if x > 0 {
                self.accumulate_local(x - 1, y, &mut loc_num_sig, &mut loc_sum_abs_pass1);
            }
            if y > 0 {
                self.accumulate_local(x, y - 1, &mut loc_num_sig, &mut loc_sum_abs_pass1);
            }
        } else {
            if (x as usize) + 1 < self.config.tb_width() {
                self.accumulate_local(x + 1, y, &mut loc_num_sig, &mut loc_sum_abs_pass1);
                if (x as usize) + 2 < self.config.tb_width() {
                    self.accumulate_local(x + 2, y, &mut loc_num_sig, &mut loc_sum_abs_pass1);
                }
                if (y as usize) + 1 < self.config.tb_height() {
                    self.accumulate_local(x + 1, y + 1, &mut loc_num_sig, &mut loc_sum_abs_pass1);
                }
            }
            if (y as usize) + 1 < self.config.tb_height() {
                self.accumulate_local(x, y + 1, &mut loc_num_sig, &mut loc_sum_abs_pass1);
                if (y as usize) + 2 < self.config.tb_height() {
                    self.accumulate_local(x, y + 2, &mut loc_num_sig, &mut loc_sum_abs_pass1);
                }
            }
        }
        VvcResidualLocalStats {
            loc_num_sig,
            loc_sum_abs_pass1,
        }
    }

    fn par_or_abs_level_ctx_inc(&self, x: u8, y: u8, abs_level_gtx: bool, gtx_idx: u8) -> u8 {
        // VVC 9.3.4.2.9. Only abs_level_gtx_flag[n][0] is wired to the cached
        // context table today; gtx_idx > 0 is labelled here for the upcoming
        // larger residual-level implementation.
        if self.config.transform_skip_residual_enabled() {
            if !abs_level_gtx {
                return 32;
            }
            if gtx_idx > 0 {
                return 67 + gtx_idx;
            }
            if self.config.bdpcm {
                return 67;
            }
            return 64
                + if x > 0 && self.sig_coeff_at(x - 1, y) {
                    1
                } else {
                    0
                }
                + if y > 0 && self.sig_coeff_at(x, y - 1) {
                    1
                } else {
                    0
                };
        }

        let base = if x == self.config.last_significant_x && y == self.config.last_significant_y {
            if self.config.is_luma() {
                0
            } else {
                21
            }
        } else {
            let stats = self.local_stats(x, y);
            let ctx_offset = stats
                .loc_sum_abs_pass1
                .saturating_sub(stats.loc_num_sig)
                .min(4);
            let d = x + y;
            if self.config.is_luma() {
                1 + ctx_offset
                    + if d == 0 {
                        15
                    } else if d < 3 {
                        10
                    } else if d < 10 {
                        5
                    } else {
                        0
                    }
            } else {
                22 + ctx_offset + if d == 0 { 5 } else { 0 }
            }
        };
        base + if abs_level_gtx && gtx_idx == 1 { 32 } else { 0 }
    }

    fn accumulate_local(&self, x: u8, y: u8, loc_num_sig: &mut u8, loc_sum_abs_pass1: &mut u8) {
        if self.sig_coeff_at(x, y) {
            *loc_num_sig += 1;
        }
        *loc_sum_abs_pass1 = loc_sum_abs_pass1.saturating_add(self.abs_level_pass1_at(x, y));
    }

    pub(in crate::vvc) fn sig_coeff_at(&self, x: u8, y: u8) -> bool {
        self.coefficient_index(x, y)
            .is_some_and(|index| self.sig_coeff[index])
    }

    pub(in crate::vvc) fn abs_level_pass1_at(&self, x: u8, y: u8) -> u8 {
        self.coefficient_index(x, y)
            .map_or(0, |index| self.abs_level_pass1[index])
    }

    fn rice_abs_level_at(&self, x: u8, y: u8) -> u16 {
        self.coefficient_index(x, y)
            .map_or(0, |index| self.rice_abs_level[index])
    }

    pub(in crate::vvc) fn sb_coded_at(&self, x_s: u8, y_s: u8) -> bool {
        let index = self.config.subblock_index(x_s, y_s);
        index < VVC_MAX_RESIDUAL_SUBBLOCKS && self.sb_coded[index]
    }

    fn coefficient_index(&self, x: u8, y: u8) -> Option<usize> {
        let x = usize::from(x);
        let y = usize::from(y);
        if x >= self.config.tb_width() || y >= self.config.tb_height() {
            return None;
        }
        let index = y * self.config.tb_width() + x;
        (index < VVC_RESIDUAL_CONTEXT_COEFFS).then_some(index)
    }
}
