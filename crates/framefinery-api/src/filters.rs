#[cfg(feature = "filter-pattern")]
use std::str::FromStr;

use crate::error::MediaError;
use crate::error::Result;
use crate::pipeline::{Filter, FrameSourceRawVideoAdapter, Source};
#[cfg(any(
    feature = "filter-pattern",
    feature = "filter-crop",
    feature = "filter-scale"
))]
use crate::ChromaSampling;
use crate::Frame;
use crate::FrameInfo;
#[cfg(any(
    feature = "filter-pattern",
    feature = "filter-crop",
    feature = "filter-scale"
))]
use crate::PixelFormat;
use crate::RawVideoFrameSource;
#[cfg(feature = "filter-pattern")]
use crate::SampleBitDepth;
#[cfg(feature = "filter-pattern")]
use crate::{scale_sample_bit_depth, write_planar_sample};

/// Pipeline position served by a filter manifest entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterStageKind {
    /// A source filter generates frames and replaces file input.
    Source,
    /// A transform filter consumes and emits frames.
    Transform,
}

/// Implementation status for a declared filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterStatus {
    /// The filter can be constructed and executed.
    Implemented,
    /// The filter is declared for help/discovery but execution is not ready.
    Scaffold,
}

/// Value shape for one parameter in a filter spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterSpecValue {
    /// Parameter must be one of the listed strings.
    Choice(&'static [&'static str]),
    /// Parameter must be an integer greater than zero.
    PositiveInteger,
    /// Parameter must be an integer greater than or equal to zero.
    UnsignedInteger,
}

/// One accepted textual form for a filter spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterSpecForm {
    /// User-facing syntax, such as `pattern=<name>`.
    pub syntax: &'static str,
    /// Short explanation of the syntax form.
    pub summary: &'static str,
}

/// One named parameter accepted by a filter spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterSpecParameter {
    /// Parameter name used in the spec string.
    pub name: &'static str,
    /// Placeholder name shown for the parameter value.
    pub value_name: &'static str,
    /// Whether this parameter is required.
    pub required: bool,
    /// Value shape accepted by this parameter.
    pub value: FilterSpecValue,
    /// Short explanation of the parameter.
    pub summary: &'static str,
}

/// One example filter spec for help and generated documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterSpecExample {
    /// Complete filter spec string.
    pub spec: &'static str,
    /// Short explanation of the example.
    pub summary: &'static str,
}

/// Documentation manifest for one filter's accepted spec strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterSpecManifest {
    /// Supported syntax forms.
    pub forms: &'static [FilterSpecForm],
    /// Declared parameters.
    pub parameters: &'static [FilterSpecParameter],
    /// Example specs.
    pub examples: &'static [FilterSpecExample],
    /// Additional behavior notes.
    pub notes: &'static [&'static str],
}

/// Public manifest entry for a compiled filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterManifest {
    /// Stable filter name.
    pub name: &'static str,
    /// Pipeline position served by this filter.
    pub stage: FilterStageKind,
    /// Cargo feature that enables this filter.
    pub feature: &'static str,
    /// Whether the filter executes today or is only a scaffold.
    pub status: FilterStatus,
    /// Structured spec/help manifest.
    pub spec: &'static FilterSpecManifest,
    /// Short user-facing summary.
    pub summary: &'static str,
}

impl FilterManifest {
    /// Return a stable string label for this filter's implementation status.
    pub const fn implementation_status(self) -> &'static str {
        match self.status {
            FilterStatus::Implemented => "implemented",
            FilterStatus::Scaffold => "scaffold",
        }
    }
}

/// Canonical pattern source names accepted by the `pattern` source filter.
pub const PATTERN_SOURCE_NAMES: &[&str] = &[
    "black",
    "checker",
    "gradient",
    "color_blocks",
    "bitdepth_canary",
];

const PATTERN_SPEC_FORMS: &[FilterSpecForm] = &[
    FilterSpecForm {
        syntax: "pattern=<name>",
        summary: "generate frames from a named deterministic pattern",
    },
    FilterSpecForm {
        syntax: "pattern:<name>",
        summary: "alternate spelling accepted by the parser",
    },
];

const PATTERN_SPEC_PARAMETERS: &[FilterSpecParameter] = &[FilterSpecParameter {
    name: "name",
    value_name: "pattern",
    required: true,
    value: FilterSpecValue::Choice(PATTERN_SOURCE_NAMES),
    summary: "pattern sequence to generate",
}];

const PATTERN_SPEC_EXAMPLES: &[FilterSpecExample] = &[
    FilterSpecExample {
        spec: "pattern=black",
        summary: "generate black frames",
    },
    FilterSpecExample {
        spec: "pattern=color_blocks",
        summary: "generate moving color-block frames",
    },
];

const PATTERN_SPEC_NOTES: &[&str] = &[
    "source filters must be first and cannot be combined with an input path",
    "source filters require --video metadata and --frames",
    "currently generates planar YUV/gray frames across supported bit depths, gbrp8, and rgb24",
    "blocks is accepted as a short alias for color_blocks",
];

/// Spec manifest for the `pattern` source filter.
pub const PATTERN_FILTER_SPEC: FilterSpecManifest = FilterSpecManifest {
    forms: PATTERN_SPEC_FORMS,
    parameters: PATTERN_SPEC_PARAMETERS,
    examples: PATTERN_SPEC_EXAMPLES,
    notes: PATTERN_SPEC_NOTES,
};

const IDENTITY_SPEC_FORMS: &[FilterSpecForm] = &[FilterSpecForm {
    syntax: "identity",
    summary: "pass frames through unchanged",
}];

const IDENTITY_SPEC_EXAMPLES: &[FilterSpecExample] = &[FilterSpecExample {
    spec: "identity",
    summary: "exercise the frame filter pipeline without changing pixels",
}];

/// Spec manifest for the `identity` transform filter.
pub const IDENTITY_FILTER_SPEC: FilterSpecManifest = FilterSpecManifest {
    forms: IDENTITY_SPEC_FORMS,
    parameters: &[],
    examples: IDENTITY_SPEC_EXAMPLES,
    notes: &[],
};

const CROP_SPEC_FORMS: &[FilterSpecForm] = &[FilterSpecForm {
    syntax: "crop=x=<px>:y=<px>:w=<px>:h=<px>",
    summary: "crop a rectangular frame region",
}];

const CROP_SPEC_PARAMETERS: &[FilterSpecParameter] = &[
    FilterSpecParameter {
        name: "x",
        value_name: "px",
        required: true,
        value: FilterSpecValue::UnsignedInteger,
        summary: "left coordinate of the crop rectangle",
    },
    FilterSpecParameter {
        name: "y",
        value_name: "px",
        required: true,
        value: FilterSpecValue::UnsignedInteger,
        summary: "top coordinate of the crop rectangle",
    },
    FilterSpecParameter {
        name: "w",
        value_name: "px",
        required: true,
        value: FilterSpecValue::PositiveInteger,
        summary: "crop width",
    },
    FilterSpecParameter {
        name: "h",
        value_name: "px",
        required: true,
        value: FilterSpecValue::PositiveInteger,
        summary: "crop height",
    },
];

const CROP_SPEC_EXAMPLES: &[FilterSpecExample] = &[FilterSpecExample {
    spec: "crop=x=0:y=0:w=640:h=360",
    summary: "select a 640x360 region from the top-left corner",
}];

const CROP_SPEC_NOTES: &[&str] = &[
    "subsampled formats require crop coordinates and dimensions aligned to chroma samples",
    "the output keeps the input pixel format",
];

/// Spec manifest for the `crop` transform filter.
pub const CROP_FILTER_SPEC: FilterSpecManifest = FilterSpecManifest {
    forms: CROP_SPEC_FORMS,
    parameters: CROP_SPEC_PARAMETERS,
    examples: CROP_SPEC_EXAMPLES,
    notes: CROP_SPEC_NOTES,
};

const SCALE_SPEC_FORMS: &[FilterSpecForm] = &[FilterSpecForm {
    syntax: "scale=w=<px>:h=<px>",
    summary: "resize frames to the requested output geometry",
}];

const SCALE_SPEC_PARAMETERS: &[FilterSpecParameter] = &[
    FilterSpecParameter {
        name: "w",
        value_name: "px",
        required: true,
        value: FilterSpecValue::PositiveInteger,
        summary: "output width",
    },
    FilterSpecParameter {
        name: "h",
        value_name: "px",
        required: true,
        value: FilterSpecValue::PositiveInteger,
        summary: "output height",
    },
];

