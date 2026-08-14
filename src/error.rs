use std::io;

/// 统一错误类型，所有模块的错误都收敛到这里，保证对 CLI 用户友好。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("当前目录不是 Git 仓库（或者任何父目录都不是）")]
    NotGitRepository,

    #[error("Git 命令执行失败：{command}\n{stderr}")]
    GitCommandFailed { command: String, stderr: String },

    #[error("没有暂存的变更（staged changes），请先 git add")]
    NoStagedChanges,

    #[error("工作区是干净的，没有任何变更需要提交")]
    CleanWorkingTree,

    #[error("git add 失败：{stderr}")]
    GitAddFailed { stderr: String },

    #[error("AI provider 配置错误：{0}")]
    ProviderConfig(String),

    #[error("AI 认证失败：{0}")]
    AuthFailed(String),

    #[error("AI 请求失败：{0}")]
    AiRequestFailed(String),

    #[error("AI 返回了无法解析的内容：{0}")]
    InvalidAiResponse(String),

    #[error("git commit 失败：{stderr}")]
    CommitFailed { stderr: String },

    #[error("操作已取消")]
    Cancelled,

    #[error("配置文件错误：{0}")]
    Config(String),

    #[error("IO 错误：{0}")]
    Io(#[from] io::Error),

    #[error("交互输入错误：{0}")]
    Interactive(String),
}

/// 便捷 Result 别名。
pub type Result<T> = std::result::Result<T, Error>;
