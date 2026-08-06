use std::collections::VecDeque;

use crate::error::Result;
use crate::{Frame, FrameInfo, MediaError, Packet, RawVideoFrameSource};

/// Counters returned after running a frame-filter pipeline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FilterPipelineStats {
    /// Frames pulled from the source.
    pub input_frames: usize,
    /// Frames pushed to the sink after every filter stage.
    pub output_frames: usize,
}

/// Counters returned after running a frame-to-packet encode pipeline.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EncodePipelineStats {
    /// Frames pulled from the source.
    pub input_frames: usize,
    /// Frames submitted to the encoder after filtering.
    pub encoded_frames: usize,
    /// Packets pushed to the sink, including encoder flush packets.
    pub output_packets: usize,
}

/// Pull-based source stage for generic media pipelines.
pub trait Source {
    /// Item produced by this source.
    type Output;

    /// Pull the next item, returning `Ok(None)` at clean end of stream.
    fn pull(&mut self) -> Result<Option<Self::Output>>;
}

impl<T> Source for Box<T>
where
    T: Source + ?Sized,
{
    type Output = T::Output;

    fn pull(&mut self) -> Result<Option<Self::Output>> {
        (**self).pull()
    }
}

/// Push-based terminal stage for generic media pipelines.
pub trait Sink<I> {
    /// Consume one pipeline item.
    fn push(&mut self, input: I) -> Result<()>;

    /// Finish the sink after every upstream stage has completed.
    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Frame-to-frame transform stage.
pub trait Filter {
    /// Process one frame into zero or more output frames.
    fn process(&mut self, frame: Frame) -> Result<Vec<Frame>>;

    /// Flush delayed frames at end of stream.
    fn finish(&mut self) -> Result<Vec<Frame>> {
        Ok(Vec::new())
    }
}

/// Frame-to-packet encoder stage for the generic pipeline helpers.
pub trait Encoder {
    /// Encode one frame into zero or more packets.
    fn encode(&mut self, frame: Frame) -> Result<Vec<Packet>>;