const SCALE_SPEC_EXAMPLES: &[FilterSpecExample] = &[FilterSpecExample {
    spec: "scale=w=1280:h=720",
    summary: "resize frames to 1280x720",
}];

const SCALE_SPEC_NOTES: &[&str] = &[
    "uses deterministic nearest-neighbor sampling",
    "the output keeps the input pixel format",
];

/// Spec manifest for the `scale` transform filter.
pub const SCALE_FILTER_SPEC: FilterSpecManifest = FilterSpecManifest {
    forms: SCALE_SPEC_FORMS,
    parameters: SCALE_SPEC_PARAMETERS,
    examples: SCALE_SPEC_EXAMPLES,
    notes: SCALE_SPEC_NOTES,
};

/// Filter manifests compiled into this build.
pub const FILTERS: &[FilterManifest] = &[
    #[cfg(feature = "filter-pattern")]
    FilterManifest {
        name: "pattern",
        stage: FilterStageKind::Source,
        feature: "filter-pattern",
        status: FilterStatus::Implemented,
        spec: &PATTERN_FILTER_SPEC,
        summary: "generated raw-video pattern source",
    },
    #[cfg(feature = "filter-identity")]
    FilterManifest {
        name: "identity",
        stage: FilterStageKind::Transform,
        feature: "filter-identity",
        status: FilterStatus::Implemented,
        spec: &IDENTITY_FILTER_SPEC,
        summary: "no-op frame pass-through transform",
    },
    #[cfg(feature = "filter-crop")]
    FilterManifest {
        name: "crop",
        stage: FilterStageKind::Transform,
        feature: "filter-crop",
        status: FilterStatus::Implemented,
        spec: &CROP_FILTER_SPEC,
        summary: "rectangular crop transform",
    },
    #[cfg(feature = "filter-scale")]
    FilterManifest {
        name: "scale",
        stage: FilterStageKind::Transform,
        feature: "filter-scale",
        status: FilterStatus::Implemented,
        spec: &SCALE_FILTER_SPEC,
        summary: "nearest-neighbor resize transform",
    },
];

/// Find a compiled filter manifest by name.
pub fn filter_manifest(name: &str) -> Option<FilterManifest> {
    FILTERS.iter().copied().find(|filter| filter.name == name)
}

/// Find a compiled filter's spec manifest by name.
pub fn filter_spec_manifest(name: &str) -> Option<&'static FilterSpecManifest> {
    filter_manifest(name).map(|filter| filter.spec)
}

/// Parsed filter stage spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterStageSpec {
    /// Filter name parsed from `spec`.
    pub name: String,
    /// Original full filter spec string.
    pub spec: String,
}

/// Parsed filter pipeline specification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterPipelineSpec {
    /// Optional source filter. Present only when no file input is used.
    pub source: Option<FilterStageSpec>,
    /// Ordered transform filters.
    pub transforms: Vec<FilterStageSpec>,
}

/// Builder for a parsed filter pipeline specification.
///
/// This is a small API wrapper around the same filter manifest validation used
/// by the CLI. It intentionally stores textual filter specs because filter
/// syntax is still the stable interchange form between command-line arguments,
/// manifests, validation files, and future frontends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterPipelineBuilder {
    input_present: bool,
    specs: Vec<String>,
}

impl FilterStageSpec {
    /// Parse one raw filter spec into a stage spec.
    pub fn new(spec: impl Into<String>) -> Result<Self> {
        let spec = spec.into();
        let name = filter_spec_name(&spec).to_string();
        if name.is_empty() {
            return Err(MediaError::Message(
                "filter spec cannot be empty".to_string(),
            ));
        }
        Ok(Self { name, spec })
    }
}

impl FilterPipelineSpec {
    /// Start building a pipeline that consumes an external input frame stream.
    pub fn from_input() -> FilterPipelineBuilder {
        FilterPipelineBuilder::from_input()
    }

    /// Start building a pipeline that must begin with a source filter.
    pub fn from_source_filter() -> FilterPipelineBuilder {
        FilterPipelineBuilder::from_source_filter()
    }

    /// Return the frame metadata produced by this pipeline's transform stages.
    pub fn output_info(&self, input: FrameInfo) -> Result<FrameInfo> {
        filter_pipeline_output_info(input, &self.transforms)
    }

    /// Build executable transform filters for this pipeline.
    pub fn build_transforms(&self) -> Result<Vec<Box<dyn Filter>>> {
        self.transforms
            .iter()
            .map(build_filter_transform)
            .collect::<Result<Vec<_>>>()
    }

    /// Build the executable source filter, when this pipeline has one.
    pub fn build_source(
        &self,
        info: FrameInfo,
        frames: usize,
    ) -> Result<Option<Box<dyn Source<Output = Frame>>>> {
        self.source
            .as_ref()
            .map(|source| build_source_filter(source, info, frames))
            .transpose()
    }

    /// Build the executable source filter as a raw-frame callback, when present.
    pub fn build_raw_video_source(
        &self,
        info: FrameInfo,
        frames: usize,
    ) -> Result<Option<Box<dyn RawVideoFrameSource>>> {
        self.source
            .as_ref()
            .map(|source| build_raw_video_source_filter(source, info, frames))
            .transpose()
    }
}

impl FilterPipelineBuilder {
    /// Start building a pipeline that consumes an external input frame stream.
    pub fn from_input() -> Self {
        Self {
            input_present: true,
            specs: Vec::new(),
        }
    }

    /// Start building a pipeline that must begin with a source filter.
    pub fn from_source_filter() -> Self {
        Self {
            input_present: false,
            specs: Vec::new(),
        }
    }

    /// Set whether this pipeline consumes an external input frame stream.
    pub fn with_input_present(mut self, input_present: bool) -> Self {
        self.input_present = input_present;
        self
    }

    /// Append one textual filter spec.
    pub fn filter(mut self, spec: impl Into<String>) -> Result<Self> {
        let stage = FilterStageSpec::new(spec)?;
        self.specs.push(stage.spec);
        Ok(self)
    }

    /// Append several textual filter specs.
    pub fn filters<I, S>(mut self, specs: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for spec in specs {
            self = self.filter(spec)?;
        }
        Ok(self)
    }

    /// Parse and validate this builder into a pipeline specification.
    pub fn build(self) -> Result<FilterPipelineSpec> {
        parse_filter_pipeline_specs(&self.specs, self.input_present)
    }
}

/// Return the filter name portion of a filter spec string.
pub fn filter_spec_name(spec: &str) -> &str {
    spec.split_once('=')
        .or_else(|| spec.split_once(':'))
        .map_or(spec, |(name, _)| name)
}

/// Parse and validate an ordered set of filter specs for an encode pipeline.
pub fn parse_filter_pipeline_specs(
    specs: &[String],
    input_present: bool,
) -> Result<FilterPipelineSpec> {
    let mut source = None;
    let mut transforms = Vec::new();
    for (index, spec) in specs.iter().enumerate() {
        let stage = FilterStageSpec::new(spec.clone())?;
        let Some(manifest) = filter_manifest(&stage.name) else {
            return Err(MediaError::Message(format!(
                "unknown filter '{}'",
                stage.name
            )));
        };

        match manifest.stage {
            FilterStageKind::Source => {
                if input_present {
                    return Err(MediaError::Message(format!(
                        "source filter '{}' cannot be used after an input path",
                        stage.name
                    )));
                }
                if index != 0 {
                    return Err(MediaError::Message(format!(
                        "source filter '{}' must be the first filter",
                        stage.name
                    )));
                }
                if source.is_some() {
                    return Err(MediaError::Message(
                        "encode accepts only one source filter".to_string(),
                    ));
                }
                validate_executable_filter_spec(manifest, &stage)?;
                source = Some(stage);
            }
            FilterStageKind::Transform => {
                validate_executable_filter_spec(manifest, &stage)?;
                transforms.push(stage);
            }
        }
    }
    if !input_present && source.is_none() {
        return Err(MediaError::Message(
            "encode without an input requires a source filter such as --filter pattern=black"
                .to_string(),
        ));
    }
    Ok(FilterPipelineSpec { source, transforms })
}

/// Return the frame metadata produced by an ordered transform filter pipeline.
pub fn filter_pipeline_output_info(
    input: FrameInfo,
    transforms: &[FilterStageSpec],
) -> Result<FrameInfo> {
    let mut info = input;
    for stage in transforms {
        let Some(manifest) = filter_manifest(&stage.name) else {
            return Err(MediaError::Message(format!(
                "unknown filter '{}'",
                stage.name
            )));
        };
        if manifest.stage != FilterStageKind::Transform {
            return Err(MediaError::Message(format!(
                "filter '{}' is not a transform filter",
                stage.name
            )));
        }
        validate_executable_filter_spec(manifest, stage)?;
        info = filter_transform_output_info(stage, info)?;
    }
    Ok(info)
}

