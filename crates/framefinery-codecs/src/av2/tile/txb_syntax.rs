#[derive(Clone, Copy)]
enum Av2LumaTxbSyntaxPolicy {
    Intra,
    Inter,
}

impl Av2LumaTxbSyntaxPolicy {
    fn write_skip(self, writer: &mut Av2EntropyWriter, skip_ctx: u8, all_zero: bool) {
        match (self, all_zero) {
            (Self::Intra, true) => write_y_txb_all_zero(writer, skip_ctx),
            (Self::Intra, false) => write_y_txb_nonzero(writer, skip_ctx),
            (Self::Inter, true) => write_y_inter_txb_all_zero(writer, skip_ctx),
            (Self::Inter, false) => write_y_inter_txb_nonzero(writer, skip_ctx),
        }
    }

    fn write_eob_and_tx_type(self, writer: &mut Av2EntropyWriter, eob: usize) {
        match self {
            Self::Intra => write_eob_y(writer, eob),
            Self::Inter => {
                write_eob_y_inter(writer, eob);
                write_regular_inter_dct_dct_tx_type(writer, eob);
            }
        }
    }
}

fn write_luma_palette_residual_txb(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    dc_sign_ctx: u8,
    coefficients: &[i32; TX4X4_SAMPLES],
) -> (u8, bool) {
    write_luma_regular_residual_txb(
        writer,
        skip_ctx,
        dc_sign_ctx,
        coefficients,
        Av2LumaTxbSyntaxPolicy::Intra,
    )
}

fn write_luma_inter_residual_txb(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    dc_sign_ctx: u8,
    coefficients: &[i32; TX4X4_SAMPLES],
) -> (u8, bool) {
    write_luma_regular_residual_txb(
        writer,
        skip_ctx,
        dc_sign_ctx,
        coefficients,
        Av2LumaTxbSyntaxPolicy::Inter,
    )
}

fn write_luma_regular_residual_txb(
    writer: &mut Av2EntropyWriter,
    skip_ctx: u8,
    dc_sign_ctx: u8,
    coefficients: &[i32; TX4X4_SAMPLES],
    syntax: Av2LumaTxbSyntaxPolicy,
) -> (u8, bool) {
    let (levels, bounds) = lossless_coefficient_levels_and_bounds(coefficients);
    let Some((_, eob)) = bounds else {
        syntax.write_skip(writer, skip_ctx, true);
        return (0, false);
    };

    syntax.write_skip(writer, skip_ctx, false);
    syntax.write_eob_and_tx_type(writer, eob);

    for scan_index in (1..eob).rev() {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        let coeff_ctx = luma_nz_map_context(&levels, pos, scan_index, scan_index + 1 == eob);
        write_luma_coefficient_level(
            writer,
            &levels,
            pos,
            scan_index + 1 == eob,
            coeff_ctx,
            level,
        );
    }

    let dc_level = levels[0];
    let dc_ctx = luma_nz_map_context(&levels, 0, 0, eob == 1);
    write_luma_coefficient_level(writer, &levels, 0, eob == 1, dc_ctx, dc_level);

    let mut cul_level = 0u32;
    let mut dc_val = 0i32;
    let mut hr_level_avg = 0u32;
    for scan_index in (0..eob).rev() {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        if level == 0 {
            continue;
        }
        let negative = coefficients[pos] < 0;
        if scan_index == 0 {
            write_y_dc_sign(writer, negative, dc_sign_ctx);
            dc_val = if negative {
                -(level as i32)
            } else {
                level as i32
            };
        } else {
            writer.write_literal_bit("tile.coeff.y.ac_sign_negative", negative);
        }
        write_luma_high_range(writer, pos, level, &mut hr_level_avg);
        cul_level += level;
    }

    (lossless_entropy_context(cul_level, dc_val), true)
}

