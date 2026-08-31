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
        return expected_eob_context(scan_index, levels.len());
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

fn expected_eob_context(scan_index: usize, samples: usize) -> usize {
    if scan_index == 0 {
        0
    } else if scan_index <= samples / 8 {
        1
    } else if scan_index <= samples / 4 {
        2
    } else {
        3
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

fn expected_luma_lower_levels_context(
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    low_frequency: bool,
) -> usize {
    if !low_frequency && pos == 0 {
        return 0;
    }

    let neighbour_limit = if low_frequency { 5 } else { 3 };
    let magnitude = expected_level_at(levels, TX4X4_SIZE, TX4X4_SIZE, pos, 0, 1)
        .min(neighbour_limit)
        + expected_level_at(levels, TX4X4_SIZE, TX4X4_SIZE, pos, 1, 0).min(neighbour_limit)
        + expected_level_at(levels, TX4X4_SIZE, TX4X4_SIZE, pos, 1, 1).min(neighbour_limit)
        + expected_level_at(levels, TX4X4_SIZE, TX4X4_SIZE, pos, 0, 2).min(neighbour_limit)
        + expected_level_at(levels, TX4X4_SIZE, TX4X4_SIZE, pos, 2, 0).min(neighbour_limit);
    let row_col_sum = pos / TX4X4_SIZE + pos % TX4X4_SIZE;
    let context = (magnitude + 1) >> 1;

    if low_frequency {
        if pos == 0 {
            context.min(8) as usize
        } else if row_col_sum < 2 {
            context.min(6) as usize + 9
        } else {
            context.min(4) as usize + 16
        }
    } else {
        let context = context.min(4) as usize;
        if row_col_sum < 6 {
            context
        } else if row_col_sum < 8 {
            context + 5
        } else {
            context + 10
        }
    }
}

fn expected_luma_nz_map_context(
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    scan_index: usize,
    is_eob_coefficient: bool,
) -> usize {
    if is_eob_coefficient {
        return expected_eob_context(scan_index, TX4X4_SAMPLES);
    }
    let low_frequency = pos / TX4X4_SIZE + pos % TX4X4_SIZE < 4;
    expected_luma_lower_levels_context(levels, pos, low_frequency)
}

fn expected_luma_br_context(
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    low_frequency: bool,
) -> usize {
    let magnitude = expected_level_at(levels, TX4X4_SIZE, TX4X4_SIZE, pos, 0, 1).min(5)
        + expected_level_at(levels, TX4X4_SIZE, TX4X4_SIZE, pos, 1, 0).min(5)
        + expected_level_at(levels, TX4X4_SIZE, TX4X4_SIZE, pos, 1, 1).min(5);
    let context = ((magnitude + 1) >> 1).min(6) as usize;
    if low_frequency && pos != 0 {
        context + 7
    } else {
        context
    }
}

fn assert_luma_contexts_for_levels(levels: &[u32; TX4X4_SAMPLES]) {
    for pos in 0..TX4X4_SAMPLES {
        assert_eq!(
            luma_lower_levels_context(levels, pos, true),
            expected_luma_lower_levels_context(levels, pos, true),
            "LF lower-level context mismatch: pos={pos}, levels={levels:?}"
        );
        assert_eq!(
            luma_lower_levels_context(levels, pos, false),
            expected_luma_lower_levels_context(levels, pos, false),
            "non-LF lower-level context mismatch: pos={pos}, levels={levels:?}"
        );
        assert_eq!(
            luma_br_context(levels, pos, true),
            expected_luma_br_context(levels, pos, true),
            "LF BR context mismatch: pos={pos}, levels={levels:?}"
        );
        assert_eq!(
            luma_br_context(levels, pos, false),
            expected_luma_br_context(levels, pos, false),
            "non-LF BR context mismatch: pos={pos}, levels={levels:?}"
        );
        for scan_index in 0..TX4X4_SAMPLES {
            for is_eob_coefficient in [false, true] {
                assert_eq!(
                    luma_nz_map_context(levels, pos, scan_index, is_eob_coefficient),
                    expected_luma_nz_map_context(
                        levels,
                        pos,
                        scan_index,
                        is_eob_coefficient,
                    ),
                    "luma NZ-map mismatch: pos={pos}, scan={scan_index}, eob={is_eob_coefficient}, levels={levels:?}"
                );
            }
        }
    }
}

#[test]
fn luma_contexts_preserve_lf_non_lf_and_clamping_formulas() {
    for levels in [
        [0; TX4X4_SAMPLES],
        core::array::from_fn(|index| (index % 7) as u32),
        core::array::from_fn(|index| ((index * 71 + 17) % 211) as u32),
    ] {
        assert_luma_contexts_for_levels(&levels);
    }

    for source_pos in 0..TX4X4_SAMPLES {
        for level in [1, 2, 3, 4, 5, 6, 7, 126, 127, 128, 211] {
            let mut levels = [0; TX4X4_SAMPLES];
            levels[source_pos] = level;
            assert_luma_contexts_for_levels(&levels);
        }
    }
}
