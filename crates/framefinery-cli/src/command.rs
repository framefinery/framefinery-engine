#![cfg_attr(not(feature = "video-encoders"), allow(dead_code))]

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[cfg(feature = "filter-identity")]
use framefinery_core::IdentityFilter;
use framefinery_core::{
    boolean_setting_enabled, convert_frame_format, planar_sample_sse, run_frame_filter_pipeline,
    setting_name, setting_value, CodecEncodeFrameMetrics, CodecEncodeFrameMetricsCallback,
    CodecEncodeRequest, CodecManifest, Filter, Frame, FrameInfo, MediaError, PixelFormat,
    SampleBitDepth, SettingManifest, Sink, Source, VERSION,
};
#[cfg(feature = "filter-pattern")]
use framefinery_core::{generate_pattern_stream, PatternKind};

use crate::args::{self, Command, EncodeArgs, HelpTopic};
use crate::catalog::{
    self, setting_values_label, settings_label, FilterManifest, CODECS, FILTERS, GLOBAL_SETTINGS,
};

pub fn run<I>(raw_args: I) -> ExitCode
where
    I: IntoIterator<Item = OsString>,
{
    match args::parse_os(raw_args) {
        Ok(Command::Help(topic)) => print_help(topic),
        Ok(Command::Version) => {
            println!("ff {VERSION}");
            ExitCode::SUCCESS
        }
        Ok(Command::Codecs) => {
            print_codec_table("Codecs", CODECS);
            ExitCode::SUCCESS
        }
        Ok(Command::Filters) => {
            print_filter_table("Filters", FILTERS);
            ExitCode::SUCCESS
        }
        Ok(Command::Encode(args)) => run_encode(*args),
        Err(message) => {
            eprintln!("error: {message}");
            eprintln!("run 'ff --help' for usage");
            ExitCode::from(2)
        }
    }
}

fn run_encode(args: EncodeArgs) -> ExitCode {
    let codec_name = args.codec.as_deref().expect("encode parser requires codec");
    let Some(codec) = catalog::codec(codec_name) else {
        eprintln!("error: unknown codec '{codec_name}'");
        eprintln!("run 'ff codecs' to list known codec stages");
        return ExitCode::from(2);
    };

    if let Some(exit) = validate_codec_settings(codec, &args.settings) {
        return exit;
    }

    if let Some(exit) = validate_filters(&args) {
        return exit;
    }

    let job = match encode_job_for_codec(codec, &args) {
        Ok(job) => job,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::from(1);
        }
    };

    print_encode_config(codec.name, &args, &job);

    match encode_with_model(codec, &args, job) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(1)
        }
    }
}

fn validate_codec_settings(codec: CodecManifest, settings: &[String]) -> Option<ExitCode> {
    for spec in settings {
        let name = setting_name(spec);
        let Some(setting) = catalog::global_setting(name).or_else(|| codec.setting(name)) else {
            eprintln!("error: unknown encode setting '{name}'");
            eprintln!(
                "accepted settings: {}",
                settings_label(GLOBAL_SETTINGS, codec.settings)
            );
            return Some(ExitCode::from(2));
        };
        let value = setting_value(spec).unwrap_or("true");
        if !setting.value.accepts(value) {
            eprintln!(
                "error: codec '{}' setting '{}' expects one of {}, got '{}'",
                codec.name,
                setting.name,
                setting_values_label(setting),
                value
            );
            return Some(ExitCode::from(2));
        }
    }
    None
}

fn validate_filters(args: &EncodeArgs) -> Option<ExitCode> {
    let filters = &args.filters;
    for filter_name in args::filter_names(filters) {
        if catalog::filter(filter_name).is_none() {
            eprintln!("error: unknown filter '{filter_name}'");
            eprintln!("run 'ff filters' to list known filter stages");
            return Some(ExitCode::from(2));
        };
    }
    match parse_filter_pipeline(args) {
        Ok(_) => None,
        Err(message) => {
            eprintln!("error: {message}");
            Some(ExitCode::from(4))
        }
    }
}

#[derive(Debug, Clone)]
enum EncodeInput {
    Path(PathBuf),
    Pattern(PatternSourceSpec),
}

#[derive(Debug, Clone)]
struct PatternSourceSpec {
    pattern: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransformFilterSpec {
    Identity,
}

#[derive(Debug, Clone)]
struct FilterPipelineSpec {
    source: Option<PatternSourceSpec>,
    transforms: Vec<TransformFilterSpec>,
}

impl PatternSourceSpec {
    fn from_filter(spec: &str) -> Result<Self, String> {
        if args::filter_names(&[spec.to_string()]).next() != Some("pattern") {
            return Err("source filter must be pattern=<name>".to_string());
        }
        let Some((_, value)) = spec.split_once('=').or_else(|| spec.split_once(':')) else {
            return Err("pattern source expects --filter pattern=<name>".to_string());
        };
        let pattern = parse_pattern_source_name(value)?;
        Ok(Self { pattern })
    }
}

fn parse_pattern_source_name(value: &str) -> Result<String, String> {
    #[cfg(feature = "filter-pattern")]
    {
        let pattern = PatternKind::parse(value).map_err(|err| err.to_string())?;
        Ok(pattern.name().to_string())
    }
    #[cfg(not(feature = "filter-pattern"))]
    {
        let _ = value;
        Err("unknown filter 'pattern'".to_string())
    }
}

fn parse_filter_pipeline(args: &EncodeArgs) -> Result<FilterPipelineSpec, String> {
    let mut source = None;
    let mut transforms = Vec::new();
    for (index, spec) in args.filters.iter().enumerate() {
        let name = args::filter_names(std::slice::from_ref(spec))
            .next()
            .unwrap_or(spec.as_str());
        match name {
            "pattern" => {
                if catalog::filter("pattern").is_none() {
                    return Err("unknown filter 'pattern'".to_string());
                }
                if args.input.is_some() {
                    return Err(
                        "source filter 'pattern' cannot be used after an input path".to_string()
                    );
                }
                if index != 0 {
                    return Err("source filter 'pattern' must be the first filter".to_string());
                }
                if source.is_some() {
                    return Err("encode accepts only one source filter".to_string());
                }
                source = Some(PatternSourceSpec::from_filter(spec)?);
            }
            "identity" => {
                if catalog::filter("identity").is_none() {
                    return Err("unknown filter 'identity'".to_string());
                }
                if spec.contains('=') || spec.contains(':') {
                    return Err("identity filter does not accept options".to_string());
                }
                transforms.push(TransformFilterSpec::Identity);
            }
            "crop" | "scale" => {
                if catalog::filter(name).is_none() {
                    return Err(format!("unknown filter '{name}'"));
                }
                return Err(format!(
                    "filter '{name}' is available as a discovery scaffold but execution is not implemented yet"
                ));
            }
            other => {
                return Err(format!("filter '{other}' has no execution model wired yet"));
            }
        }
    }
    if args.input.is_none() && source.is_none() {
        return Err(
            "encode without an input requires a source filter such as --filter pattern=black"
                .to_string(),
        );
    }
    Ok(FilterPipelineSpec { source, transforms })
}

fn generated_pattern_input(job: &EncodeJob, source: &PatternSourceSpec) -> Result<Vec<u8>, String> {
    #[cfg(feature = "filter-pattern")]
    {
        let pattern = PatternKind::parse(&source.pattern).map_err(|err| err.to_string())?;
        let info =
            FrameInfo::new(job.width, job.height, job.format).map_err(|err| err.to_string())?;
        return generate_pattern_stream(info, pattern, job.frames).map_err(|err| err.to_string());
    }
    #[cfg(not(feature = "filter-pattern"))]
    {
        let _ = (job, source);
        Err("unknown filter 'pattern'".to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Y4mMetadata {
    width: usize,
    height: usize,
    format: PixelFormat,
    fps: Option<String>,
}

fn is_y4m_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("y4m"))
}

fn read_y4m_file_metadata(path: &Path) -> Result<Option<Y4mMetadata>, String> {
    if !is_y4m_path(path) {
        return Ok(None);
    }
    let file = File::open(path)
        .map_err(|err| format!("failed to open input '{}': {err}", path.display()))?;
    let mut reader = BufReader::new(file);
    read_y4m_stream_header(&mut reader, &y4m_context(path)).map(Some)
}

fn y4m_context(path: &Path) -> String {
    format!("Y4M input '{}'", path.display())
}

fn read_y4m_stream_header<R: BufRead>(
    reader: &mut R,
    context: &str,
) -> Result<Y4mMetadata, String> {
    let mut header = Vec::new();
    let bytes = reader
        .read_until(b'\n', &mut header)
        .map_err(|err| format!("failed to read {context} header: {err}"))?;
    if bytes == 0 {
        return Err(format!("{context} is empty"));
    }
    if !header.ends_with(b"\n") {
        return Err(format!("{context} header is missing a newline"));
    }
    let header = String::from_utf8(header)
        .map_err(|_| format!("{context} header must be valid UTF-8/ASCII"))?;
    parse_y4m_metadata(
        header.trim_end_matches(|ch| ch == '\r' || ch == '\n'),
        context,
    )
}

fn parse_y4m_metadata(header: &str, context: &str) -> Result<Y4mMetadata, String> {
    let fields = y4m_header_fields(header, context)?;
    Ok(Y4mMetadata {
        width: parse_y4m_positive_usize(y4m_header_tag(&fields, 'W'), "width", context)?,
        height: parse_y4m_positive_usize(y4m_header_tag(&fields, 'H'), "height", context)?,
        format: y4m_pixel_format(y4m_header_tag(&fields, 'C'))?,
        fps: y4m_fps(y4m_header_tag(&fields, 'F'), context)?,
    })
}

fn y4m_header_fields<'a>(header: &'a str, context: &str) -> Result<Vec<&'a str>, String> {
    let fields = header.split_whitespace().collect::<Vec<_>>();
    if fields.first() != Some(&"YUV4MPEG2") {
        return Err(format!("{context} is not a Y4M stream"));
    }
    Ok(fields)
}

fn y4m_header_tag<'a>(fields: &'a [&str], tag: char) -> Option<&'a str> {
    fields.iter().skip(1).find_map(|field| {
        let mut chars = field.chars();
        if chars.next() == Some(tag) {
            Some(chars.as_str())
        } else {
            None
        }
    })
}