fn write_luma_palette_fsc_txb(
    writer: &mut Av2EntropyWriter,
    coefficients: &[i32; TX4X4_SAMPLES],
) -> (u8, bool) {
    let (levels, bounds) = lossless_coefficient_levels_and_bounds(coefficients);
    let Some((bob, _)) = bounds else {
        write_y_fsc_txb_all_zero(writer);
        return (0, false);
    };

    write_y_fsc_txb_nonzero(writer);
    write_eob_y(writer, TX4X4_SAMPLES - bob);

    for scan_index in bob..TX4X4_SAMPLES {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        if scan_index == bob {
            let coeff_ctx = idtx_bob_context(scan_index);
            let mut cdf = DEFAULT_COEFF_BASE_BOB_IDTX_CDFS[coeff_ctx];
            writer.write_symbol(
                "tile.coeff.y.idtx_base_bob",
                level.min(3) as usize - 1,
                &mut cdf,
                3,
                false,
            );
        } else {
            let coeff_ctx = idtx_upper_levels_context(&levels, pos);
            let mut cdf = DEFAULT_COEFF_BASE_IDTX_CDFS[coeff_ctx];
            writer.write_symbol(
                "tile.coeff.y.idtx_base",
                level.min(3) as usize,
                &mut cdf,
                4,
                false,
            );
        }
        if level > 2 {
            write_idtx_low_range(writer, &levels, pos, level);
        }
    }

    let mut cul_level = 0u32;
    let mut dc_val = 0i32;
    let mut hr_level_avg = 0u32;
    for scan_index in 0..TX4X4_SAMPLES {
        let pos = TX4X4_SCAN[scan_index];
        let level = levels[pos];
        if level == 0 {
            continue;
        }
        let negative = coefficients[pos] < 0;
        let sign_ctx = idtx_sign_context(&levels, coefficients, pos);
        let mut cdf = DEFAULT_IDTX_SIGN_CDFS[sign_ctx];
        writer.write_symbol(
            "tile.coeff.y.idtx_sign_negative",
            usize::from(negative),
            &mut cdf,
            2,
            false,
        );
        write_idtx_high_range(writer, level, &mut hr_level_avg);
        if scan_index == 0 {
            dc_val = if negative {
                -(level as i32)
            } else {
                level as i32
            };
        }
        cul_level += level;
    }

    (lossless_entropy_context(cul_level, dc_val), true)
}

#[derive(Clone, Copy)]
struct Av2ChromaCoefficientFields {
    base_lf_eob: &'static str,
    base_eob: &'static str,
    base_lf: &'static str,
    base: &'static str,
    low_range: &'static str,
}

trait Av2ChromaTxbSyntax<const SAMPLES: usize> {
    const SCAN: &'static [usize; SAMPLES];
    const SCALING_INVARIANT: &'static str;
    const COEFFICIENT_FIELDS: Av2ChromaCoefficientFields;

    fn write_u_skip(
        writer: &mut Av2EntropyWriter,
        skip_ctx: u8,
        syntax_context: bool,
        all_zero: bool,
    );
    fn write_eob(writer: &mut Av2EntropyWriter, eob: usize);
    fn nz_map_context(
        levels: &[u32; SAMPLES],
        pos: usize,
        scan_index: usize,
        is_eob_coefficient: bool,
        plane: Av2ChromaPlane,
    ) -> usize;
    fn sign_field(plane: Av2ChromaPlane, dc: bool) -> &'static str;
    fn br_context(levels: &[u32; SAMPLES], pos: usize) -> usize;

    fn write_skip(
        writer: &mut Av2EntropyWriter,
        plane: Av2ChromaPlane,
        skip_ctx: u8,
        syntax_context: bool,
        all_zero: bool,
    ) {
        match plane {
            Av2ChromaPlane::U => {
                Self::write_u_skip(writer, skip_ctx, syntax_context, all_zero)
            }
            Av2ChromaPlane::V if all_zero => write_v_txb_all_zero(writer, skip_ctx),
            Av2ChromaPlane::V => write_v_txb_nonzero(writer, skip_ctx),
        }
    }
}

struct Av2ChromaTx4x4Syntax;
struct Av2ChromaTx8x8Syntax;
struct Av2ChromaTx4x8Syntax;

impl Av2ChromaTxbSyntax<TX4X4_SAMPLES> for Av2ChromaTx4x4Syntax {
    const SCAN: &'static [usize; TX4X4_SAMPLES] = &TX4X4_SCAN;
    const SCALING_INVARIANT: &'static str =
        "AV2 lossless WHT coefficient must be divisible by UNIT_QUANT_FACTOR";
    const COEFFICIENT_FIELDS: Av2ChromaCoefficientFields = Av2ChromaCoefficientFields {
        base_lf_eob: "tile.coeff.uv.base_lf_eob",
        base_eob: "tile.coeff.uv.base_eob",
        base_lf: "tile.coeff.uv.base_lf",
        base: "tile.coeff.uv.base",
        low_range: "tile.coeff.uv.low_range",
    };

