//! Experimental video encoders for FrameFinery.
#![cfg_attr(not(feature = "dead-code-audit"), allow(dead_code, unused_imports))]
//!
//! The public API is the generic encoder registry exposed as [`ENCODERS`],
//! [`find_encoder_manifest`], [`encoder`], [`create_encoder`],
//! [`encode_frame`], and [`encode_source`]. Codec-specific modules are
//! internal implementation territory while the generic v0 video API settles.

#[cfg(feature = "av2")]
#[doc(hidden)]
mod av2;
mod bitstream;
mod builder;
mod instrumentation;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod picture;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod session;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod settings;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod timing;
#[cfg(test)]
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

use framefinery_api::{
    Frame, MediaError, RawVideoFrameSource, Result, VideoEncodeFrameMetricsCallback,
    VideoEncodeOutput, VideoEncodeSourceRequest, VideoEncoderConfig, VideoEncoderManifest,
    VideoEncoderSession,
};

pub use builder::{encoder, VideoEncoderBuilder};
pub use framefinery_api::{ChromaSampling, PixelFormat, SampleBitDepth};
#[cfg(feature = "vvc")]
pub use vvc::VvcProfile;

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

/// Fetch a compiled video encoder manifest or return a structured error.
pub fn fetch_encoder_manifest(name: impl AsRef<str>) -> Result<VideoEncoderManifest> {
    let name = name.as_ref();
    find_encoder_manifest(name).ok_or_else(|| unsupported_codec(name))
}

/// Apply CLI-compatible encoder setting specs through the selected manifest.
///
/// This is a convenience wrapper around
/// [`VideoEncoderManifest::apply_setting_specs`] for callers that already have a
/// [`VideoEncoderConfig`] with its codec id set.
pub fn apply_encoder_settings<I, S>(
    config: VideoEncoderConfig,
    specs: I,
) -> Result<VideoEncoderConfig>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    fetch_encoder_manifest(config.codec.as_str())?.apply_setting_specs(config, specs)
}

/// Render effective encoder setting specs through the selected manifest.
///
/// This includes manifest defaults and uses the same ordering as the CLI
/// startup summary.
pub fn effective_encoder_settings(config: &VideoEncoderConfig) -> Result<Vec<String>> {
    fetch_encoder_manifest(config.codec.as_str())?.effective_setting_specs(config)
}

/// Create a persistent encoder session from a codec-neutral config.
///
/// AV2 emits complete frame chunks per call. VVC still buffers input until
/// flush, an open implementation gap against the streaming contract documented
/// on [`VideoEncoderSession`].
pub fn create_encoder(config: VideoEncoderConfig) -> Result<Box<dyn VideoEncoderSession>> {
    let manifest = fetch_encoder_manifest(config.codec.as_str())?;
    manifest.validate_config(&config)?;
    (manifest.session_factory())(config)
}

/// Encode frames pulled from `source` using the codec selected by `config`.
///
/// This path avoids buffering whole streams in memory and is intended for file,
/// capture, and validation adapters. `frame_metrics`, when present, is called
/// after each encoded frame with timing, per-frame bytes, cumulative bytes, and
/// optional PSNR while source and reconstruction samples are still available.
pub fn encode_source<'callback>(
    config: &VideoEncoderConfig,
    source: &mut dyn RawVideoFrameSource,
    output: &mut dyn Write,
    recon: Option<&mut dyn Write>,
    frame_metrics: Option<VideoEncodeFrameMetricsCallback<'callback>>,
) -> Result<()> {
    let manifest = fetch_encoder_manifest(config.codec.as_str())?;
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

pub(crate) fn unsupported_codec(codec: &str) -> MediaError {
    MediaError::UnsupportedCodec {
        codec: codec.to_string(),
        reason: "no encoder with this codec id is compiled into this build".to_string(),
    }
}

#[cfg(all(test, feature = "av2"))]
mod tests {
    use super::*;
    use framefinery_api::{
        CodecId, Frame, FrameInfo, PixelFormat, ReconstructionMode, VideoEncoderConfig,
        VideoRateControl,
    };

    #[test]
    fn generic_encoder_session_emits_av2_frames_before_terminal_flush() {
        let info = FrameInfo::new(8, 8, PixelFormat::Yuv420p8).unwrap();
        let config = VideoEncoderConfig::new(CodecId::new("av2").unwrap(), info)
            .with_rate_control(VideoRateControl::Lossless)
            .with_reconstruction(ReconstructionMode::Frames);
        let mut encoder = create_encoder(config).expect("generic av2 encoder");
        for frame_index in 0..2 {
            let output = encoder
                .encode_frame(Frame::blank(info))
                .expect("encode frame through generic session");
            assert_eq!(output.chunks.len(), 1);
            assert_eq!(
                output.chunks[0].kind,
                framefinery_api::VideoChunkKind::Frame
            );
            assert_eq!(output.chunks[0].frame_index, Some(frame_index));
            assert!(!output.chunks[0].data.is_empty());
            assert_eq!(output.reconstructions, vec![Frame::blank(info)]);
        }
        assert_eq!(encoder.flush().unwrap(), VideoEncodeOutput::default());
        assert_eq!(encoder.flush().unwrap(), VideoEncodeOutput::default());
        assert_eq!(
            encoder.encode_frame(Frame::blank(info)),
            Err(MediaError::EncodeAfterFlush)
        );
    }
}