fn parse_y4m_positive_usize(
    value: Option<&str>,
    field: &str,
    context: &str,
) -> Result<usize, String> {
    let value = value.ok_or_else(|| format!("{context} header is missing {field}"))?;
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{context} {field} expects an integer, got '{value}'"))?;
    if parsed == 0 {
        Err(format!(
            "{context} {field} expects a positive integer, got 0"
        ))
    } else {
        Ok(parsed)
    }
}

fn y4m_pixel_format(chroma_tag: Option<&str>) -> Result<PixelFormat, String> {
    let normalized = chroma_tag.unwrap_or("420").to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "420" | "420jpeg" | "420mpeg2" | "420paldv"
    ) {
        return Ok(PixelFormat::yuv420(8).expect("8-bit YUV must be supported"));
    }
    if let Some(bits) = numeric_y4m_bit_depth(&normalized, "420p") {
        return PixelFormat::yuv420(bits)
            .ok_or_else(|| format!("unsupported Y4M chroma format: {normalized}"));
    }
    if normalized == "422" {
        return Ok(PixelFormat::yuv422(8).expect("8-bit YUV must be supported"));
    }
    if let Some(bits) = numeric_y4m_bit_depth(&normalized, "422p") {
        return PixelFormat::yuv422(bits)
            .ok_or_else(|| format!("unsupported Y4M chroma format: {normalized}"));
    }
    if normalized == "444" {
        return Ok(PixelFormat::yuv444(8).expect("8-bit YUV must be supported"));
    }
    if let Some(bits) = numeric_y4m_bit_depth(&normalized, "444p") {
        return PixelFormat::yuv444(bits)
            .ok_or_else(|| format!("unsupported Y4M chroma format: {normalized}"));
    }
    Err(format!(
        "unsupported Y4M chroma format: {}",
        chroma_tag.unwrap_or("<default>")
    ))
}

fn numeric_y4m_bit_depth(normalized: &str, prefix: &str) -> Option<u8> {
    normalized.strip_prefix(prefix)?.parse::<u8>().ok()
}

fn y4m_fps(value: Option<&str>, context: &str) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let (num, den) = value
        .split_once(':')
        .ok_or_else(|| format!("{context} fps expects N:D, got '{value}'"))?;
    let num = num
        .parse::<u32>()
        .map_err(|_| format!("{context} fps expects N:D, got '{value}'"))?;
    let den = den
        .parse::<u32>()
        .map_err(|_| format!("{context} fps expects N:D, got '{value}'"))?;
    if num == 0 || den == 0 {
        return Err(format!("{context} fps expects positive N:D, got '{value}'"));
    }
    if den == 1 {
        Ok(Some(num.to_string()))
    } else {
        Ok(Some(format!("{num}/{den}")))
    }
}

fn validate_y4m_job_metadata(
    metadata: &Y4mMetadata,
    job: &EncodeJob,
    path: &Path,
) -> Result<(), String> {
    if metadata.width != job.width
        || metadata.height != job.height
        || metadata.format != job.source_format
    {
        return Err(format!(
            "Y4M input '{}' declares {}x{}:{}, but encode job expects {}x{}:{}",
            path.display(),
            metadata.width,
            metadata.height,
            metadata.format,
            job.width,
            job.height,
            job.source_format
        ));
    }
    Ok(())
}

struct Y4mFrameReader<R> {
    inner: R,
    frame_len: usize,
    frame: Vec<u8>,
    frame_offset: usize,
    frames_remaining: usize,
    frame_index: usize,
    context: String,
}

impl<R: BufRead> Y4mFrameReader<R> {
    fn new(mut inner: R, job: &EncodeJob, path: &Path) -> Result<Self, String> {
        let context = y4m_context(path);
        let metadata = read_y4m_stream_header(&mut inner, &context)?;
        if job.validate_y4m_metadata {
            validate_y4m_job_metadata(&metadata, job, path)?;
        }
        let frame_len = job
            .source_format
            .frame_len(job.width, job.height)
            .ok_or_else(|| {
                format!(
                    "frame length overflow for {}x{}:{}",
                    job.width, job.height, job.source_format
                )
            })?;
        Ok(Self {
            inner,
            frame_len,
            frame: vec![0; frame_len],
            frame_offset: frame_len,
            frames_remaining: job.frames,
            frame_index: 0,
            context,
        })
    }

    fn fill_frame(&mut self) -> io::Result<bool> {
        if self.frames_remaining == 0 {
            return Ok(false);
        }
        let mut header = Vec::new();
        let bytes = self.inner.read_until(b'\n', &mut header)?;
        if bytes == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("{} is missing frame {}", self.context, self.frame_index + 1),
            ));
        }
        if !valid_y4m_frame_header(&header) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} has invalid frame marker at frame {}",
                    self.context,
                    self.frame_index + 1
                ),
            ));
        }
        self.inner.read_exact(&mut self.frame)?;
        self.frame_offset = 0;
        self.frames_remaining -= 1;
        self.frame_index += 1;
        Ok(true)
    }
}

impl<R: BufRead> Read for Y4mFrameReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.frame_offset >= self.frame_len && !self.fill_frame()? {
            return Ok(0);
        }
        let remaining = &self.frame[self.frame_offset..];
        let count = remaining.len().min(output.len());
        output[..count].copy_from_slice(&remaining[..count]);
        self.frame_offset += count;
        Ok(count)
    }
}

fn valid_y4m_frame_header(header: &[u8]) -> bool {
    header.ends_with(b"\n")
        && header.starts_with(b"FRAME")
        && header.get(5).is_some_and(|byte| byte.is_ascii_whitespace())
}

fn open_job_reader(job: &EncodeJob) -> Result<Box<dyn Read>, String> {
    let reader = open_unfiltered_job_reader(job)?;
    if job.transform_filters.is_empty() {
        return Ok(reader);
    }
    apply_transform_filters_to_reader(reader, job)
}

fn open_unfiltered_job_reader(job: &EncodeJob) -> Result<Box<dyn Read>, String> {
    match &job.input {
        EncodeInput::Path(path) => {
            let file = File::open(path)
                .map_err(|err| format!("failed to open input '{}': {err}", path.display()))?;
            let reader: Box<dyn Read> = if is_y4m_path(path) {
                Box::new(Y4mFrameReader::new(BufReader::new(file), job, path)?)
            } else {
                Box::new(BufReader::new(file).take(selected_input_byte_len(job)?))
            };
            if job.source_format == job.format {
                Ok(reader)
            } else {
                Ok(Box::new(FrameFormatConvertingReader::new(reader, job)?))
            }
        }
        EncodeInput::Pattern(source) => {
            Ok(Box::new(Cursor::new(generated_pattern_input(job, source)?)))
        }
    }
}

fn apply_transform_filters_to_reader(
    reader: Box<dyn Read>,
    job: &EncodeJob,
) -> Result<Box<dyn Read>, String> {
    let info = FrameInfo::new(job.width, job.height, job.format).map_err(|err| err.to_string())?;
    let mut source = RawFrameReaderSource::new(reader, info, job.frames);
    let mut sink = RawFrameVecSink::new(info);
    let mut filters = job
        .transform_filters
        .iter()
        .copied()
        .map(build_transform_filter)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| err.to_string())?;
    let mut filter_refs = filters
        .iter_mut()
        .map(|filter| filter.as_mut() as &mut dyn Filter)
        .collect::<Vec<_>>();
    let stats = run_frame_filter_pipeline(&mut source, filter_refs.as_mut_slice(), &mut sink)
        .map_err(|err| err.to_string())?;
    if stats.input_frames != job.frames {
        return Err(format!(
            "filter pipeline consumed {} frame(s), expected {}",
            stats.input_frames, job.frames
        ));
    }
    Ok(Box::new(Cursor::new(sink.into_bytes())))
}

fn build_transform_filter(spec: TransformFilterSpec) -> framefinery_core::Result<Box<dyn Filter>> {
    match spec {
        TransformFilterSpec::Identity => build_identity_filter(),
    }
}

#[cfg(feature = "filter-identity")]
fn build_identity_filter() -> framefinery_core::Result<Box<dyn Filter>> {
    Ok(Box::new(IdentityFilter))
}

#[cfg(not(feature = "filter-identity"))]
fn build_identity_filter() -> framefinery_core::Result<Box<dyn Filter>> {
    Err(MediaError::Message("unknown filter 'identity'".to_string()))
}

struct RawFrameReaderSource<R> {
    inner: R,
    info: FrameInfo,
    frames_remaining: usize,
    frame_index: usize,
}

impl<R: Read> RawFrameReaderSource<R> {
    fn new(inner: R, info: FrameInfo, frames: usize) -> Self {
        Self {
            inner,
            info,
            frames_remaining: frames,
            frame_index: 0,
        }
    }
}

impl<R: Read> Source for RawFrameReaderSource<R> {
    type Output = Frame;

    fn pull(&mut self) -> framefinery_core::Result<Option<Self::Output>> {
        if self.frames_remaining == 0 {
            return Ok(None);
        }
        let mut data = vec![0; self.info.expected_len()];
        self.inner.read_exact(&mut data).map_err(|err| {
            MediaError::Message(format!(
                "failed to read frame {} for filter pipeline: {err}",
                self.frame_index + 1
            ))
        })?;
        self.frames_remaining -= 1;
        self.frame_index += 1;
        Frame::new(self.info, data).map(Some)
    }
}

struct RawFrameVecSink {
    info: FrameInfo,
    data: Vec<u8>,
}

