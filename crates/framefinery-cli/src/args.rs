use std::ffi::OsString;
use std::path::Path;

use framefinery_core::PixelFormat;

use crate::options::{self, CliOptionManifest, CliOptionScope};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help(Option<HelpTopic>),
    Version,
    Codecs,
    Filters,
    Encode(Box<EncodeArgs>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpTopic {
    Codecs,
    Filters(Option<String>),
    Pixfmt,
    Settings(Option<String>),
    Presets,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EncodeArgs {
    pub input: Option<String>,
    pub output: Option<String>,
    pub recon: Option<String>,
    pub psnr: bool,
    pub codec: Option<String>,
    pub video: Option<VideoSpec>,
    pub frames: Option<u32>,
    pub fps: Option<String>,
    pub explicit_video: bool,
    pub explicit_fps: bool,
    pub filters: Vec<String>,
    pub settings: Vec<String>,
    pub preset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoSpec {
    pub width: u32,
    pub height: u32,
    pub pixel_format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecPathSpec {
    pub codec: String,
    pub path: String,
}

pub fn help(version: &str) -> String {
    let mut text = format!("FrameFinery {version}\n\nUsage:\n");
    for usage in options::CLI_USAGE {
        text.push_str("  ");
        text.push_str(usage);
        text.push('\n');
    }

    text.push_str("\nInput options apply after <input>; output options apply after --encode.\n");

    text.push_str("\nInput options:\n");
    push_help_rows(
        &mut text,
        options::cli_options_for_scope(CliOptionScope::Input),
    );

    text.push_str("\nFilter options:\n");
    push_help_rows(
        &mut text,
        options::cli_options_for_scope(CliOptionScope::Filter),
    );

    text.push_str("\nOutput options:\n");
    push_help_rows(
        &mut text,
        options::cli_options_for_scope(CliOptionScope::Output),
    );

    text.push_str("\nStage discovery:\n");
    push_help_rows(
        &mut text,
        options::cli_options_for_scope(CliOptionScope::Discovery),
    );
    text
}

fn push_help_rows<'a>(text: &mut String, rows: impl IntoIterator<Item = &'a CliOptionManifest>) {
    let rows = rows.into_iter().collect::<Vec<_>>();
    let width = rows.iter().map(|row| row.syntax.len()).max().unwrap_or(0) + 2;
    for row in rows {
        text.push_str(&format!(
            "  {:<width$} {}\n",
            row.syntax,
            row.summary,
            width = width
        ));
    }
}

pub fn parse<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut cursor = Cursor::new(args.into_iter().skip(1).collect());
    let Some(command) = cursor.next() else {
        return Ok(Command::Help(None));
    };

    if let Some(topic) = command.strip_prefix("--help=") {
        return parse_help_from_topic(topic, cursor);
    }

    match command.as_str() {
        value if options::HELP_OPTION.matches_name(value) => parse_help(cursor),
        value if options::VERSION_OPTION.matches_name(value) => Ok(Command::Version),
        "codecs" => parse_no_extra(cursor, Command::Codecs, "codecs", Some(HelpTopic::Codecs)),
        "filters" => parse_no_extra(
            cursor,
            Command::Filters,
            "filters",
            Some(HelpTopic::Filters(None)),
        ),
        "encode" => parse_encode(cursor),
        other => Err(format!("unknown command '{other}'")),
    }
}

pub fn parse_os<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut converted = Vec::new();
    for arg in args {
        converted.push(
            arg.into_string()
                .map_err(|_| "arguments must be valid UTF-8".to_string())?,
        );
    }
    parse(converted)
}

fn parse_help(mut cursor: Cursor) -> Result<Command, String> {
    let Some(topic) = cursor.next() else {
        return Ok(Command::Help(None));
    };
    parse_help_from_topic(&topic, cursor)
}

fn parse_help_from_topic(topic: &str, cursor: Cursor) -> Result<Command, String> {
    let topic = parse_help_topic(topic)?;
    parse_help_topic_args(topic, cursor)
}

fn parse_help_topic(value: &str) -> Result<HelpTopic, String> {
    match value {
        "codecs" => Ok(HelpTopic::Codecs),
        "filters" => Ok(HelpTopic::Filters(None)),
        "pixfmt" => Ok(HelpTopic::Pixfmt),
        "settings" => Ok(HelpTopic::Settings(None)),
        "presets" => Ok(HelpTopic::Presets),
        other => Err(format!(
            "unknown help topic '{other}'; expected codecs, filters, pixfmt, settings, or presets"
        )),
    }
}

fn parse_help_topic_args(topic: HelpTopic, mut cursor: Cursor) -> Result<Command, String> {
    match topic {
        HelpTopic::Filters(None) => {
            parse_named_help_detail(&mut cursor, "filters", |name| HelpTopic::Filters(name))
        }
        HelpTopic::Settings(None) => {
            parse_named_help_detail(&mut cursor, "settings", |name| HelpTopic::Settings(name))
        }
        other => match cursor.next() {
            None => Ok(Command::Help(Some(other))),
            Some(extra) => Err(format!("--help accepts at most one topic, got '{extra}'")),
        },
    }
}

fn parse_named_help_detail(
    cursor: &mut Cursor,
    topic_name: &str,
    topic: impl FnOnce(Option<String>) -> HelpTopic,
) -> Result<Command, String> {
    let detail = cursor.next();
    match cursor.next() {
        None => Ok(Command::Help(Some(topic(detail)))),
        Some(extra) => Err(format!(
            "--help {topic_name} accepts at most one name, got '{extra}'"
        )),
    }
}

fn parse_no_extra(
    mut cursor: Cursor,
    command: Command,
    name: &str,
    help_topic: Option<HelpTopic>,
) -> Result<Command, String> {
    match cursor.next().as_deref() {
        None => Ok(command),
        Some(extra) if is_help_flag(extra) => Ok(Command::Help(help_topic)),
        Some(extra) => Err(format!("'{name}' does not accept argument '{extra}'")),
    }
}

fn parse_encode(mut cursor: Cursor) -> Result<Command, String> {
    let mut args = EncodeArgs::default();
    while let Some(arg) = cursor.next() {
        let option = arg.as_str();
        if is_help_flag(option) {
            return Ok(Command::Help(None));
        }
        if options::ENCODE_OPTION.matches_name(option) {
            if args.codec.is_some() || args.output.is_some() {
                return Err("encode accepts only one --encode endpoint".to_string());
            }
            let endpoint = parse_codec_path_spec(option, &cursor.value(option)?)?;
            args.codec = Some(endpoint.codec);
            args.output = Some(endpoint.path);
        } else if options::RECON_OPTION.matches_name(option) {
            if args.recon.is_some() {
                return Err("encode accepts only one reconstruction output".to_string());
            }
            args.recon = Some(cursor.value(option)?);
        } else if options::PSNR_OPTION.matches_name(option) {
            args.psnr = true;
        } else if options::VIDEO_OPTION.matches_name(option) {
            args.video = Some(parse_video_spec(option, &cursor.value(option)?)?);
            args.explicit_video = true;
        } else if options::FRAMES_OPTION.matches_name(option) {
            args.frames = Some(parse_u32(option, &cursor.value(option)?)?);
        } else if options::FPS_OPTION.matches_name(option) {
            args.fps = Some(parse_fps(option, &cursor.value(option)?)?);
            args.explicit_fps = true;
        } else if options::FILTER_OPTION.matches_name(option) {
            args.filters.push(cursor.value(option)?);
        } else if options::SET_OPTION.matches_name(option) {
            args.settings.push(parse_setting(&cursor.value(option)?));
        } else if options::PRESET_OPTION.matches_name(option) {
            args.preset = Some(cursor.value(option)?);
        } else {
            match option {
                other if other.starts_with('-') => {
                    return Err(format!("unknown encode option '{other}'"));
                }
                other => {
                    if args.input.is_some() {
                        return Err(format!("unexpected encode argument '{other}'"));
                    }
                    args.input = Some(other.to_string());
                }
            }
        }
    }

    resolve_encode_input_metadata(&mut args)?;
    if args.input.is_none() && args.filters.is_empty() {
        return Err("encode requires an input path or source filter".to_string());
    }
    if args.codec.is_none() || args.output.is_none() {
        return Err("encode requires --encode codec:path".to_string());
    }
    Ok(Command::Encode(Box::new(args)))
}

fn is_help_flag(value: &str) -> bool {
    matches!(value, "-h" | "--help")
}

fn parse_u32(option: &str, value: &str) -> Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("{option} expects a positive integer, got '{value}'"))?;
    if parsed == 0 {
        Err(format!("{option} expects a positive integer, got 0"))
    } else {
        Ok(parsed)
    }
}

