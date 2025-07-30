use thiserror::Error;

/// 项目中的错误类型定义
#[derive(Error, Debug)]
pub enum WatermarkError {
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("图像处理错误: {0}")]
    Image(#[from] image::ImageError),

    #[error("音频处理错误: {0}")]
    Audio(#[from] hound::Error),

    #[error("不支持的文件格式: {0}")]
    UnsupportedFormat(String),

    #[error("无效的水印数据")]
    InvalidWatermark,

    #[error("水印提取失败")]
    ExtractionFailed,

    #[error("算法错误: {0}")]
    Algorithm(String),

    #[error("参数错误: {0}")]
    InvalidArgument(String),

    #[error("处理错误: {0}")]
    ProcessingError(String),
}

pub type Result<T> = std::result::Result<T, WatermarkError>; 