impl RawFrameVecSink {
    fn new(info: FrameInfo) -> Self {
        Self {
            info,
            data: Vec::new(),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.data
    }
}

impl Sink<Frame> for RawFrameVecSink {
    fn push(&mut self, input: Frame) -> framefinery_core::Result<()> {
        if input.info() != self.info {
            return Err(MediaError::Unsupported {
                feature: "filter pipeline frame format change".to_string(),
                reason: format!(
                    "expected {}x{}:{}, got {}x{}:{}",
                    self.info.width,
                    self.info.height,
                    self.info.format,
                    input.info().width,
                    input.info().height,
                    input.info().format
                ),
            });
        }
        self.data.extend(input.into_data());
        Ok(())
    }
}

fn selected_input_byte_len(job: &EncodeJob) -> Result<u64, String> {
    let frame_len = job
        .source_format
        .frame_len(job.width, job.height)
        .ok_or_else(|| {
            format!(
                "frame length overflow for {}x{}:{}",
                job.width, job.height, job.source_format
            )
        })?;
    let byte_len = frame_len
        .checked_mul(job.frames)
        .ok_or_else(|| "selected input byte length overflow".to_string())?;
    u64::try_from(byte_len).map_err(|_| "selected input byte length overflows u64".to_string())
}

struct FrameFormatConvertingReader<R> {
    inner: R,
    width: usize,
    height: usize,
    source_format: PixelFormat,
    target_format: PixelFormat,
    source_frame: Vec<u8>,
    converted_frame: Vec<u8>,
    converted_offset: usize,
    frames_remaining: usize,
}

impl<R: Read> FrameFormatConvertingReader<R> {
    fn new(inner: R, job: &EncodeJob) -> Result<Self, String> {
        let source_frame_len = job
            .source_format
            .frame_len(job.width, job.height)
            .ok_or_else(|| {
                format!(
                    "frame length overflow for {}x{}:{}",
                    job.width, job.height, job.source_format
                )
            })?;
        Ok(Self {
            inner,
            width: job.width,
            height: job.height,
            source_format: job.source_format,
            target_format: job.format,
            source_frame: vec![0; source_frame_len],
            converted_frame: Vec::new(),
            converted_offset: 0,
            frames_remaining: job.frames,
        })
    }

    fn fill_converted_frame(&mut self) -> std::io::Result<bool> {
        if self.frames_remaining == 0 {
            return Ok(false);
        }
        self.inner.read_exact(&mut self.source_frame)?;
        self.converted_frame = convert_frame_format(
            &self.source_frame,
            self.width,
            self.height,
            self.source_format,
            self.target_format,
        )
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))?;
        self.converted_offset = 0;
        self.frames_remaining -= 1;
        Ok(true)
    }
}

impl<R: Read> Read for FrameFormatConvertingReader<R> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.converted_offset >= self.converted_frame.len() && !self.fill_converted_frame()? {
            return Ok(0);
        }

        let remaining = &self.converted_frame[self.converted_offset..];
        let count = remaining.len().min(output.len());
        output[..count].copy_from_slice(&remaining[..count]);
        self.converted_offset += count;
        Ok(count)
    }
}

struct FrameFormatConvertingWriter<'a, W: Write + ?Sized> {
    inner: &'a mut W,
    width: usize,
    height: usize,
    source_format: PixelFormat,
    target_format: PixelFormat,
    source_frame: Vec<u8>,
    source_offset: usize,
}

impl<'a, W: Write + ?Sized> FrameFormatConvertingWriter<'a, W> {
    fn new(inner: &'a mut W, job: &EncodeJob) -> Result<Self, String> {
        let source_frame_len = job.format.frame_len(job.width, job.height).ok_or_else(|| {
            format!(
                "frame length overflow for {}x{}:{}",
                job.width, job.height, job.format
            )
        })?;
        Ok(Self {
            inner,
            width: job.width,
            height: job.height,
            source_format: job.format,
            target_format: job.source_format,
            source_frame: vec![0; source_frame_len],
            source_offset: 0,
        })
    }

    fn finish(&mut self) -> std::io::Result<()> {
        if self.source_offset != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "partial reconstruction frame left in format converter",
            ));
        }
        self.inner.flush()
    }

    fn convert_and_write_frame(&mut self) -> std::io::Result<()> {
        let converted = convert_frame_format(
            &self.source_frame,
            self.width,
            self.height,
            self.source_format,
            self.target_format,
        )
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string()))?;
        self.inner.write_all(&converted)?;
        self.source_offset = 0;
        Ok(())
    }
}

impl<W: Write + ?Sized> Write for FrameFormatConvertingWriter<'_, W> {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        let mut consumed = 0usize;
        while consumed < input.len() {
            let available = self.source_frame.len() - self.source_offset;
            let count = available.min(input.len() - consumed);
            self.source_frame[self.source_offset..self.source_offset + count]
                .copy_from_slice(&input[consumed..consumed + count]);
            self.source_offset += count;
            consumed += count;
            if self.source_offset == self.source_frame.len() {
                self.convert_and_write_frame()?;
            }
        }
        Ok(input.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.finish()
    }
}

fn input_label(input: &EncodeInput) -> String {
    match input {
        EncodeInput::Path(path) => format!("path={}", path.display()),
        EncodeInput::Pattern(source) => format!("source=pattern:{}", source.pattern),
    }
}

#[derive(Debug, Clone, Copy)]
struct FramePsnr {
    y: f64,
    u: f64,
    v: f64,
    all: f64,
}

fn print_frame_metrics(
    codec: &str,
    job: &EncodeJob,
    frame_idx: usize,
    frame_count: usize,
    bitstream_bytes: usize,
    source: &[u8],
    reconstruction: &[u8],
) {
    let bits = bitstream_bytes * 8;
    match frame_psnr(job, source, reconstruction) {
        Some(psnr) => {
            if job.format.is_rgb() {
                eprintln!(
                    "frame: codec={} index={}/{} bits={} bytes={} psnr_r={} psnr_g={} psnr_b={} psnr_all={}",
                    codec,
                    frame_idx + 1,
                    frame_count,
                    bits,
                    bitstream_bytes,
                    format_psnr(psnr.y),
                    format_psnr(psnr.u),
                    format_psnr(psnr.v),
                    format_psnr(psnr.all),
                );
            } else {
                eprintln!(
                    "frame: codec={} index={}/{} bits={} bytes={} psnr_y={} psnr_u={} psnr_v={} psnr_all={}",
                    codec,
                    frame_idx + 1,
                    frame_count,
                    bits,
                    bitstream_bytes,
                    format_psnr(psnr.y),
                    format_psnr(psnr.u),
                    format_psnr(psnr.v),
                    format_psnr(psnr.all),
                );
            }
        }
        None => eprintln!(
            "frame: codec={} index={}/{} bits={} bytes={} psnr=n/a",
            codec,
            frame_idx + 1,
            frame_count,
            bits,
            bitstream_bytes,
        ),
    }
}

fn frame_psnr(job: &EncodeJob, source: &[u8], reconstruction: &[u8]) -> Option<FramePsnr> {
    let y_samples = job.width.checked_mul(job.height)?;
    if job.format == PixelFormat::Rgb24 {
        return rgb24_frame_psnr(y_samples, source, reconstruction);
    }
    if job.format == PixelFormat::Gbrp8 {
        return gbrp8_frame_psnr(y_samples, source, reconstruction);
    }
    let chroma_sampling = job.format.chroma_sampling()?;
    let chroma_width = job.width.checked_div(chroma_sampling.subsample_x())?;
    let chroma_height = job.height.checked_div(chroma_sampling.subsample_y())?;
    let chroma_samples = chroma_width.checked_mul(chroma_height)?;
    let bytes_per_sample = job.format.bit_depth().bytes_per_sample();
    let y_len = y_samples.checked_mul(bytes_per_sample)?;
    let chroma_len = chroma_samples.checked_mul(bytes_per_sample)?;
    let frame_len = y_len.checked_add(chroma_len.checked_mul(2)?)?;
    if source.len() != frame_len || reconstruction.len() != frame_len {
        return None;
    }
    if source == reconstruction {
        return Some(FramePsnr {
            y: f64::INFINITY,
            u: f64::INFINITY,
            v: f64::INFINITY,
            all: f64::INFINITY,
        });
    }

    let y_src = &source[..y_len];
    let y_rec = &reconstruction[..y_len];
    let u_start = y_len;
    let v_start = y_len + chroma_len;
    let u_src = &source[u_start..v_start];
    let u_rec = &reconstruction[u_start..v_start];
    let v_src = &source[v_start..frame_len];
    let v_rec = &reconstruction[v_start..frame_len];

    let bit_depth = job.format.bit_depth();
    let y_sse = planar_sample_sse(y_src, y_rec, bit_depth)?;
    let u_sse = planar_sample_sse(u_src, u_rec, bit_depth)?;
    let v_sse = planar_sample_sse(v_src, v_rec, bit_depth)?;
    let max_sample = f64::from(bit_depth.max_sample());
    Some(FramePsnr {
        y: psnr_from_sse(y_sse, y_samples, max_sample),
        u: psnr_from_sse(u_sse, chroma_samples, max_sample),
        v: psnr_from_sse(v_sse, chroma_samples, max_sample),
        all: psnr_from_sse(
            y_sse + u_sse + v_sse,
            y_samples + chroma_samples * 2,
            max_sample,
        ),
    })
}

fn gbrp8_frame_psnr(pixels: usize, source: &[u8], reconstruction: &[u8]) -> Option<FramePsnr> {
    let plane_len = pixels;
    let frame_len = plane_len.checked_mul(3)?;
    if source.len() != frame_len || reconstruction.len() != frame_len {
        return None;
    }
    if source == reconstruction {
        return Some(FramePsnr {
            y: f64::INFINITY,
            u: f64::INFINITY,
            v: f64::INFINITY,
            all: f64::INFINITY,
        });
    }

    let (source_g, source_chroma) = source.split_at(plane_len);
    let (source_b, source_r) = source_chroma.split_at(plane_len);
    let (recon_g, recon_chroma) = reconstruction.split_at(plane_len);
    let (recon_b, recon_r) = recon_chroma.split_at(plane_len);
    let r_sse = planar_sample_sse(source_r, recon_r, SampleBitDepth::new(8).unwrap())?;
    let g_sse = planar_sample_sse(source_g, recon_g, SampleBitDepth::new(8).unwrap())?;
    let b_sse = planar_sample_sse(source_b, recon_b, SampleBitDepth::new(8).unwrap())?;
    Some(FramePsnr {
        y: psnr_from_sse(r_sse, pixels, 255.0),
        u: psnr_from_sse(g_sse, pixels, 255.0),
        v: psnr_from_sse(b_sse, pixels, 255.0),
        all: psnr_from_sse(r_sse + g_sse + b_sse, frame_len, 255.0),
    })
}

