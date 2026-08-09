use std::cell::RefCell;

use framefinery::{
    encoder, FrameInfo, PixelFormat, Result as FrameFineryResult, VideoEncodeFrameMetrics,
};

const STATUS_OK: i32 = 0;
const STATUS_ERROR: i32 = -1;
const CODEC_AV2: u32 = 1;
const CODEC_VVC: u32 = 2;
const MAX_CAPTURE_BYTES: usize = 512 * 1024 * 1024;
const VERSION: &str = env!("CARGO_PKG_VERSION");

thread_local! {
    static STATE: RefCell<WasmState> = RefCell::new(WasmState::default());
    static LAST_ERROR: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

#[derive(Default)]
struct WasmState {
    config: Option<CaptureConfig>,
    input_rgba: Vec<u8>,
    frames_gbrp: Vec<u8>,
    output: Vec<u8>,
    stats: EncodeStats,
}

#[derive(Clone, Copy)]
struct CaptureConfig {
    codec: WasmCodec,
    width: usize,
    height: usize,
    rgba_frame_len: usize,
    gbrp_frame_len: usize,
    max_frames: usize,
    lossless: bool,
    qp: u8,
    gop: i32,
}

#[derive(Clone, Copy)]
enum WasmCodec {
    Av2,
    Vvc,
}

#[derive(Clone, Copy)]
struct EncodeStats {
    frames: usize,
    total_bytes: usize,
    encode_ms: f64,
    psnr_all: f64,
}

impl Default for EncodeStats {
    fn default() -> Self {
        Self {
            frames: 0,
            total_bytes: 0,
            encode_ms: 0.0,
            psnr_all: f64::NAN,
        }
    }
}

impl WasmCodec {
    fn parse(value: u32) -> WasmResult<Self> {
        match value {
            CODEC_AV2 => Ok(Self::Av2),
            CODEC_VVC => Ok(Self::Vvc),
            _ => Err(format!(
                "unsupported codec id {value}; expected {CODEC_AV2} for av2 or {CODEC_VVC} for vvc"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Av2 => "av2",
            Self::Vvc => "vvc",
        }
    }
}

type WasmResult<T> = std::result::Result<T, String>;

#[no_mangle]
pub extern "C" fn ff_wasm_codec_av2() -> u32 {
    CODEC_AV2
}

#[no_mangle]
pub extern "C" fn ff_wasm_codec_vvc() -> u32 {
    CODEC_VVC
}

#[no_mangle]
pub extern "C" fn ff_wasm_version_ptr() -> u32 {
    VERSION.as_ptr() as u32
}

#[no_mangle]
pub extern "C" fn ff_wasm_version_len() -> u32 {
    VERSION.len() as u32
}

#[no_mangle]
pub extern "C" fn ff_wasm_configure(
    codec: u32,
    width: u32,
    height: u32,
    max_frames: u32,
    lossless: u32,
    qp: u32,
    gop: i32,
) -> i32 {
    run_status(|| configure(codec, width, height, max_frames, lossless != 0, qp, gop))
}

#[no_mangle]
pub extern "C" fn ff_wasm_input_ptr() -> u32 {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.input_rgba.is_empty() {
            0
        } else {
            state.input_rgba.as_mut_ptr() as u32
        }
    })
}

#[no_mangle]
pub extern "C" fn ff_wasm_input_len() -> u32 {
    STATE.with(|state| saturating_u32(state.borrow().input_rgba.len()))
}

#[no_mangle]
pub extern "C" fn ff_wasm_push_rgba_frame() -> i32 {
    run_status(push_rgba_frame)
}

#[no_mangle]
pub extern "C" fn ff_wasm_encode() -> i32 {
    run_status(encode_captured_frames)
}

#[no_mangle]
pub extern "C" fn ff_wasm_reset_capture() {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.frames_gbrp.clear();
        state.output.clear();
        state.stats = EncodeStats::default();
    });
    clear_error();
}

#[no_mangle]
pub extern "C" fn ff_wasm_frame_count() -> u32 {
    STATE.with(|state| {
        let state = state.borrow();
        let Some(config) = state.config else {
            return 0;
        };
        saturating_u32(state.frames_gbrp.len() / config.gbrp_frame_len)
    })
}

#[no_mangle]
pub extern "C" fn ff_wasm_output_ptr() -> u32 {
    STATE.with(|state| {
        let state = state.borrow();
        if state.output.is_empty() {
            0
        } else {
            state.output.as_ptr() as u32
        }
    })
}