    fn write_u_skip(
        writer: &mut Av2EntropyWriter,
        skip_ctx: u8,
        use_fsc: bool,
        all_zero: bool,
    ) {
        if all_zero {
            write_u_txb_all_zero(writer, skip_ctx, use_fsc);
        } else {
            write_u_txb_nonzero(writer, skip_ctx, use_fsc);
        }
    }

    fn write_eob(writer: &mut Av2EntropyWriter, eob: usize) {
        write_eob_uv(writer, eob);
    }

    fn nz_map_context(
        levels: &[u32; TX4X4_SAMPLES],
        pos: usize,
        scan_index: usize,
        is_eob_coefficient: bool,
        plane: Av2ChromaPlane,
    ) -> usize {
        chroma_nz_map_context(levels, pos, scan_index, is_eob_coefficient, plane)
    }

    fn sign_field(plane: Av2ChromaPlane, dc: bool) -> &'static str {
        match (plane, dc) {
            (Av2ChromaPlane::U, true) => "tile.coeff.u.dc_sign_negative",
            (Av2ChromaPlane::V, true) => "tile.coeff.v.dc_sign_negative",
            (Av2ChromaPlane::U, false) => "tile.coeff.u.ac_sign_negative",
            (Av2ChromaPlane::V, false) => "tile.coeff.v.ac_sign_negative",
        }
    }

    fn br_context(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> usize {
        chroma_br_context(levels, pos)
    }
}

impl Av2ChromaTxbSyntax<TX8X8_SAMPLES> for Av2ChromaTx8x8Syntax {
    const SCAN: &'static [usize; TX8X8_SAMPLES] = &TX8X8_SCAN;
    const SCALING_INVARIANT: &'static str =
        "AV2 quantized DCT coefficient must be scaled by UNIT_QUANT_FACTOR";
    const COEFFICIENT_FIELDS: Av2ChromaCoefficientFields = Av2ChromaCoefficientFields {
        base_lf_eob: "tile.coeff.uv.base_lf_eob_tx8x8",
        base_eob: "tile.coeff.uv.base_eob_tx8x8",
        base_lf: "tile.coeff.uv.base_lf_tx8x8",
        base: "tile.coeff.uv.base_tx8x8",
        low_range: "tile.coeff.uv.low_range_tx8x8",
    };

    fn write_u_skip(
        writer: &mut Av2EntropyWriter,
        skip_ctx: u8,
        use_inter_contexts: bool,
        all_zero: bool,
    ) {
        if all_zero {
            write_u_txb_all_zero_tx8x8(writer, skip_ctx, use_inter_contexts);
        } else {
            write_u_txb_nonzero_tx8x8(writer, skip_ctx, use_inter_contexts);
        }
    }

    fn write_eob(writer: &mut Av2EntropyWriter, eob: usize) {
        write_eob_uv_tx8x8(writer, eob);
    }

    fn nz_map_context(
        levels: &[u32; TX8X8_SAMPLES],
        pos: usize,
        scan_index: usize,
        is_eob_coefficient: bool,
        plane: Av2ChromaPlane,
    ) -> usize {
        chroma_tx8x8_nz_map_context(levels, pos, scan_index, is_eob_coefficient, plane)
    }

    fn sign_field(plane: Av2ChromaPlane, dc: bool) -> &'static str {
        match (plane, dc) {
            (Av2ChromaPlane::U, true) => "tile.coeff.u.dc_sign_negative_tx8x8",
            (Av2ChromaPlane::V, true) => "tile.coeff.v.dc_sign_negative_tx8x8",
            (Av2ChromaPlane::U, false) => "tile.coeff.u.ac_sign_negative_tx8x8",
            (Av2ChromaPlane::V, false) => "tile.coeff.v.ac_sign_negative_tx8x8",
        }
    }

    fn br_context(levels: &[u32; TX8X8_SAMPLES], pos: usize) -> usize {
        chroma_tx8x8_br_context(levels, pos)
    }
}

impl Av2ChromaTxbSyntax<TX4X8_SAMPLES> for Av2ChromaTx4x8Syntax {
    const SCAN: &'static [usize; TX4X8_SAMPLES] = &TX4X8_SCAN;
    const SCALING_INVARIANT: &'static str =
        "AV2 quantized DCT coefficient must be scaled by UNIT_QUANT_FACTOR";
    const COEFFICIENT_FIELDS: Av2ChromaCoefficientFields = Av2ChromaCoefficientFields {
        base_lf_eob: "tile.coeff.uv.base_lf_eob_tx4x8",
        base_eob: "tile.coeff.uv.base_eob_tx4x8",
        base_lf: "tile.coeff.uv.base_lf_tx4x8",
        base: "tile.coeff.uv.base_tx4x8",
        low_range: "tile.coeff.uv.low_range_tx4x8",
    };

