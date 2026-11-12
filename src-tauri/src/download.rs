use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

const RUNNING: u8 = 0;
const PAUSED: u8 = 1;
const CANCELLED: u8 = 2;

/// Управление одной фоновой загрузкой: пауза, продолжение, отмена.
#[derive(Debug)]
pub struct DownloadControl {
    state: AtomicU8,
}

impl DownloadControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: AtomicU8::new(RUNNING),
        })
    }

    pub fn pause(&self) {
        let _ = self
            .state
            .compare_exchange(RUNNING, PAUSED, Ordering::Relaxed, Ordering::Relaxed);
    }

    pub fn resume(&self) {
        let _ = self
            .state
            .compare_exchange(PAUSED, RUNNING, Ordering::Relaxed, Ordering::Relaxed);
    }

    pub fn cancel(&self) {
        self.state.store(CANCELLED, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::Relaxed) == CANCELLED
    }

    pub fn is_paused(&self) -> bool {
        self.state.load(Ordering::Relaxed) == PAUSED
    }
}

#[cfg(test)]
mod tests {
    use super::DownloadControl;

    #[test]
    fn pause_resume_cancel() {
        let control = DownloadControl::new();
        assert!(!control.is_paused());
        assert!(!control.is_cancelled());

        control.pause();
        assert!(control.is_paused());
        assert!(!control.is_cancelled());

        control.resume();
        assert!(!control.is_paused());
        assert!(!control.is_cancelled());

        control.cancel();
        assert!(control.is_cancelled());
        control.resume();
        assert!(control.is_cancelled());
        assert!(!control.is_paused());
    }
}
