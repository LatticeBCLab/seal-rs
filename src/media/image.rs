use crate::error::{Result, WatermarkError};
use crate::watermark::{WatermarkAlgorithm, WatermarkUtils};
use image::{ColorType, DynamicImage, ImageBuffer, ImageFormat, Luma, Rgb};
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

        // 将水印文本转换为比特
        let watermark_bits = WatermarkUtils::string_to_bits(watermark_text);

        let watermarked_img = match img.color() {
            ColorType::L8 => {
                // 灰度图片处理
                let gray_img = img.to_luma8();
                let data = Self::image_to_array_gray(&gray_img)?;
                let watermarked_data = algorithm.embed(&data, &watermark_bits, strength)?;
                Self::array_to_image_gray(&watermarked_data)?
            }
            ColorType::Rgb8 | ColorType::Rgba8 => {
                // 彩色图片处理 - 转换为RGB并在每个通道嵌入水印
                let rgb_img = img.to_rgb8();
                let (r_data, g_data, b_data) = Self::image_to_array_rgb(&rgb_img)?;

                // 在三个通道分别嵌入水印
                let watermarked_r = algorithm.embed(&r_data, &watermark_bits, strength)?;
                let watermarked_g = algorithm.embed(&g_data, &watermark_bits, strength)?;
                let watermarked_b = algorithm.embed(&b_data, &watermark_bits, strength)?;

                Self::array_to_image_rgb(&watermarked_r, &watermarked_g, &watermarked_b)?
            }
            _ => {
                // 其他格式转换为RGB处理
                let rgb_img = img.to_rgb8();
                let (r_data, g_data, b_data) = Self::image_to_array_rgb(&rgb_img)?;

                let watermarked_r = algorithm.embed(&r_data, &watermark_bits, strength)?;
                let watermarked_g = algorithm.embed(&g_data, &watermark_bits, strength)?;
                let watermarked_b = algorithm.embed(&b_data, &watermark_bits, strength)?;

                Self::array_to_image_rgb(&watermarked_r, &watermarked_g, &watermarked_b)?
            }
        };

        // 保存图片
        watermarked_img.save(&output_path)?;

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

        let extracted_bits = match img.color() {
            ColorType::L8 => {
                // 灰度图片处理
                let gray_img = img.to_luma8();
                let data = Self::image_to_array_gray(&gray_img)?;
                algorithm.extract(&data, watermark_length * 8)?
            }
            ColorType::Rgb8 | ColorType::Rgba8 => {
                // 彩色图片处理 - 从R通道提取（也可以投票）
                let rgb_img = img.to_rgb8();
                let (r_data, _g_data, _b_data) = Self::image_to_array_rgb(&rgb_img)?;
                algorithm.extract(&r_data, watermark_length * 8)?
            }
            _ => {
                // 其他格式转换为RGB处理
                let rgb_img = img.to_rgb8();
                let (r_data, _g_data, _b_data) = Self::image_to_array_rgb(&rgb_img)?;
                algorithm.extract(&r_data, watermark_length * 8)?
            }
        };

        // 转换为字符串
        let watermark_text = WatermarkUtils::bits_to_string(&extracted_bits)?;

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

        if verbose {
            eprintln!(
                "图片信息: {}x{} 像素, 格式: {:?}",
                img.width(),
                img.height(),
                img.color()
            );
        }

        let data = match img.color() {
            ColorType::L8 => {
                let gray_img = img.to_luma8();
                Self::image_to_array_gray(&gray_img)?
            }
            ColorType::Rgb8 | ColorType::Rgba8 => {
                let rgb_img = img.to_rgb8();
                let (r_data, _g_data, _b_data) = Self::image_to_array_rgb(&rgb_img)?;
                r_data
            }
            _ => {
                let rgb_img = img.to_rgb8();
                let (r_data, _g_data, _b_data) = Self::image_to_array_rgb(&rgb_img)?;
                r_data
            }
        };

        if verbose {
            eprintln!(
                "尝试提取 {} 字符的水印 ({} 比特)...",
                watermark_length,
                watermark_length * 8
            );
        }

        // 首先尝试标准提取
        match algorithm.extract(&data, watermark_length * 8) {
            Ok(extracted_bits) => {
                if verbose {
                    eprintln!(
                        "{}",
                        WatermarkUtils::analyze_extracted_bits(&extracted_bits)
                    );
                }

                // 尝试严格转换
                match WatermarkUtils::bits_to_string(&extracted_bits) {
                    Ok(watermark_text) => {
                        eprintln!("水印提取完成:");
                        eprintln!("使用算法: {}", algorithm.name());
                        eprintln!("提取到的水印: {watermark_text}");
                        Ok(watermark_text)
                    }
                    Err(_) => {
                        if verbose {
                            eprintln!("严格UTF-8转换失败，尝试宽松模式...");
                        }

                        let lossy_text = WatermarkUtils::bits_to_string_lossy(&extracted_bits);
                        eprintln!("水印提取完成 (宽松模式):");
                        eprintln!("使用算法: {}", algorithm.name());
                        eprintln!("提取到的水印: {lossy_text}");
                        Ok(lossy_text)
                    }
                }
            }
            Err(e) => {
                if verbose {
                    eprintln!("标准提取失败: {e}");
                    eprintln!("尝试投票提取方法...");
                }

                // 尝试投票提取
                match WatermarkUtils::extract_with_voting(algorithm, &data, watermark_length * 8, 3)
                {
                    Ok(voted_bits) => {
                        if verbose {
                            eprintln!("投票提取结果:");
                            eprintln!("{}", WatermarkUtils::analyze_extracted_bits(&voted_bits));
                        }

                        let lossy_text = WatermarkUtils::bits_to_string_lossy(&voted_bits);
                        eprintln!("水印提取完成 (投票模式):");
                        eprintln!("使用算法: {}", algorithm.name());
                        eprintln!("提取到的水印: {lossy_text}");
                        Ok(lossy_text)
                    }
                    Err(_) => Err(e),
                }
            }
        }
    }

    /// 将灰度图片转换为ndarray
    /// 标准化到 [0.0, 1.0] 范围以避免精度损失
    fn image_to_array_gray(img: &ImageBuffer<Luma<u8>, Vec<u8>>) -> Result<Array2<f64>> {
        let (width, height) = img.dimensions();
        let mut array = Array2::<f64>::zeros((height as usize, width as usize));

        for (x, y, pixel) in img.enumerate_pixels() {
            // 归一化到 [0.0, 1.0] 范围，保证精度和可逆性
            array[[y as usize, x as usize]] = pixel[0] as f64 / 255.0;
        }

        Ok(array)
    }

    /// 将ndarray转换为灰度图片
    /// 从 [0.0, 1.0] 范围反标准化到 u8，保证与 image_to_array_gray 完全可逆
    fn array_to_image_gray(array: &Array2<f64>) -> Result<DynamicImage> {
        let (height, width) = array.dim();
        let mut img_buffer = ImageBuffer::new(width as u32, height as u32);

        for (x, y, pixel) in img_buffer.enumerate_pixels_mut() {
            let value = array[[y as usize, x as usize]];
            // 反标准化：[0.0, 1.0] -> [0.0, 255.0]，然后 clamp 到 u8 范围
            // 使用四舍五入而不是截断，减小精度损失
            let scaled_value = (value * 255.0).round().clamp(0.0, 255.0) as u8;
            *pixel = Luma([scaled_value]);
        }

        Ok(DynamicImage::ImageLuma8(img_buffer))
    }

    /// 将RGB图片转换为三个通道的ndarray
    /// 标准化到 [0.0, 1.0] 范围以避免精度损失
    fn image_to_array_rgb(
        img: &ImageBuffer<Rgb<u8>, Vec<u8>>,
    ) -> Result<(Array2<f64>, Array2<f64>, Array2<f64>)> {
        let (width, height) = img.dimensions();
        let mut r_array = Array2::<f64>::zeros((height as usize, width as usize));
        let mut g_array = Array2::<f64>::zeros((height as usize, width as usize));
        let mut b_array = Array2::<f64>::zeros((height as usize, width as usize));

        for (x, y, pixel) in img.enumerate_pixels() {
            // 归一化到 [0.0, 1.0] 范围，保证精度和可逆性
            r_array[[y as usize, x as usize]] = pixel[0] as f64 / 255.0;
            g_array[[y as usize, x as usize]] = pixel[1] as f64 / 255.0;
            b_array[[y as usize, x as usize]] = pixel[2] as f64 / 255.0;
        }

        Ok((r_array, g_array, b_array))
    }

    /// 将三个通道的ndarray转换为RGB图片
    /// 从 [0.0, 1.0] 范围反标准化到 u8，保证与 image_to_array_rgb 完全可逆
    fn array_to_image_rgb(
        r_array: &Array2<f64>,
        g_array: &Array2<f64>,
        b_array: &Array2<f64>,
    ) -> Result<DynamicImage> {
        let (height, width) = r_array.dim();
        let mut img_buffer = ImageBuffer::new(width as u32, height as u32);

        for (x, y, pixel) in img_buffer.enumerate_pixels_mut() {
            let r_value = r_array[[y as usize, x as usize]];
            let g_value = g_array[[y as usize, x as usize]];
            let b_value = b_array[[y as usize, x as usize]];

            // 反标准化：[0.0, 1.0] -> [0.0, 255.0]，然后 clamp 到 u8 范围
            // 使用四舍五入而不是截断，减小精度损失
            let r_scaled = (r_value * 255.0).round().clamp(0.0, 255.0) as u8;
            let g_scaled = (g_value * 255.0).round().clamp(0.0, 255.0) as u8;
            let b_scaled = (b_value * 255.0).round().clamp(0.0, 255.0) as u8;

            *pixel = Rgb([r_scaled, g_scaled, b_scaled]);
        }

        Ok(DynamicImage::ImageRgb8(img_buffer))
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
                let blocks_w = width.div_ceil(8);
                let blocks_h = height.div_ceil(8);
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