fn parse_fps(option: &str, value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{option} expects a positive frame rate"));
    }

    if let Some((num, den)) = value.split_once('/') {
        let num = parse_u32(option, num)?;
        let den = parse_u32(option, den)?;
        return Ok(format!("{num}/{den}"));
    }

    let mut saw_digit = false;
    let mut saw_dot = false;
    for byte in value.bytes() {
        if byte.is_ascii_digit() {
            saw_digit = true;
        } else if byte == b'.' && !saw_dot {
            saw_dot = true;
        } else {
            return Err(format!(
                "{option} expects a positive frame rate, got '{value}'"
            ));
        }
    }
    if !saw_digit || value.trim_matches('0').trim_matches('.').is_empty() {
        return Err(format!(
            "{option} expects a positive frame rate, got '{value}'"
        ));
    }
    Ok(value.to_string())
}

fn parse_video_spec(option: &str, value: &str) -> Result<VideoSpec, String> {
    let (dimensions, pixel_format) = value
        .split_once(':')
        .ok_or_else(|| format!("{option} expects WxH:pixfmt, got '{value}'"))?;
    let split = dimensions
        .find('x')
        .or_else(|| dimensions.find('X'))
        .ok_or_else(|| format!("{option} expects WxH:pixfmt, got '{value}'"))?;
    let width = parse_u32(option, &dimensions[..split])?;
    let height = parse_u32(option, &dimensions[split + 1..])?;
    Ok(VideoSpec {
        width,
        height,
        pixel_format: Some(normalize_pixel_format(pixel_format)?),
    })
}

