use crate::picture::{ChromaSampling, SampleBitDepth};

use super::sample_math::vvc_sample_delta_i16;

use super::super::{
    chroma_subsample_x, chroma_subsample_y, vvc_neutral_sample, VvcBdpcmMode, VvcChromaCclmMode,
    VvcChromaIntraPredictionMode, VvcCodingTreeNode, VvcIntraPredictionMode, VvcSample,
    VvcVideoGeometry, VVC_CTU_SIZE,
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
include!("prediction_dc_planar.rs");
include!("prediction_angular_params.rs");
include!("prediction_angular_references.rs");
include!("prediction_angular_oriented.rs");
include!("prediction_angular_sampling.rs");
include!("prediction_angular_orchestration.rs");
include!("prediction_reconstruction.rs");
include!("prediction_bdpcm.rs");

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
    let region = VvcIntraPredictionRegion::luma(geometry, node, reference_line);
    predict_vvc_intra_block_into(
        prediction,
        scratch,
        mode,
        mode.luma_mode_index(),
        luma,
        region,
        bit_depth,
        availability,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VvcIntraPredictionRegion {
    plane_width: usize,
    plane_height: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    is_luma: bool,
    reference_line: usize,
}

impl VvcIntraPredictionRegion {
    fn luma(geometry: VvcVideoGeometry, node: VvcCodingTreeNode, reference_line: usize) -> Self {
        Self {
            plane_width: geometry.width,
            plane_height: geometry.height,
            x: usize::from(node.x),
            y: usize::from(node.y),
            width: usize::from(node.width),
            height: usize::from(node.height),
            is_luma: true,
            reference_line,
        }
    }

    fn chroma(
        geometry: VvcVideoGeometry,
        node: VvcCodingTreeNode,
        chroma_sampling: ChromaSampling,
    ) -> Self {
        let subsample_x = chroma_subsample_x(chroma_sampling);
        let subsample_y = chroma_subsample_y(chroma_sampling);
        Self {
            plane_width: geometry.width / subsample_x,
            plane_height: geometry.height / subsample_y,
            x: usize::from(node.x) / subsample_x,
            y: usize::from(node.y) / subsample_y,
            width: usize::from(node.width) / subsample_x,
            height: usize::from(node.height) / subsample_y,
            is_luma: false,
            reference_line: 0,
        }
    }
}

fn predict_vvc_intra_block_into(
    prediction: &mut Vec<VvcSample>,
    scratch: &mut VvcDcPredictionScratch,
    mode: VvcIntraPredictionMode,
    mode_index: u8,
    plane: &[VvcSample],
    region: VvcIntraPredictionRegion,
    bit_depth: SampleBitDepth,
    availability: Option<VvcPlaneAvailability<'_>>,
) {
    match mode {
        VvcIntraPredictionMode::Planar => predict_vvc_planar_block_into(
            prediction,
            scratch,
            plane,
            region.plane_width,
            region.plane_height,
            region.x,
            region.y,
            region.width,
            region.height,
            bit_depth,
            region.is_luma,
            region.reference_line,
            availability,
        ),
        VvcIntraPredictionMode::Dc => predict_vvc_dc_block_into(
            prediction,
            scratch,
            plane,
            region.plane_width,
            region.plane_height,
            region.x,
            region.y,
            region.width,
            region.height,
            bit_depth,
            region.reference_line,
            availability,
        ),
        VvcIntraPredictionMode::Horizontal
        | VvcIntraPredictionMode::Vertical
        | VvcIntraPredictionMode::Angular(_) => predict_vvc_angular_block_into(
            prediction,
            scratch,
            plane,
            region.plane_width,
            region.plane_height,
            region.x,
            region.y,
            region.width,
            region.height,
            mode_index,
            bit_depth,
            region.is_luma,
            region.reference_line,
            availability,
        ),
    }
}

fn predict_vvc_chroma_intra_block_into_with_availability(
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
    let region = VvcIntraPredictionRegion::chroma(geometry, node, chroma_sampling);
    let mode_index = vvc_chroma_prediction_mode_index(mode, chroma_sampling);
    predict_vvc_intra_block_into(
        prediction,
        scratch,
        mode,
        mode_index,
        chroma,
        region,
        bit_depth,
        availability,
    );
}

#[derive(Default)]
struct VvcCclmPredictionScratch {
    inner_luma: Vec<i32>,
}

pub(in crate::vvc) struct VvcDcPredictionScratch {
    top: [VvcSample; VVC_ANGULAR_REFERENCE_CAPACITY],
    left: [VvcSample; VVC_ANGULAR_REFERENCE_CAPACITY],
    top_work: [i32; VVC_CTU_SIZE],
    bottom_delta: [i32; VVC_CTU_SIZE],
    cclm: VvcCclmPredictionScratch,
}

include!("prediction_cclm.rs");
include!("prediction_cclm_sampling.rs");
include!("prediction_cclm_orchestration.rs");
include!("prediction_chroma_mode.rs");

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
            cclm: VvcCclmPredictionScratch::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vvc::{VvcPartSplit, VvcTreeType};

    #[test]
    fn intra_prediction_regions_share_plane_coordinate_mapping() {
        let geometry = VvcVideoGeometry {
            width: 24,
            height: 24,
        };
        let node = VvcCodingTreeNode {
            x: 8,
            y: 8,
            width: 8,
            height: 8,
            cqt_depth: 2,
            mtt_depth: 0,
            depth_offset: 0,
            part_idx: 0,
            parent_split: VvcPartSplit::Quad,
            tree_type: VvcTreeType::SingleTree,
            split_history: [VvcPartSplit::Quad; 2],
        };

        assert_eq!(
            VvcIntraPredictionRegion::luma(geometry, node, 2),
            VvcIntraPredictionRegion {
                plane_width: 24,
                plane_height: 24,
                x: 8,
                y: 8,
                width: 8,
                height: 8,
                is_luma: true,
                reference_line: 2,
            }
        );
        assert_eq!(
            VvcIntraPredictionRegion::chroma(geometry, node, ChromaSampling::Cs444),
            VvcIntraPredictionRegion {
                plane_width: 24,
                plane_height: 24,
                x: 8,
                y: 8,
                width: 8,
                height: 8,
                is_luma: false,
                reference_line: 0,
            }
        );
        assert_eq!(
            VvcIntraPredictionRegion::chroma(geometry, node, ChromaSampling::Cs422),
            VvcIntraPredictionRegion {
                plane_width: 12,
                plane_height: 24,
                x: 4,
                y: 8,
                width: 4,
                height: 8,
                is_luma: false,
                reference_line: 0,
            }
        );
        assert_eq!(
            VvcIntraPredictionRegion::chroma(geometry, node, ChromaSampling::Cs420),
            VvcIntraPredictionRegion {
                plane_width: 12,
                plane_height: 12,
                x: 4,
                y: 4,
                width: 4,
                height: 4,
                is_luma: false,
                reference_line: 0,
            }
        );
    }

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

    #[test]
    fn cclm_single_block_reuses_owned_inner_luma_scratch() {
        let geometry = VvcVideoGeometry {
            width: 8,
            height: 8,
        };
        let node = VvcCodingTreeNode {
            x: 0,
            y: 0,
            width: 8,
            height: 8,
            cqt_depth: 0,
            mtt_depth: 0,
            depth_offset: 0,
            part_idx: 0,
            parent_split: VvcPartSplit::None,
            tree_type: VvcTreeType::SingleTree,
            split_history: [VvcPartSplit::None; 2],
        };
        let bit_depth = SampleBitDepth::new(8).expect("valid bit depth");
        let luma = vec![64; geometry.width * geometry.height];
        let chroma = vec![128; geometry.width * geometry.height];
        let mut prediction = Vec::new();
        let mut scratch = VvcDcPredictionScratch::default();

        predict_vvc_chroma_mode_block_into_with_availability(
            &mut prediction,
            &mut scratch,
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
            VvcIntraPredictionMode::Dc,
            &chroma,
            &luma,
            geometry,
            node,
            ChromaSampling::Cs444,
            bit_depth,
            None,
            None,
        );
        let first_prediction = prediction.clone();
        let retained_capacity = scratch.cclm.inner_luma.capacity();
        assert_eq!(scratch.cclm.inner_luma.len(), prediction.len());

        predict_vvc_chroma_mode_block_into_with_availability(
            &mut prediction,
            &mut scratch,
            VvcChromaIntraPredictionMode::Cclm(VvcChromaCclmMode::Linear),
            VvcIntraPredictionMode::Dc,
            &chroma,
            &luma,
            geometry,
            node,
            ChromaSampling::Cs444,
            bit_depth,
            None,
            None,
        );
        assert_eq!(prediction, first_prediction);
        assert_eq!(scratch.cclm.inner_luma.capacity(), retained_capacity);
    }
}
