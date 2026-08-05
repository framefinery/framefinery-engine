//! Core media primitives for FrameFinery.
//!
//! This crate is intentionally small at project bootstrap. It holds the stable
//! concepts that codec crates and tools can share without forcing AV2, VVC, or
//! future codecs into one internal design.

pub mod error;
pub mod filters;
pub mod frame;
pub mod packet;
pub mod pipeline;
pub mod settings;

pub use error::{MediaError, Result};
#[cfg(feature = "filter-identity")]
pub use filters::IdentityFilter;
pub use filters::{filter_manifest, FilterManifest, FilterStageKind, FilterStatus, FILTERS};
pub use filters::{
    filter_spec_manifest, FilterSpecExample, FilterSpecForm, FilterSpecManifest,
    FilterSpecParameter, FilterSpecValue, CROP_FILTER_SPEC, IDENTITY_FILTER_SPEC,
    PATTERN_FILTER_SPEC, PATTERN_SOURCE_NAMES, SCALE_FILTER_SPEC,
};
#[cfg(feature = "filter-pattern")]
pub use filters::{generate_pattern_stream, pattern_frame_data, PatternKind, PatternSource};
pub use frame::{
    convert_frame_format, convert_planar_frame_bit_depth, planar_sample_sse, read_planar_sample,
    scale_sample_bit_depth, write_planar_sample, ChromaSampling, Frame, FrameInfo, PixelFormat,
    SampleBitDepth,
};
pub use packet::{Packet, StreamId, Timestamp};
pub use pipeline::{
    run_frame_encode_pipeline, run_frame_filter_pipeline, Decoder, EncodePipelineStats, Encoder,
    Filter, FilterPipelineStats, Sink, Source,
};
pub use settings::{
    boolean_setting_enabled, setting_name, setting_value, setting_values_label, u8_setting,
    CodecEncodeFn, CodecEncodeFrameMetrics, CodecEncodeFrameMetricsCallback, CodecEncodeRequest,
    CodecManifest, SettingManifest, SettingSpecExample, SettingSpecForm, SettingSpecManifest,
    SettingValue, GLOBAL_SETTINGS, LOSSLESS_SETTING, LOSSLESS_SETTING_SPEC,
};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
