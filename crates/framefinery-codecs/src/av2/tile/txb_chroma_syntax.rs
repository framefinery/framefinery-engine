#[derive(Clone, Copy)]
struct Av2ChromaCoefficientFields {
    base_lf_eob: &'static str,
    base_eob: &'static str,
    base_lf: &'static str,
    base: &'static str,
    low_range: &'static str,
}

trait Av2ChromaTxbSyntax<const SAMPLES: usize>: Sized {
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
    fn level_at(levels: &[u32; SAMPLES], pos: usize, row_delta: usize, col_delta: usize) -> u32;
    fn sign_field(plane: Av2ChromaPlane, dc: bool) -> &'static str;

    fn nz_map_context(
        levels: &[u32; SAMPLES],
        pos: usize,
        scan_index: usize,
        is_eob_coefficient: bool,
        plane: Av2ChromaPlane,
    ) -> usize {
        chroma_nz_map_context::<SAMPLES, Self>(levels, pos, scan_index, is_eob_coefficient, plane)
    }

    fn br_context(levels: &[u32; SAMPLES], pos: usize) -> usize {
        chroma_br_context::<SAMPLES, Self>(levels, pos)
    }

    fn write_skip(
        writer: &mut Av2EntropyWriter,
        plane: Av2ChromaPlane,
        skip_ctx: u8,
        syntax_context: bool,
        all_zero: bool,
    ) {
        match plane {
            Av2ChromaPlane::U => Self::write_u_skip(writer, skip_ctx, syntax_context, all_zero),
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

    fn write_u_skip(writer: &mut Av2EntropyWriter, skip_ctx: u8, use_fsc: bool, all_zero: bool) {
        if all_zero {
            write_u_txb_all_zero(writer, skip_ctx, use_fsc);
        } else {
            write_u_txb_nonzero(writer, skip_ctx, use_fsc);
        }
    }

    fn write_eob(writer: &mut Av2EntropyWriter, eob: usize) {
        write_eob_uv(writer, eob);
    }

    fn level_at(
        levels: &[u32; TX4X4_SAMPLES],
        pos: usize,
        row_delta: usize,
        col_delta: usize,
    ) -> u32 {
        tx4x4_level_at(levels, pos, row_delta, col_delta)
    }

    fn sign_field(plane: Av2ChromaPlane, dc: bool) -> &'static str {
        match (plane, dc) {
            (Av2ChromaPlane::U, true) => "tile.coeff.u.dc_sign_negative",
            (Av2ChromaPlane::V, true) => "tile.coeff.v.dc_sign_negative",
            (Av2ChromaPlane::U, false) => "tile.coeff.u.ac_sign_negative",
            (Av2ChromaPlane::V, false) => "tile.coeff.v.ac_sign_negative",
        }
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

    fn level_at(
        levels: &[u32; TX8X8_SAMPLES],
        pos: usize,
        row_delta: usize,
        col_delta: usize,
    ) -> u32 {
        tx8x8_level_at(levels, pos, row_delta, col_delta)
    }

    fn sign_field(plane: Av2ChromaPlane, dc: bool) -> &'static str {
        match (plane, dc) {
            (Av2ChromaPlane::U, true) => "tile.coeff.u.dc_sign_negative_tx8x8",
            (Av2ChromaPlane::V, true) => "tile.coeff.v.dc_sign_negative_tx8x8",
            (Av2ChromaPlane::U, false) => "tile.coeff.u.ac_sign_negative_tx8x8",
            (Av2ChromaPlane::V, false) => "tile.coeff.v.ac_sign_negative_tx8x8",
        }
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

    fn level_at(
        levels: &[u32; TX4X8_SAMPLES],
        pos: usize,
        row_delta: usize,
        col_delta: usize,
    ) -> u32 {
        tx4x8_level_at(levels, pos, row_delta, col_delta)
    }

    fn sign_field(plane: Av2ChromaPlane, dc: bool) -> &'static str {
        match (plane, dc) {
            (Av2ChromaPlane::U, true) => "tile.coeff.u.dc_sign_negative_tx4x8",
            (Av2ChromaPlane::V, true) => "tile.coeff.v.dc_sign_negative_tx4x8",
            (Av2ChromaPlane::U, false) => "tile.coeff.u.ac_sign_negative_tx4x8",
            (Av2ChromaPlane::V, false) => "tile.coeff.v.ac_sign_negative_tx4x8",
        }
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
    let (levels, bounds) =
        coefficient_levels_and_bounds(coefficients, Syntax::SCAN, Syntax::SCALING_INVARIANT);
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
        let coeff_ctx = Syntax::nz_map_context(&levels, pos, scan_index, is_eob_coefficient, plane);
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
