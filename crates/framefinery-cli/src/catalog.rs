#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageKind {
    Codec,
    Filter,
}

#[derive(Debug, Clone, Copy)]
pub struct StageInfo {
    pub name: &'static str,
    pub kind: StageKind,
    pub feature: &'static str,
    pub compiled: bool,
    pub summary: &'static str,
    pub settings: &'static [SettingInfo],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingValue {
    Boolean,
    Choice(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy)]
pub struct SettingInfo {
    pub name: &'static str,
    pub value: SettingValue,
    pub summary: &'static str,
}

impl StageInfo {
    pub fn build_status(self) -> &'static str {
        if self.compiled {
            "compiled"
        } else {
            "not compiled"
        }
    }

    pub fn setting(self, name: &str) -> Option<SettingInfo> {
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
        }
    }
}

pub const GLOBAL_SETTINGS: &[SettingInfo] = &[SettingInfo {
    name: "lossless",
    value: SettingValue::Boolean,
    summary: "request lossless coding when supported",
}];

const NO_SETTINGS: &[SettingInfo] = &[];

const AV2_SETTINGS: &[SettingInfo] = &[SettingInfo {
    name: "predictive",
    value: SettingValue::Boolean,
    summary: "enable experimental AV2 multi-picture predictive coding tools",
}];

const PREDICTIVE_SETTING: SettingInfo = SettingInfo {
    name: "predictive",
    value: SettingValue::Boolean,
    summary: "enable experimental multi-picture predictive coding tools",
};

const VVC_FAST_SEARCH_VALUES: &[&str] = &[
    "off",
    "conservative",
    "moderate",
    "aggressive",
    "lossless-speed",
];

const VVC_SETTINGS: &[SettingInfo] = &[
    PREDICTIVE_SETTING,
    SettingInfo {
        name: "fast-search",
        value: SettingValue::Choice(VVC_FAST_SEARCH_VALUES),
        summary: "enable experimental VVC spatially guided mode-search pruning",
    },
];

pub fn global_setting(name: &str) -> Option<SettingInfo> {
    GLOBAL_SETTINGS
        .iter()
        .copied()
        .find(|setting| setting.name == name)
}

pub fn settings_label(global: &[SettingInfo], codec: &[SettingInfo]) -> String {
    let mut names = Vec::new();
    for setting in global.iter().chain(codec.iter()) {
        if !names.contains(&setting.name) {
            names.push(setting.name);
        }
    }
    if names.is_empty() {
        "-".to_string()
    } else {
        names.join(",")
    }
}

pub fn setting_values_label(setting: SettingInfo) -> String {
    match setting.value {
        SettingValue::Boolean => "true|false".to_string(),
        SettingValue::Choice(values) => values.join("|"),
    }
}

pub const CODECS: &[StageInfo] = &[
    StageInfo {
        name: "av2",
        kind: StageKind::Codec,
        feature: "codec-av2",
        compiled: cfg!(feature = "codec-av2"),
        summary: "imported experimental AV2 encoder model",
        settings: AV2_SETTINGS,
    },
    StageInfo {
        name: "vvc",
        kind: StageKind::Codec,
        feature: "codec-vvc",
        compiled: cfg!(feature = "codec-vvc"),
        summary: "imported experimental VVC/H.266 encoder model",
        settings: VVC_SETTINGS,
    },
];

pub const FILTERS: &[StageInfo] = &[
    StageInfo {
        name: "pattern",
        kind: StageKind::Filter,
        feature: "filter-pattern",
        compiled: cfg!(feature = "filter-pattern"),
        summary: "generated raw-video pattern source",
        settings: NO_SETTINGS,
    },
    StageInfo {
        name: "identity",
        kind: StageKind::Filter,
        feature: "filter-identity",
        compiled: cfg!(feature = "filter-identity"),
        summary: "no-op frame pass-through transform",
        settings: NO_SETTINGS,
    },
    StageInfo {
        name: "crop",
        kind: StageKind::Filter,
        feature: "filter-crop",
        compiled: cfg!(feature = "filter-crop"),
        summary: "rectangular crop filter scaffold",
        settings: NO_SETTINGS,
    },
    StageInfo {
        name: "scale",
        kind: StageKind::Filter,
        feature: "filter-scale",
        compiled: cfg!(feature = "filter-scale"),
        summary: "resize filter scaffold",
        settings: NO_SETTINGS,
    },
];

pub fn codec(name: &str) -> Option<StageInfo> {
    CODECS.iter().copied().find(|stage| stage.name == name)
}

pub fn filter(name: &str) -> Option<StageInfo> {
    FILTERS.iter().copied().find(|stage| stage.name == name)
}
