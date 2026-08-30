use crate::picture::{ChromaSampling, SampleBitDepth};

use super::super::{
    chroma_subsample_x, chroma_subsample_y, vvc_neutral_sample, VvcBdpcmMode, VvcChromaCclmMode,
    VvcCodingTreeNode, VvcIntraPredictionMode, VvcSample, VvcVideoGeometry, VVC_CTU_SIZE,
};

const VVC_LUMA_MODE_DIAGONAL: u8 = 34;
const VVC_LUMA_MODE_VERTICAL: u8 = 50;
const VVC_LUMA_MODE_HORIZONTAL: u8 = 18;
const VVC_MAX_MULTI_REF_LINE_IDX: usize = 2;
const VVC_ANGULAR_REFERENCE_CAPACITY: usize =
    VVC_CTU_SIZE * 2 + 4 + 33 * VVC_MAX_MULTI_REF_LINE_IDX;
const VVC_INTRA_ANG_TABLE: [i32; 32] = [
    0, 1, 2, 3, 4, 6, 8, 10, 12, 14, 16, 18, 20, 23, 26, 29, 32, 35, 39, 45, 51, 57, 64, 73, 86,
    102, 128, 171, 256, 341, 512, 1024,
];
const VVC_INTRA_INV_ANG_TABLE: [i32; 32] = [
    0, 16384, 8192, 5461, 4096, 2731, 2048, 1638, 1365, 1170, 1024, 910, 819, 712, 630, 565, 512,
    468, 420, 364, 321, 287, 256, 224, 191, 161, 128, 96, 64, 48, 32, 16,
];
const VVC_INTRA_REFERENCE_FILTER_THRESHOLD: [u8; 8] = [24, 24, 24, 14, 2, 0, 0, 0];
const VVC_CHROMA_4TAP_INTERPOLATION_FILTER: [[i32; 4]; 32] = [
    [0, 64, 0, 0],
    [-1, 63, 2, 0],
    [-2, 62, 4, 0],
    [-2, 60, 7, -1],
    [-2, 58, 10, -2],
    [-3, 57, 12, -2],
    [-4, 56, 14, -2],
    [-4, 55, 15, -2],
    [-4, 54, 16, -2],
    [-5, 53, 18, -2],
    [-6, 52, 20, -2],
    [-6, 49, 24, -3],
    [-6, 46, 28, -4],
    [-5, 44, 29, -4],
    [-4, 42, 30, -4],
    [-4, 39, 33, -4],
    [-4, 36, 36, -4],
    [-4, 33, 39, -4],
    [-4, 30, 42, -4],
    [-4, 29, 44, -5],
    [-4, 28, 46, -6],
    [-3, 24, 49, -6],
    [-2, 20, 52, -6],
    [-2, 18, 53, -5],
    [-2, 16, 54, -4],
    [-2, 15, 55, -4],
    [-2, 14, 56, -4],
    [-2, 12, 57, -3],
    [-2, 10, 58, -2],
    [-1, 7, 60, -2],
    [0, 4, 62, -2],
    [0, 2, 63, -1],
];
const VVC_CHROMA_422_INTRA_ANGLE_MAPPING_TABLE: [u8; 67] = [
    0, 1, 61, 62, 63, 64, 65, 66, 2, 3, 5, 6, 8, 10, 12, 13, 14, 16, 18, 20, 22, 23, 24, 26, 28,
    30, 31, 33, 34, 35, 36, 37, 38, 39, 40, 41, 41, 42, 43, 43, 44, 44, 45, 45, 46, 47, 48, 48, 49,
    49, 50, 51, 51, 52, 52, 53, 54, 55, 55, 56, 56, 57, 57, 58, 59, 59, 60,
];

include!("prediction_reference_edges.rs");
include!("prediction_reference_access.rs");
include!("prediction_reference_filters.rs");
include!("prediction_pdpc.rs");
include!("prediction_angular_references.rs");
include!("prediction_reconstruction.rs");

pub(in crate::vvc) fn predict_vvc_luma_intra_block_into_with_availability(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcIntraPredictionMode,
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    predict_vvc_luma_intra_block_into_with_mrl_and_availability(
        prediction,
        scratch,
        mode,
        luma,
        geometry,
        node,
        bit_depth,
        0,
        availability,
    );
}

pub(in crate::vvc) fn predict_vvc_luma_intra_block_into_with_mrl_and_availability(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcIntraPredictionMode,
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    mrl_index: u8,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    let reference_line = usize::from(mrl_index.min(VVC_MAX_MULTI_REF_LINE_IDX as u8));
    match mode {
        VvcIntraPredictionMode::Planar => predict_vvc_luma_planar_block_into(
            prediction,
            scratch,
            luma,
            geometry,
            node,
            bit_depth,
            reference_line,
            availability,
        ),
        VvcIntraPredictionMode::Dc => predict_vvc_luma_dc_block_into(
            prediction,
            scratch,
            luma,
            geometry,
            node,
            bit_depth,
            reference_line,
            availability,
        ),
        VvcIntraPredictionMode::Horizontal
        | VvcIntraPredictionMode::Vertical
        | VvcIntraPredictionMode::Angular(_) => predict_vvc_luma_angular_block_into(
            prediction,
            scratch,
            mode,
            luma,
            geometry,
            node,
            bit_depth,
            reference_line,
            availability,
        ),
    }
}

