/// Value shape accepted by a declared encoder or global setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingValue {
    /// Boolean setting, accepting common CLI spellings such as `true`, `1`, or `on`.
    Boolean,
    /// String setting constrained to one of the listed choices.
    Choice(&'static [&'static str]),
    /// Integer setting constrained to an inclusive range.
    IntegerRange {
        /// Minimum accepted value.
        min: u16,
        /// Maximum accepted value.
        max: u16,
    },
    /// Signed integer setting constrained to an inclusive range.
    SignedIntegerRange {
        /// Minimum accepted value.
        min: i32,
        /// Maximum accepted value.
        max: i32,
    },
}

/// One accepted syntax form for a setting.
#[derive(Debug, Clone, Copy)]
pub struct SettingSpecForm {
    /// User-facing syntax, such as `qp=<1..255>`.
    pub syntax: &'static str,
    /// Short explanation of the form.
    pub summary: &'static str,
}

/// One example setting string for help and generated documentation.
#[derive(Debug, Clone, Copy)]
pub struct SettingSpecExample {
    /// Complete setting spec string.
    pub spec: &'static str,
    /// Short explanation of the example.
    pub summary: &'static str,
}

/// Documentation manifest for one setting's accepted spec strings.
#[derive(Debug, Clone, Copy)]
pub struct SettingSpecManifest {
    /// Supported syntax forms.
    pub forms: &'static [SettingSpecForm],
    /// Example specs.
    pub examples: &'static [SettingSpecExample],
    /// Additional behavior notes.
    pub notes: &'static [&'static str],
}

/// Manifest entry for one global or codec-specific setting.
#[derive(Debug, Clone, Copy)]
pub struct SettingManifest {
    /// Stable setting name used in `--set name=value` and API setting specs.
    pub name: &'static str,
    /// Value shape accepted by this setting.
    pub value: SettingValue,
    /// Effective default value rendered by frontends, when the setting has a public default.
    pub default_value: Option<&'static str>,
    /// Help/spec manifest for this setting.
    pub spec: &'static SettingSpecManifest,
    /// Short user-facing summary.
    pub summary: &'static str,
}

impl SettingValue {
    /// Return whether `value` can be parsed by this declared setting shape.
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
            SettingValue::SignedIntegerRange { min, max } => value
                .parse::<i32>()
                .is_ok_and(|parsed| (min..=max).contains(&parsed)),
        }
    }
}

/// Return the name portion of a `name` or `name=value` setting spec.
pub fn setting_name(spec: &str) -> &str {
    spec.split_once('=').map_or(spec, |(name, _)| name)
}

/// Return the value portion of a `name=value` setting spec, if present.
pub fn setting_value(spec: &str) -> Option<&str> {
    spec.split_once('=').map(|(_, value)| value)
}

/// Parse a boolean setting from CLI-style setting specs.
///
/// Missing settings return `Ok(false)`. Bare matching settings imply `true`.
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

/// Parse a one-byte integer setting from CLI-style setting specs.
///
/// Returns `Ok(None)` when the setting is absent.
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

/// Parse a signed integer setting from CLI-style setting specs.
///
/// Returns `Ok(None)` when the setting is absent.
pub fn i32_setting(settings: &[String], name: &str) -> std::result::Result<Option<i32>, String> {
    for spec in settings {
        if setting_name(spec) != name {
            continue;
        }
        let value = setting_value(spec).unwrap_or("true");
        let parsed = value
            .parse::<i32>()
            .map_err(|_| format!("{name} expects a signed integer, got '{value}'"))?;
        return Ok(Some(parsed));
    }
    Ok(None)
}

/// Render a short label for the accepted values of `setting`.
pub fn setting_values_label(setting: SettingManifest) -> String {
    match setting.value {
        SettingValue::Boolean => "true|false".to_string(),
        SettingValue::Choice(values) => values.join("|"),
        SettingValue::IntegerRange { min, max } => format!("{min}..{max}"),
        SettingValue::SignedIntegerRange { min, max } => format!("{min}..{max}"),
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

/// Spec manifest for the global `lossless` setting.
pub const LOSSLESS_SETTING_SPEC: SettingSpecManifest = SettingSpecManifest {
    forms: LOSSLESS_SPEC_FORMS,
    examples: LOSSLESS_SPEC_EXAMPLES,
    notes: LOSSLESS_SPEC_NOTES,
};

/// Global setting that requests lossless coding when the selected codec supports it.
pub const LOSSLESS_SETTING: SettingManifest = SettingManifest {
    name: "lossless",
    value: SettingValue::Boolean,
    default_value: Some("false"),
    spec: &LOSSLESS_SETTING_SPEC,
    summary: "request lossless coding when supported",
};

/// Global settings that are available independently of selected codecs.
pub const GLOBAL_SETTINGS: &[SettingManifest] = &[LOSSLESS_SETTING];
