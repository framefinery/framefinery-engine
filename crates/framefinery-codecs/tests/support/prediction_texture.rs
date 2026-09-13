use framefinery_api::{write_planar_sample, Frame, FrameInfo};

pub const PHASES: [u16; 6] = [0, 90, 90, 91, 91, 92];

/// Stable texture in the right half; the left half changes between pictures.
/// Chroma also changes; the retained texture exercises low bits of 10-bit input.
pub fn frames(info: FrameInfo) -> Vec<Frame> {
    PHASES
        .into_iter()
        .map(|phase| {
            let mut data = vec![0; info.expected_len()];
            let bits = info.format.bit_depth();
            let mask = (1u16 << bits.bits()) - 1;
            let mut offset = 0;
            for (plane, (width, height)) in [
                (info.width, info.height),
                (info.width / 2, info.height / 2),
                (info.width / 2, info.height / 2),
            ]
            .into_iter()
            .enumerate()
            {
                for y in 0..height {
                    for x in 0..width {
                        let value = if x < width / 2 {
                            (16 + plane as u16 * 32 + phase) << (bits.bits() - 8)
                        } else {
                            // Deterministic spatial texture, independent of frame number.
                            let seed = (x as u32)
                                .wrapping_mul(0x45d9_f3b)
                                .wrapping_add((y as u32).wrapping_mul(0x119d_e1f3))
                                .wrapping_add(plane as u32 * 193);
                            ((seed ^ (seed >> 16)) as u16) & mask
                        };
                        write_planar_sample(&mut data, offset + y * width + x, value, bits)
                            .unwrap();
                    }
                }
                offset += width * height;
            }
            Frame::new(info, data).unwrap()
        })
        .collect()
}
