use crate::error::{Result, WatermarkError};
use crate::watermark::r#trait::WatermarkAlgorithm;
use ndarray::{Array2, s};

/// DWT水印算法实现（使用Haar小波）
pub struct DwtWatermark {
    levels: usize,
}

impl DwtWatermark {
    /// 创建新的DWT水印算法实例
    pub fn new() -> Self {
        Self { levels: 1 }
    }

    /// 设置小波分解级数
    pub fn with_levels(mut self, levels: usize) -> Self {
        self.levels = levels;
        self
    }

    /// Haar小波前向变换（一维）
    fn haar_forward_1d(&self, data: &[f64]) -> Vec<f64> {
        let n = data.len();
        if n < 2 || (n & (n - 1)) != 0 {
            // 长度必须是2的幂
            return data.to_vec();
        }

        let mut result = vec![0.0; n];
        let half = n / 2;

        // 计算平均值（低频）和差值（高频）
        for i in 0..half {
            let sum = data[2 * i] + data[2 * i + 1];
            let diff = data[2 * i] - data[2 * i + 1];
            result[i] = sum / 2.0_f64.sqrt();        // 低频系数
            result[half + i] = diff / 2.0_f64.sqrt(); // 高频系数
        }

        result
    }

    /// Haar小波逆变换（一维）
    fn haar_inverse_1d(&self, data: &[f64]) -> Vec<f64> {
        let n = data.len();
        if n < 2 || (n & (n - 1)) != 0 {
            return data.to_vec();
        }

        let mut result = vec![0.0; n];
        let half = n / 2;

        // 从低频和高频系数重构原始信号
        for i in 0..half {
            let avg = data[i] / 2.0_f64.sqrt();
            let diff = data[half + i] / 2.0_f64.sqrt();
            result[2 * i] = avg + diff;
            result[2 * i + 1] = avg - diff;
        }

        result
    }

    /// 二维Haar小波前向变换
    fn haar_forward_2d(&self, data: &Array2<f64>) -> Array2<f64> {
        let (rows, cols) = data.dim();
        let mut result = data.clone();

        // 对每一行进行小波变换
        for i in 0..rows {
            let row: Vec<f64> = result.row(i).to_vec();
            let transformed_row = self.haar_forward_1d(&row);
            for j in 0..cols {
                result[[i, j]] = transformed_row[j];
            }
        }

        // 对每一列进行小波变换
        for j in 0..cols {
            let col: Vec<f64> = result.column(j).to_vec();
            let transformed_col = self.haar_forward_1d(&col);
            for i in 0..rows {
                result[[i, j]] = transformed_col[i];
            }
        }

        result
    }

    /// 二维Haar小波逆变换
    fn haar_inverse_2d(&self, data: &Array2<f64>) -> Array2<f64> {
        let (rows, cols) = data.dim();
        let mut result = data.clone();

        // 对每一列进行逆小波变换
        for j in 0..cols {
            let col: Vec<f64> = result.column(j).to_vec();
            let inverse_col = self.haar_inverse_1d(&col);
            for i in 0..rows {
                result[[i, j]] = inverse_col[i];
            }
        }

        // 对每一行进行逆小波变换
        for i in 0..rows {
            let row: Vec<f64> = result.row(i).to_vec();
            let inverse_row = self.haar_inverse_1d(&row);
            for j in 0..cols {
                result[[i, j]] = inverse_row[j];
            }
        }

        result
    }

    /// 多级小波分解
    fn multilevel_forward(&self, data: &Array2<f64>) -> Array2<f64> {
        let mut result = data.clone();
        let (mut rows, mut cols) = data.dim();

        for _ in 0..self.levels {
            if rows < 2 || cols < 2 {
                break;
            }

            // 对当前尺寸的左上角区域进行小波变换
            let subarray = result.slice(s![0..rows, 0..cols]).to_owned();
            let transformed = self.haar_forward_2d(&subarray);
            result.slice_mut(s![0..rows, 0..cols]).assign(&transformed);

            // 下一级只处理左上角的低频部分
            rows /= 2;
            cols /= 2;
        }

        result
    }

