use crate::error::{Result, WatermarkError};
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
        let original_sample_count = reader.duration();

        // 读取音频样本
        let samples: Vec<f64> = reader
            .samples::<i16>()
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .map(|s| s as f64 / i16::MAX as f64)
            .collect();

        // 确保样本数量符合算法要求，但保持原始长度信息
        let processed_samples = Self::prepare_samples_for_watermarking(&samples, algorithm)?;
        
        // 将音频转换为二维数组进行处理
        let data = Self::audio_to_array(&processed_samples)?;

        // 将水印文本转换为比特
        let watermark_bits = WatermarkUtils::string_to_bits(watermark_text);

        // 嵌入水印
        let watermarked_data = algorithm.embed(&data, &watermark_bits, strength)?;

        // 转换回音频格式，保持原始长度
        let mut watermarked_samples = Self::array_to_audio(&watermarked_data)?;
        
        // 截断到原始样本数量，避免时长变化
        if watermarked_samples.len() > original_sample_count as usize {
            watermarked_samples.truncate(original_sample_count as usize);
        }

        // 创建临时水印音频文件
        let watermarked_temp = temp_dir.join("watermarked.wav");
        Self::write_wav(&watermarked_temp, &watermarked_samples, spec)?;

        // 使用ffmpeg转换回原始格式
        Self::convert_to_original_format(&watermarked_temp, &input_path.to_path_buf(), &output_path.to_path_buf())?;

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;

        println!("水印已成功嵌入到音频中: {:?}", output_path);
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
            return Err(WatermarkError::ProcessingError("音频格式标准化失败".to_string()));
        }

        Ok(())
    }

    /// 准备样本以适应水印算法
    fn prepare_samples_for_watermarking(samples: &[f64], algorithm: &dyn WatermarkAlgorithm) -> Result<Vec<f64>> {
        let len = samples.len();
        let matrix_size = (len as f64).sqrt().ceil() as usize;
        
        let required_size = match algorithm.name() {
            name if name.contains("DCT") => {
                let adjusted_size = matrix_size.div_ceil(8) * 8;
                adjusted_size * adjusted_size
            },
            name if name.contains("DWT") => {
                let adjusted_size = matrix_size.next_power_of_two();
                adjusted_size * adjusted_size
            },
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
        output_path: P
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
            return Err(WatermarkError::ProcessingError("音频格式转换失败".to_string()));
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

        // 准备样本以适应算法
        let processed_samples = Self::prepare_samples_for_watermarking(&samples, algorithm)?;

        // 转换为ndarray
        let data = Self::audio_to_array(&processed_samples)?;

        // 提取水印比特
        let extracted_bits = algorithm.extract(&data, watermark_length * 8)?;

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

        for i in 0..rows {
            for j in 0..cols {
                let sample = array[[i, j]];
                // 限制音频样本值在合理范围内
                let clamped_sample = sample.clamp(-1.0, 1.0);
                samples.push(clamped_sample);
            }
        }

        Ok(samples)
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
}
