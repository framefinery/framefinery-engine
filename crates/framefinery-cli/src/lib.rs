//! Public FrameFinery facade crate.
//!
//! The package also ships the `ff` command-line binary. Library users can depend
//! on `framefinery` directly, while advanced users may depend on
//! `framefinery-core` or `framefinery-codecs` for narrower APIs.
//!
//! ```no_run
//! use framefinery::{
//!     encoder, CodecId, FrameInfo, PixelFormat, RawVideoFrameReadSource, VideoEncoderConfig,
//! };
//!
//! # fn main() -> framefinery::Result<()> {
//! let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8)?;
//! let config = VideoEncoderConfig::new(CodecId::new("av2")?, info);
//! let pixels = vec![0; info.expected_len()];
//! let mut source = RawVideoFrameReadSource::new(std::io::Cursor::new(pixels));
//! let mut bitstream = Vec::new();
//!
//! let codec = encoder("av2").expect("the av2 feature is enabled");
//! codec.encode_source(&mut source, &mut bitstream, None, &config, None)?;
//! # Ok(())
//! # }
//! ```

mod args;
mod catalog;
mod command;

use std::ffi::OsString;
use std::process::ExitCode;

#[cfg(feature = "video-encoders")]
pub use framefinery_codecs::{create_encoder, encoder, ENCODERS};
pub use framefinery_core as core;
pub use framefinery_core::{
    build_filter_transform, convert_frame_format, convert_planar_frame_bit_depth, filter_manifest,
    frame_psnr, generate_source_filter_stream, parse_filter_pipeline_specs, planar_sample_sse,
    read_planar_sample, run_frame_encode_pipeline, run_frame_filter_pipeline,
    scale_sample_bit_depth, write_planar_sample, ChromaSampling, CodecId, DecodedPictureBuffer,
    Decoder, DpbEntry, EncodePipelineStats, EncodedVideoChunk, Encoder, Filter, FilterManifest,
    FilterPipelineSpec, FilterPipelineStats, FilterStageSpec, Frame, FrameEncodeMetrics, FrameInfo,
    FramePsnr, FrameRate, FrameRef, MediaError, Packet, PictureId, PixelFormat,
    RawVideoFrameReadSource, RawVideoFrameSource, ReconstructionMode, Result, SampleBitDepth, Sink,
    Source, StreamId, Timestamp, VideoChunkKind, VideoEncodeFrameMetrics,
    VideoEncodeFrameMetricsCallback, VideoEncodeOutput, VideoEncodeSourceFn,
    VideoEncodeSourceRequest, VideoEncoderConfig, VideoEncoderManifest, VideoEncoderSession,
    VideoEncoderSetting, VideoRateControl, VideoSettingValue, FILTERS,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run<I>(raw_args: I) -> ExitCode
where
    I: IntoIterator<Item = OsString>,
{
    command::run(raw_args)
}
