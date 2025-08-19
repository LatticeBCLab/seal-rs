use crate::cli::VideoWatermarkMode;
use crate::error::{Result, WatermarkError};
use crate::watermark::WatermarkAlgorithm;
use colored::*;
use ffmpeg_sidecar::command::FfmpegCommand;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;

/// # Video watermark processor
pub struct VideoWatermarker;

impl VideoWatermarker {
    /// # Embed watermark to video, return the number of processed frames
    pub fn embed_watermark<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
        strength: f64,
        lossless: bool,
        video_mode: VideoWatermarkMode,
    ) -> Result<usize> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        let video_info = Self::get_video_info(input_path)?;

        match video_mode {
            VideoWatermarkMode::Video => Self::embed_video_only(
                input_path,
                output_path,
                watermark_text,
                algorithm,
                strength,
                lossless,
                &video_info,
            ),
            VideoWatermarkMode::Audio => Self::embed_audio_only(
                input_path,
                output_path,
                watermark_text,
                algorithm,
                strength,
                &video_info,
            ),
            VideoWatermarkMode::Both => Self::embed_both(
                input_path,
                output_path,
                watermark_text,
                algorithm,
                strength,
                lossless,
                &video_info,
            ),
        }
    }

    pub fn extract_watermark<P: AsRef<Path>>(
        input_path: P,
        algorithm: &dyn WatermarkAlgorithm,
        watermark_length: usize,
        sample_frames: Option<usize>,
        confidence_threshold: Option<f64>,
        video_mode: VideoWatermarkMode,
    ) -> Result<(String, f64, usize)> {
        let input_path = input_path.as_ref();

        let video_info = Self::get_video_info(input_path)?;

        match video_mode {
            VideoWatermarkMode::Video => Self::extract_video_only(
                input_path,
                algorithm,
                watermark_length,
                sample_frames,
                confidence_threshold,
            ),
            VideoWatermarkMode::Audio => {
                Self::extract_audio_only(input_path, algorithm, watermark_length, &video_info)
            }
            VideoWatermarkMode::Both => Self::extract_both(
                input_path,
                algorithm,
                watermark_length,
                sample_frames,
                confidence_threshold,
                &video_info,
            ),
        }
    }

    /// # Check watermark capacity
    pub fn check_watermark_capacity<P: AsRef<Path>>(
        input_path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
    ) -> Result<bool> {
        // Extract a frame for capacity check
        let temp_dir = std::env::temp_dir().join(format!("video_capacity_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;

        let sample_frame = temp_dir.join("sample_frame.png");
        Self::extract_single_frame(input_path.as_ref(), &sample_frame, 1)?;

        // Use image watermark capacity check
        use crate::media::ImageWatermarker;
        let result =
            ImageWatermarker::check_watermark_capacity(&sample_frame, watermark_text, algorithm);

        // Clean up temporary files
        std::fs::remove_dir_all(&temp_dir)?;

        result
    }

    /// # Get video info
    fn get_video_info<P: AsRef<Path>>(input_path: P) -> Result<VideoInfo> {
        // Try to extract the first frame
        let temp_dir = std::env::temp_dir().join(format!("video_info_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;

        let test_frame = temp_dir.join("test_frame.png");
        let mut child = FfmpegCommand::new()
            .input(input_path.as_ref().to_str().unwrap())
            .args(["-vframes", "1"])
            .args(["-y"])
            .output(test_frame.to_str().unwrap())
            .spawn()
            .map_err(WatermarkError::Io)?;

        let status = child.wait().map_err(WatermarkError::Io)?;
        let has_video = status.success();

        if !has_video {
            std::fs::remove_dir_all(&temp_dir)?;
            return Err(WatermarkError::UnsupportedFormat(
                "输入文件不包含视频流".to_string(),
            ));
        }

        // Check if there is audio: try to extract audio
        let test_audio = temp_dir.join("test_audio.wav");
        let mut child = FfmpegCommand::new()
            .input(input_path.as_ref().to_str().unwrap())
            .args(["-vn"]) // 不包含视频
            .args(["-t", "0.1"]) // 只提取0.1秒
            .args(["-y"])
            .output(test_audio.to_str().unwrap())
            .spawn()
            .map_err(WatermarkError::Io)?;

        let audio_status = child.wait().map_err(WatermarkError::Io)?;
        let has_audio =
            audio_status.success() && test_audio.exists() && test_audio.metadata()?.len() > 0;

        // Remove temp dir
        std::fs::remove_dir_all(&temp_dir)?;

        Ok(VideoInfo {
            has_audio,
            has_video,
            duration: None, // 可以从ffmpeg输出中解析Duration信息
            fps: 30.0,      // 默认值，可以从ffmpeg输出中解析
        })
    }

    /// # Extract audio from video
    fn extract_audio<P: AsRef<Path>>(input_path: P, output_path: P) -> Result<()> {
        let input_str = input_path
            .as_ref()
            .to_str()
            .ok_or_else(|| WatermarkError::ProcessingError("输入路径包含无效字符".to_string()))?;
        let output_str = output_path
            .as_ref()
            .to_str()
            .ok_or_else(|| WatermarkError::ProcessingError("输出路径包含无效字符".to_string()))?;

        let mut child = FfmpegCommand::new()
            .input(input_str)
            .args(["-vn"]) // Do not include video
            .args(["-acodec", "pcm_s16le"]) // 使用无损PCM编码保护音频水印
            .args(["-y"]) // Overwrite output file
            .output(output_str)
            .spawn()
            .map_err(WatermarkError::Io)?;

        let status = child.wait().map_err(WatermarkError::Io)?;

        if !status.success() {
            return Err(WatermarkError::ProcessingError(format!(
                "音频提取失败: FFmpeg 命令执行失败, 错误码: {}",
                status.code().unwrap_or(-1)
            )));
        }

        Ok(())
    }

    /// 提取视频帧
    fn extract_frames<P: AsRef<Path>>(input_path: P, output_dir: P) -> Result<()> {
        let output_pattern = output_dir.as_ref().join("frame_%06d.png");

        let mut child = FfmpegCommand::new()
            .input(input_path.as_ref().to_str().unwrap())
            .args(["-vf", "fps=30"]) // 固定帧率
            .args(["-y"])
            .output(output_pattern.to_str().unwrap())
            .spawn()
            .map_err(WatermarkError::Io)?;

        let status = child.wait().map_err(WatermarkError::Io)?;

        if !status.success() {
            return Err(WatermarkError::ProcessingError(
                "视频帧提取失败".to_string(),
            ));
        }

        Ok(())
    }

    /// 提取单帧
    fn extract_single_frame<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        frame_number: u32,
    ) -> Result<()> {
        let mut child = FfmpegCommand::new()
            .input(input_path.as_ref().to_str().unwrap())
            .args(["-vf", &format!("select=eq(n\\,{frame_number})")])
            .args(["-vframes", "1"])
            .args(["-y"])
            .output(output_path.as_ref().to_str().unwrap())
            .spawn()
            .map_err(WatermarkError::Io)?;

        let status = child.wait().map_err(WatermarkError::Io)?;

        if !status.success() {
            return Err(WatermarkError::ProcessingError("单帧提取失败".to_string()));
        }

        Ok(())
    }

    /// 多帧采样提取水印
    fn extract_multiple_frames_watermark<P: AsRef<Path>>(
        input_path: P,
        temp_dir: &Path,
        algorithm: &dyn WatermarkAlgorithm,
        watermark_length: usize,
        sample_frames: usize,
    ) -> Result<Vec<(Vec<u8>, f64)>> {
        let mut results = Vec::new();
        use crate::media::ImageWatermarker;

        // 生成采样帧位置：跳过前5%帧，在剩余帧中均匀采样
        let skip_frames = 5; // 跳过前5帧避免编码问题
        let mut frame_indices = Self::generate_sample_frame_indices(
            sample_frames,
            skip_frames,
            skip_frames + sample_frames,
        );
        frame_indices.sort_unstable();
        frame_indices.dedup();
        // 控制最终抽样数量不超过请求值
        if frame_indices.len() > sample_frames {
            frame_indices.truncate(sample_frames);
        }

        for (i, &frame_idx) in frame_indices.iter().enumerate() {
            let frame_path = temp_dir.join(format!("sample_frame_{}.png", i));

            // 提取帧
            match Self::extract_single_frame(input_path.as_ref(), &frame_path, frame_idx as u32) {
                Ok(_) => {
                    // 确保帧文件真实生成
                    if !frame_path.exists() {
                        continue;
                    }
                    if let Ok(meta) = frame_path.metadata() {
                        if meta.len() == 0 {
                            let _ = std::fs::remove_file(&frame_path);
                            continue;
                        }
                    }
                    // 计算帧质量
                    let quality = match Self::assess_frame_quality(&frame_path) {
                        Ok(q) => q,
                        Err(_) => {
                            // 质量评估失败则跳过此帧
                            let _ = std::fs::remove_file(&frame_path);
                            continue;
                        }
                    };

                    // 提取水印
                    match ImageWatermarker::extract_watermark(
                        &frame_path,
                        algorithm,
                        watermark_length,
                    ) {
                        Ok(watermark_text) => {
                            // 将字符串转换为比特数组进行投票
                            let bits = Self::string_to_bits(&watermark_text, watermark_length);
                            results.push((bits, quality));
                        }
                        Err(_) => {
                            // 提取失败，跳过这一帧
                            let _ = std::fs::remove_file(&frame_path);
                            continue;
                        }
                    }
                }
                Err(_) => {
                    // 帧提取失败，跳过
                    continue;
                }
            }
        }

        if results.is_empty() {
            return Err(WatermarkError::ProcessingError(
                "所有采样帧的水印提取都失败".to_string(),
            ));
        }

        Ok(results)
    }

    /// 生成采样帧索引
    fn generate_sample_frame_indices(
        sample_count: usize,
        skip_frames: usize,
        max_frames: usize,
    ) -> Vec<usize> {
        if sample_count == 0 {
            return vec![];
        }

        let available_frames = max_frames.saturating_sub(skip_frames);
        if available_frames == 0 {
            return vec![skip_frames];
        }

        let mut indices = Vec::new();

        if sample_count == 1 {
            // 单帧情况，选择中间帧
            indices.push(skip_frames + available_frames / 2);
        } else {
            // 多帧情况，均匀分布
            for i in 0..sample_count {
                let frame_idx = skip_frames + (i * available_frames) / (sample_count - 1);
                indices.push(frame_idx.min(max_frames - 1));
            }
        }

        indices
    }

    /// 获取帧文件列表
    fn get_frame_files<P: AsRef<Path>>(frames_dir: P) -> Result<Vec<std::path::PathBuf>> {
        let mut frame_files = Vec::new();

        for entry in std::fs::read_dir(frames_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("png") {
                frame_files.push(path);
            }
        }

        frame_files.sort();
        Ok(frame_files)
    }

    /// 处理单帧
    fn process_frame<P: AsRef<Path>>(
        frame_path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
        strength: f64,
    ) -> Result<()> {
        use crate::media::ImageWatermarker;

        // 创建临时文件
        let temp_output = frame_path.as_ref().with_extension("tmp.png");

        // 使用静默模式的图片水印算法处理帧（不打印日志）
        ImageWatermarker::embed_watermark(
            frame_path.as_ref(),
            &temp_output,
            watermark_text,
            algorithm,
            strength,
        )?;

        // 替换原文件
        std::fs::rename(temp_output, frame_path)?;

        Ok(())
    }

    /// 帧质量评估（基于图像方差和清晰度）
    fn assess_frame_quality<P: AsRef<Path>>(frame_path: P) -> Result<f64> {
        use image::io::Reader as ImageReader;

        // 读取图像
        let img = ImageReader::open(frame_path.as_ref())
            .map_err(|e| WatermarkError::ProcessingError(format!("无法读取图像: {}", e)))?
            .decode()
            .map_err(|e| WatermarkError::ProcessingError(format!("无法解码图像: {}", e)))?;

        let gray = img.to_luma8();
        let (width, height) = gray.dimensions();

        // 计算图像方差（反映对比度）
        let mut sum = 0u64;
        let mut sum_sq = 0u64;
        let pixel_count = (width * height) as u64;

        for pixel in gray.as_raw() {
            let val = *pixel as u64;
            sum += val;
            sum_sq += val * val;
        }

        let mean = sum as f64 / pixel_count as f64;
        let variance = (sum_sq as f64 / pixel_count as f64) - (mean * mean);

        // 计算简单的清晰度指标（梯度幅度）
        let mut gradient_sum = 0f64;
        let gray_data = gray.as_raw();

        for y in 1..height.saturating_sub(1) {
            for x in 1..width.saturating_sub(1) {
                let idx = (y * width + x) as usize;
                let dx = gray_data[idx + 1] as f64 - gray_data[idx - 1] as f64;
                let dy =
                    gray_data[idx + width as usize] as f64 - gray_data[idx - width as usize] as f64;
                gradient_sum += (dx * dx + dy * dy).sqrt();
            }
        }

        let sharpness = gradient_sum / ((width - 2) * (height - 2)) as f64;

        // 综合质量分数（方差权重70%，清晰度权重30%）
        let quality = variance * 0.7 + sharpness * 0.3;

        Ok(quality)
    }

    /// 投票机制确定最终水印
    fn vote_watermark_bits(results: Vec<(Vec<u8>, f64)>, expected_length: usize) -> (String, f64) {
        if results.is_empty() {
            return (String::new(), 0.0);
        }

        let mut bit_votes = vec![Vec::new(); expected_length * 8]; // 每个字符8位

        // 收集所有帧的投票（按质量加权）
        for (bits, quality) in &results {
            for (i, &bit) in bits.iter().enumerate() {
                if i < bit_votes.len() {
                    bit_votes[i].push((bit, *quality));
                }
            }
        }

        // 对每个比特位进行加权投票
        let mut final_bits = Vec::new();
        let mut confidence_sum = 0.0;

        for votes in bit_votes {
            if votes.is_empty() {
                final_bits.push(0);
                continue;
            }

            let mut weight_0 = 0.0;
            let mut weight_1 = 0.0;
            let total_weight: f64 = votes.iter().map(|(_, w)| w).sum();

            for (bit, weight) in votes {
                if bit == 0 {
                    weight_0 += weight;
                } else {
                    weight_1 += weight;
                }
            }

            let winning_bit = if weight_1 > weight_0 { 1 } else { 0 };
            final_bits.push(winning_bit);

            // 计算置信度（获胜方的权重占比）
            let bit_confidence = weight_1.max(weight_0) / total_weight;
            confidence_sum += bit_confidence;
        }

        let overall_confidence = if final_bits.is_empty() {
            0.0
        } else {
            confidence_sum / final_bits.len() as f64
        };

        // 将比特转换回字符串
        let watermark_text = Self::bits_to_string(&final_bits, expected_length);

        (watermark_text, overall_confidence)
    }

    /// 字符串转比特数组
    fn string_to_bits(text: &str, expected_length: usize) -> Vec<u8> {
        let mut bits = Vec::new();
        let bytes = text.as_bytes();

        for i in 0..expected_length {
            let byte = if i < bytes.len() { bytes[i] } else { 0 };

            // 将每个字节转换为8个比特
            for bit_pos in 0..8 {
                let bit = (byte >> (7 - bit_pos)) & 1;
                bits.push(bit);
            }
        }

        bits
    }

    /// 比特数组转字符串
    fn bits_to_string(bits: &[u8], expected_length: usize) -> String {
        let mut bytes = Vec::new();

        // 每8个比特组成一个字节
        for chunk in bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                if bit != 0 {
                    byte |= 1 << (7 - i);
                }
            }
            bytes.push(byte);
        }

        // 截断到期望长度并转换为字符串
        bytes.truncate(expected_length);

        // 找到第一个null字符的位置
        if let Some(null_pos) = bytes.iter().position(|&b| b == 0) {
            bytes.truncate(null_pos);
        }

        String::from_utf8_lossy(&bytes).to_string()
    }

    /// 重新组合视频
    fn reassemble_video(
        frames_dir: &Path,
        audio_path: &Path,
        output_path: &Path,
        video_info: &VideoInfo,
        lossless: bool,
    ) -> Result<()> {
        let frame_pattern = frames_dir.join("frame_%06d.png");

        let mut command = FfmpegCommand::new();
        command.args(["-framerate", "30"]);
        command.input(frame_pattern.to_str().unwrap());

        // 如果有音频，添加音频输入
        if video_info.has_audio && audio_path.exists() {
            command.input(audio_path.to_str().unwrap());
            if lossless {
                command.args(["-c:v", "libx264", "-crf", "0", "-c:a", "copy"]);
                command.args(["-preset", "ultrafast"]); // 无损压缩时，使用ultrafast可以极大加快速度
            } else {
                command.args(["-c:v", "libx264", "-crf", "23", "-c:a", "copy"]);
                command.args(["-preset", "medium"]); // 有损压缩时，使用medium预设平衡质量和速度
            }
        } else if lossless {
            command.args(["-c:v", "libx264", "-crf", "0"]);
            command.args(["-preset", "ultrafast"]); // 无损压缩时，使用ultrafast可以极大加快速度
        } else {
            command.args(["-c:v", "libx264", "-crf", "23"]);
            command.args(["-preset", "medium"]); // 有损压缩时，使用medium预设平衡质量和速度
        }

        command.args(["-pix_fmt", "yuv420p"]);
        command.args(["-y"]);
        command.output(output_path.to_str().unwrap());

        let mut child = command.spawn().map_err(WatermarkError::Io)?;
        let status = child.wait().map_err(WatermarkError::Io)?;

        if !status.success() {
            return Err(WatermarkError::ProcessingError("视频重组失败".to_string()));
        }

        Ok(())
    }

    /// 仅对视频帧嵌入水印（原有逻辑）
    fn embed_video_only<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
        strength: f64,
        lossless: bool,
        video_info: &VideoInfo,
    ) -> Result<usize> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        // 创建总进度条
        let progress = ProgressBar::new(5);
        progress.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );

        // 创建临时目录用于处理视频帧
        progress.set_message("🗂️  创建临时目录".to_string());
        let temp_dir = std::env::temp_dir().join(format!("video_watermark_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;
        progress.inc(1);

        // 提取音频轨道（如果存在）
        let audio_path = temp_dir.join("audio.aac");
        if video_info.has_audio {
            progress.set_message("🎵  提取音频轨道".to_string());
            Self::extract_audio(input_path, &audio_path)?;
        }
        progress.inc(1);

        // 提取视频帧
        progress.set_message("🎬  提取视频帧".to_string());
        let frames_dir = temp_dir.join("frames");
        std::fs::create_dir_all(&frames_dir)?;
        Self::extract_frames(input_path, &frames_dir)?;
        progress.inc(1);

        // 处理每一帧，添加水印
        progress.set_message("🎯  处理视频帧".to_string());
        let frame_files = Self::get_frame_files(&frames_dir)?;

        // 创建帧处理进度条
        let frame_progress = ProgressBar::new(frame_files.len() as u64);
        frame_progress.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:30.yellow/red}] {pos}/{len} 帧",
                )
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );

        for frame_file in &frame_files {
            Self::process_frame(frame_file, watermark_text, algorithm, strength)?;
            frame_progress.inc(1);
        }
        frame_progress.finish_with_message(
            format!("✅ 已处理 {} 帧", frame_files.len())
                .green()
                .to_string(),
        );
        progress.inc(1);

        // 重新组合视频
        progress.set_message("🎞️  重新组合视频".to_string());
        Self::reassemble_video(&frames_dir, &audio_path, output_path, video_info, lossless)?;
        progress.inc(1);

        // 完成并清理
        progress.finish_with_message("🎉 视频水印嵌入完成!".green().bold().to_string());

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;
        eprintln!("{} {}", "🧹".blue(), "临时文件已清理".blue());

        Ok(frame_files.len())
    }

    /// # Embed watermark only to audio
    fn embed_audio_only<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
        strength: f64,
        video_info: &VideoInfo,
    ) -> Result<usize> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        if !video_info.has_audio {
            return Err(WatermarkError::ProcessingError(
                "视频文件不包含音频轨道，无法嵌入音频水印".to_string(),
            ));
        }

        // 创建总进度条
        let progress = ProgressBar::new(5);
        progress.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );

        // 创建临时目录
        progress.set_message("🗂️  创建临时目录".to_string());
        let temp_dir =
            std::env::temp_dir().join(format!("video_audio_watermark_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;
        progress.inc(1);

        // 提取音频轨道
        progress.set_message("🎵  提取音频轨道".to_string());
        let audio_path = temp_dir.join("original_audio.wav");
        Self::extract_audio_as_wav(input_path, &audio_path)?;
        progress.inc(1);

        // 对音频嵌入水印
        progress.set_message("🎯  处理音频水印".to_string());
        let watermarked_audio_path = temp_dir.join("watermarked_audio.wav");

        use crate::media::AudioWatermarker;
        AudioWatermarker::embed_watermark(
            &audio_path,
            &watermarked_audio_path,
            watermark_text,
            algorithm,
            strength,
        )?;
        progress.inc(1);

        // 提取视频流（无音频）
        progress.set_message("🎬  提取视频流".to_string());
        let video_no_audio_path = temp_dir.join("video_no_audio.mp4");
        Self::extract_video_stream(input_path, &video_no_audio_path)?;
        progress.inc(1);

        // 合并处理后的音频和原视频
        progress.set_message("🎞️  合并音视频".to_string());
        Self::merge_audio_video(
            &video_no_audio_path,
            &watermarked_audio_path,
            &output_path.to_path_buf(),
        )?;
        progress.inc(1);

        // 完成并清理
        progress.finish_with_message("🎉 音频水印嵌入完成!".green().bold().to_string());

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;
        eprintln!("{} {}", "🧹".blue(), "临时文件已清理".blue());

        Ok(1) // 音频作为单个流处理，返回1
    }

    /// 同时对视频帧和音频嵌入水印
    fn embed_both<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
        strength: f64,
        lossless: bool,
        video_info: &VideoInfo,
    ) -> Result<usize> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        // 创建总进度条
        let progress = ProgressBar::new(7);
        progress.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );

        // 创建临时目录
        progress.set_message("🗂️  创建临时目录".to_string());
        let temp_dir =
            std::env::temp_dir().join(format!("video_both_watermark_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;
        progress.inc(1);

        // 处理音频水印（如果有音频）
        let watermarked_audio_path = if video_info.has_audio {
            progress.set_message("🎵  提取并处理音频水印".to_string());
            let audio_path = temp_dir.join("original_audio.wav");
            Self::extract_audio_as_wav(input_path, &audio_path)?;

            let watermarked_audio_path = temp_dir.join("watermarked_audio.wav");
            use crate::media::AudioWatermarker;
            AudioWatermarker::embed_watermark(
                &audio_path,
                &watermarked_audio_path,
                watermark_text,
                algorithm,
                strength,
            )?;
            Some(watermarked_audio_path)
        } else {
            None
        };
        progress.inc(1);

        // 提取视频帧
        progress.set_message("🎬  提取视频帧".to_string());
        let frames_dir = temp_dir.join("frames");
        std::fs::create_dir_all(&frames_dir)?;
        Self::extract_frames(input_path, &frames_dir)?;
        progress.inc(1);

        // 处理每一帧，添加水印
        progress.set_message("🎯  处理视频帧水印".to_string());
        let frame_files = Self::get_frame_files(&frames_dir)?;

        // 创建帧处理进度条
        let frame_progress = ProgressBar::new(frame_files.len() as u64);
        frame_progress.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:30.yellow/red}] {pos}/{len} 帧",
                )
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );

        for frame_file in &frame_files {
            Self::process_frame(frame_file, watermark_text, algorithm, strength)?;
            frame_progress.inc(1);
        }
        frame_progress.finish_with_message(
            format!("✅ 已处理 {} 帧", frame_files.len())
                .green()
                .to_string(),
        );
        progress.inc(1);

        // 重新组合视频
        progress.set_message("🎞️  重新组合视频".to_string());
        if let Some(audio_path) = &watermarked_audio_path {
            Self::reassemble_video_with_custom_audio(
                &frames_dir,
                audio_path,
                output_path,
                lossless,
            )?;
        } else {
            Self::reassemble_video(
                &frames_dir,
                &temp_dir.join("dummy.aac"),
                output_path,
                video_info,
                lossless,
            )?;
        }
        progress.inc(1);

        // 完成并清理
        progress.finish_with_message("🎉 音视频水印嵌入完成!".green().bold().to_string());

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;
        eprintln!("{} {}", "🧹".blue(), "临时文件已清理".blue());

        Ok(frame_files.len())
    }

    /// # Extract audio as WAV format
    fn extract_audio_as_wav<P: AsRef<Path>>(input_path: P, output_path: P) -> Result<()> {
        let input_str = input_path
            .as_ref()
            .to_str()
            .ok_or_else(|| WatermarkError::ProcessingError("输入路径包含无效字符".to_string()))?;
        let output_str = output_path
            .as_ref()
            .to_str()
            .ok_or_else(|| WatermarkError::ProcessingError("输出路径包含无效字符".to_string()))?;

        let mut child = FfmpegCommand::new()
            .input(input_str)
            .args(["-vn"]) // 不包含视频
            .args(["-acodec", "pcm_s16le"]) // 转换为WAV格式
            .args(["-ar", "44100"]) // 采样率
            .args(["-y"]) // 覆盖输出文件
            .output(output_str)
            .spawn()
            .map_err(WatermarkError::Io)?;

        let status = child.wait().map_err(WatermarkError::Io)?;

        if !status.success() {
            return Err(WatermarkError::ProcessingError(format!(
                "音频提取失败: FFmpeg 命令执行失败, 错误码: {}",
                status.code().unwrap_or(-1)
            )));
        }

        Ok(())
    }

    /// 提取视频流（不包含音频）
    fn extract_video_stream<P: AsRef<Path>>(input_path: P, output_path: P) -> Result<()> {
        let input_str = input_path
            .as_ref()
            .to_str()
            .ok_or_else(|| WatermarkError::ProcessingError("输入路径包含无效字符".to_string()))?;
        let output_str = output_path
            .as_ref()
            .to_str()
            .ok_or_else(|| WatermarkError::ProcessingError("输出路径包含无效字符".to_string()))?;

        let mut child = FfmpegCommand::new()
            .input(input_str)
            .args(["-an"]) // 不包含音频
            .args(["-c:v", "copy"]) // 视频流复制
            .args(["-y"]) // 覆盖输出文件
            .output(output_str)
            .spawn()
            .map_err(WatermarkError::Io)?;

        let status = child.wait().map_err(WatermarkError::Io)?;

        if !status.success() {
            return Err(WatermarkError::ProcessingError(
                "视频流提取失败".to_string(),
            ));
        }

        Ok(())
    }

    /// 合并音频和视频
    fn merge_audio_video<P: AsRef<Path>>(
        video_path: P,
        audio_path: P,
        output_path: P,
    ) -> Result<()> {
        let video_str = video_path
            .as_ref()
            .to_str()
            .ok_or_else(|| WatermarkError::ProcessingError("视频路径包含无效字符".to_string()))?;
        let audio_str = audio_path
            .as_ref()
            .to_str()
            .ok_or_else(|| WatermarkError::ProcessingError("音频路径包含无效字符".to_string()))?;
        let output_str = output_path
            .as_ref()
            .to_str()
            .ok_or_else(|| WatermarkError::ProcessingError("输出路径包含无效字符".to_string()))?;

        let mut child = FfmpegCommand::new()
            .input(video_str)
            .input(audio_str)
            .args(["-c:v", "copy"]) // 视频流复制
            .args(["-c:a", "pcm_s16le"]) // 使用无损PCM编码保护音频水印
            .args(["-y"]) // 覆盖输出文件
            .output(output_str)
            .spawn()
            .map_err(WatermarkError::Io)?;

        let status = child.wait().map_err(WatermarkError::Io)?;

        if !status.success() {
            return Err(WatermarkError::ProcessingError(
                "音视频合并失败".to_string(),
            ));
        }

        Ok(())
    }

    /// 使用自定义音频重新组合视频
    fn reassemble_video_with_custom_audio(
        frames_dir: &Path,
        audio_path: &Path,
        output_path: &Path,
        lossless: bool,
    ) -> Result<()> {
        let frame_pattern = frames_dir.join("frame_%06d.png");

        let mut command = FfmpegCommand::new();
        command.args(["-framerate", "30"]);
        command.input(frame_pattern.to_str().unwrap());
        command.input(audio_path.to_str().unwrap());

        if lossless {
            command.args(["-c:v", "libx264", "-crf", "0", "-c:a", "pcm_s16le"]);
            command.args(["-preset", "ultrafast"]);
        } else {
            command.args(["-c:v", "libx264", "-crf", "23", "-c:a", "pcm_s16le"]);
            command.args(["-preset", "medium"]);
        }

        command.args(["-pix_fmt", "yuv420p"]);
        command.args(["-y"]);
        command.output(output_path.to_str().unwrap());

        let mut child = command.spawn().map_err(WatermarkError::Io)?;
        let status = child.wait().map_err(WatermarkError::Io)?;

        if !status.success() {
            return Err(WatermarkError::ProcessingError("视频重组失败".to_string()));
        }

        Ok(())
    }

    /// 仅从视频帧提取水印（原有逻辑）
    fn extract_video_only<P: AsRef<Path>>(
        input_path: P,
        algorithm: &dyn WatermarkAlgorithm,
        watermark_length: usize,
        sample_frames: Option<usize>,
        confidence_threshold: Option<f64>,
    ) -> Result<(String, f64, usize)> {
        let input_path = input_path.as_ref();
        let sample_frames = sample_frames.unwrap_or(7);
        let confidence_threshold = confidence_threshold.unwrap_or(0.6);

        // 创建提取进度条
        let progress = ProgressBar::new(4);
        progress.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );

        // 创建临时目录
        progress.set_message("🗂️  创建临时目录".to_string());
        let temp_dir = std::env::temp_dir().join(format!("video_extract_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;
        progress.inc(1);

        // 获取视频信息
        progress.set_message("📊  分析视频信息".to_string());
        let _video_info = Self::get_video_info(input_path)?;
        progress.inc(1);

        // 多帧采样提取
        progress.set_message(format!("🎬  提取{}个样本帧", sample_frames));
        let frame_results = Self::extract_multiple_frames_watermark(
            input_path,
            &temp_dir,
            algorithm,
            watermark_length,
            sample_frames,
        )?;
        let actual_frames_used = frame_results.len();
        progress.inc(1);

        // 投票机制确定最终结果
        progress.set_message("🗳️  多帧投票分析".to_string());
        let (final_watermark, confidence) =
            Self::vote_watermark_bits(frame_results, watermark_length);

        // 检查置信度
        if confidence < confidence_threshold {
            eprintln!(
                "{} 警告：提取置信度较低 ({:.1}%)，建议检查视频质量或增加采样帧数",
                "⚠️".yellow(),
                confidence * 100.0
            );
        }

        progress.inc(1);

        // 完成提取
        progress.finish_with_message(
            format!("🎉 视频水印提取完成! 置信度: {:.1}%", confidence * 100.0)
                .green()
                .bold()
                .to_string(),
        );

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;

        Ok((final_watermark, confidence, actual_frames_used))
    }

    /// 仅从音频提取水印
    fn extract_audio_only<P: AsRef<Path>>(
        input_path: P,
        algorithm: &dyn WatermarkAlgorithm,
        watermark_length: usize,
        video_info: &VideoInfo,
    ) -> Result<(String, f64, usize)> {
        let input_path = input_path.as_ref();

        if !video_info.has_audio {
            return Err(WatermarkError::ProcessingError(
                "视频文件不包含音频轨道，无法提取音频水印".to_string(),
            ));
        }

        // 创建提取进度条
        let progress = ProgressBar::new(4);
        progress.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );

        // 创建临时目录
        progress.set_message("🗂️  创建临时目录".to_string());
        let temp_dir =
            std::env::temp_dir().join(format!("video_audio_extract_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;
        progress.inc(1);

        // 提取音频轨道
        progress.set_message("🎵  提取音频轨道".to_string());
        let audio_path = temp_dir.join("extracted_audio.wav");
        Self::extract_audio_as_wav(input_path, &audio_path)?;
        progress.inc(1);

        // 从音频提取水印
        progress.set_message("🎯  提取音频水印".to_string());
        use crate::media::AudioWatermarker;
        let watermark =
            AudioWatermarker::extract_watermark(&audio_path, algorithm, watermark_length)?;
        progress.inc(1);

        progress.inc(1);

        // 完成提取
        progress.finish_with_message("🎉 音频水印提取完成!".green().bold().to_string());

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;

        Ok((watermark, 1.0, 1)) // 音频始终置信度100%，使用1帧
    }

    /// 同时从视频帧和音频提取水印，并进行融合
    fn extract_both<P: AsRef<Path>>(
        input_path: P,
        algorithm: &dyn WatermarkAlgorithm,
        watermark_length: usize,
        sample_frames: Option<usize>,
        confidence_threshold: Option<f64>,
        video_info: &VideoInfo,
    ) -> Result<(String, f64, usize)> {
        let input_path = input_path.as_ref();
        let sample_frames = sample_frames.unwrap_or(7);
        let confidence_threshold = confidence_threshold.unwrap_or(0.6);

        // 创建提取进度条
        let progress = ProgressBar::new(6);
        progress.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );

        // 创建临时目录
        progress.set_message("🗂️  创建临时目录".to_string());
        let temp_dir =
            std::env::temp_dir().join(format!("video_both_extract_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;
        progress.inc(1);

        // 从音频提取水印（如果有音频）
        let audio_result = if video_info.has_audio {
            progress.set_message("🎵  提取音频水印".to_string());
            let audio_path = temp_dir.join("extracted_audio.wav");
            Self::extract_audio_as_wav(input_path, &audio_path)?;

            use crate::media::AudioWatermarker;
            match AudioWatermarker::extract_watermark(&audio_path, algorithm, watermark_length) {
                Ok(watermark) => Some((watermark, 1.0)),
                Err(_) => None,
            }
        } else {
            None
        };
        progress.inc(1);

        // 从视频帧提取水印
        progress.set_message(format!("🎬  提取{}个样本帧", sample_frames));
        let frame_results = Self::extract_multiple_frames_watermark(
            input_path,
            &temp_dir,
            algorithm,
            watermark_length,
            sample_frames,
        )?;
        let actual_frames_used = frame_results.len();
        progress.inc(1);

        // 投票机制确定视频水印结果
        progress.set_message("🗳️  多帧投票分析".to_string());
        let (video_watermark, video_confidence) =
            Self::vote_watermark_bits(frame_results, watermark_length);
        progress.inc(1);

        // 融合音频和视频的结果
        progress.set_message("🔀  融合音视频水印结果".to_string());
        let (final_watermark, final_confidence) = match audio_result {
            Some((audio_watermark, audio_confidence)) => {
                // 如果音频和视频都有结果，选择置信度更高的
                if audio_confidence > video_confidence {
                    eprintln!(
                        "{} 选择音频水印结果（置信度: {:.1}%）",
                        "🎵".green(),
                        audio_confidence * 100.0
                    );
                    (audio_watermark, audio_confidence)
                } else {
                    eprintln!(
                        "{} 选择视频水印结果（置信度: {:.1}%）",
                        "🎬".green(),
                        video_confidence * 100.0
                    );
                    (video_watermark, video_confidence)
                }
            }
            None => {
                eprintln!("{} 仅使用视频水印结果", "🎬".blue());
                (video_watermark, video_confidence)
            }
        };
        progress.inc(1);

        // 检查置信度
        if final_confidence < confidence_threshold {
            eprintln!(
                "{} 警告：提取置信度较低 ({:.1}%)，建议检查媒体质量",
                "⚠️".yellow(),
                final_confidence * 100.0
            );
        }

        progress.inc(1);

        // 完成提取
        progress.finish_with_message(
            format!(
                "🎉 音视频水印提取完成! 置信度: {:.1}%",
                final_confidence * 100.0
            )
            .green()
            .bold()
            .to_string(),
        );

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;

        Ok((final_watermark, final_confidence, actual_frames_used))
    }
}

/// 视频信息结构
#[allow(dead_code)]
#[derive(Debug)]
struct VideoInfo {
    has_audio: bool,
    has_video: bool,
    duration: Option<f64>,
    fps: f64,
}
