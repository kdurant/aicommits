/// 被截断时的提示，必须明确告知 AI 遗漏的变更可能存在。
const TRUNCATION_NOTE: &str = "\n\nNOTE: The diff has been truncated due to context limits.\nDo not assume that omitted changes do not exist.\n";

/// 处理后的 diff 结果。
#[derive(Debug, Clone, Default)]
pub struct ProcessedDiff {
    pub text: String,
    pub truncated: bool,
}

/// diff 处理器：负责按上下文窗口截断超大 diff。
///
/// 结构上允许以后升级为：
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
        let (text, truncated) = truncate(raw, self.max_chars);
        ProcessedDiff { text, truncated }
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
}
