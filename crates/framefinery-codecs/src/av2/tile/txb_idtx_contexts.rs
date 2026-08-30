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
    let mag = idtx_left_level(levels, pos).min(3) + idtx_above_level(levels, pos).min(3);
    mag.min(6) as usize
}

fn idtx_br_context(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> usize {
    let mag = idtx_left_level(levels, pos).min(5) + idtx_above_level(levels, pos).min(5);
    mag.min(6) as usize
}

fn idtx_sign_context(
    levels: &[u32; TX4X4_SAMPLES],
    coefficients: &[i32; TX4X4_SAMPLES],
    pos: usize,
) -> usize {
    let mut sign_sum = 0i32;
    if let Some(left) = idtx_left_pos(pos).filter(|&left| levels[left] != 0) {
        sign_sum += idtx_sign_value(coefficients[left]);
    }
    if let Some(above) = idtx_above_pos(pos).filter(|&above| levels[above] != 0) {
        sign_sum += idtx_sign_value(coefficients[above]);
    }
    if let Some(above_left) = idtx_above_left_pos(pos).filter(|&above_left| levels[above_left] != 0)
    {
        sign_sum += idtx_sign_value(coefficients[above_left]);
    }
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

fn idtx_left_level(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> u32 {
    idtx_left_pos(pos).map_or(0, |left| levels[left].min(127))
}

fn idtx_above_level(levels: &[u32; TX4X4_SAMPLES], pos: usize) -> u32 {
    idtx_above_pos(pos).map_or(0, |above| levels[above].min(127))
}

fn idtx_left_pos(pos: usize) -> Option<usize> {
    if pos % TX4X4_SIZE != 0 {
        Some(pos - 1)
    } else {
        None
    }
}

fn idtx_above_pos(pos: usize) -> Option<usize> {
    if pos >= TX4X4_SIZE {
        Some(pos - TX4X4_SIZE)
    } else {
        None
    }
}

fn idtx_above_left_pos(pos: usize) -> Option<usize> {
    if pos % TX4X4_SIZE != 0 && pos >= TX4X4_SIZE {
        Some(pos - TX4X4_SIZE - 1)
    } else {
        None
    }
}