/// Generate raw video bytes from an executable source filter.
///
/// This helper materializes the requested frames into one `Vec<u8>` and is
/// intended for short fixtures and tests. Long streams should use
/// [`build_raw_video_source_filter`] or [`FilterPipelineSpec::build_raw_video_source`].
pub fn generate_source_filter_stream(
    stage: &FilterStageSpec,
    info: FrameInfo,
    frames: usize,
) -> Result<Vec<u8>> {
    let total_len = info
        .expected_len()
        .checked_mul(frames)
        .ok_or(MediaError::LengthOverflow)?;
    let mut output = Vec::with_capacity(total_len);
    let mut source = build_source_filter(stage, info, frames)?;
    while let Some(frame) = source.pull()? {
        output.extend_from_slice(frame.data());
    }
    Ok(output)
}

/// Build an executable source filter from a parsed stage spec.
pub fn build_source_filter(
    stage: &FilterStageSpec,
    info: FrameInfo,
    frames: usize,
) -> Result<Box<dyn Source<Output = Frame>>> {
    match stage.name.as_str() {
        "pattern" => build_pattern_filter_source(&stage.spec, info, frames),
        other => Err(MediaError::Message(format!(
            "filter '{other}' is not an executable source filter"
        ))),
    }
}

/// Build an executable source filter as a raw-frame callback.
pub fn build_raw_video_source_filter(
    stage: &FilterStageSpec,
    info: FrameInfo,
    frames: usize,
) -> Result<Box<dyn RawVideoFrameSource>> {
    match stage.name.as_str() {
        "pattern" => build_pattern_raw_video_source(&stage.spec, info, frames),
        _ => {
            let source = build_source_filter(stage, info, frames)?;
            Ok(Box::new(FrameSourceRawVideoAdapter::new(source, info)))
        }
    }
}

/// Build an executable transform filter from a parsed stage spec.
pub fn build_filter_transform(stage: &FilterStageSpec) -> Result<Box<dyn Filter>> {
    match stage.name.as_str() {
        "identity" => build_identity_filter(),
        "crop" => build_crop_filter(&stage.spec),
        "scale" => build_scale_filter(&stage.spec),
        other => Err(MediaError::Message(format!(
            "filter '{other}' is not an executable transform filter"
        ))),
    }
}

fn filter_transform_output_info(stage: &FilterStageSpec, input: FrameInfo) -> Result<FrameInfo> {
    match stage.name.as_str() {
        "identity" => Ok(input),
        "crop" => crop_filter_output_info(&stage.spec, input),
        "scale" => scale_filter_output_info(&stage.spec, input),
        other => Err(MediaError::Message(format!(
            "filter '{other}' has no output metadata model wired yet"
        ))),
    }
}

fn validate_executable_filter_spec(
    manifest: FilterManifest,
    stage: &FilterStageSpec,
) -> Result<()> {
    if manifest.status != FilterStatus::Implemented {
        return Err(MediaError::Message(format!(
            "filter '{}' is available as a discovery scaffold but execution is not implemented yet",
            stage.name
        )));
    }

    match stage.name.as_str() {
        "pattern" => validate_pattern_filter_spec(&stage.spec),
        "identity" => validate_identity_filter_spec(&stage.spec),
        "crop" => validate_crop_filter_spec(&stage.spec),
        "scale" => validate_scale_filter_spec(&stage.spec),
        other => Err(MediaError::Message(format!(
            "filter '{other}' has no execution model wired yet"
        ))),
    }
}

