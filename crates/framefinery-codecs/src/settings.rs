use framefinery_api::{
    i32_setting, SettingManifest, SettingSpecExample, SettingSpecForm, SettingSpecManifest,
    SettingValue,
};

const QP_SPEC_FORMS: &[SettingSpecForm] = &[SettingSpecForm {
    syntax: "qp=<1..255>",
    summary: "request lossy quantization quality",
}];

const QP_SPEC_EXAMPLES: &[SettingSpecExample] = &[SettingSpecExample {
    spec: "qp=24",
    summary: "request a lossy quantization point",
}];

const QP_SPEC_NOTES: &[&str] = &[
    "lower values preserve more detail",
    "mutually exclusive with lossless",
];

pub const QP_SETTING_SPEC: SettingSpecManifest = SettingSpecManifest {
    forms: QP_SPEC_FORMS,
    examples: QP_SPEC_EXAMPLES,
    notes: QP_SPEC_NOTES,
};

pub(crate) const QP_SETTING: SettingManifest = SettingManifest {
    name: "qp",
    value: SettingValue::IntegerRange { min: 1, max: 255 },
    default_value: Some("default"),
    spec: &QP_SETTING_SPEC,
    summary: "request lossy quantization quality",
};

const GOP_MAX: i32 = 65535;

const GOP_SPEC_FORMS: &[SettingSpecForm] = &[SettingSpecForm {
    syntax: "gop=<-1..65535>",
    summary: "set the intra-frame period for temporal prediction",
}];

const GOP_SPEC_EXAMPLES: &[SettingSpecExample] = &[
    SettingSpecExample {
        spec: "gop=-1",
        summary: "encode one intra frame followed by unbounded predictive frames",
    },
    SettingSpecExample {
        spec: "gop=0",
        summary: "encode every frame as intra-only",
    },
    SettingSpecExample {
        spec: "gop=30",
        summary: "insert an intra frame every 30 frames",
    },
];

const GOP_SPEC_NOTES: &[&str] = &[
    "default is gop=-1",
    "gop=0 disables temporal prediction",
    "positive values reset codec reference state at each GOP boundary",
];

pub(crate) const GOP_SETTING_SPEC: SettingSpecManifest = SettingSpecManifest {
    forms: GOP_SPEC_FORMS,
    examples: GOP_SPEC_EXAMPLES,
    notes: GOP_SPEC_NOTES,
};

pub(crate) const GOP_SETTING: SettingManifest = SettingManifest {
    name: "gop",
    value: SettingValue::SignedIntegerRange {
        min: -1,
        max: GOP_MAX,
    },
    default_value: Some("-1"),
    spec: &GOP_SETTING_SPEC,
    summary: "set the intra-frame period for temporal prediction",
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GopMode {
    IntraOnly,
    Infinite,
    Fixed(usize),
}

impl Default for GopMode {
    fn default() -> Self {
        Self::Infinite
    }
}

impl GopMode {
    pub(crate) fn from_i32(value: i32) -> Result<Self, String> {
        match value {
            -1 => Ok(Self::Infinite),
            0 => Ok(Self::IntraOnly),
            1..=GOP_MAX => Ok(Self::Fixed(value as usize)),
            _ => Err(format!(
                "gop expects an integer from -1 through {GOP_MAX}, got '{value}'"
            )),
        }
    }

    pub(crate) fn from_settings(settings: &[String]) -> Result<Self, String> {
        i32_setting(settings, "gop")?
            .map(Self::from_i32)
            .unwrap_or_else(|| Ok(Self::default()))
    }

    pub(crate) fn is_predictive(self) -> bool {
        !matches!(self, Self::IntraOnly)
    }

    pub(crate) fn is_intra_frame(self, frame_idx: usize) -> bool {
        match self {
            Self::IntraOnly => true,
            Self::Infinite => frame_idx == 0,
            Self::Fixed(period) => period <= 1 || frame_idx % period == 0,
        }
    }

    pub(crate) fn is_predictive_frame(self, frame_idx: usize) -> bool {
        self.is_predictive() && !self.is_intra_frame(frame_idx)
    }

    pub(crate) fn resets_references_before(self, frame_idx: usize) -> bool {
        frame_idx == 0 || self.is_intra_frame(frame_idx)
    }

    pub(crate) fn as_i32(self) -> i32 {
        match self {
            Self::IntraOnly => 0,
            Self::Infinite => -1,
            Self::Fixed(period) => period as i32,
        }
    }
}
