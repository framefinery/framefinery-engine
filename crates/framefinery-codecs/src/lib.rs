//! Experimental video encoders for FrameFinery.
#![cfg_attr(not(feature = "dead-code-audit"), allow(dead_code, unused_imports))]
//!
//! The public API is the generic encoder registry exposed as [`ENCODERS`],
//! [`find_encoder_manifest`], [`create_encoder`], [`encode_frame`], and
//! [`encode_source`]. Codec-specific modules are internal implementation
//! territory while the generic v0 video API settles.

#[cfg(feature = "av2")]
#[doc(hidden)]
mod av2;
mod bitstream;
mod instrumentation;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod picture;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod session;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod settings;
mod trace;
#[cfg(feature = "vvc")]
#[doc(hidden)]
mod vvc;

#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub mod bench {
    #[cfg(feature = "av2")]
    pub mod av2 {
        pub use crate::av2::{bench, Av2VideoGeometry};
    }

    #[cfg(feature = "vvc")]
    pub mod vvc {
        pub use crate::vvc::{bench, VvcVideoGeometry};
    }
}

use std::io::Write;

use framefinery_core::{
    Frame, MediaError, RawVideoFrameSource, Result, VideoEncodeFrameMetricsCallback,
    VideoEncodeOutput, VideoEncodeSourceRequest, VideoEncoderConfig, VideoEncoderManifest,
    VideoEncoderSession,
};

pub use framefinery_core::{ChromaSampling, PixelFormat, SampleBitDepth};

/// Video encoder manifests compiled into this build.
pub const ENCODERS: &[VideoEncoderManifest] = &[
    #[cfg(feature = "av2")]
    av2::AV2_CODEC,
    #[cfg(feature = "vvc")]
    vvc::VVC_CODEC,
];

/// Find a compiled video encoder manifest by codec id.
pub fn find_encoder_manifest(name: &str) -> Option<VideoEncoderManifest> {
    ENCODERS
        .iter()
        .copied()
        .find(|encoder| encoder.name == name)
}

/// Create a buffered encoder session from a codec-neutral config.
pub fn create_encoder(config: VideoEncoderConfig) -> Result<Box<dyn VideoEncoderSession>> {
    let Some(manifest) = find_encoder_manifest(config.codec.as_str()) else {
        return Err(unsupported_codec(config.codec.as_str()));
    };
    manifest.validate_config(&config)?;
    (manifest.session_factory())(config)
}

/// Encode frames pulled from `source` using the codec selected by `config`.
///
/// This path avoids buffering whole streams in memory and is intended for file,
/// capture, and validation adapters. `frame_metrics`, when present, is called
/// while source and reconstruction samples are still available internally.
pub fn encode_source<'a>(
    config: &'a VideoEncoderConfig,
    source: &mut dyn RawVideoFrameSource,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    frame_metrics: Option<VideoEncodeFrameMetricsCallback<'a>>,
) -> Result<()> {
    let Some(manifest) = find_encoder_manifest(config.codec.as_str()) else {
        return Err(unsupported_codec(config.codec.as_str()));
    };
    manifest.validate_config(config)?;
    (manifest.source_encode_hook())(
        source,
        output,
        recon,
        VideoEncodeSourceRequest { config },
        frame_metrics,
    )
}

/// Encode one frame using the codec selected by `config`.
///
/// This is the convenience path for one-frame callers. It creates a session,
/// submits `frame`, flushes the encoder, and returns the combined output.
pub fn encode_frame(config: VideoEncoderConfig, frame: Frame) -> Result<VideoEncodeOutput> {
    let mut encoder = create_encoder(config)?;
    let mut output = encoder.encode_frame(frame)?;
    let tail = encoder.flush()?;
    output.chunks.extend(tail.chunks);
    output.reconstructions.extend(tail.reconstructions);
    output.metrics.extend(tail.metrics);
    Ok(output)
}

fn unsupported_codec(codec: &str) -> MediaError {
    MediaError::UnsupportedCodec {
        codec: codec.to_string(),
        reason: "no encoder with this codec id is compiled into this build".to_string(),
    }
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
