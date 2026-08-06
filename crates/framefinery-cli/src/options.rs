//! Command-line interface inventory for the `ff` frontend.
//!
//! This module exposes the user-visible command shape as data so help text,
//! tests, and embedding frontends can inspect the same option vocabulary that
//! the parser accepts.

/// Area of the `ff` interface where an option is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliOptionScope {
    /// Top-level help/version options.
    General,
    /// Positional input path and raw-input metadata options.
    Input,
    /// Filter-chain options.
    Filter,
    /// Encoder/output options.
    Output,
    /// Help and catalog discovery options.
    Discovery,
}

/// Value shape expected after a command-line option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliOptionValue {
    /// The option or command does not take a value.
    None,
    /// The option requires a following value.
    Required(&'static str),
    /// The option accepts an optional following value.
    Optional(&'static str),
}

/// Manifest for one public `ff` option or documented positional form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliOptionManifest {
    /// Syntax displayed in help text.
    pub syntax: &'static str,
    /// Tokens accepted by the parser for this option.
    ///
    /// Positional help-only forms use an empty slice.
    pub names: &'static [&'static str],
    /// Value shape expected by this option.
    pub value: CliOptionValue,
    /// Interface area that owns this option.
    pub scope: CliOptionScope,
    /// Short help summary.
    pub summary: &'static str,
    /// Additional behavior notes for generated docs or richer frontends.
    pub details: &'static [&'static str],
    /// Example argument snippets.
    pub examples: &'static [&'static str],
}

impl CliOptionManifest {
    /// Return true when `arg` is one of this option's accepted names.
    pub fn matches_name(self, arg: &str) -> bool {
        self.names.contains(&arg)
    }

    /// Return the preferred command-line spelling when the option has one.
    pub fn primary_name(self) -> Option<&'static str> {
        self.names.first().copied()
    }

    /// Return whether the option takes a required or optional value.
    pub fn accepts_value(self) -> bool {
        !matches!(self.value, CliOptionValue::None)
    }
}

/// Usage lines displayed by `ff --help`.
pub(crate) const CLI_USAGE: &[&str] = &[
    "ff --help [<codecs|filters [filter]|pixfmt|settings [setting]|presets>]",
    "ff --version",
    "ff codecs",
    "ff filters",
    "ff encode [<input>] [input-options] [--filter <spec>] --encode <codec:path> [output-options]",
];

pub(crate) const HELP_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "ff --help <topic>",
    names: &["-h", "--help", "help"],
    value: CliOptionValue::Optional("topic"),
    scope: CliOptionScope::General,
    summary: "Show the main help page or a focused help topic",
    details: &["accepted topics are codecs, filters, pixfmt, settings, and presets"],
    examples: &[
        "ff --help",
        "ff --help filters pattern",
        "ff --help settings qp",
    ],
};

pub(crate) const VERSION_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "ff --version",
    names: &["-V", "--version", "version"],
    value: CliOptionValue::None,
    scope: CliOptionScope::General,
    summary: "Print the installed FrameFinery version",
    details: &[],
    examples: &["ff --version"],
};

pub(crate) const HELP_CODECS_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "ff --help codecs",
    names: &[],
    value: CliOptionValue::None,
    scope: CliOptionScope::Discovery,
    summary: "Describe compiled codec stages",
    details: &["the legacy `ff codecs` command is still accepted"],
    examples: &["ff --help codecs"],
};

pub(crate) const HELP_FILTERS_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "ff --help filters [filter]",
    names: &[],
    value: CliOptionValue::Optional("filter"),
    scope: CliOptionScope::Discovery,
    summary: "Describe compiled filter stages or one filter spec",
    details: &["the legacy `ff filters` command is still accepted"],
    examples: &["ff --help filters", "ff --help filters pattern"],
};

pub(crate) const HELP_PIXFMT_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "ff --help pixfmt",
    names: &[],
    value: CliOptionValue::None,
    scope: CliOptionScope::Discovery,
    summary: "List accepted raw pixel-format names and aliases",
    details: &[],
    examples: &["ff --help pixfmt"],
};

pub(crate) const HELP_SETTINGS_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "ff --help settings [setting]",
    names: &[],
    value: CliOptionValue::Optional("setting"),
    scope: CliOptionScope::Discovery,
    summary: "List --set keys or describe one setting",
    details: &[],
    examples: &["ff --help settings", "ff --help settings qp"],
};

pub(crate) const HELP_PRESETS_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "ff --help presets",
    names: &[],
    value: CliOptionValue::None,
    scope: CliOptionScope::Discovery,
    summary: "List named encoder presets when available",
    details: &[],
    examples: &["ff --help presets"],
};

pub(crate) const INPUT_PATH_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "<input>",
    names: &[],
    value: CliOptionValue::Required("path"),
    scope: CliOptionScope::Input,
    summary: "Raw .yuv or Y4M input path; optional when the first filter is a source",
    details: &["compressed input decode is not implemented yet"],
    examples: &["input.y4m", "clip_1920x1080_30_50f_yuv420p8.yuv"],
};

