use crate::error::{Result, WatermarkError};
use crate::watermark::{WatermarkAlgorithm, WatermarkUtils};
use hound::{WavReader, WavWriter, WavSpec, SampleFormat};
use ndarray::Array2;
use std::path::Path;

/// 音频水印处理器
pub struct AudioWatermarker;

impl AudioWatermarker {
    /// 嵌入水印到音频中
    pub fn embed_watermark<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
        strength: f64,
    ) -> Result<()> {
        // 读取音频文件
        let mut reader = WavReader::open(&input_path)?;
        let spec = reader.spec();

        // 检查音频格式
        if spec.channels != 1 {
            return Err(WatermarkError::UnsupportedFormat(
                "目前只支持单声道音频".to_string()
            ));
        }

        // 读取音频样本
        let samples: Vec<f64> = match spec.sample_format {
            SampleFormat::Float => {
                reader.samples::<f32>()
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|s| s as f64)
                    .collect()
            }
            SampleFormat::Int => {
                reader.samples::<i32>()
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|s| s as f64 / i32::MAX as f64)
                    .collect()
            }
        };

        // 将音频转换为二维数组进行处理
        let data = Self::audio_to_array(&samples)?;

        // 将水印文本转换为比特
        let watermark_bits = WatermarkUtils::string_to_bits(watermark_text);

        // 嵌入水印
        let watermarked_data = algorithm.embed(&data, &watermark_bits, strength)?;

        // 转换回音频格式
        let watermarked_samples = Self::array_to_audio(&watermarked_data)?;

        // 写入音频文件
        Self::write_wav(&output_path, &watermarked_samples, spec)?;

        println!("水印已成功嵌入到音频中: {:?}", output_path.as_ref());
        println!("使用算法: {}", algorithm.name());
        println!("水印内容: {}", watermark_text);
        println!("嵌入强度: {}", strength);

        Ok(())
    }

    /// 从音频中提取水印
    pub fn extract_watermark<P: AsRef<Path>>(
        input_path: P,
        algorithm: &dyn WatermarkAlgorithm,
        watermark_length: usize,
    ) -> Result<String> {
        // 读取音频文件
        let mut reader = WavReader::open(&input_path)?;
        let spec = reader.spec();

        if spec.channels != 1 {
            return Err(WatermarkError::UnsupportedFormat(
                "目前只支持单声道音频".to_string()
            ));
        }

        // 读取音频样本
        let samples: Vec<f64> = match spec.sample_format {
            SampleFormat::Float => {
                reader.samples::<f32>()
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|s| s as f64)
                    .collect()
            }
            SampleFormat::Int => {
                reader.samples::<i32>()
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|s| s as f64 / i32::MAX as f64)
                    .collect()
            }
        };

        // 转换为ndarray
        let data = Self::audio_to_array(&samples)?;

        // 提取水印比特
        let extracted_bits = algorithm.extract(&data, watermark_length * 8)?;

        // 转换为字符串
        let watermark_text = WatermarkUtils::bits_to_string(&extracted_bits)?;

        println!("水印提取完成:");
        println!("使用算法: {}", algorithm.name());
        println!("提取到的水印: {}", watermark_text);

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
                let clamped_sample = sample.max(-1.0).min(1.0);
                samples.push(clamped_sample);
            }
        }

        Ok(samples)
    }

    /// 写入WAV文件
    fn write_wav<P: AsRef<Path>>(
        path: P,
        samples: &[f64],
        spec: WavSpec,
    ) -> Result<()> {
        let mut writer = WavWriter::create(&path, spec)?;

        match spec.sample_format {
            SampleFormat::Float => {
                for &sample in samples.iter() {
                    writer.write_sample(sample as f32)?;
                }
            }
            SampleFormat::Int => {
                for &sample in samples.iter() {
                    let int_sample = (sample * i32::MAX as f64) as i32;
                    writer.write_sample(int_sample)?;
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
            SampleFormat::Float => {
                reader.samples::<f32>().count()
            }
            SampleFormat::Int => {
                reader.samples::<i32>().count()
            }
        };

        let matrix_size = (total_samples as f64).sqrt().ceil() as usize;
        let matrix_size = matrix_size.next_power_of_two();

        let capacity = match algorithm.name() {
            "DCT" => {
                let blocks = (matrix_size / 8) * (matrix_size / 8);
                blocks
            }
            "DWT" => {
                let coeffs = matrix_size * matrix_size / 4;
                coeffs
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
            SampleFormat::Float => {
                reader.samples::<f32>()
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|s| s as f64)
                    .collect()
            }
            SampleFormat::Int => {
                reader.samples::<i32>()
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .into_iter()
                    .map(|s| s as f64 / i32::MAX as f64)
                    .collect()
            }
        };

        // 调整样本数量以适应算法要求
        let len = samples.len();
        let matrix_size = (len as f64).sqrt().ceil() as usize;
        let required_size = match algorithm.name() {
            "DCT" => ((matrix_size + 7) / 8) * 8, // 8的倍数
            "DWT" => matrix_size.next_power_of_two(), // 2的幂
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