    /// Flush delayed packets at end of stream.
    fn finish(&mut self) -> Result<Vec<Packet>> {
        Ok(Vec::new())
    }
}

/// Adapter that exposes a [`Source`] of owned frames as a raw-frame callback.
///
/// This is useful at API boundaries where frames are naturally produced by a
/// pipeline stage, but an encoder wants to pull one raw frame into a caller
/// supplied buffer.
pub struct FrameSourceRawVideoAdapter<S> {
    source: S,
    info: FrameInfo,
}

impl<S> FrameSourceRawVideoAdapter<S>
where
    S: Source<Output = Frame>,
{
    /// Create an adapter for a source that must emit frames matching `info`.
    pub fn new(source: S, info: FrameInfo) -> Self {
        Self { source, info }
    }

    /// Return the raw frame metadata expected from the wrapped source.
    pub const fn info(&self) -> FrameInfo {
        self.info
    }

    /// Consume the adapter and return the wrapped source.
    pub fn into_inner(self) -> S {
        self.source
    }
}

impl<S> RawVideoFrameSource for FrameSourceRawVideoAdapter<S>
where
    S: Source<Output = Frame>,
{
    fn read_frame(&mut self, output: &mut [u8]) -> Result<bool> {
        validate_raw_video_output_len(self.info, output)?;
        let Some(frame) = self.source.pull()? else {
            return Ok(false);
        };
        if frame.info() != self.info {
            return Err(MediaError::IncompatibleFormat {
                format: frame.info().format.name(),
                reason: format!(
                    "expected {}x{}:{}, got {}x{}:{}",
                    self.info.width,
                    self.info.height,
                    self.info.format,
                    frame.info().width,
                    frame.info().height,
                    frame.info().format
                ),
            });
        }
        output.copy_from_slice(frame.data());
        Ok(true)
    }
}

/// Raw-frame source wrapper that applies a frame-filter chain on demand.
///
/// The wrapper owns only the current input frame and any output frames already
/// produced by the filters but not yet consumed. It does not materialize the
/// whole filtered stream, which keeps long native streams and future WASM
/// capture flows bounded in memory.
pub struct FilteredRawVideoFrameSource<S> {
    source: S,
    input_info: FrameInfo,
    output_info: FrameInfo,
    filters: Vec<Box<dyn Filter>>,
    pending: VecDeque<Frame>,
    source_finished: bool,
    flush_index: usize,
}

impl<S> FilteredRawVideoFrameSource<S>
where
    S: RawVideoFrameSource,
{
    /// Create a filtered raw-video source from concrete filter objects.
    pub fn new(
        source: S,
        input_info: FrameInfo,
        output_info: FrameInfo,
        filters: Vec<Box<dyn Filter>>,
    ) -> Self {
        Self {
            source,
            input_info,
            output_info,
            filters,
            pending: VecDeque::new(),
            source_finished: false,
            flush_index: 0,
        }
    }

    /// Frame metadata expected by the wrapped source before filters run.
    pub const fn input_info(&self) -> FrameInfo {
        self.input_info
    }

    /// Frame metadata emitted after filters run.
    pub const fn output_info(&self) -> FrameInfo {
        self.output_info
    }

    /// Consume this adapter and return the wrapped raw-frame source.
    pub fn into_inner(self) -> S {
        self.source
    }

    fn fill_pending(&mut self) -> Result<()> {
        while self.pending.is_empty() {
            if !self.source_finished {
                let mut data = vec![0; self.input_info.expected_len()];
                if self.source.read_frame(&mut data)? {
                    let frame = Frame::new(self.input_info, data)?;
                    let frames = self.process_frames_from(vec![frame], 0)?;
                    self.queue_frames(frames)?;
                    continue;
                }
                self.source_finished = true;
            }

            if self.flush_index >= self.filters.len() {
                break;
            }

            let index = self.flush_index;
            let frames = {
                let filter = &mut self.filters[index];
                filter.finish()?
            };
            let frames = self.process_frames_from(frames, index + 1)?;
            self.flush_index += 1;
            self.queue_frames(frames)?;
        }
        Ok(())
    }

    fn process_frames_from(&mut self, mut frames: Vec<Frame>, start: usize) -> Result<Vec<Frame>> {
        for filter in self.filters.iter_mut().skip(start) {
            let mut next = Vec::new();
            for frame in frames {
                next.extend(filter.process(frame)?);
            }
            frames = next;
            if frames.is_empty() {
                break;
            }
        }
        Ok(frames)
    }

    fn queue_frames(&mut self, frames: Vec<Frame>) -> Result<()> {
        for frame in frames {
            if frame.info() != self.output_info {
                return Err(MediaError::IncompatibleFormat {
                    format: frame.info().format.name(),
                    reason: format!(
                        "expected filtered output {}x{}:{}, got {}x{}:{}",
                        self.output_info.width,
                        self.output_info.height,
                        self.output_info.format,
                        frame.info().width,
                        frame.info().height,
                        frame.info().format
                    ),
                });
            }
            self.pending.push_back(frame);
        }
        Ok(())
    }
}

impl<S> RawVideoFrameSource for FilteredRawVideoFrameSource<S>
where
    S: RawVideoFrameSource,
{
    fn read_frame(&mut self, output: &mut [u8]) -> Result<bool> {
        validate_raw_video_output_len(self.output_info, output)?;
        self.fill_pending()?;
        let Some(frame) = self.pending.pop_front() else {
            return Ok(false);
        };
        output.copy_from_slice(frame.data());
        Ok(true)
    }
}

fn validate_raw_video_output_len(info: FrameInfo, output: &[u8]) -> Result<()> {
    let expected = info.expected_len();
    let actual = output.len();
    if actual != expected {
        return Err(MediaError::BufferLength { expected, actual });
    }
    Ok(())
}

/// Run a source through zero or more frame filters into a frame sink.
pub fn run_frame_filter_pipeline(
    source: &mut dyn Source<Output = Frame>,
    filters: &mut [&mut dyn Filter],
    sink: &mut dyn Sink<Frame>,
) -> Result<FilterPipelineStats> {
    let mut stats = FilterPipelineStats::default();
    while let Some(frame) = source.pull()? {
        stats.input_frames += 1;
        let frames = process_frames_from(vec![frame], 0, filters)?;
        stats.output_frames += push_frames(frames, sink)?;
    }

    for filter_index in 0..filters.len() {
        let frames = filters[filter_index].finish()?;
        let frames = process_frames_from(frames, filter_index + 1, filters)?;
        stats.output_frames += push_frames(frames, sink)?;
    }

    sink.finish()?;
    Ok(stats)
}

/// Run a source through zero or more frame filters, an encoder, and a packet sink.
pub fn run_frame_encode_pipeline(
    source: &mut dyn Source<Output = Frame>,
    filters: &mut [&mut dyn Filter],
    encoder: &mut dyn Encoder,
    sink: &mut dyn Sink<Packet>,
) -> Result<EncodePipelineStats> {
    let mut stats = EncodePipelineStats::default();
    while let Some(frame) = source.pull()? {
        stats.input_frames += 1;
        let frames = process_frames_from(vec![frame], 0, filters)?;
        stats.encoded_frames += encode_frames(frames, encoder, sink, &mut stats)?;
    }

    for filter_index in 0..filters.len() {
        let frames = filters[filter_index].finish()?;
        let frames = process_frames_from(frames, filter_index + 1, filters)?;
        stats.encoded_frames += encode_frames(frames, encoder, sink, &mut stats)?;
    }

    for packet in encoder.finish()? {
        stats.output_packets += 1;
        sink.push(packet)?;
    }
    sink.finish()?;
    Ok(stats)
}

fn process_frames_from(
    mut frames: Vec<Frame>,
    start: usize,
    filters: &mut [&mut dyn Filter],
) -> Result<Vec<Frame>> {
    for filter in filters.iter_mut().skip(start) {
        let mut next = Vec::new();
        for frame in frames {
            next.extend(filter.process(frame)?);
        }
        frames = next;
    }
    Ok(frames)
}

fn push_frames(frames: Vec<Frame>, sink: &mut dyn Sink<Frame>) -> Result<usize> {
    let count = frames.len();
    for frame in frames {
        sink.push(frame)?;
    }
    Ok(count)
}

fn encode_frames(
    frames: Vec<Frame>,
    encoder: &mut dyn Encoder,
    sink: &mut dyn Sink<Packet>,
    stats: &mut EncodePipelineStats,
) -> Result<usize> {
    let frame_count = frames.len();
    for frame in frames {
        for packet in encoder.encode(frame)? {
            stats.output_packets += 1;
            sink.push(packet)?;
        }
    }
    Ok(frame_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FrameInfo, MediaError, PixelFormat, StreamId};

    struct VecFrameSource {
        frames: std::vec::IntoIter<Frame>,
    }

    impl VecFrameSource {
        fn new(frames: Vec<Frame>) -> Self {
            Self {
                frames: frames.into_iter(),
            }
        }
    }

    struct VecRawFrameSource {
        info: FrameInfo,
        frames: VecDeque<Vec<u8>>,
        reads: usize,
    }

    impl VecRawFrameSource {
        fn new(info: FrameInfo, frames: Vec<Vec<u8>>) -> Self {
            Self {
                info,
                frames: frames.into(),
                reads: 0,
            }
        }
    }

    impl RawVideoFrameSource for VecRawFrameSource {
        fn read_frame(&mut self, output: &mut [u8]) -> Result<bool> {
            assert_eq!(output.len(), self.info.expected_len());
            let Some(frame) = self.frames.pop_front() else {
                return Ok(false);
            };
            output.copy_from_slice(&frame);
            self.reads += 1;
            Ok(true)
        }
    }

    impl Source for VecFrameSource {
        type Output = Frame;

        fn pull(&mut self) -> Result<Option<Self::Output>> {
            Ok(self.frames.next())
        }
    }

    #[derive(Default)]
    struct VecFrameSink {
        frames: Vec<Frame>,
        finished: bool,
    }

    impl Sink<Frame> for VecFrameSink {
        fn push(&mut self, input: Frame) -> Result<()> {
            self.frames.push(input);
            Ok(())
        }

        fn finish(&mut self) -> Result<()> {
            self.finished = true;
            Ok(())
        }
    }

    #[derive(Default)]
    struct VecPacketSink {
        packets: Vec<Packet>,
        finished: bool,
    }

    impl Sink<Packet> for VecPacketSink {
        fn push(&mut self, input: Packet) -> Result<()> {
            self.packets.push(input);
            Ok(())
        }

        fn finish(&mut self) -> Result<()> {
            self.finished = true;
            Ok(())
        }
    }

    struct DuplicateFilter;

    impl Filter for DuplicateFilter {
        fn process(&mut self, frame: Frame) -> Result<Vec<Frame>> {
            Ok(vec![frame.clone(), frame])
        }
    }

    struct PassthroughFilter;

    impl Filter for PassthroughFilter {
        fn process(&mut self, frame: Frame) -> Result<Vec<Frame>> {
            Ok(vec![frame])
        }
    }

    struct FlushingFilter {
        info: FrameInfo,
    }

    impl Filter for FlushingFilter {
        fn process(&mut self, frame: Frame) -> Result<Vec<Frame>> {
            Ok(vec![frame])
        }

        fn finish(&mut self) -> Result<Vec<Frame>> {
            Ok(vec![Frame::blank(self.info)])
        }
    }

    struct PacketizingEncoder {
        next_pts: i64,
    }

    impl Encoder for PacketizingEncoder {
        fn encode(&mut self, frame: Frame) -> Result<Vec<Packet>> {
            let packet = Packet::new(
                StreamId(0),
                Some(crate::Timestamp(self.next_pts)),
                frame.into_data(),
            );
            self.next_pts += 1;
            Ok(vec![packet])
        }

        fn finish(&mut self) -> Result<Vec<Packet>> {
            Ok(vec![Packet::new(StreamId(0), None, b"eos".to_vec())])
        }
    }

    fn test_frame(fill: u8) -> Frame {
        let info = FrameInfo::new(2, 2, PixelFormat::Rgb24).unwrap();
        Frame::new(info, vec![fill; info.expected_len()]).unwrap()
    }

    #[test]
    fn filter_pipeline_runs_filters_and_flushes_in_order() {
        let info = FrameInfo::new(2, 2, PixelFormat::Rgb24).unwrap();
        let mut source = VecFrameSource::new(vec![test_frame(7)]);
        let mut duplicate = DuplicateFilter;
        let mut flush = FlushingFilter { info };
        let mut filters: Vec<&mut dyn Filter> = vec![&mut duplicate, &mut flush];
        let mut sink = VecFrameSink::default();

        let stats =
            run_frame_filter_pipeline(&mut source, filters.as_mut_slice(), &mut sink).unwrap();

        assert_eq!(
            stats,
            FilterPipelineStats {
                input_frames: 1,
                output_frames: 3,
            }
        );
        assert_eq!(sink.frames.len(), 3);
        assert!(sink.finished);
        assert_eq!(sink.frames[0].data(), &[7; 12]);
        assert_eq!(sink.frames[2].data(), &[0; 12]);
    }

    #[test]
    fn encode_pipeline_pushes_encoder_packets_and_finish_packet() {
        let mut source = VecFrameSource::new(vec![test_frame(1), test_frame(2)]);
        let mut passthrough = PassthroughFilter;
        let mut filters: Vec<&mut dyn Filter> = vec![&mut passthrough];
        let mut encoder = PacketizingEncoder { next_pts: 0 };
        let mut sink = VecPacketSink::default();

        let stats =
            run_frame_encode_pipeline(&mut source, filters.as_mut_slice(), &mut encoder, &mut sink)
                .unwrap();

        assert_eq!(
            stats,
            EncodePipelineStats {
                input_frames: 2,
                encoded_frames: 2,
                output_packets: 3,
            }
        );
        assert_eq!(sink.packets.len(), 3);
        assert!(sink.finished);
        assert_eq!(sink.packets[0].pts, Some(crate::Timestamp(0)));
        assert_eq!(sink.packets[2].data, b"eos");
    }

    #[test]
    fn passthrough_filter_preserves_frame() {
        let frame = test_frame(9);
        let mut filter = PassthroughFilter;
        let out = filter.process(frame.clone()).unwrap();
        assert_eq!(out, vec![frame]);
    }

    #[test]
    fn frame_source_raw_video_adapter_reads_one_frame_at_a_time() {
        let first = test_frame(3);
        let second = test_frame(4);
        let info = first.info();
        let mut source = FrameSourceRawVideoAdapter::new(
            VecFrameSource::new(vec![first.clone(), second.clone()]),
            info,
        );
        let mut output = vec![0; info.expected_len()];

        assert!(source.read_frame(&mut output).unwrap());
        assert_eq!(output, first.data());
        assert!(source.read_frame(&mut output).unwrap());
        assert_eq!(output, second.data());
        assert!(!source.read_frame(&mut output).unwrap());
    }

    #[test]
    fn filtered_raw_video_source_pulls_input_on_demand() {
        let info = FrameInfo::new(2, 2, PixelFormat::Rgb24).unwrap();
        let first = vec![5; info.expected_len()];
        let second = vec![6; info.expected_len()];
        let source = VecRawFrameSource::new(info, vec![first.clone(), second.clone()]);
        let filters: Vec<Box<dyn Filter>> = vec![Box::new(PassthroughFilter)];
        let mut filtered = FilteredRawVideoFrameSource::new(source, info, info, filters);
        let mut output = vec![0; info.expected_len()];

        assert!(filtered.read_frame(&mut output).unwrap());
        assert_eq!(output, first);
        let source = filtered.into_inner();
        assert_eq!(source.reads, 1);
    }

    #[test]
    fn source_errors_stop_pipeline_before_sink_finish() {
        struct FailingSource;

        impl Source for FailingSource {
            type Output = Frame;

            fn pull(&mut self) -> Result<Option<Self::Output>> {
                Err(MediaError::Message("source failed".to_string()))
            }
        }

        let mut source = FailingSource;
        let mut sink = VecFrameSink::default();
        let err = run_frame_filter_pipeline(&mut source, &mut [], &mut sink).unwrap_err();

        assert_eq!(err.to_string(), "source failed");
        assert!(!sink.finished);
    }
}
