use anyhow::{Context, Result};
use image::DynamicImage;
use log::{debug, error, info, warn};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::timer::{ScanStats, ScanTiming, Timer};

/// Scan result for a single file
#[derive(Debug)]
pub struct ScanResult {
    /// File path
    pub file_path: PathBuf,
    /// Detected QR code data
    pub qr_codes: Vec<String>,
    /// Timing information
    pub timing: ScanTiming,
    /// Success status
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

impl ScanResult {
    pub fn qr_count(&self) -> usize {
        self.qr_codes.len()
    }
}

/// QR code scanner
pub struct QrScanner {
    /// Statistics
    stats: ScanStats,
}

impl QrScanner {
    pub fn new(_verbose: bool) -> Self {
        Self {
            stats: ScanStats::new(),
        }
    }

    /// Scan a single file
    pub fn scan_file(&mut self, path: &Path) -> Result<ScanResult> {
        let total_timer = Timer::start();
        let mut timing = ScanTiming::new();

        debug!("Scanning file: {}", path.display());

        // Read and decode image
        let file_data =
            fs::read(path).with_context(|| format!("Failed to read file: {}", path.display()))?;
        let img = image::load_from_memory(&file_data)
            .with_context(|| format!("Failed to decode image: {}", path.display()))?;

        // QR detection with detailed timing
        let qr_codes = self.detect_qr_codes(&img, &mut timing.qr_detection)?;

        timing.total = total_timer.elapsed();

        // Update statistics
        self.stats.total_files += 1;
        self.stats.successful_scans += 1;
        self.stats.total_duration += timing.total;

        if !qr_codes.is_empty() {
            self.stats.files_with_qr += 1;
            self.stats.total_qr_codes += qr_codes.len();
        }

        Ok(ScanResult {
            file_path: path.to_path_buf(),
            qr_codes,
            timing,
            success: true,
            error: None,
        })
    }

    /// Detect QR codes from image with detailed timing
    fn detect_qr_codes(
        &self,
        img: &DynamicImage,
        timing: &mut crate::timer::QrDetectionTiming,
    ) -> Result<Vec<String>> {
        let total_timer = Timer::start();

        // Step 1: Convert to grayscale
        let gray_timer = Timer::start();
        let gray_img = img.to_luma8();
        timing.to_grayscale = gray_timer.elapsed();
        debug!(
            "Grayscale conversion completed in {:.2}ms",
            timing.to_ms(timing.to_grayscale)
        );

        // Step 2: Prepare image for detection
        let prepare_timer = Timer::start();
        let mut img_data = rqrr::PreparedImage::prepare(gray_img);
        timing.prepare_image = prepare_timer.elapsed();
        debug!(
            "Image preparation completed in {:.2}ms",
            timing.to_ms(timing.prepare_image)
        );

        // Step 3: Detect grids
        let detect_timer = Timer::start();
        let grids = img_data.detect_grids();
        timing.detect_grids = detect_timer.elapsed();
        debug!(
            "Grid detection completed, found {} grids in {:.2}ms",
            grids.len(),
            timing.to_ms(timing.detect_grids)
        );

        // Step 4: Decode QR codes
        let decode_timer = Timer::start();
        let mut results = Vec::new();
        for grid in grids {
            match grid.decode() {
                Ok((_meta, content)) => {
                    debug!("Successfully decoded QR code: {}", content);
                    results.push(content);
                }
                Err(e) => {
                    warn!("Failed to decode QR code: {:?}", e);
                }
            }
        }
        timing.decode_qr = decode_timer.elapsed();
        debug!(
            "QR decoding completed, {} successful in {:.2}ms",
            results.len(),
            timing.to_ms(timing.decode_qr)
        );

        timing.total = total_timer.elapsed();

        Ok(results)
    }

    /// Scan all image files in a directory
    pub fn scan_directory(&mut self, dir: &Path) -> Result<Vec<ScanResult>> {
        info!("Starting directory scan: {}", dir.display());

        let dir_timer = Timer::start();
        let mut results = Vec::new();

        // Supported image extensions
        let image_extensions = ["png", "jpg", "jpeg", "bmp", "gif", "webp", "tiff", "tif"];

        for entry in WalkDir::new(dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // Check if it's an image file
            if !path.is_file() {
                continue;
            }

            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if !image_extensions.contains(&ext_str.as_str()) {
                    continue;
                }
            } else {
                continue;
            }

            // Scan file
            match self.scan_file(path) {
                Ok(result) => {
                    results.push(result);
                }
                Err(e) => {
                    error!("Failed to scan file {}: {}", path.display(), e);
                    self.stats.total_files += 1;
                    self.stats.failed_scans += 1;

                    results.push(ScanResult {
                        file_path: path.to_path_buf(),
                        qr_codes: Vec::new(),
                        timing: ScanTiming::new(),
                        success: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        let dir_elapsed = dir_timer.elapsed();
        info!(
            "Directory scan completed in {:.2}ms",
            dir_elapsed.as_secs_f64() * 1000.0
        );

        Ok(results)
    }

    /// Get statistics
    pub fn stats(&mut self) -> &ScanStats {
        self.stats.finalize();
        &self.stats
    }
}
