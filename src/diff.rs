/// 被截断时的提示，必须明确告知 AI 遗漏的变更可能存在。
const TRUNCATION_NOTE: &str = "\n\nNOTE: The diff has been truncated due to context limits.\nDo not assume that omitted changes do not exist.\n";

/// 一条敏感信息发现记录，用于向用户展示具体位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretFinding {
    /// 涉及的文件路径（从 diff header 提取）。
    pub path: String,
    /// 触发检测的内容摘要（去掉 diff 前缀，已截断）。
    pub detail: String,
}

/// 处理后的 diff 结果。
#[derive(Debug, Clone, Default)]
pub struct ProcessedDiff {
    pub text: String,
    pub truncated: bool,
    pub secrets: Vec<SecretFinding>,
}

impl ProcessedDiff {
    /// 是否检测到敏感信息。
    pub fn has_secrets(&self) -> bool {
        !self.secrets.is_empty()
    }
}

/// diff 处理器：负责脱敏敏感信息、按上下文窗口截断超大 diff。
///
/// 第一版实现为"脱敏 + 字符级截断"，结构上允许以后升级为：
/// 按文件截断、排除二进制/generated 文件、token-based truncation 等。
#[derive(Debug, Clone)]
pub struct DiffProcessor {
    max_chars: usize,
}

impl DiffProcessor {
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars }
    }

    /// 处理原始 diff。
    pub fn process(&self, raw: &str) -> ProcessedDiff {
        let (redacted, secrets) = redact_secrets(raw);
        let (text, truncated) = truncate(&redacted, self.max_chars);
        ProcessedDiff {
            text,
            truncated,
            secrets,
        }
    }
}

/// 触发敏感检测的键名关键词（小写匹配）。
const SENSITIVE_KEYWORDS: &[&str] = &[
    "api_key",
    "api-key",
    "apikey",
    "secret",
    "password",
    "passwd",
    "token",
    "access_token",
    "access-token",
    "auth",
    "authorization",
    "private_key",
    "private-key",
];

/// 触发敏感检测的文件路径关键词（小写匹配）。
const SENSITIVE_PATH_KEYWORDS: &[&str] = &[
    ".env",
    "credentials",
    "secret",
    "password",
    "private",
    ".pem",
    ".key",
    "id_rsa",
    "id_ed25519",
];

/// 对 diff 做脱敏，返回脱敏后的文本以及敏感信息明细。
fn redact_secrets(raw: &str) -> (String, Vec<SecretFinding>) {
    let mut out = String::with_capacity(raw.len());
    let mut secrets = Vec::new();
    let mut in_private_key = false;
    let mut current_file = String::new();

    for line in raw.lines() {
        // 只剥离一个 diff 前缀符号，避免误伤内容本身的 "-"（如 -----BEGIN）
        let (_, rest) = split_diff_prefix(line);
        let content = rest.trim_start();

        // 跟踪当前文件路径，用于定位敏感信息
        if let Some(path) = extract_path(content) {
            current_file = path;
        }

        let mut normal = false;
        if in_private_key {
            if content.starts_with("-----END") {
                in_private_key = false;
                out.push_str(line);
                out.push('\n');
                continue;
            } else if looks_like_private_key_body(content) {
                out.push_str(&format_redacted_line(line, "[REDACTED PRIVATE KEY BLOCK]"));
                secrets.push(SecretFinding {
                    path: current_file.clone(),
                    detail: "私钥块内容".to_string(),
                });
                out.push('\n');
                continue;
            } else {
                // 内容不像私钥体（可能是文档中的示例文字），视为误报并退出
                in_private_key = false;
                normal = true;
            }
        }

        if !normal && content.starts_with("-----BEGIN") && content.contains("PRIVATE KEY") {
            in_private_key = true;
            out.push_str(line);
            secrets.push(SecretFinding {
                path: current_file.clone(),
                detail: "检测到 PRIVATE KEY 块".to_string(),
            });
            out.push('\n');
            continue;
        }

        let redacted = redact_key_value_line(line);
        if redacted != line {
            secrets.push(SecretFinding {
                path: current_file.clone(),
                detail: summarize(&content),
            });
        }

        if let Some(reason) = sensitive_path_reason(content) {
            secrets.push(SecretFinding {
                path: current_file.clone(),
                detail: format!("敏感文件路径匹配：{reason}"),
            });
        }

        out.push_str(&redacted);
        out.push('\n');
    }

    (out, secrets)
}

