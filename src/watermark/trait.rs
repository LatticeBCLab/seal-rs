use crate::error::Result;
use ndarray::Array2;

/// 水印算法的通用接口
pub trait WatermarkAlgorithm {
    /// 嵌入水印到数据中
    /// 
    /// # 参数
    /// * `data` - 原始数据矩阵
    /// * `watermark` - 水印数据
    /// * `strength` - 水印强度 (0.0-1.0)
    /// 
    /// # 返回
    /// 包含水印的数据矩阵
    fn embed(
        &self,
        data: &Array2<f64>,
        watermark: &[u8],
        strength: f64,
    ) -> Result<Array2<f64>>;

    /// 从数据中提取水印
    /// 
    /// # 参数
    /// * `data` - 包含水印的数据矩阵
    /// * `expected_length` - 期望的水印长度
    /// 
    /// # 返回
    /// 提取的水印数据
    fn extract(
        &self,
        data: &Array2<f64>,
        expected_length: usize,
    ) -> Result<Vec<u8>>;

    /// 获取算法名称
    fn name(&self) -> &'static str;
}

/// 水印数据转换工具
pub struct WatermarkUtils;

impl WatermarkUtils {
    /// 将字符串转换为二进制数据
    pub fn string_to_bits(s: &str) -> Vec<u8> {
        let mut bits = Vec::new();
        for byte in s.bytes() {
            for i in (0..8).rev() {
                bits.push((byte >> i) & 1);
            }
        }
        bits
    }

    /// 将二进制数据转换为字符串
    pub fn bits_to_string(bits: &[u8]) -> Result<String> {
        if bits.len() % 8 != 0 {
            return Err(crate::error::WatermarkError::InvalidWatermark);
        }

        let mut bytes = Vec::new();
        for chunk in bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                if bit != 0 {
                    byte |= 1 << (7 - i);
                }
            }
            bytes.push(byte);
        }

        String::from_utf8(bytes)
            .map_err(|_| crate::error::WatermarkError::InvalidWatermark)
    }
} 