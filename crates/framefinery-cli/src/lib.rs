//! Public FrameFinery facade crate.
//!
//! The package also ships the `ff` command-line binary. Library users can depend
//! on `framefinery` directly, while advanced users may depend on
//! `framefinery-core` or `framefinery-codecs` for narrower APIs.

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
    generate_source_filter_stream, parse_filter_pipeline_specs, planar_sample_sse,
    read_planar_sample, run_frame_encode_pipeline, run_frame_filter_pipeline,
    scale_sample_bit_depth, write_planar_sample, ChromaSampling, CodecId, Decoder,
    EncodePipelineStats, EncodedVideoChunk, Encoder, Filter, FilterManifest, FilterPipelineSpec,
    FilterPipelineStats, FilterStageSpec, Frame, FrameEncodeMetrics, FrameInfo, FrameRate,
    MediaError, Packet, PixelFormat, ReconstructionMode, Result, SampleBitDepth, Sink, Source,
    StreamId, Timestamp, VideoChunkKind, VideoEncodeOutput, VideoEncoderConfig,
    VideoEncoderManifest, VideoEncoderSession, VideoEncoderSetting, VideoRateControl,
    VideoSettingValue, FILTERS,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run<I>(raw_args: I) -> ExitCode
where
    I: IntoIterator<Item = OsString>,
{
    command::run(raw_args)
}
