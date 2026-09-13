use framefinery_api::{
    ChromaSampling, CodecId, Frame, FrameInfo, PixelFormat, ReconstructionMode, VideoEncoderConfig,
    VideoEncoderSetting, VideoRateControl,
};

pub fn config(format: PixelFormat, lossless: bool, gop: i64) -> VideoEncoderConfig {
    VideoEncoderConfig::new(
        CodecId::new("vvc").unwrap(),
        FrameInfo::new(128, 64, format).unwrap(),
    )
    .with_rate_control(if lossless {
        VideoRateControl::Lossless
    } else {
        VideoRateControl::ConstantQuantizer(24)
    })
    .with_reconstruction(ReconstructionMode::Frames)
    .with_setting(VideoEncoderSetting::integer("gop", gop).unwrap())
}

pub fn frame(info: FrameInfo, phase: u16) -> Frame {
    let mut data = vec![0; info.expected_len()];
    let chroma = info
        .format
        .chroma_sampling()
        .unwrap_or(ChromaSampling::Cs444);
    let cw = if chroma == ChromaSampling::Cs444 {
        info.width
    } else {
        info.width.div_ceil(2)
    };
    let ch = if chroma == ChromaSampling::Cs420 {
        info.height.div_ceil(2)
    } else {
        info.height
    };
    let mut offset = 0;
    for (channel, (width, height)) in [(info.width, info.height), (cw, ch), (cw, ch)]
        .into_iter()
        .enumerate()
    {
        for y in 0..height {
            for x in 0..width {
                let value = 16 + channel as u16 * 32 + if x >= width / 2 { phase } else { 0 };
                framefinery_api::write_planar_sample(
                    &mut data,
                    offset + y * width + x,
                    value << (info.format.bit_depth().bits() - 8),
                    info.format.bit_depth(),
                )
                .unwrap();
            }
        }
        offset += width * height;
    }
    Frame::new(info, data).unwrap()
}

pub fn formats() -> Vec<PixelFormat> {
    (8..=12)
        .flat_map(|bits| {
            [
                PixelFormat::yuv420(bits).unwrap(),
                PixelFormat::yuv422(bits).unwrap(),
                PixelFormat::yuv444(bits).unwrap(),
            ]
        })
        .chain([PixelFormat::Gbrp8])
        .collect()
}