fn rgb24_frame_psnr(pixels: usize, source: &[u8], reconstruction: &[u8]) -> Option<FramePsnr> {
    let frame_len = pixels.checked_mul(3)?;
    if source.len() != frame_len || reconstruction.len() != frame_len {
        return None;
    }
    if source == reconstruction {
        return Some(FramePsnr {
            y: f64::INFINITY,
            u: f64::INFINITY,
            v: f64::INFINITY,
            all: f64::INFINITY,
        });
    }

    let mut r_sse = 0u64;
    let mut g_sse = 0u64;
    let mut b_sse = 0u64;
    for (src, rec) in source.chunks_exact(3).zip(reconstruction.chunks_exact(3)) {
        let r_diff = src[0] as i32 - rec[0] as i32;
        let g_diff = src[1] as i32 - rec[1] as i32;
        let b_diff = src[2] as i32 - rec[2] as i32;
        r_sse += (r_diff * r_diff) as u64;
        g_sse += (g_diff * g_diff) as u64;
        b_sse += (b_diff * b_diff) as u64;
    }

    Some(FramePsnr {
        y: psnr_from_sse(r_sse, pixels, 255.0),
        u: psnr_from_sse(g_sse, pixels, 255.0),
        v: psnr_from_sse(b_sse, pixels, 255.0),
        all: psnr_from_sse(r_sse + g_sse + b_sse, frame_len, 255.0),
    })
}

fn psnr_from_sse(sse: u64, samples: usize, max_sample: f64) -> f64 {
    if sse == 0 {
        f64::INFINITY
    } else {
        10.0 * ((max_sample * max_sample * samples as f64) / sse as f64).log10()
    }
}

fn format_psnr(value: f64) -> String {
    if value.is_infinite() {
        "inf".to_string()
    } else {
        format!("{value:.3}")
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "video-encoders"), allow(dead_code))]
struct EncodeJob {
    input: EncodeInput,
    output: PathBuf,
    recon: Option<PathBuf>,
    psnr: bool,
    transform_filters: Vec<TransformFilterSpec>,
    frames: usize,
    fps: Option<String>,
    validate_y4m_metadata: bool,
    width: usize,
    height: usize,
    source_format: PixelFormat,
    format: PixelFormat,
    lossless: bool,
}

fn print_encode_config(codec_name: &str, args: &EncodeArgs, job: &EncodeJob) {
    let settings = if args.settings.is_empty() {
        "none".to_string()
    } else {
        args.settings.join(",")
    };
    eprintln!(
        "input: {} video={}x{}:{} frames={} fps={}",
        input_label(&job.input),
        job.width,
        job.height,
        job.source_format,
        job.frames,
        job.fps.as_deref().unwrap_or("unspecified")
    );
    if job.source_format != job.format {
        eprintln!("input-convert: {} -> {}", job.source_format, job.format);
    }
    for filter in &args.filters {
        eprintln!("filter: {filter}");
    }
    eprintln!(
        "encoder: codec={} output={} recon={} settings={} preset={}",
        codec_name,
        job.output.display(),
        job.recon
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        settings,
        args.preset.as_deref().unwrap_or("default")
    );
    if job.psnr {
        eprintln!("metrics: psnr=enabled");
    }
}

fn encode_job_for_codec(codec: CodecManifest, args: &EncodeArgs) -> Result<EncodeJob, String> {
    let filter_pipeline = parse_filter_pipeline(args)?;
    let input = match args.input.as_deref() {
        Some(path) => EncodeInput::Path(PathBuf::from(path)),
        None => EncodeInput::Pattern(
            filter_pipeline
                .source
                .clone()
                .ok_or_else(|| "encode requires an input path or source filter".to_string())?,
        ),
    };
    let output = PathBuf::from(args.output.as_deref().expect("parser requires output"));
    let recon = args.recon.as_deref().map(PathBuf::from);
    let y4m_metadata = match &input {
        EncodeInput::Path(path) => read_y4m_file_metadata(path)?,
        EncodeInput::Pattern(_) => None,
    };
    let (width, height, source_format) = resolve_video_metadata(args, y4m_metadata.as_ref())?;
    let frames = resolve_frame_count(args, &input, source_format, width, height)?;
    let lossless = boolean_setting_enabled(&args.settings, "lossless")?;
    let format = if lossless && source_format != PixelFormat::Rgb24 {
        source_format
    } else {
        codec_input_format(codec, source_format)
    };
    if lossless && !(codec.supports_lossless_format)(format) {
        return Err(format!(
            "lossless encode is not implemented for {} {format}",
            codec.name
        ));
    }
    Ok(EncodeJob {
        input,
        output,
        recon,
        psnr: args.psnr,
        transform_filters: filter_pipeline.transforms,
        frames,
        fps: resolve_fps_metadata(args, y4m_metadata.as_ref()),
        validate_y4m_metadata: y4m_metadata.is_some() && !args.explicit_video,
        width,
        height,
        source_format,
        format,
        lossless,
    })
}

fn resolve_video_metadata(
    args: &EncodeArgs,
    y4m_metadata: Option<&Y4mMetadata>,
) -> Result<(usize, usize, PixelFormat), String> {
    match (args.video.as_ref(), y4m_metadata) {
        (Some(video), Some(metadata)) if args.explicit_video => {
            resolve_video_spec(video, Some(metadata.format))
        }
        (Some(_), Some(metadata)) | (None, Some(metadata)) => {
            Ok((metadata.width, metadata.height, metadata.format))
        }
        (Some(video), None) => resolve_video_spec(video, None),
        (None, None) => Err(
            "encode requires --video WxH:pixfmt, filename metadata, or a Y4M header".to_string(),
        ),
    }
}

fn resolve_video_spec(
    video: &args::VideoSpec,
    fallback_format: Option<PixelFormat>,
) -> Result<(usize, usize, PixelFormat), String> {
    let source_format = match video.pixel_format.as_deref() {
        Some(format) => format.parse::<PixelFormat>()?,
        None => fallback_format.ok_or_else(|| {
            "encode requires a pixel format in --video, input filename, or Y4M header".to_string()
        })?,
    };
    Ok((video.width as usize, video.height as usize, source_format))
}

fn resolve_fps_metadata(args: &EncodeArgs, y4m_metadata: Option<&Y4mMetadata>) -> Option<String> {
    if args.explicit_fps {
        return args.fps.clone();
    }
    y4m_metadata
        .and_then(|metadata| metadata.fps.clone())
        .or_else(|| args.fps.clone())
}

fn codec_input_format(codec: CodecManifest, source_format: PixelFormat) -> PixelFormat {
    if (codec.accepts_format)(source_format) {
        return source_format;
    }
    if source_format == PixelFormat::Rgb24 && (codec.accepts_format)(PixelFormat::Gbrp8) {
        return PixelFormat::Gbrp8;
    }
    let Some(target_depth) = SampleBitDepth::new(8) else {
        return source_format;
    };
    let Some(target_format) = source_format.with_bit_depth(target_depth) else {
        return source_format;
    };
    if source_format.bit_depth().bits() != 8 && (codec.accepts_format)(target_format) {
        target_format
    } else {
        source_format
    }
}

fn resolve_frame_count(
    args: &EncodeArgs,
    input: &EncodeInput,
    format: PixelFormat,
    width: usize,
    height: usize,
) -> Result<usize, String> {
    let frame_len = format
        .frame_len(width, height)
        .ok_or_else(|| format!("frame length overflow for {width}x{height}:{format}"))?;
    if let Some(frames) = args.frames {
        return match input {
            EncodeInput::Path(path) => {
                if is_y4m_path(path) {
                    infer_y4m_complete_frame_count(path, frame_len, Some(frames as usize))
                } else {
                    let available = infer_file_complete_frame_count(path, frame_len)?;
                    Ok((frames as usize).min(available))
                }
            }
            EncodeInput::Pattern(_) => Ok(frames as usize),
        };
    }

    match input {
        EncodeInput::Path(path) => {
            if is_y4m_path(path) {
                infer_y4m_complete_frame_count(path, frame_len, None)
            } else {
                infer_file_frame_count_from_eof(path, frame_len)
            }
        }
        EncodeInput::Pattern(_) => {
            Err("source filters require --frames because there is no input EOF".to_string())
        }
    }
}

fn infer_y4m_complete_frame_count(
    path: &Path,
    frame_len: usize,
    limit: Option<usize>,
) -> Result<usize, String> {
    let file = File::open(path)
        .map_err(|err| format!("failed to open input '{}': {err}", path.display()))?;
    let mut reader = BufReader::new(file);
    let context = y4m_context(path);
    read_y4m_stream_header(&mut reader, &context)?;
    let mut frames = 0usize;
    while limit.map_or(true, |limit| frames < limit) {
        let mut frame_header = Vec::new();
        let bytes = reader
            .read_until(b'\n', &mut frame_header)
            .map_err(|err| format!("failed to read {context} frame marker: {err}"))?;
        if bytes == 0 {
            break;
        }
        if !valid_y4m_frame_header(&frame_header) {
            return Err(format!(
                "{context} has invalid frame marker at frame {}",
                frames + 1
            ));
        }
        skip_exact_y4m_payload(&mut reader, frame_len, &context, frames + 1)?;
        frames += 1;
    }
    if frames == 0 {
        return Err(format!("{context} contains no complete frames"));
    }
    Ok(frames)
}

fn skip_exact_y4m_payload<R: Read>(
    reader: &mut R,
    frame_len: usize,
    context: &str,
    frame_number: usize,
) -> Result<(), String> {
    let mut remaining = frame_len;
    let mut buffer = [0u8; 64 * 1024];
    while remaining > 0 {
        let chunk = remaining.min(buffer.len());
        reader
            .read_exact(&mut buffer[..chunk])
            .map_err(|err| match err.kind() {
                io::ErrorKind::UnexpectedEof => {
                    format!("{context} is too short while reading frame {frame_number}")
                }
                _ => format!("failed to read {context} frame {frame_number}: {err}"),
            })?;
        remaining -= chunk;
    }
    Ok(())
}

