use anyhow::{Context, Result};
use bardecoder::default_decoder;
use image::{DynamicImage, GrayImage};
use log::{debug, error, info};
use rxing::{
    BinaryBitmap, DecodeHints, Exceptions, Luma8LuminanceSource, Reader, common::HybridBinarizer,
    qrcode::QRCodeReader,
};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use zbar_pack::{Image as ZBarPackImage, ImageScanner as ZBarPackScanner};

use crate::preprocessor::ImagePreprocessor;
use crate::timer::{ScanStats, ScanTiming, Timer};

/// Results from individual detection engine
#[derive(Debug, Clone)]
pub struct EngineResult {
    pub engine_name: String,
    pub qr_codes: Vec<String>,
    pub duration_ms: f64, // Time spent by this engine alone
}

/// Scan result for a single file
#[derive(Debug)]
pub struct ScanResult {
    /// File path
    pub file_path: PathBuf,
    /// Detected QR code data (deduplicated across all engines)
    pub qr_codes: Vec<String>,
    /// Results from each detection engine
    pub engine_results: Vec<EngineResult>,
    /// Timing information
    pub timing: ScanTiming,
    /// Success status
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
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
        let (qr_codes, engine_results) = self.detect_qr_codes(&img, &mut timing.qr_detection)?;

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
            engine_results,
            timing,
            success: true,
            error: None,
        })
    }

    /// Detect QR codes from image with detailed timing (multi-engine approach)
    /// Returns (all_qr_codes, engine_results)
    fn detect_qr_codes(
        &self,
        img: &DynamicImage,
        timing: &mut crate::timer::QrDetectionTiming,
    ) -> Result<(Vec<String>, Vec<EngineResult>)> {
        let total_timer = Timer::start();

        // Step 0: Resize large images for better performance and detection
        let gray_timer = Timer::start();
        let width = img.width();
        let height = img.height();
        let max_dimension = width.max(height);

        let working_img = if max_dimension > 2000 {
            let scale = 2000.0 / max_dimension as f32;
            debug!(
                "Resizing large image ({}x{}) by {:.2}x",
                width, height, scale
            );
            ImagePreprocessor::resize(img, scale)
        } else {
            img.clone()
        };

        // Step 1: Convert to grayscale and preprocess
        let variants = ImagePreprocessor::generate_variants(&working_img);
        timing.to_grayscale = gray_timer.elapsed();
        debug!(
            "Image preprocessing completed, generated {} variants in {:.2}ms",
            variants.len(),
            timing.to_ms(timing.to_grayscale)
        );

        let mut all_results = std::collections::HashSet::new();
        let mut engine_results = Vec::new();

        // Step 2: Try rqrr (fast)
        let prepare_timer = Timer::start();
        let rqrr_timer = Timer::start();
        let mut rqrr_codes = std::collections::HashSet::new();
        for (variant_name, gray_img) in &variants {
            debug!("Trying rqrr with variant: {}", variant_name);
            match self.detect_with_rqrr(gray_img) {
                Ok(codes) if !codes.is_empty() => {
                    debug!(
                        "rqrr found {} codes with variant: {}",
                        codes.len(),
                        variant_name
                    );
                    rqrr_codes.extend(codes);
                }
                _ => {}
            }
        }
        let rqrr_duration = rqrr_timer.elapsed();
        all_results.extend(rqrr_codes.iter().cloned());
        engine_results.push(EngineResult {
            engine_name: "rqrr".to_string(),
            qr_codes: rqrr_codes.into_iter().collect(),
            duration_ms: timing.to_ms(rqrr_duration),
        });
        timing.prepare_image = prepare_timer.elapsed();
        timing.detect_grids = prepare_timer.elapsed();

        // Step 3: Try rxing (more robust)
        let decode_timer = Timer::start();
        let rxing_timer = Timer::start();
        let mut rxing_codes = std::collections::HashSet::new();
        debug!("Trying rxing for more robust detection");
        for (variant_name, gray_img) in &variants {
            debug!("Trying rxing with variant: {}", variant_name);
            match self.detect_with_rxing(gray_img, &working_img) {
                Ok(codes) if !codes.is_empty() => {
                    debug!(
                        "rxing found {} codes with variant: {}",
                        codes.len(),
                        variant_name
                    );
                    rxing_codes.extend(codes);
                }
                _ => {}
            }
        }
        let rxing_duration = rxing_timer.elapsed();
        all_results.extend(rxing_codes.iter().cloned());
        engine_results.push(EngineResult {
            engine_name: "rxing".to_string(),
            qr_codes: rxing_codes.into_iter().collect(),
            duration_ms: timing.to_ms(rxing_duration),
        });

        // Step 4: Try quircs (pure Rust library)
        let quircs_timer = Timer::start();
        let mut quircs_codes = std::collections::HashSet::new();
        debug!("Trying quircs for detection");
        for (variant_name, gray_img) in &variants {
            debug!("Trying quircs with variant: {}", variant_name);
            match self.detect_with_quircs(gray_img) {
                Ok(codes) if !codes.is_empty() => {
                    debug!(
                        "quircs found {} codes with variant: {}",
                        codes.len(),
                        variant_name
                    );
                    quircs_codes.extend(codes);
                }
                Err(e) => {
                    debug!("quircs failed: {:?}", e);
                }
                _ => {}
            }
        }
        let quircs_duration = quircs_timer.elapsed();
        all_results.extend(quircs_codes.iter().cloned());
        engine_results.push(EngineResult {
            engine_name: "quircs".to_string(),
            qr_codes: quircs_codes.into_iter().collect(),
            duration_ms: timing.to_ms(quircs_duration),
        });

        // Step 5: Try bardecoder (image-based decoder)
        let bardecoder_timer = Timer::start();
        let mut bardecoder_codes = std::collections::HashSet::new();
        debug!("Trying bardecoder for detection");
        for (variant_name, gray_img) in &variants {
            debug!("Trying bardecoder with variant: {}", variant_name);
            match self.detect_with_bardecoder(gray_img) {
                Ok(codes) if !codes.is_empty() => {
                    debug!(
                        "bardecoder found {} codes with variant: {}",
                        codes.len(),
                        variant_name
                    );
                    bardecoder_codes.extend(codes);
                }
                Err(e) => {
                    debug!("bardecoder failed: {:?}", e);
                }
                _ => {}
            }
        }
        let bardecoder_duration = bardecoder_timer.elapsed();
        all_results.extend(bardecoder_codes.iter().cloned());
        engine_results.push(EngineResult {
            engine_name: "bardecoder".to_string(),
            qr_codes: bardecoder_codes.into_iter().collect(),
            duration_ms: timing.to_ms(bardecoder_duration),
        });

        // Step 6: Try zbar-pack (safe vendored ZBar bindings)
        let zbar_pack_timer = Timer::start();
        let mut zbar_pack_codes = std::collections::HashSet::new();
        debug!("Trying zbar-pack for detection");
        for (variant_name, gray_img) in &variants {
            debug!("Trying zbar-pack with variant: {}", variant_name);
            match self.detect_with_zbar_pack(gray_img) {
                Ok(codes) if !codes.is_empty() => {
                    debug!(
                        "zbar-pack found {} codes with variant: {}",
                        codes.len(),
                        variant_name
                    );
                    zbar_pack_codes.extend(codes);
                }
                Err(e) => {
                    debug!("zbar-pack failed: {:?}", e);
                }
                _ => {}
            }
        }
        let zbar_pack_duration = zbar_pack_timer.elapsed();
        all_results.extend(zbar_pack_codes.iter().cloned());
        engine_results.push(EngineResult {
            engine_name: "zbar-pack".to_string(),
            qr_codes: zbar_pack_codes.into_iter().collect(),
            duration_ms: timing.to_ms(zbar_pack_duration),
        });

        timing.decode_qr = decode_timer.elapsed();
        timing.total = total_timer.elapsed();

        let results: Vec<String> = all_results.into_iter().collect();
        debug!(
            "Total QR codes found: {} in {:.2}ms",
            results.len(),
            timing.to_ms(timing.total)
        );

        Ok((results, engine_results))
    }

    /// Detect QR codes using rqrr (fast, good for standard QR codes)
    fn detect_with_rqrr(&self, gray_img: &GrayImage) -> Result<Vec<String>> {
        let mut img_data = rqrr::PreparedImage::prepare(gray_img.clone());
        let grids = img_data.detect_grids();

        debug!("rqrr detected {} grids", grids.len());

        let mut results = Vec::new();
        for (i, grid) in grids.iter().enumerate() {
            match grid.decode() {
                Ok((_meta, content)) => {
                    debug!("rqrr grid {} decoded successfully", i);
                    results.push(content);
                }
                Err(e) => {
                    debug!("rqrr grid {} decode failed: {:?}", i, e);
                }
            }
        }

        if !results.is_empty() {
            debug!(
                "rqrr successfully decoded {}/{} grids",
                results.len(),
                grids.len()
            );
        }

        Ok(results)
    }

    /// Detect QR codes using rxing (robust, handles deformed/multiple QR codes)
    fn detect_with_rxing(&self, gray_img: &GrayImage, _img: &DynamicImage) -> Result<Vec<String>> {
        let width = gray_img.width();
        let height = gray_img.height();

        // Convert to rxing format
        let luminance_source = Luma8LuminanceSource::new(gray_img.as_raw().clone(), width, height);

        let mut bitmap = BinaryBitmap::new(HybridBinarizer::new(luminance_source));

        // Configure hints
        let hints = DecodeHints::default();

        let mut reader = QRCodeReader::new();

        let mut results = Vec::new();

        // Try to decode
        match reader.decode_with_hints(&mut bitmap, &hints) {
            Ok(result) => {
                results.push(result.getText().to_string());
            }
            Err(e) => {
                if !matches!(e, Exceptions::NotFoundException(_)) {
                    debug!("rxing decode failed: {:?}", e);
                }
            }
        }

        Ok(results)
    }

    /// Detect QR codes using quircs (pure Rust, based on quirc library)
    fn detect_with_quircs(&self, gray_img: &GrayImage) -> Result<Vec<String>> {
        let width = gray_img.width() as usize;
        let height = gray_img.height() as usize;

        // Create quircs decoder
        let mut decoder = quircs::Quirc::new();

        // Identify QR codes in the image
        let codes = decoder.identify(width, height, gray_img.as_raw());

        let mut results = Vec::new();
        let mut count = 0;

        for code in codes {
            count += 1;
            match code {
                Ok(code) => match code.decode() {
                    Ok(decoded) => {
                        if let Ok(text) = std::str::from_utf8(&decoded.payload) {
                            debug!("quircs decoded QR code successfully");
                            results.push(text.to_string());
                        }
                    }
                    Err(e) => {
                        debug!("quircs decode failed: {:?}", e);
                    }
                },
                Err(e) => {
                    debug!("quircs extract failed: {:?}", e);
                }
            }
        }

        if count > 0 {
            debug!("quircs identified {} codes", count);
        }

        Ok(results)
    }

    /// Detect QR codes using bardecoder (image-based decoder)
    fn detect_with_bardecoder(&self, gray_img: &GrayImage) -> Result<Vec<String>> {
        // bardecoder uses image 0.24, we use 0.25
        // Convert via raw pixels to avoid slow PNG encode/decode
        let width = gray_img.width();
        let height = gray_img.height();
        let pixels = gray_img.as_raw().clone();

        // Create image 0.24 GrayImage from raw pixels
        let gray_v24 = image_v24::GrayImage::from_raw(width, height, pixels)
            .context("Failed to create image_v24::GrayImage")?;

        // Convert to DynamicImage for bardecoder
        let img_v24 = image_v24::DynamicImage::ImageLuma8(gray_v24);

        let decoder = default_decoder();
        let decoded_results = decoder.decode(&img_v24);

        debug!("bardecoder found {} results", decoded_results.len());

        let mut results = Vec::new();
        for result in decoded_results {
            match result {
                Ok(text) => {
                    debug!("bardecoder decoded QR code successfully");
                    results.push(text);
                }
                Err(e) => {
                    debug!("bardecoder decode failed: {:?}", e);
                }
            }
        }

        Ok(results)
    }

    /// Detect QR codes using zbar-pack (safe vendored ZBar bindings)
    fn detect_with_zbar_pack(&self, gray_img: &GrayImage) -> Result<Vec<String>> {
        let width = gray_img.width();
        let height = gray_img.height();

        // Create zbar-pack image and scanner
        let image = ZBarPackImage::from_gray(gray_img.as_raw(), width, height)
            .map_err(|e| anyhow::anyhow!("zbar-pack image creation failed: {:?}", e))?;

        let mut scanner = ZBarPackScanner::new()
            .map_err(|e| anyhow::anyhow!("zbar-pack scanner creation failed: {:?}", e))?;

        // Scan for barcodes
        let symbols = scanner
            .scan_image(&image)
            .map_err(|e| anyhow::anyhow!("zbar-pack scan failed: {:?}", e))?;

        let mut results = Vec::new();
        for symbol in symbols {
            // Only include QR codes
            if symbol.symbol_type() == zbar_pack::SymbolType::QRCODE {
                debug!("zbar-pack decoded QR code successfully");
                results.push(symbol.data().to_string());
            }
        }

        debug!("zbar-pack found {} QR codes", results.len());
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
                        engine_results: Vec::new(),
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
