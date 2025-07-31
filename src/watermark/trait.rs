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
    fn embed(&self, data: &Array2<f64>, watermark: &[u8], strength: f64) -> Result<Array2<f64>>;

    /// 从数据中提取水印
    ///
    /// # 参数
    /// * `data` - 包含水印的数据矩阵
    /// * `expected_length` - 期望的水印长度
    ///
    /// # 返回
    /// 提取的水印数据
    fn extract(&self, data: &Array2<f64>, expected_length: usize) -> Result<Vec<u8>>;

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

    /// 将二进制数据转换为字符串（严格模式）
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

        String::from_utf8(bytes).map_err(|_| crate::error::WatermarkError::InvalidWatermark)
    }

    /// 将二进制数据转换为字符串（宽松模式，用于调试）
    pub fn bits_to_string_lossy(bits: &[u8]) -> String {
        if bits.len() % 8 != 0 {
            return format!("[错误: 长度{}不是8的倍数]", bits.len());
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

        String::from_utf8_lossy(&bytes).to_string()
    }

    /// 分析提取的比特数据，提供调试信息
    pub fn analyze_extracted_bits(bits: &[u8]) -> String {
        let mut analysis = "比特数据分析:\n".to_string();
        analysis.push_str(&format!("- 总长度: {} 比特\n", bits.len()));
        analysis.push_str(&format!(
            "- 字节数: {} ({}完整)\n",
            bits.len() / 8,
            if bits.len() % 8 == 0 { "" } else { "不" }
        ));

        // 统计0和1的分布
        let ones = bits.iter().filter(|&&bit| bit == 1).count();
        let zeros = bits.len() - ones;
        analysis.push_str(&format!(
            "- 1的数量: {} ({:.1}%)\n",
            ones,
            ones as f32 * 100.0 / bits.len() as f32
        ));
        analysis.push_str(&format!(
            "- 0的数量: {} ({:.1}%)\n",
            zeros,
            zeros as f32 * 100.0 / bits.len() as f32
        ));

        // 尝试转换为字节并显示
        if bits.len() % 8 == 0 {
            analysis.push_str("- 字节值: [");
            for chunk in bits.chunks(8) {
                let mut byte = 0u8;
                for (i, &bit) in chunk.iter().enumerate() {
                    if bit != 0 {
                        byte |= 1 << (7 - i);
                    }
                }
                analysis.push_str(&format!("{}, ", byte));
            }
            analysis.push_str("]\n");

            // 尝试UTF-8转换
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

            match String::from_utf8(bytes.clone()) {
                Ok(string) => analysis.push_str(&format!("- UTF-8解码: '{}'\n", string)),
                Err(_) => analysis.push_str(&format!(
                    "- UTF-8解码失败，使用lossy: '{}'\n",
                    String::from_utf8_lossy(&bytes)
                )),
            }
        }

        analysis
    }

    /// 改进的水印提取，使用多数投票来提高鲁棒性
    pub fn extract_with_voting(
        algorithm: &dyn WatermarkAlgorithm,
        data: &Array2<f64>,
        expected_length: usize,
        vote_rounds: usize,
    ) -> Result<Vec<u8>> {
        if vote_rounds == 0 {
            return algorithm.extract(data, expected_length);
        }

        let mut vote_counts = vec![0i32; expected_length];

        // 进行多次提取
        for _ in 0..vote_rounds {
            match algorithm.extract(data, expected_length) {
                Ok(bits) => {
                    for (i, &bit) in bits.iter().enumerate() {
                        if i < vote_counts.len() {
                            if bit == 1 {
                                vote_counts[i] += 1;
                            } else {
                                vote_counts[i] -= 1;
                            }
                        }
                    }
                }
                Err(_) => continue,
            }
        }

        // 基于投票结果确定最终比特
        let final_bits: Vec<u8> = vote_counts
            .iter()
            .map(|&count| if count > 0 { 1 } else { 0 })
            .collect();

        Ok(final_bits)
    }
}
