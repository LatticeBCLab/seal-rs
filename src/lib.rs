//! # Media Seal RS
//! 
//! 一个用Rust实现的数字水印工具，支持给图片和音频文件添加和提取数字水印。
//! 
//! ## 支持的算法
//! 
//! - DCT (离散余弦变换)
//! - DWT (离散小波变换)
//! 
//! ## 支持的文件格式
//! 
//! - 图片: JPG, PNG, BMP, GIF, TIFF, WebP
//! - 音频: WAV
//! 
//! ## 使用示例
//! 
//! ```rust
//! use media_seal_rs::prelude::*;
//! 
//! // 嵌入水印到图片
//! let algorithm = WatermarkFactory::create_algorithm(Algorithm::Dct)?;
//! ImageWatermarker::embed_watermark(
//!     "input.jpg",
//!     "output.jpg", 
//!     "我的水印",
//!     algorithm.as_ref(),
//!     0.1
//! )?;
//! 
//! // 提取水印
//! let watermark = ImageWatermarker::extract_watermark(
//!     "output.jpg",
//!     algorithm.as_ref(),
//!     4 // "我的水印"的字符数
//! )?;
//! ```

pub mod error;
pub mod cli;
pub mod watermark;
pub mod media;

/// 便于使用的预导入模块
pub mod prelude {
    pub use crate::error::{Result, WatermarkError};
    pub use crate::cli::{Algorithm, Cli, Commands};
    pub use crate::watermark::{
        WatermarkAlgorithm, WatermarkUtils, WatermarkFactory,
        DctWatermark, DwtWatermark
    };
    pub use crate::media::{
        ImageWatermarker, AudioWatermarker, MediaUtils, MediaType
    };
} 