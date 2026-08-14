use std::path::PathBuf;

use serde::Deserialize;

use crate::cli::Cli;
use crate::error::{Error, Result};

/// 默认 diff 上限字符数。
const DEFAULT_MAX_DIFF_CHARS: usize = 20_000;
/// 默认 AI 请求超时（秒）。
const DEFAULT_TIMEOUT_SECS: u64 = 180;
/// 默认 OpenCode Go 订阅 API 端点（OpenAI-compatible）。
pub const DEFAULT_BASE_URL: &str = "https://opencode.ai/zen/go/v1";
/// 未配置模型时的默认模型。
pub const DEFAULT_MODEL: &str = "deepseek-v4-flash";

/// 运行时配置：由配置文件 + 环境变量 + CLI 参数合并得到。
#[derive(Debug, Clone)]
pub struct Config {
    pub provider: String,
    pub conventional: bool,
    pub max_subject_length: usize,
    pub auto_stage: bool,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub base_url: String,
    pub max_diff_chars: usize,
    pub timeout: std::time::Duration,
    /// 仅供 mock provider 使用，测试时可通过环境变量注入。
    pub mock_response: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: "opencode".to_string(),
            conventional: true,
            max_subject_length: 72,
            auto_stage: false,
            model: None,
            api_key: None,
            base_url: DEFAULT_BASE_URL.to_string(),
            max_diff_chars: DEFAULT_MAX_DIFF_CHARS,
            timeout: std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            mock_response: "feat: mock commit message".to_string(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FileConfig {
    provider: Option<String>,
    commit: CommitConfig,
    ai: AiConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CommitConfig {
    conventional: Option<bool>,
    max_subject_length: Option<usize>,
    auto_stage: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AiConfig {
    model: Option<String>,
    api_key: Option<String>,
    base_url: Option<String>,
    max_diff_chars: Option<usize>,
    timeout_secs: Option<u64>,
    mock_response: Option<String>,
}

impl Config {
    /// 加载配置：默认值 → 配置文件 → 环境变量 → CLI 参数（后者的优先级更高）。
    pub fn load(cli: &Cli) -> Result<Self> {
        let mut file = FileConfig::default();
        let path = cli.config.clone().unwrap_or(Self::default_config_path());
        if path.exists() {
            let content = std::fs::read_to_string(&path).map_err(Error::Io)?;
            file = toml::from_str(&content)
                .map_err(|e| Error::Config(format!("{}: {}", path.display(), e)))?;
        }

        let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());

        let provider = cli
            .provider
            .clone()
            .or_else(|| env("AICOMMITS_PROVIDER"))
            .or(file.provider)
            .unwrap_or_else(|| "opencode".to_string());

        let model = cli
            .model
            .clone()
            .or_else(|| env("AICOMMITS_MODEL"))
            .or(file.ai.model);

        let api_key = env("AICOMMITS_API_KEY").or(file.ai.api_key);

        let base_url = env("OPENCODE_BASE_URL")
            .or(file.ai.base_url)
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        Ok(Self {
            provider,
            conventional: file.commit.conventional.unwrap_or(true),
            max_subject_length: file.commit.max_subject_length.unwrap_or(72),
            auto_stage: file.commit.auto_stage.unwrap_or(false),
            model,
            api_key,
            base_url,
            max_diff_chars: file.ai.max_diff_chars.unwrap_or(DEFAULT_MAX_DIFF_CHARS),
            timeout: std::time::Duration::from_secs(
                file.ai.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS),
            ),
            mock_response: env("AICOMMITS_MOCK_RESPONSE")
                .or(file.ai.mock_response)
                .unwrap_or_else(|| "feat: mock commit message".to_string()),
        })
    }

    /// 默认配置文件路径：~/.config/aicommits/config.toml。
    pub fn default_config_path() -> PathBuf {
        dirs::config_dir()
            .map(|dir| dir.join("aicommits").join("config.toml"))
            .unwrap_or_else(|| PathBuf::from("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use std::sync::Mutex;

    fn cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    /// 序列化 env 相关测试，避免并行运行时互相覆盖进程级环境变量。
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// 在隔离的环境变量下运行闭包。
    fn with_env<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(
        key: K,
        value: V,
        f: impl FnOnce(),
    ) {
        let _guard = ENV_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var(&key, &value);
        }
        f();
        unsafe {
            std::env::remove_var(&key);
        }
    }

    #[test]
    fn defaults_are_sane() {
        let cfg = Config::default();
        assert_eq!(cfg.provider, "opencode");
        assert_eq!(cfg.max_subject_length, 72);
        assert!(!cfg.auto_stage);
    }

    #[test]
    fn env_var_overrides_provider() {
        with_env("AICOMMITS_PROVIDER", "mock", || {
            let cfg = Config::load(&cli(&["aicommits"])).unwrap();
            assert_eq!(cfg.provider, "mock");
        });
    }

    #[test]
    fn env_var_overrides_model() {
        with_env("AICOMMITS_MODEL", "gpt-4o", || {
            let cfg = Config::load(&cli(&["aicommits"])).unwrap();
            assert_eq!(cfg.model.as_deref(), Some("gpt-4o"));
        });
    }

    #[test]
    fn cli_provider_wins_over_env() {
        with_env("AICOMMITS_PROVIDER", "mock", || {
            let cfg = Config::load(&cli(&["aicommits", "--provider", "opencode"])).unwrap();
            assert_eq!(cfg.provider, "opencode");
        });
    }

    #[test]
    fn cli_model_wins_over_env() {
        with_env("AICOMMITS_MODEL", "a", || {
            let cfg = Config::load(&cli(&["aicommits", "--model", "b"])).unwrap();
            assert_eq!(cfg.model.as_deref(), Some("b"));
        });
    }

    #[test]
    fn missing_config_file_uses_defaults() {
        // 指向不存在的配置文件，避免读到用户真实的 ~/.config/aicommits/config.toml
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = tempfile::TempDir::new().unwrap();
        let missing = dir.path().join("nonexistent.toml");
        let cfg =
            Config::load(&cli(&["aicommits", "--config", missing.to_str().unwrap()])).unwrap();
        assert_eq!(cfg.provider, "opencode");
        assert!(cfg.model.is_none());
    }

    #[test]
    fn invalid_toml_returns_config_error() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "provider = [1, 2]\n").unwrap();
        let cfg = Config::load(&cli(&["aicommits", "--config", path.to_str().unwrap()]));
        assert!(cfg.is_err());
    }
}
