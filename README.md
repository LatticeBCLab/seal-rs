# Media Seal RS

一个用Rust实现的数字水印CLI工具，支持给图片和音频文件添加和提取数字水印。

## 特性

- 🖼️ **图片水印**: 支持JPG, PNG, BMP, GIF, TIFF, WebP等格式
- 🎵 **音频水印**: 支持WAV格式
- 🔢 **多种算法**: 
  - DCT (离散余弦变换)
  - DWT (离散小波变换)
- ⚡ **高性能**: 基于Rust，安全且高效
- 🎯 **精确控制**: 可调节水印强度

## 安装

### 从源码构建

确保你已经安装了Rust和Cargo，然后运行：

```bash
git clone <repository-url>
cd media-seal-rs
cargo build --release
```

编译后的可执行文件位于 `target/release/media-seal-rs`

## 使用方法

### 基本语法

```bash
media-seal-rs <COMMAND> [OPTIONS]
```

### 命令

#### 嵌入水印 (embed)

```bash
media-seal-rs embed -i <输入文件> -o <输出文件> -w <水印内容> [-a <算法>] [-s <强度>]
```

**参数说明:**
- `-i, --input <文件>`: 输入文件路径
- `-o, --output <文件>`: 输出文件路径  
- `-w, --watermark <文本>`: 水印内容
- `-a, --algorithm <算法>`: 使用的算法 (dct 或 dwt，默认: dct)
- `-s, --strength <强度>`: 水印强度 0.0-1.0 (默认: 0.1)
- `-v, --verbose`: 详细输出

**示例:**

```bash
# 给图片添加水印
media-seal-rs embed -i photo.jpg -o photo_watermarked.jpg -w "版权所有" -a dct -s 0.1

# 给音频添加水印
media-seal-rs embed -i audio.wav -o audio_watermarked.wav -w "我的音乐" -a dwt -s 0.05
```

#### 提取水印 (extract)

```bash
media-seal-rs extract -i <输入文件> [-a <算法>] [-o <输出文件>]
```

**参数说明:**
- `-i, --input <文件>`: 包含水印的文件路径
- `-a, --algorithm <算法>`: 使用的算法 (dct 或 dwt，默认: dct)
- `-o, --output <文件>`: 保存提取水印的文件 (可选)
- `-v, --verbose`: 详细输出

**示例:**

```bash
# 从图片提取水印
media-seal-rs extract -i photo_watermarked.jpg -a dct

# 从音频提取水印并保存到文件
media-seal-rs extract -i audio_watermarked.wav -a dwt -o extracted_watermark.txt
```

## 算法说明

### DCT (离散余弦变换)

- **优点**: 鲁棒性好，抗压缩能力强，高性能实现
- **适用**: 需要较强抗攻击能力的场景
- **特性**: 
  - 使用`rustdct`库，性能优异
  - 支持任意尺寸图像（自动填充）
  - 基于8×8块处理

### DWT (离散小波变换)

- **优点**: 多分辨率分析，局部化特性好
- **适用**: 需要保持图片质量的场景
- **要求**: 图片尺寸需要是2的幂

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

## 注意事项

1. **水印长度**: 确保水印文本长度适中，过长的水印可能影响嵌入效果
2. **强度设置**: 
   - 强度过低可能导致水印提取困难
   - 强度过高可能影响媒体质量
   - 建议范围: 0.05-0.2
3. **格式兼容**: DCT算法现已支持任意尺寸图像，DWT要求图像尺寸为2的幂
4. **质量保持**: 嵌入水印会轻微影响原始媒体质量

## 开发

### 项目结构

```
src/
├── main.rs          # CLI主程序
├── lib.rs           # 库入口
├── cli.rs           # 命令行参数定义
├── error.rs         # 错误处理
├── watermark/       # 水印算法模块
│   ├── mod.rs
│   ├── trait.rs     # 通用接口
│   ├── dct.rs       # DCT算法实现
│   └── dwt.rs       # DWT算法实现
└── media/           # 媒体处理模块
    ├── mod.rs
    ├── image.rs     # 图片处理
    └── audio.rs     # 音频处理
```

### 作为库使用

```rust
use media_seal_rs::prelude::*;

fn main() -> Result<()> {
    // 创建算法
    let algorithm = WatermarkFactory::create_algorithm(Algorithm::Dct)?;
    
    // 嵌入水印
    ImageWatermarker::embed_watermark(
        "input.jpg",
        "output.jpg",
        "我的水印",
        algorithm.as_ref(),
        0.1
    )?;
    
    // 提取水印
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

### v0.1.1 - DCT算法优化
- ✅ 使用`rustdct`库替换自实现的DCT算法
- ✅ 支持任意尺寸图像（自动填充处理）
- ✅ 显著提升DCT算法性能
- ✅ 改进错误提示信息

## TODO

- [ ] 支持更多图片格式
- [ ] 支持MP3等音频格式  
- [ ] 视频水印功能
- [ ] GUI界面
- [ ] 水印强度自动优化
- [ ] 批量处理功能