pub(in crate::vvc) fn predict_vvc_luma_dc_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    predict_vvc_dc_block_into(
        prediction,
        scratch,
        luma,
        geometry.width,
        geometry.height,
        usize::from(node.x),
        usize::from(node.y),
        usize::from(node.width),
        usize::from(node.height),
        bit_depth,
        reference_line,
        availability,
    );
}

pub(in crate::vvc) fn predict_vvc_luma_planar_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    predict_vvc_planar_block_into(
        prediction,
        scratch,
        luma,
        geometry.width,
        geometry.height,
        usize::from(node.x),
        usize::from(node.y),
        usize::from(node.width),
        usize::from(node.height),
        bit_depth,
        true,
        reference_line,
        availability,
    );
}

fn predict_vvc_luma_angular_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcIntraPredictionMode,
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    let mode_index = mode.luma_mode_index();
    predict_vvc_angular_block_into(
        prediction,
        scratch,
        luma,
        geometry.width,
        geometry.height,
        usize::from(node.x),
        usize::from(node.y),
        usize::from(node.width),
        usize::from(node.height),
        mode_index,
        bit_depth,
        true,
        reference_line,
        availability,
    );
}

fn predict_vvc_chroma_dc_block_into_with_availability(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    chroma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    let subsample_x = chroma_subsample_x(chroma_sampling);
    let subsample_y = chroma_subsample_y(chroma_sampling);
    predict_vvc_dc_block_into(
        prediction,
        scratch,
        chroma,
        geometry.width / subsample_x,
        geometry.height / subsample_y,
        usize::from(node.x) / subsample_x,
        usize::from(node.y) / subsample_y,
        usize::from(node.width) / subsample_x,
        usize::from(node.height) / subsample_y,
        bit_depth,
        0,
        availability,
    );
}

pub(in crate::vvc) fn predict_vvc_chroma_intra_block_into_with_availability(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcIntraPredictionMode,
    chroma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    match mode {
        VvcIntraPredictionMode::Planar => predict_vvc_chroma_planar_block_into(
            prediction,
            scratch,
            chroma,
            geometry,
            node,
            chroma_sampling,
            bit_depth,
            availability,
        ),
        VvcIntraPredictionMode::Dc => predict_vvc_chroma_dc_block_into_with_availability(
            prediction,
            scratch,
            chroma,
            geometry,
            node,
            chroma_sampling,
            bit_depth,
            availability,
        ),
        VvcIntraPredictionMode::Horizontal
        | VvcIntraPredictionMode::Vertical
        | VvcIntraPredictionMode::Angular(_) => predict_vvc_chroma_angular_block_into(
            prediction,
            scratch,
            mode,
            chroma,
            geometry,
            node,
            chroma_sampling,
            bit_depth,
            availability,
        ),
    }
}

pub(in crate::vvc) fn predict_vvc_chroma_cclm_block_into_with_availability(
    prediction: &mut Vec<VvcSample>,
    mode: VvcChromaCclmMode,
    chroma: &[VvcSample],
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    chroma_availability: Option<VvcPlaneAvailability<'_>>,
    luma_availability: Option<VvcPlaneAvailability<'_>>,
) {
    let mut inner_luma = Vec::new();
    predict_vvc_chroma_cclm_block_with_luma_scratch_into(
        prediction,
        &mut inner_luma,
        mode,
        chroma,
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        chroma_availability,
        luma_availability,
    );
}

pub(in crate::vvc) fn predict_vvc_chroma_cclm_pair_into_with_availability(
    cb_prediction: &mut Vec<VvcSample>,
    cr_prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcChromaCclmMode,
    cb: &[VvcSample],
    cr: &[VvcSample],
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    cb_availability: Option<VvcPlaneAvailability<'_>>,
    cr_availability: Option<VvcPlaneAvailability<'_>>,
    luma_availability: Option<VvcPlaneAvailability<'_>>,
) {
    let cb_layout = vvc_cclm_layout(mode, cb_availability, geometry, node, chroma_sampling);
    let cr_layout = vvc_cclm_layout(mode, cr_availability, geometry, node, chroma_sampling);
    if cb_layout != cr_layout {
        predict_vvc_chroma_cclm_block_with_luma_scratch_into(
            cb_prediction,
            &mut scratch.cclm_inner_luma,
            mode,
            cb,
            luma,
            geometry,
            node,
            chroma_sampling,
            bit_depth,
            cb_availability,
            luma_availability,
        );
        predict_vvc_chroma_cclm_block_with_luma_scratch_into(
            cr_prediction,
            &mut scratch.cclm_inner_luma,
            mode,
            cr,
            luma,
            geometry,
            node,
            chroma_sampling,
            bit_depth,
            cr_availability,
            luma_availability,
        );
        return;
    }

    prepare_vvc_cclm_inner_luma_into(
        &mut scratch.cclm_inner_luma,
        cb_layout,
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        luma_availability,
    );
    let luma_selection = derive_vvc_cclm_luma_selection(
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        luma_availability,
        cb_layout,
    );
    predict_vvc_chroma_cclm_block_from_inner_luma_into(
        cb_prediction,
        &scratch.cclm_inner_luma,
        cb_layout,
        luma_selection,
        cb,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        cb_availability,
    );
    predict_vvc_chroma_cclm_block_from_inner_luma_into(
        cr_prediction,
        &scratch.cclm_inner_luma,
        cb_layout,
        luma_selection,
        cr,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        cr_availability,
    );
}

