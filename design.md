# 项目：aicommits

请使用 Rust 从零实现一个命令行工具 `aicommits`，核心目标是：

> 分析当前 Git 仓库的代码变更，使用 AI 自动生成合适的 Git commit message，并可交互确认后执行 `git commit`。

## 1. 核心功能

执行：

```bash
aicommits
```

默认行为：

1. 检查当前目录是否为 Git repository。
2. 获取当前 Git 工作区状态。
3. 获取 staged changes。
4. 如果没有 staged changes，给出明确提示，并询问是否自动执行：

   ```bash
   git add -A
   ```
5. 获取：

   ```bash
   git diff --cached
   ```
6. 将 diff、必要的 Git context 发送给 AI。
7. AI 生成 commit message。
8. 在终端展示生成结果。
9. 用户确认后执行：

   ```bash
   git commit -m "..."
   ```
10. commit 成功后显示 commit hash 和简要结果。

同时支持：

```bash
aicommits --yes
aicommits -y
```

跳过人工确认，直接 commit。

支持：

```bash
aicommits --dry-run
```

只生成 commit message，不执行 commit。

支持：

```bash
aicommits --verbose
```

显示更多 Git 和 AI 请求调试信息。

---

## 2. AI Provider

默认使用 OpenCode Go 订阅。

设计一个抽象的 AI provider：

```rust
trait AiProvider {
    async fn generate_commit_message(
        &self,
        context: &GitContext,
    ) -> Result<CommitMessage>;
}
```

不要把 OpenCode Go 的实现硬编码到业务逻辑中。

代码结构应该允许以后增加：

* OpenCode
* OpenAI
* Anthropic
* DeepSeek
* Ollama
* 其他 OpenAI-compatible API

例如：

```text
src/
├── main.rs
├── cli.rs
├── config.rs
├── error.rs
├── git.rs
├── prompt.rs
├── commit.rs
└── ai/
    ├── mod.rs
    ├── provider.rs
    └── opencode.rs
```

如果 OpenCode Go 实际上是通过 OpenCode CLI 或 OpenAI-compatible API 使用，请先调查当前 OpenCode Go 的实际接口和认证方式，不要猜测 API。

认证信息不要写死在代码中。

---

## 3. Git 操作

使用 Rust 原生方式调用 Git，例如：

```rust
std::process::Command
```

或者选择成熟的 Git crate。

至少封装以下操作：

```rust
git status --short
git diff --cached
git diff
git rev-parse --show-toplevel
git branch --show-current
git log -5 --oneline
git commit -m "..."
git add -A
```

Git 操作不要散落在 `main.rs`。

应该集中封装在 `git.rs` 中。

例如：

```rust
pub struct GitRepository {
    root: PathBuf,
}

impl GitRepository {
    pub fn discover() -> Result<Self>;
    pub fn status(&self) -> Result<GitStatus>;
    pub fn staged_diff(&self) -> Result<String>;
    pub fn recent_commits(&self, count: usize) -> Result<Vec<String>>;
    pub fn current_branch(&self) -> Result<String>;
    pub fn stage_all(&self) -> Result<()>;
    pub fn commit(&self, message: &str) -> Result<String>;
}
```

---

## 4. Commit Message 生成规则

AI 不应该简单总结 diff，而应该生成适合 Git history 的 commit message。

默认采用 Conventional Commits：

```text
<type>(<scope>): <description>
```

例如：

```text
feat(cli): add interactive commit confirmation
fix(git): handle repositories without staged changes
refactor(ai): introduce provider abstraction
docs: update installation instructions
```

支持的 type：

```text
feat
fix
docs
style
refactor
perf
test
build
ci
chore
revert
```

默认生成一行简洁的 commit subject。

subject 尽量：

* 不超过 72 个字符
* 使用祈使语气
* 不以句号结尾
* 准确描述实际代码变化
* 不夸大 diff 中没有实现的功能
* 不包含 AI 自我描述
* 不包含 Markdown code fence

如果 diff 足够复杂，可以生成：