pub(crate) const FILENAME_METADATA_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "filename metadata",
    names: &[],
    value: CliOptionValue::None,
    scope: CliOptionScope::Input,
    summary: "Names imply metadata with *_<WxH>[_<fps>][_<frames>f][_<pixfmt>].yuv; Y4M headers also provide metadata",
    details: &[
        "a bare .yuv filename with dimensions and no pixel-format suffix defaults to yuv420p8",
    ],
    examples: &["clip_640x360_30_50f_yuv444p8.yuv"],
};

pub(crate) const VIDEO_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "--video <WxH:fmt>",
    names: &["--video"],
    value: CliOptionValue::Required("WxH:fmt"),
    scope: CliOptionScope::Input,
    summary: "Override or provide raw metadata, e.g. 1920x1080:yuv444p",
    details: &["Y4M headers provide this metadata unless --video explicitly overrides it"],
    examples: &["--video 1920x1080:yuv420p8"],
};

pub(crate) const FPS_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "--fps <rate>",
    names: &["--fps"],
    value: CliOptionValue::Required("rate"),
    scope: CliOptionScope::Input,
    summary: "Input frame rate, e.g. 30, 29.97, or 30000/1001",
    details: &["Y4M headers provide this metadata unless --fps explicitly overrides it"],
    examples: &["--fps 30000/1001"],
};

pub(crate) const FRAMES_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "-n, --frames <count>",
    names: &["-n", "--frames"],
    value: CliOptionValue::Required("count"),
    scope: CliOptionScope::Input,
    summary: "Number of frames to process; omitted file inputs run to EOF",
    details: &["source filters require a frame count because they do not have input EOF"],
    examples: &["--frames 50", "-n 1"],
};

pub(crate) const FILTER_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "-f, --filter <spec>",
    names: &["-f", "--filter"],
    value: CliOptionValue::Required("spec"),
    scope: CliOptionScope::Filter,
    summary: "Filter stage, repeatable, e.g. pattern=black or identity",
    details: &["run `ff --help filters` for compiled filter specs"],
    examples: &[
        "--filter pattern=black",
        "--filter crop=x=0:y=0:w=640:h=360",
    ],
};

pub(crate) const ENCODE_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "--encode <codec:path>",
    names: &["--encode"],
    value: CliOptionValue::Required("codec:path"),
    scope: CliOptionScope::Output,
    summary: "Encoder codec/output endpoint, e.g. av2:output.obu",
    details: &["run `ff --help codecs` for compiled codec ids"],
    examples: &["--encode av2:output.obu", "--encode vvc:output.266"],
};

pub(crate) const RECON_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "--recon <path>",
    names: &["--recon", "--reconstruction"],
    value: CliOptionValue::Required("path"),
    scope: CliOptionScope::Output,
    summary: "Write the encoder's internal reconstructed raw frame stream",
    details: &["use only when the raw reconstruction file is needed"],
    examples: &["--recon out_recon.yuv"],
};

pub(crate) const PSNR_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "--psnr",
    names: &["--psnr"],
    value: CliOptionValue::None,
    scope: CliOptionScope::Output,
    summary: "Print per-frame PSNR from the encoder's internal reconstruction without writing it",
    details: &["uses the same in-encoder reconstruction path as --recon"],
    examples: &["--psnr"],
};

pub(crate) const NO_PROGRESS_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "--no-progress",
    names: &["--no-progress"],
    value: CliOptionValue::None,
    scope: CliOptionScope::Output,
    summary: "Suppress per-frame progress and metrics lines on stderr",
    details: &["errors and final process status are still reported normally"],
    examples: &["--no-progress"],
};

pub(crate) const SET_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "--set <key[=value]>",
    names: &["--set"],
    value: CliOptionValue::Required("key[=value]"),
    scope: CliOptionScope::Output,
    summary: "Encode setting; run ff --help settings for accepted keys",
    details: &["bare boolean setting keys imply true"],
    examples: &["--set lossless", "--set qp=24", "--set gop=30"],
};

pub(crate) const PRESET_OPTION: CliOptionManifest = CliOptionManifest {
    syntax: "--preset <name>",
    names: &["--preset"],
    value: CliOptionValue::Required("name"),
    scope: CliOptionScope::Output,
    summary: "Encoder preset name",
    details: &["reserved for future preset catalogs"],
    examples: &["--preset fast"],
};

pub(crate) const CLI_OPTIONS: &[CliOptionManifest] = &[
    HELP_OPTION,
    VERSION_OPTION,
    HELP_CODECS_OPTION,
    HELP_FILTERS_OPTION,
    HELP_PIXFMT_OPTION,
    HELP_SETTINGS_OPTION,
    HELP_PRESETS_OPTION,
    INPUT_PATH_OPTION,
    FILENAME_METADATA_OPTION,
    VIDEO_OPTION,
    FPS_OPTION,
    FRAMES_OPTION,
    FILTER_OPTION,
    ENCODE_OPTION,
    RECON_OPTION,
    PSNR_OPTION,
    NO_PROGRESS_OPTION,
    SET_OPTION,
    PRESET_OPTION,
];

/// Return every documented `ff` option manifest compiled into this build.
pub fn cli_options() -> &'static [CliOptionManifest] {
    CLI_OPTIONS
}

/// Return documented `ff` option manifests for one interface area.
pub fn cli_options_for_scope(
    scope: CliOptionScope,
) -> impl Iterator<Item = &'static CliOptionManifest> {
    CLI_OPTIONS
        .iter()
        .filter(move |option| option.scope == scope)
}