fn predict_vvc_chroma_cclm_block_with_luma_scratch_into(
    prediction: &mut Vec<VvcSample>,
    inner_luma: &mut Vec<i32>,
    mode: VvcChromaCclmMode,
    chroma: &[VvcSample],
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    chroma_availability: Option<VvcPlaneAvailability<'_>>,
    luma_availability: Option<VvcPlaneAvailability<'_>>,
) {
    let layout = vvc_cclm_layout(mode, chroma_availability, geometry, node, chroma_sampling);
    prepare_vvc_cclm_inner_luma_into(
        inner_luma,
        layout,
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        luma_availability,
    );
    let luma_selection = derive_vvc_cclm_luma_selection(
        luma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        luma_availability,
        layout,
    );
    predict_vvc_chroma_cclm_block_from_inner_luma_into(
        prediction,
        inner_luma,
        layout,
        luma_selection,
        chroma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        chroma_availability,
    );
}

fn predict_vvc_chroma_cclm_block_from_inner_luma_into(
    prediction: &mut Vec<VvcSample>,
    inner_luma: &[i32],
    layout: VvcCclmLayout,
    luma_selection: VvcCclmLumaSelection,
    chroma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    chroma_availability: Option<VvcPlaneAvailability<'_>>,
) {
    let params = derive_vvc_cclm_parameters_from_selection(
        chroma,
        geometry,
        node,
        chroma_sampling,
        bit_depth,
        chroma_availability,
        layout,
        luma_selection,
    );
    prediction.clear();
    prediction.resize(layout.width * layout.height, 0);
    let max_sample = i32::from(bit_depth.max_sample());
    for (dst, luma_sample) in prediction.iter_mut().zip(inner_luma.iter().copied()) {
        let predicted = right_shift_i32(params.a * luma_sample, params.shift) + params.b;
        *dst = predicted.clamp(0, max_sample) as VvcSample;
    }
}

fn prepare_vvc_cclm_inner_luma_into(
    inner_luma: &mut Vec<i32>,
    layout: VvcCclmLayout,
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    luma_availability: Option<VvcPlaneAvailability<'_>>,
) {
    inner_luma.clear();
    inner_luma.reserve(layout.width * layout.height);
    for y in 0..layout.height {
        for x in 0..layout.width {
            inner_luma.push(cclm_downsample_inner_luma(
                luma,
                geometry,
                node,
                chroma_sampling,
                bit_depth,
                luma_availability,
                x,
                y,
                layout.template.downsample_left_available,
            ));
        }
    }
}

fn vvc_cclm_layout(
    mode: VvcChromaCclmMode,
    chroma_availability: Option<VvcPlaneAvailability<'_>>,
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
) -> VvcCclmLayout {
    let subsample_x = chroma_subsample_x(chroma_sampling);
    let subsample_y = chroma_subsample_y(chroma_sampling);
    let chroma_width = usize::from(node.width) / subsample_x;
    let chroma_height = usize::from(node.height) / subsample_y;
    let chroma_x = usize::from(node.x) / subsample_x;
    let chroma_y = usize::from(node.y) / subsample_y;
    let plane_width = geometry.width / subsample_x;
    let plane_height = geometry.height / subsample_y;
    let template = vvc_cclm_template(
        mode,
        chroma_availability,
        plane_width,
        plane_height,
        chroma_x,
        chroma_y,
        chroma_width,
        chroma_height,
        chroma_sampling,
    );
    VvcCclmLayout {
        width: chroma_width,
        height: chroma_height,
        template,
    }
}

pub(in crate::vvc) fn predict_vvc_luma_bdpcm_block_into_with_availability(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcBdpcmMode,
    luma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    predict_vvc_bdpcm_block_into(
        prediction,
        scratch,
        mode,
        luma,
        geometry.width,
        geometry.height,
        usize::from(node.x),
        usize::from(node.y),
        usize::from(node.width),
        usize::from(node.height),
        bit_depth,
        availability,
    );
}

