use std::path::PathBuf;

use clap::Parser;

/// 生成 AI 驱动的 Git commit message 并可选地直接提交。
#[derive(Debug, Parser)]
#[command(name = "aicommits", version, about, disable_help_subcommand = true)]
pub struct Cli {
    /// 跳过人工确认，直接提交
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,

    /// 只生成 commit message，不执行 commit
    #[arg(long)]
    pub dry_run: bool,

    /// 显示更多 Git 和 AI 请求调试信息
    #[arg(long)]
    pub verbose: bool,

    /// 指定 AI provider（opencode / mock）
    #[arg(long)]
    pub provider: Option<String>,

    /// 指定 AI 模型
    #[arg(long)]
    pub model: Option<String>,

    /// 指定配置文件路径（默认 ~/.config/aicommits/config.toml）
    #[arg(long)]
    pub config: Option<PathBuf>,
}