fn validate_identity_filter_spec(spec: &str) -> Result<()> {
    if spec.contains('=') || spec.contains(':') {
        return Err(MediaError::Message(
            "identity filter does not accept options".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "filter-identity")]
fn build_identity_filter() -> Result<Box<dyn Filter>> {
    Ok(Box::new(IdentityFilter))
}

#[cfg(not(feature = "filter-identity"))]
fn build_identity_filter() -> Result<Box<dyn Filter>> {
    Err(MediaError::Message("unknown filter 'identity'".to_string()))
}

#[cfg(feature = "filter-crop")]
fn validate_crop_filter_spec(spec: &str) -> Result<()> {
    parse_crop_filter_spec(spec).map(|_| ())
}

#[cfg(not(feature = "filter-crop"))]
fn validate_crop_filter_spec(_spec: &str) -> Result<()> {
    Err(MediaError::Message("unknown filter 'crop'".to_string()))
}

#[cfg(feature = "filter-scale")]
fn validate_scale_filter_spec(spec: &str) -> Result<()> {
    parse_scale_filter_spec(spec).map(|_| ())
}

#[cfg(not(feature = "filter-scale"))]
fn validate_scale_filter_spec(_spec: &str) -> Result<()> {
    Err(MediaError::Message("unknown filter 'scale'".to_string()))
}

#[cfg(feature = "filter-crop")]
fn build_crop_filter(spec: &str) -> Result<Box<dyn Filter>> {
    parse_crop_filter_spec(spec).map(|filter| Box::new(filter) as Box<dyn Filter>)
}

#[cfg(not(feature = "filter-crop"))]
fn build_crop_filter(_spec: &str) -> Result<Box<dyn Filter>> {
    Err(MediaError::Message("unknown filter 'crop'".to_string()))
}

#[cfg(feature = "filter-scale")]
fn build_scale_filter(spec: &str) -> Result<Box<dyn Filter>> {
    parse_scale_filter_spec(spec).map(|filter| Box::new(filter) as Box<dyn Filter>)
}

#[cfg(not(feature = "filter-scale"))]
fn build_scale_filter(_spec: &str) -> Result<Box<dyn Filter>> {
    Err(MediaError::Message("unknown filter 'scale'".to_string()))
}

#[cfg(feature = "filter-crop")]
fn crop_filter_output_info(spec: &str, input: FrameInfo) -> Result<FrameInfo> {
    parse_crop_filter_spec(spec)?.output_info(input)
}

#[cfg(not(feature = "filter-crop"))]
fn crop_filter_output_info(_spec: &str, _input: FrameInfo) -> Result<FrameInfo> {
    Err(MediaError::Message("unknown filter 'crop'".to_string()))
}

#[cfg(feature = "filter-scale")]
fn scale_filter_output_info(spec: &str, input: FrameInfo) -> Result<FrameInfo> {
    parse_scale_filter_spec(spec)?.output_info(input)
}

#[cfg(not(feature = "filter-scale"))]
fn scale_filter_output_info(_spec: &str, _input: FrameInfo) -> Result<FrameInfo> {
    Err(MediaError::Message("unknown filter 'scale'".to_string()))
}

#[cfg(feature = "filter-pattern")]
fn validate_pattern_filter_spec(spec: &str) -> Result<()> {
    parse_pattern_stage_kind(spec).map(|_| ())
}

#[cfg(not(feature = "filter-pattern"))]
fn validate_pattern_filter_spec(_spec: &str) -> Result<()> {
    Err(MediaError::Message("unknown filter 'pattern'".to_string()))
}

#[cfg(feature = "filter-pattern")]
fn build_pattern_filter_source(
    spec: &str,
    info: FrameInfo,
    frames: usize,
) -> Result<Box<dyn Source<Output = Frame>>> {
    Ok(Box::new(PatternSource::new(
        info,
        parse_pattern_stage_kind(spec)?,
        frames,
    )?))
}

#[cfg(not(feature = "filter-pattern"))]
fn build_pattern_filter_source(
    _spec: &str,
    _info: FrameInfo,
    _frames: usize,
) -> Result<Box<dyn Source<Output = Frame>>> {
    Err(MediaError::Message("unknown filter 'pattern'".to_string()))
}

#[cfg(feature = "filter-pattern")]
fn build_pattern_raw_video_source(
    spec: &str,
    info: FrameInfo,
    frames: usize,
) -> Result<Box<dyn RawVideoFrameSource>> {
    Ok(Box::new(PatternSource::new(
        info,
        parse_pattern_stage_kind(spec)?,
        frames,
    )?))
}

#[cfg(not(feature = "filter-pattern"))]
fn build_pattern_raw_video_source(
    _spec: &str,
    _info: FrameInfo,
    _frames: usize,
) -> Result<Box<dyn RawVideoFrameSource>> {
    Err(MediaError::Message("unknown filter 'pattern'".to_string()))
}

#[cfg(feature = "filter-pattern")]
fn parse_pattern_stage_kind(spec: &str) -> Result<PatternKind> {
    if filter_spec_name(spec) != "pattern" {
        return Err(MediaError::Message(
            "source filter must be pattern=<name>".to_string(),
        ));
    }
    let Some((_, value)) = spec.split_once('=').or_else(|| spec.split_once(':')) else {
        return Err(MediaError::Message(
            "pattern source expects --filter pattern=<name>".to_string(),
        ));
    };
    PatternKind::parse(value)
}

/// No-op frame transform that returns each input frame unchanged.
#[derive(Debug, Clone, Copy, Default)]
#[cfg(feature = "filter-identity")]
pub struct IdentityFilter;

#[cfg(feature = "filter-identity")]
impl Filter for IdentityFilter {
    fn process(&mut self, frame: Frame) -> Result<Vec<Frame>> {
        Ok(vec![frame])
    }
}

/// Rectangular frame crop transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "filter-crop")]
pub struct CropFilter {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

#[cfg(feature = "filter-crop")]
impl CropFilter {
    /// Create a crop transform from a rectangle in luma/pixel coordinates.
    pub fn new(x: usize, y: usize, width: usize, height: usize) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(MediaError::InvalidDimensions { width, height });
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    /// Left coordinate of the crop rectangle.
    pub const fn x(self) -> usize {
        self.x
    }

    /// Top coordinate of the crop rectangle.
    pub const fn y(self) -> usize {
        self.y
    }

    /// Width of the crop rectangle.
    pub const fn width(self) -> usize {
        self.width
    }

    /// Height of the crop rectangle.
    pub const fn height(self) -> usize {
        self.height
    }

    /// Metadata produced when this crop is applied to `input`.
    pub fn output_info(self, input: FrameInfo) -> Result<FrameInfo> {
        crop_output_info(input, self.x, self.y, self.width, self.height)
    }
}

#[cfg(feature = "filter-crop")]
impl Filter for CropFilter {
    fn process(&mut self, frame: Frame) -> Result<Vec<Frame>> {
        let input = frame.info();
        let output = self.output_info(input)?;
        let data = crop_frame_data(frame.data(), input, self.x, self.y, self.width, self.height)?;
        Ok(vec![Frame::new(output, data)?])
    }
}

/// Nearest-neighbor frame resize transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "filter-scale")]
pub struct ScaleFilter {
    width: usize,
    height: usize,
}

#[cfg(feature = "filter-scale")]
impl ScaleFilter {
    /// Create a nearest-neighbor scale transform.
    pub fn new(width: usize, height: usize) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(MediaError::InvalidDimensions { width, height });
        }
        Ok(Self { width, height })
    }

    /// Output width in pixels.
    pub const fn width(self) -> usize {
        self.width
    }

    /// Output height in pixels.
    pub const fn height(self) -> usize {
        self.height
    }

    /// Metadata produced when this scale is applied to `input`.
    pub fn output_info(self, input: FrameInfo) -> Result<FrameInfo> {
        FrameInfo::new(self.width, self.height, input.format)
    }
}

#[cfg(feature = "filter-scale")]
impl Filter for ScaleFilter {
    fn process(&mut self, frame: Frame) -> Result<Vec<Frame>> {
        let input = frame.info();
        let output = self.output_info(input)?;
        let data = scale_frame_data(frame.data(), input, self.width, self.height)?;
        Ok(vec![Frame::new(output, data)?])
    }
}

#[cfg(feature = "filter-crop")]
fn parse_crop_filter_spec(spec: &str) -> Result<CropFilter> {
    let params = parse_filter_params("crop", spec)?;
    let mut x = None;
    let mut y = None;
    let mut width = None;
    let mut height = None;
    for (key, value) in params {
        match key {
            "x" => assign_usize_param("crop", "x", value, false, &mut x)?,
            "y" => assign_usize_param("crop", "y", value, false, &mut y)?,
            "w" => assign_usize_param("crop", "w", value, true, &mut width)?,
            "h" => assign_usize_param("crop", "h", value, true, &mut height)?,
            other => {
                return Err(MediaError::Message(format!(
                    "crop filter does not accept parameter '{other}'"
                )));
            }
        }
    }
    CropFilter::new(
        require_param("crop", "x", x)?,
        require_param("crop", "y", y)?,
        require_param("crop", "w", width)?,
        require_param("crop", "h", height)?,
    )
}

#[cfg(feature = "filter-scale")]
fn parse_scale_filter_spec(spec: &str) -> Result<ScaleFilter> {
    let params = parse_filter_params("scale", spec)?;
    let mut width = None;
    let mut height = None;
    for (key, value) in params {
        match key {
            "w" => assign_usize_param("scale", "w", value, true, &mut width)?,
            "h" => assign_usize_param("scale", "h", value, true, &mut height)?,
            other => {
                return Err(MediaError::Message(format!(
                    "scale filter does not accept parameter '{other}'"
                )));
            }
        }
    }
    ScaleFilter::new(
        require_param("scale", "w", width)?,
        require_param("scale", "h", height)?,
    )
}

#[cfg(any(feature = "filter-crop", feature = "filter-scale"))]
fn parse_filter_params<'a>(filter_name: &str, spec: &'a str) -> Result<Vec<(&'a str, &'a str)>> {
    if filter_spec_name(spec) != filter_name {
        return Err(MediaError::Message(format!(
            "{filter_name} filter expects a spec starting with {filter_name}="
        )));
    }
    let Some((_, body)) = spec.split_once('=') else {
        return Err(MediaError::Message(format!(
            "{filter_name} filter expects key=value parameters"
        )));
    };
    if body.is_empty() {
        return Err(MediaError::Message(format!(
            "{filter_name} filter expects key=value parameters"
        )));
    }
    let mut params = Vec::new();
    for token in body.split(':') {
        let Some((key, value)) = token.split_once('=') else {
            return Err(MediaError::Message(format!(
                "{filter_name} filter parameter '{token}' must use key=value syntax"
            )));
        };
        if key.is_empty() || value.is_empty() {
            return Err(MediaError::Message(format!(
                "{filter_name} filter parameters cannot have empty keys or values"
            )));
        }
        params.push((key, value));
    }
    Ok(params)
}

#[cfg(any(feature = "filter-crop", feature = "filter-scale"))]
fn assign_usize_param(
    filter: &str,
    key: &str,
    value: &str,
    positive: bool,
    slot: &mut Option<usize>,
) -> Result<()> {
    if slot.is_some() {
        return Err(MediaError::Message(format!(
            "{filter} filter parameter '{key}' was provided more than once"
        )));
    }
    let parsed = value.parse::<usize>().map_err(|_| {
        MediaError::Message(format!(
            "{filter} filter parameter '{key}' expects an integer, got '{value}'"
        ))
    })?;
    if positive && parsed == 0 {
        return Err(MediaError::Message(format!(
            "{filter} filter parameter '{key}' must be greater than zero"
        )));
    }
    *slot = Some(parsed);
    Ok(())
}

#[cfg(any(feature = "filter-crop", feature = "filter-scale"))]
fn require_param(filter: &str, key: &str, value: Option<usize>) -> Result<usize> {
    value.ok_or_else(|| {
        MediaError::Message(format!(
            "{filter} filter is missing required parameter '{key}'"
        ))
    })
}

