pub mod image;
pub mod audio;

pub use image::ImageWatermarker;
pub use audio::AudioWatermarker;

use crate::error::{Result, WatermarkError};
use std::path::Path;

/// 媒体文件类型检测
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Image,
    Audio,
    Video, // 预留，暂未实现
}

/// 媒体处理工具
pub struct MediaUtils;

impl MediaUtils {
    /// 根据文件扩展名检测媒体类型
    pub fn detect_media_type<P: AsRef<Path>>(path: P) -> Result<MediaType> {
        let path = path.as_ref();
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .ok_or_else(|| WatermarkError::UnsupportedFormat("无法确定文件类型".to_string()))?;

        match extension.as_str() {
            "jpg" | "jpeg" | "png" | "bmp" | "gif" | "tiff" | "webp" => Ok(MediaType::Image),
            "wav" | "wave" => Ok(MediaType::Audio),
            "mp4" | "avi" | "mov" | "mkv" => Ok(MediaType::Video),
            _ => Err(WatermarkError::UnsupportedFormat(format!(
                "不支持的文件格式: {}", extension
            ))),
        }
    }

    /// 获取支持的图片格式列表
    pub fn supported_image_formats() -> Vec<&'static str> {
        vec!["jpg", "jpeg", "png", "bmp", "gif", "tiff", "webp"]
    }

    /// 获取支持的音频格式列表
    pub fn supported_audio_formats() -> Vec<&'static str> {
        vec!["wav", "wave"]
    }

    /// 获取支持的视频格式列表（预留）
    pub fn supported_video_formats() -> Vec<&'static str> {
        vec!["mp4", "avi", "mov", "mkv"]
    }

    /// 检查文件是否存在
    pub fn file_exists<P: AsRef<Path>>(path: P) -> bool {
        path.as_ref().exists()
    }

    /// 创建输出目录（如果不存在）
    pub fn ensure_output_dir<P: AsRef<Path>>(path: P) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        Ok(())
    }
} 