#[no_mangle]
pub extern "C" fn ff_wasm_output_len() -> u32 {
    STATE.with(|state| saturating_u32(state.borrow().output.len()))
}

#[no_mangle]
pub extern "C" fn ff_wasm_encoded_frames() -> u32 {
    STATE.with(|state| saturating_u32(state.borrow().stats.frames))
}

#[no_mangle]
pub extern "C" fn ff_wasm_encoded_bytes() -> u32 {
    STATE.with(|state| saturating_u32(state.borrow().stats.total_bytes))
}

#[no_mangle]
pub extern "C" fn ff_wasm_encode_ms() -> f64 {
    STATE.with(|state| state.borrow().stats.encode_ms)
}

#[no_mangle]
pub extern "C" fn ff_wasm_psnr_all() -> f64 {
    STATE.with(|state| state.borrow().stats.psnr_all)
}

#[no_mangle]
pub extern "C" fn ff_wasm_last_error_ptr() -> u32 {
    LAST_ERROR.with(|error| {
        let error = error.borrow();
        if error.is_empty() {
            0
        } else {
            error.as_ptr() as u32
        }
    })
}

#[no_mangle]
pub extern "C" fn ff_wasm_last_error_len() -> u32 {
    LAST_ERROR.with(|error| saturating_u32(error.borrow().len()))
}

fn configure(
    codec: u32,
    width: u32,
    height: u32,
    max_frames: u32,
    lossless: bool,
    qp: u32,
    gop: i32,
) -> WasmResult<()> {
    let codec = WasmCodec::parse(codec)?;
    let width = usize::try_from(width).map_err(|_| "width does not fit usize".to_string())?;
    let height = usize::try_from(height).map_err(|_| "height does not fit usize".to_string())?;
    let max_frames =
        usize::try_from(max_frames).map_err(|_| "frame count does not fit usize".to_string())?;
    if max_frames == 0 {
        return Err("max_frames must be greater than zero".to_string());
    }
    let qp = u8::try_from(qp).map_err(|_| "qp must be in the range 1..255".to_string())?;
    let info = FrameInfo::new(width, height, PixelFormat::Gbrp8).map_err(|err| err.to_string())?;
    let rgba_frame_len = checked_frame_len(width, height, 4)?;
    let gbrp_frame_len = checked_frame_len(width, height, 3)?;
    let max_capture_len = gbrp_frame_len
        .checked_mul(max_frames)
        .ok_or_else(|| "capture byte length overflow".to_string())?;
    if max_capture_len > MAX_CAPTURE_BYTES {
        return Err(format!(
            "capture would reserve up to {max_capture_len} bytes; current target-practice limit is {MAX_CAPTURE_BYTES} bytes"
        ));
    }

    let builder = encoder(codec.as_str())
        .map_err(|err| err.to_string())?
        .input(info)
        .frame_limit(max_frames)
        .metrics_only()
        .gop(gop)
        .map_err(|err| err.to_string())?;
    let builder = if lossless {
        builder.lossless()
    } else {
        builder.qp(qp).map_err(|err| err.to_string())?
    };
    builder.into_config().map_err(|err| err.to_string())?;

    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.config = Some(CaptureConfig {
            codec,
            width,
            height,
            rgba_frame_len,
            gbrp_frame_len,
            max_frames,
            lossless,
            qp,
            gop,
        });
        state.input_rgba.clear();
        state.input_rgba.resize(rgba_frame_len, 0);
        state.frames_gbrp.clear();
        state.output.clear();
        state.stats = EncodeStats::default();
    });
    Ok(())
}

fn push_rgba_frame() -> WasmResult<()> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let config = state
            .config
            .ok_or_else(|| "WASM encoder is not configured".to_string())?;
        let frames = state.frames_gbrp.len() / config.gbrp_frame_len;
        if frames >= config.max_frames {
            return Err(format!(
                "capture already holds {frames} frame(s), max is {}",
                config.max_frames
            ));
        }
        if state.input_rgba.len() != config.rgba_frame_len {
            return Err("input RGBA buffer length does not match the configured frame".to_string());
        }

        let WasmState {
            input_rgba,
            frames_gbrp,
            ..
        } = &mut *state;
        frames_gbrp
            .try_reserve_exact(config.gbrp_frame_len)
            .map_err(|_| "failed to reserve frame storage".to_string())?;
        let start = frames_gbrp.len();
        frames_gbrp.resize(start + config.gbrp_frame_len, 0);
        let output = &mut frames_gbrp[start..start + config.gbrp_frame_len];
        rgba_to_gbrp8(input_rgba, output, config.width * config.height);
        Ok(())
    })
}