pub(in crate::vvc) fn residual_vvc_luma_bdpcm_block_into_with_availability(
    residuals: &mut Vec<i16>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcBdpcmMode,
    source_luma: &[VvcSample],
    reference_luma: &[VvcSample],
    source_geometry: VvcVideoGeometry,
    reference_geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    let start_x = usize::from(node.x);
    let start_y = usize::from(node.y);
    let width = usize::from(node.width);
    let height = usize::from(node.height);
    let max_source_x = source_geometry.width.saturating_sub(1);
    let max_source_y = source_geometry.height.saturating_sub(1);

    residuals.clear();
    residuals.resize(width * height, 0);
    match mode {
        VvcBdpcmMode::None => unreachable!("BDPCM residual requires an enabled direction"),
        VvcBdpcmMode::Horizontal => {
            left_references_into(
                &mut scratch.left[..height],
                reference_luma,
                reference_geometry.width,
                reference_geometry.height,
                start_x,
                start_y,
                height,
                bit_depth,
                0,
                availability,
            );
            for y in 0..height {
                let predictor = scratch.left[y];
                let dst = y * width;
                let src_y = (start_y + y).min(max_source_y);
                let src_row = src_y * source_geometry.width;
                for x in 0..width {
                    let src_x = (start_x + x).min(max_source_x);
                    residuals[dst + x] =
                        vvc_sample_delta_i16(source_luma[src_row + src_x], predictor);
                }
            }
        }
        VvcBdpcmMode::Vertical => {
            top_references_into(
                &mut scratch.top[..width],
                reference_luma,
                reference_geometry.width,
                reference_geometry.height,
                start_x,
                start_y,
                width,
                bit_depth,
                0,
                availability,
            );
            for y in 0..height {
                let dst = y * width;
                let src_y = (start_y + y).min(max_source_y);
                let src_row = src_y * source_geometry.width;
                for x in 0..width {
                    let src_x = (start_x + x).min(max_source_x);
                    residuals[dst + x] =
                        vvc_sample_delta_i16(source_luma[src_row + src_x], scratch.top[x]);
                }
            }
        }
    }
}

pub(in crate::vvc) fn predict_vvc_chroma_bdpcm_block_into_with_availability(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcBdpcmMode,
    chroma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    let subsample_x = chroma_subsample_x(chroma_sampling);
    let subsample_y = chroma_subsample_y(chroma_sampling);
    predict_vvc_bdpcm_block_into(
        prediction,
        scratch,
        mode,
        chroma,
        geometry.width / subsample_x,
        geometry.height / subsample_y,
        usize::from(node.x) / subsample_x,
        usize::from(node.y) / subsample_y,
        usize::from(node.width) / subsample_x,
        usize::from(node.height) / subsample_y,
        bit_depth,
        availability,
    );
}

pub(in crate::vvc) struct VvcDcPredictionScratch {
    top: [VvcSample; VVC_ANGULAR_REFERENCE_CAPACITY],
    left: [VvcSample; VVC_ANGULAR_REFERENCE_CAPACITY],
    top_work: [i32; VVC_CTU_SIZE],
    bottom_delta: [i32; VVC_CTU_SIZE],
    cclm_inner_luma: Vec<i32>,
}

fn predict_vvc_bdpcm_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcBdpcmMode,
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    debug_assert!(mode.is_enabled());
    prediction.clear();
    prediction.resize(width * height, 0);
    match mode {
        VvcBdpcmMode::None => unreachable!("BDPCM predictor requires an enabled direction"),
        VvcBdpcmMode::Horizontal => {
            left_references_into(
                &mut scratch.left[..height],
                plane,
                plane_width,
                plane_height,
                start_x,
                start_y,
                height,
                bit_depth,
                0,
                availability,
            );
            for y in 0..height {
                prediction[y * width..(y + 1) * width].fill(scratch.left[y]);
            }
        }
        VvcBdpcmMode::Vertical => {
            top_references_into(
                &mut scratch.top[..width],
                plane,
                plane_width,
                plane_height,
                start_x,
                start_y,
                width,
                bit_depth,
                0,
                availability,
            );
            for row in prediction.chunks_exact_mut(width) {
                row.copy_from_slice(&scratch.top[..width]);
            }
        }
    }
}

fn vvc_sample_delta_i16(sample: VvcSample, predicted: VvcSample) -> i16 {
    (i32::from(sample) - i32::from(predicted)).clamp(i32::from(i16::MIN), i32::from(i16::MAX))
        as i16
}

include!("prediction_cclm.rs");

fn predict_vvc_chroma_planar_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    chroma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    let subsample_x = chroma_subsample_x(chroma_sampling);
    let subsample_y = chroma_subsample_y(chroma_sampling);
    predict_vvc_planar_block_into(
        prediction,
        scratch,
        chroma,
        geometry.width / subsample_x,
        geometry.height / subsample_y,
        usize::from(node.x) / subsample_x,
        usize::from(node.y) / subsample_y,
        usize::from(node.width) / subsample_x,
        usize::from(node.height) / subsample_y,
        bit_depth,
        false,
        0,
        availability,
    );
}

fn predict_vvc_chroma_angular_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcIntraPredictionMode,
    chroma: &[VvcSample],
    geometry: VvcVideoGeometry,
    node: VvcCodingTreeNode,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    let subsample_x = chroma_subsample_x(chroma_sampling);
    let subsample_y = chroma_subsample_y(chroma_sampling);
    let mode_index = vvc_chroma_prediction_mode_index(mode, chroma_sampling);
    predict_vvc_angular_block_into(
        prediction,
        scratch,
        chroma,
        geometry.width / subsample_x,
        geometry.height / subsample_y,
        usize::from(node.x) / subsample_x,
        usize::from(node.y) / subsample_y,
        usize::from(node.width) / subsample_x,
        usize::from(node.height) / subsample_y,
        mode_index,
        bit_depth,
        false,
        0,
        availability,
    );
}

fn vvc_chroma_prediction_mode_index(
    mode: VvcIntraPredictionMode,
    chroma_sampling: ChromaSampling,
) -> u8 {
    let mode_index = mode.luma_mode_index();
    if chroma_sampling == ChromaSampling::Cs422 {
        VVC_CHROMA_422_INTRA_ANGLE_MAPPING_TABLE[usize::from(mode_index)]
    } else {
        mode_index
    }
}

