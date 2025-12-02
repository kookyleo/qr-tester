mod analyzer;
mod preprocessor;
mod scanner;
mod timer;

use analyzer::QrAnalyzer;
use anyhow::{Context, Result, bail};
use clap::Parser;
use colored::Colorize;
use log::info;
use std::path::PathBuf;

use scanner::QrScanner;

/// QR code scanning and performance testing tool
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input path (file or directory)
    #[arg(value_name = "PATH")]
    input: PathBuf,

    /// Verbose output mode
    #[arg(short, long)]
    verbose: bool,

    /// Output in JSON format
    #[arg(short, long)]
    json: bool,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Analyze QR code detection failures in detail
    #[arg(short, long)]
    analyze: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logger
    init_logger(args.debug);

    // Silence ZBar C library warnings unless debug mode is enabled
    if !args.debug {
        zbar_pack::set_verbosity(0);
    }

    info!("QR code scanner starting");
    info!("Input path: {}", args.input.display());

    // Validate input path
    if !args.input.exists() {
        bail!("Path does not exist: {}", args.input.display());
    }

    // Handle analyze mode
    if args.analyze {
        if !args.input.is_file() {
            bail!("Analyze mode requires a single file, not a directory");
        }

        let analyzer = QrAnalyzer::new();
        let report = analyzer
            .analyze_file(&args.input)
            .with_context(|| format!("Failed to analyze file: {}", args.input.display()))?;

        if args.json {
            output_analysis_json(&report)?;
        } else {
            analyzer.print_report(&report);
        }

        return Ok(());
    }

    let mut scanner = QrScanner::new(args.verbose);

    // Scan based on input type
    let results = if args.input.is_file() {
        info!("Detected single file input");
        vec![
            scanner
                .scan_file(&args.input)
                .with_context(|| format!("Failed to scan file: {}", args.input.display()))?,
        ]
    } else if args.input.is_dir() {
        info!("Detected directory input");
        scanner
            .scan_directory(&args.input)
            .with_context(|| format!("Failed to scan directory: {}", args.input.display()))?
    } else {
        bail!("Unsupported input type: {}", args.input.display());
    };

    // Output results
    if args.json {
        output_json(&results, scanner.stats())?;
    } else {
        output_text(&results, scanner.stats(), args.verbose);
    }

    Ok(())
}

/// Initialize logging system
fn init_logger(debug: bool) {
    let log_level = if debug { "debug" } else { "error" };

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .format_timestamp_millis()
        .init();
}

/// Output text format results (tabular)
fn output_text(results: &[scanner::ScanResult], stats: &timer::ScanStats, _verbose: bool) {
    if results.is_empty() {
        println!("No image files found");
        return;
    }

    println!(
        "\n{}",
        "QR Code Detection Performance Test Results (By Engine)"
            .bright_cyan()
            .bold()
    );
    println!("{}", "=".repeat(140).bright_blue());

    // Table header
    println!(
        "{:<50} {:>10} {:>6} {:>14} {:>14} {:>14} {:>14}",
        "File Path".bright_yellow(),
        "Engine".bright_yellow(),
        "QRs".bright_yellow(),
        "Preprocess".bright_yellow(),
        "Detection".bright_yellow(),
        "Total".bright_yellow(),
        "File Total".bright_yellow()
    );
    println!("{}", "-".repeat(140));

    // Data rows - show each engine's result separately
    for result in results {
        if !result.success {
            println!(
                "{:<50} {}",
                truncate_path(&result.file_path.display().to_string(), 50),
                "FAILED".red()
            );
            continue;
        }

        let path = truncate_path(&result.file_path.display().to_string(), 50);
        let timing = &result.timing.qr_detection;
        let file_total = timing.to_ms(timing.total);

        // Show each engine's results as separate rows
        let preprocess_time = timing.to_ms(timing.to_grayscale);

        for (idx, engine_result) in result.engine_results.iter().enumerate() {
            let display_path = if idx == 0 {
                path.clone()
            } else {
                "".to_string()
            };
            let display_file_total = if idx == 0 {
                format!("{:.2}ms", file_total)
            } else {
                "".to_string()
            };

            // Each engine has its own detection time
            let detection_time = engine_result.duration_ms;
            let engine_total = preprocess_time + detection_time;

            println!(
                "{:<50} {:>10} {:>6} {:>13.2}ms {:>13.2}ms {:>13.2}ms {:>14}",
                display_path,
                engine_result.engine_name,
                engine_result.qr_codes.len(),
                preprocess_time,
                detection_time,
                engine_total,
                display_file_total
            );
        }

        // Add separator between files
        println!("{}", "-".repeat(140).dimmed());
    }

    println!("{}", "=".repeat(140).bright_blue());

    // Statistics
    println!(
        "\n{}  Success: {}  Failed: {}  With QR: {}  Total QRs: {}  Avg Time: {:.2}ms",
        "Stats:".bright_cyan(),
        stats.successful_scans.to_string().green(),
        stats.failed_scans.to_string().red(),
        stats.files_with_qr.to_string().yellow(),
        stats.total_qr_codes.to_string().bright_green(),
        stats.avg_duration_per_file.as_secs_f64() * 1000.0
    );
    println!();
}

