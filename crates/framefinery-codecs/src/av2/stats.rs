#[cfg(feature = "av2-stats")]
use std::time::Instant;

#[cfg(feature = "av2-stats")]
use crate::instrumentation::JsonlInstrumentationSink;

use super::{Av2StreamFormat, Av2VideoGeometry};
use crate::PixelFormat;

#[cfg(feature = "av2-stats")]
const AV2_STATS_ENV: &str = "FRAMEFINERY_AV2_STATS";

pub(super) struct Av2StatsSink {
    #[cfg(feature = "av2-stats")]
    sink: Option<JsonlInstrumentationSink>,
}

impl Av2StatsSink {
    pub(super) fn from_env() -> Result<Self, String> {
        Ok(Self {
            #[cfg(feature = "av2-stats")]
            sink: JsonlInstrumentationSink::append_from_env(AV2_STATS_ENV)
                .map_err(|err| err.to_string())?,
        })
    }

    pub(super) fn write_frame(&mut self, frame: &Av2FrameStats) -> Result<(), String> {
        #[cfg(feature = "av2-stats")]
        {
            let Some(sink) = self.sink.as_mut() else {
                return Ok(());
            };
            sink.write_json_line(&frame.to_json_line())
                .map_err(|err| err.to_string())?;
            sink.flush().map_err(|err| err.to_string())?;
        }
        #[cfg(not(feature = "av2-stats"))]
        let _ = frame;
        Ok(())
    }
}

pub(super) struct Av2FrameStats {
    #[cfg(feature = "av2-stats")]
    frame_idx: usize,
    #[cfg(feature = "av2-stats")]
    width: usize,
    #[cfg(feature = "av2-stats")]
    height: usize,
    #[cfg(feature = "av2-stats")]
    input_format: PixelFormat,
    #[cfg(feature = "av2-stats")]
    stream_format: Av2StreamFormat,
    #[cfg(feature = "av2-stats")]
    lossless: bool,
    #[cfg(feature = "av2-stats")]
    qp: Option<u8>,
    #[cfg(feature = "av2-stats")]
    predictive: bool,
    #[cfg(feature = "av2-stats")]
    bitstream_bytes: usize,
    #[cfg(feature = "av2-stats")]
    stages: Vec<Av2StageStats>,
}

impl Av2FrameStats {
    pub(super) fn new(
        frame_idx: usize,
        geometry: Av2VideoGeometry,
        input_format: PixelFormat,
        stream_format: Av2StreamFormat,
        lossless: bool,
        qp: Option<u8>,
        predictive: bool,
    ) -> Self {
        #[cfg(not(feature = "av2-stats"))]
        let _ = (
            frame_idx,
            geometry,
            input_format,
            stream_format,
            lossless,
            qp,
            predictive,
        );
        Self {
            #[cfg(feature = "av2-stats")]
            frame_idx,
            #[cfg(feature = "av2-stats")]
            width: geometry.width,
            #[cfg(feature = "av2-stats")]
            height: geometry.height,
            #[cfg(feature = "av2-stats")]
            input_format,
            #[cfg(feature = "av2-stats")]
            stream_format,
            #[cfg(feature = "av2-stats")]
            lossless,
            #[cfg(feature = "av2-stats")]
            qp,
            #[cfg(feature = "av2-stats")]
            predictive,
            #[cfg(feature = "av2-stats")]
            bitstream_bytes: 0,
            #[cfg(feature = "av2-stats")]
            stages: Vec::new(),
        }
    }

    pub(super) fn set_bitstream_bytes(&mut self, bytes: usize) {
        #[cfg(feature = "av2-stats")]
        {
            self.bitstream_bytes = bytes;
        }
        #[cfg(not(feature = "av2-stats"))]
        let _ = bytes;
    }

    pub(super) fn add_elapsed(&mut self, name: &'static str, start: Av2StageStart) {
        #[cfg(feature = "av2-stats")]
        self.add_stage(name, start.elapsed_nanos());
        #[cfg(not(feature = "av2-stats"))]
        let _ = (name, start);
    }

    #[cfg(feature = "av2-stats")]
    fn add_stage(&mut self, name: &'static str, nanos: u64) {
        if let Some(stage) = self.stages.iter_mut().find(|stage| stage.name == name) {
            stage.nanos += nanos;
        } else {
            self.stages.push(Av2StageStats { name, nanos });
        }
    }

    #[cfg(feature = "av2-stats")]
    fn to_json_line(&self) -> String {
        let qp = self
            .qp
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_owned());
        let mut json = format!(
            "{{\"kind\":\"framefinery.av2.stats.v1\",\"frame_index\":{},\"width\":{},\"height\":{},\"input_format\":\"{}\",\"chroma_sampling\":\"{:?}\",\"bit_depth\":{},\"lossless\":{},\"qp\":{},\"predictive\":{},\"bitstream_bytes\":{},\"stages\":[",
            self.frame_idx,
            self.width,
            self.height,
            self.input_format,
            self.stream_format.chroma_format,
            self.stream_format.bit_depth.bits(),
            self.lossless,
            qp,
            self.predictive,
            self.bitstream_bytes
        );
        for (index, stage) in self.stages.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str(&format!(
                "{{\"name\":\"{}\",\"ns\":{}}}",
                stage.name, stage.nanos
            ));
        }
        json.push_str("],\"counters\":[]}");
        json
    }
}

#[cfg(feature = "av2-stats")]
struct Av2StageStats {
    name: &'static str,
    nanos: u64,
}

pub(super) struct Av2StageStart {
    #[cfg(feature = "av2-stats")]
    start: Instant,
}

impl Av2StageStart {
    pub(super) fn now() -> Self {
        Self {
            #[cfg(feature = "av2-stats")]
            start: Instant::now(),
        }
    }

    #[cfg(feature = "av2-stats")]
    fn elapsed_nanos(self) -> u64 {
        self.start.elapsed().as_nanos() as u64
    }
}
