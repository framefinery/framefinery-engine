use std::io::{Read, Write};

use crate::PixelFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingValue {
    Boolean,
    Choice(&'static [&'static str]),
    IntegerRange { min: u16, max: u16 },
}

#[derive(Debug, Clone, Copy)]
pub struct SettingSpecForm {
    pub syntax: &'static str,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct SettingSpecExample {
    pub spec: &'static str,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct SettingSpecManifest {
    pub forms: &'static [SettingSpecForm],
    pub examples: &'static [SettingSpecExample],
    pub notes: &'static [&'static str],
}

#[derive(Debug, Clone, Copy)]
pub struct SettingManifest {
    pub name: &'static str,
    pub value: SettingValue,
    pub spec: &'static SettingSpecManifest,
    pub summary: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct CodecManifest {
    pub name: &'static str,
    pub feature: &'static str,
    pub summary: &'static str,
    pub settings: &'static [SettingManifest],
    pub accepts_format: fn(PixelFormat) -> bool,
    pub supports_lossless_format: fn(PixelFormat) -> bool,
    pub encode: CodecEncodeFn,
}

#[derive(Debug, Clone, Copy)]
pub struct CodecEncodeRequest<'a> {
    pub frames: usize,
    pub width: usize,
    pub height: usize,
    pub format: PixelFormat,
    pub lossless: bool,
    pub settings: &'a [String],
}

pub struct CodecEncodeFrameMetrics<'a> {
    pub frame_idx: usize,
    pub frame_count: usize,
    pub bitstream_bytes: usize,
    pub source: &'a [u8],
    pub reconstruction: &'a [u8],
}

pub type CodecEncodeFrameMetricsCallback<'a> =
    &'a mut dyn for<'frame> FnMut(CodecEncodeFrameMetrics<'frame>);

pub type CodecEncodeFn = for<'request> fn(
    &mut dyn Read,
    &mut dyn Write,
    Option<&mut dyn Write>,
    CodecEncodeRequest<'request>,
    Option<CodecEncodeFrameMetricsCallback<'request>>,
) -> std::result::Result<(), String>;

impl CodecManifest {
    pub fn setting(self, name: &str) -> Option<SettingManifest> {
        self.settings
            .iter()
            .copied()
            .find(|setting| setting.name == name)
    }
}

impl SettingValue {
    pub fn accepts(self, value: &str) -> bool {
        match self {
            SettingValue::Boolean => matches!(
                value,
                "true" | "false" | "1" | "0" | "yes" | "no" | "on" | "off"
            ),
            SettingValue::Choice(values) => values.contains(&value),
            SettingValue::IntegerRange { min, max } => value
                .parse::<u16>()
                .is_ok_and(|parsed| (min..=max).contains(&parsed)),
        }
    }
}

pub fn setting_name(spec: &str) -> &str {
    spec.split_once('=').map_or(spec, |(name, _)| name)
}

pub fn setting_value(spec: &str) -> Option<&str> {
    spec.split_once('=').map(|(_, value)| value)
}

pub fn boolean_setting_enabled(
    settings: &[String],
    name: &str,
) -> std::result::Result<bool, String> {
    for spec in settings {
        if setting_name(spec) != name {
            continue;
        }
        let value = setting_value(spec).unwrap_or("true");
        match value {
            "true" | "1" | "yes" | "on" => return Ok(true),
            "false" | "0" | "no" | "off" => return Ok(false),
            _ => return Err(format!("{name} expects true or false, got '{value}'")),
        }
    }
    Ok(false)
}

pub fn u8_setting(settings: &[String], name: &str) -> std::result::Result<Option<u8>, String> {
    for spec in settings {
        if setting_name(spec) != name {
            continue;
        }
        let value = setting_value(spec).unwrap_or("true");
        let parsed = value
            .parse::<u16>()
            .map_err(|_| format!("{name} expects an integer from 1 through 255, got '{value}'"))?;
        if parsed == 0 || parsed > u16::from(u8::MAX) {
            return Err(format!(
                "{name} expects an integer from 1 through 255, got '{value}'"
            ));
        }
        return Ok(Some(parsed as u8));
    }
    Ok(None)
}

pub fn setting_values_label(setting: SettingManifest) -> String {
    match setting.value {
        SettingValue::Boolean => "true|false".to_string(),
        SettingValue::Choice(values) => values.join("|"),
        SettingValue::IntegerRange { min, max } => format!("{min}..{max}"),
    }
}

const LOSSLESS_SPEC_FORMS: &[SettingSpecForm] = &[
    SettingSpecForm {
        syntax: "lossless",
        summary: "enable lossless coding",
    },
    SettingSpecForm {
        syntax: "lossless=<bool>",
        summary: "explicitly enable or disable lossless coding",
    },
];

const LOSSLESS_SPEC_EXAMPLES: &[SettingSpecExample] = &[
    SettingSpecExample {
        spec: "lossless",
        summary: "request lossless output",
    },
    SettingSpecExample {
        spec: "lossless=false",
        summary: "explicitly leave lossy coding enabled",
    },
];

const LOSSLESS_SPEC_NOTES: &[&str] = &[
    "bare boolean settings imply true",
    "mutually exclusive with codec quantization settings",
];

pub const LOSSLESS_SETTING_SPEC: SettingSpecManifest = SettingSpecManifest {
    forms: LOSSLESS_SPEC_FORMS,
    examples: LOSSLESS_SPEC_EXAMPLES,
    notes: LOSSLESS_SPEC_NOTES,
};

pub const LOSSLESS_SETTING: SettingManifest = SettingManifest {
    name: "lossless",
    value: SettingValue::Boolean,
    spec: &LOSSLESS_SETTING_SPEC,
    summary: "request lossless coding when supported",
};

pub const GLOBAL_SETTINGS: &[SettingManifest] = &[LOSSLESS_SETTING];
