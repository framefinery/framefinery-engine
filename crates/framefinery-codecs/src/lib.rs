//! Experimental codec models imported from the FrameFinery hardware workspace.
//!
//! The modules in this crate are software-only codec models. They share public
//! frame/pixel primitives with `framefinery-core`, but intentionally keep AV2
//! and VVC internals separate while the APIs settle.

#[cfg(feature = "av2")]
pub mod av2;
pub mod bitstream;
pub mod instrumentation;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod picture;
#[cfg(any(feature = "av2", feature = "vvc"))]
mod settings;
pub mod trace;
#[cfg(feature = "vvc")]
pub mod vvc;

use framefinery_core::CodecManifest;

pub use framefinery_core::{ChromaSampling, PixelFormat, SampleBitDepth};

pub const CODECS: &[CodecManifest] = &[
    #[cfg(feature = "av2")]
    av2::AV2_CODEC,
    #[cfg(feature = "vvc")]
    vvc::VVC_CODEC,
];

pub fn codec(name: &str) -> Option<CodecManifest> {
    CODECS.iter().copied().find(|codec| codec.name == name)
}
