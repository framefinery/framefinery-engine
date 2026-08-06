//! Public FrameFinery facade crate.
//!
//! The package also ships the `ff` command-line binary. Library users can depend
//! on `framefinery` directly, while advanced users may depend on
//! `framefinery-core` or `framefinery-codecs` for narrower APIs.
//!
//! ```no_run
//! use framefinery::{encoder, Frame, FrameInfo, PixelFormat};
//!
//! # fn main() -> framefinery::Result<()> {
//! let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8)?;
//! let frame = Frame::blank(info);
//!
//! let output = encoder("av2")?
//!     .input(info)
//!     .lossless()
//!     .reconstruction_frames()
//!     .encode_frame(frame)?;
//! assert_eq!(output.reconstructions.len(), 1);
//! # Ok(())
//! # }
//! ```

mod args;
mod catalog;
mod command;
mod options;

use std::ffi::OsString;
use std::process::ExitCode;

#[cfg(feature = "video-encoders")]
pub use framefinery_codecs::{
    create_encoder, encode_frame, encode_source, encoder, find_encoder_manifest,
    VideoEncoderBuilder, ENCODERS,
};
pub use framefinery_core as core;
pub use framefinery_core::{
    build_filter_transform, build_raw_video_source_filter, build_source_filter,
    convert_frame_format, convert_planar_frame_bit_depth, filter_manifest, frame_psnr,
    generate_source_filter_stream, parse_filter_pipeline_specs, planar_sample_sse,
    read_planar_sample, run_frame_encode_pipeline, run_frame_filter_pipeline,
    scale_sample_bit_depth, write_planar_sample, ChromaSampling, CodecId, DecodedPictureBuffer,
    DpbEntry, EncodePipelineStats, EncodedVideoChunk, Encoder, Filter, FilterManifest,
    FilterPipelineBuilder, FilterPipelineSpec, FilterPipelineStats, FilterStageSpec,
    FilteredRawVideoFrameSource, Frame, FrameEncodeMetrics, FrameInfo, FramePsnr, FrameRate,
    FrameRef, FrameSourceRawVideoAdapter, MediaError, Packet, PictureId, PixelFormat,
    RawVideoFrameReadSource, RawVideoFrameSource, RawVideoFrameSourceReadAdapter,
    ReconstructionMode, Result, SampleBitDepth, Sink, Source, StreamId, Timestamp, VideoChunkKind,
    VideoEncodeFrameMetrics, VideoEncodeFrameMetricsCallback, VideoEncodeOutput,
    VideoEncoderConfig, VideoEncoderManifest, VideoEncoderSession, VideoEncoderSetting,
    VideoRateControl, VideoSettingValue, FILTERS,
};
pub use options::{
    cli_options, cli_options_for_scope, CliOptionManifest, CliOptionScope, CliOptionValue,
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
