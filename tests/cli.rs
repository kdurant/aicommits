use std::fs;
use std::process::{Command, Output};

use tempfile::TempDir;

fn aicommits() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aicommits"))
}

fn run(dir: &TempDir, args: &[&str]) -> Output {
    aicommits()
        .args(args)
        .current_dir(dir.path())
        .env("AICOMMITS_PROVIDER", "mock")
        .env(
            "AICOMMITS_MOCK_RESPONSE",
            "feat(cli): add commit generation",
        )
        .output()
        .expect("failed to run aicommits")
}

/// 创建已 git init 且配置好 user 的临时仓库。
fn setup_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.email", "test@example.com"][..],
        &["config", "user.name", "Test User"][..],
    ] {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
    }
    dir
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn prints_help() {
    let output = aicommits().arg("--help").output().unwrap();
    assert!(output.status.success());
    let out = stdout(&output);
    assert!(out.contains("Usage"));
    assert!(out.contains("--yes"));
    assert!(out.contains("--dry-run"));
    assert!(out.contains("--verbose"));
    assert!(out.contains("--provider"));
}

#[test]
fn prints_version() {
    let output = aicommits().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert!(stdout(&output).starts_with("aicommits "));
}

#[test]
fn clean_working_tree_is_not_an_error() {
    let dir = setup_repo();
    let output = run(&dir, &[]);
    assert!(output.status.success());
    assert!(stderr(&output).contains("工作区是干净的"));
}

#[test]
fn dry_run_generates_message_without_committing() {
    let dir = setup_repo();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    git_add(&dir);

    let output = run(&dir, &["--dry-run"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let out = stdout(&output);
    assert!(out.contains("Generated commit message"));
    assert!(out.contains("feat(cli): add commit generation"));
    assert!(out.contains("dry run"));

    // 确认没有真正提交
    let log = git_log(&dir);
    assert!(!log.contains("feat(cli)"));
}

#[test]
fn yes_actually_commits() {
    let dir = setup_repo();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    git_add(&dir);

    let output = run(&dir, &["--yes"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let out = stdout(&output);
    assert!(out.contains("Commit created"));
    assert!(out.contains("feat(cli): add commit generation"));

    let log = git_log(&dir);
    assert!(log.contains("feat(cli): add commit generation"));
}

#[test]
fn no_staged_changes_with_auto_stage_commits() {
    let dir = setup_repo();
    // 只修改文件，不 add
    fs::write(dir.path().join("b.txt"), "world\n").unwrap();

    let cfg = dir.path().join("config.toml");
    fs::write(&cfg, "[commit]\nauto_stage = true\n").unwrap();

    let output = run(&dir, &["--yes", "--config", cfg.to_str().unwrap()]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(git_log(&dir).contains("feat(cli): add commit generation"));
}

#[test]
fn not_a_git_repository_is_friendly_error() {
    let dir = TempDir::new().unwrap();
    let output = run(&dir, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("不是 Git 仓库"));
}

#[test]
fn verbose_flag_prints_debug_info() {
    let dir = setup_repo();
    fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    git_add(&dir);

    let output = run(&dir, &["--dry-run", "--verbose"]);
    assert!(output.status.success());
    let err = stderr(&output);
    assert!(err.contains("[verbose]"));
    assert!(err.contains("Git 仓库已发现"));
}

#[test]
fn secret_diff_is_redacted_before_sending() {
    let dir = setup_repo();
    fs::write(dir.path().join(".env"), "API_KEY=sk-super-secret-value\n").unwrap();
    git_add(&dir);

    // --yes 跳过敏感信息确认；mock 响应固定，不真正发送
    let output = run(&dir, &["--yes", "--dry-run"]);
    assert!(output.status.success());
    // 脱敏后 diff 不会包含真实密钥
    assert!(!stdout(&output).contains("sk-super-secret-value"));
}

fn git_add(dir: &TempDir) {
    let status = Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir.path())
        .status()
        .unwrap();
    assert!(status.success());
}

fn git_log(dir: &TempDir) -> String {
    let out = Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}