    fn write_u_skip(
        writer: &mut Av2EntropyWriter,
        skip_ctx: u8,
        use_inter_contexts: bool,
        all_zero: bool,
    ) {
        if all_zero {
            write_u_txb_all_zero_tx8x8(writer, skip_ctx, use_inter_contexts);
        } else {
            write_u_txb_nonzero_tx8x8(writer, skip_ctx, use_inter_contexts);
        }
    }

    fn write_eob(writer: &mut Av2EntropyWriter, eob: usize) {
        write_eob_uv_tx4x8(writer, eob);
    }

    fn nz_map_context(
        levels: &[u32; TX4X8_SAMPLES],
        pos: usize,
        scan_index: usize,
        is_eob_coefficient: bool,
        plane: Av2ChromaPlane,
    ) -> usize {
        chroma_tx4x8_nz_map_context(levels, pos, scan_index, is_eob_coefficient, plane)
    }

    fn sign_field(plane: Av2ChromaPlane, dc: bool) -> &'static str {
        match (plane, dc) {
            (Av2ChromaPlane::U, true) => "tile.coeff.u.dc_sign_negative_tx4x8",
            (Av2ChromaPlane::V, true) => "tile.coeff.v.dc_sign_negative_tx4x8",
            (Av2ChromaPlane::U, false) => "tile.coeff.u.ac_sign_negative_tx4x8",
            (Av2ChromaPlane::V, false) => "tile.coeff.v.ac_sign_negative_tx4x8",
        }
    }

    fn br_context(levels: &[u32; TX4X8_SAMPLES], pos: usize) -> usize {
        chroma_tx4x8_br_context(levels, pos)
    }
}

fn write_chroma_signs_and_high_range<const SAMPLES: usize, Syntax>(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    coefficients: &[i32; SAMPLES],
    levels: &[u32; SAMPLES],
    scan: &[usize; SAMPLES],
    eob: usize,
) -> (u32, i32)
where
    Syntax: Av2ChromaTxbSyntax<SAMPLES>,
{
    let mut cul_level = 0u32;
    let mut dc_val = 0i32;
    let mut hr_level_avg = 0u32;
    for scan_index in (0..eob).rev() {
        let pos = scan[scan_index];
        let level = levels[pos];
        if level == 0 {
            continue;
        }
        let negative = coefficients[pos] < 0;
        writer.write_literal_bit(Syntax::sign_field(plane, scan_index == 0), negative);
        write_chroma_high_range(writer, plane, pos, level, &mut hr_level_avg);
        if scan_index == 0 {
            dc_val = if negative {
                -(level as i32)
            } else {
                level as i32
            };
        }
        cul_level += level;
    }
    (cul_level, dc_val)
}

fn write_chroma_bdpcm_txb(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    skip_ctx: u8,
    coefficients: &[i32; TX4X4_SAMPLES],
    use_fsc: bool,
) -> (u8, bool) {
    write_chroma_txb::<TX4X4_SAMPLES, Av2ChromaTx4x4Syntax>(
        writer,
        plane,
        skip_ctx,
        coefficients,
        use_fsc,
    )
}

fn write_chroma_tx8x8_txb(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    skip_ctx: u8,
    coefficients: &[i32; TX8X8_SAMPLES],
    use_inter_contexts: bool,
) -> (u8, bool) {
    write_chroma_txb::<TX8X8_SAMPLES, Av2ChromaTx8x8Syntax>(
        writer,
        plane,
        skip_ctx,
        coefficients,
        use_inter_contexts,
    )
}

fn write_chroma_tx4x8_txb(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    skip_ctx: u8,
    coefficients: &[i32; TX4X8_SAMPLES],
    use_inter_contexts: bool,
) -> (u8, bool) {
    write_chroma_txb::<TX4X8_SAMPLES, Av2ChromaTx4x8Syntax>(
        writer,
        plane,
        skip_ctx,
        coefficients,
        use_inter_contexts,
    )
}

