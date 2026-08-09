use std::fmt;
use std::str::FromStr;

use crate::error::{MediaError, Result};

/// Checked planar sample bit depth for raw frame formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SampleBitDepth(u8);

/// Chroma subsampling layout for planar YUV or monochrome data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromaSampling {
    /// One luma or gray plane with no chroma planes.
    Monochrome,
    /// 4:2:0 chroma, subsampled by two in width and height.
    Cs420,
    /// 4:2:2 chroma, subsampled by two in width only.
    Cs422,
    /// 4:4:4 chroma, no chroma subsampling.
    Cs444,
}

/// Raw pixel layout accepted by FrameFinery frame buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Planar YUV with explicit chroma sampling and sample bit depth.
    PlanarYuv {
        /// Chroma sampling layout.
        chroma_sampling: ChromaSampling,
        /// Bits per sample.
        bit_depth: SampleBitDepth,
    },
    /// Monochrome gray plane with explicit sample bit depth.
    Gray {
        /// Bits per sample.
        bit_depth: SampleBitDepth,
    },
    /// Planar GBR, 8 bits per sample, ordered G plane, B plane, R plane.
    Gbrp8,
    /// Packed RGB, 8 bits per channel, ordered R, G, B per pixel.
    Rgb24,
}

impl ChromaSampling {
    /// Horizontal subsampling factor.
    pub const fn subsample_x(self) -> usize {
        match self {
            Self::Monochrome | Self::Cs444 => 1,
            Self::Cs420 | Self::Cs422 => 2,
        }
    }

    /// Vertical subsampling factor.
    pub const fn subsample_y(self) -> usize {
        match self {
            Self::Monochrome | Self::Cs422 | Self::Cs444 => 1,
            Self::Cs420 => 2,
        }
    }

    /// Number of samples in one chroma plane for this layout.
    pub fn chroma_plane_samples(self, width: usize, height: usize) -> Option<usize> {
        let luma = width.checked_mul(height)?;
        match self {
            Self::Monochrome => Some(0),
            Self::Cs420 => luma.checked_div(4),
            Self::Cs422 => luma.checked_div(2),
            Self::Cs444 => Some(luma),
        }
    }
}

impl SampleBitDepth {
    /// Number of bits used for each stored sample.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Create a bit depth in the supported 8 through 16 bit range.
    pub const fn new(bits: u8) -> Option<Self> {
        if bits >= 8 && bits <= 16 {
            Some(Self(bits))
        } else {
            None
        }
    }

    /// Alias for [`SampleBitDepth::new`].
    pub const fn from_bits(bits: u8) -> Option<Self> {
        Self::new(bits)
    }

    const fn new_unchecked(bits: u8) -> Self {
        Self(bits)
    }

    /// Number of bytes used to store one sample.
    pub fn bytes_per_sample(self) -> usize {
        if self.bits() <= 8 {
            1
        } else {
            2
        }
    }

    /// Maximum legal sample value for this bit depth.
    pub fn max_sample(self) -> u16 {
        (1u32.checked_shl(self.bits() as u32).unwrap() - 1) as u16
    }
}

impl PixelFormat {
    // TODO(deprecated): prefer numeric constructors such as
    // `PixelFormat::yuv420(8)` over named 8-bit compatibility constants.
    #[allow(non_upper_case_globals)]
    /// Compatibility constant for 8-bit 4:2:0 planar YUV.
    pub const Yuv420p8: Self =
        Self::planar_yuv(ChromaSampling::Cs420, SampleBitDepth::new_unchecked(8));
    #[allow(non_upper_case_globals)]
    /// Compatibility constant for 8-bit 4:2:2 planar YUV.
    pub const Yuv422p8: Self =
        Self::planar_yuv(ChromaSampling::Cs422, SampleBitDepth::new_unchecked(8));
    #[allow(non_upper_case_globals)]
    /// Compatibility constant for 8-bit 4:4:4 planar YUV.
    pub const Yuv444p8: Self =
        Self::planar_yuv(ChromaSampling::Cs444, SampleBitDepth::new_unchecked(8));
    #[allow(non_upper_case_globals)]
    /// Compatibility constant for 8-bit monochrome gray.
    pub const Gray8: Self = Self::gray_with_depth(SampleBitDepth::new_unchecked(8));

    /// Create a planar YUV format from chroma sampling and checked bit depth.
    pub const fn planar_yuv(chroma_sampling: ChromaSampling, bit_depth: SampleBitDepth) -> Self {
        Self::PlanarYuv {
            chroma_sampling,
            bit_depth,
        }
    }

    /// Create a planar YUV format from chroma sampling and numeric bit depth.
    pub const fn yuv(chroma_sampling: ChromaSampling, bit_depth: u8) -> Option<Self> {
        match SampleBitDepth::new(bit_depth) {
            Some(bit_depth) => Some(Self::planar_yuv(chroma_sampling, bit_depth)),
            None => None,
        }
    }

    /// Create a 4:2:0 planar YUV format with the requested bit depth.
    pub const fn yuv420(bit_depth: u8) -> Option<Self> {
        Self::yuv(ChromaSampling::Cs420, bit_depth)
    }

