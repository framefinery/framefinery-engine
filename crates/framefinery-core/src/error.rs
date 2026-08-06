use std::error::Error;
use std::fmt;

/// Result type used by FrameFinery core APIs.
pub type Result<T> = std::result::Result<T, MediaError>;

/// Error type shared by FrameFinery media primitives and public codec APIs.
///
/// Stable API boundaries should prefer structured variants so callers can
/// match failures without parsing display strings. Experimental codec internals
/// may still use [`MediaError::Message`] until their failures become part of
/// the public contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaError {
    /// A frame or stream geometry has a zero or otherwise invalid dimension.
    InvalidDimensions {
        /// Invalid width in pixels.
        width: usize,
        /// Invalid height in pixels.
        height: usize,
    },
    /// A valid format was used in a context that requires different metadata.
    IncompatibleFormat {
        /// Format name or family involved in the failure.
        format: String,
        /// Human-readable reason for the incompatibility.
        reason: String,
    },
    /// The requested codec is missing or mismatched with a selected manifest.
    UnsupportedCodec {
        /// Requested codec id.
        codec: String,
        /// Reason the codec cannot be used.
        reason: String,
    },
    /// The selected codec does not accept the requested input pixel format.
    UnsupportedPixelFormat {
        /// Codec id that rejected the format.
        codec: String,
        /// Input format description.
        format: String,
    },
    /// A named feature or mode is not currently implemented.
    Unsupported {
        /// Feature or subsystem that rejected the request.
        feature: String,
        /// Reason the feature cannot be used.
        reason: String,
    },
    /// A frame, packet, or stream length overflowed addressable memory.
    LengthOverflow,
    /// A buffer did not match the byte length implied by its metadata.
    BufferLength {
        /// Expected byte length.
        expected: usize,
        /// Actual byte length.
        actual: usize,
    },
    /// A raw frame source ended after reading only part of one frame.
    ShortFrameRead {
        /// Full frame byte length requested by the caller.
        expected: usize,
        /// Number of bytes read before EOF.
        actual: usize,
    },
    /// A codec setting name is not declared by the selected codec manifest.
    UnknownSetting {
        /// Codec id whose setting table was checked.
        codec: String,
        /// Unknown setting name.
        setting: String,
    },
    /// The same encoder setting was provided more than once.
    DuplicateSetting {
        /// Duplicate setting name.
        setting: String,
    },
    /// A setting value has the wrong type or is outside the declared range.
    InvalidSettingValue {
        /// Codec id whose setting table was checked.
        codec: String,
        /// Setting name that rejected the value.
        setting: String,
        /// Expected value form, such as `true|false` or `1..255`.
        expected: String,
        /// Actual value supplied by the caller.
        actual: String,
    },
    /// Two configuration paths requested mutually exclusive settings.
    ConflictingSettings {
        /// Setting name that conflicts with another configuration field.
        setting: String,
        /// Description of the conflicting option or mode.
        conflict: String,
    },
    /// A caller attempted to encode another frame after flushing a session.
    EncodeAfterFlush,
    /// A buffered session received more frames than its configured limit.
    FrameLimitExceeded {
        /// Configured frame limit that was exceeded.
        limit: usize,
    },
    /// Human-readable error used for failures that are not structured yet.
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
