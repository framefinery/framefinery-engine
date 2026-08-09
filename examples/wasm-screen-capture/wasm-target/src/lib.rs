use std::cell::RefCell;

use framefinery::{encoder, Frame, FrameInfo, PixelFormat, VideoEncodeOutput};

const STATUS_OK: i32 = 0;
const STATUS_ERROR: i32 = -1;
const CODEC_AV2: u32 = 1;
const CODEC_VVC: u32 = 2;
const VERSION: &str = env!("CARGO_PKG_VERSION");

thread_local! {
    static STATE: RefCell<WasmState> = RefCell::new(WasmState::default());
    static LAST_ERROR: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

#[derive(Default)]
struct WasmState {
    config: Option<CaptureConfig>,
    input_rgba: Vec<u8>,
    frame_gbrp: Vec<u8>,
    last_output: Vec<u8>,
    output: Vec<u8>,
    stats: EncodeStats,
}

#[derive(Clone, Copy)]
struct CaptureConfig {
    codec: WasmCodec,
    width: usize,
    height: usize,
    rgba_frame_len: usize,
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
    psnr_count: usize,
    psnr_all: f64,
}

impl Default for EncodeStats {
    fn default() -> Self {
        Self {
            frames: 0,
            total_bytes: 0,
            psnr_count: 0,
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
pub extern "C" fn ff_wasm_encode_frame() -> i32 {
    run_status(encode_frame_from_rgba)
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
pub extern "C" fn ff_wasm_last_output_ptr() -> u32 {
    STATE.with(|state| {
        let state = state.borrow();
        if state.last_output.is_empty() {
            0
        } else {
            state.last_output.as_ptr() as u32
        }
    })
}

#[no_mangle]
pub extern "C" fn ff_wasm_last_output_len() -> u32 {
    STATE.with(|state| saturating_u32(state.borrow().last_output.len()))
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
            max_frames,
            lossless,
            qp,
            gop,
        });
        state.input_rgba.clear();
        state.input_rgba.resize(rgba_frame_len, 0);
        state.frame_gbrp.clear();
        state.frame_gbrp.resize(gbrp_frame_len, 0);
        state.last_output.clear();
        state.output.clear();
        state.stats = EncodeStats::default();
    });
    Ok(())
}

fn encode_frame_from_rgba() -> WasmResult<()> {
    let (config, frame_data) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let config = state
            .config
            .ok_or_else(|| "WASM encoder is not configured".to_string())?;
        if state.stats.frames >= config.max_frames {
            return Err(format!(
                "encoder already accepted {} frame(s), max is {}",
                state.stats.frames, config.max_frames
            ));
        }
        if state.input_rgba.len() != config.rgba_frame_len {
            return Err("input RGBA buffer length does not match the configured frame".to_string());
        }

        let WasmState {
            input_rgba,
            frame_gbrp,
            ..
        } = &mut *state;
        rgba_to_gbrp8(input_rgba, frame_gbrp, config.width * config.height);
        Ok::<_, String>((config, frame_gbrp.clone()))
    })?;

    let info = FrameInfo::new(config.width, config.height, PixelFormat::Gbrp8)
        .map_err(|err| err.to_string())?;
    let builder = encoder(config.codec.as_str())
        .map_err(|err| err.to_string())?
        .input(info)
        .frame_limit(1)
        .metrics_only()
        .gop(config.gop)
        .map_err(|err| err.to_string())?;
    let builder = if config.lossless {
        builder.lossless()
    } else {
        builder.qp(config.qp).map_err(|err| err.to_string())?
    };

    let frame = Frame::new(info, frame_data).map_err(|err| err.to_string())?;
    let frame_output = builder.encode_frame(frame).map_err(|err| err.to_string())?;

    STATE.with(|state| {
        let mut state = state.borrow_mut();
        append_encode_output(&mut state, frame_output);
    });
    Ok(())
}

fn append_encode_output(state: &mut WasmState, frame_output: VideoEncodeOutput) {
    state.last_output.clear();
    for chunk in frame_output.chunks {
        state.last_output.extend_from_slice(&chunk.data);
    }
    let frame_bytes = state.last_output.len();
    if frame_bytes > 0 {
        state.output.extend_from_slice(&state.last_output);
    }
    state.stats.frames += 1;
    state.stats.total_bytes += frame_bytes;
    for metric in frame_output.metrics {
        if let Some(psnr) = metric.psnr {
            let previous_sum = if state.stats.psnr_count == 0 {
                0.0
            } else {
                state.stats.psnr_all * state.stats.psnr_count as f64
            };
            state.stats.psnr_count += 1;
            state.stats.psnr_all = (previous_sum + psnr) / state.stats.psnr_count as f64;
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
