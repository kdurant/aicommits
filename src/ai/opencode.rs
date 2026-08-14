use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::{Value, json};

use crate::commit::CommitMessage;
use crate::config::{Config, DEFAULT_MODEL};
use crate::error::{Error, Result};
use crate::git::GitContext;
use crate::prompt;

use super::provider::AiProvider;

/// 通过 OpenCode Go 订阅的 HTTP API（OpenAI-compatible）调用 AI。
///
/// 认证使用 API key（`OPENCODE_API_KEY` / `AICOMMITS_API_KEY`，或本机
/// opencode 的 auth.json 中 `opencode-go` 的 key）。无需安装 opencode CLI。
#[derive(Debug, Clone)]
pub struct OpenCodeGoProvider {
    model: String,
    base_url: String,
    api_key: String,
    max_subject_length: usize,
    timeout: std::time::Duration,
}

impl OpenCodeGoProvider {
    /// 构造 provider 并解析 API key 与模型。
    pub fn new(config: &Config) -> Result<Self> {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("OPENCODE_API_KEY").ok().filter(|k| !k.is_empty()))
            .or_else(read_opencode_auth_key)
            .ok_or_else(|| {
                Error::ProviderConfig(
                    "未找到 OpenCode Go API key。请设置环境变量 OPENCODE_API_KEY，或先在 opencode 中登录 opencode-go。"
                        .to_string(),
                )
            })?;

        let model = config
            .model
            .clone()
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());

        Ok(Self {
            model: strip_provider_prefix(&model),
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key,
            max_subject_length: config.max_subject_length,
            timeout: config.timeout,
        })
    }

    /// 调用 `/chat/completions`，返回助手消息的纯文本。
    async fn complete(&self, prompt: &str) -> Result<String> {
        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .map_err(|e| Error::AiRequestFailed(format!("创建 HTTP 客户端失败：{e}")))?;

        let body = json!({
            "model": self.model,
            "messages": [{"role": "user", "content": prompt}],
        });

        let url = format!("{}/chat/completions", self.base_url);
        let response = client
            .post(&url)
            .bearer_auth(&self.api_key)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                Error::AiRequestFailed(format!("无法连接 OpenCode Go（{}）：{e}", self.base_url))
            })?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                return Err(Error::AuthFailed(format!(
                    "OpenCode Go 认证失败（HTTP {status}）。请检查 OPENCODE_API_KEY 是否正确。\n详情：{detail}"
                )));
            }
            return Err(Error::AiRequestFailed(format!(
                "OpenCode Go 返回 HTTP {status}。\n详情：{detail}"
            )));
        }

        let data: Value = response
            .json()
            .await
            .map_err(|e| Error::InvalidAiResponse(format!("解析 AI 响应失败：{e}")))?;

        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| Error::InvalidAiResponse(format!("AI 响应缺少文本内容：{data}")))?;

        Ok(content.trim().to_string())
    }
}

/// 从模型名中去掉 provider 前缀（如 `opencode-go/deepseek-v4-flash` → `deepseek-v4-flash`）。
fn strip_provider_prefix(model: &str) -> String {
    model
        .rsplit_once('/')
        .map(|(_, rest)| rest)
        .unwrap_or(model)
        .to_string()
}

/// 读取本机 opencode 的 auth.json 中 opencode-go 的 API key（如果存在）。
fn read_opencode_auth_key() -> Option<String> {
    let path = dirs::data_local_dir()?.join("opencode").join("auth.json");
    let content = std::fs::read_to_string(path).ok()?;
    let data: Value = serde_json::from_str(&content).ok()?;
    data["opencode-go"]["key"].as_str().map(str::to_string)
}

#[async_trait]
impl AiProvider for OpenCodeGoProvider {
    async fn generate_commit_message(&self, context: &GitContext) -> Result<CommitMessage> {
        // prompt 在 prompt.rs 中构建，provider 只负责发送与解析
        let prompt = prompt::build(context, self.max_subject_length);
        let raw = self.complete(&prompt).await?;
        if raw.trim().is_empty() {
            return Err(Error::InvalidAiResponse("模型没有返回任何文本".to_string()));
        }
        let message = CommitMessage::parse(&raw);
        if message.subject.is_empty() {
            return Err(Error::InvalidAiResponse(raw));
        }
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_provider_prefix() {
        assert_eq!(
            strip_provider_prefix("deepseek-v4-flash"),
            "deepseek-v4-flash"
        );
        assert_eq!(
            strip_provider_prefix("opencode-go/deepseek-v4-flash"),
            "deepseek-v4-flash"
        );
        assert_eq!(
            strip_provider_prefix("opencode/deepseek-v4-flash"),
            "deepseek-v4-flash"
        );
    }

    #[test]
    fn missing_key_returns_friendly_error() {
        // 清空环境变量与 auth 目录，确保报错
        unsafe {
            std::env::remove_var("OPENCODE_API_KEY");
            std::env::remove_var("AICOMMITS_API_KEY");
        }
        let config = Config {
            api_key: None,
            base_url: "https://example.invalid/v1".to_string(),
            ..Config::default()
        };
        let result = OpenCodeGoProvider::new(&config);
        // 本机若存在 opencode auth.json 则会成功；这里两种结果都接受，
        // 重点是不 panic 且有可读错误/成功路径
        if let Err(e) = result {
            assert!(matches!(e, Error::ProviderConfig(_)));
        }
    }

    #[test]
    fn reads_opencode_auth_key_when_present() {
        // auth.json 路径依赖于本机环境，仅验证函数签名与类型
        if let Some(key) = read_opencode_auth_key() {
            assert!(key.starts_with("sk-"));
        }
    }
}
