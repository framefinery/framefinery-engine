//! Public FrameFinery facade crate.
//!
//! The package also ships the `ff` command-line binary. Library users can depend
//! on `framefinery` directly, while advanced users may depend on
//! `framefinery-core` or `framefinery-codecs` for narrower APIs.
//!
//! ```no_run
//! use framefinery::{
//!     encode_frame, CodecId, Frame, FrameInfo, PixelFormat, ReconstructionMode,
//!     VideoEncoderConfig, VideoRateControl,
//! };
//!
//! # fn main() -> framefinery::Result<()> {
//! let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8)?;
//! let config = VideoEncoderConfig::new(CodecId::new("av2")?, info)
//!     .with_rate_control(VideoRateControl::Lossless)
//!     .with_reconstruction(ReconstructionMode::Frames);
//! let frame = Frame::blank(info);
//!
//! let output = encode_frame(config, frame)?;
//! assert_eq!(output.reconstructions.len(), 1);
//! # Ok(())
//! # }
//! ```

mod args;
mod catalog;
mod command;

use std::ffi::OsString;
use std::process::ExitCode;

#[cfg(feature = "video-encoders")]
pub use framefinery_codecs::{
    create_encoder, encode_frame, encode_source, find_encoder_manifest, ENCODERS,
};
pub use framefinery_core as core;
pub use framefinery_core::{
    build_filter_transform, convert_frame_format, convert_planar_frame_bit_depth, filter_manifest,
    frame_psnr, generate_source_filter_stream, parse_filter_pipeline_specs, planar_sample_sse,
    read_planar_sample, run_frame_encode_pipeline, run_frame_filter_pipeline,
    scale_sample_bit_depth, write_planar_sample, ChromaSampling, CodecId, DecodedPictureBuffer,
    DpbEntry, EncodePipelineStats, EncodedVideoChunk, Encoder, Filter, FilterManifest,
    FilterPipelineSpec, FilterPipelineStats, FilterStageSpec, Frame, FrameEncodeMetrics, FrameInfo,
    FramePsnr, FrameRate, FrameRef, MediaError, Packet, PictureId, PixelFormat,
    RawVideoFrameReadSource, RawVideoFrameSource, ReconstructionMode, Result, SampleBitDepth, Sink,
    Source, StreamId, Timestamp, VideoChunkKind, VideoEncodeFrameMetrics,
    VideoEncodeFrameMetricsCallback, VideoEncodeOutput, VideoEncoderConfig, VideoEncoderManifest,
    VideoEncoderSession, VideoEncoderSetting, VideoRateControl, VideoSettingValue, FILTERS,
};

/// Version of the `framefinery` facade crate and `ff` binary.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Run the `ff` command-line frontend with an iterator of raw OS arguments.
pub fn run<I>(raw_args: I) -> ExitCode
where
    I: IntoIterator<Item = OsString>,
{
    command::run(raw_args)
}
