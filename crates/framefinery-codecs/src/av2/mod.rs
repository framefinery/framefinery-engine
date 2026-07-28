use std::io::{Read, Write};

use crate::picture::{
    read_input_frame, ChromaSampling, FrameLimit, Picture, PixelFormat, SampleBitDepth,
};

// Keep the public/internal module names stable while grouping the imported AV2
// implementation by domain on disk.
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
#[path = "benchmarks/bench.rs"]
pub mod bench;
#[path = "mode/decision.rs"]
mod decision;
#[path = "bitstream/entropy.rs"]
pub mod entropy;
#[path = "inter/ibc.rs"]
mod ibc;
#[path = "prediction/intra_prediction.rs"]
mod intra_prediction;
#[path = "inter/motion.rs"]
mod motion;
mod palette;
#[path = "image/planar.rs"]
mod planar;
#[cfg(feature = "av2-sb-bit-profile")]
#[path = "bitstream/sb_bits.rs"]
mod sb_bits;
#[path = "bitstream/syntax.rs"]
mod syntax;
mod tile;

use ibc::{Av2LocalIbc444, Av2LocalIbcStats, Av2LocalIbcTileBounds};
use motion::{
    Av2LosslessMotionMap, Av2MotionSearchRegion, Av2MotionVector, AV2_LOSSLESS_ME_BLOCK_SIZE,
};
use palette::Av2LumaPalette444;
use syntax::{Av2SyntaxPayload, Av2SyntaxWriter};
use tile::{
    av2_black_444_tile_entropy_payload_for_region_with_fields,
    av2_black_444_tile_entropy_payload_for_region_with_intrabc_and_fields,
    av2_black_tile_entropy_payload_for_region,
    av2_lossless_mixed_inter_intra_tile_entropy_payload_for_region_with_fields,
    av2_lossless_mixed_inter_tile_entropy_payload_for_region_with_fields,
    av2_lossless_new_mv_inter_tile_entropy_payload_for_region_with_fields,
    av2_lossless_subsampled_fast_tile_entropy_payload_for_region_with_fields,
    av2_lossless_subsampled_regular_inter_intra_tile_entropy_payload_for_region_with_fields,
    av2_lossless_subsampled_tile_entropy_payload_for_region_with_fields,
    av2_lossless_zero_mv_inter_tile_entropy_payload_for_region_with_fields,
    av2_lossy_fixed_inter_intra_tile_entropy_payload_for_region_with_fields,
    av2_lossy_subsampled_tile_entropy_payload_for_region,
    av2_lossy_subsampled_tile_entropy_payload_for_region_with_fields,
    av2_luma_palette_444_tile_entropy_payload_for_region_with_fields, Av2LosslessInterBlockMode,
    Av2LosslessInterTileBlockModes, Av2TileRegion,
};

include!("format.rs");
include!("layout.rs");
include!("api.rs");
include!("frame_mode.rs");
include!("encode.rs");
include!("image/rgb.rs");
include!("predictive.rs");
include!("trace.rs");
include!("reconstruction.rs");
include!("headers.rs");
include!("obu.rs");

#[cfg(test)]
mod tests;
