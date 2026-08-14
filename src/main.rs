mod ai;
mod cli;
mod commit;
mod config;
mod diff;
mod error;
mod git;
mod prompt;

use std::process::exit;

use clap::Parser;
use dialoguer::Confirm;
use error::{Error, Result};

use crate::cli::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match run(&cli).await {
        Ok(()) => {}
        Err(err) => {
            match &err {
                // 这些是"非错误"退出，用户可预期，run 内部已打印提示
                Error::Cancelled | Error::CleanWorkingTree => {}
                _ => {
                    eprintln!("\n✗ {err}");
                    if cli.verbose {
                        eprintln!("\n[debug] {err:?}");
                    }
                }
            }
            let code = if matches!(err, Error::Cancelled | Error::CleanWorkingTree) {
                0
            } else {
                1
            };
            exit(code);
        }
    }
}

/// 主流程编排。
async fn run(cli: &Cli) -> Result<()> {
    let config = config::Config::load(cli)?;
    log_verbose(cli.verbose, "配置已加载", |c| {
        c.push_str(&format!(
            "provider={}, model={:?}",
            config.provider, config.model
        ));
    });

    let repo = git::GitRepository::discover()?;
    log_verbose(cli.verbose, "Git 仓库已发现", |c| {
        c.push_str(&format!("root={}", repo.root().display()));
    });

    // 1. 工作区是否干净
    let status = repo.status()?;
    if status.is_empty() {
        eprintln!("工作区是干净的，没有任何变更需要提交。");
        return Err(Error::CleanWorkingTree);
    }

    // 2. 没有 staged changes 时的处理
    if !repo.has_staged_changes()? {
        if cli.yes || config.auto_stage {
            log_verbose(cli.verbose, "自动暂存（--yes 或 auto_stage=true）", |_| {});
            repo.stage_all()?;
        } else if confirm("没有暂存的变更。是否自动执行 `git add -A`？", true)? {
            repo.stage_all()?;
        } else {
            eprintln!("Commit cancelled.");
            return Err(Error::Cancelled);
        }
    }

    // 3. 获取并处理 diff
    let raw_diff = repo.staged_diff()?;
    if raw_diff.trim().is_empty() {
        return Err(Error::NoStagedChanges);
    }
    let summary = repo.diff_summary()?;
    let processor = diff::DiffProcessor::new(config.max_diff_chars);
    let processed = processor.process(&raw_diff);
    log_verbose(cli.verbose, "diff 处理完成", |c| {
        c.push_str(&format!(
            "raw={} chars, processed={} chars, truncated={}",
            raw_diff.len(),
            processed.text.len(),
            processed.truncated,
        ));
    });

    // 4. 收集 Git 上下文
    let context = repo.context(processed.text, processed.truncated, summary)?;
    log_verbose(cli.verbose, "Git 上下文已收集", |c| {
        c.push_str(&format!(
            "branch={}, recent_commits={}",
            context.branch,
            context.recent_commits.len()
        ));
    });

    // 5. 展示基本信息
    println!("\nAnalyzing Git changes...\n");
    println!("Branch: {}", context.branch);
    println!(
        "Changes: {} files, +{} -{}",
        context.summary.files, context.summary.insertions, context.summary.deletions
    );

    // 6. 调用 AI 生成 commit message
    let provider = ai::create(&config)?;
    log_verbose(cli.verbose, "AI provider 已创建", |c| {
        c.push_str(&format!(
            "provider={}, model={:?}, api_key={}, base_url={}",
            config.provider,
            config.model,
            if config.api_key.is_some() {
                "config/env"
            } else {
                "env/auth.json"
            },
            config.base_url
        ));
    });
    let message = provider.generate_commit_message(&context).await?;
    log_verbose(cli.verbose, "AI 响应已解析", |c| {
        c.push_str(&format!("subject={}", message.subject));
    });

    // 7. 校验并提示
    for warning in message.check(&config) {
        eprintln!("⚠ {warning}");
    }

    // 8. 展示结果
    println!("\nGenerated commit message:\n");
    println!("  {}", message.subject);
    if !message.body.is_empty() {
        println!();
        for line in &message.body {
            println!("  {line}");
        }
    }
    println!();

    // 9. dry-run 提前结束
    if cli.dry_run {
        println!("（dry run）未执行提交。");
        return Ok(());
    }

    // 10. 确认提交
    if !cli.yes && !confirm("Commit this change?", true)? {
        eprintln!("Commit cancelled.");
        return Err(Error::Cancelled);
    }

    // 11. 执行提交
    let hash = repo.commit(&message.formatted())?;
    println!("✓ Commit created\n");
    println!("[{}] {}", hash, message.subject);
    Ok(())
}

/// 交互确认，默认值由 caller 指定。
fn confirm(prompt: &str, default: bool) -> Result<bool> {
    Confirm::new()
        .with_prompt(prompt)
        .default(default)
        .interact()
        .map_err(|e| Error::Interactive(e.to_string()))
}

/// --verbose 调试日志，输出到 stderr。
fn log_verbose(enabled: bool, label: &str, detail: impl FnOnce(&mut String)) {
    if !enabled {
        return;
    }
    let mut detail_str = String::new();
    detail(&mut detail_str);
    if detail_str.is_empty() {
        eprintln!("[verbose] {label}");
    } else {
        eprintln!("[verbose] {label}: {detail_str}");
    }
}