fn write_chroma_txb<const SAMPLES: usize, Syntax>(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    skip_ctx: u8,
    coefficients: &[i32; SAMPLES],
    syntax_context: bool,
) -> (u8, bool)
where
    Syntax: Av2ChromaTxbSyntax<SAMPLES>,
{
    let (levels, bounds) = coefficient_levels_and_bounds(
        coefficients,
        Syntax::SCAN,
        Syntax::SCALING_INVARIANT,
    );
    let Some((_, eob)) = bounds else {
        Syntax::write_skip(writer, plane, skip_ctx, syntax_context, true);
        return (0, false);
    };

    Syntax::write_skip(writer, plane, skip_ctx, syntax_context, false);
    Syntax::write_eob(writer, eob);

    for scan_index in (1..eob).rev() {
        let pos = Syntax::SCAN[scan_index];
        let level = levels[pos];
        let is_eob_coefficient = scan_index + 1 == eob;
        let coeff_ctx = Syntax::nz_map_context(
            &levels,
            pos,
            scan_index,
            is_eob_coefficient,
            plane,
        );
        write_chroma_coefficient_level::<SAMPLES, Syntax>(
            writer,
            &levels,
            pos,
            is_eob_coefficient,
            coeff_ctx,
            level,
        );
    }

    let dc_level = levels[0];
    let dc_ctx = Syntax::nz_map_context(&levels, 0, 0, eob == 1, plane);
    write_chroma_coefficient_level::<SAMPLES, Syntax>(
        writer,
        &levels,
        0,
        eob == 1,
        dc_ctx,
        dc_level,
    );

    let (cul_level, dc_val) = write_chroma_signs_and_high_range::<SAMPLES, Syntax>(
        writer,
        plane,
        coefficients,
        &levels,
        Syntax::SCAN,
        eob,
    );

    (lossless_entropy_context(cul_level, dc_val), true)
}

fn coefficient_levels_and_bounds<const SAMPLES: usize>(
    coefficients: &[i32; SAMPLES],
    scan: &[usize; SAMPLES],
    scaling_invariant: &'static str,
) -> ([u32; SAMPLES], Option<(usize, usize)>) {
    let mut levels = [0u32; SAMPLES];
    let mut first = None;
    let mut eob = 0usize;
    for (scan_index, &index) in scan.iter().enumerate() {
        let coefficient = coefficients[index];
        debug_assert_eq!(coefficient % 8, 0, "{scaling_invariant}");
        let level = coefficient.unsigned_abs() / 8;
        levels[index] = level;
        if level != 0 {
            first.get_or_insert(scan_index);
            eob = scan_index + 1;
        }
    }
    (levels, first.map(|first| (first, eob)))
}

fn lossless_coefficient_levels_and_bounds(
    coefficients: &[i32; TX4X4_SAMPLES],
) -> ([u32; TX4X4_SAMPLES], Option<(usize, usize)>) {
    coefficient_levels_and_bounds(
        coefficients,
        &TX4X4_SCAN,
        "AV2 lossless WHT coefficient must be divisible by UNIT_QUANT_FACTOR",
    )
}

#[derive(Clone, Copy)]
struct Av2EobSyntax {
    point_name: &'static str,
    point_cdf_key: usize,
    point_symbols: usize,
    extra_name: &'static str,
}

const EOB_Y_TX4X4_SYNTAX: Av2EobSyntax = Av2EobSyntax {
    point_name: "tile.coeff.y.eob_pt_tx4x4",
    point_cdf_key: AV2_STATIC_CDF_EOB_Y,
    point_symbols: 5,
    extra_name: "tile.coeff.y.eob_extra",
};
const EOB_Y_INTER_TX4X4_SYNTAX: Av2EobSyntax = Av2EobSyntax {
    point_name: "tile.coeff.y.inter_eob_pt_tx4x4",
    point_cdf_key: AV2_STATIC_CDF_EOB_Y_INTER,
    point_symbols: 5,
    extra_name: "tile.coeff.y.inter_eob_extra",
};
const EOB_UV_TX4X4_SYNTAX: Av2EobSyntax = Av2EobSyntax {
    point_name: "tile.coeff.uv.eob_pt_tx4x4",
    point_cdf_key: AV2_STATIC_CDF_EOB_UV,
    point_symbols: 5,
    extra_name: "tile.coeff.uv.eob_extra",
};
const EOB_UV_TX8X8_SYNTAX: Av2EobSyntax = Av2EobSyntax {
    point_name: "tile.coeff.uv.eob_pt_tx8x8",
    point_cdf_key: AV2_STATIC_CDF_EOB_UV_TX8X8,
    point_symbols: 7,
    extra_name: "tile.coeff.uv.eob_extra_tx8x8",
};
const EOB_UV_TX4X8_SYNTAX: Av2EobSyntax = Av2EobSyntax {
    point_name: "tile.coeff.uv.eob_pt_tx4x8",
    point_cdf_key: AV2_STATIC_CDF_EOB_UV_TX4X8,
    point_symbols: 6,
    extra_name: "tile.coeff.uv.eob_extra_tx4x8",
};

