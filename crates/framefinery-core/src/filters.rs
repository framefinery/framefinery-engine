#[cfg(feature = "filter-pattern")]
use std::str::FromStr;

use crate::error::MediaError;
use crate::error::Result;
use crate::pipeline::Filter;
#[cfg(feature = "filter-pattern")]
use crate::pipeline::Source;
#[cfg(feature = "filter-identity")]
use crate::Frame;
use crate::FrameInfo;
#[cfg(feature = "filter-pattern")]
use crate::PixelFormat;

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
pub const PATTERN_SOURCE_NAMES: &[&str] = &["black", "checker", "gradient", "color_blocks"];

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
    "currently generates yuv420p8 and yuv444p8 raw frames",
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

const CROP_SPEC_NOTES: &[&str] = &["execution is still a scaffold"];

/// Spec manifest for the `crop` transform scaffold.
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

const SCALE_SPEC_NOTES: &[&str] = &["execution is still a scaffold"];

/// Spec manifest for the `scale` transform scaffold.
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
        status: FilterStatus::Scaffold,
        spec: &CROP_FILTER_SPEC,
        summary: "rectangular crop filter scaffold",
    },
    #[cfg(feature = "filter-scale")]
    FilterManifest {
        name: "scale",
        stage: FilterStageKind::Transform,
        feature: "filter-scale",
        status: FilterStatus::Scaffold,
        spec: &SCALE_FILTER_SPEC,
        summary: "resize filter scaffold",
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

/// Generate raw video bytes from an executable source filter.
pub fn generate_source_filter_stream(
    stage: &FilterStageSpec,
    info: FrameInfo,
    frames: usize,
) -> Result<Vec<u8>> {
    match stage.name.as_str() {
        "pattern" => generate_pattern_filter_stream(&stage.spec, info, frames),
        other => Err(MediaError::Message(format!(
            "filter '{other}' is not an executable source filter"
        ))),
    }
}

/// Build an executable transform filter from a parsed stage spec.
pub fn build_filter_transform(stage: &FilterStageSpec) -> Result<Box<dyn Filter>> {
    match stage.name.as_str() {
        "identity" => build_identity_filter(),
        other => Err(MediaError::Message(format!(
            "filter '{other}' is not an executable transform filter"
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

#[cfg(feature = "filter-pattern")]
fn validate_pattern_filter_spec(spec: &str) -> Result<()> {
    parse_pattern_stage_kind(spec).map(|_| ())
}

#[cfg(not(feature = "filter-pattern"))]
fn validate_pattern_filter_spec(_spec: &str) -> Result<()> {
    Err(MediaError::Message("unknown filter 'pattern'".to_string()))
}

#[cfg(feature = "filter-pattern")]
fn generate_pattern_filter_stream(spec: &str, info: FrameInfo, frames: usize) -> Result<Vec<u8>> {
    generate_pattern_stream(info, parse_pattern_stage_kind(spec)?, frames)
}

#[cfg(not(feature = "filter-pattern"))]
fn generate_pattern_filter_stream(
    _spec: &str,
    _info: FrameInfo,
    _frames: usize,
) -> Result<Vec<u8>> {
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
        validate_pattern_format(info.format)?;
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
}

#[cfg(feature = "filter-pattern")]
impl Source for PatternSource {
    type Output = Frame;

    fn pull(&mut self) -> Result<Option<Self::Output>> {
        if self.frames_remaining == 0 {
            return Ok(None);
        }
        let data = pattern_frame_data(self.info, self.pattern, self.frame_index)?;
        self.frames_remaining -= 1;
        self.frame_index += 1;
        Frame::new(self.info, data).map(Some)
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
    validate_pattern_format(info.format)?;
    match info.format {
        PixelFormat::Yuv420p8 => Ok(generate_yuv420p8(info.width, info.height, frame, pattern)),
        PixelFormat::Yuv444p8 => Ok(generate_yuv444p8(info.width, info.height, frame, pattern)),
        other => Err(MediaError::Unsupported {
            feature: "pattern source".to_string(),
            reason: format!("currently supports yuv420p8 and yuv444p8; got {other}"),
        }),
    }
}

#[cfg(feature = "filter-pattern")]
fn validate_pattern_format(format: PixelFormat) -> Result<()> {
    match format {
        PixelFormat::Yuv420p8 | PixelFormat::Yuv444p8 => Ok(()),
        other => Err(MediaError::Unsupported {
            feature: "pattern source".to_string(),
            reason: format!("currently supports yuv420p8 and yuv444p8; got {other}"),
        }),
    }
}

#[cfg(feature = "filter-pattern")]
fn generate_yuv420p8(width: usize, height: usize, frame: usize, pattern: PatternKind) -> Vec<u8> {
    let (y_plane, u444, v444) = render_pattern_frame(width, height, frame, pattern);
    let mut out = Vec::with_capacity(width * height * 3 / 2);
    let mut u_plane = Vec::with_capacity(width * height / 4);
    let mut v_plane = Vec::with_capacity(width * height / 4);
    for y in (0..height).step_by(2) {
        for x in (0..width).step_by(2) {
            let indices = (
                y * width + x,
                y * width + x + 1,
                (y + 1) * width + x,
                (y + 1) * width + x + 1,
            );
            u_plane.push(
                ((u444[indices.0] as u16
                    + u444[indices.1] as u16
                    + u444[indices.2] as u16
                    + u444[indices.3] as u16)
                    / 4) as u8,
            );
            v_plane.push(
                ((v444[indices.0] as u16
                    + v444[indices.1] as u16
                    + v444[indices.2] as u16
                    + v444[indices.3] as u16)
                    / 4) as u8,
            );
        }
    }
    out.extend(y_plane);
    out.extend(u_plane);
    out.extend(v_plane);
    out
}

#[cfg(feature = "filter-pattern")]
fn generate_yuv444p8(width: usize, height: usize, frame: usize, pattern: PatternKind) -> Vec<u8> {
    let (y_plane, u_plane, v_plane) = render_pattern_frame(width, height, frame, pattern);
    let mut out = Vec::with_capacity(width * height * 3);
    out.extend(y_plane);
    out.extend(u_plane);
    out.extend(v_plane);
    out
}

#[cfg(feature = "filter-pattern")]
fn render_pattern_frame(
    width: usize,
    height: usize,
    frame: usize,
    pattern: PatternKind,
) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut y_plane = vec![0; width * height];
    let mut u_plane = vec![0; width * height];
    let mut v_plane = vec![0; width * height];
    for y in 0..height {
        for x in 0..width {
            let (yy, uu, vv) = pattern_sample(pattern, x, y, frame);
            let idx = y * width + x;
            y_plane[idx] = yy;
            u_plane[idx] = uu;
            v_plane[idx] = vv;
        }
    }
    (y_plane, u_plane, v_plane)
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
    fn crop_manifest_reports_scaffold_contract() {
        let crop = filter_manifest("crop").expect("crop manifest");
        assert_eq!(crop.stage, FilterStageKind::Transform);
        assert_eq!(crop.status, FilterStatus::Scaffold);
        assert_eq!(
            crop.spec.forms[0].syntax,
            "crop=x=<px>:y=<px>:w=<px>:h=<px>"
        );
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
            ["black", "checker", "gradient", "color_blocks"]
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
    fn pattern_source_rejects_unsupported_formats() {
        let info = FrameInfo::new(8, 8, PixelFormat::Rgb24).unwrap();
        let err = PatternSource::new(info, PatternKind::Black, 1).unwrap_err();
        assert!(err.to_string().contains("pattern source"));
    }
}
