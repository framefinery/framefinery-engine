fn expected_idtx_positions(pos: usize) -> [Option<usize>; 3] {
    let row = pos / TX4X4_SIZE;
    let col = pos % TX4X4_SIZE;
    [
        (col > 0).then(|| pos - 1),
        (row > 0).then(|| pos - TX4X4_SIZE),
        (row > 0 && col > 0).then(|| pos - TX4X4_SIZE - 1),
    ]
}

fn expected_idtx_level_context(
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    neighbour_limit: u32,
) -> usize {
    let [left, above, _] = expected_idtx_positions(pos);
    let level =
        |neighbour: Option<usize>| neighbour.map_or(0, |neighbour| levels[neighbour].min(127));
    (level(left).min(neighbour_limit) + level(above).min(neighbour_limit)).min(6) as usize
}

fn expected_idtx_sign_context(
    levels: &[u32; TX4X4_SAMPLES],
    coefficients: &[i32; TX4X4_SAMPLES],
    pos: usize,
) -> usize {
    let sign_sum = expected_idtx_positions(pos)
        .into_iter()
        .flatten()
        .filter(|&neighbour| levels[neighbour] != 0)
        .map(|neighbour| if coefficients[neighbour] < 0 { -1 } else { 1 })
        .sum::<i32>();
    let mut context = if sign_sum > 2 {
        5
    } else if sign_sum < -2 {
        6
    } else if sign_sum > 0 {
        1
    } else if sign_sum < 0 {
        2
    } else {
        0
    };
    if levels[pos] > 3 && context != 0 {
        context += 2;
    }
    context
}

fn assert_idtx_level_contexts(levels: &[u32; TX4X4_SAMPLES]) {
    for pos in 0..TX4X4_SAMPLES {
        assert_eq!(
            idtx_upper_levels_context(levels, pos),
            expected_idtx_level_context(levels, pos, 3),
            "IDTX upper-level context mismatch: pos={pos}, levels={levels:?}"
        );
        assert_eq!(
            idtx_br_context(levels, pos),
            expected_idtx_level_context(levels, pos, 5),
            "IDTX base-range context mismatch: pos={pos}, levels={levels:?}"
        );
    }
}

#[test]
fn idtx_contexts_preserve_edges_magnitude_limits_and_sign_policy() {
    for scan_index in 0..TX4X4_SAMPLES {
        let expected = if scan_index <= TX4X4_SAMPLES / 8 {
            0
        } else if scan_index <= TX4X4_SAMPLES / 4 {
            1
        } else {
            2
        };
        assert_eq!(idtx_bob_context(scan_index), expected);
    }

    for levels in [
        [0; TX4X4_SAMPLES],
        core::array::from_fn(|index| (index % 7) as u32),
        core::array::from_fn(|index| ((index * 71 + 17) % 211) as u32),
    ] {
        assert_idtx_level_contexts(&levels);
    }
    for source_pos in 0..TX4X4_SAMPLES {
        for level in [1, 2, 3, 4, 5, 6, 126, 127, 128, 211] {
            let mut levels = [0; TX4X4_SAMPLES];
            levels[source_pos] = level;
            assert_idtx_level_contexts(&levels);
        }
    }

    for pos in 0..TX4X4_SAMPLES {
        let positions = expected_idtx_positions(pos);
        for presence_mask in 0u8..8 {
            for sign_mask in 0u8..8 {
                for current_level in [3, 4] {
                    let mut levels = [0; TX4X4_SAMPLES];
                    let mut coefficients = [0; TX4X4_SAMPLES];
                    levels[pos] = current_level;
                    for (index, neighbour) in positions.into_iter().enumerate() {
                        let Some(neighbour) = neighbour else {
                            continue;
                        };
                        if presence_mask & (1 << index) != 0 {
                            levels[neighbour] = 128;
                            coefficients[neighbour] =
                                if sign_mask & (1 << index) != 0 { -1 } else { 1 };
                        }
                    }
                    assert_eq!(
                        idtx_sign_context(&levels, &coefficients, pos),
                        expected_idtx_sign_context(&levels, &coefficients, pos),
                        "IDTX sign context mismatch: pos={pos}, presence={presence_mask:03b}, signs={sign_mask:03b}, current_level={current_level}"
                    );
                }
            }
        }
    }
}
