/// Identifier for a logical stream within a media pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamId(pub u32);

/// Integer timestamp used by packet and encoded-chunk metadata.
///
/// The time base is owned by the caller or future muxing layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub i64);

/// Encoded packet passed between generic pipeline stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    /// Stream this packet belongs to.
    pub stream_id: StreamId,
    /// Optional presentation timestamp.
    pub pts: Option<Timestamp>,
    /// Packet payload bytes.
    pub data: Vec<u8>,
}

impl Packet {
    /// Create a packet with stream id, optional timestamp, and payload bytes.
    pub fn new(stream_id: StreamId, pts: Option<Timestamp>, data: Vec<u8>) -> Self {
        Self {
            stream_id,
            pts,
            data,
        }
    }
}