impl Default for VvcDcPredictionScratch {
    fn default() -> Self {
        Self {
            top: [0; VVC_ANGULAR_REFERENCE_CAPACITY],
            left: [0; VVC_ANGULAR_REFERENCE_CAPACITY],
            top_work: [0; VVC_CTU_SIZE],
            bottom_delta: [0; VVC_CTU_SIZE],
            cclm_inner_luma: Vec::new(),
        }
    }
}

fn predict_vvc_dc_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    debug_assert!(width <= VVC_CTU_SIZE);
    debug_assert!(height <= VVC_CTU_SIZE);
    top_references_into(
        &mut scratch.top,
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        width,
        bit_depth,
        reference_line,
        availability,
    );
    left_references_into(
        &mut scratch.left,
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        height,
        bit_depth,
        reference_line,
        availability,
    );
    let top = &scratch.top[..width];
    let left = &scratch.left[..height];
    let dc = dc_prediction_value(top, left, width, height);
    prediction.clear();
    prediction.resize(width * height, dc);

    // VTM IntraPrediction::predIntraAng applies PDPC to DC mode when the
    // luma TU is at least MIN_TB_SIZEY in both dimensions and multiRefIdx is
    // zero.
    if reference_line == 0 && width >= 4 && height >= 4 {
        let scale = ((width.ilog2() as i32 - 2 + height.ilog2() as i32 - 2 + 2) >> 2) as u32;
        let max_sample = i32::from(bit_depth.max_sample());
        for y in 0..height {
            let wt = 32i32 >> ((y << 1) >> scale).min(31);
            let left_sample = i32::from(left[y]);
            for x in 0..width {
                let wl = 32i32 >> ((x << 1) >> scale).min(31);
                let top_sample = i32::from(top[x]);
                let val = i32::from(dc);
                prediction[y * width + x] = (val
                    + ((wl * (left_sample - val) + wt * (top_sample - val) + 32) >> 6))
                    .clamp(0, max_sample) as VvcSample;
            }
        }
    }
}

fn predict_vvc_planar_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    filter_luma_references: bool,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    debug_assert!(width <= VVC_CTU_SIZE);
    debug_assert!(height <= VVC_CTU_SIZE);
    top_references_into(
        &mut scratch.top,
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        width + 2,
        bit_depth,
        reference_line,
        availability,
    );
    left_references_into(
        &mut scratch.left,
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        height + 2,
        bit_depth,
        reference_line,
        availability,
    );
    if reference_line == 0 && filter_luma_references && width * height > 32 {
        let top_left = top_left_reference(
            plane,
            plane_width,
            plane_height,
            start_x,
            start_y,
            bit_depth,
            0,
            availability,
        );
        filter_vvc_planar_references_in_place(
            &mut scratch.top,
            &mut scratch.left,
            top_left,
            width,
            height,
        );
    }
    let log2_w = width.ilog2();
    let log2_h = height.ilog2();
    let offset = 1i32 << (log2_w + log2_h);
    let final_shift = 1 + log2_w + log2_h;
    let bottom_left = i32::from(scratch.left[height]);
    let top_right = i32::from(scratch.top[width]);
    let max_sample = i32::from(bit_depth.max_sample());

    for x in 0..width {
        let top = i32::from(scratch.top[x]);
        scratch.bottom_delta[x] = bottom_left - top;
        scratch.top_work[x] = top << log2_h;
    }

    prediction.clear();
    prediction.resize(width * height, 0);
    for y in 0..height {
        let left = i32::from(scratch.left[y]);
        let right_delta = top_right - left;
        let mut hor_pred = left << log2_w;
        for x in 0..width {
            hor_pred += right_delta;
            scratch.top_work[x] += scratch.bottom_delta[x];
            let vert_pred = scratch.top_work[x];
            let sample = ((hor_pred << log2_h) + (vert_pred << log2_w) + offset) >> final_shift;
            debug_assert!((0..=max_sample).contains(&sample));
            prediction[y * width + x] = sample as VvcSample;
        }
    }

    if reference_line == 0 && width >= 4 && height >= 4 {
        apply_vvc_planar_dc_pdpc(
            prediction,
            &scratch.top[..width],
            &scratch.left[..height],
            width,
            height,
            bit_depth,
        );
    }
}