#[cfg(feature = "filter-crop")]
fn crop_output_info(
    input: FrameInfo,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> Result<FrameInfo> {
    let x_end = x.checked_add(width).ok_or(MediaError::LengthOverflow)?;
    let y_end = y.checked_add(height).ok_or(MediaError::LengthOverflow)?;
    if x_end > input.width || y_end > input.height {
        return Err(MediaError::IncompatibleFormat {
            format: input.format.name(),
            reason: format!(
                "crop rectangle x={x}:y={y}:w={width}:h={height} exceeds {}x{} input",
                input.width, input.height
            ),
        });
    }
    validate_crop_alignment(input.format, x, y, width, height)?;
    FrameInfo::new(width, height, input.format)
}

#[cfg(feature = "filter-crop")]
fn validate_crop_alignment(
    format: PixelFormat,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> Result<()> {
    let Some(sampling) = format.chroma_sampling() else {
        return Ok(());
    };
    let subsample_x = sampling.subsample_x();
    let subsample_y = sampling.subsample_y();
    if !x.is_multiple_of(subsample_x)
        || !width.is_multiple_of(subsample_x)
        || !y.is_multiple_of(subsample_y)
        || !height.is_multiple_of(subsample_y)
    {
        return Err(MediaError::IncompatibleFormat {
            format: format.name(),
            reason: format!(
                "crop coordinates and dimensions must be aligned to {subsample_x}x{subsample_y} samples"
            ),
        });
    }
    Ok(())
}

#[cfg(feature = "filter-crop")]
fn crop_frame_data(
    input: &[u8],
    info: FrameInfo,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> Result<Vec<u8>> {
    let output = crop_output_info(info, x, y, width, height)?;
    if input.len() != info.expected_len() {
        return Err(MediaError::BufferLength {
            expected: info.expected_len(),
            actual: input.len(),
        });
    }
    let mut data = vec![0; output.expected_len()];
    match info.format {
        PixelFormat::Rgb24 => crop_packed_rgb24(input, &mut data, info.width, x, y, width, height),
        PixelFormat::Gbrp8 => crop_full_planes(
            input,
            &mut data,
            info.width,
            info.height,
            1,
            x,
            y,
            width,
            height,
            3,
        ),
        PixelFormat::Gray { bit_depth } => crop_full_planes(
            input,
            &mut data,
            info.width,
            info.height,
            bit_depth.bytes_per_sample(),
            x,
            y,
            width,
            height,
            1,
        ),
        PixelFormat::PlanarYuv {
            chroma_sampling,
            bit_depth,
        } => crop_planar_yuv(
            input,
            &mut data,
            info.width,
            info.height,
            chroma_sampling,
            bit_depth,
            x,
            y,
            width,
            height,
        ),
    }
    Ok(data)
}

#[cfg(feature = "filter-scale")]
fn scale_frame_data(input: &[u8], info: FrameInfo, width: usize, height: usize) -> Result<Vec<u8>> {
    let output = FrameInfo::new(width, height, info.format)?;
    if input.len() != info.expected_len() {
        return Err(MediaError::BufferLength {
            expected: info.expected_len(),
            actual: input.len(),
        });
    }
    let mut data = vec![0; output.expected_len()];
    match info.format {
        PixelFormat::Rgb24 => {
            scale_packed_rgb24(input, &mut data, info.width, info.height, width, height)
        }
        PixelFormat::Gbrp8 => scale_full_planes(
            input,
            &mut data,
            info.width,
            info.height,
            width,
            height,
            1,
            3,
        ),
        PixelFormat::Gray { bit_depth } => scale_full_planes(
            input,
            &mut data,
            info.width,
            info.height,
            width,
            height,
            bit_depth.bytes_per_sample(),
            1,
        ),
        PixelFormat::PlanarYuv {
            chroma_sampling,
            bit_depth,
        } => scale_planar_yuv(
            input,
            &mut data,
            info.width,
            info.height,
            width,
            height,
            chroma_sampling,
            bit_depth,
        ),
    }
    Ok(data)
}

#[cfg(feature = "filter-crop")]
#[allow(clippy::too_many_arguments)]
fn crop_packed_rgb24(
    input: &[u8],
    output: &mut [u8],
    input_width: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) {
    crop_plane_bytes(input, output, input_width, x, y, width, height, width, 3);
}

#[cfg(feature = "filter-scale")]
fn scale_packed_rgb24(
    input: &[u8],
    output: &mut [u8],
    input_width: usize,
    input_height: usize,
    output_width: usize,
    output_height: usize,
) {
    scale_plane_bytes(
        input,
        output,
        input_width,
        input_height,
        output_width,
        output_height,
        3,
    );
}

#[cfg(feature = "filter-crop")]
#[allow(clippy::too_many_arguments)]
fn crop_full_planes(
    input: &[u8],
    output: &mut [u8],
    input_width: usize,
    input_height: usize,
    bytes_per_sample: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    planes: usize,
) {
    let input_plane_len = input_width * input_height * bytes_per_sample;
    let output_plane_len = width * height * bytes_per_sample;
    for plane in 0..planes {
        let input_start = plane * input_plane_len;
        let output_start = plane * output_plane_len;
        crop_plane_bytes(
            &input[input_start..input_start + input_plane_len],
            &mut output[output_start..output_start + output_plane_len],
            input_width,
            x,
            y,
            width,
            height,
            width,
            bytes_per_sample,
        );
    }
}

#[cfg(feature = "filter-scale")]
#[allow(clippy::too_many_arguments)]
fn scale_full_planes(
    input: &[u8],
    output: &mut [u8],
    input_width: usize,
    input_height: usize,
    output_width: usize,
    output_height: usize,
    bytes_per_sample: usize,
    planes: usize,
) {
    let input_plane_len = input_width * input_height * bytes_per_sample;
    let output_plane_len = output_width * output_height * bytes_per_sample;
    for plane in 0..planes {
        let input_start = plane * input_plane_len;
        let output_start = plane * output_plane_len;
        scale_plane_bytes(
            &input[input_start..input_start + input_plane_len],
            &mut output[output_start..output_start + output_plane_len],
            input_width,
            input_height,
            output_width,
            output_height,
            bytes_per_sample,
        );
    }
}

#[cfg(feature = "filter-crop")]
#[allow(clippy::too_many_arguments)]
fn crop_planar_yuv(
    input: &[u8],
    output: &mut [u8],
    input_width: usize,
    input_height: usize,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) {
    let bytes_per_sample = bit_depth.bytes_per_sample();
    let input_luma_len = input_width * input_height * bytes_per_sample;
    let output_luma_len = width * height * bytes_per_sample;
    crop_plane_bytes(
        &input[..input_luma_len],
        &mut output[..output_luma_len],
        input_width,
        x,
        y,
        width,
        height,
        width,
        bytes_per_sample,
    );
    if chroma_sampling == ChromaSampling::Monochrome {
        return;
    }

    let subsample_x = chroma_sampling.subsample_x();
    let subsample_y = chroma_sampling.subsample_y();
    let input_chroma_width = input_width / subsample_x;
    let input_chroma_height = input_height / subsample_y;
    let output_chroma_width = width / subsample_x;
    let output_chroma_height = height / subsample_y;
    let input_chroma_len = input_chroma_width * input_chroma_height * bytes_per_sample;
    let output_chroma_len = output_chroma_width * output_chroma_height * bytes_per_sample;
    let chroma_x = x / subsample_x;
    let chroma_y = y / subsample_y;
    for plane in 0..2 {
        let input_start = input_luma_len + plane * input_chroma_len;
        let output_start = output_luma_len + plane * output_chroma_len;
        crop_plane_bytes(
            &input[input_start..input_start + input_chroma_len],
            &mut output[output_start..output_start + output_chroma_len],
            input_chroma_width,
            chroma_x,
            chroma_y,
            output_chroma_width,
            output_chroma_height,
            output_chroma_width,
            bytes_per_sample,
        );
    }
}

#[cfg(feature = "filter-scale")]
#[allow(clippy::too_many_arguments)]
fn scale_planar_yuv(
    input: &[u8],
    output: &mut [u8],
    input_width: usize,
    input_height: usize,
    output_width: usize,
    output_height: usize,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
) {
    let bytes_per_sample = bit_depth.bytes_per_sample();
    let input_luma_len = input_width * input_height * bytes_per_sample;
    let output_luma_len = output_width * output_height * bytes_per_sample;
    scale_plane_bytes(
        &input[..input_luma_len],
        &mut output[..output_luma_len],
        input_width,
        input_height,
        output_width,
        output_height,
        bytes_per_sample,
    );
    if chroma_sampling == ChromaSampling::Monochrome {
        return;
    }

    let subsample_x = chroma_sampling.subsample_x();
    let subsample_y = chroma_sampling.subsample_y();
    let input_chroma_width = input_width / subsample_x;
    let input_chroma_height = input_height / subsample_y;
    let output_chroma_width = output_width / subsample_x;
    let output_chroma_height = output_height / subsample_y;
    let input_chroma_len = input_chroma_width * input_chroma_height * bytes_per_sample;
    let output_chroma_len = output_chroma_width * output_chroma_height * bytes_per_sample;
    for plane in 0..2 {
        let input_start = input_luma_len + plane * input_chroma_len;
        let output_start = output_luma_len + plane * output_chroma_len;
        scale_plane_bytes(
            &input[input_start..input_start + input_chroma_len],
            &mut output[output_start..output_start + output_chroma_len],
            input_chroma_width,
            input_chroma_height,
            output_chroma_width,
            output_chroma_height,
            bytes_per_sample,
        );
    }
}

#[cfg(feature = "filter-crop")]
#[allow(clippy::too_many_arguments)]
fn crop_plane_bytes(
    input: &[u8],
    output: &mut [u8],
    input_width: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    output_width: usize,
    bytes_per_sample: usize,
) {
    let row_bytes = width * bytes_per_sample;
    let input_stride = input_width * bytes_per_sample;
    let output_stride = output_width * bytes_per_sample;
    for row in 0..height {
        let input_start = (y + row) * input_stride + x * bytes_per_sample;
        let output_start = row * output_stride;
        output[output_start..output_start + row_bytes]
            .copy_from_slice(&input[input_start..input_start + row_bytes]);
    }
}

#[cfg(feature = "filter-scale")]
#[allow(clippy::too_many_arguments)]
fn scale_plane_bytes(
    input: &[u8],
    output: &mut [u8],
    input_width: usize,
    input_height: usize,
    output_width: usize,
    output_height: usize,
    bytes_per_sample: usize,
) {
    let input_stride = input_width * bytes_per_sample;
    let output_stride = output_width * bytes_per_sample;
    for y in 0..output_height {
        let source_y = y * input_height / output_height;
        for x in 0..output_width {
            let source_x = x * input_width / output_width;
            let input_start = source_y * input_stride + source_x * bytes_per_sample;
            let output_start = y * output_stride + x * bytes_per_sample;
            output[output_start..output_start + bytes_per_sample]
                .copy_from_slice(&input[input_start..input_start + bytes_per_sample]);
        }
    }
}

/// Deterministic pattern produced by the `pattern` source filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "filter-pattern")]
pub enum PatternKind {
    /// Solid black frames.
    Black,
    /// Moving checkerboard frames.
    Checker,
    /// Moving luma/chroma gradient frames.
    Gradient,
    /// Moving colored block frames.
    ColorBlocks,
    /// High-bit-depth canary frames with nonzero lower bits.
    BitdepthCanary,
}

#[cfg(feature = "filter-pattern")]
impl PatternKind {
    /// Canonical names accepted by the pattern parser.
    pub const CANONICAL_NAMES: &'static [&'static str] = PATTERN_SOURCE_NAMES;

    /// Parse a pattern kind by name.
    pub fn parse(value: &str) -> Result<Self> {
        value.parse()
    }

    /// Canonical name for this pattern kind.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Black => "black",
            Self::Checker => "checker",
            Self::Gradient => "gradient",
            Self::ColorBlocks => "color_blocks",
            Self::BitdepthCanary => "bitdepth_canary",
        }
    }
}

