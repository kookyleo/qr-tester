use image::{DynamicImage, GrayImage, ImageBuffer, Luma};
use imageproc::contrast::{
    ThresholdType, adaptive_threshold, otsu_level, stretch_contrast, threshold,
};
use log::debug;

/// Preprocess image for better QR code detection
pub struct ImagePreprocessor;

impl ImagePreprocessor {
    /// Apply Otsu's binarization
    pub fn otsu_binarization(gray: &GrayImage) -> GrayImage {
        let level = otsu_level(gray);
        debug!("Otsu threshold level: {}", level);
        threshold(gray, level, ThresholdType::Binary)
    }

    /// Apply adaptive thresholding with block radius
    pub fn adaptive_threshold_image(gray: &GrayImage, block_radius: u32) -> GrayImage {
        adaptive_threshold(gray, block_radius)
    }

    /// Stretch contrast to full range
    pub fn enhance_contrast(gray: &GrayImage) -> GrayImage {
        // Find min and max pixel values
        let (min, max) = gray.pixels().fold((255u8, 0u8), |(min, max), pixel| {
            let val = pixel[0];
            (min.min(val), max.max(val))
        });

        debug!("Contrast stretch: min={}, max={}", min, max);

        if min < max {
            stretch_contrast(gray, min, max, 0, 255)
        } else {
            gray.clone()
        }
    }

    /// Invert image colors (for white QR on black background)
    pub fn invert(gray: &GrayImage) -> GrayImage {
        ImageBuffer::from_fn(gray.width(), gray.height(), |x, y| {
            let pixel = gray.get_pixel(x, y);
            Luma([255 - pixel[0]])
        })
    }

    /// Resize image for better detection
    pub fn resize(img: &DynamicImage, scale: f32) -> DynamicImage {
        let new_width = (img.width() as f32 * scale) as u32;
        let new_height = (img.height() as f32 * scale) as u32;

        if new_width > 0 && new_height > 0 {
            img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3)
        } else {
            img.clone()
        }
    }

    /// Generate multiple preprocessed versions of an image
    pub fn generate_variants(img: &DynamicImage) -> Vec<(String, GrayImage)> {
        let mut variants = Vec::new();
        let gray = img.to_luma8();

        // Original grayscale
        variants.push(("original".to_string(), gray.clone()));

        // Contrast enhanced
        let enhanced = Self::enhance_contrast(&gray);
        variants.push(("contrast_enhanced".to_string(), enhanced.clone()));

        // Otsu binarization (safer than adaptive threshold)
        variants.push(("otsu".to_string(), Self::otsu_binarization(&enhanced)));

        // Inverted for dark backgrounds
        variants.push(("inverted".to_string(), Self::invert(&enhanced)));

        // Only add adaptive threshold for reasonably sized images
        // Skip for large images to avoid integral_image overflow (u32 limit)
        let width = gray.width();
        let height = gray.height();
        let pixel_count = (width as u64) * (height as u64);

        // Conservative limit: ensure width * height * 255 < u32::MAX
        // u32::MAX / 255 ≈ 16,843,009 pixels
        if width > 100 && height > 100 && pixel_count < 10_000_000 {
            // Adaptive threshold (small blocks) - safely
            let block_radius = (width.min(height) / 50).clamp(5, 50);
            debug!(
                "Using adaptive threshold with block_radius: {}",
                block_radius
            );
            variants.push((
                "adaptive".to_string(),
                Self::adaptive_threshold_image(&enhanced, block_radius),
            ));
        } else if pixel_count >= 10_000_000 {
            debug!(
                "Skipping adaptive threshold for large image ({}x{} = {} pixels)",
                width, height, pixel_count
            );
        }

        variants
    }
}
