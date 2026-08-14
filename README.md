# aicommits

分析当前 Git 仓库的代码变更，使用 AI 自动生成符合 Conventional Commits 的 commit message，并可交互确认后执行 `git commit`。

## Features

- 自动分析 staged diff，参考最近提交风格生成 commit message
- 遵循 [Conventional Commits](https://www.conventionalcommits.org/)（可配置）
- 交互确认后执行 `git commit`，支持 `--yes` 全自动
- `--dry-run` 只生成消息不提交
- `--verbose` 显示 Git 与 AI 请求调试信息
- diff 处理器：大 diff 自动截断
- 抽象 AI provider：默认使用 OpenCode Go 订阅的 HTTP API（OpenAI-compatible），无需安装任何 CLI

## Installation

### 从源码安装

```bash
cargo install --path .
```

或者发布后：

```bash
cargo install aicommits
```

### 从 GitHub Release 安装

从 [Releases](https://github.com/anomalyco/opencode/releases)（或项目的 Release 页面）下载对应平台的二进制，解压后放入 `PATH`：

```bash
chmod +x aicommits
sudo mv aicommits /usr/local/bin/
```

## Configuration

### 依赖：OpenCode Go 订阅

默认通过 [OpenCode Go](https://opencode.ai) 订阅的 OpenAI-compatible API 生成 commit message，直接访问 `https://opencode.ai/zen/go/v1`，无需安装 opencode CLI。

提供 API key 的方式（优先级从高到低）：

1. 环境变量 `AICOMMITS_API_KEY`
2. 环境变量 `OPENCODE_API_KEY`
3. 本机 opencode 的 `auth.json`（`~/.local/share/opencode/auth.json` 中 `opencode-go` 的 key，若本机已登录 opencode 会自动复用）

```bash
export OPENCODE_API_KEY="sk-..."
```

## Usage

```bash
aicommits           # 交互式：生成消息 → 确认 → 提交
aicommits -y        # 跳过人工确认，直接提交
aicommits --yes
aicommits --dry-run # 只生成消息，不提交
aicommits --verbose # 显示调试信息
aicommits --provider opencode
aicommits --config ~/path/to/config.toml
aicommits --help
aicommits --version
```

当没有 staged changes 时，工具会提示是否自动执行 `git add -A`；`--yes` 或配置 `auto_stage = true` 时会自动暂存。

## Examples

```text
$ aicommits

Analyzing Git changes...

Branch: feature/ai-commit
Changes: 4 files, +128 -37

Generated commit message:

  feat(cli): add AI-powered commit generation

Commit this change? [Y/n]
```

```text
✓ Commit created

[abc1234] feat(cli): add AI-powered commit generation
```

输入 `n` 则取消提交，不会丢失任何工作区修改。

## Configuration File

配置文件默认位于 `~/.config/aicommits/config.toml`，也可以用 `--config <PATH>` 指定。

```toml
provider = "opencode"

[commit]
conventional = true
max_subject_length = 72
auto_stage = false

[ai]
model = "opencode-go/deepseek-v4-flash"  # 可选，默认 deepseek-v4-flash
base_url = "https://opencode.ai/zen/go/v1" # 可选，默认 OpenCode Go 端点
max_diff_chars = 20000                    # 发送给 AI 的 diff 字符上限
timeout_secs = 180                        # AI 请求超时（秒）
```

### 环境变量覆盖

环境变量的优先级高于配置文件：

| 变量                 | 说明                          |
| -------------------- | ----------------------------- |
| `AICOMMITS_PROVIDER` | 覆盖 provider                 |
| `AICOMMITS_MODEL`    | 覆盖模型                      |
| `AICOMMITS_API_KEY`  | OpenCode Go API key           |
| `OPENCODE_API_KEY`   | OpenCode Go API key（次优先） |
| `OPENCODE_BASE_URL`  | 覆盖 API 端点                 |

CLI 参数（`--provider` / `--model`）优先级最高。模型名可以带 `opencode-go/` 前缀（如 `opencode-go/deepseek-v4-flash`），工具会自动去除前缀。

## Security

- 大 diff 会按 `max_diff_chars` 截断，并明确告知 AI「截断的部分可能仍存在变更」。
- API key 通过环境变量 `OPENCODE_API_KEY` / `AICOMMITS_API_KEY` 提供，不要写入 Git 仓库。

## Development

```bash
cargo build
cargo run -- --dry-run   # 在任意 git 仓库中测试
```

模块结构：

```text
src/
├── main.rs          # 入口与流程编排
├── cli.rs           # clap 参数定义
├── config.rs        # 配置文件 + 环境变量
├── error.rs         # 统一错误类型
├── git.rs           # Git 操作封装
├── diff.rs          # diff 截断
├── prompt.rs        # AI prompt 构建
├── commit.rs        # commit message 解析/校验
└── ai/
    ├── mod.rs       # provider 工厂
    ├── provider.rs  # AiProvider trait + mock
    └── opencode.rs  # OpenCode Go HTTP API 集成（OpenAI-compatible）
```

## Testing

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

## License

MIT