#[cfg(feature = "filter-pattern")]
impl FromStr for PatternKind {
    type Err = MediaError;

    fn from_str(value: &str) -> Result<Self> {
        match value.trim() {
            "black" => Ok(Self::Black),
            "checker" => Ok(Self::Checker),
            "gradient" => Ok(Self::Gradient),
            "color_blocks" | "blocks" => Ok(Self::ColorBlocks),
            "bitdepth_canary" => Ok(Self::BitdepthCanary),
            other => Err(MediaError::Message(format!(
                "unknown pattern source '{other}'; accepted patterns: {}",
                PatternKind::CANONICAL_NAMES.join(", ")
            ))),
        }
    }
}

/// Source stage that generates deterministic raw frames from a pattern.
#[derive(Debug, Clone)]
#[cfg(feature = "filter-pattern")]
pub struct PatternSource {
    info: FrameInfo,
    pattern: PatternKind,
    frames_remaining: usize,
    frame_index: usize,
}

#[cfg(feature = "filter-pattern")]
impl PatternSource {
    /// Create a finite pattern source.
    pub fn new(info: FrameInfo, pattern: PatternKind, frames: usize) -> Result<Self> {
        validate_pattern_kind_format(info.format, pattern)?;
        Ok(Self {
            info,
            pattern,
            frames_remaining: frames,
            frame_index: 0,
        })
    }

    /// Pattern emitted by this source.
    pub const fn pattern(&self) -> PatternKind {
        self.pattern
    }

    /// Frame metadata emitted by this source.
    pub const fn info(&self) -> FrameInfo {
        self.info
    }

    /// Number of frames still available before EOF.
    pub const fn frames_remaining(&self) -> usize {
        self.frames_remaining
    }

    /// Fill `output` with the next generated frame.
    ///
    /// Returns `Ok(false)` when the finite pattern source is exhausted.
    pub fn fill_frame(&mut self, output: &mut [u8]) -> Result<bool> {
        if self.frames_remaining == 0 {
            return Ok(false);
        }
        fill_pattern_frame(self.info, self.pattern, self.frame_index, output)?;
        self.frames_remaining -= 1;
        self.frame_index += 1;
        Ok(true)
    }
}

#[cfg(feature = "filter-pattern")]
impl Source for PatternSource {
    type Output = Frame;

    fn pull(&mut self) -> Result<Option<Self::Output>> {
        let mut data = vec![0; self.info.expected_len()];
        if !self.fill_frame(&mut data)? {
            return Ok(None);
        }
        Frame::new(self.info, data).map(Some)
    }
}

#[cfg(feature = "filter-pattern")]
impl RawVideoFrameSource for PatternSource {
    fn read_frame(&mut self, frame: &mut [u8]) -> Result<bool> {
        self.fill_frame(frame)
    }
}

#[cfg(feature = "filter-pattern")]
/// Generate a finite raw byte stream for a deterministic pattern.
pub fn generate_pattern_stream(
    info: FrameInfo,
    pattern: PatternKind,
    frames: usize,
) -> Result<Vec<u8>> {
    let total_len = info
        .expected_len()
        .checked_mul(frames)
        .ok_or(MediaError::LengthOverflow)?;
    let mut output = Vec::with_capacity(total_len);
    let mut source = PatternSource::new(info, pattern, frames)?;
    while let Some(frame) = source.pull()? {
        output.extend_from_slice(frame.data());
    }
    Ok(output)
}

#[cfg(feature = "filter-pattern")]
/// Generate one deterministic raw frame for a pattern.
pub fn pattern_frame_data(info: FrameInfo, pattern: PatternKind, frame: usize) -> Result<Vec<u8>> {
    validate_pattern_kind_format(info.format, pattern)?;
    let mut output = vec![0; info.expected_len()];
    fill_pattern_frame(info, pattern, frame, &mut output)?;
    Ok(output)
}

#[cfg(feature = "filter-pattern")]
fn validate_pattern_format(format: PixelFormat) -> Result<()> {
    match format {
        PixelFormat::PlanarYuv { .. }
        | PixelFormat::Gray { .. }
        | PixelFormat::Gbrp8
        | PixelFormat::Rgb24 => Ok(()),
    }
}

#[cfg(feature = "filter-pattern")]
fn validate_pattern_kind_format(format: PixelFormat, pattern: PatternKind) -> Result<()> {
    validate_pattern_format(format)?;
    if pattern == PatternKind::BitdepthCanary {
        validate_bitdepth_canary_format(format)?;
    }
    Ok(())
}

#[cfg(feature = "filter-pattern")]
fn validate_bitdepth_canary_format(format: PixelFormat) -> Result<()> {
    match format {
        PixelFormat::Gray { bit_depth } | PixelFormat::PlanarYuv { bit_depth, .. } => {
            validate_bitdepth_canary_depth(bit_depth)
        }
        PixelFormat::Gbrp8 | PixelFormat::Rgb24 => Err(MediaError::Message(
            "bitdepth_canary is only supported for high-bit-depth planar formats".to_string(),
        )),
    }
}

#[cfg(feature = "filter-pattern")]
fn fill_pattern_frame(
    info: FrameInfo,
    pattern: PatternKind,
    frame: usize,
    output: &mut [u8],
) -> Result<()> {
    let expected = info.expected_len();
    if output.len() != expected {
        return Err(MediaError::BufferLength {
            expected,
            actual: output.len(),
        });
    }
    validate_pattern_kind_format(info.format, pattern)?;
    if pattern == PatternKind::BitdepthCanary {
        return fill_bitdepth_canary_frame(info, frame, output);
    }
    match info.format {
        PixelFormat::Rgb24 => fill_pattern_rgb24(info.width, info.height, pattern, frame, output),
        PixelFormat::Gbrp8 => fill_pattern_gbrp8(info.width, info.height, pattern, frame, output),
        PixelFormat::Gray { bit_depth } => {
            fill_pattern_gray(info.width, info.height, bit_depth, pattern, frame, output)
        }
        PixelFormat::PlanarYuv {
            chroma_sampling,
            bit_depth,
        } => fill_pattern_planar_yuv(
            info.width,
            info.height,
            chroma_sampling,
            bit_depth,
            pattern,
            frame,
            output,
        ),
    }
    Ok(())
}

#[cfg(feature = "filter-pattern")]
fn fill_pattern_rgb24(
    width: usize,
    height: usize,
    pattern: PatternKind,
    frame: usize,
    output: &mut [u8],
) {
    for y in 0..height {
        for x in 0..width {
            let (r, g, b) = pattern_sample(pattern, x, y, frame);
            let offset = (y * width + x) * 3;
            output[offset] = r;
            output[offset + 1] = g;
            output[offset + 2] = b;
        }
    }
}

