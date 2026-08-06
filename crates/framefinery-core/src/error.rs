use std::error::Error;
use std::fmt;

pub type Result<T> = std::result::Result<T, MediaError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaError {
    InvalidDimensions {
        width: usize,
        height: usize,
    },
    IncompatibleFormat {
        format: String,
        reason: String,
    },
    UnsupportedCodec {
        codec: String,
        reason: String,
    },
    UnsupportedPixelFormat {
        codec: String,
        format: String,
    },
    Unsupported {
        feature: String,
        reason: String,
    },
    LengthOverflow,
    BufferLength {
        expected: usize,
        actual: usize,
    },
    ShortFrameRead {
        expected: usize,
        actual: usize,
    },
    UnknownSetting {
        codec: String,
        setting: String,
    },
    DuplicateSetting {
        setting: String,
    },
    InvalidSettingValue {
        codec: String,
        setting: String,
        expected: String,
        actual: String,
    },
    ConflictingSettings {
        setting: String,
        conflict: String,
    },
    EncodeAfterFlush,
    FrameLimitExceeded {
        limit: usize,
    },
    Message(String),
}

impl fmt::Display for MediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions { width, height } => {
                write!(f, "invalid frame dimensions {width}x{height}")
            }
            Self::IncompatibleFormat { format, reason } => {
                write!(f, "incompatible {format} frame: {reason}")
            }
            Self::UnsupportedCodec { codec, reason } => {
                write!(f, "unsupported codec '{codec}': {reason}")
            }
            Self::UnsupportedPixelFormat { codec, format } => {
                write!(f, "codec '{codec}' does not support {format} input")
            }
            Self::Unsupported { feature, reason } => {
                write!(f, "unsupported {feature}: {reason}")
            }
            Self::LengthOverflow => f.write_str("frame length overflowed addressable memory"),
            Self::BufferLength { expected, actual } => {
                write!(
                    f,
                    "buffer length mismatch: expected {expected} byte(s), got {actual}"
                )
            }
            Self::ShortFrameRead { expected, actual } => {
                write!(
                    f,
                    "short frame read: expected {expected} byte(s), got {actual}"
                )
            }
            Self::UnknownSetting { codec, setting } => {
                write!(f, "unknown setting '{setting}' for codec '{codec}'")
            }
            Self::DuplicateSetting { setting } => {
                write!(f, "duplicate encoder setting '{setting}'")
            }
            Self::InvalidSettingValue {
                codec,
                setting,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "codec '{codec}' setting '{setting}' expects {expected}, got '{actual}'"
                )
            }
            Self::ConflictingSettings { setting, conflict } => {
                write!(f, "encoder setting '{setting}' conflicts with {conflict}")
            }
            Self::EncodeAfterFlush => f.write_str("cannot encode a frame after encoder flush"),
            Self::FrameLimitExceeded { limit } => {
                write!(f, "encoder frame limit {limit} was exceeded")
            }
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl Error for MediaError {}
