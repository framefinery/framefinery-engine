use std::time::Duration;

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct StageStart(std::time::Instant);

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StageStart;

impl StageStart {
    pub(crate) fn now() -> Self {
        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        {
            Self(std::time::Instant::now())
        }
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        {
            Self
        }
    }

    pub(crate) fn elapsed(self) -> Duration {
        #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
        {
            self.0.elapsed()
        }
        #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
        {
            Duration::ZERO
        }
    }

    pub(crate) fn elapsed_nanos(self) -> u64 {
        let nanos = self.elapsed().as_nanos();
        nanos.min(u128::from(u64::MAX)) as u64
    }
}
