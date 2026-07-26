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
pub mod trace;
#[cfg(feature = "vvc")]
pub mod vvc;

pub use framefinery_core::{ChromaSampling, PixelFormat, SampleBitDepth};
