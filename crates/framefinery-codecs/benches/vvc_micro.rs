use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use framefinery_codecs::vvc::{bench, VvcVideoGeometry};
use framefinery_codecs::{ChromaSampling, PixelFormat, SampleBitDepth};

struct VvcBenchCase {
    name: &'static str,
    width: usize,
    height: usize,
    chroma_sampling: ChromaSampling,
    bit_depth: u8,
}

struct VvcBenchMode {
    name: &'static str,
    lossless: bool,
    qp: Option<u8>,
}

fn vvc_residual_ctu(c: &mut Criterion) {
    let cases = [
        VvcBenchCase {
            name: "420p8_64x64",
            width: 64,
            height: 64,
            chroma_sampling: ChromaSampling::Cs420,
            bit_depth: 8,
        },
        VvcBenchCase {
            name: "444p8_64x64",
            width: 64,
            height: 64,
            chroma_sampling: ChromaSampling::Cs444,
            bit_depth: 8,
        },
        VvcBenchCase {
            name: "444p10_64x64",
            width: 64,
            height: 64,
            chroma_sampling: ChromaSampling::Cs444,
            bit_depth: 10,
        },
    ];
    let modes = [
        VvcBenchMode {
            name: "lossless",
            lossless: true,
            qp: None,
        },
        VvcBenchMode {
            name: "lossy_qp24",
            lossless: false,
            qp: Some(24),
        },
    ];

    let mut group = c.benchmark_group("vvc_residual_ctu_screen_content");
    for case in cases {
        let bit_depth =
            SampleBitDepth::new(case.bit_depth).expect("benchmark bit depth must be valid");
        let format = PixelFormat::planar_yuv(case.chroma_sampling, bit_depth);
        let geometry = VvcVideoGeometry {
            width: case.width,
            height: case.height,
        };
        let frame =
            screen_content_planar_frame(case.width, case.height, case.chroma_sampling, bit_depth);
        let input = bench::ResidualCtuInput::from_planar_frame(&frame, geometry, format)
            .expect("deterministic VVC benchmark input should be valid");
        group.throughput(Throughput::Elements((case.width * case.height) as u64));
        for mode in &modes {
            group.bench_with_input(
                BenchmarkId::new(case.name, mode.name),
                &input,
                |b, input| {
                    b.iter(|| {
                        bench::residual_ctu_checksum(
                            black_box(input),
                            black_box(mode.lossless),
                            black_box(mode.qp),
                        )
                    })
                },
            );
        }
    }
    group.finish();
}

fn screen_content_planar_frame(
    width: usize,
    height: usize,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
) -> Vec<u8> {
    let chroma_width = width / chroma_sampling.subsample_x();
    let chroma_height = height / chroma_sampling.subsample_y();
    let bytes_per_sample = bit_depth.bytes_per_sample();
    let frame_samples = width * height + chroma_width * chroma_height * 2;
    let mut frame = Vec::with_capacity(frame_samples * bytes_per_sample);
    for y in 0..height {
        for x in 0..width {
            let tile = ((x / 8) ^ (y / 8)) as u16;
            let edge = if x.is_multiple_of(16) || y.is_multiple_of(16) {
                96
            } else {
                0
            };
            let sample = tile
                .wrapping_mul(83)
                .wrapping_add(((x + y * 3) & 63) as u16)
                .wrapping_add(edge);
            push_sample(&mut frame, scale_sample(sample, bit_depth), bit_depth);
        }
    }
    for y in 0..chroma_height {
        for x in 0..chroma_width {
            let sample = 128u16.wrapping_add((((x / 4) * 11 + (y / 2) * 17 + y) & 63) as u16);
            push_sample(&mut frame, scale_sample(sample, bit_depth), bit_depth);
        }
    }
    for y in 0..chroma_height {
        for x in 0..chroma_width {
            let sample = 192u16.wrapping_sub((((x / 2) * 7 + y * 13 + x) & 63) as u16);
            push_sample(&mut frame, scale_sample(sample, bit_depth), bit_depth);
        }
    }
    frame
}

fn scale_sample(sample_8bit: u16, bit_depth: SampleBitDepth) -> u16 {
    let sample = sample_8bit.min(255);
    let shift = bit_depth.bits().saturating_sub(8);
    sample << shift
}

fn push_sample(frame: &mut Vec<u8>, sample: u16, bit_depth: SampleBitDepth) {
    if bit_depth.bytes_per_sample() == 1 {
        frame.push(sample as u8);
    } else {
        frame.extend_from_slice(&sample.to_le_bytes());
    }
}

criterion_group!(benches, vvc_residual_ctu);
criterion_main!(benches);
