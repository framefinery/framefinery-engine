struct VvcChromaTuMetadata {
    chroma_tu_intra_modes: [VvcChromaIntraPredictionMode; MAX_VVC_CHROMA_TUS],
    cb_tu_dc_levels: [i16; MAX_VVC_CHROMA_TUS],
    cr_tu_dc_levels: [i16; MAX_VVC_CHROMA_TUS],
    cb_tu_ac_levels: [[i16; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS],
    cr_tu_ac_levels: [[i16; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS],
    cb_tu_has_ac: [bool; MAX_VVC_CHROMA_TUS],
    cr_tu_has_ac: [bool; MAX_VVC_CHROMA_TUS],
    cb_tu_transform_skip: [bool; MAX_VVC_CHROMA_TUS],
    cr_tu_transform_skip: [bool; MAX_VVC_CHROMA_TUS],
    chroma_tu_bdpcm_modes: [VvcBdpcmMode; MAX_VVC_CHROMA_TUS],
}

impl VvcChromaTuMetadata {
    fn new() -> Self {
        Self {
            chroma_tu_intra_modes: [VvcChromaIntraPredictionMode::Derived; MAX_VVC_CHROMA_TUS],
            cb_tu_dc_levels: [0; MAX_VVC_CHROMA_TUS],
            cr_tu_dc_levels: [0; MAX_VVC_CHROMA_TUS],
            cb_tu_ac_levels: [[0; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS],
            cr_tu_ac_levels: [[0; VVC_CHROMA_AC_COEFFS_PER_TU]; MAX_VVC_CHROMA_TUS],
            cb_tu_has_ac: [false; MAX_VVC_CHROMA_TUS],
            cr_tu_has_ac: [false; MAX_VVC_CHROMA_TUS],
            cb_tu_transform_skip: [false; MAX_VVC_CHROMA_TUS],
            cr_tu_transform_skip: [false; MAX_VVC_CHROMA_TUS],
            chroma_tu_bdpcm_modes: [VvcBdpcmMode::None; MAX_VVC_CHROMA_TUS],
        }
    }

    fn record_mode_hint(
        &mut self,
        index: usize,
        mode: VvcChromaIntraPredictionMode,
        bdpcm_mode: VvcBdpcmMode,
    ) {
        self.assert_index(index);
        self.chroma_tu_intra_modes[index] = mode;
        self.chroma_tu_bdpcm_modes[index] = bdpcm_mode;
    }

    fn record_finalized(
        &mut self,
        index: usize,
        mode: VvcChromaIntraPredictionMode,
        tu: VvcFinalizedChromaTu,
    ) {
        self.assert_index(index);
        self.chroma_tu_intra_modes[index] = mode;
        self.cb_tu_dc_levels[index] = tu.cb_dc_level;
        self.cr_tu_dc_levels[index] = tu.cr_dc_level;
        self.cb_tu_ac_levels[index] = tu.cb_ac_levels;
        self.cr_tu_ac_levels[index] = tu.cr_ac_levels;
        self.cb_tu_has_ac[index] = tu.cb_has_ac;
        self.cr_tu_has_ac[index] = tu.cr_has_ac;
        self.cb_tu_transform_skip[index] = tu.cb_transform_skip;
        self.cr_tu_transform_skip[index] = tu.cr_transform_skip;
        self.chroma_tu_bdpcm_modes[index] = tu.bdpcm_mode;
    }

    fn assert_index(&self, index: usize) {
        assert!(index < MAX_VVC_CHROMA_TUS, "chroma TU index out of range");
    }
}
