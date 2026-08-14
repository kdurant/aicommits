use crate::config::Config;

/// Conventional Commits 支持的 type 列表。
pub const CONVENTIONAL_TYPES: &[&str] = &[
    "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore", "revert",
];

/// 解析并规范化后的 commit message。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMessage {
    pub subject: String,
    pub body: Vec<String>,
}

impl CommitMessage {
    /// 从 AI 原始输出中解析出 subject 和 body。
    ///
    /// 会清理 Markdown code fence、去掉解释性前缀行（如 "Here is the commit message:"）。
    pub fn parse(raw: &str) -> Self {
        let mut lines: Vec<String> = Vec::new();
        let mut in_fence = false;

        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if in_fence || !is_noise_line(trimmed) {
                lines.push(trimmed.to_string());
            }
        }

        // 去掉首尾空行
        while lines.first().is_some_and(|l| l.is_empty()) {
            lines.remove(0);
        }
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }

        if lines.is_empty() {
            return Self {
                subject: String::new(),
                body: Vec::new(),
            };
        }

        let subject = lines.remove(0);
        // 正文中多余的空行不保留（bullet 列表场景）
        let body = lines.into_iter().filter(|l| !l.is_empty()).collect();
        Self { subject, body }
    }

    /// 组装成适合直接提交的完整 message。
    pub fn formatted(&self) -> String {
        let mut text = self.subject.clone();
        for line in &self.body {
            text.push('\n');
            text.push_str(line);
        }
        text
    }

    /// subject 是否符合 Conventional Commits 格式。
    pub fn is_conventional(&self) -> bool {
        let Some((type_part, rest)) = self.subject.split_once(':') else {
            return false;
        };
        let rest = rest.trim_start();
        if rest.is_empty() {
            return false;
        }
        if let Some(scope_start) = type_part.find('(') {
            if !type_part.ends_with(')') {
                return false;
            }
            let name = &type_part[..scope_start];
            let scope = &type_part[scope_start + 1..type_part.len() - 1];
            CONVENTIONAL_TYPES.contains(&name)
                && !scope.is_empty()
                && scope
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_/.".contains(c))
        } else {
            CONVENTIONAL_TYPES.contains(&type_part)
        }
    }

    /// 检查 subject 是否符合配置约束，返回警告列表。
    pub fn check(&self, config: &Config) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.subject.is_empty() {
            warnings.push("AI 未返回可用的 commit message".to_string());
        }
        if config.conventional && !self.is_conventional() {
            warnings.push(format!(
                "subject 不符合 Conventional Commits 格式：{}",
                self.subject
            ));
        }
        let len = self.subject.chars().count();
        if len > config.max_subject_length {
            warnings.push(format!(
                "subject 长度 {} 超过配置的最大 {} 字符",
                len, config.max_subject_length
            ));
        }
        warnings
    }
}

/// 判断是否是需要忽略的解释性前缀行。
fn is_noise_line(trimmed: &str) -> bool {
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    let prefixes = [
        "here is the commit message",
        "here's the commit message",
        "here are the commit message",
        "here are some commit messages",
        "generated commit message",
        "the commit message is",
        "suggested commit message",
    ];
    // 前缀匹配且以冒号结尾，通常是解释而非消息本身
    prefixes
        .iter()
        .any(|p| lower.starts_with(p) && trimmed.ends_with(':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config::default()
    }

    #[test]
    fn parses_simple_subject() {
        let msg = CommitMessage::parse("feat(cli): add commit generation");
        assert_eq!(msg.subject, "feat(cli): add commit generation");
        assert!(msg.body.is_empty());
    }

    #[test]
    fn parses_subject_with_body() {
        let raw = "feat(cli): add AI-powered commit generation\n\n- analyze staged Git changes\n- support confirmation";
        let msg = CommitMessage::parse(raw);
        assert_eq!(msg.subject, "feat(cli): add AI-powered commit generation");
        assert_eq!(msg.body.len(), 2);
        assert_eq!(msg.body[0], "- analyze staged Git changes");
    }

    #[test]
    fn cleans_markdown_code_fence() {
        let raw = "```text\nfeat(cli): add commit generation\n```";
        let msg = CommitMessage::parse(raw);
        assert_eq!(msg.subject, "feat(cli): add commit generation");
    }

    #[test]
    fn cleans_markdown_code_fence_with_leading_text() {
        let raw = "```\nfeat(git): handle repositories without staged changes\n```\n";
        let msg = CommitMessage::parse(raw);
        assert_eq!(
            msg.subject,
            "feat(git): handle repositories without staged changes"
        );
    }

    #[test]
    fn strips_explanatory_prefix() {
        let raw = "Here is the commit message:\n\nfix(api): retry on network errors";
        let msg = CommitMessage::parse(raw);
        assert_eq!(msg.subject, "fix(api): retry on network errors");
        assert!(msg.body.is_empty());
    }

    #[test]
    fn strips_trailing_newlines_and_whitespace() {
        let raw = "\n\n  chore: update dependencies  \n\n";
        let msg = CommitMessage::parse(raw);
        assert_eq!(msg.subject, "chore: update dependencies");
    }

    #[test]
    fn empty_input_yields_empty_subject() {
        let msg = CommitMessage::parse("");
        assert!(msg.subject.is_empty());
    }

    #[test]
    fn recognizes_conventional_subjects() {
        for t in CONVENTIONAL_TYPES {
            assert!(CommitMessage::parse(&format!("{t}: subject")).is_conventional());
            assert!(CommitMessage::parse(&format!("{t}(scope): subject")).is_conventional());
        }
    }

    #[test]
    fn rejects_non_conventional_subjects() {
        assert!(!CommitMessage::parse("this is not conventional").is_conventional());
        assert!(!CommitMessage::parse("nope: no colon after type").is_conventional());
        assert!(!CommitMessage::parse("feat:").is_conventional());
        assert!(!CommitMessage::parse("feat(): empty scope").is_conventional());
    }

    #[test]
    fn check_warns_on_too_long_subject() {
        let mut cfg = config();
        cfg.max_subject_length = 10;
        let msg = CommitMessage::parse("feat(cli): a much too long subject line here");
        let warnings = msg.check(&cfg);
        assert!(warnings.iter().any(|w| w.contains("长度")));
    }

    #[test]
    fn formatted_joins_subject_and_body() {
        let msg = CommitMessage {
            subject: "feat: x".to_string(),
            body: vec!["- one".to_string(), "- two".to_string()],
        };
        assert_eq!(msg.formatted(), "feat: x\n- one\n- two");
    }
}
