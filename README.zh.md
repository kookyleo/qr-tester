# QR Tester

用于扫描图片中 QR 码并测试性能的 CLI 工具。

[English Documentation](README.md)

## 功能特性

- 支持单个图片文件或整个目录的扫描
- QR 检测各阶段的详细性能指标：
  - 灰度转换
  - 图像预处理
  - 网格检测
  - QR 码解码
- 支持多种图片格式（PNG、JPG、BMP、GIF、WebP、TIFF 等）
- 彩色终端输出，表格化展示结果
- 可选 JSON 格式输出
- 全面的统计信息（总文件数、成功率、平均耗时等）

## 安装

### 从源码安装

```bash
cargo install --git https://github.com/kookyleo/qr-tester.git
```

或者克隆并编译：

```bash
git clone https://github.com/kookyleo/qr-tester.git
cd qr-tester
cargo build --release
```

编译完成后，可执行文件位于 `target/release/qr-tester`。

## 使用方法

### 基本用法

扫描单个图片文件：

```bash
qr-tester image.png
```

扫描整个目录：

```bash
qr-tester /path/to/images/
```

### 选项参数

- `-v, --verbose`: 详细输出模式
- `-j, --json`: 以 JSON 格式输出结果
- `-d, --debug`: 启用调试日志
- `-h, --help`: 显示帮助信息
- `-V, --version`: 显示版本信息

### 示例

1. 扫描目录并显示详细过程：

```bash
qr-tester -v /path/to/images/
```

2. 扫描单个文件并输出 JSON：

```bash
qr-tester -j qrcode.png
```

3. 启用调试日志进行详细分析：

```bash
qr-tester -d -v /path/to/images/
```

## 输出格式

### 文本输出

工具以表格形式显示结果：

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

### 性能指标说明

- **QRs**: 该文件中检测到的 QR 码数量
- **Grayscale**: 图像转换为灰度的耗时
- **Prepare**: 图像预处理的耗时
- **Detect Grids**: 检测 QR 码网格的耗时（通常是最慢的阶段）
- **Decode QR**: 解码 QR 码数据的耗时
- **Total**: 所有阶段的总耗时

### 统计信息

扫描完成后显示：
- 总文件数
- 成功扫描数
- 失败扫描数
- 包含 QR 码的文件数
- 总 QR 码数量
- 平均每文件耗时

## QR 码检测算法

本工具使用 `rqrr` 库进行 QR 码检测。QR 码检测算法虽然各家实现细节有所不同，但核心原理大同小异：

### 通用流程

1. **图像预处理**
   - 转换为灰度图
   - 二值化处理
   - 可能的图像增强

2. **定位标记检测**
   - 查找 QR 码的三个定位图案（角上的方块）
   - 通过比例特征识别定位标记

3. **网格检测与校正**
   - 根据定位标记确定 QR 码的位置和方向
   - 进行透视变换校正

4. **数据解码**
   - 读取模块（黑白方块）
   - Reed-Solomon 纠错解码
   - 提取最终数据

### rqrr 库的特点

- **纯 Rust 实现**：无需外部 C/C++ 依赖
- **性能优良**：针对 Rust 优化的高效实现
- **代表性强**：算法流程符合标准 QR 码规范
- **适合测试**：可准确反映各阶段的性能特征

虽然商业级 QR 码扫描库（如 ZXing、OpenCV）在某些优化细节上可能有所不同，但 `rqrr` 的实现足以代表主流 QR 码检测算法的性能特征，适合用于性能基准测试。

## 依赖库

- `clap`: 命令行参数解析
- `image`: 图像加载和处理
- `rqrr`: 高性能纯 Rust QR 码识别库
- `walkdir`: 目录遍历
- `anyhow`: 错误处理
- `colored`: 彩色终端输出
- `serde/serde_json`: JSON 序列化

## 许可证

Apache-2.0
