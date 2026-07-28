const VVC_PALETTE_CU_SIZE: u16 = 8;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VvcPaletteTreeType {
    SingleTree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VvcPalette444Syntax {
    pub(super) tree_type: VvcPaletteTreeType,
    pub(super) bit_depth: SampleBitDepth,
    pub(super) slice_qp: i32,
    pub(super) cb_width: usize,
    pub(super) cb_height: usize,
    pub(super) start_comp: u8,
    pub(super) num_comps: u8,
    pub(super) max_num_palette_entries: u8,
    pub(super) num_predicted_palette_entries: u8,
    pub(super) num_signalled_palette_entries: u8,
    pub(super) new_palette_entries: Vec<VvcSampledColor>,
    pub(super) current_palette_size: u8,
    pub(super) palette_escape_val_present_flag: bool,
    pub(super) max_palette_index: u8,
    pub(super) palette_indices: Vec<u8>,
    /// Coded PaletteEscapeVal levels from H.266 7.4.12.6. Palette escape
    /// reconstruction is QP-dependent, so the syntax records SliceQpY with the
    /// CU. Lossless palette slices choose a bit-depth-adjusted QP that makes
    /// H.266 8.4.5.3 reconstruct native samples exactly.
    ///
    /// TODO(area): the RTL currently mirrors this as full-CU escape banks.
    /// Keep this semantic model simple, but use it as the reference for a
    /// later subset-streamed RTL path that feeds escape values directly to
    /// CABAC without storing every escaped component twice.
    pub(super) palette_escape_values: Vec<Option<VvcSampledColor>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VvcPaletteSyntaxTokenKind {
    Eg0 { value: u32 },
    FixedLength { value: u32, bit_count: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VvcPaletteSyntaxToken {
    pub(super) name: &'static str,
    kind: VvcPaletteSyntaxTokenKind,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct VvcPalette444DecodedPicture {
    pub(super) luma: Vec<VvcSample>,
    pub(super) cb: Vec<VvcSample>,
    pub(super) cr: Vec<VvcSample>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcPalette444TileEntry {
    x: usize,
    y: usize,
    color: VvcSampledColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VvcPalettePredictorMode {
    SignalNewEntry,
    SignalNewEntryAfterPredictor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcPaletteCuEmitRequest {
    origin_x: u16,
    origin_y: u16,
    write_split_flag: bool,
    split_ctx: u8,
    predictor_mode: VvcPalettePredictorMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VvcTransformSkipResidual444Cu {
    decision: VvcIbcCuDecision,
    y_coeffs: Vec<i16>,
    cb_coeffs: Vec<i16>,
    cr_coeffs: Vec<i16>,
    cbf_y: bool,
    cbf_cb: bool,
    cbf_cr: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VvcBdpcm444Cu {
    y_coeffs: Vec<i16>,
    cb_coeffs: Vec<i16>,
    cr_coeffs: Vec<i16>,
    cbf_y: bool,
    cbf_cb: bool,
    cbf_cr: bool,
}