fn write_eob<const CDF_SIZE: usize>(
    writer: &mut Av2EntropyWriter,
    eob: usize,
    mut point_cdf: [u16; CDF_SIZE],
    syntax: Av2EobSyntax,
) {
    let (eob_pt, eob_extra) = eob_pos_token(eob);
    debug_assert_eq!(point_cdf.len(), syntax.point_symbols + 4);
    debug_assert!((1..=syntax.point_symbols).contains(&eob_pt));
    writer.write_symbol_with_static_cdf_key(
        syntax.point_name,
        syntax.point_cdf_key,
        eob_pt - 1,
        &mut point_cdf,
        syntax.point_symbols,
        false,
    );

    let eob_offset_bits = eob_offset_bits(eob_pt);
    if eob_offset_bits > 0 {
        let eob_shift = eob_offset_bits - 1;
        let bit = (eob_extra & (1 << eob_shift)) != 0;
        let mut extra_cdf = DEFAULT_EOB_EXTRA_CDF;
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.eob_extra_bit",
            AV2_STATIC_CDF_EOB_EXTRA,
            usize::from(bit),
            &mut extra_cdf,
            2,
            false,
        );
        let low_bits = eob_extra & ((1 << eob_shift) - 1);
        writer.write_literal(syntax.extra_name, low_bits as u32, eob_shift as u8);
    }
}

fn write_eob_y(writer: &mut Av2EntropyWriter, eob: usize) {
    write_eob(
        writer,
        eob,
        DEFAULT_EOB_MULTI16_Y_CTX0_CDF,
        EOB_Y_TX4X4_SYNTAX,
    );
}

fn write_eob_y_inter(writer: &mut Av2EntropyWriter, eob: usize) {
    write_eob(
        writer,
        eob,
        DEFAULT_EOB_MULTI16_Y_INTER_CTX0_CDF,
        EOB_Y_INTER_TX4X4_SYNTAX,
    );
}

fn regular_inter_tx_type_eob_ctx_4x4(eob: usize) -> usize {
    debug_assert!((1..=TX4X4_SAMPLES).contains(&eob));
    // AVM get_lp2tx_ctx() derives the transform-type context from eob - 1 as
    // a raster last-position value, not from the coefficient scan position.
    let last = eob - 1;
    let diag = last % TX4X4_SIZE + last / TX4X4_SIZE;
    if diag < 2 {
        1
    } else if diag > 4 {
        2
    } else {
        0
    }
}

fn write_regular_inter_dct_dct_tx_type(writer: &mut Av2EntropyWriter, eob: usize) {
    let eob_ctx = regular_inter_tx_type_eob_ctx_4x4(eob);
    let mut cdf = DEFAULT_INTER_EXT_TX_DCT_IDTX_4X4_CDF;
    // With reduced_tx_set_used=2, inter 4x4 uses EXT_TX_SET_DCT_IDTX:
    // symbol 1 maps to DCT_DCT and symbol 0 maps to IDTX.
    writer.write_symbol_with_static_cdf_key(
        "tile.tx_type.inter_reduced_dct_dct",
        AV2_STATIC_CDF_INTER_EXT_TX_DCT_IDTX_4X4_BASE + eob_ctx,
        1,
        &mut cdf,
        2,
        false,
    );
}

fn write_eob_uv(writer: &mut Av2EntropyWriter, eob: usize) {
    write_eob(
        writer,
        eob,
        DEFAULT_EOB_MULTI16_UV_CTX2_CDF,
        EOB_UV_TX4X4_SYNTAX,
    );
}

fn write_eob_uv_tx8x8(writer: &mut Av2EntropyWriter, eob: usize) {
    write_eob(
        writer,
        eob,
        DEFAULT_EOB_MULTI64_UV_CTX2_CDF,
        EOB_UV_TX8X8_SYNTAX,
    );
}

fn write_eob_uv_tx4x8(writer: &mut Av2EntropyWriter, eob: usize) {
    write_eob(
        writer,
        eob,
        DEFAULT_EOB_MULTI32_UV_CTX2_CDF,
        EOB_UV_TX4X8_SYNTAX,
    );
}

