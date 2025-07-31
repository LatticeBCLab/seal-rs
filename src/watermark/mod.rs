pub mod dct;
pub mod r#trait;

pub use dct::DctWatermark;
pub use r#trait::{WatermarkAlgorithm, WatermarkUtils};

use crate::cli::Algorithm;
use std::sync::Arc;

/// 水印算法工厂
pub struct WatermarkFactory;

impl WatermarkFactory {
    /// 根据算法类型创建水印算法实例
    pub fn create_algorithm(algorithm: Algorithm) -> Arc<dyn WatermarkAlgorithm + Send + Sync> {
        match algorithm {
            Algorithm::Dct => Arc::new(DctWatermark::new()),
        }
    }
}
