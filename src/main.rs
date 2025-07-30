use clap::Parser;
use media_seal_rs::prelude::*;
use std::process;

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("错误: {}", e);
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
        } => {
            if !MediaUtils::file_exists(input) {
                return Err(WatermarkError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("输入文件不存在: {:?}", input),
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
                        println!("处理图片文件: {:?}", input);
                        
                        // 检查水印容量
                        if !ImageWatermarker::check_watermark_capacity(
                            input,
                            watermark,
                            watermark_algorithm.as_ref(),
                        )? {
                            println!("警告: 水印可能太长，可能影响嵌入效果");
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
                        println!("处理音频文件: {:?}", input);
                        
                        // 检查水印容量
                        if !AudioWatermarker::check_watermark_capacity(
                            input,
                            watermark,
                            watermark_algorithm.as_ref(),
                        )? {
                            println!("警告: 水印可能太长，可能影响嵌入效果");
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
                    return Err(WatermarkError::UnsupportedFormat(
                        "视频水印功能暂未实现".to_string(),
                    ));
                }
            }

            if cli.verbose {
                println!("水印嵌入完成!");
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
                    format!("输入文件不存在: {:?}", input),
                )));
            }

            // 检测媒体类型
            let media_type = MediaUtils::detect_media_type(input)?;

            // 创建水印算法
            let watermark_algorithm = WatermarkFactory::create_algorithm(algorithm.clone());

            if cli.verbose {
                println!("从文件提取水印: {:?}", input);
                println!("使用算法: {:?}", algorithm);
            }

            let watermark_length = *length;

            // 根据媒体类型选择处理方式
            let extracted_watermark = match media_type {
                MediaType::Image => {
                    ImageWatermarker::extract_watermark_debug(
                        input,
                        watermark_algorithm.as_ref(),
                        watermark_length,
                        cli.verbose,
                    )?
                }
                MediaType::Audio => {
                    AudioWatermarker::extract_watermark(
                        input,
                        watermark_algorithm.as_ref(),
                        watermark_length,
                    )?
                }
                MediaType::Video => {
                    return Err(WatermarkError::UnsupportedFormat(
                        "视频水印功能暂未实现".to_string(),
                    ));
                }
            };

            // 输出到文件（如果指定）
            if let Some(output_path) = output {
                MediaUtils::ensure_output_dir(output_path)?;
                std::fs::write(output_path, &extracted_watermark)?;
                println!("提取的水印已保存到: {:?}", output_path);
            }

            // 总是在控制台显示结果
            println!("\n=== 提取结果 ===");
            println!("水印内容: {}", extracted_watermark);
            println!("水印长度: {} 字符", extracted_watermark.len());

            if cli.verbose {
                println!("水印提取完成!");
            }
        }
    }

    Ok(())
}