    /// Create a 4:2:2 planar YUV format with the requested bit depth.
    pub const fn yuv422(bit_depth: u8) -> Option<Self> {
        Self::yuv(ChromaSampling::Cs422, bit_depth)
    }

    /// Create a 4:4:4 planar YUV format with the requested bit depth.
    pub const fn yuv444(bit_depth: u8) -> Option<Self> {
        Self::yuv(ChromaSampling::Cs444, bit_depth)
    }

    /// Create a monochrome gray format with the requested bit depth.
    pub const fn gray(bit_depth: u8) -> Option<Self> {
        match SampleBitDepth::new(bit_depth) {
            Some(bit_depth) => Some(Self::Gray { bit_depth }),
            None => None,
        }
    }

    const fn gray_with_depth(bit_depth: SampleBitDepth) -> Self {
        Self::Gray { bit_depth }
    }

    /// Canonical CLI/raw-format name for this pixel format.
    pub fn name(self) -> String {
        match self {
            Self::PlanarYuv {
                chroma_sampling,
                bit_depth,
            } => {
                let sampling = match chroma_sampling {
                    ChromaSampling::Cs420 => "420",
                    ChromaSampling::Cs422 => "422",
                    ChromaSampling::Cs444 => "444",
                    ChromaSampling::Monochrome => unreachable!("YUV cannot be monochrome"),
                };
                if bit_depth.bits() == 8 {
                    format!("yuv{sampling}p8")
                } else {
                    format!("yuv{}p{}le", sampling, bit_depth.bits())
                }
            }
            Self::Gray { bit_depth } => {
                if bit_depth.bits() == 8 {
                    "gray8".to_string()
                } else {
                    format!("gray{}le", bit_depth.bits())
                }
            }
            Self::Gbrp8 => "gbrp8".to_string(),
            Self::Rgb24 => "rgb24".to_string(),
        }
    }

    /// Sample bit depth used by this format.
    pub fn bit_depth(self) -> SampleBitDepth {
        match self {
            Self::PlanarYuv { bit_depth, .. } | Self::Gray { bit_depth } => bit_depth,
            Self::Gbrp8 | Self::Rgb24 => SampleBitDepth::new_unchecked(8),
        }
    }

    /// Number of bytes used to store one sample in this format.
    pub fn bytes_per_sample(self) -> usize {
        self.bit_depth().bytes_per_sample()
    }

    /// Return whether this is planar YUV 4:2:0.
    pub fn is_yuv420(self) -> bool {
        self.chroma_sampling() == Some(ChromaSampling::Cs420)
    }

    /// Return whether this is a planar YUV format.
    pub fn is_yuv(self) -> bool {
        self.chroma_sampling()
            .is_some_and(|sampling| sampling != ChromaSampling::Monochrome)
    }

    /// Return whether this is planar RGB-family data.
    pub fn is_planar_rgb(self) -> bool {
        self == Self::Gbrp8
    }

    /// Return whether this is an RGB-family format.
    pub fn is_rgb(self) -> bool {
        matches!(self, Self::Gbrp8 | Self::Rgb24)
    }

    /// Chroma sampling for planar formats, or `None` for RGB-family formats.
    pub fn chroma_sampling(self) -> Option<ChromaSampling> {
        match self {
            Self::PlanarYuv {
                chroma_sampling, ..
            } => Some(chroma_sampling),
            Self::Gray { .. } => Some(ChromaSampling::Monochrome),
            Self::Gbrp8 | Self::Rgb24 => None,
        }
    }

    /// Number of samples in one chroma plane for the given frame geometry.
    pub fn chroma_plane_samples(self, width: usize, height: usize) -> Option<usize> {
        self.chroma_sampling()?.chroma_plane_samples(width, height)
    }

    /// Return the same planar layout with a different bit depth.
    pub fn with_bit_depth(self, bit_depth: SampleBitDepth) -> Option<Self> {
        match self.chroma_sampling()? {
            ChromaSampling::Cs420 | ChromaSampling::Cs422 | ChromaSampling::Cs444 => {
                Some(Self::planar_yuv(self.chroma_sampling()?, bit_depth))
            }
            ChromaSampling::Monochrome => Some(Self::gray_with_depth(bit_depth)),
        }
    }

    /// Total byte length of one frame with this format and geometry.
    pub fn frame_len(self, width: usize, height: usize) -> Option<usize> {
        let luma = width.checked_mul(height)?;
        let bytes_per_sample = self.bytes_per_sample();
        match self.chroma_sampling() {
            Some(ChromaSampling::Cs420 | ChromaSampling::Cs422 | ChromaSampling::Cs444) => {
                let chroma_plane = self.chroma_plane_samples(width, height)?;
                luma.checked_add(chroma_plane.checked_mul(2)?)?
                    .checked_mul(bytes_per_sample)
            }
            Some(ChromaSampling::Monochrome) => luma.checked_mul(bytes_per_sample),
            None => luma.checked_mul(3),
        }
    }

