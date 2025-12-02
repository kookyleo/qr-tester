//! QR code debug analyzer module
//!
//! Provides detailed analysis of QR code detection and decoding failures,
//! with precision down to module (码点) or codeword (码字) level.

use anyhow::{Context, Result};
use colored::Colorize;
use image::{DynamicImage, GrayImage};
use rxing::{
    BinaryBitmap, DecodeHints, Exceptions, Luma8LuminanceSource, Reader, common::HybridBinarizer,
    qrcode::QRCodeReader,
};
use std::path::Path;

use crate::preprocessor::ImagePreprocessor;

/// Analysis result for a single engine
#[derive(Debug)]
pub struct EngineAnalysis {
    pub engine_name: String,
    pub grids_detected: usize,
    pub decode_results: Vec<GridAnalysis>,
    pub success: bool,
    pub summary: String,
}

/// Analysis result for a single detected grid
#[derive(Debug)]
pub struct GridAnalysis {
    pub grid_index: usize,
    #[allow(dead_code)]
    pub version: Option<u32>,
    #[allow(dead_code)]
    pub module_size: Option<(u32, u32)>,
    pub decode_success: bool,
    pub error_type: Option<String>,
    pub error_detail: String,
    pub content: Option<String>,
}

/// Detailed QR code analysis result
#[derive(Debug)]
pub struct AnalysisReport {
    pub file_path: String,
    pub image_size: (u32, u32),
    pub variants_tested: usize,
    pub engine_analyses: Vec<EngineAnalysis>,
    pub overall_success: bool,
    pub recommendations: Vec<String>,
}

/// QR code debug analyzer
pub struct QrAnalyzer;

