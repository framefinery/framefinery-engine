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
pub use framefinery_codecs as codecs;
pub use framefinery_core as core;
#[cfg(feature = "filter-identity")]
pub use framefinery_core::IdentityFilter;
pub use framefinery_core::{
    convert_frame_format, convert_planar_frame_bit_depth, planar_sample_sse, read_planar_sample,
    run_frame_encode_pipeline, run_frame_filter_pipeline, scale_sample_bit_depth,
    write_planar_sample, ChromaSampling, Decoder, EncodePipelineStats, Encoder, Filter,
    FilterPipelineStats, Frame, FrameInfo, MediaError, Packet, PixelFormat, Result, SampleBitDepth,
    Sink, Source, StreamId, Timestamp,
};
#[cfg(feature = "filter-pattern")]
pub use framefinery_core::{
    generate_pattern_stream, pattern_frame_data, PatternKind, PatternSource,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run<I>(raw_args: I) -> ExitCode
where
    I: IntoIterator<Item = OsString>,
{
    command::run(raw_args)
}
