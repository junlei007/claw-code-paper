# Claw Code Rust Workspace

这是当前项目的 **主开发面**：一个以 Rust 实现的本地 agent CLI/runtime 工作区。

在这个仓库里，Rust 层不再只是“移植尝试”，而是当前真正承载产品能力的核心内核。当前方向也不再是单纯做通用 coding agent，而是把这个 CLI/runtime 逐步演进成一个更适合**科研数据分析**的智能体基础设施。

换句话说：

- `rust/` 负责稳定、可持续演进的 CLI/runtime/kernel
- 研究分析差异化能力主要放在插件、prompts、skills、Python/R bridge 上
- 当前最重要的领域插件是 `research-survey`

---

## 最短上手路径

如果你来 `rust/` 目录，目标只是尽快把 survey 模式跑起来，先走这条路径：

```bash
cd rust
~/.cargo/bin/cargo build -p claw-cli
./target/debug/claw init --research survey
```

然后做三件事：

1. 在项目目录里填写 `.claw/settings.local.json`
2. 确认 `.claw/artifacts/` 已创建
3. 按顺序使用：

```text
survey_metadata
  -> survey_score
  -> survey_psychometrics
  -> survey_report
```

如果你要看 survey tool 的详细输入/输出契约，直接跳到：

- [`crates/plugins/bundled/research-survey/README.md`](crates/plugins/bundled/research-survey/README.md)

如果你要看：
- 已经集成了哪些科研方法、标准链路怎么走：
  [`../docs/research-method-registry.md`](../docs/research-method-registry.md)
- 没有集成的方法应该做成 skill 还是 plugin：
  [`../docs/research-method-standards.md`](../docs/research-method-standards.md)
- 如何快速生成一个项目级 skill 草稿：
  [`../docs/project-skill-synthesis.md`](../docs/project-skill-synthesis.md)
- regression external plugin 原型长什么样：
  [`../examples/external-plugins/research-regression/README.md`](../examples/external-plugins/research-regression/README.md)

最短命令入口：

```bash
./target/debug/claw project-skill init survey-cleaning-sop \
  --title "Survey Cleaning SOP" \
  --description "Draft workflow for local survey cleaning." \
  --domain survey \
  --use-when "Use before scoring." \
  --source ../docs/research-method-standards.md
```

---

## Workspace 定位

当前 Rust workspace 的角色是：

1. 提供 `claw` CLI 二进制
2. 提供本地 agent runtime
3. 提供 provider/profile 配置能力
4. 提供插件发现、启停和工具调用能力
5. 作为科研分析插件的宿主内核

这意味着后续即使上游内核继续演进，我们也尽量通过：
- 配置层
- plugin 层
- Python + R 分析桥接层

来扩展产品能力，而不是把科研逻辑深埋进每个 Rust crate 里。

---

## 当前工作区包含什么

当前 Rust workspace 包括：

- `crates/claw-cli`：CLI 入口与 REPL/命令面
- `crates/runtime`：运行时、配置、权限、prompt、session
- `crates/plugins`：插件发现、注册、启用与生命周期
- `crates/api`：provider client 与流式接口
- `crates/tools`：工具注册与执行桥接
- `crates/commands`：slash commands
- `crates/lsp`：LSP 辅助能力
- `crates/server`：服务端相关能力
- `crates/compat-harness`：兼容/辅助桥接

---

## 当前科研分析能力如何接入 Rust 内核

当前科研方向不是直接把统计模型硬编码进 Rust，而是通过插件体系挂载到 runtime 上。

最核心的插件位于：

- `crates/plugins/bundled/research-survey/`

这个插件当前提供：

- `survey_metadata`
- `survey_score`
- `survey_psychometrics`
- `survey_report`

推荐工作流：

```text
survey_metadata
  -> survey_score
  -> survey_psychometrics
  -> survey_report
```

详细说明见：

- [`crates/plugins/bundled/research-survey/README.md`](crates/plugins/bundled/research-survey/README.md)
- [`../docs/research-method-registry.md`](../docs/research-method-registry.md)
- [`../docs/research-method-standards.md`](../docs/research-method-standards.md)

---

## 支持的模型 / Provider

当前 Rust CLI **不绑定 Claude 单一后端**。

已经支持通过 provider profiles 配置 **OpenAI-compatible** 后端，因此可以接：

- DeepSeek
- 其他 OpenAI-compatible API / 网关

这也是当前科研 agent 方向里的重要原则：

> 模型层可替换，科研能力层不要绑定单一模型厂商。

---

## 本地配置示例

### 推荐入口：先用 bootstrap 生成本地配置

最推荐的方式是先运行：

