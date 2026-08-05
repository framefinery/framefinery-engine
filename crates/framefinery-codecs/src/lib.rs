//! Experimental video encoders for FrameFinery.
//!
//! The public API is the generic encoder registry exposed as [`ENCODERS`] and
//! [`encoder`]. Codec-specific modules remain available for internal
//! experiments, but applications should prefer the generic v0 video API while
//! it settles.

#[cfg(feature = "av2")]
#[doc(hidden)]
pub mod av2;
pub mod bitstream;
pub mod instrumentation;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod picture;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod session;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod settings;
pub mod trace;
#[cfg(feature = "vvc")]
#[doc(hidden)]
pub mod vvc;

use framefinery_core::{
    MediaError, Result, VideoEncoderConfig, VideoEncoderManifest, VideoEncoderSession,
};

pub use framefinery_core::{ChromaSampling, PixelFormat, SampleBitDepth};

pub const ENCODERS: &[VideoEncoderManifest] = &[
    #[cfg(feature = "av2")]
    av2::AV2_CODEC,
    #[cfg(feature = "vvc")]
    vvc::VVC_CODEC,
];

pub fn encoder(name: &str) -> Option<VideoEncoderManifest> {
    ENCODERS
        .iter()
        .copied()
        .find(|encoder| encoder.name == name)
}

pub fn create_encoder(config: VideoEncoderConfig) -> Result<Box<dyn VideoEncoderSession>> {
    let Some(encoder) = encoder(config.codec.as_str()) else {
        return Err(MediaError::Unsupported {
            feature: format!("codec '{}'", config.codec),
            reason: "no encoder with this codec id is compiled into this build".to_string(),
        });
    };
    encoder.create_encoder(config)
}

#[cfg(all(test, feature = "av2"))]
mod tests {
    use super::*;
    use framefinery_core::{
        CodecId, Frame, FrameInfo, PixelFormat, ReconstructionMode, VideoEncoderConfig,
        VideoRateControl,
    };

    #[test]
    fn generic_encoder_session_encodes_buffered_av2_stream() {
        let info = FrameInfo::new(8, 8, PixelFormat::Yuv420p8).unwrap();
        let config = VideoEncoderConfig::new(CodecId::new("av2").unwrap(), info)
            .with_rate_control(VideoRateControl::Lossless)
            .with_reconstruction(ReconstructionMode::Frames);
        let mut encoder = create_encoder(config).expect("generic av2 encoder");
        encoder
            .encode_frame(Frame::blank(info))
            .expect("queue frame through generic session");
        let output = encoder.flush().expect("flush generic session");

        assert_eq!(output.chunks.len(), 1);
        assert!(!output.chunks[0].data.is_empty());
        assert_eq!(output.reconstructions.len(), 1);
        assert_eq!(output.reconstructions[0], Frame::blank(info));
    }
}
