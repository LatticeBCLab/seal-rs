pub mod cli;
pub mod error;
pub mod media;
pub mod watermark;

/// 便于使用的预导入模块
pub mod prelude {
    pub use crate::cli::{Algorithm, Cli, Commands};
    pub use crate::error::{Result, WatermarkError};
    pub use crate::media::{
        AudioWatermarker, ImageWatermarker, MediaType, MediaUtils, VideoWatermarker,
    };
    pub use crate::watermark::{
        DctWatermark, WatermarkAlgorithm, WatermarkFactory, WatermarkUtils,
    };
}
