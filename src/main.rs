use clap::Parser;
use colored::*;
use seal::prelude::*;
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

    if let Err(e) = run(cli) {
        eprintln!("{} {}", "错误:".red().bold(), e.to_string().red());
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
            match media_type {
                MediaType::Image => {
                    if cli.verbose {
                        println!(
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
                            println!(
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
                        println!(
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
                            println!(
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
                        println!(
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
                            println!(
                                "{} {}",
                                "⚠️".yellow(),
                                "警告: 水印可能太长，可能影响嵌入效果".yellow()
                            );
                        }
                    }

                    VideoWatermarker::embed_watermark(
                        input,
                        output,
                        watermark,
                        watermark_algorithm.as_ref(),
                        *strength,
                        *lossless,
                    )?;
                }
            }

            if cli.verbose {
                println!("{} {}", "✅".green(), "水印嵌入完成!".green().bold());
            }
        }

        Commands::Extract {
            input,
            algorithm,
            length,
            output,
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
                println!(
                    "{} {}",
                    "🔍  从文件提取水印:".blue().bold(),
                    format!("{input:?}").cyan()
                );
                println!(
                    "{} {}",
                    "🔧  使用算法:".blue().bold(),
                    format!("{algorithm:?}").cyan()
                );
            }

            let watermark_length = *length;

            // 根据媒体类型选择处理方式
            let extracted_watermark = match media_type {
                MediaType::Image => ImageWatermarker::extract_watermark(
                    input,
                    watermark_algorithm.as_ref(),
                    watermark_length,
                )?,
                MediaType::Audio => AudioWatermarker::extract_watermark(
                    input,
                    watermark_algorithm.as_ref(),
                    watermark_length,
                )?,
                MediaType::Video => VideoWatermarker::extract_watermark(
                    input,
                    watermark_algorithm.as_ref(),
                    watermark_length,
                )?,
            };

            // 输出到文件（如果指定）
            if let Some(output_path) = output {
                MediaUtils::ensure_output_dir(output_path)?;
                std::fs::write(output_path, &extracted_watermark)?;
                println!(
                    "{} {}",
                    "💾".green(),
                    format!("提取的水印已保存到: {output_path:?}").green()
                );
            }

            // 总是在控制台显示结果
            println!("\n{}", "=== 提取结果 ===".cyan().bold());
            println!(
                "{} {}",
                "📜  水印内容:".blue().bold(),
                extracted_watermark.green()
            );
            println!(
                "{} {}",
                "📊  水印长度:".blue().bold(),
                format!("{} 字符", extracted_watermark.len()).yellow()
            );

            if cli.verbose {
                println!("{} {}", "✅".green(), "水印提取完成!".green().bold());
            }
        }
    }

    Ok(())
}
