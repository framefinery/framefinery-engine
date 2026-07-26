use crate::SampleBitDepth;

use super::{palette, tile, Av2VideoGeometry};

pub fn luma_palette_444_checksum(
    frame: &[u8],
    geometry: Av2VideoGeometry,
    bit_depth: SampleBitDepth,
) -> Result<u64, String> {
    let palette = palette::build_luma_palette_444(frame, geometry, bit_depth)?;
    let mut checksum = mix_u64(0xcbf2_9ce4_8422_2325, palette.reconstruction().len() as u64);
    for y in (0..palette.height()).step_by(8) {
        for x in (0..palette.width()).step_by(8) {
            let region = palette.syntax_region_palette(x, y, 8, 8);
            checksum = mix_u64(checksum, region.color_count() as u64);
            for &color in region.colors() {
                checksum = mix_u64(checksum, u64::from(color));
            }
        }
    }
    Ok(checksum)
}

pub fn transform_quant_roundtrip_checksum(
    residuals: &[[i32; 16]],
    qindex: u16,
    bit_depth: SampleBitDepth,
) -> u64 {
    tile::bench_transform_quant_roundtrip_checksum(residuals, qindex, bit_depth)
}

fn mix_u64(checksum: u64, value: u64) -> u64 {
    checksum.rotate_left(5) ^ value.wrapping_mul(0x9e37_79b1_85eb_ca87)
}
