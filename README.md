# Seal

一个用Rust实现的数字水印CLI工具，支持给图片、音频和视频文件添加和提取数字水印。

## 特性

- 🖼️ **图片水印**: 支持JPG, PNG, BMP, GIF, TIFF, WebP等格式
- 🎵 **音频水印**: 支持WAV格式
- 🎬 **视频水印**: 支持多种视频格式，含FFmpeg集成
- 🔢 **DCT算法**: 基于离散余弦变换的高性能水印算法
- ⚡ **高性能**: 基于Rust和rustdct库，安全且高效
- 🎯 **精确控制**: 可调节水印强度
- 📊 **进度显示**: 美观的进度条和状态提示

## 安装

### 从源码构建

确保你已经安装了Rust和Cargo，然后运行：

```bash
git clone <repository-url>
cd media-seal-rs
cargo build --release
```

编译后的可执行文件位于 `target/release/seal`

## 使用方法

### 基本语法

```bash
seal <COMMAND> [OPTIONS]
```

### 命令

#### 嵌入水印 (embed)

```bash
seal embed -i <输入文件> -o <输出文件> -w <水印内容> [-a <算法>] [-s <强度>] [--lossless]
```

**参数说明:**
- `-i, --input <文件>`: 输入文件路径
- `-o, --output <文件>`: 输出文件路径  
- `-w, --watermark <文本>`: 水印内容
- `-a, --algorithm <算法>`: 使用的算法 (目前仅支持dct，默认: dct)
- `-s, --strength <强度>`: 水印强度 0.0-1.0 (默认: 0.1)
- `--lossless`: 是否使用无损压缩（仅对视频有效）
- `-v, --verbose`: 详细输出

**示例:**

```bash
# 给图片添加水印
seal embed -i photo.jpg -o photo_watermarked.jpg -w "版权所有" -s 0.1

# 给音频添加水印
seal embed -i audio.wav -o audio_watermarked.wav -w "我的音乐" -s 0.05

# 给视频添加水印
seal embed -i video.mp4 -o video_watermarked.mp4 -w "版权所有" -s 0.1

# 给视频添加无损水印
seal embed -i video.mp4 -o video_watermarked.mp4 -w "版权所有" --lossless
```

#### 提取水印 (extract)

```bash
seal extract -i <输入文件> -l <长度> [-a <算法>] [-o <输出文件>]
```

**参数说明:**
- `-i, --input <文件>`: 包含水印的文件路径
- `-l, --length <长度>`: 期望的水印文本长度（字符数）
- `-a, --algorithm <算法>`: 使用的算法 (目前仅支持dct，默认: dct)
- `-o, --output <文件>`: 保存提取水印的文件 (可选)
- `-v, --verbose`: 详细输出

**示例:**

```bash
# 从图片提取水印
seal extract -i photo_watermarked.jpg -l 4

# 从音频提取水印并保存到文件
seal extract -i audio_watermarked.wav -l 6 -o extracted_watermark.txt

# 从视频提取水印
seal extract -i video_watermarked.mp4 -l 4
```

## 算法说明

### DCT (离散余弦变换)

- **优点**: 鲁棒性好，抗压缩能力强，高性能实现
- **适用**: 需要较强抗攻击能力的场景
- **特性**: 
  - 使用`rustdct`库，性能优异
  - 支持任意尺寸图像（自动填充）
  - 基于8×8块处理
  - 支持图片、音频和视频水印

## 支持格式

### 图片格式
- JPEG (.jpg, .jpeg)
- PNG (.png)
- BMP (.bmp)
- GIF (.gif)
- TIFF (.tiff)
- WebP (.webp)

### 音频格式
- WAV (.wav, .wave)

### 视频格式
- MP4 (.mp4)
- AVI (.avi)
- MKV (.mkv)
- MOV (.mov)
- 其他FFmpeg支持的格式

## 注意事项

1. **水印长度**: 确保水印文本长度适中，过长的水印可能影响嵌入效果
2. **强度设置**: 
   - 强度过低可能导致水印提取困难
   - 强度过高可能影响媒体质量
   - 建议范围: 0.05-0.2
3. **格式兼容**: DCT算法支持任意尺寸图像（自动填充处理）
4. **质量保持**: 嵌入水印会轻微影响原始媒体质量
5. **视频处理**: 视频水印会逐帧处理，处理时间较长
6. **FFmpeg依赖**: 视频功能需要FFmpeg支持，会自动下载

## 开发

### 项目结构

```
src/
├── main.rs          # CLI主程序
├── lib.rs           # 库入口
├── cli.rs           # 命令行参数定义
├── error.rs         # 错误处理
├── watermark/       # 水印算法模块
│   ├── mod.rs       # 算法工厂
│   ├── trait.rs     # 通用接口
│   └── dct.rs       # DCT算法实现
└── media/           # 媒体处理模块
    ├── mod.rs
    ├── image.rs     # 图片处理
    ├── audio.rs     # 音频处理
    └── video.rs     # 视频处理
```

### 作为库使用

```rust
use seal::prelude::*;

fn main() -> Result<()> {
    // 创建DCT算法
    let algorithm = WatermarkFactory::create_algorithm(Algorithm::Dct);
    
    // 嵌入图片水印
    ImageWatermarker::embed_watermark(
        "input.jpg",
        "output.jpg",
        "我的水印",
        algorithm.as_ref(),
        0.1
    )?;
    
    // 提取图片水印
    let watermark = ImageWatermarker::extract_watermark(
        "output.jpg",
        algorithm.as_ref(),
        4 // 水印字符数
    )?;
    
    println!("提取的水印: {}", watermark);
    Ok(())
}
```

### 运行测试

```bash
cargo test
```

### 性能测试

```bash
cargo bench
```

## 贡献

欢迎提交Issue和Pull Request！

## 许可

[MIT License](LICENSE)

## 更新日志

### v0.1.0 - 初始版本
- ✅ 实现DCT数字水印算法
- ✅ 支持图片水印（JPG, PNG, BMP, GIF, TIFF, WebP）
- ✅ 支持音频水印（WAV格式）
- ✅ 支持视频水印（多种格式，含FFmpeg集成）
- ✅ 使用rustdct库，高性能实现
- ✅ 美观的进度条和状态提示
- ✅ 支持无损视频压缩选项
- ✅ 完善的错误处理和用户体验

## 依赖库

- `clap` - 命令行参数解析
- `image` - 图片处理
- `hound` - 音频处理
- `rustdct` - DCT算法实现
- `ffmpeg-sidecar` - 视频处理
- `ndarray` - 数组运算
- `colored` - 彩色输出
- `indicatif` - 进度条显示

## TODO

- [ ] 支持更多音频格式（MP3、FLAC等）
- [ ] 添加DWT（离散小波变换）算法
- [ ] GUI界面
- [ ] 水印强度自动优化
- [ ] 批量处理功能
- [ ] 性能基准测试