    /// Validate that geometry is legal for this format and has an addressable length.
    pub fn validate_geometry(self, width: usize, height: usize) -> Result<()> {
        if width == 0 || height == 0 {
            return Err(MediaError::InvalidDimensions { width, height });
        }

        if self.is_yuv420() && (!width.is_multiple_of(2) || !height.is_multiple_of(2)) {
            return Err(MediaError::IncompatibleFormat {
                format: self.name(),
                reason: "width and height must be even".to_string(),
            });
        }

        if matches!(self.chroma_sampling(), Some(ChromaSampling::Cs422)) && !width.is_multiple_of(2)
        {
            return Err(MediaError::IncompatibleFormat {
                format: self.name(),
                reason: "width must be even".to_string(),
            });
        }

        self.frame_len(width, height)
            .ok_or(MediaError::LengthOverflow)?;
        Ok(())
    }
}

/// Convert a planar YUV or gray frame between supported bit depths.
///
/// The chroma layout must remain the same; this helper does not convert RGB,
/// YUV, or chroma subsampling.
pub fn convert_planar_frame_bit_depth(
    input: &[u8],
    width: usize,
    height: usize,
    source_format: PixelFormat,
    target_format: PixelFormat,
) -> Result<Vec<u8>> {
    if source_format.chroma_sampling().is_none() || target_format.chroma_sampling().is_none() {
        return Err(MediaError::Message(
            "bit-depth conversion supports only planar YUV and gray formats".to_string(),
        ));
    }
    if source_format.chroma_sampling() != target_format.chroma_sampling() {
        return Err(MediaError::Message(format!(
            "bit-depth conversion cannot change chroma layout: {source_format} -> {target_format}"
        )));
    }

    source_format.validate_geometry(width, height)?;
    target_format.validate_geometry(width, height)?;
    let expected = source_format
        .frame_len(width, height)
        .ok_or(MediaError::LengthOverflow)?;
    if input.len() != expected {
        return Err(MediaError::BufferLength {
            expected,
            actual: input.len(),
        });
    }

    let target_bytes = target_format.bytes_per_sample();
    let sample_count = expected / source_format.bytes_per_sample();
    let mut output = vec![0; sample_count * target_bytes];
    for sample_idx in 0..sample_count {
        let source_sample = read_planar_sample(input, sample_idx, source_format.bit_depth())
            .expect("validated source frame length must contain every source sample");
        let target_sample = scale_sample_bit_depth(
            source_sample,
            source_format.bit_depth(),
            target_format.bit_depth(),
        );
        write_planar_sample(
            &mut output,
            sample_idx,
            target_sample,
            target_format.bit_depth(),
        )
        .expect("allocated target frame length must contain every target sample");
    }
    Ok(output)
}

/// Convert between closely related raw frame formats.
///
/// Currently supports same-format copies, planar bit-depth conversion, and
/// reversible `rgb24` to/from `gbrp8` repacking.
pub fn convert_frame_format(
    input: &[u8],
    width: usize,
    height: usize,
    source_format: PixelFormat,
    target_format: PixelFormat,
) -> Result<Vec<u8>> {
    source_format.validate_geometry(width, height)?;
    target_format.validate_geometry(width, height)?;
    let expected = source_format
        .frame_len(width, height)
        .ok_or(MediaError::LengthOverflow)?;
    if input.len() != expected {
        return Err(MediaError::BufferLength {
            expected,
            actual: input.len(),
        });
    }
    if source_format == target_format {
        return Ok(input.to_vec());
    }
    match (source_format, target_format) {
        (PixelFormat::Rgb24, PixelFormat::Gbrp8) => rgb24_to_gbrp8(input, width, height),
        (PixelFormat::Gbrp8, PixelFormat::Rgb24) => gbrp8_to_rgb24(input, width, height),
        _ => convert_planar_frame_bit_depth(input, width, height, source_format, target_format),
    }
}

fn rgb24_to_gbrp8(input: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let pixels = width
        .checked_mul(height)
        .ok_or(MediaError::LengthOverflow)?;
    let expected = pixels.checked_mul(3).ok_or(MediaError::LengthOverflow)?;
    if input.len() != expected {
        return Err(MediaError::BufferLength {
            expected,
            actual: input.len(),
        });
    }
    let mut output = vec![0; expected];
    let (g_plane, br_planes) = output.split_at_mut(pixels);
    let (b_plane, r_plane) = br_planes.split_at_mut(pixels);
    for (idx, pixel) in input.chunks_exact(3).enumerate() {
        r_plane[idx] = pixel[0];
        g_plane[idx] = pixel[1];
        b_plane[idx] = pixel[2];
    }
    Ok(output)
}

fn gbrp8_to_rgb24(input: &[u8], width: usize, height: usize) -> Result<Vec<u8>> {
    let pixels = width
        .checked_mul(height)
        .ok_or(MediaError::LengthOverflow)?;
    let expected = pixels.checked_mul(3).ok_or(MediaError::LengthOverflow)?;
    if input.len() != expected {
        return Err(MediaError::BufferLength {
            expected,
            actual: input.len(),
        });
    }
    let (g_plane, br_planes) = input.split_at(pixels);
    let (b_plane, r_plane) = br_planes.split_at(pixels);
    let mut output = vec![0; expected];
    for idx in 0..pixels {
        let offset = idx * 3;
        output[offset] = r_plane[idx];
        output[offset + 1] = g_plane[idx];
        output[offset + 2] = b_plane[idx];
    }
    Ok(output)
}

