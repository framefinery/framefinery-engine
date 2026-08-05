#[cfg(feature = "filter-pattern")]
use std::str::FromStr;

#[cfg(feature = "filter-pattern")]
use crate::error::MediaError;
#[cfg(any(feature = "filter-identity", feature = "filter-pattern"))]
use crate::error::Result;
#[cfg(feature = "filter-identity")]
use crate::pipeline::Filter;
#[cfg(feature = "filter-pattern")]
use crate::pipeline::Source;
#[cfg(any(feature = "filter-identity", feature = "filter-pattern"))]
use crate::Frame;
#[cfg(feature = "filter-pattern")]
use crate::{FrameInfo, PixelFormat};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterStageKind {
    Source,
    Transform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterStatus {
    Implemented,
    Scaffold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterSpecValue {
    Choice(&'static [&'static str]),
    PositiveInteger,
    UnsignedInteger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterSpecForm {
    pub syntax: &'static str,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterSpecParameter {
    pub name: &'static str,
    pub value_name: &'static str,
    pub required: bool,
    pub value: FilterSpecValue,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterSpecExample {
    pub spec: &'static str,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterSpecManifest {
    pub forms: &'static [FilterSpecForm],
    pub parameters: &'static [FilterSpecParameter],
    pub examples: &'static [FilterSpecExample],
    pub notes: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterManifest {
    pub name: &'static str,
    pub stage: FilterStageKind,
    pub feature: &'static str,
    pub status: FilterStatus,
    pub spec: &'static FilterSpecManifest,
    pub summary: &'static str,
}

impl FilterManifest {
    pub const fn implementation_status(self) -> &'static str {
        match self.status {
            FilterStatus::Implemented => "implemented",
            FilterStatus::Scaffold => "scaffold",
        }
    }
}

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

pub const SCALE_FILTER_SPEC: FilterSpecManifest = FilterSpecManifest {
    forms: SCALE_SPEC_FORMS,
    parameters: SCALE_SPEC_PARAMETERS,
    examples: SCALE_SPEC_EXAMPLES,
    notes: SCALE_SPEC_NOTES,
};

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

pub fn filter_manifest(name: &str) -> Option<FilterManifest> {
    FILTERS.iter().copied().find(|filter| filter.name == name)
}

pub fn filter_spec_manifest(name: &str) -> Option<&'static FilterSpecManifest> {
    filter_manifest(name).map(|filter| filter.spec)
}

#[derive(Debug, Clone, Copy, Default)]
#[cfg(feature = "filter-identity")]
pub struct IdentityFilter;

#[cfg(feature = "filter-identity")]
impl Filter for IdentityFilter {
    fn process(&mut self, frame: Frame) -> Result<Vec<Frame>> {
        Ok(vec![frame])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "filter-pattern")]
pub enum PatternKind {
    Black,
    Checker,
    Gradient,
    ColorBlocks,
}

#[cfg(feature = "filter-pattern")]
impl PatternKind {
    pub const CANONICAL_NAMES: &'static [&'static str] = PATTERN_SOURCE_NAMES;

    pub fn parse(value: &str) -> Result<Self> {
        value.parse()
    }

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
    pub fn new(info: FrameInfo, pattern: PatternKind, frames: usize) -> Result<Self> {
        validate_pattern_format(info.format)?;
        Ok(Self {
            info,
            pattern,
            frames_remaining: frames,
            frame_index: 0,
        })
    }

    pub const fn pattern(&self) -> PatternKind {
        self.pattern
    }

    pub const fn info(&self) -> FrameInfo {
        self.info
    }

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