fn eob_pos_token(eob: usize) -> (usize, usize) {
    const EOB_TO_POS_SMALL: [usize; 33] = [
        0, 1, 2, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
        6, 6, 6,
    ];
    const EOB_GROUP_START: [usize; 12] = [0, 1, 2, 3, 5, 9, 17, 33, 65, 129, 257, 513];
    assert!((1..=TX8X8_SAMPLES).contains(&eob));
    let token = if eob < EOB_TO_POS_SMALL.len() {
        EOB_TO_POS_SMALL[eob]
    } else {
        7
    };
    (token, eob - EOB_GROUP_START[token])
}

fn eob_offset_bits(eob_pt: usize) -> usize {
    const EOB_OFFSET_BITS: [usize; 12] = [0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    EOB_OFFSET_BITS[eob_pt]
}

fn write_chroma_coefficient_level<const SAMPLES: usize, Syntax>(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; SAMPLES],
    pos: usize,
    is_eob_coefficient: bool,
    coeff_ctx: usize,
    level: u32,
)
where
    Syntax: Av2ChromaTxbSyntax<SAMPLES>,
{
    let limits = chroma_lf_limits(pos);
    let fields = Syntax::COEFFICIENT_FIELDS;
    if is_eob_coefficient {
        assert!(level > 0, "AV2 EOB coefficient must be non-zero");
        if limits {
            let mut cdf = DEFAULT_COEFF_BASE_LF_EOB_UV_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                fields.base_lf_eob,
                AV2_STATIC_CDF_COEFF_UV_BASE_LF_EOB_BASE + coeff_ctx,
                level.min(5) as usize - 1,
                &mut cdf,
                5,
                false,
            );
        } else {
            let mut cdf = DEFAULT_COEFF_BASE_EOB_UV_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                fields.base_eob,
                AV2_STATIC_CDF_COEFF_UV_BASE_EOB_BASE + coeff_ctx,
                level.min(3) as usize - 1,
                &mut cdf,
                3,
                false,
            );
            if level > 2 {
                write_chroma_low_range::<SAMPLES, Syntax>(
                    writer,
                    levels,
                    pos,
                    level - 3,
                    fields.low_range,
                );
            }
        }
    } else if limits {
        let mut cdf = DEFAULT_COEFF_BASE_LF_UV_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            fields.base_lf,
            AV2_STATIC_CDF_COEFF_UV_BASE_LF_BASE + coeff_ctx,
            level.min(5) as usize,
            &mut cdf,
            6,
            false,
        );
    } else {
        let mut cdf = DEFAULT_COEFF_BASE_UV_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            fields.base,
            AV2_STATIC_CDF_COEFF_UV_BASE_BASE + coeff_ctx,
            level.min(3) as usize,
            &mut cdf,
            4,
            false,
        );
        if level > 2 {
            write_chroma_low_range::<SAMPLES, Syntax>(
                writer,
                levels,
                pos,
                level - 3,
                fields.low_range,
            );
        }
    }
}

fn write_luma_coefficient_level(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    is_eob_coefficient: bool,
    coeff_ctx: usize,
    level: u32,
) {
    let limits = luma_lf_limits(pos);
    if is_eob_coefficient {
        assert!(level > 0, "AV2 EOB coefficient must be non-zero");
        if limits {
            let mut cdf = DEFAULT_COEFF_BASE_LF_EOB_Y_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                "tile.coeff.y.base_lf_eob",
                AV2_STATIC_CDF_COEFF_Y_BASE_LF_EOB_BASE + coeff_ctx,
                level.min(5) as usize - 1,
                &mut cdf,
                5,
                false,
            );
            if level > 4 {
                write_luma_low_range(writer, levels, pos, true, level - 5);
            }
        } else {
            let mut cdf = DEFAULT_COEFF_BASE_EOB_Y_CDFS[coeff_ctx];
            writer.write_symbol_with_static_cdf_key(
                "tile.coeff.y.base_eob",
                AV2_STATIC_CDF_COEFF_Y_BASE_EOB_BASE + coeff_ctx,
                level.min(3) as usize - 1,
                &mut cdf,
                3,
                false,
            );
            if level > 2 {
                write_luma_low_range(writer, levels, pos, false, level - 3);
            }
        }
    } else if limits {
        let mut cdf = DEFAULT_COEFF_BASE_LF_Y_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.y.base_lf",
            AV2_STATIC_CDF_COEFF_Y_BASE_LF_BASE + coeff_ctx,
            level.min(5) as usize,
            &mut cdf,
            6,
            false,
        );
        if level > 4 {
            write_luma_low_range(writer, levels, pos, true, level - 5);
        }
    } else {
        let mut cdf = DEFAULT_COEFF_BASE_Y_CDFS[coeff_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.y.base",
            AV2_STATIC_CDF_COEFF_Y_BASE_BASE + coeff_ctx,
            level.min(3) as usize,
            &mut cdf,
            4,
            false,
        );
        if level > 2 {
            write_luma_low_range(writer, levels, pos, false, level - 3);
        }
    }
}