/// Scale one sample value between two bit depths with rounding.
pub fn scale_sample_bit_depth(
    sample: u16,
    source_depth: SampleBitDepth,
    target_depth: SampleBitDepth,
) -> u16 {
    let source_max = u32::from(source_depth.max_sample());
    let target_max = u32::from(target_depth.max_sample());
    let sample = u32::from(sample).min(source_max);
    if source_max == target_max {
        return sample as u16;
    }
    ((sample * target_max + (source_max / 2)) / source_max) as u16
}

/// Read one little-endian planar sample from a raw sample buffer.
///
/// Returns `None` when `sample_index` is out of bounds.
pub fn read_planar_sample(
    input: &[u8],
    sample_index: usize,
    bit_depth: SampleBitDepth,
) -> Option<u16> {
    let offset = sample_index.checked_mul(bit_depth.bytes_per_sample())?;
    if bit_depth.bits() <= 8 {
        input.get(offset).copied().map(u16::from)
    } else {
        Some(u16::from_le_bytes([
            *input.get(offset)?,
            *input.get(offset + 1)?,
        ]))
    }
}

/// Write one little-endian planar sample into a raw sample buffer.
///
/// Returns `None` when `sample_index` is out of bounds.
pub fn write_planar_sample(
    output: &mut [u8],
    sample_index: usize,
    sample: u16,
    bit_depth: SampleBitDepth,
) -> Option<()> {
    let offset = sample_index.checked_mul(bit_depth.bytes_per_sample())?;
    if bit_depth.bits() <= 8 {
        if offset >= output.len() {
            return None;
        }
        output[offset] = sample as u8;
    } else {
        if offset + 1 >= output.len() {
            return None;
        }
        let bytes = sample.to_le_bytes();
        output[offset] = bytes[0];
        output[offset + 1] = bytes[1];
    }
    Some(())
}

/// Compute sum of squared error for two equally sized planar sample buffers.
///
/// Returns `None` when the buffers differ in length or high-bit-depth buffers
/// are not made of complete little-endian samples.
pub fn planar_sample_sse(
    source: &[u8],
    reconstruction: &[u8],
    bit_depth: SampleBitDepth,
) -> Option<u64> {
    if source.len() != reconstruction.len() {
        return None;
    }
    if bit_depth.bits() <= 8 {
        return Some(
            source
                .iter()
                .zip(reconstruction)
                .map(|(&src, &rec)| {
                    let diff = i32::from(src) - i32::from(rec);
                    (diff * diff) as u64
                })
                .sum(),
        );
    }
    if !source.len().is_multiple_of(2) {
        return None;
    }

    Some(
        source
            .chunks_exact(2)
            .zip(reconstruction.chunks_exact(2))
            .map(|(src, rec)| {
                let src = u16::from_le_bytes([src[0], src[1]]).min(bit_depth.max_sample());
                let rec = u16::from_le_bytes([rec[0], rec[1]]).min(bit_depth.max_sample());
                let diff = i32::from(src) - i32::from(rec);
                (diff * diff) as u64
            })
            .sum(),
    )
}

impl FromStr for PixelFormat {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        let value = value.trim().to_ascii_lowercase();
        match value.as_str() {
            "yuv420p" | "yuv420p8" | "i420" => Ok(Self::Yuv420p8),
            "yuv422p" | "yuv422p8" | "i422" => Ok(Self::Yuv422p8),
            "yuv444p" | "yuv444p8" | "i444" => Ok(Self::Yuv444p8),
            "gray8" | "y8" => Ok(Self::Gray8),
            "gbrp" | "gbrp8" => Ok(Self::Gbrp8),
            "rgb24" => Ok(Self::Rgb24),
            other => {
                if let Some(format) = parse_planar_yuv_pixel_format(other)? {
                    return Ok(format);
                }
                if let Some(format) = parse_gray_pixel_format(other)? {
                    return Ok(format);
                }
                if let Some(format) = parse_hardware_yuv_alias(other)? {
                    return Ok(format);
                }
                Err(format!(
                    "unsupported format '{other}'; supported formats: yuv420p8..16le, yuv422p8..16le, yuv444p8..16le, gray8..16le, gbrp8, and rgb24"
                ))
            }
        }
    }
}

fn parse_planar_yuv_pixel_format(value: &str) -> std::result::Result<Option<PixelFormat>, String> {
    for (prefix, chroma_sampling) in [
        ("yuv420p", ChromaSampling::Cs420),
        ("yuv422p", ChromaSampling::Cs422),
        ("yuv444p", ChromaSampling::Cs444),
    ] {
        let Some(suffix) = value.strip_prefix(prefix) else {
            continue;
        };
        let bit_depth = parse_planar_bit_depth_suffix(value, suffix)?;
        return Ok(Some(PixelFormat::planar_yuv(chroma_sampling, bit_depth)));
    }
    Ok(None)
}