/// 判断一行是否像 PEM 私钥体（base64 字符，长度足够）。
fn looks_like_private_key_body(line: &str) -> bool {
    let line = line.trim();
    if line.len() < 16 {
        return false;
    }
    line.chars()
        .all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c))
}

/// 从 diff header 行提取文件路径（b/ 一侧）。
fn extract_path(content: &str) -> Option<String> {
    if let Some(p) = content.strip_prefix("+++ ") {
        return path_from_marker(p);
    }
    if content.starts_with("diff --git") {
        for word in content.split_whitespace() {
            if let Some(p) = path_from_marker(word) {
                return Some(p);
            }
        }
    }
    None
}

fn path_from_marker(word: &str) -> Option<String> {
    let path = word
        .strip_prefix("b/")
        .or_else(|| word.strip_prefix("a/"))?;
    if path == "/dev/null" {
        return None;
    }
    Some(path.to_string())
}

/// 检查 diff 行内容是否涉及敏感路径，返回匹配的关键词。
fn sensitive_path_reason(content: &str) -> Option<&'static str> {
    if !(content.starts_with("diff --git")
        || content.starts_with("+++")
        || content.starts_with("---"))
    {
        return None;
    }
    let lower = content.to_lowercase();
    SENSITIVE_PATH_KEYWORDS
        .iter()
        .find(|kw| lower.contains(**kw))
        .copied()
}

/// 截断内容摘要，供展示使用。
fn summarize(content: &str) -> String {
    const MAX: usize = 80;
    let trimmed = content.trim();
    if trimmed.chars().count() <= MAX {
        trimmed.to_string()
    } else {
        let cut: String = trimmed.chars().take(MAX).collect();
        format!("{cut}…")
    }
}

/// 处理 `key=value` / `key: value` 形式的敏感配置，替换值为占位符。
fn redact_key_value_line(line: &str) -> String {
    let (prefix, content) = split_diff_prefix(line);

    // 关键词必须落在键（分隔符之前）的部分
    let sep_idx = content.find(['=', ':']);
    let key_is_sensitive = content.split_once(['=', ':']).is_some_and(|(key, _)| {
        let key_lower = key.to_lowercase();
        SENSITIVE_KEYWORDS.iter().any(|kw| key_lower.contains(*kw))
    });
    if let Some(sep_idx) = sep_idx {
        let key = &content[..sep_idx];
        let value = &content[sep_idx + 1..];
        let sep = &content[sep_idx..sep_idx + 1];
        // 只有值看起来像"真实的密钥/配置值"才脱敏，避免误伤代码行
        // 例如 `let token = get_token();` 应原样保留
        let value_trimmed = value.trim();
        let code_like = value_trimmed.contains(';')
            || value_trimmed.contains('(')
            || value_trimmed.contains('{');
        if key_is_sensitive && !value_trimmed.is_empty() && !code_like {
            return format!("{prefix}{key}{sep}[REDACTED]");
        }
    }
    line.to_string()
}

/// 将一行替换为脱敏占位符，同时保留 diff 前缀符号（+/-/空格）。
fn format_redacted_line(line: &str, placeholder: &str) -> String {
    let (prefix, _) = split_diff_prefix(line);
    format!("{prefix}{placeholder}")
}

/// 把 diff 行拆分为前缀符号（+/–/空格）与正文。
fn split_diff_prefix(line: &str) -> (&str, &str) {
    match line.as_bytes().first() {
        Some(b'+') | Some(b'-') => line.split_at(1),
        _ => ("", line),
    }
}