/// Truncate path to fit column width (handles UTF-8 safely)
fn truncate_path(path: &str, max_len: usize) -> String {
    // Use char count instead of byte count for proper handling
    let char_count = path.chars().count();

    if char_count <= max_len {
        return path.to_string();
    }

    let prefix = "...";
    let prefix_len = prefix.chars().count();
    let available = max_len.saturating_sub(prefix_len);

    // Skip chars from the start and take the remaining
    let suffix: String = path
        .chars()
        .skip(char_count.saturating_sub(available))
        .collect();
    format!("{}{}", prefix, suffix)
}

/// Output results in JSON format
fn output_json(results: &[scanner::ScanResult], stats: &timer::ScanStats) -> Result<()> {
    #[derive(serde::Serialize)]
    struct JsonOutput<'a> {
        results: Vec<JsonResult>,
        stats: &'a timer::ScanStats,
    }

    #[derive(serde::Serialize)]
    struct JsonResult {
        file_path: String,
        qr_codes: Vec<String>,
        timing: timer::ScanTiming,
        success: bool,
        error: Option<String>,
    }

    let json_results: Vec<JsonResult> = results
        .iter()
        .map(|r| JsonResult {
            file_path: r.file_path.display().to_string(),
            qr_codes: r.qr_codes.clone(),
            timing: r.timing.clone(),
            success: r.success,
            error: r.error.clone(),
        })
        .collect();

    let output = JsonOutput {
        results: json_results,
        stats,
    };

    let json = serde_json::to_string_pretty(&output).context("Failed to serialize JSON")?;

    println!("{}", json);

    Ok(())
}

/// Output analysis results in JSON format
fn output_analysis_json(report: &analyzer::AnalysisReport) -> Result<()> {
    #[derive(serde::Serialize)]
    struct JsonAnalysisOutput {
        file_path: String,
        image_size: (u32, u32),
        variants_tested: usize,
        overall_success: bool,
        engine_analyses: Vec<JsonEngineAnalysis>,
        recommendations: Vec<String>,
    }

    #[derive(serde::Serialize)]
    struct JsonEngineAnalysis {
        engine_name: String,
        grids_detected: usize,
        success: bool,
        summary: String,
        decode_results: Vec<JsonGridAnalysis>,
    }

    #[derive(serde::Serialize)]
    struct JsonGridAnalysis {
        grid_index: usize,
        decode_success: bool,
        error_type: Option<String>,
        error_detail: String,
        content: Option<String>,
    }

    let engine_analyses: Vec<JsonEngineAnalysis> = report
        .engine_analyses
        .iter()
        .map(|a| JsonEngineAnalysis {
            engine_name: a.engine_name.clone(),
            grids_detected: a.grids_detected,
            success: a.success,
            summary: a.summary.clone(),
            decode_results: a
                .decode_results
                .iter()
                .map(|r| JsonGridAnalysis {
                    grid_index: r.grid_index,
                    decode_success: r.decode_success,
                    error_type: r.error_type.clone(),
                    error_detail: r.error_detail.clone(),
                    content: r.content.clone(),
                })
                .collect(),
        })
        .collect();

    let output = JsonAnalysisOutput {
        file_path: report.file_path.clone(),
        image_size: report.image_size,
        variants_tested: report.variants_tested,
        overall_success: report.overall_success,
        engine_analyses,
        recommendations: report.recommendations.clone(),
    };

    let json = serde_json::to_string_pretty(&output).context("Failed to serialize JSON")?;

    println!("{}", json);

    Ok(())
}
