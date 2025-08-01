use crate::error::{Result, WatermarkError};
use crate::watermark::r#trait::WatermarkAlgorithm;
use ndarray::{s, Array2};
use rustdct::DctPlanner;

/// DCT水印算法实现 - 使用rustdct库
pub struct DctWatermark {
    block_size: usize,
    dct2_planner: DctPlanner<f64>,
    dct3_planner: DctPlanner<f64>,
}

impl DctWatermark {
    /// 创建新的DCT水印算法实例
    pub fn new() -> Self {
        Self {
            block_size: 8,
            dct2_planner: DctPlanner::new(),
            dct3_planner: DctPlanner::new(),
        }
    }

    /// 设置DCT块大小
    pub fn with_block_size(mut self, size: usize) -> Self {
        self.block_size = size;
        self
    }

    /// 将图像填充到块大小的倍数
    fn pad_to_block_size(&self, data: &Array2<f64>) -> Array2<f64> {
        let (height, width) = data.dim();
        let new_height = height.div_ceil(self.block_size) * self.block_size;
        let new_width = width.div_ceil(self.block_size) * self.block_size;

        if new_height == height && new_width == width {
            return data.clone();
        }

        let mut padded = Array2::<f64>::zeros((new_height, new_width));

        // 复制原始数据
        padded.slice_mut(s![0..height, 0..width]).assign(data);

        // 边缘镜像填充
        // 右边填充
        if new_width > width {
            for i in 0..height {
                for j in width..new_width {
                    let mirror_j = width - 1 - (j - width).min(width - 1);
                    padded[[i, j]] = padded[[i, mirror_j]];
                }
            }
        }

        // 下边填充
        if new_height > height {
            for i in height..new_height {
                for j in 0..new_width {
                    let mirror_i = height - 1 - (i - height).min(height - 1);
                    padded[[i, j]] = padded[[mirror_i, j]];
                }
            }
        }

        padded
    }

    /// 从填充的图像中提取原始尺寸
    fn unpad_from_block_size(
        &self,
        padded: &Array2<f64>,
        original_height: usize,
        original_width: usize,
    ) -> Array2<f64> {
        padded
            .slice(s![0..original_height, 0..original_width])
            .to_owned()
    }

    /// 执行2D DCT变换
    fn dct_2d(&mut self, block: &Array2<f64>) -> Array2<f64> {
        let (rows, cols) = block.dim();
        let mut result = block.clone();

        // 创建DCT-II计划
        let dct2 = self.dct2_planner.plan_dct2(cols);

        // 对每一行进行DCT
        for mut row in result.rows_mut() {
            let mut row_data: Vec<f64> = row.to_vec();
            dct2.process_dct2(&mut row_data);
            for (i, &val) in row_data.iter().enumerate() {
                row[i] = val;
            }
        }

        // 对每一列进行DCT
        let dct2_cols = self.dct2_planner.plan_dct2(rows);
        for j in 0..cols {
            let mut col_data: Vec<f64> = result.column(j).to_vec();
            dct2_cols.process_dct2(&mut col_data);
            for (i, &val) in col_data.iter().enumerate() {
                result[[i, j]] = val;
            }
        }

        result
    }

    /// 执行2D 逆DCT变换（使用DCT-III）
    fn idct_2d(&mut self, dct_block: &Array2<f64>) -> Array2<f64> {
        let (rows, cols) = dct_block.dim();
        let mut result = dct_block.clone();

        // 创建DCT-III计划
        let dct3_cols = self.dct3_planner.plan_dct3(rows);

        // 对每一列进行逆DCT（DCT-III）
        for j in 0..cols {
            let mut col_data: Vec<f64> = result.column(j).to_vec();
            dct3_cols.process_dct3(&mut col_data);
            for (i, &val) in col_data.iter().enumerate() {
                result[[i, j]] = val;
            }
        }

        // 对每一行进行逆DCT（DCT-III）
        let dct3 = self.dct3_planner.plan_dct3(cols);
        for mut row in result.rows_mut() {
            let mut row_data: Vec<f64> = row.to_vec();
            dct3.process_dct3(&mut row_data);
            for (i, &val) in row_data.iter().enumerate() {
                row[i] = val;
            }
        }

        // DCT-III需要除以2N来得到正确的逆变换
        result.mapv(|x| x / (2.0 * cols as f64))
    }

