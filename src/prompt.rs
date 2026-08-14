use crate::git::GitContext;

/// 构建发送给 AI 的完整 prompt。
///
/// 集中管理 prompt 文本，避免散落在业务代码中。
/// 输出只包含模型需要看到的信息，不包含任何解释。
pub fn build(context: &GitContext, max_subject_length: usize) -> String {
    let mut prompt = String::new();
    prompt.push_str("You are an expert software engineer.\n\n");
    prompt.push_str(
        "Analyze the staged Git changes and generate a high-quality Git commit message.\n\n",
    );
    prompt.push_str("Requirements:\n\n");
    prompt.push_str("- Follow Conventional Commits.\n");
    prompt.push_str("- Use: type(scope): description when a scope is useful.\n");

    prompt.push_str(&format!(
        "- Keep the subject under {max_subject_length} characters.\n"
    ));
    prompt.push_str("- Use imperative mood.\n");
    prompt.push_str("- Write the subject and body in Simplified Chinese.\n");
    prompt.push_str("- Keep the type keyword in English (e.g. feat, fix, docs).\n");
    prompt.push_str("- Do not invent functionality.\n");
    prompt.push_str("- Base the message only on the actual changes.\n");
    prompt.push_str("- Prefer a concise single-line subject.\n");
    prompt.push_str("- Do not include Markdown.\n");
    prompt.push_str("- Do not explain your reasoning.\n");
    prompt.push_str("- Output only the commit message.\n\n");

    prompt.push_str("Repository context:\n\n");
    prompt.push_str("Branch:\n");
    prompt.push_str(&context.branch);
    prompt.push_str("\n\n");

    if !context.recent_commits.is_empty() {
        prompt.push_str("Recent commits (follow this style when appropriate):\n");
        for commit in &context.recent_commits {
            prompt.push_str(commit);
            prompt.push('\n');
        }
        prompt.push('\n');
    }

    prompt.push_str("Staged diff:\n");
    prompt.push_str(&context.diff);

    if context.truncated {
        prompt.push_str("\n\nNOTE: The diff has been truncated due to context limits.\n");
        prompt.push_str("Do not assume that omitted changes do not exist.\n");
    }

    prompt.push('\n');
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::DiffSummary;

    fn sample_context() -> GitContext {
        GitContext {
            branch: "feature/ai-commit".to_string(),
            recent_commits: vec![
                "abc1234 feat: add parser".to_string(),
                "def5678 fix: handle edge case".to_string(),
            ],
            diff: "diff --git a/src/main.rs b/src/main.rs\n+fn main() {}\n".to_string(),
            truncated: false,
            summary: DiffSummary {
                files: 1,
                insertions: 1,
                deletions: 0,
            },
        }
    }

    #[test]
    fn prompt_includes_diff() {
        let prompt = build(&sample_context(), 72);
        assert!(prompt.contains("fn main() {}"));
    }

    #[test]
    fn prompt_includes_branch() {
        let prompt = build(&sample_context(), 72);
        assert!(prompt.contains("feature/ai-commit"));
    }

    #[test]
    fn prompt_includes_recent_commits() {
        let prompt = build(&sample_context(), 72);
        assert!(prompt.contains("abc1234 feat: add parser"));
        assert!(prompt.contains("def5678 fix: handle edge case"));
    }

    #[test]
    fn prompt_mentions_truncation_when_truncated() {
        let mut context = sample_context();
        context.truncated = true;
        let prompt = build(&context, 72);
        assert!(prompt.contains("truncated due to context limits"));
        assert!(prompt.contains("Do not assume that omitted changes do not exist"));
    }

    #[test]
    fn prompt_no_truncation_note_when_complete() {
        let prompt = build(&sample_context(), 72);
        assert!(!prompt.contains("truncated due to context limits"));
    }

    #[test]
    fn prompt_respects_max_subject_length() {
        let prompt = build(&sample_context(), 50);
        assert!(prompt.contains("under 50 characters"));
    }
}