fn parse_codec_path_spec(option: &str, value: &str) -> Result<CodecPathSpec, String> {
    let (codec, path) = value
        .split_once(':')
        .ok_or_else(|| format!("{option} expects codec:path, got '{value}'"))?;
    if codec.is_empty() {
        return Err(format!("{option} codec must not be empty"));
    }
    if path.is_empty() {
        return Err(format!("{option} path must not be empty"));
    }
    Ok(CodecPathSpec {
        codec: codec.to_string(),
        path: path.to_string(),
    })
}

fn parse_setting(value: &str) -> String {
    if value.contains('=') {
        value.to_string()
    } else {
        format!("{value}=true")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct InputMetadata {
    video: Option<VideoSpec>,
    frames: Option<u32>,
    fps: Option<String>,
}

fn resolve_encode_input_metadata(args: &mut EncodeArgs) -> Result<(), String> {
    let inferred = match args.input.as_deref() {
        Some(input) => infer_input_metadata(input)?,
        None => InputMetadata::default(),
    };
    if args.frames.is_none() {
        args.frames = inferred.frames;
    }
    if args.fps.is_none() {
        args.fps = inferred.fps;
    }

    if args.video.is_none() {
        args.video = inferred.video;
    }
    Ok(())
}

fn infer_input_metadata(path: &str) -> Result<InputMetadata, String> {
    let Some(name) = Path::new(path).file_name().and_then(|name| name.to_str()) else {
        return Ok(InputMetadata::default());
    };
    let name = name.to_ascii_lowercase();
    let dimensions = find_dimensions(&name)?;
    let pixel_format = find_pixel_format(&name)?;
    let video = dimensions.map(|(width, height)| VideoSpec {
        width,
        height,
        pixel_format,
    });
    Ok(InputMetadata {
        video,
        frames: find_frame_count(&name)?,
        fps: find_fps(&name)?,
    })
}

fn find_dimensions(text: &str) -> Result<Option<(u32, u32)>, String> {
    let bytes = text.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() {
            start += 1;
            continue;
        }

        let mut split = start;
        while split < bytes.len() && bytes[split].is_ascii_digit() {
            split += 1;
        }
        if split == bytes.len() || !matches!(bytes[split], b'x' | b'X') {
            start = split.saturating_add(1);
            continue;
        }

        let height_start = split + 1;
        let mut end = height_start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == height_start {
            start = end.saturating_add(1);
            continue;
        }

        let width = parse_u32("input filename width", &text[start..split])?;
        let height = parse_u32("input filename height", &text[height_start..end])?;
        return Ok(Some((width, height)));
    }
    Ok(None)
}

fn find_pixel_format(text: &str) -> Result<Option<String>, String> {
    for token in pixel_format_filename_candidates() {
        if text.contains(&token) {
            return Ok(Some(normalize_pixel_format(&token)?));
        }
    }
    if text.ends_with(".yuv") {
        return Ok(Some("yuv420p8".to_string()));
    }
    Ok(None)
}

fn pixel_format_filename_candidates() -> Vec<String> {
    let mut tokens = Vec::new();
    for family in ["yuv420p", "yuv422p", "yuv444p", "gray"] {
        for bit_depth in (9..=16).rev() {
            tokens.push(format!("{family}{bit_depth}le"));
            tokens.push(format!("{family}{bit_depth}"));
        }
        tokens.push(format!("{family}8"));
        if family != "gray" {
            tokens.push(family.to_string());
        }
    }
    tokens.extend(
        ["gbrp8", "gbrp", "rgb24", "i420", "i422", "i444"]
            .into_iter()
            .map(str::to_string),
    );
    for chroma_digit in ["0", "2", "4"] {
        for bit_depth in (9..=16).rev() {
            tokens.push(format!("i{chroma_digit}{bit_depth:02}"));
        }
    }
    tokens
}

fn find_frame_count(text: &str) -> Result<Option<u32>, String> {
    let bytes = text.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        let suffix = &text[end..];
        if suffix.starts_with("frames") || suffix.starts_with('f') {
            return Ok(Some(parse_u32(
                "input filename frame count",
                &text[start..end],
            )?));
        }
        start = end.saturating_add(1);
    }
    Ok(None)
}

