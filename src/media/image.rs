use crate::error::{Result, WatermarkError};
use crate::watermark::{WatermarkAlgorithm, WatermarkUtils};
use image::{ImageBuffer, Luma, DynamicImage, ImageFormat};
use ndarray::Array2;
use std::path::Path;

/// 图片水印处理器
pub struct ImageWatermarker;

impl ImageWatermarker {
    /// 嵌入水印到图片中
    pub fn embed_watermark<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
        strength: f64,
    ) -> Result<()> {
        // 加载图片
        let img = image::open(&input_path)?;
        let gray_img = img.to_luma8();

        // 转换为ndarray
        let data = Self::image_to_array(&gray_img)?;

        // 将水印文本转换为比特
        let watermark_bits = WatermarkUtils::string_to_bits(watermark_text);

        // 嵌入水印
        let watermarked_data = algorithm.embed(&data, &watermark_bits, strength)?;

        // 转换回图片格式
        let watermarked_img = Self::array_to_image(&watermarked_data)?;

        // 保存图片
        watermarked_img.save(&output_path)?;

        println!("水印已成功嵌入到图片中: {:?}", output_path.as_ref());
        println!("使用算法: {}", algorithm.name());
        println!("水印内容: {}", watermark_text);
        println!("嵌入强度: {}", strength);

        Ok(())
    }

    /// 从图片中提取水印
    pub fn extract_watermark<P: AsRef<Path>>(
        input_path: P,
        algorithm: &dyn WatermarkAlgorithm,
        watermark_length: usize,
    ) -> Result<String> {
        // 加载图片
        let img = image::open(&input_path)?;
        let gray_img = img.to_luma8();

        // 转换为ndarray
        let data = Self::image_to_array(&gray_img)?;

        // 提取水印比特
        let extracted_bits = algorithm.extract(&data, watermark_length * 8)?;

        // 转换为字符串
        let watermark_text = WatermarkUtils::bits_to_string(&extracted_bits)?;

        println!("水印提取完成:");
        println!("使用算法: {}", algorithm.name());
        println!("提取到的水印: {}", watermark_text);

        Ok(watermark_text)
    }

    /// 从图片中提取水印（调试模式）
    pub fn extract_watermark_debug<P: AsRef<Path>>(
        input_path: P,
        algorithm: &dyn WatermarkAlgorithm,
        watermark_length: usize,
        verbose: bool,
    ) -> Result<String> {
        // 加载图片
        let img = image::open(&input_path)?;
        let gray_img = img.to_luma8();

        if verbose {
            println!("图片信息: {}x{} 像素", img.width(), img.height());
        }

        // 转换为ndarray
        let data = Self::image_to_array(&gray_img)?;

        if verbose {
            println!("尝试提取 {} 字符的水印 ({} 比特)...", watermark_length, watermark_length * 8);
        }

        // 首先尝试标准提取
        match algorithm.extract(&data, watermark_length * 8) {
            Ok(extracted_bits) => {
                if verbose {
                    println!("{}", WatermarkUtils::analyze_extracted_bits(&extracted_bits));
                }

                // 尝试严格转换
                match WatermarkUtils::bits_to_string(&extracted_bits) {
                    Ok(watermark_text) => {
                        println!("水印提取完成:");
                        println!("使用算法: {}", algorithm.name());
                        println!("提取到的水印: {}", watermark_text);
                        return Ok(watermark_text);
                    }
                    Err(_) => {
                        if verbose {
                            println!("严格UTF-8转换失败，尝试宽松模式...");
                        }
                        
                        let lossy_text = WatermarkUtils::bits_to_string_lossy(&extracted_bits);
                        println!("水印提取完成 (宽松模式):");
                        println!("使用算法: {}", algorithm.name());
                        println!("提取到的水印: {}", lossy_text);
                        return Ok(lossy_text);
                    }
                }
            }
            Err(e) => {
                if verbose {
                    println!("标准提取失败: {}", e);
                    println!("尝试投票提取方法...");
                }

                // 尝试投票提取
                match WatermarkUtils::extract_with_voting(algorithm, &data, watermark_length * 8, 3) {
                    Ok(voted_bits) => {
                        if verbose {
                            println!("投票提取结果:");
                            println!("{}", WatermarkUtils::analyze_extracted_bits(&voted_bits));
                        }

                        let lossy_text = WatermarkUtils::bits_to_string_lossy(&voted_bits);
                        println!("水印提取完成 (投票模式):");
                        println!("使用算法: {}", algorithm.name());
                        println!("提取到的水印: {}", lossy_text);
                        return Ok(lossy_text);
                    }
                    Err(_) => return Err(e),
                }
            }
        }
    }

    /// 将图片转换为ndarray
    fn image_to_array(img: &ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<Array2<f64>> {
        let (width, height) = img.dimensions();
        let mut array = Array2::<f64>::zeros((height as usize, width as usize));

        for (x, y, pixel) in img.enumerate_pixels() {
            array[[y as usize, x as usize]] = pixel[0] as f64;
        }

        Ok(array)
    }

    /// 将ndarray转换为图片
    fn array_to_image(array: &Array2<f64>) -> Result<DynamicImage> {
        let (height, width) = array.dim();
        let mut img_buffer = ImageBuffer::new(width as u32, height as u32);

        for (x, y, pixel) in img_buffer.enumerate_pixels_mut() {
            let value = array[[y as usize, x as usize]];
            // 限制像素值在0-255范围内
            let clamped_value = value.max(0.0).min(255.0) as u8;
            *pixel = Luma([clamped_value]);
        }

        Ok(DynamicImage::ImageLuma8(img_buffer))
    }

    /// 获取图片尺寸信息
    pub fn get_image_info<P: AsRef<Path>>(path: P) -> Result<(u32, u32, ImageFormat)> {
        let img = image::open(&path)?;
        let format = image::ImageFormat::from_path(&path)
            .map_err(|_| WatermarkError::UnsupportedFormat("无法确定图片格式".to_string()))?;
        
        Ok((img.width(), img.height(), format))
    }

    /// 检查图片是否适合嵌入水印
    pub fn check_watermark_capacity<P: AsRef<Path>>(
        path: P,
        watermark_text: &str,
        algorithm: &dyn WatermarkAlgorithm,
    ) -> Result<bool> {
        let (width, height, _) = Self::get_image_info(&path)?;
        let watermark_bits = WatermarkUtils::string_to_bits(watermark_text);

        // 根据算法计算容量
        let capacity = match algorithm.name() {
            name if name.contains("DCT") => {
                // DCT算法基于8x8块，现在支持任意尺寸
                let blocks_w = (width + 7) / 8;
                let blocks_h = (height + 7) / 8;
                (blocks_w * blocks_h) as usize
            }
            name if name.contains("DWT") => {
                // DWT算法基于小波系数，支持偶数尺寸
                let padded_width = if width % 2 == 0 { width } else { width + 1 };
                let padded_height = if height % 2 == 0 { height } else { height + 1 };
                let coeffs = (padded_width * padded_height) / 4;
                coeffs as usize
            }
            _ => return Err(WatermarkError::Algorithm("未知算法".to_string())),
        };

        Ok(watermark_bits.len() <= capacity)
    }


} 