#[cfg(feature = "filter-pattern")]
fn fill_pattern_gbrp8(
    width: usize,
    height: usize,
    pattern: PatternKind,
    frame: usize,
    output: &mut [u8],
) {
    let pixels = width * height;
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let (r, g, b) = pattern_sample(pattern, x, y, frame);
            output[idx] = g;
            output[pixels + idx] = b;
            output[2 * pixels + idx] = r;
        }
    }
}

#[cfg(feature = "filter-pattern")]
fn fill_pattern_gray(
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    pattern: PatternKind,
    frame: usize,
    output: &mut [u8],
) {
    for y in 0..height {
        for x in 0..width {
            let (sample, _, _) = pattern_sample(pattern, x, y, frame);
            write_pattern_sample(output, y * width + x, sample, bit_depth);
        }
    }
}

#[cfg(feature = "filter-pattern")]
#[allow(clippy::too_many_arguments)]
fn fill_pattern_planar_yuv(
    width: usize,
    height: usize,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    pattern: PatternKind,
    frame: usize,
    output: &mut [u8],
) {
    for y in 0..height {
        for x in 0..width {
            let (luma, _, _) = pattern_sample(pattern, x, y, frame);
            write_pattern_sample(output, y * width + x, luma, bit_depth);
        }
    }

    if chroma_sampling == ChromaSampling::Monochrome {
        return;
    }

    let bytes_per_sample = bit_depth.bytes_per_sample();
    let luma_len = width * height * bytes_per_sample;
    let subsample_x = chroma_sampling.subsample_x();
    let subsample_y = chroma_sampling.subsample_y();
    let chroma_width = width / subsample_x;
    let chroma_height = height / subsample_y;
    let chroma_len = chroma_width * chroma_height * bytes_per_sample;
    let (u_plane, v_plane) = output[luma_len..luma_len + chroma_len * 2].split_at_mut(chroma_len);

    for chroma_y in 0..chroma_height {
        for chroma_x in 0..chroma_width {
            let mut u_sum = 0u32;
            let mut v_sum = 0u32;
            for dy in 0..subsample_y {
                for dx in 0..subsample_x {
                    let x = chroma_x * subsample_x + dx;
                    let y = chroma_y * subsample_y + dy;
                    let (_, u, v) = pattern_sample(pattern, x, y, frame);
                    u_sum += u32::from(u);
                    v_sum += u32::from(v);
                }
            }
            let denom = (subsample_x * subsample_y) as u32;
            let index = chroma_y * chroma_width + chroma_x;
            write_pattern_sample(u_plane, index, (u_sum / denom) as u8, bit_depth);
            write_pattern_sample(v_plane, index, (v_sum / denom) as u8, bit_depth);
        }
    }
}

#[cfg(feature = "filter-pattern")]
fn fill_bitdepth_canary_frame(info: FrameInfo, frame: usize, output: &mut [u8]) -> Result<()> {
    match info.format {
        PixelFormat::Gray { bit_depth } => {
            fill_bitdepth_canary_plane(info.width, info.height, bit_depth, frame, 0, output);
        }
        PixelFormat::PlanarYuv {
            chroma_sampling,
            bit_depth,
        } => {
            fill_bitdepth_canary_planar_yuv(
                info.width,
                info.height,
                chroma_sampling,
                bit_depth,
                frame,
                output,
            );
        }
        PixelFormat::Gbrp8 | PixelFormat::Rgb24 => {
            unreachable!("bitdepth_canary format validation rejects RGB-family formats");
        }
    }
    Ok(())
}

#[cfg(feature = "filter-pattern")]
fn validate_bitdepth_canary_depth(bit_depth: SampleBitDepth) -> Result<()> {
    if bit_depth.bits() <= 8 {
        return Err(MediaError::Message(
            "bitdepth_canary is intended for high-bit-depth generated vectors".to_string(),
        ));
    }
    Ok(())
}

#[cfg(feature = "filter-pattern")]
fn fill_bitdepth_canary_planar_yuv(
    width: usize,
    height: usize,
    chroma_sampling: ChromaSampling,
    bit_depth: SampleBitDepth,
    frame: usize,
    output: &mut [u8],
) {
    let bytes_per_sample = bit_depth.bytes_per_sample();
    let luma_len = width * height * bytes_per_sample;
    fill_bitdepth_canary_plane(width, height, bit_depth, frame, 0, &mut output[..luma_len]);

    if chroma_sampling == ChromaSampling::Monochrome {
        return;
    }

    let subsample_x = chroma_sampling.subsample_x();
    let subsample_y = chroma_sampling.subsample_y();
    let chroma_width = width / subsample_x;
    let chroma_height = height / subsample_y;
    let chroma_len = chroma_width * chroma_height * bytes_per_sample;
    let (u_plane, v_plane) = output[luma_len..luma_len + chroma_len * 2].split_at_mut(chroma_len);
    fill_bitdepth_canary_plane(chroma_width, chroma_height, bit_depth, frame, 1, u_plane);
    fill_bitdepth_canary_plane(chroma_width, chroma_height, bit_depth, frame, 2, v_plane);
}

#[cfg(feature = "filter-pattern")]
fn fill_bitdepth_canary_plane(
    width: usize,
    height: usize,
    bit_depth: SampleBitDepth,
    frame: usize,
    plane: usize,
    output: &mut [u8],
) {
    for y in 0..height {
        for x in 0..width {
            write_planar_sample(
                output,
                y * width + x,
                bitdepth_canary_sample(x, y, frame, plane, bit_depth),
                bit_depth,
            )
            .expect("validated pattern output must contain every sample");
        }
    }
}

#[cfg(feature = "filter-pattern")]
fn bitdepth_canary_sample(
    x: usize,
    y: usize,
    frame: usize,
    plane: usize,
    bit_depth: SampleBitDepth,
) -> u16 {
    let shift = u32::from(bit_depth.bits() - 8);
    let block_index = ((x / 8) + (y / 8) * 2 + frame) % 4;
    let base = match plane {
        0 => [32u16, 96, 160, 224][block_index],
        1 => [80u16, 144, 208, 48][block_index],
        2 => [112u16, 176, 64, 240][block_index],
        _ => 0,
    };
    (base << shift) | bitdepth_canary_lower(x, y, frame, plane, shift)
}

#[cfg(feature = "filter-pattern")]
fn bitdepth_canary_lower(x: usize, y: usize, frame: usize, plane: usize, shift: u32) -> u16 {
    let low_mask = (1u16 << shift) - 1;
    let mut lower = ((x & 3) | ((y & 3) << 2) | ((plane & 3) << 1) | frame) as u16 & low_mask;
    if lower == 0 {
        lower = low_mask;
    }
    lower
}

#[cfg(feature = "filter-pattern")]
fn write_pattern_sample(
    output: &mut [u8],
    sample_index: usize,
    sample: u8,
    bit_depth: SampleBitDepth,
) {
    let sample = scale_sample_bit_depth(
        u16::from(sample),
        SampleBitDepth::new(8).expect("8-bit samples must be supported"),
        bit_depth,
    );
    write_planar_sample(output, sample_index, sample, bit_depth)
        .expect("validated pattern output must contain every sample");
}