fn find_fps(text: &str) -> Result<Option<String>, String> {
    let Some((_, height_end)) = find_dimensions_span(text) else {
        return Ok(None);
    };
    let bytes = text.as_bytes();

    if height_end < bytes.len() && bytes[height_end] == b'p' {
        let fps_start = height_end + 1;
        let mut fps_end = fps_start;
        while fps_end < bytes.len() && bytes[fps_end].is_ascii_digit() {
            fps_end += 1;
        }
        if fps_end > fps_start {
            return Ok(Some(normalize_filename_fps(&text[fps_start..fps_end])?));
        }
    }

    let mut idx = height_end;
    while idx < bytes.len() && matches!(bytes[idx], b'_' | b'-' | b'.') {
        idx += 1;
    }
    let fps_start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx > fps_start {
        let suffix = &text[idx..];
        if suffix.starts_with(".yuv")
            || suffix.starts_with(".y4m")
            || suffix.starts_with('_')
            || suffix.starts_with('-')
        {
            return Ok(Some(normalize_filename_fps(&text[fps_start..idx])?));
        }
    }
    Ok(None)
}

fn normalize_filename_fps(value: &str) -> Result<String, String> {
    let fps = parse_u32("input filename fps", value)?;
    if fps >= 1000 && fps % 100 == 97 {
        return Ok(format!("{}.{:02}", fps / 100, fps % 100));
    }
    Ok(fps.to_string())
}

