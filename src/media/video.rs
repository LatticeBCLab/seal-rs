use crate::error::{Result, WatermarkError};
use crate::watermark::WatermarkAlgorithm;
use ffmpeg_sidecar::command::FfmpegCommand;
use std::path::Path;

/// 视频水印处理器
pub struct VideoWatermarker;

impl VideoWatermarker {
    /// 嵌入水印到视频
    pub fn embed_watermark<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
        strength: f64,
        lossless: bool,
    ) -> Result<()> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        // 创建临时目录用于处理视频帧
        let temp_dir = std::env::temp_dir().join(format!("video_watermark_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;

        // 使用ffmpeg提取视频信息
        let video_info = Self::get_video_info(input_path)?;

        // 提取音频轨道（如果存在）
        let audio_path = temp_dir.join("audio.aac");
        if video_info.has_audio {
            Self::extract_audio(input_path, &audio_path)?;
        }

        // 提取视频帧
        let frames_dir = temp_dir.join("frames");
        std::fs::create_dir_all(&frames_dir)?;
        Self::extract_frames(input_path, &frames_dir)?;

        // 处理每一帧，添加水印
        let frame_files = Self::get_frame_files(&frames_dir)?;
        for frame_file in &frame_files {
            Self::process_frame(frame_file, watermark_text, algorithm, strength)?;
        }

        // 重新组合视频
        Self::reassemble_video(&frames_dir, &audio_path, output_path, &video_info, lossless)?;

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;

        Ok(())
    }

    /// 从视频中提取水印
    pub fn extract_watermark<P: AsRef<Path>>(
        input_path: P,
        algorithm: &dyn WatermarkAlgorithm,
        watermark_length: usize,
    ) -> Result<String> {
        let input_path = input_path.as_ref();

        // 创建临时目录
        let temp_dir = std::env::temp_dir().join(format!("video_extract_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;

        // 提取几帧进行水印提取（使用第10帧作为样本）
        let sample_frame = temp_dir.join("sample_frame.png");
        Self::extract_single_frame(input_path, &sample_frame, 1)?;

        // 使用图片水印提取算法
        use crate::media::ImageWatermarker;
        let watermark =
            ImageWatermarker::extract_watermark(&sample_frame, algorithm, watermark_length)?;

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;

        Ok(watermark)
    }

    /// 检查水印容量
    pub fn check_watermark_capacity<P: AsRef<Path>>(
        input_path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
    ) -> Result<bool> {
        // 提取一帧进行容量检查
        let temp_dir = std::env::temp_dir().join(format!("video_capacity_{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir)?;

        let sample_frame = temp_dir.join("sample_frame.png");
        Self::extract_single_frame(input_path.as_ref(), &sample_frame, 1)?;

        // 使用图片水印容量检查
        use crate::media::ImageWatermarker;
        let result =
            ImageWatermarker::check_watermark_capacity(&sample_frame, watermark_text, algorithm);

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;

        result
    }

    /// 获取视频信息
    fn get_video_info<P: AsRef<Path>>(input_path: P) -> Result<VideoInfo> {
        // 使用ffmpeg来检查文件流信息
        // 我们可以通过尝试提取音频来判断是否有音频轨道

        // 先检查是否是有效的视频文件
        // 尝试提取第一帧
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

        // 检查是否有音频：尝试提取音频
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

        // 清理临时文件
        std::fs::remove_dir_all(&temp_dir)?;

        Ok(VideoInfo {
            has_audio,
            has_video,
            duration: None, // 可以从ffmpeg输出中解析Duration信息
            fps: 30.0,      // 默认值，可以从ffmpeg输出中解析
        })
    }

    /// 提取音频
    fn extract_audio<P: AsRef<Path>>(input_path: P, output_path: P) -> Result<()> {
        let mut child = FfmpegCommand::new()
            .input(input_path.as_ref().to_str().unwrap())
            .args(["-vn"]) // 不包含视频
            .args(["-acodec", "copy"])
            .args(["-y"]) // 覆盖输出文件
            .output(output_path.as_ref().to_str().unwrap())
            .spawn()
            .map_err(WatermarkError::Io)?;

        let status = child.wait().map_err(WatermarkError::Io)?;

        if !status.success() {
            return Err(WatermarkError::ProcessingError("音频提取失败".to_string()));
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
            .args(["-vf", &format!("select=eq(n\\,{})", frame_number)])
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

        // 使用图片水印算法处理帧
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
