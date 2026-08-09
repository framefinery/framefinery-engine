/// VVC profile selection for the experimental VVC encoder.
///
/// `Auto` is the default and selects the lowest 4:4:4-capable profile for the
/// input bit depth. That keeps screen-content tools such as palette legal for
/// the default encode path. Concrete lower profiles can be selected when a
/// caller wants tighter conformance; unsupported tools are then gated out
/// before block-level mode decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VvcProfile {
    /// Choose the lowest 4:4:4-capable profile that supports the input bit depth.
    #[default]
    Auto,
    /// VVC Main 10 profile: up to 10-bit 4:2:0.
    Main10,
    /// VVC Main 10 4:4:4 profile: up to 10-bit 4:4:4.
    Main10FourFourFour,
    /// VVC Main 12 profile: up to 12-bit 4:2:0.
    Main12,
    /// VVC Main 12 4:4:4 profile: up to 12-bit 4:4:4.
    Main12FourFourFour,
}

impl VvcProfile {
    /// User-facing setting values accepted by `--set profile=<value>`.
    pub const VALUES: &'static [&'static str] = &[
        "auto",
        "main-10-444",
        "main-12-444",
        "main-10",
        "main-12",
    ];

    /// Return the CLI/API setting spelling for this profile.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Main10 => "main-10",
            Self::Main10FourFourFour => "main-10-444",
            Self::Main12 => "main-12",
            Self::Main12FourFourFour => "main-12-444",
        }
    }

    /// Parse a CLI/API profile setting value.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "main-10" => Some(Self::Main10),
            "main-10-444" => Some(Self::Main10FourFourFour),
            "main-12" => Some(Self::Main12),
            "main-12-444" => Some(Self::Main12FourFourFour),
            _ => None,
        }
    }

    pub(in crate::vvc) fn validate_for_format(
        self,
        format: VvcPictureFormat,
    ) -> Result<Self, String> {
        let effective = self.effective_for_bit_depth(format.bit_depth)?;
        if !effective.supports_chroma_sampling(format.chroma_sampling) {
            return Err(format!(
                "VVC profile {} does not support {:?} input; select profile=auto or a 444-capable profile",
                effective.as_str(),
                format.chroma_sampling
            ));
        }
        Ok(effective)
    }

    pub(in crate::vvc) fn effective_for_bit_depth(
        self,
        bit_depth: SampleBitDepth,
    ) -> Result<Self, String> {
        match self {
            Self::Auto if bit_depth.bits() > 10 => Ok(Self::Main12FourFourFour),
            Self::Auto => Ok(Self::Main10FourFourFour),
            Self::Main10 | Self::Main10FourFourFour if bit_depth.bits() > 10 => Err(format!(
                "VVC profile {} supports at most 10-bit input; got {}-bit",
                self.as_str(),
                bit_depth.bits()
            )),
            Self::Main10 | Self::Main10FourFourFour | Self::Main12 | Self::Main12FourFourFour => {
                Ok(self)
            }
        }
    }

    pub(in crate::vvc) const fn general_profile_idc(self) -> u32 {
        match self {
            Self::Auto => 33,
            Self::Main10 => 1,
            Self::Main12 => 2,
            Self::Main10FourFourFour => 33,
            Self::Main12FourFourFour => 34,
        }
    }

    pub(in crate::vvc) const fn allows_palette(self) -> bool {
        matches!(self, Self::Auto | Self::Main10FourFourFour | Self::Main12FourFourFour)
    }

    pub(in crate::vvc) const fn allows_ibc(self) -> bool {
        self.allows_palette()
    }

    const fn supports_chroma_sampling(self, chroma_sampling: ChromaSampling) -> bool {
        match self {
            Self::Auto | Self::Main10FourFourFour | Self::Main12FourFourFour => true,
            Self::Main10 | Self::Main12 => {
                matches!(chroma_sampling, ChromaSampling::Monochrome | ChromaSampling::Cs420)
            }
        }
    }
}

impl std::fmt::Display for VvcProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for VvcProfile {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or_else(|| {
            format!(
                "VVC profile expects one of {}, got '{value}'",
                Self::VALUES.join("|")
            )
        })
    }
}

impl From<VvcProfile> for framefinery_api::VideoSettingValue {
    fn from(value: VvcProfile) -> Self {
        Self::Text(value.as_str().to_string())
    }
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
    pub gop: crate::settings::GopMode,
    pub fast_search: VvcFastSearch,
    pub profile: VvcProfile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VvcEncodeArtifacts {
    pub bitstream: Vec<u8>,
    pub reconstruction: Vec<u8>,
}

pub struct VvcEncodeFrameMetrics<'a> {
    pub frame_idx: usize,
    pub frame_count: Option<usize>,
    pub bitstream_bytes: usize,
    pub total_bitstream_bytes: usize,
    pub encode_elapsed: std::time::Duration,
    pub source: &'a [u8],
    pub reconstruction: &'a [u8],
}