impl QrAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Analyze a single image file and produce detailed debug report
    pub fn analyze_file(&self, path: &Path) -> Result<AnalysisReport> {
        let file_data = std::fs::read(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;
        let img = image::load_from_memory(&file_data)
            .with_context(|| format!("Failed to decode image: {}", path.display()))?;

        let width = img.width();
        let height = img.height();

        // Resize large images
        let working_img = if width.max(height) > 2000 {
            let scale = 2000.0 / width.max(height) as f32;
            ImagePreprocessor::resize(&img, scale)
        } else {
            img.clone()
        };

        // Generate variants
        let variants = ImagePreprocessor::generate_variants(&working_img);
        let variants_tested = variants.len();

        let mut engine_analyses = Vec::new();
        let mut overall_success = false;

        // Analyze with first variant only to avoid excessive output
        if let Some((variant_name, gray_img)) = variants.first() {
            // rqrr analysis
            let rqrr_analysis = self.analyze_with_rqrr(gray_img, variant_name);
            if rqrr_analysis.success {
                overall_success = true;
            }
            engine_analyses.push(rqrr_analysis);

            // quircs analysis
            let quircs_analysis = self.analyze_with_quircs(gray_img, variant_name);
            if quircs_analysis.success {
                overall_success = true;
            }
            engine_analyses.push(quircs_analysis);

            // rxing analysis
            let rxing_analysis = self.analyze_with_rxing(gray_img, &working_img, variant_name);
            if rxing_analysis.success {
                overall_success = true;
            }
            engine_analyses.push(rxing_analysis);
        }

        // Generate recommendations based on failures
        let recommendations = self.generate_recommendations(&engine_analyses);

        Ok(AnalysisReport {
            file_path: path.display().to_string(),
            image_size: (width, height),
            variants_tested,
            engine_analyses,
            overall_success,
            recommendations,
        })
    }

    /// Detailed analysis using rqrr
    fn analyze_with_rqrr(&self, gray_img: &GrayImage, variant_name: &str) -> EngineAnalysis {
        let mut img_data = rqrr::PreparedImage::prepare(gray_img.clone());
        let grids = img_data.detect_grids();

        let grids_detected = grids.len();
        let mut decode_results = Vec::new();
        let mut any_success = false;

        for (i, grid) in grids.iter().enumerate() {
            let (decode_success, error_type, error_detail, content) = match grid.decode() {
                Ok((meta, content)) => {
                    any_success = true;
                    (
                        true,
                        None,
                        format!(
                            "Version {}, ECC level {:?}, {} modules",
                            meta.version.0,
                            meta.ecc_level,
                            meta.version.0 * 4 + 17
                        ),
                        Some(content),
                    )
                }
                Err(e) => {
                    let (err_type, detail) = self.analyze_rqrr_error(&e);
                    (false, Some(err_type), detail, None)
                }
            };

            decode_results.push(GridAnalysis {
                grid_index: i,
                version: None, // rqrr doesn't expose version before decode
                module_size: None,
                decode_success,
                error_type,
                error_detail,
                content,
            });
        }

        let summary = if grids_detected == 0 {
            "No QR code patterns detected".to_string()
        } else if any_success {
            format!(
                "Successfully decoded {}/{} grids",
                decode_results.iter().filter(|r| r.decode_success).count(),
                grids_detected
            )
        } else {
            format!("Detected {} grids but all failed to decode", grids_detected)
        };

        EngineAnalysis {
            engine_name: format!("rqrr ({})", variant_name),
            grids_detected,
            decode_results,
            success: any_success,
            summary,
        }
    }

    /// Analyze rqrr error in detail
    fn analyze_rqrr_error(&self, error: &rqrr::DeQRError) -> (String, String) {
        match error {
            rqrr::DeQRError::DataUnderflow => (
                "DataUnderflow".to_string(),
                "数据不足: QR 码数据区域的码字数量少于预期。\n\
                 可能原因: 图像模糊、码点缺损、版本检测错误。"
                    .to_string(),
            ),
            rqrr::DeQRError::DataOverflow => (
                "DataOverflow".to_string(),
                "数据溢出: QR 码数据区域的码字数量超出预期。\n\
                 可能原因: 版本检测错误、干扰码点被误读。"
                    .to_string(),
            ),
            rqrr::DeQRError::UnknownDataType => (
                "UnknownDataType".to_string(),
                "未知数据类型: 无法识别 QR 码的编码模式。\n\
                 可能原因: 数据区域损坏、纠错失败导致模式指示符错误。"
                    .to_string(),
            ),
            rqrr::DeQRError::DataEcc => (
                "DataEcc".to_string(),
                "数据纠错失败: RS 纠错码无法恢复数据。\n\
                 可能原因: 码点损坏超出纠错能力、污染或遮挡严重。\n\
                 这是最常见的失败原因，通常意味着 QR 码损坏程度超过了纠错等级的容错能力。"
                    .to_string(),
            ),
            rqrr::DeQRError::FormatEcc => (
                "FormatEcc".to_string(),
                "格式信息纠错失败: 无法从两个位置读取有效的格式信息。\n\
                 可能原因: 定位图案周围的格式区域损坏。\n\
                 格式信息包含纠错等级和掩码图案，位于 QR 码的固定位置。"
                    .to_string(),
            ),
            rqrr::DeQRError::InvalidVersion => (
                "InvalidVersion".to_string(),
                "无效版本: 检测到不支持或不存在的 QR 码版本。\n\
                 可能原因: 版本信息区域损坏 (版本 7+ 的 QR 码有专门的版本信息区)。"
                    .to_string(),
            ),
            rqrr::DeQRError::InvalidGridSize => (
                "InvalidGridSize".to_string(),
                "无效网格尺寸: 检测到的 QR 码尺寸不符合规范。\n\
                 可能原因: 图像变形、定位图案检测错误。"
                    .to_string(),
            ),
            rqrr::DeQRError::EncodingError => (
                "EncodingError".to_string(),
                "编码错误: 解码后的数据不是有效的 UTF-8。\n\
                 可能原因: QR 码使用了非 UTF-8 编码 (如 Shift-JIS)。"
                    .to_string(),
            ),
            rqrr::DeQRError::IoError => (
                "IoError".to_string(),
                "I/O 错误: 输出写入失败。".to_string(),
            ),
        }
    }

    /// Detailed analysis using quircs
    fn analyze_with_quircs(&self, gray_img: &GrayImage, variant_name: &str) -> EngineAnalysis {
        let width = gray_img.width() as usize;
        let height = gray_img.height() as usize;

        let mut decoder = quircs::Quirc::new();
        let codes = decoder.identify(width, height, gray_img.as_raw());

        let mut grids_detected = 0;
        let mut decode_results = Vec::new();
        let mut any_success = false;

        for (i, code_result) in codes.enumerate() {
            grids_detected += 1;

            let (decode_success, error_type, error_detail, content) = match code_result {
                Ok(code) => match code.decode() {
                    Ok(decoded) => {
                        any_success = true;
                        let text = std::str::from_utf8(&decoded.payload)
                            .unwrap_or("<binary data>")
                            .to_string();
                        (
                            true,
                            None,
                            format!(
                                "Version {}, ECC level {:?}, Data type {:?}",
                                decoded.version, decoded.ecc_level, decoded.data_type
                            ),
                            Some(text),
                        )
                    }
                    Err(e) => {
                        let (err_type, detail) = self.analyze_quircs_decode_error(&e);
                        (false, Some(err_type), detail, None)
                    }
                },
                Err(e) => {
                    let (err_type, detail) = self.analyze_quircs_extract_error(&e);
                    (false, Some(err_type), detail, None)
                }
            };

            decode_results.push(GridAnalysis {
                grid_index: i,
                version: None,
                module_size: None,
                decode_success,
                error_type,
                error_detail,
                content,
            });
        }

        let summary = if grids_detected == 0 {
            "No QR code patterns detected".to_string()
        } else if any_success {
            format!(
                "Successfully decoded {}/{} codes",
                decode_results.iter().filter(|r| r.decode_success).count(),
                grids_detected
            )
        } else {
            format!("Detected {} codes but all failed to decode", grids_detected)
        };

        EngineAnalysis {
            engine_name: format!("quircs ({})", variant_name),
            grids_detected,
            decode_results,
            success: any_success,
            summary,
        }
    }

    /// Analyze quircs decode error
    fn analyze_quircs_decode_error(&self, error: &quircs::DecodeError) -> (String, String) {
        match error {
            quircs::DecodeError::InvalidGridSize => (
                "InvalidGridSize".to_string(),
                "无效网格尺寸: QR 码的模块数量不符合规范 (应为 21+4n)。".to_string(),
            ),
            quircs::DecodeError::InvalidVersion => (
                "InvalidVersion".to_string(),
                "无效版本: 检测到的版本号超出 1-40 的有效范围。".to_string(),
            ),
            quircs::DecodeError::DataEcc => (
                "DataEcc".to_string(),
                "数据纠错失败: RS 码无法恢复损坏的数据码字。\n\
                 QR 码损坏程度超过了纠错能力。"
                    .to_string(),
            ),
            quircs::DecodeError::FormatEcc => (
                "FormatEcc".to_string(),
                "格式信息错误: 无法读取纠错等级和掩码信息。\n\
                 定位图案周围的 15 位格式信息区域损坏。"
                    .to_string(),
            ),
            quircs::DecodeError::UnkownDataType => (
                "UnknownDataType".to_string(),
                "未知数据类型: 数据模式指示符无法识别 (0001=数字, 0010=字母数字, 0100=字节, 1000=汉字)。"
                    .to_string(),
            ),
            quircs::DecodeError::DataOverflow => (
                "DataOverflow".to_string(),
                "数据溢出: 解码的数据长度超出该版本容量。".to_string(),
            ),
            quircs::DecodeError::DataUnderflow => (
                "DataUnderflow".to_string(),
                "数据不足: 数据码字不完整。".to_string(),
            ),
        }
    }

    /// Analyze quircs extract error
    fn analyze_quircs_extract_error(&self, error: &quircs::ExtractError) -> (String, String) {
        match error {
            quircs::ExtractError::OutOfBounds => (
                "OutOfBounds".to_string(),
                "越界错误: 提取码点时坐标超出图像边界。\n\
                 可能原因: QR 码靠近图像边缘、透视变换计算错误。"
                    .to_string(),
            ),
        }
    }

    /// Detailed analysis using rxing
    fn analyze_with_rxing(
        &self,
        gray_img: &GrayImage,
        _img: &DynamicImage,
        variant_name: &str,
    ) -> EngineAnalysis {
        let width = gray_img.width();
        let height = gray_img.height();

        let luminance_source = Luma8LuminanceSource::new(gray_img.as_raw().clone(), width, height);
        let mut bitmap = BinaryBitmap::new(HybridBinarizer::new(luminance_source));
        let hints = DecodeHints::default();
        let mut reader = QRCodeReader::new();

        let mut decode_results = Vec::new();
        let (grids_detected, any_success, summary) =
            match reader.decode_with_hints(&mut bitmap, &hints) {
                Ok(result) => {
                    decode_results.push(GridAnalysis {
                        grid_index: 0,
                        version: None,
                        module_size: None,
                        decode_success: true,
                        error_type: None,
                        error_detail: format!("Format: {:?}", result.getBarcodeFormat()),
                        content: Some(result.getText().to_string()),
                    });
                    (1, true, "Successfully decoded 1 QR code".to_string())
                }
                Err(e) => {
                    let (err_type, detail) = self.analyze_rxing_error(&e);
                    decode_results.push(GridAnalysis {
                        grid_index: 0,
                        version: None,
                        module_size: None,
                        decode_success: false,
                        error_type: Some(err_type.clone()),
                        error_detail: detail,
                        content: None,
                    });
                    (0, false, format!("Detection failed: {}", err_type))
                }
            };

        EngineAnalysis {
            engine_name: format!("rxing ({})", variant_name),
            grids_detected,
            decode_results,
            success: any_success,
            summary,
        }
    }

    /// Analyze rxing error
    fn analyze_rxing_error(&self, error: &Exceptions) -> (String, String) {
        match error {
            Exceptions::NotFoundException(_) => (
                "NotFoundException".to_string(),
                "未找到 QR 码: 无法在图像中检测到有效的 QR 码定位图案。\n\
                 可能原因: 图像模糊、对比度不足、QR 码被遮挡或变形。"
                    .to_string(),
            ),
            Exceptions::FormatException(msg) => (
                "FormatException".to_string(),
                format!(
                    "格式错误: 检测到的图案不符合 QR 码规范。\n\
                     详情: {}\n\
                     可能原因: 图像噪点、非标准 QR 码。",
                    if msg.is_empty() {
                        "无详细信息"
                    } else {
                        msg
                    }
                ),
            ),
            Exceptions::ChecksumException(msg) => (
                "ChecksumException".to_string(),
                format!(
                    "校验和错误: 数据验证失败。\n\
                     详情: {}\n\
                     可能原因: 数据区域损坏、纠错能力不足。",
                    if msg.is_empty() {
                        "无详细信息"
                    } else {
                        msg
                    }
                ),
            ),
            Exceptions::ReedSolomonException(msg) => (
                "ReedSolomonException".to_string(),
                format!(
                    "Reed-Solomon 纠错失败: 无法恢复损坏的码字。\n\
                     详情: {}\n\
                     这是 QR 码纠错算法层面的失败，意味着损坏程度超过了容错能力。",
                    if msg.is_empty() {
                        "无详细信息"
                    } else {
                        msg
                    }
                ),
            ),
            _ => (
                format!("{:?}", error),
                "其他错误，请查看详细日志。".to_string(),
            ),
        }
    }

    /// Generate recommendations based on analysis
    fn generate_recommendations(&self, analyses: &[EngineAnalysis]) -> Vec<String> {
        let mut recommendations = Vec::new();

        // Check if any grid was detected
        let total_grids: usize = analyses.iter().map(|a| a.grids_detected).sum();
        if total_grids == 0 {
            recommendations.push(
                "未检测到任何 QR 码图案。建议:\n\
                 1. 检查图像是否包含 QR 码\n\
                 2. 尝试提高图像对比度\n\
                 3. 确保 QR 码完整可见，没有被裁剪\n\
                 4. 如果 QR 码很小，尝试放大图像"
                    .to_string(),
            );
            return recommendations;
        }

        // Analyze common failures
        let mut has_data_ecc = false;
        let mut has_format_ecc = false;

        for analysis in analyses {
            for result in &analysis.decode_results {
                if let Some(ref err_type) = result.error_type {
                    if err_type.contains("DataEcc") {
                        has_data_ecc = true;
                    }
                    if err_type.contains("FormatEcc") {
                        has_format_ecc = true;
                    }
                }
            }
        }

        if has_data_ecc {
            recommendations.push(
                "数据纠错失败 (DataEcc) 是最常见的问题。建议:\n\
                 1. QR 码可能损坏严重 - 检查是否有污渍、划痕或褪色\n\
                 2. 尝试更清晰的图像源\n\
                 3. 如果是印刷品，确保扫描分辨率足够 (至少 300 DPI)\n\
                 4. 检查 QR 码生成时使用的纠错等级 (L/M/Q/H)"
                    .to_string(),
            );
        }

        if has_format_ecc {
            recommendations.push(
                "格式信息错误 (FormatEcc) 表示 QR 码的元数据区域损坏。建议:\n\
                 1. 检查 QR 码三个角的定位图案周围区域\n\
                 2. 格式信息位于定位图案旁的 L 形区域\n\
                 3. 这些区域不能有任何损坏"
                    .to_string(),
            );
        }

        if recommendations.is_empty() && !analyses.iter().any(|a| a.success) {
            recommendations.push(
                "所有引擎都检测到了 QR 码但解码失败。建议:\n\
                 1. 尝试调整图像亮度和对比度\n\
                 2. 使用 --debug 模式查看更多细节\n\
                 3. 尝试不同角度重新拍摄"
                    .to_string(),
            );
        }

        recommendations
    }

    /// Print analysis report to console
    pub fn print_report(&self, report: &AnalysisReport) {
        println!("\n{}", "═".repeat(80).bright_cyan());
        println!("{}", " QR Code Debug Analysis Report ".bright_cyan().bold());
        println!("{}\n", "═".repeat(80).bright_cyan());

        // File info
        println!("{}: {}", "File".bright_yellow(), report.file_path);
        println!(
            "{}: {}x{} pixels",
            "Image Size".bright_yellow(),
            report.image_size.0,
            report.image_size.1
        );
        println!(
            "{}: {}",
            "Variants Tested".bright_yellow(),
            report.variants_tested
        );
        println!(
            "{}: {}",
            "Overall Result".bright_yellow(),
            if report.overall_success {
                "SUCCESS".green().bold()
            } else {
                "FAILED".red().bold()
            }
        );

        // Engine analyses
        println!("\n{}", "─".repeat(80));
        println!("{}\n", "Engine Analysis Details".bright_cyan().bold());

        for analysis in &report.engine_analyses {
            println!(
                "▶ {} - {}",
                analysis.engine_name.bright_white().bold(),
                if analysis.success {
                    "✓".green()
                } else {
                    "✗".red()
                }
            );
            println!(
                "  {}: {}",
                "Grids Detected".dimmed(),
                analysis.grids_detected
            );
            println!("  {}: {}", "Summary".dimmed(), analysis.summary);

            for result in &analysis.decode_results {
                println!("\n  {} #{}:", "Grid".bright_white(), result.grid_index);

                if let Some(ref content) = result.content {
                    let display_content = if content.len() > 100 {
                        format!("{}...", &content[..100])
                    } else {
                        content.clone()
                    };
                    println!("    {}: {}", "Content".green(), display_content);
                }

                if let Some(ref err_type) = result.error_type {
                    println!("    {}: {}", "Error Type".red(), err_type);
                }

                // Print error detail with proper indentation
                if !result.error_detail.is_empty() {
                    println!("    {}:", "Detail".yellow());
                    for line in result.error_detail.lines() {
                        println!("      {}", line);
                    }
                }
            }
            println!();
        }

        // Recommendations
        if !report.recommendations.is_empty() {
            println!("{}", "─".repeat(80));
            println!("{}\n", "Recommendations".bright_cyan().bold());

            for (i, rec) in report.recommendations.iter().enumerate() {
                println!("{}. ", i + 1);
                for line in rec.lines() {
                    println!("   {}", line);
                }
                println!();
            }
        }

        println!("{}", "═".repeat(80).bright_cyan());
    }
}
