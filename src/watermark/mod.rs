pub mod r#trait;
pub mod dct;
pub mod dwt;

pub use r#trait::{WatermarkAlgorithm, WatermarkUtils};
pub use dct::DctWatermark;
pub use dwt::DwtWatermark;

use crate::cli::Algorithm;
use crate::error::Result;
use std::sync::Arc;

/// 水印算法工厂
pub struct WatermarkFactory;

impl WatermarkFactory {
    /// 根据算法类型创建对应的水印算法实例
    pub fn create_algorithm(algorithm: Algorithm) -> Result<Arc<dyn WatermarkAlgorithm>> {
        match algorithm {
            Algorithm::Dct => Ok(Arc::new(DctWatermark::new())),
            Algorithm::Dwt => Ok(Arc::new(DwtWatermark::new())),
        }
    }

    /// 获取所有支持的算法列表
    pub fn supported_algorithms() -> Vec<&'static str> {
        vec!["DCT", "DWT"]
    }
} 