/// Luma coded-picture dimensions are rounded to this granularity before SPS/PPS
/// signaling and crop-offset derivation.
///
/// The visible picture dimensions remain the caller-provided size. The padded
/// coded canvas is an internal encoder representation for the current VVC
/// residual path and is removed with conformance-window crop signaling.
pub const VVC_CODED_DIMENSION_GRANULARITY: usize = 8;
const VVC_CTU_SIZE: usize = 64;
const VVC_CURRENT_MIN_LUMA_CB_SIZE: u16 = 4;
const VVC_CURRENT_MAX_LUMA_LEAF_SIZE: u16 = 8;
const VVC_LOSSLESS_LUMA_LEAF_SIZE: u16 = 4;
const VVC_CURRENT_MAX_LUMA_BT_SIZE: u16 = VVC_CURRENT_MIN_LUMA_QT_SIZE << 2;
const VVC_CURRENT_MAX_LUMA_TT_SIZE: u16 = VVC_CURRENT_MIN_LUMA_QT_SIZE << 2;
const VVC_CURRENT_MAX_LUMA_MTT_DEPTH: u8 = 5;
const VVC_CURRENT_ENCODER_CHROMA_420_TB_SIZE: u16 = 4;
const VVC_CURRENT_MAX_CHROMA_420_BT_SIZE: u16 = VVC_CURRENT_MIN_CHROMA_420_QT_SIZE << 3;
const VVC_CURRENT_MAX_CHROMA_420_TT_SIZE: u16 = VVC_CURRENT_MIN_CHROMA_420_QT_SIZE << 2;
const VVC_CURRENT_MAX_CHROMA_420_MTT_DEPTH: u8 = 3;
const VVC_CURRENT_MIN_CHROMA_420_QT_SIZE: u16 = VVC_CURRENT_MIN_LUMA_QT_SIZE;
const VVC_CURRENT_MIN_LUMA_QT_SIZE: u16 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VvcVideoGeometry {
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcCodedGeometry {
    width: usize,
    height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VvcVideoLimits {
    pub max_width: usize,
    pub max_height: usize,
}

impl VvcVideoLimits {
    pub const fn max_64x64() -> Self {
        Self {
            max_width: 64,
            max_height: 64,
        }
    }

    pub const fn unbounded() -> Self {
        Self {
            max_width: usize::MAX,
            max_height: usize::MAX,
        }
    }
}

impl VvcVideoGeometry {
    pub const fn validation_minimum() -> Self {
        Self {
            width: 4,
            height: 4,
        }
    }

    pub fn validate_against(self, limits: VvcVideoLimits) -> Result<(), String> {
        self.validate_shape()?;
        if self.width > limits.max_width || self.height > limits.max_height {
            return Err(format!(
                "VVC geometry supports at most {}x{} visible pictures at this entry point; got {}x{}",
                limits.max_width, limits.max_height, self.width, self.height
            ));
        }
        Ok(())
    }

    fn validate_against_format(
        self,
        limits: VvcVideoLimits,
        format: VvcPictureFormat,
    ) -> Result<(), String> {
        self.validate_against(limits)?;
        let crop_width = self.coded_width() - self.width;
        let crop_height = self.coded_height() - self.height;
        let crop_unit_x = chroma_subsample_x(format.chroma_sampling);
        let crop_unit_y = chroma_subsample_y(format.chroma_sampling);
        if !crop_width.is_multiple_of(crop_unit_x) || !crop_height.is_multiple_of(crop_unit_y) {
            return Err(format!(
                "VVC {:?} visible geometry {}x{} cannot be represented by the current {}x{} coded canvas crop units",
                format.chroma_sampling,
                self.width,
                self.height,
                crop_unit_x,
                crop_unit_y
            ));
        }
        Ok(())
    }

    fn validate_shape(self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 {
            return Err("VVC geometry expects non-zero width and height".to_string());
        }
        Ok(())
    }

    fn luma_samples(self) -> usize {
        self.width * self.height
    }

    fn coded_width(self) -> usize {
        self.coded().width
    }

    fn coded_height(self) -> usize {
        self.coded().height
    }

    fn coded(self) -> VvcCodedGeometry {
        VvcCodedGeometry {
            width: coded_canvas_dimension(self.width),
            height: coded_canvas_dimension(self.height),
        }
    }

    fn crop_right(self, chroma_sampling: ChromaSampling) -> u32 {
        ((self.coded_width() - self.width) / chroma_subsample_x(chroma_sampling)) as u32
    }

    fn crop_bottom(self, chroma_sampling: ChromaSampling) -> u32 {
        ((self.coded_height() - self.height) / chroma_subsample_y(chroma_sampling)) as u32
    }
}

pub(in crate::vvc) fn chroma_subsample_x(chroma_sampling: ChromaSampling) -> usize {
    planar_chroma_subsample_x(chroma_sampling)
}

pub(in crate::vvc) fn chroma_subsample_y(chroma_sampling: ChromaSampling) -> usize {
    planar_chroma_subsample_y(chroma_sampling)
}

fn coded_canvas_dimension(value: usize) -> usize {
    value.div_ceil(VVC_CODED_DIMENSION_GRANULARITY) * VVC_CODED_DIMENSION_GRANULARITY
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VvcSampledColor {
    pub y: VvcSample,
    pub u: VvcSample,
    pub v: VvcSample,
}

pub(in crate::vvc) type VvcSample = u16;
pub(in crate::vvc) const VVC_MIN_BIT_DEPTH: u8 = 8;
pub(in crate::vvc) const VVC_MAX_BIT_DEPTH: u8 = 12;
const VVC_PALETTE_DEFAULT_SLICE_QP: i32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
struct VvcSampledFrame {
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
    luma: Vec<VvcSample>,
    cb: Vec<VvcSample>,
    cr: Vec<VvcSample>,
    chroma_len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcCtuRegion {
    slice_address: usize,
    origin_x: usize,
    origin_y: usize,
    geometry: VvcVideoGeometry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::vvc) struct VvcQuantizedCtu {
    slice_address: usize,
    geometry: VvcVideoGeometry,
    payload: VvcQuantizedCtuPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::vvc) enum VvcQuantizedCtuPayload {
    Intra(Box<VvcCtuPartitionParams>),
    InterSkip,
}

impl VvcQuantizedCtuPayload {
    pub(in crate::vvc) fn is_inter_coded(&self) -> bool {
        match self {
            Self::InterSkip => true,
            Self::Intra(params) => params
                .luma_tu_inter_decisions
                .iter()
                .take(params.luma_tu_count)
                .any(Option::is_some),
        }
    }
}