    /// 获取中频DCT系数的位置（适合嵌入水印）
    fn get_mid_frequency_positions(&self) -> Vec<(usize, usize)> {
        // 选择中频系数位置，避免低频（视觉重要）和高频（容易被压缩丢失）
        vec![
            (2, 1),
            (1, 2),
            (3, 1),
            (2, 2),
            (1, 3),
            (4, 1),
            (3, 2),
            (2, 3),
            (1, 4),
            (5, 1),
            (4, 2),
            (3, 3),
            (2, 4),
            (1, 5),
            (6, 1),
            (5, 2),
            (4, 3),
            (3, 4),
            (2, 5),
            (1, 6),
        ]
    }

    /// 计算块的方差用于感知加权
    fn calculate_block_variance(&self, block: &Array2<f64>) -> f64 {
        let mean = block.mean().unwrap_or(0.0);
        let variance =
            block.iter().map(|&x| (x - mean).powi(2)).sum::<f64>() / (block.len() as f64);
        variance
    }

    /// 计算自适应阈值
    fn calculate_adaptive_threshold(&self, dct_block: &Array2<f64>, base_strength: f64) -> f64 {
        let positions = self.get_mid_frequency_positions();
        let mut coeffs = Vec::new();

        for &(u, v) in &positions {
            if u < self.block_size && v < self.block_size {
                coeffs.push(dct_block[[u, v]].abs());
            }
        }

        if coeffs.is_empty() {
            return 2.0;
        }

        let mean_coeff = coeffs.iter().sum::<f64>() / coeffs.len() as f64;
        (mean_coeff * base_strength * 0.1).clamp(1.0, 5.0)
    }
}

impl Default for DctWatermark {
    fn default() -> Self {
        Self::new()
    }
}

impl WatermarkAlgorithm for DctWatermark {
    fn embed(&self, data: &Array2<f64>, watermark: &[u8], strength: f64) -> Result<Array2<f64>> {
        let original_height = data.nrows();
        let original_width = data.ncols();

        // 填充到块大小的倍数
        let padded_data = self.pad_to_block_size(data);
        let (height, width) = padded_data.dim();
        let mut result = padded_data.clone();

        let blocks_h = height / self.block_size;
        let blocks_w = width / self.block_size;
        let total_blocks = blocks_h * blocks_w;

        if watermark.len() > total_blocks {
            return Err(WatermarkError::InvalidArgument(format!(
                "水印数据太长，超过了可嵌入的块数。最大可嵌入{}比特，实际需要{}比特",
                total_blocks,
                watermark.len()
            )));
        }

        let positions = self.get_mid_frequency_positions();
        let mut watermark_idx = 0;
        let mut dct_algorithm = DctWatermark::new();

        for block_y in 0..blocks_h {
            for block_x in 0..blocks_w {
                if watermark_idx >= watermark.len() {
                    break;
                }

                // 提取当前块
                let start_y = block_y * self.block_size;
                let start_x = block_x * self.block_size;
                let end_y = start_y + self.block_size;
                let end_x = start_x + self.block_size;

                let block = padded_data
                    .slice(s![start_y..end_y, start_x..end_x])
                    .to_owned();

                // 执行DCT
                let mut dct_block = dct_algorithm.dct_2d(&block);

                // 嵌入水印比特
                let bit = watermark[watermark_idx];
                let pos_idx = watermark_idx % positions.len();
                let (u, v) = positions[pos_idx];

                if u < self.block_size && v < self.block_size {
                    // 条件符号嵌入法：智能选择温和调整或符号强制
                    let coeff = dct_block[[u, v]];
                    let magnitude = coeff.abs();

                    // 计算自适应阈值和感知加权
                    let adaptive_threshold =
                        self.calculate_adaptive_threshold(&dct_block, strength);
                    let block_variance = self.calculate_block_variance(&block);
                    let perceptual_weight = if block_variance < 10.0 { 0.5 } else { 1.0 };

                    let target_change = strength * magnitude.max(1.0) * perceptual_weight;

                    if bit == 1 {
                        // 目标：确保系数为正且足够大
                        if coeff + target_change >= adaptive_threshold {
                            // 温和增加就足够了，保持原有符号特性
                            dct_block[[u, v]] = coeff + target_change;
                        } else {
                            // 需要符号强制，但使用最小必要强度
                            dct_block[[u, v]] =
                                magnitude.max(adaptive_threshold) + target_change * 0.5;
                        }
                    } else {
                        // 目标：确保系数为负且绝对值够大
                        if coeff - target_change <= -adaptive_threshold {
                            // 温和减少就足够了，保持原有符号特性
                            dct_block[[u, v]] = coeff - target_change;
                        } else {
                            // 需要符号强制，但使用最小必要强度
                            dct_block[[u, v]] =
                                -(magnitude.max(adaptive_threshold) + target_change * 0.5);
                        }
                    }
                }

                // 执行逆DCT
                let watermarked_block = dct_algorithm.idct_2d(&dct_block);

                // 将修改后的块写回结果
                result
                    .slice_mut(s![start_y..end_y, start_x..end_x])
                    .assign(&watermarked_block);

                watermark_idx += 1;
            }
            if watermark_idx >= watermark.len() {
                break;
            }
        }

        // 移除填充，返回原始尺寸
        let final_result = self.unpad_from_block_size(&result, original_height, original_width);
        Ok(final_result)
    }

