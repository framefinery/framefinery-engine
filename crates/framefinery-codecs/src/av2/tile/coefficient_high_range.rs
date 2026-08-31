// Own the decoded-base gate and rolling context used by both mode scoring and
// entropy emission so the proxy cannot drift from the coded syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Av2CoefficientCodingKind {
    LumaTransform,
    LumaIdtx,
    ChromaTransform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Av2CoefficientHighRangeSymbol {
    value: u32,
    context: u32,
}

impl Av2CoefficientCodingKind {
    fn next_high_range_symbol(
        self,
        level: u32,
        pos: usize,
        level_average: &mut u32,
    ) -> Option<Av2CoefficientHighRangeSymbol> {
        let decoded_base = match self {
            Self::LumaIdtx => 6,
            Self::LumaTransform if luma_lf_limits(pos) => 8,
            Self::LumaTransform => 6,
            Self::ChromaTransform if chroma_lf_limits(pos) => 5,
            Self::ChromaTransform => 6,
        };
        let value = level.checked_sub(decoded_base)?;
        let context = *level_average;
        *level_average = (*level_average + value) >> 1;
        Some(Av2CoefficientHighRangeSymbol { value, context })
    }
}

fn write_luma_high_range(
    writer: &mut Av2EntropyWriter,
    pos: usize,
    level: u32,
    level_average: &mut u32,
) {
    write_coefficient_high_range(
        writer,
        "tile.coeff.y.high_range",
        Av2CoefficientCodingKind::LumaTransform,
        pos,
        level,
        level_average,
    );
}

fn write_idtx_high_range(writer: &mut Av2EntropyWriter, level: u32, level_average: &mut u32) {
    write_coefficient_high_range(
        writer,
        "tile.coeff.y.idtx_high_range",
        Av2CoefficientCodingKind::LumaIdtx,
        0,
        level,
        level_average,
    );
}

fn write_chroma_high_range(
    writer: &mut Av2EntropyWriter,
    plane: Av2ChromaPlane,
    pos: usize,
    level: u32,
    level_average: &mut u32,
) {
    let name = match plane {
        Av2ChromaPlane::U => "tile.coeff.u.high_range",
        Av2ChromaPlane::V => "tile.coeff.v.high_range",
    };
    write_coefficient_high_range(
        writer,
        name,
        Av2CoefficientCodingKind::ChromaTransform,
        pos,
        level,
        level_average,
    );
}

fn write_coefficient_high_range(
    writer: &mut Av2EntropyWriter,
    name: &'static str,
    kind: Av2CoefficientCodingKind,
    pos: usize,
    level: u32,
    level_average: &mut u32,
) {
    let Some(symbol) = kind.next_high_range_symbol(level, pos, level_average) else {
        return;
    };
    write_adaptive_high_range_with_context(writer, name, symbol.value, symbol.context);
}

fn write_dc_only_high_range(
    writer: &mut Av2EntropyWriter,
    name: &'static str,
    kind: Av2CoefficientCodingKind,
    level: u32,
) {
    let mut level_average = 0;
    write_coefficient_high_range(writer, name, kind, 0, level, &mut level_average);
}