fn parse_gray_pixel_format(value: &str) -> std::result::Result<Option<PixelFormat>, String> {
    for prefix in ["gray", "y"] {
        let Some(suffix) = value.strip_prefix(prefix) else {
            continue;
        };
        let bit_depth = parse_planar_bit_depth_suffix(value, suffix)?;
        return Ok(Some(PixelFormat::gray_with_depth(bit_depth)));
    }
    Ok(None)
}

fn parse_hardware_yuv_alias(value: &str) -> std::result::Result<Option<PixelFormat>, String> {
    let Some(rest) = value.strip_prefix('i') else {
        return Ok(None);
    };
    if rest.len() != 3 {
        return Ok(None);
    }
    let (sampling_digit, depth_text) = rest.split_at(1);
    let chroma_sampling = match sampling_digit {
        "0" => ChromaSampling::Cs420,
        "2" => ChromaSampling::Cs422,
        "4" => ChromaSampling::Cs444,
        _ => return Ok(None),
    };
    let bit_depth = parse_bit_depth(value, depth_text)?;
    Ok(Some(PixelFormat::planar_yuv(chroma_sampling, bit_depth)))
}

fn parse_planar_bit_depth_suffix(
    value: &str,
    suffix: &str,
) -> std::result::Result<SampleBitDepth, String> {
    let suffix = suffix.strip_suffix("le").unwrap_or(suffix);
    if suffix.ends_with("be") {
        return Err(format!(
            "unsupported format '{value}'; big-endian raw samples are not supported yet"
        ));
    }
    if suffix.is_empty() {
        return Ok(SampleBitDepth::new_unchecked(8));
    }
    parse_bit_depth(value, suffix)
}

fn parse_bit_depth(value: &str, bits: &str) -> std::result::Result<SampleBitDepth, String> {
    let parsed = bits
        .parse::<u8>()
        .map_err(|_| format!("unsupported format '{value}'; bit depth must be 8..16"))?;
    SampleBitDepth::new(parsed)
        .ok_or_else(|| format!("unsupported format '{value}'; bit depth must be 8..16"))
}

impl fmt::Display for PixelFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

/// Validated raw frame geometry and pixel format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameInfo {
    /// Frame width in pixels.
    pub width: usize,
    /// Frame height in pixels.
    pub height: usize,
    /// Raw pixel format.
    pub format: PixelFormat,
}

impl FrameInfo {
    /// Validate and create frame metadata.
    pub fn new(width: usize, height: usize, format: PixelFormat) -> Result<Self> {
        format.validate_geometry(width, height)?;
        Ok(Self {
            width,
            height,
            format,
        })
    }

    /// Byte length required for one complete frame with this metadata.
    pub fn expected_len(self) -> usize {
        self.format
            .frame_len(self.width, self.height)
            .expect("validated frame dimensions must have a byte length")
    }
}

/// Owned raw frame buffer paired with validated frame metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    info: FrameInfo,
    data: Vec<u8>,
}

/// Borrowed raw frame buffer paired with validated frame metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRef<'a> {
    info: FrameInfo,
    data: &'a [u8],
}

/// Per-plane and aggregate PSNR values for a reconstructed frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FramePsnr {
    /// First plane PSNR. For RGB-family formats this is red.
    pub plane0: f64,
    /// Second plane PSNR. For RGB-family formats this is green.
    pub plane1: f64,
    /// Third plane PSNR. For RGB-family formats this is blue.
    pub plane2: f64,
    /// Aggregate PSNR over every sample.
    pub all: f64,
}

impl Frame {
    /// Create an owned frame, validating that `data` has exactly the expected length.
    pub fn new(info: FrameInfo, data: Vec<u8>) -> Result<Self> {
        let expected = info.expected_len();
        let actual = data.len();
        if actual != expected {
            return Err(MediaError::BufferLength { expected, actual });
        }

        Ok(Self { info, data })
    }

    /// Create an all-zero frame with the expected byte length for `info`.
    pub fn blank(info: FrameInfo) -> Self {
        Self {
            info,
            data: vec![0; info.expected_len()],
        }
    }

    /// Metadata describing this frame.
    pub fn info(&self) -> FrameInfo {
        self.info
    }

    /// Raw frame bytes.
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Borrow this frame as a [`FrameRef`].
    pub fn as_frame_ref(&self) -> FrameRef<'_> {
        FrameRef {
            info: self.info,
            data: &self.data,
        }
    }

    /// Consume the frame and return its raw bytes.
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

impl<'a> FrameRef<'a> {
    /// Create a borrowed frame, validating that `data` has exactly the expected length.
    pub fn new(info: FrameInfo, data: &'a [u8]) -> Result<Self> {
        let expected = info.expected_len();
        let actual = data.len();
        if actual != expected {
            return Err(MediaError::BufferLength { expected, actual });
        }

        Ok(Self { info, data })
    }

    /// Metadata describing this borrowed frame.
    pub fn info(self) -> FrameInfo {
        self.info
    }

    /// Raw borrowed frame bytes.
    pub fn data(self) -> &'a [u8] {
        self.data
    }

    /// Copy this borrowed frame into an owned [`Frame`].
    pub fn to_owned_frame(self) -> Frame {
        Frame {
            info: self.info,
            data: self.data.to_vec(),
        }
    }
}