fn predict_vvc_angular_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    plane: &[VvcSample],
    plane_width: usize,
    plane_height: usize,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    mode_index: u8,
    bit_depth: SampleBitDepth,
    is_luma: bool,
    reference_line: usize,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    debug_assert!(width <= VVC_CTU_SIZE);
    debug_assert!(height <= VVC_CTU_SIZE);
    debug_assert!((2..=66).contains(&mode_index));
    let reference_len = ((width.max(height)) << 1) + 4;
    top_references_into(
        &mut scratch.top,
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        reference_len,
        bit_depth,
        reference_line,
        availability,
    );
    left_references_into(
        &mut scratch.left,
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        reference_len,
        bit_depth,
        reference_line,
        availability,
    );
    let mut params = vvc_angular_prediction_params(width, height, mode_index, is_luma);
    if is_luma && reference_line != 0 {
        params.interpolation = VvcAngularInterpolation::FourTapDct;
        params.filter_luma_references = false;
    }
    if params.angle == 0 && reference_line == 0 {
        let top_left = top_left_reference(
            plane,
            plane_width,
            plane_height,
            start_x,
            start_y,
            bit_depth,
            reference_line,
            availability,
        );
        predict_vvc_zero_angle_angular_block_into(
            prediction,
            scratch,
            width,
            height,
            params.is_vertical,
            params.abs_inv_angle,
            params.pdpc_scale,
            top_left,
            bit_depth,
        );
        return;
    }
    let mut top_left = angular_main_zero_reference(
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        bit_depth,
        reference_line,
        params.is_vertical,
        availability,
    );
    let mut angular_refs = angular_shifted_reference_samples(
        plane,
        plane_width,
        plane_height,
        start_x,
        start_y,
        bit_depth,
        reference_line,
        params.is_vertical,
        top_left,
        availability,
    );
    if reference_line == 0 && params.filter_luma_references {
        top_left = filter_vvc_angular_references_in_place(
            &mut scratch.top,
            &mut scratch.left,
            top_left,
            width << 1,
            height << 1,
        );
        angular_refs.main_zero = top_left;
        angular_refs.side_zero = top_left;
    }
    if params.angle >= 0 {
        if params.is_vertical {
            replicate_vvc_positive_angular_main_extension(&mut scratch.top, width << 1);
        } else {
            replicate_vvc_positive_angular_main_extension(&mut scratch.left, height << 1);
        }
    }

    prediction.clear();
    prediction.resize(width * height, 0);
    if params.is_vertical {
        predict_vvc_vertical_oriented_angular_block(
            prediction,
            &scratch.top[..reference_len],
            &scratch.left[..reference_len],
            top_left,
            width,
            height,
            params.angle,
            params.abs_inv_angle,
            params.interpolation,
            reference_line,
            angular_refs,
            (reference_line == 0).then_some(params.pdpc_scale).flatten(),
            bit_depth,
        );
    } else {
        predict_vvc_horizontal_oriented_angular_block(
            prediction,
            &scratch.left[..reference_len],
            &scratch.top[..reference_len],
            top_left,
            width,
            height,
            params.angle,
            params.abs_inv_angle,
            params.interpolation,
            reference_line,
            angular_refs,
            (reference_line == 0).then_some(params.pdpc_scale).flatten(),
            bit_depth,
        );
    }
}