    fn extract(&self, data: &Array2<f64>, expected_length: usize) -> Result<Vec<u8>> {
        // 填充到块大小的倍数
        let padded_data = self.pad_to_block_size(data);
        let (height, width) = padded_data.dim();

        let blocks_h = height / self.block_size;
        let blocks_w = width / self.block_size;
        let total_blocks = blocks_h * blocks_w;

        if expected_length > total_blocks {
            return Err(WatermarkError::InvalidArgument(format!(
                "期望长度{expected_length}超过了可提取的块数{total_blocks}"
            )));
        }

        let positions = self.get_mid_frequency_positions();
        let mut extracted_bits = Vec::new();
        let mut dct_algorithm = DctWatermark::new();

        for block_y in 0..blocks_h {
            for block_x in 0..blocks_w {
                if extracted_bits.len() >= expected_length {
                    break;
                }

                // 提取当前块
                let start_y = block_y * self.block_size;
                let start_x = block_x * self.block_size;
                let end_y = start_y + self.block_size;
                let end_x = start_x + self.block_size;

                let block = padded_data
                    .slice(s![start_y..end_y, start_x..end_x])
                    .to_owned();

                // 执行DCT
                let dct_block = dct_algorithm.dct_2d(&block);

                // 提取水印比特
                let pos_idx = extracted_bits.len() % positions.len();
                let (u, v) = positions[pos_idx];

                if u < self.block_size && v < self.block_size {
                    // 根据DCT系数的符号确定比特值
                    let bit = if dct_block[[u, v]] >= 0.0 { 1 } else { 0 };
                    extracted_bits.push(bit);
                }
            }
            if extracted_bits.len() >= expected_length {
                break;
            }
        }

        extracted_bits.truncate(expected_length);
        Ok(extracted_bits)
    }

    fn name(&self) -> &'static str {
        "DCT"
    }
}

impl DctWatermark {
    /// 专为音频优化的温和水印嵌入方法
    pub fn embed_audio_optimized(
        &self,
        data: &Array2<f64>,
        watermark: &[u8],
        strength: f64,
    ) -> Result<Array2<f64>> {
        let original_height = data.nrows();
        let original_width = data.ncols();

        // 填充到块大小的倍数
        let padded_data = self.pad_to_block_size(data);
        let (height, width) = padded_data.dim();
        let mut result = padded_data.clone();

        let blocks_h = height / self.block_size;
        let blocks_w = width / self.block_size;
        let total_blocks = blocks_h * blocks_w;

        if watermark.len() > total_blocks {
            return Err(WatermarkError::InvalidArgument(format!(
                "水印数据太长，超过了可嵌入的块数。最大可嵌入{}比特，实际需要{}比特",
                total_blocks,
                watermark.len()
            )));
        }

        // 使用与标准DCT完全相同的位置，确保兼容性
        let audio_positions = self.get_mid_frequency_positions();
        let mut watermark_idx = 0;
        let mut dct_algorithm = DctWatermark::new();

        println!(
            "🎵 使用音频优化的DCT水印嵌入，块数: {}, 水印长度: {}",
            total_blocks,
            watermark.len()
        );

        for block_y in 0..blocks_h {
            for block_x in 0..blocks_w {
                if watermark_idx >= watermark.len() {
                    break;
                }

                // 提取当前块
                let start_y = block_y * self.block_size;
                let start_x = block_x * self.block_size;
                let end_y = start_y + self.block_size;
                let end_x = start_x + self.block_size;

                let block = padded_data
                    .slice(s![start_y..end_y, start_x..end_x])
                    .to_owned();

                // 执行DCT
                let mut dct_block = dct_algorithm.dct_2d(&block);

                // 使用音频友好的温和嵌入
                let bit = watermark[watermark_idx];
                let pos_idx = watermark_idx % audio_positions.len();
                let (u, v) = audio_positions[pos_idx];

                if u < self.block_size && v < self.block_size {
                    self.embed_audio_friendly_bit(&mut dct_block, u, v, bit, strength);
                }

                // 执行逆DCT
                let watermarked_block = dct_algorithm.idct_2d(&dct_block);

                // 将修改后的块写回结果
                result
                    .slice_mut(s![start_y..end_y, start_x..end_x])
                    .assign(&watermarked_block);

                watermark_idx += 1;
            }
            if watermark_idx >= watermark.len() {
                break;
            }
        }

        // 移除填充，返回原始尺寸
        let final_result = self.unpad_from_block_size(&result, original_height, original_width);
        Ok(final_result)
    }

