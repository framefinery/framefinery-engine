use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use framefinery_codecs::bench::av2::{bench, Av2VideoGeometry};
use framefinery_codecs::SampleBitDepth;

fn av2_palette_selection(c: &mut Criterion) {
    let bit_depth = SampleBitDepth::new(8).expect("8-bit samples are supported");
    let mut group = c.benchmark_group("av2_palette_selection_444");
    for &(width, height) in &[(64usize, 64usize), (128usize, 128usize)] {
        let geometry = Av2VideoGeometry { width, height };
        let frame = screen_content_444_frame(width, height);
        group.throughput(Throughput::Elements((width * height) as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{width}x{height}")),
            &frame,
            |b, frame| {
                b.iter(|| {
                    bench::luma_palette_444_checksum(
                        black_box(frame),
                        black_box(geometry),
                        black_box(bit_depth),
                    )
                    .expect("deterministic palette benchmark input should be valid")
                })
            },
        );
    }
    group.finish();
}

fn av2_transform_quant(c: &mut Criterion) {
    let bit_depth = SampleBitDepth::new(8).expect("8-bit samples are supported");
    let residuals = residual_blocks(4096);
    let mut group = c.benchmark_group("av2_transform_quant_roundtrip_4x4");
    group.throughput(Throughput::Elements(residuals.len() as u64));
    for &qindex in &[40u16, 80u16, 128u16] {
        group.bench_with_input(
            BenchmarkId::from_parameter(qindex),
            &qindex,
            |b, &qindex| {
                b.iter(|| {
                    bench::transform_quant_roundtrip_checksum(
                        black_box(&residuals),
                        black_box(qindex),
                        black_box(bit_depth),
                    )
                })
            },
        );
    }
    group.finish();
}

fn screen_content_444_frame(width: usize, height: usize) -> Vec<u8> {
    let plane_len = width * height;
    let mut frame = Vec::with_capacity(plane_len * 3);
    for y in 0..height {
        for x in 0..width {
            let tile = ((x / 8) ^ (y / 8)) as u8;
            frame.push(tile.wrapping_mul(37).wrapping_add((x % 8) as u8));
        }
    }
    for y in 0..height {
        for x in 0..width {
            frame.push(96u8.wrapping_add((((x / 16) + y) % 64) as u8));
        }
    }
    for y in 0..height {
        for x in 0..width {
            frame.push(160u8.wrapping_sub(((x + (y / 16)) % 64) as u8));
        }
    }
    frame
}

fn residual_blocks(count: usize) -> Vec<[i32; 16]> {
    let mut blocks = Vec::with_capacity(count);
    let mut state = 0x1234_5678u32;
    for block_idx in 0..count {
        let mut block = [0i32; 16];
        for (sample_idx, sample) in block.iter_mut().enumerate() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let trend = ((block_idx + sample_idx) & 7) as i32 - 3;
            *sample = ((state >> 24) as i32 - 128) / 3 + trend;
        }
        blocks.push(block);
    }
    blocks
}

criterion_group!(benches, av2_palette_selection, av2_transform_quant);
criterion_main!(benches);
