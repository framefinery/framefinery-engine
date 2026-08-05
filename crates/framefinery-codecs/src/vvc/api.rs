#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VvcProfileTarget {
    MinimalVvcAllIntra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VvcSubset {
    pub all_intra: bool,
    pub single_picture: bool,
    pub one_tile: bool,
    pub one_slice: bool,
}

impl Default for VvcSubset {
    fn default() -> Self {
        Self {
            all_intra: true,
            single_picture: true,
            one_tile: true,
            one_slice: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VvcEncodeParams {
    pub frames: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcEncodeRequest {
    params: VvcEncodeParams,
    geometry: VvcVideoGeometry,
    limits: VvcVideoLimits,
    format: PixelFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VvcValidatedEncodeRequest {
    frame_limit: FrameLimit,
    geometry: VvcVideoGeometry,
    format: VvcPictureFormat,
}

impl VvcEncodeRequest {
    fn validate(self) -> Result<VvcValidatedEncodeRequest, String> {
        self.geometry.validate_against(self.limits)?;
        let frame_limit = FrameLimit::from_frame_count(self.params.frames);
        let format = Picture::validate_format_shape(
            self.geometry.width,
            self.geometry.height,
            self.format,
            validate_vvc_input_format,
        )?;
        Ok(VvcValidatedEncodeRequest {
            frame_limit,
            geometry: self.geometry,
            format,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VvcFastSearch {
    Off,
    Conservative,
    Moderate,
    Aggressive,
    #[default]
    LosslessSpeed,
}

impl VvcFastSearch {
    pub const VALUES: &'static [&'static str] =
        &["off", "conservative", "moderate", "aggressive", "lossless-speed"];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Conservative => "conservative",
            Self::Moderate => "moderate",
            Self::Aggressive => "aggressive",
            Self::LosslessSpeed => "lossless-speed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" => Some(Self::Off),
            "conservative" => Some(Self::Conservative),
            "moderate" => Some(Self::Moderate),
            "aggressive" => Some(Self::Aggressive),
            "lossless-speed" => Some(Self::LosslessSpeed),
            _ => None,
        }
    }
}

impl std::str::FromStr for VvcFastSearch {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or_else(|| {
            format!(
                "VVC fast-search expects one of {}, got '{value}'",
                Self::VALUES.join("|")
            )
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VvcEncodeOptions {
    pub lossless: bool,
    pub qp: Option<u8>,
    pub predictive: bool,
    pub fast_search: VvcFastSearch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VvcEncodeArtifacts {
    pub bitstream: Vec<u8>,
    pub reconstruction: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VvcEncodeProgress {
    pub frame_idx: usize,
    pub frame_count: Option<usize>,
}

pub struct VvcEncodeFrameMetrics<'a> {
    pub frame_idx: usize,
    pub frame_count: Option<usize>,
    pub bitstream_bytes: usize,
    pub source: &'a [u8],
    pub reconstruction: &'a [u8],
}