fn infer_file_complete_frame_count(path: &Path, frame_len: usize) -> Result<usize, String> {
    let metadata = fs::metadata(path)
        .map_err(|err| format!("failed to stat input '{}': {err}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "cannot infer frame count for non-file input '{}'; pass --frames",
            path.display()
        ));
    }
    let byte_len = metadata.len();
    if byte_len == 0 {
        return Err(format!(
            "input '{}' is empty; no complete frames are available",
            path.display()
        ));
    }
    let frame_len = frame_len as u64;
    let frames = byte_len / frame_len;
    if frames == 0 {
        return Err(format!(
            "input '{}' has {} byte(s), less than one {} byte frame",
            path.display(),
            byte_len,
            frame_len
        ));
    }
    usize::try_from(frames).map_err(|_| {
        format!(
            "input '{}' contains too many frames for this platform",
            path.display()
        )
    })
}

fn infer_file_frame_count_from_eof(path: &Path, frame_len: usize) -> Result<usize, String> {
    let complete_frames = infer_file_complete_frame_count(path, frame_len)?;
    let byte_len = fs::metadata(path)
        .map_err(|err| format!("failed to stat input '{}': {err}", path.display()))?
        .len();
    let frame_len = frame_len as u64;
    if byte_len % frame_len != 0 {
        return Err(format!(
            "input '{}' has {} byte(s), which is not a whole number of {} byte frame(s); pass --frames to encode the complete-frame prefix",
            path.display(),
            byte_len,
            frame_len
        ));
    }
    Ok(complete_frames)
}

fn encode_with_model(
    codec: CodecManifest,
    args: &EncodeArgs,
    job: EncodeJob,
) -> Result<(), String> {
    let mut input = open_job_reader(&job)?;
    let mut output = create_writer(&job.output)?;
    let mut recon = create_optional_writer(job.recon.as_deref())?;
    let request = CodecEncodeRequest {
        frames: job.frames,
        width: job.width,
        height: job.height,
        format: job.format,
        lossless: job.lossless,
        settings: &args.settings,
    };
    let mut frame_metrics = |metrics: CodecEncodeFrameMetrics<'_>| {
        print_frame_metrics(
            codec.name,
            &job,
            metrics.frame_idx,
            metrics.frame_count,
            metrics.bitstream_bytes,
            metrics.source,
            metrics.reconstruction,
        );
    };
    let frame_metrics = if job.psnr {
        Some(&mut frame_metrics as CodecEncodeFrameMetricsCallback<'_>)
    } else {
        None
    };
    if job.source_format != job.format && recon.is_some() {
        let mut recon_converter =
            FrameFormatConvertingWriter::new(recon.as_mut().expect("checked Some"), &job)?;
        (codec.encode)(
            &mut input,
            &mut output,
            Some(&mut recon_converter as &mut dyn Write),
            request,
            frame_metrics,
        )?;
        recon_converter
            .finish()
            .map_err(|err| format!("failed to finish reconstruction conversion: {err}"))?;
    } else {
        (codec.encode)(
            &mut input,
            &mut output,
            recon.as_mut().map(|writer| writer as &mut dyn Write),
            request,
            frame_metrics,
        )?;
    }
    if let (Some(path), Some(writer)) = (job.recon.as_deref(), recon.as_mut()) {
        flush_writer(path, writer)?;
    }
    flush_writer(&job.output, &mut output)
}

#[cfg_attr(not(feature = "video-encoders"), allow(dead_code))]
fn create_writer(path: &Path) -> Result<BufWriter<File>, String> {
    let file = File::create(path)
        .map_err(|err| format!("failed to create output '{}': {err}", path.display()))?;
    Ok(BufWriter::new(file))
}

fn create_optional_writer(path: Option<&Path>) -> Result<Option<BufWriter<File>>, String> {
    path.map(create_writer).transpose()
}

#[cfg_attr(not(feature = "video-encoders"), allow(dead_code))]
fn flush_writer(path: &Path, writer: &mut BufWriter<File>) -> Result<(), String> {
    writer
        .flush()
        .map_err(|err| format!("failed to flush output '{}': {err}", path.display()))
}

fn print_help(topic: Option<HelpTopic>) -> ExitCode {
    match topic {
        None => print!("{}", args::help(VERSION)),
        Some(HelpTopic::Codecs) => print_codec_table("Codecs", CODECS),
        Some(HelpTopic::Filters(None)) => print_filter_table("Filters", FILTERS),
        Some(HelpTopic::Filters(Some(filter))) => return print_filter_detail(&filter),
        Some(HelpTopic::Pixfmt) => print_pixel_format_help(),
        Some(HelpTopic::Settings(None)) => print_settings_help(),
        Some(HelpTopic::Settings(Some(setting))) => return print_setting_detail(&setting),
        Some(HelpTopic::Presets) => print_presets_help(),
    }
    ExitCode::SUCCESS
}

fn print_codec_table(title: &str, codecs: &[CodecManifest]) {
    println!("{title}:");
    if codecs.is_empty() {
        println!("  No video encoders are compiled into this binary.");
        return;
    }
    println!("{:<12} {:<44} Summary", "Name", "Settings");
    for codec in codecs {
        println!(
            "{:<12} {:<44} {}",
            codec.name,
            settings_label(GLOBAL_SETTINGS, codec.settings),
            codec.summary
        );
    }

    if codecs.iter().any(|codec| !codec.settings.is_empty()) {
        println!();
        println!("Codec-specific settings:");
        let mut printed = Vec::new();
        for codec in codecs {
            for setting in codec.settings {
                if printed.contains(&setting.name) {
                    continue;
                }
                printed.push(setting.name);
                println!(
                    "  {} ({}) - {}",
                    setting.name,
                    setting_values_label(*setting),
                    setting.summary
                );
            }
        }
    }

    if !GLOBAL_SETTINGS.is_empty() {
        println!();
        println!("Global settings:");
        for setting in GLOBAL_SETTINGS {
            println!(
                "  {} ({}) - {}",
                setting.name,
                setting_values_label(*setting),
                setting.summary
            );
        }
    }
}

fn print_filter_table(title: &str, filters: &[FilterManifest]) {
    println!("{title}:");
    if filters.is_empty() {
        println!("  No filters are compiled into this binary.");
        return;
    }
    println!(
        "{:<12} {:<10} {:<40} {:<12} Summary",
        "Name", "Kind", "Spec", "Execution"
    );
    for filter in filters {
        let spec = filter.spec.forms.first().map_or("-", |form| form.syntax);
        println!(
            "{:<12} {:<10} {:<40} {:<12} {}",
            filter.name,
            filter_stage_name(filter.stage),
            spec,
            filter.implementation_status(),
            filter.summary
        );
    }

    println!();
    println!("Run `ff --help filters <name>` for the full spec contract.");
}

fn print_filter_detail(name: &str) -> ExitCode {
    let Some(filter) = catalog::filter(name) else {
        eprintln!("error: unknown filter '{name}'");
        eprintln!("run 'ff --help filters' to list known filter stages");
        return ExitCode::from(2);
    };
    println!("Filter: {}", filter.name);
    println!("Kind: {}", filter_stage_name(filter.stage));
    println!("Execution: {}", filter.implementation_status());
    println!("Summary: {}", filter.summary);

    if !filter.spec.forms.is_empty() {
        println!();
        println!("Spec forms:");
        println!("{:<40} Summary", "Syntax");
        for form in filter.spec.forms {
            println!("{:<40} {}", form.syntax, form.summary);
        }
    }

    if !filter.spec.parameters.is_empty() {
        println!();
        println!("Parameters:");
        println!("{:<12} {:<36} {:<10} Summary", "Name", "Values", "Required");
        for parameter in filter.spec.parameters {
            println!(
                "{:<12} {:<36} {:<10} {}",
                parameter.name,
                filter_spec_value_label(parameter.value),
                if parameter.required { "yes" } else { "no" },
                parameter.summary
            );
        }
    }

    if !filter.spec.examples.is_empty() {
        println!();
        println!("Examples:");
        for example in filter.spec.examples {
            println!("  {:<40} {}", example.spec, example.summary);
        }
    }

    if !filter.spec.notes.is_empty() {
        println!();
        println!("Notes:");
        for note in filter.spec.notes {
            println!("  {note}");
        }
    }

    ExitCode::SUCCESS
}

fn filter_spec_value_label(value: catalog::FilterSpecValue) -> String {
    match value {
        catalog::FilterSpecValue::Choice(values) => values.join("|"),
        catalog::FilterSpecValue::PositiveInteger => "positive integer".to_string(),
        catalog::FilterSpecValue::UnsignedInteger => "unsigned integer".to_string(),
    }
}

fn filter_stage_name(kind: catalog::FilterStageKind) -> &'static str {
    match kind {
        catalog::FilterStageKind::Source => "source",
        catalog::FilterStageKind::Transform => "transform",
    }
}

fn print_pixel_format_help() {
    println!("Pixel formats:");
    println!("{:<24} Summary", "Name");
    println!("{:<24} {}", "yuv420p8", "8-bit planar YUV 4:2:0");
    println!(
        "{:<24} {}",
        "yuv420p9le..16le", "little-endian planar YUV 4:2:0, 9 through 16 bits"
    );
    println!("{:<24} {}", "yuv422p8", "8-bit planar YUV 4:2:2");
    println!(
        "{:<24} {}",
        "yuv422p9le..16le", "little-endian planar YUV 4:2:2, 9 through 16 bits"
    );
    println!("{:<24} {}", "yuv444p8", "8-bit planar YUV 4:4:4");
    println!(
        "{:<24} {}",
        "yuv444p9le..16le", "little-endian planar YUV 4:4:4, 9 through 16 bits"
    );
    println!("{:<24} {}", "gray8", "8-bit monochrome");
    println!(
        "{:<24} {}",
        "gray9le..16le", "little-endian monochrome, 9 through 16 bits"
    );
    println!("{:<24} {}", "gbrp8", "8-bit planar GBR identity RGB");
    println!("{:<24} {}", "rgb24", "8-bit packed RGB");
    println!();
    println!("Aliases:");
    println!("  yuv420p, i420 -> yuv420p8");
    println!("  yuv422p, i422 -> yuv422p8");
    println!("  yuv444p, i444 -> yuv444p8");
    println!("  y8 -> gray8");
    println!("  gbrp -> gbrp8");
    println!("  i010, i210, i410 and related i<sampling><depth> aliases are accepted");
}

