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

        // DCT-III已经是正确缩放的逆变换，不需要额外除法
        result
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
                    // 使用符号嵌入法：确保系数符号与水印比特对应
                    let coeff = dct_block[[u, v]];
                    let min_strength = 10.0; // 最小强度值，确保系数有足够的幅度
                    let magnitude = coeff.abs().max(min_strength);

                    if bit == 1 {
                        // bit=1时，强制系数为正数
                        dct_block[[u, v]] = magnitude + strength * magnitude;
                    } else {
                        // bit=0时，强制系数为负数
                        dct_block[[u, v]] = -(magnitude + strength * magnitude);
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
        "DCT (rustdct)"
    }
}
