use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};

/// 汇总 diff 的基本统计信息，用于终端展示。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffSummary {
    pub files: usize,
    pub insertions: usize,
    pub deletions: usize,
}

/// 提供给 AI 的 Git 上下文，聚合了生成 commit message 所需的全部信息。
#[derive(Debug, Clone)]
pub struct GitContext {
    pub branch: String,
    pub recent_commits: Vec<String>,
    pub diff: String,
    pub truncated: bool,
    pub has_secrets: bool,
    pub summary: DiffSummary,
}

/// 集中封装所有 Git 操作，业务代码不直接调用 git 命令。
#[derive(Debug, Clone)]
pub struct GitRepository {
    root: PathBuf,
}

impl GitRepository {
    /// 从当前目录（或任意子目录）向上查找 Git 仓库根目录。
    pub fn discover() -> Result<Self> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .map_err(|_| Error::NotGitRepository)?;
        if !output.status.success() {
            return Err(Error::NotGitRepository);
        }
        let root = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
        Ok(Self { root })
    }

    /// 仓库根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 执行 git 命令，成功时返回 stdout，失败时返回带 stderr 的友好错误。
    fn run(&self, args: &[&str]) -> Result<String> {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .output()
            .map_err(Error::Io)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(Error::GitCommandFailed {
                command: format!("git {}", args.join(" ")),
                stderr,
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// 执行 git 命令并忽略非零退出码，返回退出码（用于有意义的检查命令）。
    fn run_exit_code(&self, args: &[&str]) -> Result<i32> {
        let status = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .status()
            .map_err(Error::Io)?;
        Ok(status.code().unwrap_or(1))
    }

    /// `git status --short`，逐行返回工作区变更。
    pub fn status(&self) -> Result<Vec<String>> {
        let out = self.run(&["status", "--short"])?;
        if out.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(out.lines().map(str::to_string).collect())
        }
    }

    /// `git diff --cached`，返回暂存区 diff 原文。
    pub fn staged_diff(&self) -> Result<String> {
        self.run(&["diff", "--cached"])
    }

    /// 是否存在暂存区变更。
    pub fn has_staged_changes(&self) -> Result<bool> {
        // 0 表示无差异，1 表示有差异
        Ok(self.run_exit_code(&["diff", "--cached", "--quiet"])? != 0)
    }

    /// `git log -n count --oneline`，返回最近若干条提交。
    pub fn recent_commits(&self, count: usize) -> Result<Vec<String>> {
        let count = count.to_string();
        let output = Command::new("git")
            .args(["log", &format!("-n{count}"), "--oneline"])
            .current_dir(&self.root)
            .output()
            .map_err(Error::Io)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            // 空仓库还没有任何提交，这不是错误
            if stderr.contains("does not have any commits") || stderr.contains("尚无任何提交")
            {
                return Ok(Vec::new());
            }
            return Err(Error::GitCommandFailed {
                command: format!("git log -n{count} --oneline"),
                stderr,
            });
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(stdout.lines().map(str::to_string).collect())
        }
    }

    /// `git branch --show-current`，返回当前分支名（HEAD 分离时可能为空）。
    pub fn current_branch(&self) -> Result<String> {
        self.run(&["branch", "--show-current"])
    }

    /// `git add -A`，暂存所有变更。
    pub fn stage_all(&self) -> Result<()> {
        let status = self.run_exit_code(&["add", "-A"])?;
        if status != 0 {
            // run_exit_code 不抛错，这里重新读取 stderr 以生成友好错误
            let output = Command::new("git")
                .args(["add", "-A"])
                .current_dir(&self.root)
                .output()
                .map_err(Error::Io)?;
            return Err(Error::GitAddFailed {
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        Ok(())
    }

    /// `git commit -m ...`，成功后返回短 commit hash。
    pub fn commit(&self, message: &str) -> Result<String> {
        let output = Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(&self.root)
            .output()
            .map_err(Error::Io)?;
        if !output.status.success() {
            return Err(Error::CommitFailed {
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        let hash = self.run(&["rev-parse", "--short", "HEAD"])?;
        Ok(hash)
    }

    /// 统计暂存区变更的文件数、新增行数和删除行数。
    pub fn diff_summary(&self) -> Result<DiffSummary> {
        let out = self.run(&["diff", "--cached", "--numstat"])?;
        let mut summary = DiffSummary::default();
        for line in out.lines() {
            let mut parts = line.split('\t');
            let add = parts.next().unwrap_or("0");
            let del = parts.next().unwrap_or("0");
            // 二进制文件在 numstat 中为 "-"；跳过非数字
            if let (Ok(add), Ok(del)) = (add.parse::<usize>(), del.parse::<usize>()) {
                summary.insertions += add;
                summary.deletions += del;
            }
            summary.files += 1;
        }
        Ok(summary)
    }

    /// 收集生成 commit message 所需的完整上下文。
    pub fn context(
        &self,
        diff: String,
        truncated: bool,
        has_secrets: bool,
        summary: DiffSummary,
    ) -> Result<GitContext> {
        let branch = self.current_branch()?;
        let recent_commits = self.recent_commits(5)?;
        Ok(GitContext {
            branch,
            recent_commits,
            diff,
            truncated,
            has_secrets,
            summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// 创建带 git 环境的临时仓库，避免依赖用户全局配置。
    fn setup_repo() -> (TempDir, GitRepository) {
        let dir = TempDir::new().unwrap();
        let cmds: &[&[&str]] = &[
            &["init", "-q", "-b", "main"],
            &["config", "user.email", "test@example.com"],
            &["config", "user.name", "Test User"],
        ];
        for cmd in cmds {
            let status = Command::new("git")
                .args(*cmd)
                .current_dir(dir.path())
                .status()
                .unwrap();
            assert!(status.success(), "git {:?} failed", cmd);
        }
        let repo = GitRepository {
            root: dir.path().to_path_buf(),
        };
        (dir, repo)
    }

    #[test]
    fn discover_finds_repository() {
        let (dir, _repo) = setup_repo();
        with_cwd(dir.path(), || {
            let repo = GitRepository::discover().unwrap();
            assert_eq!(repo.root(), dir.path());
        });
    }

    #[test]
    fn discover_detects_non_repository() {
        let dir = TempDir::new().unwrap();
        with_cwd(dir.path(), || {
            // /tmp 下的临时目录不是 git 仓库
            assert!(matches!(
                GitRepository::discover(),
                Err(Error::NotGitRepository)
            ));
        });
    }

    /// 在指定目录下执行闭包，结束后恢复原工作目录。
    fn with_cwd<F: FnOnce()>(path: &Path, f: F) {
        let orig = std::env::current_dir().unwrap();
        std::env::set_current_dir(path).unwrap();
        f();
        std::env::set_current_dir(orig).unwrap();
    }

    #[test]
    fn staged_diff_and_status() {
        let (_dir, repo) = setup_repo();
        fs::write(repo.root().join("a.txt"), "hello\n").unwrap();
        repo.stage_all().unwrap();

        assert!(repo.has_staged_changes().unwrap());
        let diff = repo.staged_diff().unwrap();
        assert!(diff.contains("a.txt"));
        assert!(diff.contains("+hello"));

        let status = repo.status().unwrap();
        assert_eq!(status.len(), 1);
        assert!(status[0].contains("a.txt"));
    }

    #[test]
    fn clean_repo_has_no_staged_changes() {
        let (_dir, repo) = setup_repo();
        assert!(!repo.has_staged_changes().unwrap());
        assert!(repo.staged_diff().unwrap().is_empty());
        assert!(repo.status().unwrap().is_empty());
    }

    #[test]
    fn branch_detection() {
        let (_dir, repo) = setup_repo();
        assert_eq!(repo.current_branch().unwrap(), "main");
    }

    #[test]
    fn recent_commits_returns_history() {
        let (_dir, repo) = setup_repo();
        fs::write(repo.root().join("b.txt"), "x\n").unwrap();
        repo.stage_all().unwrap();
        repo.commit("feat: initial commit").unwrap();

        let commits = repo.recent_commits(5).unwrap();
        assert_eq!(commits.len(), 1);
        assert!(commits[0].contains("feat: initial commit"));
    }

    #[test]
    fn commit_returns_short_hash() {
        let (_dir, repo) = setup_repo();
        fs::write(repo.root().join("c.txt"), "y\n").unwrap();
        repo.stage_all().unwrap();
        let hash = repo.commit("fix: something").unwrap();
        assert!(hash.len() >= 7);
    }

    #[test]
    fn diff_summary_counts_lines() {
        let (_dir, repo) = setup_repo();
        fs::write(repo.root().join("d.txt"), "l1\nl2\nl3\n").unwrap();
        repo.stage_all().unwrap();
        let summary = repo.diff_summary().unwrap();
        assert_eq!(summary.files, 1);
        assert_eq!(summary.insertions, 3);
        assert_eq!(summary.deletions, 0);
    }

    #[test]
    fn commit_failure_returns_commit_failed() {
        let (_dir, repo) = setup_repo();
        // 没有暂存内容直接 commit 会失败
        let err = repo.commit("nothing staged").unwrap_err();
        assert!(matches!(err, Error::CommitFailed { .. }));
    }

    #[test]
    fn git_command_failure_returns_friendly_error() {
        let (_dir, repo) = setup_repo();
        // 不存在的 git 子命令会失败，返回 GitCommandFailed
        let err = repo.run(&["this-command-does-not-exist"]).unwrap_err();
        assert!(matches!(err, Error::GitCommandFailed { .. }));
    }

    #[test]
    fn recent_commits_is_empty_in_fresh_repo() {
        let (_dir, repo) = setup_repo();
        assert!(repo.recent_commits(5).unwrap().is_empty());
    }
}
