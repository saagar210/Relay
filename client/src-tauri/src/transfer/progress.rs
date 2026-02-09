use std::collections::VecDeque;
use std::time::Instant;

use serde::Serialize;

/// Tracks transfer progress, calculates speed and ETA.
pub struct ProgressTracker {
    start_time: Instant,
    bytes_transferred: u64,
    bytes_total: u64,
    /// Sliding window of (timestamp, cumulative_bytes) for speed smoothing.
    speed_samples: VecDeque<(Instant, u64)>,
    /// Max age of samples in the window (3 seconds).
    window_secs: f64,
}

impl ProgressTracker {
    pub fn new(bytes_total: u64) -> Self {
        let now = Instant::now();
        let mut samples = VecDeque::with_capacity(64);
        samples.push_back((now, 0));

        Self {
            start_time: now,
            bytes_transferred: 0,
            bytes_total,
            speed_samples: samples,
            window_secs: 3.0,
        }
    }

    /// Record that `bytes` more bytes have been transferred.
    pub fn update(&mut self, bytes: u64) {
        self.bytes_transferred += bytes;
        let now = Instant::now();
        self.speed_samples.push_back((now, self.bytes_transferred));

        // Evict old samples outside the window
        let cutoff = now - std::time::Duration::from_secs_f64(self.window_secs);
        while self.speed_samples.len() > 2 {
            if self.speed_samples[0].0 < cutoff {
                self.speed_samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Current transfer speed in bytes per second (moving average).
    pub fn speed_bps(&self) -> u64 {
        if self.speed_samples.len() < 2 {
            return 0;
        }
        let oldest = self.speed_samples.front().unwrap();
        let newest = self.speed_samples.back().unwrap();
        let elapsed = newest.0.duration_since(oldest.0).as_secs_f64();
        if elapsed < 0.01 {
            return 0;
        }
        let bytes_diff = newest.1.saturating_sub(oldest.1);
        (bytes_diff as f64 / elapsed) as u64
    }

    /// Estimated seconds remaining.
    pub fn eta_seconds(&self) -> u32 {
        let speed = self.speed_bps();
        if speed == 0 {
            return 0;
        }
        let remaining = self.bytes_total.saturating_sub(self.bytes_transferred);
        (remaining / speed) as u32
    }

    /// Completion percentage (0.0 to 100.0).
    pub fn percent(&self) -> f32 {
        if self.bytes_total == 0 {
            return 100.0;
        }
        (self.bytes_transferred as f64 / self.bytes_total as f64 * 100.0) as f32
    }

    pub fn bytes_transferred(&self) -> u64 {
        self.bytes_transferred
    }

    pub fn bytes_total(&self) -> u64 {
        self.bytes_total
    }

    pub fn elapsed_seconds(&self) -> u32 {
        self.start_time.elapsed().as_secs() as u32
    }

    pub fn average_speed(&self) -> u64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed < 0.01 {
            return 0;
        }
        (self.bytes_transferred as f64 / elapsed) as u64
    }
}

/// Events emitted to the frontend via Tauri events.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ProgressEvent {
    StateChanged {
        state: String,
    },
    TransferProgress {
        bytes_transferred: u64,
        bytes_total: u64,
        speed_bps: u64,
        eta_seconds: u32,
        current_file: String,
        percent: f32,
    },
    FileCompleted {
        name: String,
    },
    TransferComplete {
        duration_seconds: u32,
        average_speed: u64,
        total_bytes: u64,
        file_count: u32,
    },
    Error {
        message: String,
    },
    FileOffer {
        session_id: String,
        files: Vec<FileOfferInfo>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct FileOfferInfo {
    pub name: String,
    pub size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_progress_zero_total() {
        let tracker = ProgressTracker::new(0);
        assert_eq!(tracker.percent(), 100.0);
    }

    #[test]
    fn test_progress_update() {
        let mut tracker = ProgressTracker::new(1000);
        tracker.update(500);
        assert_eq!(tracker.bytes_transferred(), 500);
        assert!((tracker.percent() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_speed_calculation() {
        let mut tracker = ProgressTracker::new(10_000_000);
        // Simulate transfer over time
        sleep(Duration::from_millis(100));
        tracker.update(1_000_000);
        let speed = tracker.speed_bps();
        // Should be roughly 10 MB/s (1MB in 0.1s) — allow wide tolerance
        assert!(speed > 1_000_000, "speed should be > 1 MB/s, got {speed}");
    }
}
