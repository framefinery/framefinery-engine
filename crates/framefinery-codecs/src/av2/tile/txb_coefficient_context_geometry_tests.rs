type LevelAt<const SAMPLES: usize> = fn(&[u32; SAMPLES], usize, usize, usize) -> u32;

fn assert_level_sampler<const SAMPLES: usize>(
    width: usize,
    height: usize,
    level_at: LevelAt<SAMPLES>,
) {
    assert_eq!(SAMPLES, width * height);
    let levels = core::array::from_fn(|index| ((index * 53 + 11) % 191) as u32);

    for pos in 0..SAMPLES {
        for row_delta in 0..=height {
            for col_delta in 0..=width {
                let row = pos / width + row_delta;
                let col = pos % width + col_delta;
                let expected = if row < height && col < width {
                    levels[row * width + col].min(127)
                } else {
                    0
                };
                assert_eq!(
                    level_at(&levels, pos, row_delta, col_delta),
                    expected,
                    "level sampler mismatch: {width}x{height}, pos={pos}, delta=({row_delta},{col_delta})"
                );
            }
        }
    }
}

#[test]
fn coefficient_level_samplers_preserve_geometry_edges_and_clamping() {
    assert_level_sampler(TX4X4_SIZE, TX4X4_SIZE, tx4x4_level_at);
    assert_level_sampler(TX8X8_SIZE, TX8X8_SIZE, tx8x8_level_at);
    assert_level_sampler(TX4X8_WIDTH, TX4X8_HEIGHT, tx4x8_level_at);
}

fn expected_level_at(
    levels: &[u32],
    width: usize,
    height: usize,
    pos: usize,
    row_delta: usize,
    col_delta: usize,
) -> u32 {
    let row = pos / width + row_delta;
    let col = pos % width + col_delta;
    if row < height && col < width {
        levels[row * width + col].min(127)
    } else {
        0
    }
}

fn expected_chroma_nz_map_context(
    levels: &[u32],
    width: usize,
    height: usize,
    pos: usize,
    scan_index: usize,
    is_eob_coefficient: bool,
    plane: Av2ChromaPlane,
) -> usize {
    if is_eob_coefficient {
        return if scan_index == 0 {
            0
        } else if scan_index <= levels.len() / 8 {
            1
        } else if scan_index <= levels.len() / 4 {
            2
        } else {
            3
        };
    }

    let neighbour_limit = if pos == 0 { 5 } else { 3 };
    let magnitude = expected_level_at(levels, width, height, pos, 0, 1).min(neighbour_limit)
        + expected_level_at(levels, width, height, pos, 1, 0).min(neighbour_limit)
        + expected_level_at(levels, width, height, pos, 1, 1).min(neighbour_limit);
    let context = ((magnitude + 1) >> 1).min(3) as usize;
    context
        + match plane {
            Av2ChromaPlane::U => 0,
            Av2ChromaPlane::V => 4,
        }
}

fn expected_chroma_br_context(levels: &[u32], width: usize, height: usize, pos: usize) -> usize {
    let magnitude = expected_level_at(levels, width, height, pos, 0, 1)
        + expected_level_at(levels, width, height, pos, 1, 0)
        + expected_level_at(levels, width, height, pos, 1, 1);
    ((magnitude + 1) >> 1).min(3) as usize
}

fn assert_chroma_context_geometry<const SAMPLES: usize, Syntax>(width: usize, height: usize)
where
    Syntax: Av2ChromaTxbSyntax<SAMPLES>,
{
    assert_eq!(SAMPLES, width * height);
    let levels = core::array::from_fn(|index| ((index * 71 + 17) % 211) as u32);

    for plane in [Av2ChromaPlane::U, Av2ChromaPlane::V] {
        for pos in 0..SAMPLES {
            assert_eq!(
                Syntax::br_context(&levels, pos),
                expected_chroma_br_context(&levels, width, height, pos),
                "BR context mismatch: {width}x{height}, pos={pos}"
            );
            for scan_index in 0..SAMPLES {
                for is_eob_coefficient in [false, true] {
                    assert_eq!(
                        Syntax::nz_map_context(
                            &levels,
                            pos,
                            scan_index,
                            is_eob_coefficient,
                            plane,
                        ),
                        expected_chroma_nz_map_context(
                            &levels,
                            width,
                            height,
                            pos,
                            scan_index,
                            is_eob_coefficient,
                            plane,
                        ),
                        "NZ-map context mismatch: {width}x{height}, pos={pos}, scan={scan_index}, eob={is_eob_coefficient}, plane={plane:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn chroma_contexts_preserve_all_geometry_and_plane_formulas() {
    assert_chroma_context_geometry::<TX4X4_SAMPLES, Av2ChromaTx4x4Syntax>(TX4X4_SIZE, TX4X4_SIZE);
    assert_chroma_context_geometry::<TX8X8_SAMPLES, Av2ChromaTx8x8Syntax>(TX8X8_SIZE, TX8X8_SIZE);
    assert_chroma_context_geometry::<TX4X8_SAMPLES, Av2ChromaTx4x8Syntax>(
        TX4X8_WIDTH,
        TX4X8_HEIGHT,
    );
}