```text
feat(cli): add AI-powered commit generation

- analyze staged Git changes
- generate Conventional Commit messages
- support interactive confirmation
```

但默认优先使用简洁的一行 commit message。

---

## 5. AI Prompt

AI prompt 必须要求模型：

1. 只根据实际 Git diff 判断修改内容。
2. 不要编造不存在的功能。
3. 优先查看 staged diff。
4. 可以参考最近几次 commit 的风格。
5. 输出最终 commit message，不要输出解释。
6. 遵循 Conventional Commits。
7. subject 不超过 72 个字符。
8. 如果存在多个独立修改，选择能够概括整体变化的 commit message。
9. 不要输出：

   * `Here is the commit message`
   * Markdown code block
   * 分析过程
   * AI 自我介绍

例如：

```text
You are an expert software engineer.

Analyze the staged Git changes and generate a high-quality Git commit message.

Requirements:

- Follow Conventional Commits.
- Use: type(scope): description when a scope is useful.
- Keep the subject under 72 characters.
- Use imperative mood.
- Do not invent functionality.
- Base the message only on the actual changes.
- Prefer a concise single-line subject.
- Do not include Markdown.
- Do not explain your reasoning.
- Output only the commit message.

Repository context:

Branch:
{branch}

Recent commits:
{recent_commits}

Staged diff:
{diff}
```

请将 prompt 独立放在 `prompt.rs` 中，而不是散落在业务代码中。

---

## 6. Diff 处理

这是项目的重要部分。

不要无条件把整个：

```bash
git diff --cached
```

直接发送给 AI。

需要考虑：

* 巨大的 diff
* 二进制文件
* lock 文件
* generated files
* minified files
* 大型日志
* 单个文件极大
* token/context limit

设计一个 diff processor。

例如：

```rust
pub struct DiffProcessor {
    max_chars: usize,
}

impl DiffProcessor {
    pub fn process(&self, diff: &str) -> ProcessedDiff;
}
```

第一版可以采用简单的字符限制策略，但代码结构应该允许以后升级成：

* 按文件截断
* 排除二进制文件
* 排除 generated files
* 优先保留文件名和 hunk header
* token-based truncation

如果 diff 被截断，需要明确告诉 AI：

```text
NOTE: The diff has been truncated due to context limits.
Do not assume that omitted changes do not exist.
```

---

## 7. CLI

使用成熟的 Rust CLI crate，例如：

```text
clap
```

建议支持：

```bash
aicommits
aicommits -y
aicommits --yes
aicommits --dry-run
aicommits --verbose
aicommits --provider opencode
aicommits --config
aicommits --help
aicommits --version
```

CLI 参数设计要保持简单，不要一开始加入大量不必要的参数。

---

## 8. 配置

设计配置文件，例如：

```text
~/.config/aicommits/config.toml
```

示例：

```toml
provider = "opencode"

[commit]
conventional = true
max_subject_length = 72
auto_stage = false

[ai]
model = "..."
```

同时支持环境变量覆盖配置。

例如：

```text
AICOMMITS_PROVIDER
AICOMMITS_MODEL
AICOMMITS_API_KEY
```

不要把 API key 写入 Git repository。

---

## 9. 交互体验

CLI 应该具有比较好的终端体验。

例如：

```text
$ aicommits

Analyzing Git changes...

Branch: feature/ai-commit
Changes: 4 files, +128 -37

Generated commit message:

  feat(cli): add AI-powered commit generation

Commit this change? [Y/n]
```

用户输入：

```text
y
```

然后：

```text
✓ Commit created

[abc1234] feat(cli): add AI-powered commit generation
```

如果用户输入：

```text
n
```

则：

```text
Commit cancelled.
```

如果 AI 请求失败，不应该丢失 Git 工作区修改。

---

## 10. 错误处理

不要使用：

```rust
unwrap()
expect()
```

处理正常运行过程中可能发生的错误。

建立统一错误类型，例如：

```rust
thiserror
```

错误至少覆盖：

