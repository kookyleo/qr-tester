use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Detailed timing for QR detection stages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QrDetectionTiming {
    /// Time spent converting to grayscale
    pub to_grayscale: Duration,
    /// Time spent preparing image for detection
    pub prepare_image: Duration,
    /// Time spent detecting QR code grids
    pub detect_grids: Duration,
    /// Time spent decoding QR codes
    pub decode_qr: Duration,
    /// Total detection time
    pub total: Duration,
}

impl QrDetectionTiming {
    pub fn new() -> Self {
        Self {
            to_grayscale: Duration::ZERO,
            prepare_image: Duration::ZERO,
            detect_grids: Duration::ZERO,
            decode_qr: Duration::ZERO,
            total: Duration::ZERO,
        }
    }

    /// Convert duration to milliseconds
    pub fn to_ms(&self, duration: Duration) -> f64 {
        duration.as_secs_f64() * 1000.0
    }
}

/// Timing information for scanning a single file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanTiming {
    /// Detailed QR detection timing
    pub qr_detection: QrDetectionTiming,
    /// Total processing time
    pub total: Duration,
}

impl ScanTiming {
    pub fn new() -> Self {
        Self {
            qr_detection: QrDetectionTiming::new(),
            total: Duration::ZERO,
        }
    }
}

/// Timer for measuring execution time
pub struct Timer {
    start: Instant,
}

impl Timer {
    pub fn start() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Overall scan statistics
#[derive(Debug, Serialize, Deserialize)]
pub struct ScanStats {
    /// Total number of files
    pub total_files: usize,
    /// Number of successfully scanned files
    pub successful_scans: usize,
    /// Number of failed scans
    pub failed_scans: usize,
    /// Number of files containing QR codes
    pub files_with_qr: usize,
    /// Total number of QR codes found
    pub total_qr_codes: usize,
    /// Total time spent scanning
    pub total_duration: Duration,
    /// Average time per file
    pub avg_duration_per_file: Duration,
}

impl ScanStats {
    pub fn new() -> Self {
        Self {
            total_files: 0,
            successful_scans: 0,
            failed_scans: 0,
            files_with_qr: 0,
            total_qr_codes: 0,
            total_duration: Duration::ZERO,
            avg_duration_per_file: Duration::ZERO,
        }
    }

    pub fn finalize(&mut self) {
        if self.total_files > 0 {
            self.avg_duration_per_file = self.total_duration / self.total_files as u32;
        }
    }
}
