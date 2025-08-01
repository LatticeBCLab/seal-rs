use crate::error::{Result, WatermarkError};
use crate::watermark::dct::DctWatermark;
use crate::watermark::{WatermarkAlgorithm, WatermarkUtils};
use ffmpeg_sidecar::command::FfmpegCommand;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use ndarray::Array2;
use std::path::Path;

/// 音频水印处理器
pub struct AudioWatermarker;

impl AudioWatermarker {
    /// # 嵌入水印到音频中
    ///
    /// # 参数
    /// * `input_path` - 输入音频文件路径
    /// * `output_path` - 输出音频文件路径
    /// * `watermark_text` - 水印文本
    /// * `algorithm` - 水印算法
    /// * `strength` - 水印强度
    ///
    /// # 返回
    /// * `Ok(())` - 成功嵌入水印
    /// * `Err(WatermarkError)` - 嵌入水印失败
    pub fn embed_watermark<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
        strength: f64,
    ) -> Result<()> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        // 创建临时目录
        let temp_dir = std::env::temp_dir().join(format!("audio_watermark_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;

        // 使用ffmpeg转换为统一格式（16bit 44.1kHz 单声道 WAV）
        let normalized_audio = temp_dir.join("normalized.wav");
        Self::normalize_audio_format(input_path, &normalized_audio)?;

        // 读取标准化后的音频
        let mut reader = WavReader::open(&normalized_audio)?;
        let spec = reader.spec();

        // 读取音频样本
        let samples: Vec<f64> = reader
            .samples::<i16>()
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(|s| s as f64 / i16::MAX as f64)
            .collect();

        // 将水印文本转换为比特
        let watermark_bits = WatermarkUtils::string_to_bits(watermark_text);

        // 使用音频专用DCT算法，确保无噪声
        let ultra_low_strength = strength * 0.05; // 5%的强度，配合音频专用算法
        println!(
            "🔇 使用音频专用DCT水印：{ultra_low_strength:.4} (原始强度: {strength:.3})"
        );

        let watermarked_samples =
            Self::ultra_gentle_embed(&samples, &watermark_bits, algorithm, ultra_low_strength)?;

        // 创建临时水印音频文件
        let watermarked_temp = temp_dir.join("watermarked.wav");
        Self::write_wav(&watermarked_temp, &watermarked_samples, spec)?;

        // 使用ffmpeg转换回原始格式
        Self::convert_to_original_format(
            &watermarked_temp,
            &input_path.to_path_buf(),
            &output_path.to_path_buf(),
        )?;

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;

        println!("水印已成功嵌入到音频中: {output_path:?}");
        println!("使用算法: {}", algorithm.name());
        println!("水印内容: {watermark_text}");
        println!("嵌入强度: {strength}");

        Ok(())
    }

    /// 将音频标准化为统一格式
    fn normalize_audio_format<P: AsRef<Path>>(input_path: P, output_path: P) -> Result<()> {
        let mut command = FfmpegCommand::new();
        command
            .input(input_path.as_ref().to_str().unwrap())
            .args(["-ac", "1"]) // 转换为单声道
            .args(["-ar", "44100"]) // 采样率44.1kHz
            .args(["-acodec", "pcm_s16le"]) // 16位PCM
            .args(["-y"]) // 覆盖输出文件
            .output(output_path.as_ref().to_str().unwrap());

        let mut child = command.spawn().map_err(WatermarkError::Io)?;
        let status = child.wait().map_err(WatermarkError::Io)?;

        if !status.success() {
            return Err(WatermarkError::ProcessingError(
                "音频格式标准化失败".to_string(),
            ));
        }

        Ok(())
    }

    /// 准备样本以适应水印算法
    fn prepare_samples_for_watermarking(
        samples: &[f64],
        algorithm: &dyn WatermarkAlgorithm,
    ) -> Result<Vec<f64>> {
        let len = samples.len();
        let matrix_size = (len as f64).sqrt().ceil() as usize;

        let required_size = match algorithm.name() {
            name if name.contains("DCT") => {
                let adjusted_size = matrix_size.div_ceil(8) * 8;
                adjusted_size * adjusted_size
            }
            name if name.contains("DWT") => {
                let adjusted_size = matrix_size.next_power_of_two();
                adjusted_size * adjusted_size
            }
            _ => return Err(WatermarkError::Algorithm("未知算法".to_string())),
        };

        let mut prepared_samples = samples.to_vec();

        if prepared_samples.len() < required_size {
            // 使用零填充而不是重复填充，避免引入噪声
            prepared_samples.resize(required_size, 0.0);
        } else if prepared_samples.len() > required_size {
            prepared_samples.truncate(required_size);
        }

        Ok(prepared_samples)
    }

    /// 转换回原始格式
    fn convert_to_original_format<P: AsRef<Path>>(
        watermarked_path: P,
        _original_path: P,
        output_path: P,
    ) -> Result<()> {
        // 直接复制水印音频，保持WAV格式
        let mut command = FfmpegCommand::new();
        command
            .input(watermarked_path.as_ref().to_str().unwrap())
            .args(["-y"])
            .output(output_path.as_ref().to_str().unwrap());

        let mut child = command.spawn().map_err(WatermarkError::Io)?;
        let status = child.wait().map_err(WatermarkError::Io)?;

        if !status.success() {
            return Err(WatermarkError::ProcessingError(
                "音频格式转换失败".to_string(),
            ));
        }

        Ok(())
    }

    /// # 从音频中提取水印
    ///
    /// # 参数
    /// * `input_path` - 输入音频文件路径
    /// * `algorithm` - 水印算法
    /// * `watermark_length` - 期望的水印长度
    ///
    /// # 返回
    /// * `Ok(String)` - 提取的水印文本
    /// * `Err(WatermarkError)` - 提取水印失败
    pub fn extract_watermark<P: AsRef<Path>>(
        input_path: P,
        algorithm: &dyn WatermarkAlgorithm,
        watermark_length: usize,
    ) -> Result<String> {
        let input_path = input_path.as_ref();

        // 创建临时目录
        let temp_dir = std::env::temp_dir().join(format!("audio_extract_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;

        // 使用ffmpeg标准化音频格式
        let normalized_audio = temp_dir.join("normalized.wav");
        Self::normalize_audio_format(input_path, &normalized_audio)?;

        // 读取标准化后的音频文件
        let mut reader = WavReader::open(&normalized_audio)?;
        let _spec = reader.spec();

        // 读取音频样本
        let samples: Vec<f64> = reader
            .samples::<i16>()
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(|s| s as f64 / i16::MAX as f64)
            .collect();

        // 使用相同的音频专用DCT提取
        let extracted_bits = Self::ultra_gentle_extract(&samples, algorithm, watermark_length * 8)?;

        // 转换为字符串
        let watermark_text = WatermarkUtils::bits_to_string(&extracted_bits)?;

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;

        println!("水印提取完成:");
        println!("使用算法: {}", algorithm.name());
        println!("提取到的水印: {watermark_text}");

        Ok(watermark_text)
    }

    /// 将音频样本转换为二维数组
    fn audio_to_array(samples: &[f64]) -> Result<Array2<f64>> {
        let len = samples.len();

        // 找到最接近的完全平方数作为矩阵尺寸
        let size = (len as f64).sqrt().ceil() as usize;
        let matrix_size = size.next_power_of_two(); // 确保是2的幂，适用于DWT

        let mut array = Array2::<f64>::zeros((matrix_size, matrix_size));

        // 填充数组，不足的部分用0填充
        for (i, &sample) in samples.iter().enumerate() {
            if i >= matrix_size * matrix_size {
                break;
            }
            let row = i / matrix_size;
            let col = i % matrix_size;
            array[[row, col]] = sample;
        }

        Ok(array)
    }

    /// 将二维数组转换回音频样本
    fn array_to_audio(array: &Array2<f64>) -> Result<Vec<f64>> {
        let (rows, cols) = array.dim();
        let mut samples = Vec::new();

        // 首先收集所有原始样本
        for i in 0..rows {
            for j in 0..cols {
                samples.push(array[[i, j]]);
            }
        }

        // 应用专业的音频处理，避免硬限幅引起的失真
        Self::apply_professional_audio_limiting(&mut samples);

        Ok(samples)
    }

    /// 专业的音频限制处理，避免硬限幅失真
    fn apply_professional_audio_limiting(samples: &mut [f64]) {
        if samples.is_empty() {
            return;
        }

        // 1. 分析峰值分布
        let max_abs = samples.iter().map(|&x| x.abs()).fold(0.0f64, f64::max);

        if max_abs <= 1.0 {
            // 如果没有超限，直接返回
            return;
        }

        println!("检测到音频峰值超限 ({max_abs:.3})，应用专业音频处理");

        // 2. 使用软限制器而不是硬限幅
        let threshold = 0.95; // 软限制阈值
        let ratio = 0.2; // 压缩比，更温和的处理

        for sample in samples.iter_mut() {
            *sample = Self::soft_limiter(*sample, threshold, ratio);
        }

        // 3. 应用去加重滤波，减少高频失真
        Self::apply_deemphasis_filter(samples);

        // 4. 对开头应用特殊的平滑处理
        Self::smooth_audio_start(samples);
    }

    /// 软限制器 - 专业音频处理技术
    fn soft_limiter(input: f64, threshold: f64, ratio: f64) -> f64 {
        let abs_input = input.abs();
        let sign = if input >= 0.0 { 1.0 } else { -1.0 };

        if abs_input <= threshold {
            input
        } else {
            // 使用tanh软限制曲线，比硬限幅平滑得多
            let excess = abs_input - threshold;
            let compressed_excess = excess * ratio;
            let limited_excess = compressed_excess.tanh() * 0.05; // 很温和的限制
            sign * (threshold + limited_excess)
        }
    }

    /// 去加重滤波器，减少高频失真
    fn apply_deemphasis_filter(samples: &mut [f64]) {
        if samples.len() < 2 {
            return;
        }

        // 简单的去加重滤波器：y[n] = x[n] + 0.95 * y[n-1]
        let alpha = 0.95;
        let mut prev_output = 0.0;

        for sample in samples.iter_mut() {
            let current_input = *sample;
            let current_output = current_input + alpha * prev_output;
            *sample = current_output;
            prev_output = current_output;
        }

        // 应用归一化，避免滤波器引入的增益
        let max_after_filter = samples.iter().map(|&x| x.abs()).fold(0.0f64, f64::max);
        if max_after_filter > 0.98 {
            let normalize_factor = 0.95 / max_after_filter;
            for sample in samples.iter_mut() {
                *sample *= normalize_factor;
            }
        }
    }

    /// 对音频开头进行特殊平滑处理
    fn smooth_audio_start(samples: &mut [f64]) {
        let smooth_length = (samples.len() / 100).clamp(64, 2048); // 1%的长度，最少64样本，最多2048样本

        if samples.len() < smooth_length {
            return;
        }

        // 对开头应用Hann窗函数的前半部分，实现平滑启动
        for (i, sample) in samples.iter_mut().enumerate().take(smooth_length) {
            let window_pos = i as f64 / smooth_length as f64;
            let hann_factor = 0.5 * (1.0 - (2.0 * std::f64::consts::PI * window_pos).cos());
            *sample *= hann_factor;
        }
    }

    /// 写入WAV文件
    fn write_wav<P: AsRef<Path>>(path: P, samples: &[f64], spec: WavSpec) -> Result<()> {
        let mut writer = WavWriter::create(&path, spec)?;

        match spec.sample_format {
            SampleFormat::Float => {
                for &sample in samples.iter() {
                    writer.write_sample(sample as f32)?;
                }
            }
            SampleFormat::Int => {
                // 根据实际位数进行转换
                match spec.bits_per_sample {
                    16 => {
                        for &sample in samples.iter() {
                            let int_sample = (sample * i16::MAX as f64) as i16;
                            writer.write_sample(int_sample)?;
                        }
                    }
                    24 => {
                        for &sample in samples.iter() {
                            // 24位音频处理
                            let max_24bit = (1 << 23) - 1; // 2^23 - 1
                            let int_sample = (sample * max_24bit as f64) as i32;
                            writer.write_sample(int_sample)?;
                        }
                    }
                    32 => {
                        for &sample in samples.iter() {
                            let int_sample = (sample * i32::MAX as f64) as i32;
                            writer.write_sample(int_sample)?;
                        }
                    }
                    _ => {
                        return Err(WatermarkError::UnsupportedFormat(format!(
                            "不支持的位深度: {} bits",
                            spec.bits_per_sample
                        )));
                    }
                }
            }
        }

        writer.finalize()?;
        Ok(())
    }

    /// 获取音频文件信息
    pub fn get_audio_info<P: AsRef<Path>>(path: P) -> Result<WavSpec> {
        let reader = WavReader::open(&path)?;
        Ok(reader.spec())
    }

    /// 检查音频是否适合嵌入水印
    pub fn check_watermark_capacity<P: AsRef<Path>>(
        path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
    ) -> Result<bool> {
        // 读取音频文件获取样本数量
        let mut reader = WavReader::open(&path)?;
        let spec = reader.spec();
        let watermark_bits = WatermarkUtils::string_to_bits(watermark_text);

        // 计算总样本数
        let total_samples = match spec.sample_format {
            SampleFormat::Float => reader.samples::<f32>().count(),
            SampleFormat::Int => match spec.bits_per_sample {
                16 => reader.samples::<i16>().count(),
                24 | 32 => reader.samples::<i32>().count(),
                _ => {
                    return Err(WatermarkError::UnsupportedFormat(format!(
                        "不支持的位深度: {} bits",
                        spec.bits_per_sample
                    )));
                }
            },
        };

        let matrix_size = (total_samples as f64).sqrt().ceil() as usize;

        let capacity = match algorithm.name() {
            name if name.contains("DCT") => {
                let adjusted_size = matrix_size.div_ceil(8) * 8;
                (adjusted_size / 8) * (adjusted_size / 8)
            }
            name if name.contains("DWT") => {
                let adjusted_size = if matrix_size % 2 == 0 {
                    matrix_size
                } else {
                    matrix_size + 1
                };
                adjusted_size * adjusted_size / 4
            }
            _ => return Err(WatermarkError::Algorithm("未知算法".to_string())),
        };

        Ok(watermark_bits.len() <= capacity)
    }

    /// 调整音频格式以适应算法要求
    pub fn prepare_audio_for_algorithm<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        algorithm: &dyn WatermarkAlgorithm,
    ) -> Result<WavSpec> {
        let mut reader = WavReader::open(&input_path)?;
        let mut spec = reader.spec();

        // 转换为单声道
        if spec.channels != 1 {
            println!("将音频转换为单声道...");
            // 这里简化处理，实际应该实现立体声到单声道的转换
            spec.channels = 1;
        }

        // 读取样本并重新保存
        let samples: Vec<f64> = match spec.sample_format {
            SampleFormat::Float => reader
                .samples::<f32>()
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .map(|s| s as f64)
                .collect(),
            SampleFormat::Int => {
                // 根据位深度选择正确的整数类型
                match spec.bits_per_sample {
                    16 => reader
                        .samples::<i16>()
                        .collect::<std::result::Result<Vec<_>, _>>()?
                        .into_iter()
                        .map(|s| s as f64 / i16::MAX as f64)
                        .collect(),
                    24 | 32 => reader
                        .samples::<i32>()
                        .collect::<std::result::Result<Vec<_>, _>>()?
                        .into_iter()
                        .map(|s| {
                            if spec.bits_per_sample == 24 {
                                let max_24bit = (1 << 23) - 1;
                                s as f64 / max_24bit as f64
                            } else {
                                s as f64 / i32::MAX as f64
                            }
                        })
                        .collect(),
                    _ => {
                        return Err(WatermarkError::UnsupportedFormat(format!(
                            "不支持的位深度: {} bits",
                            spec.bits_per_sample
                        )));
                    }
                }
            }
        };

        // 调整样本数量以适应算法要求
        let len = samples.len();
        let matrix_size = (len as f64).sqrt().ceil() as usize;
        let required_size = match algorithm.name() {
            name if name.contains("DCT") => matrix_size.div_ceil(8) * 8, // 8的倍数
            name if name.contains("DWT") => {
                if matrix_size % 2 == 0 {
                    matrix_size
                } else {
                    matrix_size + 1
                }
            } // 偶数
            _ => return Err(WatermarkError::Algorithm("未知算法".to_string())),
        };

        let required_samples = required_size * required_size;
        let mut adjusted_samples = samples;

        if adjusted_samples.len() < required_samples {
            // 用零填充
            adjusted_samples.resize(required_samples, 0.0);
        } else if adjusted_samples.len() > required_samples {
            // 截断
            adjusted_samples.truncate(required_samples);
        }

        // 写入调整后的音频
        Self::write_wav(&output_path, &adjusted_samples, spec)?;

        println!("音频已调整格式以适应{}算法", algorithm.name());

        Ok(spec)
    }

    /// 超温和音频水印嵌入 - 使用专门的音频优化DCT算法
    fn ultra_gentle_embed(
        samples: &[f64],
        watermark_bits: &[u8],
        algorithm: &dyn WatermarkAlgorithm,
        strength: f64,
    ) -> Result<Vec<f64>> {
        println!("🎵 开始音频专用DCT水印嵌入，强度: {strength:.4}");

        // 检查是否是DCT算法，如果是则使用音频优化版本
        if algorithm.name() == "DCT" {
            // 使用专门的音频优化DCT算法
            let dct_algorithm = DctWatermark::new();
            let processed_samples = Self::prepare_samples_for_watermarking(samples, algorithm)?;
            let data = Self::audio_to_array(&processed_samples)?;

            // 调用音频优化的嵌入方法
            let watermarked_data =
                dct_algorithm.embed_audio_optimized(&data, watermark_bits, strength)?;
            let mut watermarked_samples = Self::array_to_audio(&watermarked_data)?;

            // 截断到原始长度
            if watermarked_samples.len() > samples.len() {
                watermarked_samples.truncate(samples.len());
            }

            // 应用轻量化的音频后处理
            Self::apply_minimal_audio_postprocessing(&mut watermarked_samples);

            println!("✅ 音频专用DCT水印嵌入完成");
            Ok(watermarked_samples)
        } else {
            // 对于非DCT算法，使用原来的流程
            let processed_samples = Self::prepare_samples_for_watermarking(samples, algorithm)?;
            let data = Self::audio_to_array(&processed_samples)?;
            let watermarked_data = algorithm.embed(&data, watermark_bits, strength)?;
            let mut watermarked_samples = Self::array_to_audio(&watermarked_data)?;

            if watermarked_samples.len() > samples.len() {
                watermarked_samples.truncate(samples.len());
            }

            Self::apply_ultra_smooth_audio_pipeline(&mut watermarked_samples, samples);
            println!("✅ 通用音频水印嵌入完成");
            Ok(watermarked_samples)
        }
    }

    /// 混合音频水印提取 - 音频专用嵌入但标准提取
    fn ultra_gentle_extract(
        samples: &[f64],
        algorithm: &dyn WatermarkAlgorithm,
        bit_count: usize,
    ) -> Result<Vec<u8>> {
        println!("🎵 开始混合音频水印提取（标准DCT提取）");

        // 无论什么算法，都使用标准提取流程
        // 因为嵌入时虽然用了音频专用算法，但基本的DCT位置是相同的
        let processed_samples = Self::prepare_samples_for_watermarking(samples, algorithm)?;
        let data = Self::audio_to_array(&processed_samples)?;
        let extracted_bits = algorithm.extract(&data, bit_count)?;

        println!("✅ 混合音频水印提取完成");
        Ok(extracted_bits)
    }

    /// 高级音频平滑处理流水线 - 彻底消除artifacts和噪声
    fn apply_ultra_smooth_audio_pipeline(
        watermarked_samples: &mut [f64],
        original_samples: &[f64],
    ) {
        if watermarked_samples.is_empty() || original_samples.is_empty() {
            return;
        }

        println!("🔧 应用高级音频平滑处理流水线...");

        // 第1步：全局动态范围分析与保护性归一化
        let max_abs = watermarked_samples
            .iter()
            .map(|&x| x.abs())
            .fold(0.0f64, f64::max);
        if max_abs > 0.99 {
            let protection_factor = 0.95 / max_abs;
            for sample in watermarked_samples.iter_mut() {
                *sample *= protection_factor;
            }
            println!("  📊 应用了保护性归一化，因子: {protection_factor:.4}");
        }

        // 第2步：温和的全局低通滤波，减少高频artifacts
        Self::apply_global_gentle_lowpass(watermarked_samples);

        // 第3步：自适应动态范围压缩
        Self::apply_adaptive_compression(watermarked_samples);

        // 第4步：边界平滑处理（开头和结尾）
        Self::apply_boundary_smoothing(watermarked_samples);

        // 第5步：最终的感知优化限制
        Self::apply_perceptual_limiting(watermarked_samples);

        println!("✅ 高级音频平滑处理完成");
    }

    /// 全局温和低通滤波
    fn apply_global_gentle_lowpass(samples: &mut [f64]) {
        if samples.len() < 3 {
            return;
        }

        // 使用非常温和的三点移动平均滤波器
        let alpha = 0.02; // 极小的滤波强度
        let mut filtered = samples.to_vec();

        for i in 1..samples.len() - 1 {
            let smoothed = (samples[i - 1] + samples[i] * 2.0 + samples[i + 1]) * 0.25;
            filtered[i] = samples[i] * (1.0 - alpha) + smoothed * alpha;
        }

        samples.copy_from_slice(&filtered);
        println!("  🎛️ 应用了全局温和低通滤波");
    }

    /// 自适应动态范围压缩
    fn apply_adaptive_compression(samples: &mut [f64]) {
        let window_size = 1024;
        let step_size = 512; // 50% overlap

        for start in (0..samples.len()).step_by(step_size) {
            let end = (start + window_size).min(samples.len());
            let window = &mut samples[start..end];

            // 计算窗口内的RMS
            let rms = (window.iter().map(|&x| x * x).sum::<f64>() / window.len() as f64).sqrt();

            if rms > 0.1 {
                // 只对相对较强的信号应用压缩
                let compression_ratio = 0.8 + 0.2 * (0.1 / rms).min(1.0);
                for sample in window.iter_mut() {
                    *sample *= compression_ratio;
                }
            }
        }

        println!("  🎚️ 应用了自适应动态范围压缩");
    }

    /// 边界平滑处理
    fn apply_boundary_smoothing(samples: &mut [f64]) {
        let fade_length = (samples.len() / 200).clamp(32, 512); // 0.5%的长度，32-512样本

        // 开头淡入
        for i in 0..fade_length.min(samples.len()) {
            let fade_factor = (i as f64 / fade_length as f64).powf(0.5); // 平方根曲线，更平滑
            samples[i] *= fade_factor;
        }

        // 结尾淡出
        let start_fade_out = samples.len().saturating_sub(fade_length);
        for i in start_fade_out..samples.len() {
            let fade_factor = ((samples.len() - i) as f64 / fade_length as f64).powf(0.5);
            samples[i] *= fade_factor;
        }

        println!("  🎭 应用了边界平滑处理，淡入淡出长度: {fade_length}样本");
    }

    /// 感知优化限制
    fn apply_perceptual_limiting(samples: &mut [f64]) {
        for sample in samples.iter_mut() {
            let abs_val = sample.abs();
            if abs_val > 0.95 {
                let sign = if *sample >= 0.0 { 1.0 } else { -1.0 };
                // 使用软限制曲线
                let excess = abs_val - 0.95;
                let limited_excess = excess.tanh() * 0.04; // 非常温和的限制
                *sample = sign * (0.95 + limited_excess);
            }
        }

        println!("  🔊 应用了感知优化限制");
    }

    /// 轻量化的音频后处理 - 专为音频优化DCT设计
    fn apply_minimal_audio_postprocessing(samples: &mut [f64]) {
        if samples.is_empty() {
            return;
        }

        println!("🔧 应用轻量化音频后处理...");

        // 第1步：保护性限制（很温和）
        let max_abs = samples.iter().map(|&x| x.abs()).fold(0.0f64, f64::max);
        if max_abs > 1.0 {
            let protection_factor = 0.98 / max_abs;
            for sample in samples.iter_mut() {
                *sample *= protection_factor;
            }
            println!("  📊 应用了保护性归一化，因子: {protection_factor:.4}");
        }

        // 第2步：极轻微的平滑处理
        Self::apply_ultra_light_smoothing(samples);

        // 第3步：边界柔化（很短的淡入淡出）
        Self::apply_light_boundary_softening(samples);

        println!("✅ 轻量化音频后处理完成");
    }

    /// 超轻微的平滑处理
    fn apply_ultra_light_smoothing(samples: &mut [f64]) {
        if samples.len() < 3 {
            return;
        }

        // 使用极轻微的三点平滑
        let alpha = 0.005; // 极小的平滑强度
        let mut smoothed = samples.to_vec();

        for i in 1..samples.len() - 1 {
            let avg = (samples[i - 1] + samples[i] + samples[i + 1]) / 3.0;
            smoothed[i] = samples[i] * (1.0 - alpha) + avg * alpha;
        }

        samples.copy_from_slice(&smoothed);
        println!("🎛️  应用了超轻微平滑处理");
    }

    /// 轻微的边界柔化
    fn apply_light_boundary_softening(samples: &mut [f64]) {
        let fade_length = (samples.len() / 500).clamp(16, 128); // 很短的淡入淡出

        // 开头轻微淡入
        for i in 0..fade_length.min(samples.len()) {
            let fade_factor = (i as f64 / fade_length as f64).sqrt();
            samples[i] *= fade_factor;
        }

        // 结尾轻微淡出
        let start_fade_out = samples.len().saturating_sub(fade_length);
        for i in start_fade_out..samples.len() {
            let fade_factor = ((samples.len() - i) as f64 / fade_length as f64).sqrt();
            samples[i] *= fade_factor;
        }

        println!("🎭 应用了轻微边界柔化，长度: {fade_length}样本");
    }
}