/// Compute PSNR between a source frame and reconstruction.
///
/// Returns `None` when the buffers do not match the byte length implied by
/// `info` or the format is unsupported by the metric helper.
pub fn frame_psnr(info: FrameInfo, source: &[u8], reconstruction: &[u8]) -> Option<FramePsnr> {
    let luma_samples = info.width.checked_mul(info.height)?;
    if info.format == PixelFormat::Rgb24 {
        return rgb24_frame_psnr(luma_samples, source, reconstruction);
    }
    if info.format == PixelFormat::Gbrp8 {
        return gbrp8_frame_psnr(luma_samples, source, reconstruction);
    }
    let chroma_sampling = info.format.chroma_sampling()?;
    let chroma_width = info.width.checked_div(chroma_sampling.subsample_x())?;
    let chroma_height = info.height.checked_div(chroma_sampling.subsample_y())?;
    let chroma_samples = chroma_width.checked_mul(chroma_height)?;
    let bytes_per_sample = info.format.bit_depth().bytes_per_sample();
    let luma_len = luma_samples.checked_mul(bytes_per_sample)?;
    let chroma_len = chroma_samples.checked_mul(bytes_per_sample)?;
    let frame_len = luma_len.checked_add(chroma_len.checked_mul(2)?)?;
    if source.len() != frame_len || reconstruction.len() != frame_len {
        return None;
    }
    if source == reconstruction {
        return Some(FramePsnr::infinite());
    }

    let y_src = &source[..luma_len];
    let y_rec = &reconstruction[..luma_len];
    let u_start = luma_len;
    let v_start = luma_len + chroma_len;
    let u_src = &source[u_start..v_start];
    let u_rec = &reconstruction[u_start..v_start];
    let v_src = &source[v_start..frame_len];
    let v_rec = &reconstruction[v_start..frame_len];

    let bit_depth = info.format.bit_depth();
    let y_sse = planar_sample_sse(y_src, y_rec, bit_depth)?;
    let u_sse = planar_sample_sse(u_src, u_rec, bit_depth)?;
    let v_sse = planar_sample_sse(v_src, v_rec, bit_depth)?;
    let max_sample = f64::from(bit_depth.max_sample());
    Some(FramePsnr {
        plane0: psnr_from_sse(y_sse, luma_samples, max_sample),
        plane1: psnr_from_sse(u_sse, chroma_samples, max_sample),
        plane2: psnr_from_sse(v_sse, chroma_samples, max_sample),
        all: psnr_from_sse(
            y_sse + u_sse + v_sse,
            luma_samples + chroma_samples * 2,
            max_sample,
        ),
    })
}

impl FramePsnr {
    fn infinite() -> Self {
        Self {
            plane0: f64::INFINITY,
            plane1: f64::INFINITY,
            plane2: f64::INFINITY,
            all: f64::INFINITY,
        }
    }
}

fn gbrp8_frame_psnr(pixels: usize, source: &[u8], reconstruction: &[u8]) -> Option<FramePsnr> {
    let plane_len = pixels;
    let frame_len = plane_len.checked_mul(3)?;
    if source.len() != frame_len || reconstruction.len() != frame_len {
        return None;
    }
    if source == reconstruction {
        return Some(FramePsnr::infinite());
    }

    let (source_g, source_chroma) = source.split_at(plane_len);
    let (source_b, source_r) = source_chroma.split_at(plane_len);
    let (recon_g, recon_chroma) = reconstruction.split_at(plane_len);
    let (recon_b, recon_r) = recon_chroma.split_at(plane_len);
    let r_sse = planar_sample_sse(source_r, recon_r, SampleBitDepth::new_unchecked(8))?;
    let g_sse = planar_sample_sse(source_g, recon_g, SampleBitDepth::new_unchecked(8))?;
    let b_sse = planar_sample_sse(source_b, recon_b, SampleBitDepth::new_unchecked(8))?;
    Some(FramePsnr {
        plane0: psnr_from_sse(r_sse, pixels, 255.0),
        plane1: psnr_from_sse(g_sse, pixels, 255.0),
        plane2: psnr_from_sse(b_sse, pixels, 255.0),
        all: psnr_from_sse(r_sse + g_sse + b_sse, frame_len, 255.0),
    })
}

fn rgb24_frame_psnr(pixels: usize, source: &[u8], reconstruction: &[u8]) -> Option<FramePsnr> {
    let frame_len = pixels.checked_mul(3)?;
    if source.len() != frame_len || reconstruction.len() != frame_len {
        return None;
    }
    if source == reconstruction {
        return Some(FramePsnr::infinite());
    }

    let mut r_sse = 0u64;
    let mut g_sse = 0u64;
    let mut b_sse = 0u64;
    for (src, rec) in source.chunks_exact(3).zip(reconstruction.chunks_exact(3)) {
        let r_diff = src[0] as i32 - rec[0] as i32;
        let g_diff = src[1] as i32 - rec[1] as i32;
        let b_diff = src[2] as i32 - rec[2] as i32;
        r_sse += (r_diff * r_diff) as u64;
        g_sse += (g_diff * g_diff) as u64;
        b_sse += (b_diff * b_diff) as u64;
    }

    Some(FramePsnr {
        plane0: psnr_from_sse(r_sse, pixels, 255.0),
        plane1: psnr_from_sse(g_sse, pixels, 255.0),
        plane2: psnr_from_sse(b_sse, pixels, 255.0),
        all: psnr_from_sse(r_sse + g_sse + b_sse, frame_len, 255.0),
    })
}

