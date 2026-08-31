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