/// 按最大字符数截断，尽量在完整行边界处切断，并附加截断提示。
fn truncate(text: &str, max_chars: usize) -> (String, bool) {
    if text.len() <= max_chars || max_chars == 0 {
        return (text.to_string(), false);
    }

    // 寻找不超过 max_chars 的最后一个换行位置，保证截断处行完整
    let mut last_newline = 0;
    let mut char_end = 0;
    for (idx, ch) in text.char_indices() {
        let end = idx + ch.len_utf8();
        if end > max_chars {
            break;
        }
        if ch == '\n' {
            last_newline = end;
        }
        char_end = end;
    }

    let cutoff = if last_newline > 0 {
        last_newline
    } else {
        char_end
    };
    let mut result = text[..cutoff].to_string();
    result.push_str(TRUNCATION_NOTE);
    (result, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_diff_unchanged() {
        let proc = DiffProcessor::new(100_000);
        let out = proc.process("diff --git a/x b/x\n+hello\n");
        assert!(!out.truncated);
        assert_eq!(out.text, "diff --git a/x b/x\n+hello\n");
    }

    #[test]
    fn redacts_key_value_secret() {
        let proc = DiffProcessor::new(100_000);
        let out =
            proc.process("diff --git a/.env b/.env\n+API_KEY=sk-abc123\n+DB_PASSWORD=secret\n");
        assert!(out.has_secrets());
        assert!(out.text.contains("API_KEY=[REDACTED]"));
        assert!(out.text.contains("DB_PASSWORD=[REDACTED]"));
        assert!(!out.text.contains("sk-abc123"));
        // 明细应包含文件路径
        assert!(out.secrets.iter().all(|f| f.path == ".env"));
    }

    #[test]
    fn redacts_private_key_block() {
        let proc = DiffProcessor::new(100_000);
        let raw = "diff --git a/id_rsa b/id_rsa\n+-----BEGIN RSA PRIVATE KEY-----\n+MIIEpAIBAAKCAQEA\n+-----END RSA PRIVATE KEY-----\n";
        let out = proc.process(raw);
        assert!(out.has_secrets());
        assert!(out.text.contains("[REDACTED PRIVATE KEY BLOCK]"));
        assert!(!out.text.contains("MIIEpAIBAAKCAQEA"));
        assert!(out.secrets.iter().any(|f| f.path == "id_rsa"));
    }

    #[test]
    fn detects_sensitive_path() {
        let proc = DiffProcessor::new(100_000);
        let out = proc.process("diff --git a/.env b/.env\n+FOO=bar\n");
        assert!(out.has_secrets());
        assert!(out.secrets.iter().any(|f| f.path == ".env"));
    }

    #[test]
    fn secrets_include_file_path_and_detail() {
        let proc = DiffProcessor::new(100_000);
        let out = proc.process("diff --git a/config.toml b/config.toml\n+PASSWORD=hunter2\n");
        assert!(out.has_secrets());
        let finding = &out.secrets[0];
        assert_eq!(finding.path, "config.toml");
        assert!(finding.detail.contains("PASSWORD=hunter2"));
    }

    #[test]
    fn no_secrets_for_normal_diff() {
        let proc = DiffProcessor::new(100_000);
        let out = proc.process("diff --git a/src/main.rs b/src/main.rs\n+fn main() {}\n");
        assert!(!out.has_secrets());
        assert!(out.secrets.is_empty());
    }

    #[test]
    fn truncates_large_diff_and_adds_note() {
        let proc = DiffProcessor::new(50);
        let big = format!(
            "diff --git a/x b/x\n{}\n",
            "+a very long line that should be cut off somewhere".repeat(20)
        );
        let out = proc.process(&big);
        assert!(out.truncated);
        assert!(out.text.contains("truncated due to context limits"));
        // 截断后的正文部分（不含 note）不超过上限
        let body = out
            .text
            .split("NOTE: The diff has been truncated")
            .next()
            .unwrap();
        assert!(body.len() <= 50);
    }

    #[test]
    fn truncated_diff_keeps_complete_lines() {
        let proc = DiffProcessor::new(20);
        let raw = "line one\nline two\nline three\n";
        let out = proc.process(raw);
        assert!(out.truncated);
        // 截断点应在换行后，且保留行完整性
        assert!(out.text.starts_with("line one\n"));
    }

    #[test]
    fn no_false_redaction_without_separator() {
        let proc = DiffProcessor::new(100_000);
        let raw = "diff --git a/src/main.rs b/src/main.rs\n+let token = get_token();\n";
        let out = proc.process(raw);
        // 无 `key=value` 分隔符形式，不应被改写
        assert!(!out.text.contains("[REDACTED]"));
    }
}