fn psnr_from_sse(sse: u64, samples: usize, max_sample: f64) -> f64 {
    if sse == 0 {
        f64::INFINITY
    } else {
        10.0 * ((max_sample * max_sample * samples as f64) / sse as f64).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_common_frame_lengths() {
        assert_eq!(PixelFormat::Yuv420p8.frame_len(16, 16), Some(384));
        assert_eq!(PixelFormat::Yuv444p8.frame_len(16, 16), Some(768));
        assert_eq!(PixelFormat::Gbrp8.frame_len(16, 16), Some(768));
        assert_eq!(PixelFormat::Rgb24.frame_len(16, 16), Some(768));
        assert_eq!(PixelFormat::yuv420(9).unwrap().frame_len(16, 16), Some(768));
        assert_eq!(
            PixelFormat::yuv420(10).unwrap().frame_len(16, 16),
            Some(768)
        );
        assert_eq!(
            PixelFormat::yuv444(15).unwrap().frame_len(16, 16),
            Some(1536)
        );
    }

    #[test]
    fn chroma_sampling_reports_subsampling_factors() {
        assert_eq!(ChromaSampling::Cs420.subsample_x(), 2);
        assert_eq!(ChromaSampling::Cs420.subsample_y(), 2);
        assert_eq!(ChromaSampling::Cs422.subsample_x(), 2);
        assert_eq!(ChromaSampling::Cs422.subsample_y(), 1);
        assert_eq!(ChromaSampling::Cs444.subsample_x(), 1);
        assert_eq!(ChromaSampling::Cs444.subsample_y(), 1);
    }

    #[test]
    fn planar_sample_sse_handles_8_and_high_bit_depth_samples() {
        assert_eq!(
            planar_sample_sse(&[0, 10, 20], &[1, 8, 20], SampleBitDepth::new(8).unwrap()),
            Some(5)
        );

        let source = [0u16, 1023, 512]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let reconstruction = [1u16, 1020, 512]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        assert_eq!(
            planar_sample_sse(&source, &reconstruction, SampleBitDepth::new(10).unwrap()),
            Some(10)
        );
        assert_eq!(
            planar_sample_sse(
                &source[..5],
                &reconstruction[..5],
                SampleBitDepth::new(10).unwrap()
            ),
            None
        );
        assert_eq!(
            planar_sample_sse(
                &source,
                &reconstruction[..4],
                SampleBitDepth::new(10).unwrap()
            ),
            None
        );
    }

    #[test]
    fn rejects_odd_420_dimensions() {
        let err = FrameInfo::new(15, 16, PixelFormat::Yuv420p8).unwrap_err();
        assert!(err.to_string().contains("must be even"));
    }

    #[test]
    fn validates_frame_buffer_length() {
        let info = FrameInfo::new(8, 8, PixelFormat::Yuv444p8).unwrap();
        let frame = Frame::new(info, vec![0; 192]);
        assert!(frame.is_ok());

        let err = Frame::new(info, vec![0; 191]).unwrap_err();
        assert_eq!(
            err,
            MediaError::BufferLength {
                expected: 192,
                actual: 191,
            }
        );
    }

    #[test]
    fn frame_ref_validates_borrowed_frame_length() {
        let info = FrameInfo::new(4, 4, PixelFormat::Rgb24).unwrap();
        let data = vec![7; info.expected_len()];
        let view = FrameRef::new(info, &data).expect("valid borrowed frame");

        assert_eq!(view.info(), info);
        assert_eq!(view.data(), data.as_slice());
        assert_eq!(view.to_owned_frame(), Frame::new(info, data).unwrap());
    }

    #[test]
    fn frame_psnr_reports_yuv422p8() {
        let info = FrameInfo::new(4, 2, PixelFormat::Yuv422p8).unwrap();
        let source = vec![0u8; info.expected_len()];
        let mut reconstruction = source.clone();
        reconstruction[0] = 16;
        reconstruction[8] = 8;
        reconstruction[12] = 4;

        let psnr = frame_psnr(info, &source, &reconstruction).expect("4:2:2 PSNR");
        assert!(psnr.plane0.is_finite());
        assert!(psnr.plane1.is_finite());
        assert!(psnr.plane2.is_finite());
        assert!(psnr.all.is_finite());
    }

    #[test]
    fn frame_psnr_uses_high_bit_depth_peak_sample() {
        let info = FrameInfo::new(4, 2, PixelFormat::yuv420(10).unwrap()).unwrap();
        let mut source = vec![0u8; info.expected_len()];
        let mut reconstruction = source.clone();
        source[0..2].copy_from_slice(&1023u16.to_le_bytes());
        reconstruction[0..2].copy_from_slice(&1022u16.to_le_bytes());

        let psnr = frame_psnr(info, &source, &reconstruction).expect("10-bit PSNR");
        assert!(psnr.all > 70.0, "10-bit peak sample should be used");
    }

    #[test]
    fn parses_hardware_model_pixel_format_aliases() {
        assert_eq!(
            "i010".parse::<PixelFormat>(),
            Ok(PixelFormat::yuv420(10).unwrap())
        );
        assert_eq!("yuv444p".parse::<PixelFormat>(), Ok(PixelFormat::Yuv444p8));
        assert_eq!("gbrp".parse::<PixelFormat>(), Ok(PixelFormat::Gbrp8));
        assert_eq!("gbrp8".parse::<PixelFormat>(), Ok(PixelFormat::Gbrp8));
        assert_eq!("rgb24".parse::<PixelFormat>(), Ok(PixelFormat::Rgb24));
    }

    #[test]
    fn parses_planar_input_depths_between_8_and_16_bits() {
        assert_eq!(
            "yuv420p9le".parse::<PixelFormat>(),
            Ok(PixelFormat::yuv420(9).unwrap())
        );
        assert_eq!(
            "yuv422p11le".parse::<PixelFormat>(),
            Ok(PixelFormat::yuv422(11).unwrap())
        );
        assert_eq!(
            "yuv444p14le".parse::<PixelFormat>(),
            Ok(PixelFormat::yuv444(14).unwrap())
        );
        assert_eq!(
            "gray15le".parse::<PixelFormat>(),
            Ok(PixelFormat::gray(15).unwrap())
        );
        assert_eq!(
            SampleBitDepth::from_bits(13),
            Some(SampleBitDepth::new(13).unwrap())
        );
    }

    #[test]
    fn maps_planar_formats_to_the_same_layout_at_another_depth() {
        assert_eq!(
            PixelFormat::yuv420(13)
                .unwrap()
                .with_bit_depth(SampleBitDepth::new(8).unwrap()),
            Some(PixelFormat::Yuv420p8)
        );
        assert_eq!(
            PixelFormat::Yuv444p8.with_bit_depth(SampleBitDepth::new(16).unwrap()),
            PixelFormat::yuv444(16)
        );
        assert_eq!(
            PixelFormat::Rgb24.with_bit_depth(SampleBitDepth::new(8).unwrap()),
            None
        );
        assert_eq!(
            PixelFormat::Gbrp8.with_bit_depth(SampleBitDepth::new(8).unwrap()),
            None
        );
    }

    #[test]
    fn converts_planar_frame_bit_depth_without_changing_layout() {
        let input = [
            0u16, 1023, 512, 256, // Y
            128, // U
            768, // V
        ]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();

        let output = convert_planar_frame_bit_depth(
            &input,
            2,
            2,
            PixelFormat::yuv420(10).unwrap(),
            PixelFormat::Yuv420p8,
        )
        .unwrap();

        assert_eq!(output, vec![0, 255, 128, 64, 32, 191]);
    }

    #[test]
    fn converts_packed_rgb24_to_planar_gbrp8_and_back() {
        let rgb24 = vec![
            10, 20, 30, //
            40, 50, 60, //
            70, 80, 90, //
            100, 110, 120,
        ];

        let gbrp8 =
            convert_frame_format(&rgb24, 2, 2, PixelFormat::Rgb24, PixelFormat::Gbrp8).unwrap();
        assert_eq!(
            gbrp8,
            vec![20, 50, 80, 110, 30, 60, 90, 120, 10, 40, 70, 100]
        );

        let roundtrip =
            convert_frame_format(&gbrp8, 2, 2, PixelFormat::Gbrp8, PixelFormat::Rgb24).unwrap();
        assert_eq!(roundtrip, rgb24);
    }

    #[test]
    fn convert_frame_format_preserves_existing_planar_bit_depth_path() {
        let input = [
            0u16, 1023, 512, 256, // Y
            128, // U
            768, // V
        ]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();

        let output = convert_frame_format(
            &input,
            2,
            2,
            PixelFormat::yuv420(10).unwrap(),
            PixelFormat::Yuv420p8,
        )
        .unwrap();

        assert_eq!(output, vec![0, 255, 128, 64, 32, 191]);
    }

    #[test]
    fn rejects_bit_depth_conversion_that_changes_chroma_layout() {
        let input = vec![0; PixelFormat::yuv420(10).unwrap().frame_len(2, 2).unwrap()];
        let err = convert_planar_frame_bit_depth(
            &input,
            2,
            2,
            PixelFormat::yuv420(10).unwrap(),
            PixelFormat::Yuv444p8,
        )
        .unwrap_err();
        assert!(err.to_string().contains("cannot change chroma layout"));
    }

    #[test]
    fn reads_and_writes_planar_samples_by_numeric_bit_depth() {
        let depth = SampleBitDepth::new(10).unwrap();
        let mut data = vec![0; 4];
        assert_eq!(write_planar_sample(&mut data, 0, 1023, depth), Some(()));
        assert_eq!(write_planar_sample(&mut data, 1, 1, depth), Some(()));
        assert_eq!(read_planar_sample(&data, 0, depth), Some(1023));
        assert_eq!(read_planar_sample(&data, 1, depth), Some(1));
        assert_eq!(read_planar_sample(&data, 2, depth), None);
    }
}
