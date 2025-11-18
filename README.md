# QR Tester

A CLI tool for scanning QR codes from images and benchmarking performance.

[中文文档](README.zh.md)

## Features

- Scan single image files or entire directories
- Detailed performance metrics for QR detection stages:
  - Grayscale conversion
  - Image preparation
  - Grid detection
  - QR code decoding
- Support for multiple image formats (PNG, JPG, BMP, GIF, WebP, TIFF, etc.)
- Colorful terminal output with tabular results
- Optional JSON format output
- Comprehensive statistics (total files, success rate, average time, etc.)

## Installation

### From crates.io

```bash
cargo install qr-tester
```

### From Source

```bash
cargo install --git https://github.com/kookyleo/qr-tester.git
```

Or clone and build:

```bash
git clone https://github.com/kookyleo/qr-tester.git
cd qr-tester
cargo build --release
```

## Usage

### Basic Usage

Scan a single image file:

```bash
qr-tester image.png
```

Scan an entire directory:

```bash
qr-tester /path/to/images/
```

### Options

- `-v, --verbose`: Verbose output mode
- `-j, --json`: Output results in JSON format
- `-d, --debug`: Enable debug logging
- `-h, --help`: Display help information
- `-V, --version`: Display version information

### Examples

1. Scan directory with verbose output:

```bash
qr-tester -v /path/to/images/
```

2. Scan single file and output JSON:

```bash
qr-tester -j qrcode.png
```

3. Enable debug logging for detailed analysis:

```bash
qr-tester -d -v /path/to/images/
```

## Output Format

### Text Output

The tool displays results in a table format:

```
QR Code Detection Performance Test Results
======================================================================================
File Path                                                    QRs     Grayscale        Prepare   Detect Grids      Decode QR          Total
--------------------------------------------------------------------------------------
/path/to/qrcode1.png                                          1          5.23ms       10.45ms       50.12ms          8.34ms       74.14ms
/path/to/qrcode2.jpg                                          2          6.78ms       12.67ms       65.23ms         12.45ms       97.13ms
--------------------------------------------------------------------------------------

Stats:  Success: 2  Failed: 0  With QR: 2  Total QRs: 3  Avg Time: 85.64ms
```

### Performance Metrics

- **QRs**: Number of QR codes detected in the file
- **Grayscale**: Time spent converting image to grayscale
- **Prepare**: Time spent preparing image for detection
- **Detect Grids**: Time spent detecting QR code grids (usually the slowest stage)
- **Decode QR**: Time spent decoding QR code data
- **Total**: Sum of all stages

### Statistics

After scanning, the tool displays:
- Total files processed
- Successful scans
- Failed scans
- Files containing QR codes
- Total QR codes found
- Average time per file

## Dependencies

- `clap`: Command-line argument parsing
- `image`: Image loading and processing
- `rqrr`: High-performance pure Rust QR code recognition
- `walkdir`: Directory traversal
- `anyhow`: Error handling
- `colored`: Colorful terminal output
- `serde/serde_json`: JSON serialization

## License

Apache-2.0