* 当前目录不是 Git repository
* Git command failed
* 没有 staged changes
* `git add` 失败
* AI provider 配置错误
* API authentication 失败
* 网络错误
* AI 返回格式错误
* commit 失败
* 用户取消操作

错误信息应该对 CLI 用户友好。

例如不要直接：

```text
Error: reqwest::Error { ... }
```

而应该：

```text
✗ Failed to contact OpenCode

Please check your OpenCode authentication and network connection.
```

调试信息通过：

```bash
aicommits --verbose
```

显示。

---

## 11. 安全要求

非常重要：

不要把以下内容无条件发送给 AI：

* `.env`
* credentials
* private keys
* passwords
* tokens
* SSH keys
* secret configuration

至少实现一个基本的 secret redaction 层。

例如检测：

```text
API_KEY=
SECRET=
PASSWORD=
TOKEN=
PRIVATE KEY
```

以及常见 private key：

```text
-----BEGIN PRIVATE KEY-----
-----BEGIN RSA PRIVATE KEY-----
```

对于明显包含敏感信息的 diff，应该：

1. 警告用户；
2. 尽可能进行脱敏；
3. 或要求用户确认后再发送。

---

## 12. 测试

不要只实现代码。

需要编写测试：

### Git

测试：

* repository discovery
* staged diff
* branch detection
* recent commits
* commit message
* git command failure

### Prompt

测试：

* prompt 正确包含 diff
* prompt 正确包含 branch
* prompt 正确包含 recent commits
* 超长 diff 截断

### Commit Message

测试：

```text
feat(cli): add commit generation
```

可以正确解析。

测试：

````text
```text
feat(cli): add commit generation
````

````

能够正确清理 Markdown code fence。

### CLI

测试：

```bash
aicommits --help
aicommits --version
aicommits --dry-run
````

---

## 13. Rust 技术要求

使用稳定版 Rust。

优先选择成熟、维护良好的 crate。

建议：

```text
clap
tokio
reqwest
serde
serde_json
toml
thiserror
anyhow
dialoguer
console
```

如果实际实现中不需要某个 crate，不要为了凑数量而添加。

代码要求：

* Rust 2024 edition
* `cargo fmt`
* `cargo clippy`
* `cargo test`
* 尽可能避免不必要的 clone
* 正确处理 async
* 模块职责清晰
* 不要把所有代码写到 main.rs
* 不要过度设计

---

## 14. 项目开发方式

请按照以下顺序实现：

### Phase 1

完成：

```text
CLI
↓
Git repository
↓
staged diff
↓
模拟 AI
↓
生成 commit message
↓
用户确认
↓
git commit
```

先确保整个流程能够工作。

### Phase 2

接入 OpenCode Go。

### Phase 3

增加：

* 配置文件
* 环境变量
* diff truncation
* secret redaction
* verbose logging

### Phase 4

完善测试和错误处理。

### Phase 5

完善 README、安装方式和发布流程。

---

## 15. README

最终 README 至少包含：

```text
# aicommits

## Features

## Installation

## Configuration

## OpenCode Go

## Usage

## Examples

## Configuration File

## Security

## Development

## Testing

## License
```

提供：

```bash
cargo install aicommits
```

以及从 GitHub Release 安装的说明。

---

## 16. 重要要求

在开始编码之前：

1. 先检查当前项目目录结构。
2. 如果项目已经存在代码，先理解现有代码，不要直接覆盖。
3. 确认 OpenCode Go 当前可用的认证/API方式，不要猜测。
4. 给出一个简短的实现计划。
5. 然后直接开始实现。

实现过程中：

* 每完成一个阶段就运行测试。
* 遇到编译错误立即修复。
* 最终运行：

  ```bash
  cargo fmt --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test
  cargo build --release
  ```
* 最终检查是否存在不必要的依赖、unwrap、死代码和明显安全问题。

不要为了“看起来完整”而过度增加功能。

优先目标是：

> **做出一个小而可靠、真正能每天使用的 Rust `aicommits` CLI。**
