use async_trait::async_trait;

use crate::Result;
use crate::commit::CommitMessage;
use crate::git::GitContext;

/// AI provider 抽象。业务逻辑只依赖此 trait，不依赖具体实现。
#[async_trait]
pub trait AiProvider: Send + Sync {
    /// 根据 Git 上下文生成 commit message。
    async fn generate_commit_message(&self, context: &GitContext) -> Result<CommitMessage>;
}

/// 测试/离线场景使用的 mock provider，直接返回配置中的固定响应。
#[derive(Debug, Clone)]
pub struct MockProvider {
    response: String,
}

impl MockProvider {
    pub fn new(config: &crate::config::Config) -> Self {
        Self {
            response: config.mock_response.clone(),
        }
    }
}

#[async_trait]
impl AiProvider for MockProvider {
    async fn generate_commit_message(&self, _context: &GitContext) -> Result<CommitMessage> {
        Ok(CommitMessage::parse(&self.response))
    }
}
