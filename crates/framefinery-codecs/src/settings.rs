use framefinery_core::{
    SettingManifest, SettingSpecExample, SettingSpecForm, SettingSpecManifest, SettingValue,
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

pub const QP_SETTING: SettingManifest = SettingManifest {
    name: "qp",
    value: SettingValue::IntegerRange { min: 1, max: 255 },
    spec: &QP_SETTING_SPEC,
    summary: "request lossy quantization quality",
};

const PREDICTIVE_SPEC_FORMS: &[SettingSpecForm] = &[
    SettingSpecForm {
        syntax: "predictive",
        summary: "enable experimental multi-picture predictive coding",
    },
    SettingSpecForm {
        syntax: "predictive=<bool>",
        summary: "explicitly enable or disable predictive coding",
    },
];

const PREDICTIVE_SPEC_EXAMPLES: &[SettingSpecExample] = &[SettingSpecExample {
    spec: "predictive",
    summary: "enable temporal prediction tools where the selected codec supports them",
}];

const PREDICTIVE_SPEC_NOTES: &[&str] = &["experimental setting; behavior is codec-specific"];

pub const PREDICTIVE_SETTING_SPEC: SettingSpecManifest = SettingSpecManifest {
    forms: PREDICTIVE_SPEC_FORMS,
    examples: PREDICTIVE_SPEC_EXAMPLES,
    notes: PREDICTIVE_SPEC_NOTES,
};

pub const PREDICTIVE_SETTING: SettingManifest = SettingManifest {
    name: "predictive",
    value: SettingValue::Boolean,
    spec: &PREDICTIVE_SETTING_SPEC,
    summary: "enable experimental multi-picture predictive coding tools",
};
