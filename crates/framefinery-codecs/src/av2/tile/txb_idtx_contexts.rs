fn idtx_bob_context(scan_index: usize) -> usize {
    if scan_index <= TX4X4_SAMPLES / 8 {
        0
    } else if scan_index <= TX4X4_SAMPLES / 4 {
        1
    } else {
        2
    }
}

fn idtx_upper_levels_context(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> usize {
    idtx_neighbour_levels_context(levels, pos, 3)
}

fn idtx_br_context(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> usize {
    idtx_neighbour_levels_context(levels, pos, 5)
}

fn idtx_sign_context(
    levels: &[u32; TX4X4_SAMPLES],
    coefficients: &[i32; TX4X4_SAMPLES],
    pos: usize,
) -> usize {
    let neighbours = idtx_neighbour_positions(pos);
    let sign_sum = [neighbours.left, neighbours.above, neighbours.above_left]
        .into_iter()
        .flatten()
        .filter(|&neighbour| levels[neighbour] != 0)
        .map(|neighbour| idtx_sign_value(coefficients[neighbour]))
        .sum::<i32>();
    let mut ctx = if sign_sum > 2 {
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
    if levels[pos] > 3 && ctx != 0 {
        ctx += 2;
    }
    ctx
}

fn idtx_sign_value(coefficient: i32) -> i32 {
    if coefficient < 0 {
        -1
    } else {
        1
    }
}

fn idtx_neighbour_levels_context(
    levels: &[u32; TX4X4_SAMPLES],
    pos: usize,
    neighbour_limit: u32,
) -> usize {
    let neighbours = idtx_neighbour_positions(pos);
    let magnitude = idtx_neighbour_level(levels, neighbours.left).min(neighbour_limit)
        + idtx_neighbour_level(levels, neighbours.above).min(neighbour_limit);
    magnitude.min(6) as usize
}

fn idtx_neighbour_level(levels: &[u32; TX4X4_SAMPLES], neighbour: Option<usize>) -> u32 {
    neighbour.map_or(0, |neighbour| levels[neighbour].min(127))
}

struct Av2IdtxNeighbourPositions {
    left: Option<usize>,
    above: Option<usize>,
    above_left: Option<usize>,
}

fn idtx_neighbour_positions(pos: usize) -> Av2IdtxNeighbourPositions {
    debug_assert!(pos < TX4X4_SAMPLES);
    let row = pos / TX4X4_SIZE;
    let col = pos % TX4X4_SIZE;
    Av2IdtxNeighbourPositions {
        left: (col > 0).then(|| pos - 1),
        above: (row > 0).then(|| pos - TX4X4_SIZE),
        above_left: (row > 0 && col > 0).then(|| pos - TX4X4_SIZE - 1),
    }
}
