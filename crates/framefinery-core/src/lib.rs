//! Core media primitives for FrameFinery.
//!
//! This crate is intentionally small at project bootstrap. It holds the stable
//! concepts that codec crates and tools can share without forcing AV2, VVC, or
//! future codecs into one internal design.
//!
//! ```
//! use framefinery_core::{Frame, FrameInfo, PixelFormat};
//!
//! let info = FrameInfo::new(16, 16, PixelFormat::Yuv420p8)?;
//! let frame = Frame::blank(info);
//! assert_eq!(frame.as_frame_ref().data().len(), info.expected_len());
//! # Ok::<(), framefinery_core::MediaError>(())
//! ```

mod dpb;
mod error;
mod filters;
mod frame;
mod packet;
mod pipeline;
mod settings;
mod video;

pub use dpb::{DecodedPictureBuffer, DpbEntry, PictureId};
pub use error::{MediaError, Result};
#[cfg(feature = "filter-identity")]
pub use filters::IdentityFilter;
pub use filters::{
    build_filter_transform, filter_spec_manifest, filter_spec_name, generate_source_filter_stream,
    parse_filter_pipeline_specs, FilterPipelineSpec, FilterSpecExample, FilterSpecForm,
    FilterSpecManifest, FilterSpecParameter, FilterSpecValue, FilterStageSpec, CROP_FILTER_SPEC,
    IDENTITY_FILTER_SPEC, PATTERN_FILTER_SPEC, PATTERN_SOURCE_NAMES, SCALE_FILTER_SPEC,
};
pub use filters::{filter_manifest, FilterManifest, FilterStageKind, FilterStatus, FILTERS};
#[cfg(feature = "filter-pattern")]
pub use filters::{generate_pattern_stream, pattern_frame_data, PatternKind, PatternSource};
pub use frame::{
    convert_frame_format, convert_planar_frame_bit_depth, frame_psnr, planar_sample_sse,
    read_planar_sample, scale_sample_bit_depth, write_planar_sample, ChromaSampling, Frame,
    FrameInfo, FramePsnr, FrameRef, PixelFormat, SampleBitDepth,
};
pub use packet::{Packet, StreamId, Timestamp};
pub use pipeline::{
    run_frame_encode_pipeline, run_frame_filter_pipeline, EncodePipelineStats, Encoder, Filter,
    FilterPipelineStats, Sink, Source,
};
pub use settings::{
    boolean_setting_enabled, setting_name, setting_value, setting_values_label, u8_setting,
    SettingManifest, SettingSpecExample, SettingSpecForm, SettingSpecManifest, SettingValue,
    GLOBAL_SETTINGS, LOSSLESS_SETTING, LOSSLESS_SETTING_SPEC,
};
pub use video::{
    CodecId, EncodedVideoChunk, FrameEncodeMetrics, FrameRate, RawVideoFrameReadSource,
    RawVideoFrameSource, ReconstructionMode, VideoChunkKind, VideoEncodeFrameMetrics,
    VideoEncodeFrameMetricsCallback, VideoEncodeOutput, VideoEncoderConfig, VideoEncoderManifest,
    VideoEncoderSession, VideoEncoderSetting, VideoRateControl, VideoSettingValue,
};
#[doc(hidden)]
pub use video::{VideoEncodeSourceFn, VideoEncodeSourceRequest, VideoEncoderSessionFactory};

/// Version of the `framefinery-core` crate compiled into this build.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
