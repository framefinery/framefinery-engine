use crate::PixelFormat;

use super::residual::{
    quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes,
    VvcLumaModeSearchState, VvcTransformSkipQuantTables, VVC_DEFAULT_LOSSY_CHROMA_QP,
    VVC_DEFAULT_LOSSY_LUMA_QP,
};
use super::{
    sample_vvc_yuv_frame, vvc_lossless_slice_qp, VvcCtuRegion, VvcEncodeParams,
    VvcReconstructionFrame, VvcResidualCodingMode, VvcResidualCodingPolicy, VvcSampledFrame,
    VvcVideoGeometry,
};

#[derive(Debug, Clone)]
pub struct ResidualCtuInput {
    frame: VvcSampledFrame,
}

impl ResidualCtuInput {
    pub fn from_planar_frame(
        frame: &[u8],
        geometry: VvcVideoGeometry,
        format: PixelFormat,
    ) -> Result<Self, String> {
        Ok(Self {
            frame: sample_vvc_yuv_frame(frame, VvcEncodeParams { frames: 1 }, geometry, format)?,
        })
    }
}

pub fn residual_ctu_checksum(input: &ResidualCtuInput, lossless: bool, qp: Option<u8>) -> u64 {
    let residual_mode = if lossless {
        VvcResidualCodingMode::Lossless
    } else {
        VvcResidualCodingMode::Lossy
    };
    let luma_qp = if lossless {
        vvc_lossless_slice_qp(input.frame.format.bit_depth)
    } else {
        qp.map_or(VVC_DEFAULT_LOSSY_LUMA_QP, i32::from)
    };
    let chroma_qp = if lossless {
        luma_qp
    } else {
        VVC_DEFAULT_LOSSY_CHROMA_QP
    };
    let mut reconstruction =
        VvcReconstructionFrame::new_neutral(input.frame.geometry, input.frame.format);
    let mut luma_mode_search_state = VvcLumaModeSearchState::new_for_geometry(input.frame.geometry);
    let transform_skip_quant_tables =
        VvcTransformSkipQuantTables::new(input.frame.format.bit_depth, luma_qp, chroma_qp);
    let region = VvcCtuRegion {
        slice_address: 0,
        origin_x: 0,
        origin_y: 0,
        geometry: input.frame.geometry,
    };
    let policy = VvcResidualCodingPolicy::new(input.frame.format, residual_mode);
    let quantized = quantize_vvc_residual_ctu_into_frame_reconstruction_with_qp_and_luma_modes(
        &input.frame,
        &mut reconstruction,
        region,
        policy,
        luma_qp,
        chroma_qp,
        &mut luma_mode_search_state,
        &transform_skip_quant_tables,
    );

    let mut checksum = mix_u64(0x9ae1_6a3b_2f90_4405, quantized.luma_tu_count as u64);
    checksum = mix_u64(checksum, quantized.chroma_tu_count as u64);
    checksum = mix_i16_slice(
        checksum,
        &quantized.luma_tu_dc_levels[..quantized.luma_tu_count],
    );
    checksum = mix_i16_matrix(
        checksum,
        &quantized.luma_tu_ac_levels[..quantized.luma_tu_count],
    );
    checksum = mix_i16_slice(
        checksum,
        &quantized.cb_tu_dc_levels[..quantized.chroma_tu_count],
    );
    checksum = mix_i16_slice(
        checksum,
        &quantized.cr_tu_dc_levels[..quantized.chroma_tu_count],
    );
    checksum = mix_i16_matrix(
        checksum,
        &quantized.cb_tu_ac_levels[..quantized.chroma_tu_count],
    );
    checksum = mix_i16_matrix(
        checksum,
        &quantized.cr_tu_ac_levels[..quantized.chroma_tu_count],
    );
    checksum = mix_u16_slice(checksum, &reconstruction.luma);
    checksum = mix_u16_slice(checksum, &reconstruction.cb);
    mix_u16_slice(checksum, &reconstruction.cr)
}

fn mix_i16_slice(mut checksum: u64, values: &[i16]) -> u64 {
    for &value in values {
        checksum = mix_u64(checksum, u64::from(value as u16));
    }
    checksum
}

fn mix_i16_matrix<const N: usize>(mut checksum: u64, rows: &[[i16; N]]) -> u64 {
    for row in rows {
        checksum = mix_i16_slice(checksum, row);
    }
    checksum
}

fn mix_u16_slice(mut checksum: u64, values: &[u16]) -> u64 {
    for &value in values {
        checksum = mix_u64(checksum, u64::from(value));
    }
    checksum
}

fn mix_u64(checksum: u64, value: u64) -> u64 {
    checksum.rotate_left(7) ^ value.wrapping_mul(0x9e37_79b1_85eb_ca87)
}