#[cfg(feature = "filter-pattern")]
fn pattern_sample(pattern: PatternKind, x: usize, y: usize, frame: usize) -> (u8, u8, u8) {
    match pattern {
        PatternKind::Black => (0, 0, 0),
        PatternKind::Checker => {
            if ((x / 8) + (y / 8) + frame) & 1 == 0 {
                (208, 176, 80)
            } else {
                (48, 96, 160)
            }
        }
        PatternKind::Gradient => (
            ((x * 7 + y * 5 + frame * 17) & 0xFF) as u8,
            ((64 + x * 3 + frame * 11) & 0xFF) as u8,
            ((96 + y * 4 + frame * 13) & 0xFF) as u8,
        ),
        PatternKind::ColorBlocks => {
            const PALETTE: [(u8, u8, u8); 4] = [
                (32, 128, 128),
                (80, 96, 176),
                (144, 176, 96),
                (224, 112, 144),
            ];
            PALETTE[((x / 8) + (y / 8) * 2 + frame) % PALETTE.len()]
        }
        PatternKind::BitdepthCanary => unreachable!("bitdepth_canary uses native-depth samples"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_manifest_reports_compiled_filters() {
        assert_eq!(
            filter_manifest("pattern").is_some(),
            cfg!(feature = "filter-pattern")
        );
        assert_eq!(
            filter_manifest("identity").is_some(),
            cfg!(feature = "filter-identity")
        );
        assert_eq!(
            filter_manifest("crop").is_some(),
            cfg!(feature = "filter-crop")
        );
        assert_eq!(
            filter_manifest("scale").is_some(),
            cfg!(feature = "filter-scale")
        );
    }

    #[cfg(feature = "filter-pattern")]
    #[test]
    fn pattern_manifest_reports_source_contract() {
        let pattern = filter_manifest("pattern").expect("pattern manifest");
        assert_eq!(pattern.stage, FilterStageKind::Source);
        assert_eq!(pattern.status, FilterStatus::Implemented);
        assert_eq!(pattern.feature, "filter-pattern");
        assert_eq!(pattern.spec.forms[0].syntax, "pattern=<name>");
        assert_eq!(pattern.spec.parameters[0].name, "name");
        assert_eq!(
            pattern.spec.parameters[0].value,
            FilterSpecValue::Choice(PATTERN_SOURCE_NAMES)
        );
    }

    #[cfg(feature = "filter-crop")]
    #[test]
    fn crop_manifest_reports_transform_contract() {
        let crop = filter_manifest("crop").expect("crop manifest");
        assert_eq!(crop.stage, FilterStageKind::Transform);
        assert_eq!(crop.status, FilterStatus::Implemented);
        assert_eq!(
            crop.spec.forms[0].syntax,
            "crop=x=<px>:y=<px>:w=<px>:h=<px>"
        );
    }

    #[cfg(feature = "filter-scale")]
    #[test]
    fn scale_manifest_reports_transform_contract() {
        let scale = filter_manifest("scale").expect("scale manifest");
        assert_eq!(scale.stage, FilterStageKind::Transform);
        assert_eq!(scale.status, FilterStatus::Implemented);
        assert_eq!(scale.spec.forms[0].syntax, "scale=w=<px>:h=<px>");
    }

    #[cfg(feature = "filter-pattern")]
    #[test]
    fn pattern_kind_parses_aliases_and_reports_canonical_names() {
        assert_eq!(PatternKind::parse("black").unwrap(), PatternKind::Black);
        assert_eq!(
            PatternKind::parse("blocks").unwrap(),
            PatternKind::ColorBlocks
        );
        assert_eq!(
            PatternKind::CANONICAL_NAMES,
            [
                "black",
                "checker",
                "gradient",
                "color_blocks",
                "bitdepth_canary"
            ]
        );
    }

    #[cfg(feature = "filter-identity")]
    #[test]
    fn identity_filter_preserves_frame() {
        let info = FrameInfo::new(2, 2, PixelFormat::Rgb24).unwrap();
        let frame = Frame::new(info, vec![9; info.expected_len()]).unwrap();
        let mut filter = IdentityFilter;
        let out = filter.process(frame.clone()).unwrap();
        assert_eq!(out, vec![frame]);
    }

    #[cfg(feature = "filter-crop")]
    #[test]
    fn crop_filter_extracts_aligned_yuv420_region() {
        let info = FrameInfo::new(4, 4, PixelFormat::Yuv420p8).unwrap();
        let input = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 100, 101, 102, 103, 200, 201,
            202, 203,
        ];
        let frame = Frame::new(info, input.to_vec()).unwrap();
        let mut filter = CropFilter::new(2, 2, 2, 2).unwrap();
        let output = filter.process(frame).unwrap();

        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].info(),
            FrameInfo::new(2, 2, PixelFormat::Yuv420p8).unwrap()
        );
        assert_eq!(output[0].data(), &[10, 11, 14, 15, 103, 203]);
    }

    #[cfg(feature = "filter-crop")]
    #[test]
    fn crop_filter_rejects_unaligned_yuv420_region() {
        let info = FrameInfo::new(4, 4, PixelFormat::Yuv420p8).unwrap();
        let err = CropFilter::new(1, 0, 2, 2)
            .unwrap()
            .output_info(info)
            .unwrap_err();
        assert!(err.to_string().contains("aligned to 2x2"), "{err}");
    }

    #[cfg(feature = "filter-scale")]
    #[test]
    fn scale_filter_resizes_yuv420_with_nearest_neighbor() {
        let info = FrameInfo::new(2, 2, PixelFormat::Yuv420p8).unwrap();
        let frame = Frame::new(info, vec![1, 2, 3, 4, 5, 6]).unwrap();
        let mut filter = ScaleFilter::new(4, 4).unwrap();
        let output = filter.process(frame).unwrap();

        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].info(),
            FrameInfo::new(4, 4, PixelFormat::Yuv420p8).unwrap()
        );
        assert_eq!(
            output[0].data(),
            &[1, 1, 2, 2, 1, 1, 2, 2, 3, 3, 4, 4, 3, 3, 4, 4, 5, 5, 5, 5, 6, 6, 6, 6,]
        );
    }

    #[cfg(all(feature = "filter-crop", feature = "filter-scale"))]
    #[test]
    fn filter_pipeline_output_info_tracks_geometry_changes() {
        let input = FrameInfo::new(16, 16, PixelFormat::Yuv420p8).unwrap();
        let transforms = [
            FilterStageSpec::new("crop=x=0:y=0:w=8:h=8").unwrap(),
            FilterStageSpec::new("scale=w=16:h=16").unwrap(),
        ];
        let output = filter_pipeline_output_info(input, &transforms).unwrap();
        assert_eq!(output, input);
    }

    #[cfg(feature = "filter-pattern")]
    #[test]
    fn pattern_source_generates_black_yuv420_frame() {
        let info = FrameInfo::new(8, 8, PixelFormat::Yuv420p8).unwrap();
        let mut source = PatternSource::new(info, PatternKind::Black, 1).unwrap();
        let frame = source.pull().unwrap().expect("frame");
        assert_eq!(frame.data(), vec![0; info.expected_len()]);
        assert!(source.pull().unwrap().is_none());
    }

    #[cfg(feature = "filter-pattern")]
    #[test]
    fn pattern_stream_generates_all_requested_frames() {
        let info = FrameInfo::new(8, 8, PixelFormat::Yuv444p8).unwrap();
        let data = generate_pattern_stream(info, PatternKind::Checker, 2).unwrap();
        assert_eq!(data.len(), info.expected_len() * 2);
        assert_ne!(&data[..info.expected_len()], &data[info.expected_len()..]);
    }

    #[cfg(feature = "filter-pattern")]
    #[test]
    fn pattern_source_generates_rgb24_frame() {
        let info = FrameInfo::new(8, 8, PixelFormat::Rgb24).unwrap();
        let mut source = PatternSource::new(info, PatternKind::Checker, 1).unwrap();
        let frame = source.pull().unwrap().expect("frame");
        assert_eq!(frame.data().len(), info.expected_len());
        assert!(frame.data().chunks_exact(3).any(|pixel| pixel != [0, 0, 0]));
    }

    #[cfg(feature = "filter-pattern")]
    #[test]
    fn pattern_source_generates_high_bit_depth_planar_frame() {
        let info = FrameInfo::new(8, 8, PixelFormat::yuv444(10).unwrap()).unwrap();
        let frame = pattern_frame_data(info, PatternKind::Gradient, 0).unwrap();
        assert_eq!(frame.len(), info.expected_len());
        let first = crate::read_planar_sample(&frame, 1, info.format.bit_depth()).unwrap();
        assert!(first > 0);
    }

    #[cfg(feature = "filter-pattern")]
    #[test]
    fn pattern_source_generates_bitdepth_canary_yuv444p10_frame() {
        let info = FrameInfo::new(8, 8, PixelFormat::yuv444(10).unwrap()).unwrap();
        let frame = pattern_frame_data(info, PatternKind::BitdepthCanary, 0).unwrap();

        assert_eq!(frame.len(), info.expected_len());
        assert_eq!(
            crate::read_planar_sample(&frame, 0, info.format.bit_depth()).unwrap(),
            131
        );
        assert_eq!(
            crate::read_planar_sample(&frame, 64, info.format.bit_depth()).unwrap(),
            322
        );
        assert_eq!(
            crate::read_planar_sample(&frame, 128, info.format.bit_depth()).unwrap(),
            451
        );
    }

    #[cfg(feature = "filter-pattern")]
    #[test]
    fn pattern_source_rejects_bitdepth_canary_for_8_bit_formats() {
        let info = FrameInfo::new(8, 8, PixelFormat::Yuv444p8).unwrap();
        let err = PatternSource::new(info, PatternKind::BitdepthCanary, 1).unwrap_err();
        assert!(err.to_string().contains("high-bit-depth"), "{err}");
    }
}
