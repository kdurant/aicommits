pub mod opencode;
pub mod provider;

use crate::config::Config;
use crate::error::{Error, Result};

use self::provider::{AiProvider, MockProvider};

/// 根据配置创建 AI provider。
///
/// 增加新 provider 时只需在此注册，业务代码无需改动。
pub fn create(config: &Config) -> Result<Box<dyn AiProvider>> {
    match config.provider.as_str() {
        // OpenCode Go 订阅（OpenAI-compatible HTTP API），无需安装 opencode CLI
        "opencode" | "opencode-go" | "opencodego" => {
            Ok(Box::new(opencode::OpenCodeGoProvider::new(config)?))
        }
        "mock" => Ok(Box::new(MockProvider::new(config))),
        other => Err(Error::ProviderConfig(format!(
            "未知 provider：{other}（支持：opencode, mock）"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_mock_provider() {
        let config = Config {
            provider: "mock".to_string(),
            ..Config::default()
        };
        assert!(create(&config).is_ok());
    }

    #[test]
    fn rejects_unknown_provider() {
        let config = Config {
            provider: "nope".to_string(),
            ..Config::default()
        };
        let err = create(&config);
        assert!(matches!(err, Err(Error::ProviderConfig(_))));
    }
}