fn print_settings_help() {
    println!("Settings:");
    let rows = setting_rows();
    print_setting_rows(&rows);
    println!();
    println!("Use settings as repeated `--set key[=value]` output options after `--encode`.");
    println!("Run `ff --help settings <name>` for the full setting spec contract.");
}

#[derive(Debug, Clone)]
struct SettingHelpRow {
    setting: SettingManifest,
    applies_to: String,
}

fn setting_rows() -> Vec<SettingHelpRow> {
    let mut rows = Vec::new();
    for setting in GLOBAL_SETTINGS {
        rows.push(SettingHelpRow {
            setting: *setting,
            applies_to: "global".to_string(),
        });
    }
    for codec in CODECS {
        for setting in codec.settings {
            if let Some(row) = rows.iter_mut().find(|row| row.setting.name == setting.name) {
                if row.applies_to == "global" {
                    continue;
                }
                if !row.applies_to.split(", ").any(|name| name == codec.name) {
                    if !row.applies_to.is_empty() {
                        row.applies_to.push_str(", ");
                    }
                    row.applies_to.push_str(codec.name);
                }
            } else {
                rows.push(SettingHelpRow {
                    setting: *setting,
                    applies_to: codec.name.to_string(),
                });
            }
        }
    }
    rows
}

fn setting_help(name: &str) -> Option<SettingHelpRow> {
    setting_rows()
        .into_iter()
        .find(|row| row.setting.name == name)
}

fn print_setting_rows(settings: &[SettingHelpRow]) {
    println!(
        "{:<16} {:<28} {:<30} {:<62} Summary",
        "Name", "Applies to", "Spec", "Values"
    );
    for row in settings {
        let setting = row.setting;
        let spec = setting.spec.forms.first().map_or("-", |form| form.syntax);
        println!(
            "{:<16} {:<28} {:<30} {:<62} {}",
            setting.name,
            row.applies_to,
            spec,
            setting_values_label(setting),
            setting.summary
        );
    }
}

fn print_setting_detail(name: &str) -> ExitCode {
    let Some(row) = setting_help(name) else {
        eprintln!("error: unknown setting '{name}'");
        eprintln!("run 'ff --help settings' to list known settings");
        return ExitCode::from(2);
    };
    let setting = row.setting;
    println!("Setting: {}", setting.name);
    println!("Applies to: {}", row.applies_to);
    println!("Values: {}", setting_values_label(setting));
    println!("Summary: {}", setting.summary);

    if !setting.spec.forms.is_empty() {
        println!();
        println!("Spec forms:");
        println!("{:<32} Summary", "Syntax");
        for form in setting.spec.forms {
            println!("{:<32} {}", form.syntax, form.summary);
        }
    }

    if !setting.spec.examples.is_empty() {
        println!();
        println!("Examples:");
        for example in setting.spec.examples {
            println!("  {:<32} {}", example.spec, example.summary);
        }
    }

    if !setting.spec.notes.is_empty() {
        println!();
        println!("Notes:");
        for note in setting.spec.notes {
            println!("  {note}");
        }
    }

    ExitCode::SUCCESS
}

