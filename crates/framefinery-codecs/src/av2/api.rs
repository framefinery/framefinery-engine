#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Av2EncodeParams {
    /// Number of frames to encode. Zero means read complete frames until EOF.
    pub frames: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Av2EncodeRequest {
    pub params: Av2EncodeParams,
    pub geometry: Av2VideoGeometry,
    pub format: PixelFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Av2EncodeOptions {
    pub lossless: bool,
    pub qp: Option<u8>,
    pub predictive: bool,
}

pub struct Av2EncodeFrameMetrics<'a> {
    pub frame_idx: usize,
    pub frame_count: Option<usize>,
    pub bitstream_bytes: usize,
    pub total_bitstream_bytes: usize,
    pub encode_elapsed: std::time::Duration,
    pub source: &'a [u8],
    pub reconstruction: &'a [u8],
}

impl Av2EncodeRequest {
    pub fn validate(&self) -> Result<(), String> {
        self.geometry.validate_shape()?;
        Picture::validate_format_shape(
            self.geometry.width,
            self.geometry.height,
            self.format,
            validate_av2_input_format,
        )?;
        Ok(())
    }
}