```bash
./target/debug/claw init --research survey
```

它会为当前项目生成：

- `.claw.json`
- `.claw/settings.local.json`
- `.claw/artifacts/`
- `CLAW.md`

生成后的 `CLAW.md` 会直接写出 survey workflow、artifact 路径，以及 Python/R 双后端分工。

### 手动配置示例

Rust runtime 会读取项目配置与本地配置。推荐在仓库根目录使用：

- `.claw/settings.local.json`

示例：

```json
{
  "model": "deepseek-chat",
  "providers": {
    "default": "deepseek",
    "profiles": {
      "deepseek": {
        "type": "openai-compat",
        "providerName": "DeepSeek",
        "apiKeyEnv": "DEEPSEEK_API_KEY",
        "baseUrl": "https://api.deepseek.com/v1",
        "defaultModel": "deepseek-chat"
      }
    }
  },
  "research": {
    "enabled": true,
    "profile": "survey",
    "artifactDir": ".claw/artifacts"
  },
  "plugins": {
    "enabled": {
      "research-survey@bundled": true
    }
  }
}
```

然后配置 key：

```bash
export DEEPSEEK_API_KEY=your_key_here
```

---

## 构建与运行

### 前置要求

- Rust stable toolchain
- Cargo
- Python 3（科研插件 Python 侧需要）
- R（psychometrics / CFA 路径需要）
- 对应 provider 的 API key

### 构建

如果 `cargo` 已在 PATH：

```bash
cd rust
cargo build -p claw-cli
```

如果需要显式路径：

```bash
cd rust
~/.cargo/bin/cargo build -p claw-cli
```

### 查看帮助

```bash
./target/debug/claw --help
```

### 常用命令

```bash
./target/debug/claw agents
./target/debug/claw skills
./target/debug/claw system-prompt --cwd .. --date 2026-04-02
./target/debug/claw
```

### 一次性 prompt

```bash
./target/debug/claw prompt "summarize this workspace"
```

或者：

```bash
./target/debug/claw "summarize this workspace"
```

---

## CLI 当前已验证可用的基本面

当前这几个入口已经过本地验证：

- `claw --help`
- `claw agents`
- `claw skills`
- `claw system-prompt`

科研插件方面，`survey_score` 的最小 fixture 运行也已经验证通过。

---

## 直接体验科研插件

即使先不接模型，也可以先测试插件能力。

例如：

```bash
printf '%s' '{
  "datasetPath": "rust/crates/plugins/bundled/research-survey/fixtures/mini_survey.csv",
  "scaleDefinitions": [{
    "name": "engagement",
    "items": ["q1", "q2", "q3", "q4"],
    "reverseItems": ["q3"],
    "outputColumn": "engagement_mean",
    "method": "mean",
    "minValidItems": 3
  }],
  "reverseItems": ["q3"],
  "responseScale": {"min": 1, "max": 5},
  "idColumns": ["id"]
}' | \
env CLAW_TOOL_NAME=survey_score \
    CLAW_PLUGIN_ID=research-survey@bundled \
    CLAW_PLUGIN_ROOT=$(pwd)/crates/plugins/bundled/research-survey \
    CLAW_WORKSPACE_ROOT=$(cd .. && pwd) \
    python3 crates/plugins/bundled/research-survey/tools/survey_tools.py
```

如果你是在仓库根目录运行，请改用根目录相对路径版本，见仓库首页 README。

---

## 验证命令

建议在 `rust/` 目录下执行：

```bash
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

插件侧最小校验：

```bash
python3 -m py_compile crates/plugins/bundled/research-survey/tools/survey_tools.py
python3 -m json.tool crates/plugins/bundled/research-survey/.claw-plugin/plugin.json >/dev/null
```

---

## 当前限制

当前 Rust workspace 已经能承载主开发线，但仍有一些现实边界：

- 发布方式目前仍以源码构建为主
- 科研插件能力已可用，但还处于“能力基座”阶段
- 更复杂的统计分析链路仍需要继续扩展
- GUI 尚未纳入当前主线，仍然是后续阶段任务
- Python 与 R 依赖需要在本机环境中自行准备

---

## 推荐开发原则

如果后续继续在这个 Rust workspace 上推进科研分析方向，推荐保持以下原则：

- 内核层保持稳定、轻耦合
- 领域能力优先放在插件和技能层
- 优先兼容 OpenAI-compatible provider
- 先把 CLI 工作流做扎实，再补 GUI
- 所有科研统计输出都应可追溯、可复核

---

## Release notes

- Draft 0.1.0 release notes: [`docs/releases/0.1.0.md`](docs/releases/0.1.0.md)

---

## License

See the repository root for licensing details.