fn encode_captured_frames() -> WasmResult<()> {
    let (config, frames_gbrp) = STATE.with(|state| {
        let state = state.borrow();
        let config = state
            .config
            .ok_or_else(|| "WASM encoder is not configured".to_string())?;
        Ok::<_, String>((config, state.frames_gbrp.clone()))
    })?;
    let frame_count = frames_gbrp.len() / config.gbrp_frame_len;
    if frame_count == 0 {
        return Err("no frames have been captured".to_string());
    }

    let info = FrameInfo::new(config.width, config.height, PixelFormat::Gbrp8)
        .map_err(|err| err.to_string())?;
    let builder = encoder(config.codec.as_str())
        .map_err(|err| err.to_string())?
        .input(info)
        .frame_limit(frame_count)
        .metrics_only()
        .gop(config.gop)
        .map_err(|err| err.to_string())?;
    let builder = if config.lossless {
        builder.lossless()
    } else {
        builder.qp(config.qp).map_err(|err| err.to_string())?
    };

    let mut offset = 0usize;
    let mut source = |frame: &mut [u8]| -> FrameFineryResult<bool> {
        if offset >= frames_gbrp.len() {
            return Ok(false);
        }
        let next = offset + config.gbrp_frame_len;
        frame.copy_from_slice(&frames_gbrp[offset..next]);
        offset = next;
        Ok(true)
    };
    let mut output = Vec::new();
    let mut stats = MetricsAccumulator::default();
    let mut callback = |metrics: VideoEncodeFrameMetrics<'_>| {
        stats.push(metrics);
    };

    builder
        .encode_source(&mut source, &mut output, None, Some(&mut callback))
        .map_err(|err| err.to_string())?;
    let stats = stats.finish();

    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.output = output;
        state.stats = stats;
    });
    Ok(())
}

#[derive(Default)]
struct MetricsAccumulator {
    frames: usize,
    total_bytes: usize,
    encode_ms: f64,
    psnr_sum: f64,
    psnr_count: usize,
}

impl MetricsAccumulator {
    fn push(&mut self, metrics: VideoEncodeFrameMetrics<'_>) {
        self.frames = metrics.frame_idx + 1;
        self.total_bytes = metrics.total_bitstream_bytes;
        self.encode_ms += metrics.encode_elapsed.as_secs_f64() * 1000.0;
        if let Some(psnr) = metrics.psnr {
            self.psnr_sum += psnr.all;
            self.psnr_count += 1;
        }
    }

    fn finish(self) -> EncodeStats {
        EncodeStats {
            frames: self.frames,
            total_bytes: self.total_bytes,
            encode_ms: self.encode_ms,
            psnr_all: if self.psnr_count == 0 {
                f64::NAN
            } else {
                self.psnr_sum / self.psnr_count as f64
            },
        }
    }
}

fn rgba_to_gbrp8(input: &[u8], output: &mut [u8], pixels: usize) {
    let (g_plane, rest) = output.split_at_mut(pixels);
    let (b_plane, r_plane) = rest.split_at_mut(pixels);
    for pixel in 0..pixels {
        let base = pixel * 4;
        r_plane[pixel] = input[base];
        g_plane[pixel] = input[base + 1];
        b_plane[pixel] = input[base + 2];
    }
}

fn checked_frame_len(width: usize, height: usize, channels: usize) -> WasmResult<usize> {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(channels))
        .ok_or_else(|| "frame byte length overflow".to_string())
}

fn run_status(action: impl FnOnce() -> WasmResult<()>) -> i32 {
    match action() {
        Ok(()) => {
            clear_error();
            STATUS_OK
        }
        Err(err) => {
            set_error(err);
            STATUS_ERROR
        }
    }
}

fn set_error(error: String) {
    LAST_ERROR.with(|last_error| {
        *last_error.borrow_mut() = error.into_bytes();
    });
}

fn clear_error() {
    LAST_ERROR.with(|last_error| last_error.borrow_mut().clear());
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