fn write_luma_low_range(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    lf: bool,
    base_range: u32,
) {
    if lf {
        let br_ctx = luma_br_lf_context(levels, pos);
        let mut cdf = DEFAULT_COEFF_BR_LF_Y_CDFS[br_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.y.low_range_lf",
            AV2_STATIC_CDF_COEFF_Y_BR_LF_BASE + br_ctx,
            base_range.min(3) as usize,
            &mut cdf,
            4,
            false,
        );
    } else {
        let br_ctx = luma_br_context(levels, pos);
        let mut cdf = DEFAULT_COEFF_BR_Y_CDFS[br_ctx];
        writer.write_symbol_with_static_cdf_key(
            "tile.coeff.y.low_range",
            AV2_STATIC_CDF_COEFF_Y_BR_BASE + br_ctx,
            base_range.min(3) as usize,
            &mut cdf,
            4,
            false,
        );
    }
}

fn write_chroma_low_range<const SAMPLES: usize, Syntax>(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; SAMPLES],
    pos: usize,
    base_range: u32,
    field_name: &'static str,
)
where
    Syntax: Av2ChromaTxbSyntax<SAMPLES>,
{
    let br_ctx = Syntax::br_context(levels, pos);
    let mut cdf = DEFAULT_COEFF_BR_UV_CDFS[br_ctx];
    writer.write_symbol_with_static_cdf_key(
        field_name,
        AV2_STATIC_CDF_COEFF_UV_BR_BASE + br_ctx,
        base_range.min(3) as usize,
        &mut cdf,
        4,
        false,
    );
}

fn write_idtx_low_range(
    writer: &mut Av2EntropyWriter,
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    level: u32,
) {
    let br_ctx = idtx_br_context(levels, pos);
    let mut cdf = DEFAULT_COEFF_BR_IDTX_CDFS[br_ctx];
    writer.write_symbol(
        "tile.coeff.y.idtx_low_range",
        (level - 3).min(3) as usize,
        &mut cdf,
        4,
        false,
    );
}

fn write_luma_high_range(
    writer: &mut Av2EntropyWriter,
    pos: usize,
    level: u32,
    hr_level_avg: &mut u32,
) {
    let limits = luma_lf_limits(pos);
    let threshold = if limits { 7 } else { 5 };
    if level <= threshold {
        return;
    }
    let decoded_base = threshold + 1;
    let high_range = level.saturating_sub(decoded_base);
    write_adaptive_high_range_with_context(
        writer,
        "tile.coeff.y.high_range",
        high_range,
        *hr_level_avg,
    );
    *hr_level_avg = (*hr_level_avg + high_range) >> 1;
}

fn write_idtx_high_range(writer: &mut Av2EntropyWriter, level: u32, hr_level_avg: &mut u32) {
    if level <= 5 {
        return;
    }
    let high_range = level - 6;
    write_adaptive_high_range_with_context(
        writer,
        "tile.coeff.y.idtx_high_range",
        high_range,
        *hr_level_avg,
    );
    *hr_level_avg = (*hr_level_avg + high_range) >> 1;
}

fn write_chroma_high_range(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    pos: usize,
    level: u32,
    hr_level_avg: &mut u32,
) {
    let limits = chroma_lf_limits(pos);
    let threshold = if limits { 4 } else { 5 };
    if level <= threshold {
        return;
    }
    let decoded_base = if limits { 5 } else { 6 };
    let high_range = level.saturating_sub(decoded_base);
    let name = match plane {
        Av2ChromaPlane::U => "tile.coeff.u.high_range",
        Av2ChromaPlane::V => "tile.coeff.v.high_range",
    };
    write_adaptive_high_range_with_context(writer, name, high_range, *hr_level_avg);
    *hr_level_avg = (*hr_level_avg + high_range) >> 1;
}

include!("txb_coefficient_contexts.rs");

include!("txb_syntax_writers.rs");