fn predict_vvc_zero_angle_angular_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &VvcDcPredictionScratch,
    width: usize,
    height: usize,
    is_vertical: bool,
    abs_inv_angle: i32,
    pdpc_scale: Option<u32>,
    top_left: VvcSample,
    bit_depth: SampleBitDepth,
) {
    prediction.clear();
    prediction.resize(width * height, 0);
    if is_vertical {
        for y in 0..height {
            let row = &mut prediction[y * width..(y + 1) * width];
            row.copy_from_slice(&scratch.top[..width]);
            apply_vvc_angular_pdpc_to_vertical_row(
                row,
                &scratch.left,
                top_left,
                y,
                width,
                0,
                abs_inv_angle,
                pdpc_scale,
                bit_depth,
            );
        }
    } else {
        for y in 0..height {
            prediction[y * width..(y + 1) * width].fill(scratch.left[y]);
        }
        for x in 0..width {
            apply_vvc_angular_pdpc_to_horizontal_column(
                prediction,
                &scratch.top,
                top_left,
                x,
                width,
                height,
                0,
                abs_inv_angle,
                pdpc_scale,
                bit_depth,
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VvcAngularInterpolation {
    Linear,
    FourTapDct,
    FourTapSmoothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcShiftedAngularReferences {
    main_zero: VvcSample,
    side_zero: VvcSample,
    main_prefix: [VvcSample; VVC_MAX_MULTI_REF_LINE_IDX],
    main_prefix_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcAngularPredictionParams {
    is_vertical: bool,
    angle: i32,
    abs_inv_angle: i32,
    interpolation: VvcAngularInterpolation,
    pdpc_scale: Option<u32>,
    filter_luma_references: bool,
}

fn vvc_angular_prediction_params(
    width: usize,
    height: usize,
    mode_index: u8,
    is_luma: bool,
) -> VvcAngularPredictionParams {
    let pred_mode = vvc_modified_wide_angle(width, height, mode_index);
    let is_vertical = pred_mode >= i16::from(VVC_LUMA_MODE_DIAGONAL);
    let intra_pred_angle_mode = if is_vertical {
        pred_mode - i16::from(VVC_LUMA_MODE_VERTICAL)
    } else {
        -(pred_mode - i16::from(VVC_LUMA_MODE_HORIZONTAL))
    };
    let abs_ang_mode = intra_pred_angle_mode.unsigned_abs() as usize;
    let abs_angle = VVC_INTRA_ANG_TABLE[abs_ang_mode];
    let angle = if intra_pred_angle_mode < 0 {
        -abs_angle
    } else {
        abs_angle
    };
    let abs_inv_angle = VVC_INTRA_INV_ANG_TABLE[abs_ang_mode];
    let pdpc_scale = vvc_angular_pdpc_scale(width, height, angle, abs_inv_angle, is_vertical);
    let (interpolation, filter_luma_references) = if is_luma {
        vvc_luma_angular_filter_params(width, height, pred_mode, abs_angle)
    } else {
        (VvcAngularInterpolation::Linear, false)
    };

    VvcAngularPredictionParams {
        is_vertical,
        angle,
        abs_inv_angle,
        interpolation,
        pdpc_scale,
        filter_luma_references,
    }
}

fn vvc_modified_wide_angle(width: usize, height: usize, mode_index: u8) -> i16 {
    let mut pred_mode = i16::from(mode_index);
    if (2..=66).contains(&mode_index) {
        const MODE_SHIFT: [i16; 6] = [0, 6, 10, 12, 14, 15];
        let delta_size = width.ilog2().abs_diff(height.ilog2()) as usize;
        let shift = MODE_SHIFT[delta_size.min(MODE_SHIFT.len() - 1)];
        if width > height && pred_mode < 2 + shift {
            pred_mode += 65;
        } else if height > width && pred_mode > 66 - shift {
            pred_mode -= 65;
        }
    }
    pred_mode
}

fn vvc_luma_angular_filter_params(
    width: usize,
    height: usize,
    pred_mode: i16,
    abs_angle: i32,
) -> (VvcAngularInterpolation, bool) {
    let diff = (pred_mode - i16::from(VVC_LUMA_MODE_HORIZONTAL))
        .abs()
        .min((pred_mode - i16::from(VVC_LUMA_MODE_VERTICAL)).abs());
    let log2_size = ((width.ilog2() + height.ilog2()) >> 1) as usize;
    let threshold = i16::from(
        VVC_INTRA_REFERENCE_FILTER_THRESHOLD
            [log2_size.min(VVC_INTRA_REFERENCE_FILTER_THRESHOLD.len() - 1)],
    );
    if diff <= threshold {
        return (VvcAngularInterpolation::FourTapDct, false);
    }
    if vvc_angular_integer_slope(abs_angle) {
        (VvcAngularInterpolation::FourTapDct, true)
    } else {
        (VvcAngularInterpolation::FourTapSmoothing, false)
    }
}

fn predict_vvc_vertical_oriented_angular_block(
    prediction: &mut [VvcSample],
    main: &[VvcSample],
    side: &[VvcSample],
    top_left: VvcSample,
    width: usize,
    height: usize,
    angle: i32,
    abs_inv_angle: i32,
    interpolation: VvcAngularInterpolation,
    reference_line: usize,
    shifted_refs: VvcShiftedAngularReferences,
    pdpc_scale: Option<u32>,
    bit_depth: SampleBitDepth,
) {
    for y in 0..height {
        let delta_pos = angle * (y as i32 + 1 + reference_line as i32);
        let delta_int = delta_pos >> 5;
        let delta_fract = delta_pos & 31;
        for x in 0..width {
            prediction[y * width + x] = angular_reference_prediction(
                main,
                side,
                top_left,
                delta_int + x as i32 + 1,
                delta_fract,
                abs_inv_angle,
                height,
                reference_line,
                shifted_refs,
                interpolation,
                bit_depth,
            );
        }
        apply_vvc_angular_pdpc_to_vertical_row(
            &mut prediction[y * width..(y + 1) * width],
            side,
            top_left,
            y,
            width,
            angle,
            abs_inv_angle,
            pdpc_scale,
            bit_depth,
        );
    }
}

fn predict_vvc_horizontal_oriented_angular_block(
    prediction: &mut [VvcSample],
    main: &[VvcSample],
    side: &[VvcSample],
    top_left: VvcSample,
    width: usize,
    height: usize,
    angle: i32,
    abs_inv_angle: i32,
    interpolation: VvcAngularInterpolation,
    reference_line: usize,
    shifted_refs: VvcShiftedAngularReferences,
    pdpc_scale: Option<u32>,
    bit_depth: SampleBitDepth,
) {
    for x in 0..width {
        let delta_pos = angle * (x as i32 + 1 + reference_line as i32);
        let delta_int = delta_pos >> 5;
        let delta_fract = delta_pos & 31;
        for y in 0..height {
            prediction[y * width + x] = angular_reference_prediction(
                main,
                side,
                top_left,
                delta_int + y as i32 + 1,
                delta_fract,
                abs_inv_angle,
                width,
                reference_line,
                shifted_refs,
                interpolation,
                bit_depth,
            );
        }
        apply_vvc_angular_pdpc_to_horizontal_column(
            prediction,
            side,
            top_left,
            x,
            width,
            height,
            angle,
            abs_inv_angle,
            pdpc_scale,
            bit_depth,
        );
    }
}

fn angular_reference_prediction(
    main: &[VvcSample],
    side: &[VvcSample],
    top_left: VvcSample,
    reference_index: i32,
    fract: i32,
    abs_inv_angle: i32,
    side_size: usize,
    reference_line: usize,
    shifted_refs: VvcShiftedAngularReferences,
    interpolation: VvcAngularInterpolation,
    bit_depth: SampleBitDepth,
) -> VvcSample {
    if fract == 0 {
        return angular_reference_sample(
            main,
            side,
            top_left,
            reference_index,
            abs_inv_angle,
            side_size,
            reference_line,
            shifted_refs,
        );
    }
    match interpolation {
        VvcAngularInterpolation::Linear => {
            let a = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            let b = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index + 1,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            (a + ((fract * (b - a) + 16) >> 5)) as VvcSample
        }
        VvcAngularInterpolation::FourTapDct | VvcAngularInterpolation::FourTapSmoothing => {
            let filter = match interpolation {
                VvcAngularInterpolation::FourTapDct => {
                    VVC_CHROMA_4TAP_INTERPOLATION_FILTER[fract as usize]
                }
                VvcAngularInterpolation::FourTapSmoothing => [
                    16 - (fract >> 1),
                    32 - (fract >> 1),
                    16 + (fract >> 1),
                    fract >> 1,
                ],
                VvcAngularInterpolation::Linear => unreachable!(),
            };
            let p0 = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index - 1,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            let p1 = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            let p2 = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index + 1,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            let p3 = i32::from(angular_reference_sample(
                main,
                side,
                top_left,
                reference_index + 2,
                abs_inv_angle,
                side_size,
                reference_line,
                shifted_refs,
            ));
            ((filter[0] * p0 + filter[1] * p1 + filter[2] * p2 + filter[3] * p3 + 32) >> 6)
                .clamp(0, i32::from(bit_depth.max_sample())) as VvcSample
        }
    }
}

fn angular_reference_sample(
    main: &[VvcSample],
    side: &[VvcSample],
    _top_left: VvcSample,
    index: i32,
    abs_inv_angle: i32,
    side_size: usize,
    reference_line: usize,
    shifted_refs: VvcShiftedAngularReferences,
) -> VvcSample {
    if index == 0 {
        return shifted_refs.main_zero;
    }
    if index > 0 {
        return main[(index as usize - 1).min(main.len().saturating_sub(1))];
    }
    if index >= -(shifted_refs.main_prefix_len as i32) {
        let prefix_idx = (index + shifted_refs.main_prefix_len as i32) as usize;
        return shifted_refs.main_prefix[prefix_idx];
    }

    let old_main_index = index + reference_line as i32;
    let old_side_index =
        (((-old_main_index * abs_inv_angle + 256) >> 9).max(0) as usize).min(side_size);
    match old_side_index.checked_sub(reference_line) {
        Some(0) => shifted_refs.side_zero,
        Some(new_side_index) => side[(new_side_index - 1).min(side.len().saturating_sub(1))],
        None => shifted_refs.side_zero,
    }
}

fn vvc_angular_integer_slope(abs_angle: i32) -> bool {
    (abs_angle & 0x1f) == 0
}

fn angular_side_reference_sample(side: &[VvcSample], index: usize) -> VvcSample {
    if index == 0 {
        return side[0];
    }
    side[(index - 1).min(side.len().saturating_sub(1))]
}

fn replicate_vvc_positive_angular_main_extension(main: &mut [VvcSample], reference_len: usize) {
    if reference_len == 0 || reference_len > main.len() {
        return;
    }
    let edge = main[reference_len - 1];
    for sample in &mut main[reference_len..] {
        *sample = edge;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vvc::{VvcPartSplit, VvcTreeType};

    #[test]
    fn mdlm_top_preserves_actual_left_availability_for_downsampling_padding() {
        let geometry = VvcVideoGeometry {
            width: 16,
            height: 24,
        };
        let node = VvcCodingTreeNode {
            x: 8,
            y: 16,
            width: 8,
            height: 8,
            cqt_depth: 3,
            mtt_depth: 0,
            depth_offset: 0,
            part_idx: 1,
            parent_split: VvcPartSplit::Quad,
            tree_type: VvcTreeType::DualTreeChroma,
            split_history: [VvcPartSplit::Quad; 2],
        };
        let chroma_width = 8;
        let chroma_height = 12;
        let chroma_availability = vec![true; chroma_width * chroma_height];
        let template = vvc_cclm_template(
            VvcChromaCclmMode::MdlmTop,
            Some(VvcPlaneAvailability::new(
                &chroma_availability,
                chroma_width,
            )),
            chroma_width,
            chroma_height,
            4,
            8,
            4,
            4,
            ChromaSampling::Cs420,
        );
        assert!(!template.left_available);
        assert!(template.downsample_left_available);

        let mut luma = vec![0; geometry.width * geometry.height];
        for y in 0..geometry.height {
            luma[y * geometry.width + 7] = 0;
            luma[y * geometry.width + 8] = 80;
            luma[y * geometry.width + 9] = 160;
        }
        let luma_availability = vec![true; luma.len()];
        let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
        let actual = cclm_downsample_inner_luma(
            &luma,
            geometry,
            node,
            ChromaSampling::Cs420,
            bit_depth,
            Some(VvcPlaneAvailability::new(
                &luma_availability,
                geometry.width,
            )),
            0,
            0,
            template.downsample_left_available,
        );
        let incorrectly_padded = cclm_downsample_inner_luma(
            &luma,
            geometry,
            node,
            ChromaSampling::Cs420,
            bit_depth,
            Some(VvcPlaneAvailability::new(
                &luma_availability,
                geometry.width,
            )),
            0,
            0,
            template.left_available,
        );

        assert_eq!(actual, 80);
        assert_eq!(incorrectly_padded, 100);
    }
}