fn find_dimensions_span(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        if !bytes[start].is_ascii_digit() {
            start += 1;
            continue;
        }

        let mut split = start;
        while split < bytes.len() && bytes[split].is_ascii_digit() {
            split += 1;
        }
        if split == bytes.len() || !matches!(bytes[split], b'x' | b'X') {
            start = split.saturating_add(1);
            continue;
        }

        let height_start = split + 1;
        let mut end = height_start;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end > height_start {
            return Some((start, end));
        }
        start = end.saturating_add(1);
    }
    None
}

fn normalize_pixel_format(value: &str) -> Result<String, String> {
    let pixel_format = value.trim().to_ascii_lowercase();
    if pixel_format.is_empty() {
        return Err("pixel format must not be empty".to_string());
    }
    Ok(pixel_format.parse::<PixelFormat>()?.name())
}

#[derive(Debug, Clone)]
struct Cursor {
    args: Vec<String>,
    index: usize,
}

impl Cursor {
    fn new(args: Vec<String>) -> Self {
        Self { args, index: 0 }
    }

    fn next(&mut self) -> Option<String> {
        let value = self.args.get(self.index).cloned();
        if value.is_some() {
            self.index += 1;
        }
        value
    }

    fn value(&mut self, option: &str) -> Result<String, String> {
        let value = self
            .next()
            .ok_or_else(|| format!("{option} requires a value"))?;
        if value.starts_with('-') {
            Err(format!("{option} requires a value, got option '{value}'"))
        } else {
            Ok(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(words: &[&str]) -> Result<Command, String> {
        parse(words.iter().map(|word| (*word).to_string()))
    }

    #[test]
    fn parses_optional_help_topics() {
        assert_eq!(parse_words(&["ff", "--help"]).unwrap(), Command::Help(None));
        assert_eq!(
            parse_words(&["ff", "--help", "codecs"]).unwrap(),
            Command::Help(Some(HelpTopic::Codecs))
        );
        assert_eq!(
            parse_words(&["ff", "--help", "filters"]).unwrap(),
            Command::Help(Some(HelpTopic::Filters(None)))
        );
        assert_eq!(
            parse_words(&["ff", "--help", "filters", "pattern"]).unwrap(),
            Command::Help(Some(HelpTopic::Filters(Some("pattern".to_string()))))
        );
        assert_eq!(
            parse_words(&["ff", "--help=filters", "identity"]).unwrap(),
            Command::Help(Some(HelpTopic::Filters(Some("identity".to_string()))))
        );
        assert_eq!(
            parse_words(&["ff", "--help=pixfmt"]).unwrap(),
            Command::Help(Some(HelpTopic::Pixfmt))
        );
        assert_eq!(
            parse_words(&["ff", "help", "settings"]).unwrap(),
            Command::Help(Some(HelpTopic::Settings(None)))
        );
        assert_eq!(
            parse_words(&["ff", "--help", "settings", "qp"]).unwrap(),
            Command::Help(Some(HelpTopic::Settings(Some("qp".to_string()))))
        );
        assert_eq!(
            parse_words(&["ff", "--help=settings", "lossless"]).unwrap(),
            Command::Help(Some(HelpTopic::Settings(Some("lossless".to_string()))))
        );
        assert_eq!(
            parse_words(&["ff", "--help", "presets"]).unwrap(),
            Command::Help(Some(HelpTopic::Presets))
        );
        assert_eq!(
            parse_words(&["ff", "codecs", "--help"]).unwrap(),
            Command::Help(Some(HelpTopic::Codecs))
        );
        assert_eq!(
            parse_words(&["ff", "filters", "--help"]).unwrap(),
            Command::Help(Some(HelpTopic::Filters(None)))
        );
    }

    #[test]
    fn rejects_unknown_help_topics() {
        let err = parse_words(&["ff", "--help", "unknown"]).unwrap_err();
        assert_eq!(
            err,
            "unknown help topic 'unknown'; expected codecs, filters, pixfmt, settings, or presets"
        );

        let err = parse_words(&["ff", "--help", "codecs", "extra"]).unwrap_err();
        assert_eq!(err, "--help accepts at most one topic, got 'extra'");

        let err = parse_words(&["ff", "--help=pixfmt", "extra"]).unwrap_err();
        assert_eq!(err, "--help accepts at most one topic, got 'extra'");

        let err = parse_words(&["ff", "--help", "filters", "pattern", "extra"]).unwrap_err();
        assert_eq!(err, "--help filters accepts at most one name, got 'extra'");

        let err = parse_words(&["ff", "--help", "settings", "qp", "extra"]).unwrap_err();
        assert_eq!(err, "--help settings accepts at most one name, got 'extra'");
    }

    #[test]
    fn parses_encode_command() {
        let command = parse_words(&[
            "ff",
            "encode",
            "in.yuv",
            "--video",
            "64x64:yuv444p",
            "--filter",
            "scale=w=64:h=64",
            "--encode",
            "av2:out.obu",
            "--recon",
            "out_recon.yuv",
            "--set",
            "lossless",
        ])
        .unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert_eq!(args.input.as_deref(), Some("in.yuv"));
        assert_eq!(args.output.as_deref(), Some("out.obu"));
        assert_eq!(args.recon.as_deref(), Some("out_recon.yuv"));
        assert!(!args.psnr);
        assert_eq!(args.codec.as_deref(), Some("av2"));
        assert_eq!(
            args.video,
            Some(VideoSpec {
                width: 64,
                height: 64,
                pixel_format: Some("yuv444p8".to_string())
            })
        );
        assert_eq!(args.filters, vec!["scale=w=64:h=64"]);
        assert_eq!(args.settings, vec!["lossless=true"]);
    }

    #[test]
    fn parses_encode_psnr_option() {
        let command = parse_words(&[
            "ff",
            "encode",
            "in.yuv",
            "--video",
            "64x64:yuv420p",
            "--encode",
            "av2:out.obu",
            "--psnr",
        ])
        .unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert!(args.psnr);
        assert_eq!(args.recon, None);
    }

    #[test]
    fn parses_encode_qp_setting() {
        let command = parse_words(&[
            "ff",
            "encode",
            "in.yuv",
            "--video",
            "64x64:yuv420p",
            "--encode",
            "av2:out.obu",
            "--set",
            "qp=24",
        ])
        .unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert_eq!(args.settings, vec!["qp=24"]);
    }

    #[test]
    fn infers_dimensions_fps_and_format_from_input_filename() {
        let command = parse_words(&[
            "ff",
            "encode",
            "screen_640x360_1f_yuv444p8.yuv",
            "--encode",
            "av2:out.obu",
        ])
        .unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert_eq!(
            args.video,
            Some(VideoSpec {
                width: 640,
                height: 360,
                pixel_format: Some("yuv444p8".to_string())
            })
        );
        assert_eq!(args.frames, Some(1));
        assert_eq!(args.fps, None);
    }

    #[test]
    fn infers_high_bit_depth_format_from_input_filename() {
        let command = parse_words(&[
            "ff",
            "encode",
            "screen_640x360_1f_yuv444p14le.yuv",
            "--encode",
            "av2:out.obu",
        ])
        .unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert_eq!(
            args.video,
            Some(VideoSpec {
                width: 640,
                height: 360,
                pixel_format: Some("yuv444p14le".to_string())
            })
        );
        assert_eq!(args.frames, Some(1));
    }

    #[test]
    fn infers_dimensions_and_fps_from_input_filename_without_format() {
        let command = parse_words(&[
            "ff",
            "encode",
            "RaceHorses_416x240_30.yuv",
            "--encode",
            "av2:out.obu",
        ])
        .unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert_eq!(
            args.video,
            Some(VideoSpec {
                width: 416,
                height: 240,
                pixel_format: Some("yuv420p8".to_string())
            })
        );
        assert_eq!(args.fps.as_deref(), Some("30"));
    }

    #[test]
    fn defaults_bare_yuv_filename_to_yuv420p8() {
        let command =
            parse_words(&["ff", "encode", "clip_64x32.yuv", "--encode", "av2:out.obu"]).unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert_eq!(
            args.video,
            Some(VideoSpec {
                width: 64,
                height: 32,
                pixel_format: Some("yuv420p8".to_string())
            })
        );
    }

    #[test]
    fn infers_ctc_style_fps_from_input_filename() {
        let command = parse_words(&[
            "ff",
            "encode",
            "MotorCycle_SDR_640x360p2997_yuv444p.y4m",
            "--encode",
            "av2:out.obu",
        ])
        .unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert_eq!(args.fps.as_deref(), Some("29.97"));
        assert_eq!(
            args.video,
            Some(VideoSpec {
                width: 640,
                height: 360,
                pixel_format: Some("yuv444p8".to_string())
            })
        );
    }

    #[test]
    fn explicit_input_options_override_filename_metadata() {
        let command = parse_words(&[
            "ff",
            "encode",
            "clip_416x240_30_1f_yuv420p8.yuv",
            "--video",
            "64x64:yuv444p",
            "--fps",
            "30000/1001",
            "--frames",
            "2",
            "--encode",
            "av2:out.obu",
        ])
        .unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert_eq!(
            args.video,
            Some(VideoSpec {
                width: 64,
                height: 64,
                pixel_format: Some("yuv444p8".to_string())
            })
        );
        assert_eq!(args.fps.as_deref(), Some("30000/1001"));
        assert_eq!(args.frames, Some(2));
    }

    #[test]
    fn rejects_malformed_video_spec() {
        let err = parse_words(&[
            "ff",
            "encode",
            "in.yuv",
            "--video",
            "64:yuv444p",
            "--encode",
            "av2:out.obu",
        ])
        .unwrap_err();
        assert_eq!(err, "--video expects WxH:pixfmt, got '64:yuv444p'");
    }

    #[test]
    fn encode_requires_core_io_arguments() {
        let err = parse_words(&["ff", "encode", "--encode", "av2:out.obu"]).unwrap_err();
        assert_eq!(err, "encode requires an input path or source filter");
    }

    #[test]
    fn accepts_encode_without_video_spec() {
        let command = parse_words(&["ff", "encode", "in.yuv", "--encode", "av2:out.obu"]).unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert_eq!(args.input.as_deref(), Some("in.yuv"));
        assert_eq!(args.video, None);
        assert_eq!(args.codec.as_deref(), Some("av2"));
        assert_eq!(args.output.as_deref(), Some("out.obu"));
    }

    #[test]
    fn accepts_encode_starting_with_source_filter() {
        let command = parse_words(&[
            "ff",
            "encode",
            "--filter",
            "pattern=black",
            "--video",
            "16x16:yuv420p",
            "--frames",
            "1",
            "--encode",
            "av2:out.obu",
        ])
        .unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert_eq!(args.input, None);
        assert_eq!(args.filters, vec!["pattern=black"]);
        assert_eq!(
            args.video,
            Some(VideoSpec {
                width: 16,
                height: 16,
                pixel_format: Some("yuv420p8".to_string())
            })
        );
        assert_eq!(args.frames, Some(1));
        assert_eq!(args.codec.as_deref(), Some("av2"));
        assert_eq!(args.output.as_deref(), Some("out.obu"));
    }

    #[test]
    fn accepts_numeric_planar_bit_depths_in_video_spec() {
        let command = parse_words(&[
            "ff",
            "encode",
            "in.yuv",
            "--video",
            "16x16:yuv420p9le",
            "--encode",
            "av2:out.obu",
        ])
        .unwrap();

        let Command::Encode(args) = command else {
            panic!("expected encode command");
        };
        assert_eq!(
            args.video,
            Some(VideoSpec {
                width: 16,
                height: 16,
                pixel_format: Some("yuv420p9le".to_string())
            })
        );
    }

    #[test]
    fn rejects_encode_endpoint_without_path() {
        let err = parse_words(&["ff", "encode", "in.yuv", "--encode", "av2"]).unwrap_err();
        assert_eq!(err, "--encode expects codec:path, got 'av2'");
    }

    #[test]
    fn rejects_encode_without_encoder_endpoint() {
        let err = parse_words(&["ff", "encode", "in.yuv"]).unwrap_err();
        assert_eq!(err, "encode requires --encode codec:path");
    }

    #[test]
    fn rejects_multiple_encode_inputs() {
        let err = parse_words(&[
            "ff",
            "encode",
            "in.yuv",
            "other.yuv",
            "--encode",
            "av2:out.obu",
        ])
        .unwrap_err();
        assert_eq!(err, "unexpected encode argument 'other.yuv'");
    }

    #[test]
    fn rejects_removed_compatibility_options() {
        for option in [
            "--input-format",
            "--raw-video",
            "--codec",
            "--input",
            "--output",
            "--pix-fmt",
            "--pixel-format",
            "--width",
            "--height",
            "--qp",
            "-hgt",
        ] {
            let err = parse_words(&[
                "ff",
                "encode",
                "in.yuv",
                option,
                "value",
                "--encode",
                "av2:out.obu",
            ])
            .unwrap_err();
            assert_eq!(err, format!("unknown encode option '{option}'"));
        }
    }

    #[test]
    fn help_is_owned_by_parser_options() {
        let text = help("test");
        for expected in [
            "ff encode [<input>]",
            "filename metadata",
            "*_<WxH>[_<fps>][_<frames>f][_<pixfmt>].yuv",
            "--encode <codec:path>",
            "--recon <path>",
            "--psnr",
            "--video <WxH:fmt>",
            "--fps <rate>",
            "-n, --frames <count>",
            "-f, --filter <spec>",
            "pattern=black",
            "--set <key[=value]>",
            "--preset <name>",
            "ff --help codecs",
            "ff --help filters",
            "ff --help pixfmt",
            "ff --help settings",
            "ff --help presets",
        ] {
            assert!(text.contains(expected), "missing help entry: {expected}");
        }
        for removed in [
            "-c, --codec <codec>",
            "-i, --input <path>",
            "-o, --output <path>",
            "--pix-fmt",
            "--pixel-format",
            "--width",
            "--height",
            "--lossless",
            "--set qp=<1..255>",
            "--qp <1..255>",
            "Compatibility options",
            "--input-format",
            "--raw-video",
        ] {
            assert!(!text.contains(removed), "stale help entry: {removed}");
        }
        let discovery = text
            .split("\nStage discovery:\n")
            .nth(1)
            .expect("help should have a stage discovery section");
        for removed_command in ["ff codecs", "ff filters"] {
            assert!(
                !discovery
                    .lines()
                    .any(|line| line.trim_start().starts_with(removed_command)),
                "stale help entry: {removed_command}"
            );
        }
        assert!(!text.contains("ff pipeline"));
        assert!(!text.contains("--decode"));
        assert!(text.contains("[input-options]"));
        assert!(text.contains("[output-options]"));
    }
}