    /// 专为音频优化的温和水印提取方法
    pub fn extract_audio_optimized(
        &self,
        data: &Array2<f64>,
        expected_length: usize,
    ) -> Result<Vec<u8>> {
        // 填充到块大小的倍数
        let padded_data = self.pad_to_block_size(data);
        let (height, width) = padded_data.dim();

        let blocks_h = height / self.block_size;
        let blocks_w = width / self.block_size;
        let total_blocks = blocks_h * blocks_w;

        if expected_length > total_blocks {
            return Err(WatermarkError::InvalidArgument(format!(
                "期望长度{expected_length}超过了可提取的块数{total_blocks}"
            )));
        }

        let audio_positions = self.get_mid_frequency_positions();
        let mut extracted_bits = Vec::new();
        let mut dct_algorithm = DctWatermark::new();

        println!("🎵 使用音频优化的DCT水印提取");

        for block_y in 0..blocks_h {
            for block_x in 0..blocks_w {
                if extracted_bits.len() >= expected_length {
                    break;
                }

                // 提取当前块
                let start_y = block_y * self.block_size;
                let start_x = block_x * self.block_size;
                let end_y = start_y + self.block_size;
                let end_x = start_x + self.block_size;

                let block = padded_data
                    .slice(s![start_y..end_y, start_x..end_x])
                    .to_owned();

                // 执行DCT
                let dct_block = dct_algorithm.dct_2d(&block);

                // 提取水印比特
                let pos_idx = extracted_bits.len() % audio_positions.len();
                let (u, v) = audio_positions[pos_idx];

                if u < self.block_size && v < self.block_size {
                    // 使用更稳健的提取逻辑
                    let bit = self.extract_audio_friendly_bit(&dct_block, u, v);
                    extracted_bits.push(bit);
                }
            }
            if extracted_bits.len() >= expected_length {
                break;
            }
        }

        extracted_bits.truncate(expected_length);
        Ok(extracted_bits)
    }

    /// 音频友好的温和比特嵌入
    fn embed_audio_friendly_bit(
        &self,
        dct_block: &mut Array2<f64>,
        u: usize,
        v: usize,
        bit: u8,
        strength: f64,
    ) {
        let coeff = dct_block[[u, v]];
        let magnitude = coeff.abs();

        // 音频专用的温和修改策略 - 确保与标准DCT兼容
        let audio_strength = strength * 1.0; // 使用完整强度，但采用温和的修改方式
        let min_threshold = 1.0; // 最小阈值

        // 计算目标变化量
        let base_change = audio_strength * magnitude.max(min_threshold);

        if bit == 1 {
            // 目标：确保系数为正，使用类似标准DCT但更温和的方式
            if coeff >= 0.0 {
                // 已经是正数，温和增加
                dct_block[[u, v]] = coeff + base_change * 0.3; // 30%的变化
            } else {
                // 是负数，需要变正，模拟标准DCT但更温和
                dct_block[[u, v]] = magnitude + base_change * 0.3;
            }
        } else {
            // 目标：确保系数为负
            if coeff <= 0.0 {
                // 已经是负数，温和减少
                dct_block[[u, v]] = coeff - base_change * 0.3; // 30%的变化
            } else {
                // 是正数，需要变负，模拟标准DCT但更温和
                dct_block[[u, v]] = -(magnitude + base_change * 0.3);
            }
        }
    }

    /// 音频友好的稳健比特提取
    fn extract_audio_friendly_bit(&self, dct_block: &Array2<f64>, u: usize, v: usize) -> u8 {
        let coeff = dct_block[[u, v]];

        // 使用简单的符号判断，与嵌入逻辑一致
        // 由于嵌入时修改很温和，提取时也要相应放宽
        if coeff >= 0.0 {
            1
        } else {
            0
        }
    }
}
