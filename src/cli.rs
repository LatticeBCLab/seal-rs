use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// 数字水印CLI工具
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// 详细输出
    #[arg(short, long)]
    pub verbose: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 嵌入水印
    Embed {
        /// 输入文件路径
        #[arg(short, long)]
        input: PathBuf,

        /// 输出文件路径
        #[arg(short, long)]
        output: PathBuf,

        /// 水印内容（文本或文件路径）
        #[arg(short, long)]
        watermark: String,

        /// 使用的算法
        #[arg(short, long, default_value = "dct")]
        algorithm: Algorithm,

        /// 水印强度 (0.0-1.0)
        #[arg(short, long, default_value = "0.1")]
        strength: f64,
    },
    /// 提取水印
    Extract {
        /// 输入文件路径
        #[arg(short, long)]
        input: PathBuf,

        /// 使用的算法
        #[arg(short, long, default_value = "dct")]
        algorithm: Algorithm,

        /// 期望的水印文本长度（字符数）
        #[arg(short, long)]
        length: usize,

        /// 输出水印到文件（可选）
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

/// 支持的水印算法
#[derive(ValueEnum, Clone, Debug)]
pub enum Algorithm {
    /// 离散余弦变换
    Dct,
} 