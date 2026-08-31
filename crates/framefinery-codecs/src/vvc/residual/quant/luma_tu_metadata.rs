struct VvcLumaTuMetadata {
    luma_tu_intra_modes: [VvcIntraPredictionMode; MAX_VVC_LUMA_TUS],
    luma_tu_remainders: [u8; MAX_VVC_LUMA_TUS],
    luma_tu_negative: [bool; MAX_VVC_LUMA_TUS],
    luma_tu_dc_levels: [i16; MAX_VVC_LUMA_TUS],
    luma_tu_ac_levels: [[i16; VVC_LUMA_AC_COEFFS_PER_TU]; MAX_VVC_LUMA_TUS],
    luma_tu_has_ac: [bool; MAX_VVC_LUMA_TUS],
    luma_tu_scc_decisions: [VvcLumaSccDecision; MAX_VVC_LUMA_TUS],
    luma_tu_transform_skip: [bool; MAX_VVC_LUMA_TUS],
    luma_tu_bdpcm_modes: [VvcBdpcmMode; MAX_VVC_LUMA_TUS],
    luma_tu_mrl_index: [u8; MAX_VVC_LUMA_TUS],
    luma_tu_mts_index: [u8; MAX_VVC_LUMA_TUS],
}

impl VvcLumaTuMetadata {
    fn new() -> Self {
        Self {
            luma_tu_intra_modes: [VvcIntraPredictionMode::Dc; MAX_VVC_LUMA_TUS],
            luma_tu_remainders: [0; MAX_VVC_LUMA_TUS],
            luma_tu_negative: [false; MAX_VVC_LUMA_TUS],
            luma_tu_dc_levels: [0; MAX_VVC_LUMA_TUS],
            luma_tu_ac_levels: [[0; VVC_LUMA_AC_COEFFS_PER_TU]; MAX_VVC_LUMA_TUS],
            luma_tu_has_ac: [false; MAX_VVC_LUMA_TUS],
            luma_tu_scc_decisions: [VvcLumaSccDecision::RegularIntra; MAX_VVC_LUMA_TUS],
            luma_tu_transform_skip: [false; MAX_VVC_LUMA_TUS],
            luma_tu_bdpcm_modes: [VvcBdpcmMode::None; MAX_VVC_LUMA_TUS],
            luma_tu_mrl_index: [0; MAX_VVC_LUMA_TUS],
            luma_tu_mts_index: [0; MAX_VVC_LUMA_TUS],
        }
    }

    fn record_scc_decision(&mut self, index: usize, decision: VvcLumaSccDecision) {
        self.assert_index(index);
        self.luma_tu_scc_decisions[index] = decision;
    }

    fn record_finalized(
        &mut self,
        index: usize,
        mode: VvcIntraPredictionMode,
        tu: VvcFinalizedLumaTu,
    ) {
        self.assert_index(index);
        self.luma_tu_intra_modes[index] = mode;
        self.luma_tu_remainders[index] = tu.abs_remainder;
        self.luma_tu_negative[index] = tu.negative;
        self.luma_tu_dc_levels[index] = tu.dc_level;
        self.luma_tu_ac_levels[index] = tu.ac_levels;
        self.luma_tu_has_ac[index] = tu.has_ac;
        self.luma_tu_transform_skip[index] = tu.transform_skip;
        self.luma_tu_bdpcm_modes[index] = tu.bdpcm_mode;
        self.luma_tu_mrl_index[index] = tu.mrl_index;
        self.luma_tu_mts_index[index] = tu.mts_index;
    }

    fn scc_decision(&self, index: usize) -> Option<VvcLumaSccDecision> {
        self.luma_tu_scc_decisions.get(index).copied()
    }

    fn assert_index(&self, index: usize) {
        assert!(index < MAX_VVC_LUMA_TUS, "luma TU index out of range");
    }
}
