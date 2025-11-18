mod scanner;
mod timer;

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
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logger
    init_logger(args.debug);

    info!("QR code scanner starting");
    info!("Input path: {}", args.input.display());

    // Validate input path
    if !args.input.exists() {
        bail!("Path does not exist: {}", args.input.display());
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
    let log_level = if debug { "debug" } else { "info" };

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
        "QR Code Detection Performance Test Results"
            .bright_cyan()
            .bold()
    );
    println!("{}", "=".repeat(150).bright_blue());

    // Table header
    println!(
        "{:<55} {:>6} {:>14} {:>14} {:>14} {:>14} {:>14}",
        "File Path".bright_yellow(),
        "QRs".bright_yellow(),
        "Grayscale".bright_yellow(),
        "Prepare".bright_yellow(),
        "Detect Grids".bright_yellow(),
        "Decode QR".bright_yellow(),
        "Total".bright_yellow()
    );
    println!("{}", "-".repeat(150));

    // Data rows
    for result in results {
        if !result.success {
            println!(
                "{:<55} {}",
                truncate_path(&result.file_path.display().to_string(), 55),
                "FAILED".red()
            );
            continue;
        }

        let timing = &result.timing.qr_detection;
        println!(
            "{:<55} {:>6} {:>13.2}ms {:>13.2}ms {:>13.2}ms {:>13.2}ms {:>13.2}ms",
            truncate_path(&result.file_path.display().to_string(), 55),
            result.qr_count(),
            timing.to_ms(timing.to_grayscale),
            timing.to_ms(timing.prepare_image),
            timing.to_ms(timing.detect_grids),
            timing.to_ms(timing.decode_qr),
            timing.to_ms(timing.total)
        );
    }

    println!("{}", "=".repeat(150).bright_blue());

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

/// Truncate path to fit column width
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        return path.to_string();
    }

    let prefix = "...";
    let available = max_len - prefix.len();
    format!("{}{}", prefix, &path[path.len() - available..])
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