fn print_presets_help() {
    println!("Presets:");
    println!("  No named encoder presets are currently defined.");
    println!();
    println!("The `--preset <name>` option is reserved for future encoder preset catalogs.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_CODEC_SETTINGS: &[SettingManifest] = &[];

    const TEST_CODEC: CodecManifest = CodecManifest {
        name: "test",
        feature: "test-codec",
        summary: "test codec manifest",
        settings: TEST_CODEC_SETTINGS,
        accepts_format: test_codec_accepts_format,
        supports_lossless_format: test_codec_accepts_format,
        encode: test_codec_encode,
    };

    const TEST_GBRP_CODEC: CodecManifest = CodecManifest {
        name: "test-gbrp",
        feature: "test-codec-gbrp",
        summary: "test codec manifest with gbrp8 support",
        settings: TEST_CODEC_SETTINGS,
        accepts_format: test_gbrp_codec_accepts_format,
        supports_lossless_format: test_gbrp_codec_accepts_format,
        encode: test_codec_encode,
    };

    fn test_codec_accepts_format(format: PixelFormat) -> bool {
        matches!(format, PixelFormat::Rgb24 | PixelFormat::Gbrp8)
            || (format.is_yuv() && matches!(format.bit_depth().bits(), 8 | 10))
    }

    fn test_gbrp_codec_accepts_format(format: PixelFormat) -> bool {
        format == PixelFormat::Gbrp8
            || (format.is_yuv() && matches!(format.bit_depth().bits(), 8..=12))
    }

    fn test_codec_encode(
        _input: &mut dyn Read,
        _output: &mut dyn Write,
        _recon: Option<&mut dyn Write>,
        _request: CodecEncodeRequest<'_>,
        _frame_metrics: Option<CodecEncodeFrameMetricsCallback<'_>>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn encode_job(args: &EncodeArgs) -> Result<EncodeJob, String> {
        let codec = match args.codec.as_deref() {
            Some("test-gbrp") | Some("vvc") => TEST_GBRP_CODEC,
            _ => TEST_CODEC,
        };
        encode_job_for_codec(codec, args)
    }

    fn temp_yuv_path(name: &str) -> PathBuf {
        temp_input_path(name, "yuv")
    }

    fn temp_y4m_path(name: &str) -> PathBuf {
        temp_input_path(name, "y4m")
    }

    fn temp_input_path(name: &str, extension: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX epoch")
            .as_nanos();
        let dir = std::env::current_dir()
            .expect("current working directory")
            .join("target/framefinery-test-output");
        fs::create_dir_all(&dir).expect("create test output directory");
        dir.join(format!("framefinery_media_{name}_{unique}.{extension}"))
    }

    fn write_y4m(path: &Path, header: &str, frames: &[Vec<u8>]) {
        let mut file = File::create(path).expect("create temp y4m");
        file.write_all(header.as_bytes()).expect("write y4m header");
        for frame in frames {
            file.write_all(b"FRAME\n").expect("write y4m frame marker");
            file.write_all(frame).expect("write y4m frame");
        }
    }

    #[test]
    fn encode_job_infers_file_frames_from_eof_when_frames_omitted() {
        let path = temp_yuv_path("two_frames_8x8");
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&vec![0; 8 * 8 * 3 / 2 * 2])
            .expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("yuv420p8".to_string()),
            }),
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("infer frame count");
        assert_eq!(job.frames, 2);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_rejects_partial_frame_when_frames_omitted() {
        let path = temp_yuv_path("partial_frame_8x8");
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&vec![0; 8 * 8 * 3 / 2 + 1])
            .expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("yuv420p8".to_string()),
            }),
            frames: None,
            ..EncodeArgs::default()
        };

        let err = encode_job(&args).expect_err("partial frame should fail");
        assert!(err.contains("not a whole number"), "{err}");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_clamps_requested_frames_to_available_file_frames() {
        let path = temp_yuv_path("two_frames_requested_many_8x8");
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&vec![0; 8 * 8 * 3 / 2 * 2])
            .expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("yuv420p8".to_string()),
            }),
            frames: Some(99),
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("clamp frame count");
        assert_eq!(job.frames, 2);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_infers_y4m_metadata_and_frames_from_header() {
        let path = temp_y4m_path("two_frames_4x4");
        let frame_len = PixelFormat::Yuv420p8.frame_len(4, 4).unwrap();
        let first = vec![0x11; frame_len];
        let second = vec![0x22; frame_len];
        write_y4m(
            &path,
            "YUV4MPEG2 W4 H4 F15:1 Ip A0:0 C420jpeg XYSCSS=420JPEG\n",
            &[first.clone(), second.clone()],
        );

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("infer Y4M metadata");
        assert_eq!(job.width, 4);
        assert_eq!(job.height, 4);
        assert_eq!(job.source_format, PixelFormat::Yuv420p8);
        assert_eq!(job.frames, 2);
        assert_eq!(job.fps.as_deref(), Some("15"));

        let mut reader = open_job_reader(&job).expect("open Y4M reader");
        let mut raw = Vec::new();
        reader.read_to_end(&mut raw).expect("read raw frames");
        let mut expected = first;
        expected.extend(second);
        assert_eq!(raw, expected);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_uses_y4m_metadata_before_filename_metadata() {
        let path = temp_y4m_path("clip_8x8_30_yuv444p8");
        let frame_len = PixelFormat::Yuv420p8.frame_len(4, 4).unwrap();
        write_y4m(
            &path,
            "YUV4MPEG2 W4 H4 F24:1 Ip A0:0 C420\n",
            &[vec![0; frame_len]],
        );

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("yuv444p8".to_string()),
            }),
            fps: Some("30".to_string()),
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("Y4M header should win over inferred filename metadata");
        assert_eq!(job.width, 4);
        assert_eq!(job.height, 4);
        assert_eq!(job.source_format, PixelFormat::Yuv420p8);
        assert_eq!(job.fps.as_deref(), Some("24"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_allows_explicit_video_to_override_y4m_header() {
        let path = temp_y4m_path("override_header");
        let explicit_frame_len = PixelFormat::Yuv420p8.frame_len(4, 4).unwrap();
        let frame = vec![0x33; explicit_frame_len];
        write_y4m(
            &path,
            "YUV4MPEG2 W8 H8 F30:1 Ip A0:0 C420\n",
            std::slice::from_ref(&frame),
        );

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 4,
                height: 4,
                pixel_format: Some("yuv420p8".to_string()),
            }),
            explicit_video: true,
            frames: Some(1),
            fps: Some("60".to_string()),
            explicit_fps: true,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("explicit metadata should override Y4M header");
        assert_eq!(job.width, 4);
        assert_eq!(job.height, 4);
        assert_eq!(job.source_format, PixelFormat::Yuv420p8);
        assert_eq!(job.frames, 1);
        assert_eq!(job.fps.as_deref(), Some("60"));

        let mut reader = open_job_reader(&job).expect("open overridden Y4M reader");
        let mut raw = Vec::new();
        reader.read_to_end(&mut raw).expect("read raw frame");
        assert_eq!(raw, frame);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_preserves_high_bit_depth_yuv420_for_av2_path() {
        for bits in [10] {
            let format_name = format!("yuv420p{bits}le");
            let path = temp_yuv_path(&format!("one_frame_8x8_{format_name}"));
            let format = PixelFormat::yuv420(bits).unwrap();
            let samples = format.frame_len(8, 8).unwrap() / format.bytes_per_sample();
            let max_sample = format.bit_depth().max_sample();
            let input = (0..samples)
                .flat_map(|idx| {
                    let sample = if idx % 2 == 0 { 0u16 } else { max_sample };
                    sample.to_le_bytes()
                })
                .collect::<Vec<_>>();
            let mut file = File::create(&path).expect("create temp yuv");
            file.write_all(&input).expect("write temp yuv");
            drop(file);

            let args = EncodeArgs {
                input: Some(path.to_string_lossy().to_string()),
                output: Some("out.obu".to_string()),
                codec: Some("av2".to_string()),
                video: Some(args::VideoSpec {
                    width: 8,
                    height: 8,
                    pixel_format: Some(format_name),
                }),
                frames: None,
                ..EncodeArgs::default()
            };

            let job = encode_job(&args).expect("build encode job");
            assert_eq!(job.frames, 1);
            assert_eq!(job.source_format, format);
            assert_eq!(job.format, format);

            let mut reader = open_job_reader(&job).expect("open reader");
            let mut forwarded = Vec::new();
            reader
                .read_to_end(&mut forwarded)
                .expect("read forwarded frame");
            assert_eq!(forwarded, input);
            let _ = fs::remove_file(path);
        }
    }

    #[test]
    fn encode_job_preserves_high_bit_depth_yuv444_for_av2_path() {
        for bits in [10] {
            let format_name = format!("yuv444p{bits}le");
            let path = temp_yuv_path(&format!("one_frame_8x8_{format_name}"));
            let format = PixelFormat::yuv444(bits).unwrap();
            let input = vec![0xAA; format.frame_len(8, 8).unwrap()];
            let mut file = File::create(&path).expect("create temp yuv");
            file.write_all(&input).expect("write temp yuv");
            drop(file);

            let args = EncodeArgs {
                input: Some(path.to_string_lossy().to_string()),
                output: Some("out.obu".to_string()),
                codec: Some("av2".to_string()),
                video: Some(args::VideoSpec {
                    width: 8,
                    height: 8,
                    pixel_format: Some(format_name),
                }),
                frames: None,
                ..EncodeArgs::default()
            };

            let job = encode_job(&args).expect("build encode job");
            assert_eq!(job.frames, 1);
            assert_eq!(job.source_format, format);
            assert_eq!(job.format, format);

            let mut reader = open_job_reader(&job).expect("open reader");
            let mut forwarded = Vec::new();
            reader
                .read_to_end(&mut forwarded)
                .expect("read forwarded frame");
            assert_eq!(forwarded, input);
            let _ = fs::remove_file(path);
        }
    }

    #[test]
    fn encode_job_preserves_high_bit_depth_yuv420_for_vvc_path() {
        for bits in [10, 12] {
            let format_name = format!("yuv420p{bits}le");
            let path = temp_yuv_path(&format!("one_frame_8x8_{format_name}"));
            let format = PixelFormat::yuv420(bits).unwrap();
            let input = vec![0x55; format.frame_len(8, 8).unwrap()];
            let mut file = File::create(&path).expect("create temp yuv");
            file.write_all(&input).expect("write temp yuv");
            drop(file);

            let args = EncodeArgs {
                input: Some(path.to_string_lossy().to_string()),
                output: Some("out.266".to_string()),
                codec: Some("vvc".to_string()),
                video: Some(args::VideoSpec {
                    width: 8,
                    height: 8,
                    pixel_format: Some(format_name),
                }),
                frames: None,
                ..EncodeArgs::default()
            };

            let job = encode_job(&args).expect("build encode job");
            assert_eq!(job.frames, 1);
            assert_eq!(job.source_format, format);
            assert_eq!(job.format, format);

            let mut reader = open_job_reader(&job).expect("open reader");
            let mut forwarded = Vec::new();
            reader
                .read_to_end(&mut forwarded)
                .expect("read forwarded frame");
            assert_eq!(forwarded, input);
            let _ = fs::remove_file(path);
        }
    }

    #[test]
    fn encode_job_accepts_lossless_yuv420_for_vvc_path() {
        let format_name = "yuv420p10le";
        let path = temp_yuv_path(&format!("one_frame_8x8_{format_name}"));
        let format = PixelFormat::yuv420(10).unwrap();
        let input = vec![0; format.frame_len(8, 8).unwrap()];
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&input).expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.266".to_string()),
            codec: Some("vvc".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some(format_name.to_string()),
            }),
            settings: vec!["lossless=true".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("lossless yuv420 is native for VVC");
        assert!(job.lossless);
        assert_eq!(job.source_format, format);
        assert_eq!(job.format, format);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_leaves_codec_setting_conflicts_to_encoder() {
        let path = temp_yuv_path("one_frame_8x8_qp_lossless_conflict");
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&vec![0; 8 * 8 * 3 / 2])
            .expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("yuv420p8".to_string()),
            }),
            settings: vec!["lossless=true".to_string(), "qp=16".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("codec-specific setting conflicts are codec-owned");
        assert!(job.lossless);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_accepts_qp_setting() {
        let path = temp_yuv_path("one_frame_8x8_qp_setting");
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&vec![0; 8 * 8 * 3 / 2])
            .expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("yuv420p8".to_string()),
            }),
            settings: vec!["qp=24".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("qp setting should parse");
        assert!(!job.lossless);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_preserves_high_bit_depth_yuv444_for_vvc_path() {
        for bits in [10, 12] {
            let format_name = format!("yuv444p{bits}le");
            let path = temp_yuv_path(&format!("one_frame_8x8_{format_name}"));
            let format = PixelFormat::yuv444(bits).unwrap();
            let input = vec![0x66; format.frame_len(8, 8).unwrap()];
            let mut file = File::create(&path).expect("create temp yuv");
            file.write_all(&input).expect("write temp yuv");
            drop(file);

            let args = EncodeArgs {
                input: Some(path.to_string_lossy().to_string()),
                output: Some("out.266".to_string()),
                codec: Some("vvc".to_string()),
                video: Some(args::VideoSpec {
                    width: 8,
                    height: 8,
                    pixel_format: Some(format_name),
                }),
                frames: None,
                ..EncodeArgs::default()
            };

            let job = encode_job(&args).expect("build encode job");
            assert_eq!(job.frames, 1);
            assert_eq!(job.source_format, format);
            assert_eq!(job.format, format);

            let mut reader = open_job_reader(&job).expect("open reader");
            let mut forwarded = Vec::new();
            reader
                .read_to_end(&mut forwarded)
                .expect("read forwarded frame");
            assert_eq!(forwarded, input);
            let _ = fs::remove_file(path);
        }
    }

    #[test]
    fn encode_job_rejects_lossless_without_bit_depth_fallback() {
        let bits = 13;
        let format_name = format!("yuv420p{bits}le");
        let path = temp_yuv_path(&format!("one_frame_8x8_{format_name}"));
        let format = PixelFormat::yuv420(bits).unwrap();
        let input = vec![0; format.frame_len(8, 8).unwrap()];
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&input).expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some(format_name),
            }),
            settings: vec!["lossless=true".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let err = encode_job(&args).expect_err("lossless fallback must be rejected");
        assert!(
            err.contains("lossless encode is not implemented") && err.contains("yuv420p13le"),
            "{err}"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_accepts_lossless_yuv420_for_av2_path() {
        let format_name = "yuv420p10le";
        let path = temp_yuv_path(&format!("one_frame_8x8_{format_name}"));
        let format = PixelFormat::yuv420(10).unwrap();
        let input = vec![0; format.frame_len(8, 8).unwrap()];
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&input).expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some(format_name.to_string()),
            }),
            settings: vec!["lossless=true".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("AV2 lossless 4:2:0 is native");
        assert!(job.lossless);
        assert_eq!(job.source_format, format);
        assert_eq!(job.format, format);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_accepts_lossless_rgb24_for_av2_path() {
        let path = temp_input_path("one_frame_8x8_rgb24", "rgb");
        let input = (0..PixelFormat::Rgb24.frame_len(8, 8).unwrap())
            .map(|index| ((index * 19 + 11) & 0xff) as u8)
            .collect::<Vec<_>>();
        let mut file = File::create(&path).expect("create temp rgb");
        file.write_all(&input).expect("write temp rgb");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("rgb24".to_string()),
            }),
            settings: vec!["lossless=true".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("AV2 lossless rgb24 is native");
        assert!(job.lossless);
        assert_eq!(job.source_format, PixelFormat::Rgb24);
        assert_eq!(job.format, PixelFormat::Rgb24);
        let mut reader = open_job_reader(&job).expect("open rgb reader");
        let mut forwarded = Vec::new();
        reader
            .read_to_end(&mut forwarded)
            .expect("read forwarded rgb frame");
        assert_eq!(forwarded, input);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_accepts_non_lossless_rgb24_for_av2_path() {
        let path = temp_input_path("one_frame_8x8_rgb24_lossy", "rgb");
        let input = vec![0; PixelFormat::Rgb24.frame_len(8, 8).unwrap()];
        let mut file = File::create(&path).expect("create temp rgb");
        file.write_all(&input).expect("write temp rgb");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("rgb24".to_string()),
            }),
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("AV2 non-lossless rgb24 is native");
        assert!(!job.lossless);
        assert_eq!(job.source_format, PixelFormat::Rgb24);
        assert_eq!(job.format, PixelFormat::Rgb24);
        let mut reader = open_job_reader(&job).expect("open rgb reader");
        let mut forwarded = Vec::new();
        reader
            .read_to_end(&mut forwarded)
            .expect("read forwarded rgb frame");
        assert_eq!(forwarded, input);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_accepts_lossless_gbrp8_for_av2_and_vvc_paths() {
        for codec in ["av2", "vvc"] {
            let path = temp_input_path(&format!("one_frame_8x8_gbrp8_{codec}"), "rgb");
            let input = (0..PixelFormat::Gbrp8.frame_len(8, 8).unwrap())
                .map(|index| ((index * 29 + 7) & 0xff) as u8)
                .collect::<Vec<_>>();
            let mut file = File::create(&path).expect("create temp gbrp8");
            file.write_all(&input).expect("write temp gbrp8");
            drop(file);

            let args = EncodeArgs {
                input: Some(path.to_string_lossy().to_string()),
                output: Some(format!("out.{codec}")),
                codec: Some(codec.to_string()),
                video: Some(args::VideoSpec {
                    width: 8,
                    height: 8,
                    pixel_format: Some("gbrp8".to_string()),
                }),
                settings: vec!["lossless=true".to_string()],
                frames: None,
                ..EncodeArgs::default()
            };

            let job = encode_job(&args).expect("lossless gbrp8 is native");
            assert!(job.lossless);
            assert_eq!(job.source_format, PixelFormat::Gbrp8);
            assert_eq!(job.format, PixelFormat::Gbrp8);
            let mut reader = open_job_reader(&job).expect("open gbrp8 reader");
            let mut forwarded = Vec::new();
            reader
                .read_to_end(&mut forwarded)
                .expect("read forwarded gbrp8 frame");
            assert_eq!(forwarded, input);
            let _ = fs::remove_file(path);
        }
    }

    #[test]
    fn encode_job_accepts_rgb24_for_vvc_path_via_common_repack() {
        let path = temp_input_path("one_frame_8x8_rgb24_vvc", "rgb");
        let input = (0..PixelFormat::Rgb24.frame_len(8, 8).unwrap())
            .map(|index| ((index * 13 + 5) & 0xff) as u8)
            .collect::<Vec<_>>();
        let mut file = File::create(&path).expect("create temp rgb");
        file.write_all(&input).expect("write temp rgb");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.vvc".to_string()),
            codec: Some("vvc".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("rgb24".to_string()),
            }),
            settings: vec!["lossless=true".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("VVC rgb24 is repacked into native gbrp8");
        assert!(job.lossless);
        assert_eq!(job.source_format, PixelFormat::Rgb24);
        assert_eq!(job.format, PixelFormat::Gbrp8);
        let mut reader = open_job_reader(&job).expect("open rgb reader");
        let mut forwarded = Vec::new();
        reader
            .read_to_end(&mut forwarded)
            .expect("read forwarded gbrp8 frame");
        assert_eq!(
            forwarded,
            convert_frame_format(&input, 8, 8, PixelFormat::Rgb24, PixelFormat::Gbrp8).unwrap()
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_accepts_lossless_yuv422_for_av2_without_bit_depth_fallback() {
        let format_name = "yuv422p10le";
        let path = temp_yuv_path(&format!("one_frame_8x8_av2_{format_name}"));
        let format = PixelFormat::yuv422(10).unwrap();
        let input = vec![0; format.frame_len(8, 8).unwrap()];
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&input).expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some(format_name.to_string()),
            }),
            settings: vec!["lossless=true".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("AV2 lossless 4:2:2 is native");
        assert!(job.lossless);
        assert_eq!(job.source_format, format);
        assert_eq!(job.format, format);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_accepts_lossless_yuv422_for_vvc_path() {
        let format_name = "yuv422p10le";
        let path = temp_yuv_path(&format!("one_frame_8x8_vvc_{format_name}"));
        let format = PixelFormat::yuv422(10).unwrap();
        let input = vec![0; format.frame_len(8, 8).unwrap()];
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&input).expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.266".to_string()),
            codec: Some("vvc".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some(format_name.to_string()),
            }),
            settings: vec!["lossless=true".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("VVC lossless 4:2:2 is native");
        assert!(job.lossless);
        assert_eq!(job.source_format, format);
        assert_eq!(job.format, format);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn encode_job_accepts_lossy_yuv422_high_depth_for_vvc_path() {
        let format_name = "yuv422p10le";
        let path = temp_yuv_path(&format!("one_frame_8x8_vvc_lossy_{format_name}"));
        let format = PixelFormat::yuv422(10).unwrap();
        let input = vec![0; format.frame_len(8, 8).unwrap()];
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&input).expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.266".to_string()),
            codec: Some("vvc".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some(format_name.to_string()),
            }),
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("VVC lossy 4:2:2 should stay native");
        assert!(!job.lossless);
        assert_eq!(job.source_format, format);
        assert_eq!(job.format, format);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn open_job_reader_hides_unselected_file_suffix() {
        let path = temp_yuv_path("reader_prefix_8x8");
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&vec![0xAA; 8 * 8 * 3 / 2 * 3])
            .expect("write temp yuv");
        drop(file);

        let job = EncodeJob {
            input: EncodeInput::Path(path.clone()),
            output: PathBuf::from("out.obu"),
            recon: None,
            psnr: false,
            transform_filters: Vec::new(),
            frames: 1,
            fps: None,
            validate_y4m_metadata: false,
            width: 8,
            height: 8,
            source_format: PixelFormat::Yuv420p8,
            format: PixelFormat::Yuv420p8,
            lossless: false,
        };
        let mut reader = open_job_reader(&job).expect("open reader");
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).expect("read limited prefix");
        assert_eq!(bytes.len(), 8 * 8 * 3 / 2);
        let _ = fs::remove_file(path);
    }

    #[cfg(feature = "filter-identity")]
    #[test]
    fn encode_job_accepts_identity_filter_with_file_input() {
        let path = temp_yuv_path("identity_filter_8x8");
        let input = (0..PixelFormat::Yuv420p8.frame_len(8, 8).unwrap())
            .map(|index| ((index * 17 + 3) & 0xff) as u8)
            .collect::<Vec<_>>();
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&input).expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("yuv420p8".to_string()),
            }),
            filters: vec!["identity".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("identity filter should be accepted");
        assert_eq!(job.transform_filters, vec![TransformFilterSpec::Identity]);
        let mut reader = open_job_reader(&job).expect("open filtered reader");
        let mut filtered = Vec::new();
        reader
            .read_to_end(&mut filtered)
            .expect("read identity-filtered input");
        assert_eq!(filtered, input);
        let _ = fs::remove_file(path);
    }

    #[cfg(all(feature = "filter-pattern", feature = "filter-identity"))]
    #[test]
    fn encode_job_accepts_pattern_source_followed_by_identity_filter() {
        let args = EncodeArgs {
            input: None,
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("yuv420p8".to_string()),
            }),
            filters: vec!["pattern=black".to_string(), "identity".to_string()],
            frames: Some(1),
            ..EncodeArgs::default()
        };

        let job = encode_job(&args).expect("pattern plus identity should be accepted");
        assert_eq!(job.transform_filters, vec![TransformFilterSpec::Identity]);
        let mut reader = open_job_reader(&job).expect("open filtered pattern reader");
        let mut filtered = Vec::new();
        reader
            .read_to_end(&mut filtered)
            .expect("read filtered pattern input");
        assert_eq!(
            filtered,
            vec![0; PixelFormat::Yuv420p8.frame_len(8, 8).unwrap()]
        );
    }

    #[cfg(feature = "filter-crop")]
    #[test]
    fn encode_job_rejects_transform_filter_scaffolds() {
        let path = temp_yuv_path("crop_filter_8x8");
        let mut file = File::create(&path).expect("create temp yuv");
        file.write_all(&vec![0; 8 * 8 * 3 / 2])
            .expect("write temp yuv");
        drop(file);

        let args = EncodeArgs {
            input: Some(path.to_string_lossy().to_string()),
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("yuv420p8".to_string()),
            }),
            filters: vec!["crop=x=0:y=0:w=8:h=8".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let err = encode_job(&args).expect_err("crop execution should remain explicit");
        assert!(
            err.contains("discovery scaffold but execution is not implemented"),
            "{err}"
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn frame_psnr_reports_yuv422p8() {
        let job = EncodeJob {
            input: EncodeInput::Pattern(PatternSourceSpec {
                pattern: "black".to_string(),
            }),
            output: PathBuf::from("out.vvc"),
            recon: None,
            psnr: false,
            transform_filters: Vec::new(),
            frames: 1,
            fps: None,
            validate_y4m_metadata: false,
            width: 2,
            height: 2,
            source_format: PixelFormat::Yuv422p8,
            format: PixelFormat::Yuv422p8,
            lossless: false,
        };
        let source = vec![0; 8];
        let reconstruction = vec![1; 8];

        let psnr = frame_psnr(&job, &source, &reconstruction).expect("4:2:2 PSNR");
        assert!(psnr.y.is_finite());
        assert!(psnr.u.is_finite());
        assert!(psnr.v.is_finite());
        assert!(psnr.all.is_finite());
    }

    #[test]
    fn frame_psnr_uses_high_bit_depth_peak_sample() {
        let format = PixelFormat::yuv420(10).unwrap();
        let job = EncodeJob {
            input: EncodeInput::Pattern(PatternSourceSpec {
                pattern: "black".to_string(),
            }),
            output: PathBuf::from("out.vvc"),
            recon: None,
            psnr: false,
            transform_filters: Vec::new(),
            frames: 1,
            fps: None,
            validate_y4m_metadata: false,
            width: 2,
            height: 2,
            source_format: format,
            format,
            lossless: false,
        };
        let source = vec![0; 12];
        let mut reconstruction = vec![0; 12];
        for sample in reconstruction.chunks_exact_mut(2) {
            sample.copy_from_slice(&1u16.to_le_bytes());
        }

        let psnr = frame_psnr(&job, &source, &reconstruction).expect("10-bit PSNR");
        assert!(psnr.all > 60.0, "10-bit peak sample should be used");
    }

    #[cfg(feature = "filter-pattern")]
    #[test]
    fn encode_job_requires_frames_for_pattern_source() {
        let args = EncodeArgs {
            input: None,
            output: Some("out.obu".to_string()),
            codec: Some("av2".to_string()),
            video: Some(args::VideoSpec {
                width: 8,
                height: 8,
                pixel_format: Some("yuv420p8".to_string()),
            }),
            filters: vec!["pattern=black".to_string()],
            frames: None,
            ..EncodeArgs::default()
        };

        let err = encode_job(&args).expect_err("pattern source needs explicit frame count");
        assert!(err.contains("source filters require --frames"), "{err}");
    }
}