    /// 多级小波重构
    fn multilevel_inverse(&self, data: &Array2<f64>) -> Array2<f64> {
        let mut result = data.clone();
        let (orig_rows, orig_cols) = data.dim();

        // 计算各级的尺寸
        let mut level_sizes = Vec::new();
        let (mut rows, mut cols) = (orig_rows, orig_cols);
        for _ in 0..self.levels {
            level_sizes.push((rows, cols));
            rows /= 2;
            cols /= 2;
        }

        // 逆向重构
        for &(rows, cols) in level_sizes.iter().rev() {
            if rows < 2 || cols < 2 {
                continue;
            }

            let subarray = result.slice(s![0..rows, 0..cols]).to_owned();
            let reconstructed = self.haar_inverse_2d(&subarray);
            result.slice_mut(s![0..rows, 0..cols]).assign(&reconstructed);
        }

        result
    }

    /// 获取用于嵌入水印的高频系数位置
    fn get_high_freq_positions(&self, rows: usize, cols: usize) -> Vec<(usize, usize)> {
        let mut positions = Vec::new();
        let half_rows = rows / 2;
        let half_cols = cols / 2;

        // 在HH（对角高频）、HL（水平高频）、LH（垂直高频）子带中选择位置
        // HH子带（右下角）
        for i in half_rows..rows.min(half_rows + 4) {
            for j in half_cols..cols.min(half_cols + 4) {
                positions.push((i, j));
            }
        }

        // HL子带（右上角）
        for i in 0..half_rows.min(4) {
            for j in half_cols..cols.min(half_cols + 4) {
                positions.push((i, j));
            }
        }

        // LH子带（左下角）
        for i in half_rows..rows.min(half_rows + 4) {
            for j in 0..half_cols.min(4) {
                positions.push((i, j));
            }
        }

        positions
    }
}

impl Default for DwtWatermark {
    fn default() -> Self {
        Self::new()
    }
}

impl WatermarkAlgorithm for DwtWatermark {
    fn embed(
        &self,
        data: &Array2<f64>,
        watermark: &[u8],
        strength: f64,
    ) -> Result<Array2<f64>> {
        let (rows, cols) = data.dim();

        // 确保数据尺寸是2的幂
        if (rows & (rows - 1)) != 0 || (cols & (cols - 1)) != 0 {
            return Err(WatermarkError::InvalidArgument(
                "DWT要求数据尺寸是2的幂".to_string()
            ));
        }

        // 执行多级小波分解
        let mut dwt_data = self.multilevel_forward(data);

        // 获取高频系数位置
        let positions = self.get_high_freq_positions(rows, cols);

        if watermark.len() > positions.len() {
            return Err(WatermarkError::InvalidArgument(
                "水印数据太长，超过了可嵌入的位置数".to_string()
            ));
        }

        // 嵌入水印比特
        for (i, &bit) in watermark.iter().enumerate() {
            if i >= positions.len() {
                break;
            }

            let (row, col) = positions[i];
            if row < rows && col < cols {
                // 根据水印比特修改小波系数
                let coeff = dwt_data[[row, col]];
                if bit == 1 {
                    dwt_data[[row, col]] = coeff + strength * coeff.abs();
                } else {
                    dwt_data[[row, col]] = coeff - strength * coeff.abs();
                }
            }
        }

        // 执行逆小波变换
        let result = self.multilevel_inverse(&dwt_data);
        Ok(result)
    }

    fn extract(
        &self,
        data: &Array2<f64>,
        expected_length: usize,
    ) -> Result<Vec<u8>> {
        let (rows, cols) = data.dim();

        // 确保数据尺寸是2的幂
        if (rows & (rows - 1)) != 0 || (cols & (cols - 1)) != 0 {
            return Err(WatermarkError::InvalidArgument(
                "DWT要求数据尺寸是2的幂".to_string()
            ));
        }

        // 执行多级小波分解
        let dwt_data = self.multilevel_forward(data);

        // 获取高频系数位置
        let positions = self.get_high_freq_positions(rows, cols);

        if expected_length > positions.len() {
            return Err(WatermarkError::InvalidArgument(
                "期望长度超过了可提取的位置数".to_string()
            ));
        }

        let mut extracted_bits = Vec::new();

        // 提取水印比特
        for i in 0..expected_length {
            if i >= positions.len() {
                break;
            }

            let (row, col) = positions[i];
            if row < rows && col < cols {
                // 根据小波系数的符号确定比特值
                let coeff = dwt_data[[row, col]];
                let bit = if coeff >= 0.0 { 1 } else { 0 };
                extracted_bits.push(bit);
            }
        }

        Ok(extracted_bits)
    }

    fn name(&self) -> &'static str {
        "DWT"
    }
} 