use clap::Parser;
use colored::*;
use seal::prelude::*;
use serde_json::json;
use std::process;

fn main() -> Result<()> {
    // 确保 FFmpeg 可用
    if let Err(e) = ffmpeg_sidecar::download::auto_download() {
        eprintln!(
            "{} {}",
            "警告:".yellow().bold(),
            format!("无法下载 FFmpeg: {e}").red()
        );
        eprintln!("{}", "请确保系统中已安装 FFmpeg，或者检查网络连接".yellow());
    }

    let cli = Cli::parse();

    // 记录本次动作类型，便于错误时输出JSON
    let action_for_error = match &cli.command {
        Commands::Embed { .. } => "embed",
        Commands::Extract { .. } => "extract",
    };

    if let Err(e) = run(cli) {
        // 错误信息：stderr 打印人类可读，stdout 打印单行 JSON 便于机器解析
        let err_msg = e.to_string();
        eprintln!("{} {}", "错误:".red().bold(), err_msg.red());
        println!(
            "{}",
            json!({
                "status": "error",
                "action": action_for_error,
                "message": err_msg,
            })
        );
        process::exit(1);
    }
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    match &cli.command {
        Commands::Embed {
            input,
            output,
            watermark,
            algorithm,
            strength,
            lossless,
            video_mode,
        } => {
            if !MediaUtils::file_exists(input) {
                return Err(WatermarkError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("输入文件不存在: {input:?}"),
                )));
            }

            MediaUtils::ensure_output_dir(output)?;

            // 检测媒体类型
            let media_type = MediaUtils::detect_media_type(input)?;

            // 创建水印算法
            let watermark_algorithm = WatermarkFactory::create_algorithm(algorithm.clone());

            // 根据媒体类型选择处理方式
            let mut processed_frames_opt: Option<usize> = None;
            match media_type {
                MediaType::Image => {
                    if cli.verbose {
                        eprintln!(
                            "{} {}",
                            "🖼️  处理图片文件:".blue().bold(),
                            format!("{input:?}").cyan()
                        );

                        // 检查水印容量
                        if !ImageWatermarker::check_watermark_capacity(
                            input,
                            watermark,
                            watermark_algorithm.as_ref(),
                        )? {
                            eprintln!(
                                "{} {}",
                                "⚠️".yellow(),
                                "警告: 水印可能太长，可能影响嵌入效果".yellow()
                            );
                        }
                    }

                    ImageWatermarker::embed_watermark(
                        input,
                        output,
                        watermark,
                        watermark_algorithm.as_ref(),
                        *strength,
                    )?;
                }
                MediaType::Audio => {
                    if cli.verbose {
                        eprintln!(
                            "{} {}",
                            "🎧  处理音频文件:".blue().bold(),
                            format!("{input:?}").cyan()
                        );

                        // 检查水印容量
                        if !AudioWatermarker::check_watermark_capacity(
                            input,
                            watermark,
                            watermark_algorithm.as_ref(),
                        )? {
                            eprintln!(
                                "{} {}",
                                "⚠️".yellow(),
                                "警告: 水印可能太长，可能影响嵌入效果".yellow()
                            );
                        }
                    }

                    AudioWatermarker::embed_watermark(
                        input,
                        output,
                        watermark,
                        watermark_algorithm.as_ref(),
                        *strength,
                    )?;
                }
                MediaType::Video => {
                    if cli.verbose {
                        eprintln!(
                            "{} {}",
                            "🎥  处理视频文件:".blue().bold(),
                            format!("{input:?}").cyan()
                        );

                        // 检查水印容量
                        if !VideoWatermarker::check_watermark_capacity(
                            input,
                            watermark,
                            watermark_algorithm.as_ref(),
                        )? {
                            eprintln!(
                                "{} {}",
                                "⚠️".yellow(),
                                "警告: 水印可能太长，可能影响嵌入效果".yellow()
                            );
                        }
                    }

                    let processed_frames = VideoWatermarker::embed_watermark(
                        input,
                        output,
                        watermark,
                        watermark_algorithm.as_ref(),
                        *strength,
                        *lossless,
                        video_mode.clone(),
                    )?;
                    processed_frames_opt = Some(processed_frames);
                }
            }

            // 成功：stdout 打印单行 JSON
            let mut json_output = json!({
                "status": "success",
                "action": "embed",
                "input": input.display().to_string(),
                "output": output.display().to_string(),
                "algorithm": format!("{:?}", algorithm),
                "media_type": format!("{:?}", media_type),
                "strength": strength,
                "lossless": lossless,
            });

            // 对于视频类型，添加 video_mode 信息
            if matches!(media_type, MediaType::Video) {
                json_output["video_mode"] = json!(format!("{:?}", video_mode));
            }

            if let Some(n) = processed_frames_opt {
                json_output["processed_frames"] = json!(n);
            }

            println!("{}", json_output);
        }

        Commands::Extract {
            input,
            algorithm,
            length,
            output,
            sample_frames,
            confidence_threshold,
            video_mode,
        } => {
            // 检查输入文件是否存在
            if !MediaUtils::file_exists(input) {
                return Err(WatermarkError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("输入文件不存在: {input:?}"),
                )));
            }

            // 检测媒体类型
            let media_type = MediaUtils::detect_media_type(input)?;

            // 创建水印算法
            let watermark_algorithm = WatermarkFactory::create_algorithm(algorithm.clone());

            if cli.verbose {
                eprintln!(
                    "{} {}",
                    "🔍  从文件提取水印:".blue().bold(),
                    format!("{input:?}").cyan()
                );
                eprintln!(
                    "{} {}",
                    "🔧  使用算法:".blue().bold(),
                    format!("{algorithm:?}").cyan()
                );
            }

            let watermark_length = *length;

            // 根据媒体类型选择处理方式
            let (extracted_watermark, confidence, actual_frames_used) = match media_type {
                MediaType::Image => {
                    let watermark = ImageWatermarker::extract_watermark(
                        input,
                        watermark_algorithm.as_ref(),
                        watermark_length,
                    )?;
                    (watermark, 1.0, 1) // 图片始终置信度100%，使用1帧
                }
                MediaType::Audio => {
                    let watermark = AudioWatermarker::extract_watermark(
                        input,
                        watermark_algorithm.as_ref(),
                        watermark_length,
                    )?;
                    (watermark, 1.0, 1) // 音频始终置信度100%，使用1帧
                }
                MediaType::Video => VideoWatermarker::extract_watermark(
                    input,
                    watermark_algorithm.as_ref(),
                    watermark_length,
                    Some(*sample_frames),
                    Some(*confidence_threshold),
                    video_mode.clone(),
                )?,
            };

            // 输出到文件（如果指定）
            let mut saved_to: Option<String> = None;
            if let Some(output_path) = output {
                MediaUtils::ensure_output_dir(output_path)?;
                std::fs::write(output_path, &extracted_watermark)?;
                saved_to = Some(output_path.display().to_string());
                eprintln!(
                    "{} {}",
                    "💾".green(),
                    format!("提取的水印已保存到: {output_path:?}").green()
                );
            }

            // 成功：stdout 打印单行 JSON
            let mut json_output = json!({
                "status": "success",
                "action": "extract",
                "input": input.display().to_string(),
                "algorithm": format!("{:?}", algorithm),
                "media_type": format!("{:?}", media_type),
                "length": extracted_watermark.len(),
                "watermark": extracted_watermark,
                "output": saved_to,
            });

            // 对于视频类型，添加额外的质量信息和 video_mode
            if matches!(media_type, MediaType::Video) {
                json_output["confidence"] = json!(confidence);
                json_output["sample_frames_requested"] = json!(sample_frames);
                json_output["actual_frames_used"] = json!(actual_frames_used);
                json_output["confidence_threshold"] = json!(confidence_threshold);
                json_output["video_mode"] = json!(format!("{:?}", video_mode));
            }

            println!("{}", json_output);
        }
    }

    Ok(())
}
