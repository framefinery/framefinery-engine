#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VvcDelayedResidualCabacSymbol {
    AbsRemainder {
        x: u8,
        y: u8,
        value: u32,
        rice_param: u8,
    },
    BypassAbsLevel {
        x: u8,
        y: u8,
        value: u32,
        rice_param: u8,
    },
}

trait VvcResidualSymbolSink {
    fn last_sig_coeff_prefix(
        &mut self,
        state: &VvcResidualPass1State,
        x_prefix: bool,
        log2_tb_size: u8,
        bin_idx: u8,
        bin: bool,
    );
    fn last_sig_coeff_suffix(&mut self, x_prefix: bool, bits: u32, count: u8);
    fn sb_coded_flag(
        &mut self,
        state: &VvcResidualPass1State,
        x_s: u8,
        y_s: u8,
        coded: bool,
    );
    fn sig_coeff_flag(
        &mut self,
        state: &VvcResidualPass1State,
        x: u8,
        y: u8,
        significant: bool,
    );
    fn par_level_flag(
        &mut self,
        state: &VvcResidualPass1State,
        x: u8,
        y: u8,
        par_level: bool,
    );
    fn abs_level_gtx_flag(
        &mut self,
        state: &VvcResidualPass1State,
        x: u8,
        y: u8,
        gtx_idx: u8,
        greater_than: bool,
    );
    fn abs_remainder(&mut self, x: u8, y: u8, value: u32, rice_param: u8);
    fn bypass_abs_level(&mut self, x: u8, y: u8, value: u32, rice_param: u8);
    fn coeff_sign_pattern(&mut self, bits: u32, count: u8);
}

struct VvcDirectResidualSymbolSink<'encoder, 'contexts, 'cabac> {
    encoder: &'encoder mut VvcResidualCabacEncoder<'contexts>,
    cabac: &'cabac mut VvcCabacEncoder,
}

impl<'encoder, 'contexts, 'cabac> VvcDirectResidualSymbolSink<'encoder, 'contexts, 'cabac> {
    fn new(
        encoder: &'encoder mut VvcResidualCabacEncoder<'contexts>,
        cabac: &'cabac mut VvcCabacEncoder,
    ) -> Self {
        Self { encoder, cabac }
    }
}

impl VvcResidualSymbolSink for VvcDirectResidualSymbolSink<'_, '_, '_> {
    fn last_sig_coeff_prefix(
        &mut self,
        state: &VvcResidualPass1State,
        x_prefix: bool,
        log2_tb_size: u8,
        bin_idx: u8,
        bin: bool,
    ) {
        self.encoder.emit_last_sig_coeff_prefix_bin(
            self.cabac,
            state.config.component,
            x_prefix,
            log2_tb_size,
            bin_idx,
            bin,
        );
    }

    fn last_sig_coeff_suffix(&mut self, _x_prefix: bool, bits: u32, count: u8) {
        self.cabac.encode_bins_ep(bits, u32::from(count));
    }

    fn sb_coded_flag(
        &mut self,
        state: &VvcResidualPass1State,
        x_s: u8,
        y_s: u8,
        coded: bool,
    ) {
        self.encoder
            .emit_sb_coded_flag(self.cabac, state, x_s, y_s, coded);
    }

    fn sig_coeff_flag(
        &mut self,
        state: &VvcResidualPass1State,
        x: u8,
        y: u8,
        significant: bool,
    ) {
        self.encoder
            .emit_sig_coeff_flag(self.cabac, state, x, y, significant);
    }

    fn par_level_flag(
        &mut self,
        state: &VvcResidualPass1State,
        x: u8,
        y: u8,
        par_level: bool,
    ) {
        self.encoder
            .emit_par_level_flag(self.cabac, state, x, y, par_level);
    }

    fn abs_level_gtx_flag(
        &mut self,
        state: &VvcResidualPass1State,
        x: u8,
        y: u8,
        gtx_idx: u8,
        greater_than: bool,
    ) {
        self.encoder.emit_abs_level_gtx_flag(
            self.cabac,
            state,
            x,
            y,
            gtx_idx,
            greater_than,
        );
    }

    fn abs_remainder(&mut self, _x: u8, _y: u8, value: u32, rice_param: u8) {
        self.cabac
            .encode_rem_abs_ep(value, u32::from(rice_param));
    }

    fn bypass_abs_level(&mut self, _x: u8, _y: u8, value: u32, rice_param: u8) {
        self.cabac
            .encode_rem_abs_ep(value, u32::from(rice_param));
    }

    fn coeff_sign_pattern(&mut self, bits: u32, count: u8) {
        self.cabac.encode_bins_ep(bits, u32::from(count));
    }
}

#[cfg(test)]
impl VvcResidualSymbolSink for Vec<VvcResidualCabacSymbol> {
    fn last_sig_coeff_prefix(
        &mut self,
        _state: &VvcResidualPass1State,
        x_prefix: bool,
        _log2_tb_size: u8,
        bin_idx: u8,
        bin: bool,
    ) {
        self.push(if x_prefix {
            VvcResidualCabacSymbol::LastSigCoeffXPrefix { bin_idx, bin }
        } else {
            VvcResidualCabacSymbol::LastSigCoeffYPrefix { bin_idx, bin }
        });
    }

    fn last_sig_coeff_suffix(&mut self, x_prefix: bool, bits: u32, count: u8) {
        self.push(if x_prefix {
            VvcResidualCabacSymbol::LastSigCoeffXSuffix { bits, count }
        } else {
            VvcResidualCabacSymbol::LastSigCoeffYSuffix { bits, count }
        });
    }

    fn sb_coded_flag(
        &mut self,
        _state: &VvcResidualPass1State,
        x_s: u8,
        y_s: u8,
        coded: bool,
    ) {
        self.push(VvcResidualCabacSymbol::SbCodedFlag { x_s, y_s, coded });
    }

    fn sig_coeff_flag(
        &mut self,
        _state: &VvcResidualPass1State,
        x: u8,
        y: u8,
        significant: bool,
    ) {
        self.push(VvcResidualCabacSymbol::SigCoeffFlag { x, y, significant });
    }

    fn par_level_flag(
        &mut self,
        _state: &VvcResidualPass1State,
        x: u8,
        y: u8,
        par_level: bool,
    ) {
        self.push(VvcResidualCabacSymbol::ParLevelFlag { x, y, par_level });
    }

    fn abs_level_gtx_flag(
        &mut self,
        _state: &VvcResidualPass1State,
        x: u8,
        y: u8,
        gtx_idx: u8,
        greater_than: bool,
    ) {
        self.push(VvcResidualCabacSymbol::AbsLevelGtxFlag {
            x,
            y,
            gtx_idx,
            greater_than,
        });
    }

    fn abs_remainder(&mut self, x: u8, y: u8, value: u32, rice_param: u8) {
        self.push(VvcResidualCabacSymbol::AbsRemainder {
            x,
            y,
            value,
            rice_param,
        });
    }

    fn bypass_abs_level(&mut self, x: u8, y: u8, value: u32, rice_param: u8) {
        self.push(VvcResidualCabacSymbol::BypassAbsLevel {
            x,
            y,
            value,
            rice_param,
        });
    }

    fn coeff_sign_pattern(&mut self, bits: u32, count: u8) {
        self.push(VvcResidualCabacSymbol::CoeffSignPattern { bits, count });
    }
}
