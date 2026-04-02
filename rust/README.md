# Claw Code Rust Workspace

这里是当前项目的**主开发面**。

如果把整个仓库看成一个科研分析 agent 工作台，那么：

- `rust/` = 稳定内核
- `plugins/` = 可执行研究方法
- `docs/` = 方法治理与扩展规范
- `Python / R` = 具体分析 substrate

---

## 你从这里最常见的三条路径

### 1. 跑通已经集成的 survey 分析链路

```bash
cd rust
~/.cargo/bin/cargo build -p claw-cli
./target/debug/claw init --research survey
```

然后按顺序使用：

```text
survey_metadata
  -> survey_score
  -> survey_psychometrics
  -> survey_report
```

详细输入/输出：

- [`../docs/survey-minimal-walkthrough.md`](../docs/survey-minimal-walkthrough.md)
- [`../docs/research-user-playbook.md`](../docs/research-user-playbook.md)
- [`crates/plugins/bundled/research-survey/README.md`](crates/plugins/bundled/research-survey/README.md)

---

### 2. 生成一个项目级 skill 草稿

```bash
./target/debug/claw project-skill init survey-cleaning-sop \
  --title "Survey Cleaning SOP" \
  --description "Draft workflow for local survey cleaning." \
  --domain survey \
  --use-when "Use before scoring." \
  --source ../docs/research-method-standards.md
```

校验：

```bash
./target/debug/claw project-skill validate ./.claw/project-skills/survey-cleaning-sop
```

说明文档：

- [`../docs/project-skill-synthesis.md`](../docs/project-skill-synthesis.md)

---

### 3. 安装一个 external plugin 原型

```bash
./target/debug/claw plugins install ../examples/external-plugins/research-regression
./target/debug/claw plugins list
```

示例插件：

- [`../examples/external-plugins/research-regression/README.md`](../examples/external-plugins/research-regression/README.md)
- [`../docs/research-extension-demo.md`](../docs/research-extension-demo.md)

---

## 当前 Rust workspace 负责什么

1. 提供 `claw` CLI 二进制
2. 提供本地 runtime
3. 提供 provider/profile 配置能力
4. 提供 plugin 发现、启停和 tool 调用能力
5. 作为科研分析插件的宿主内核

设计原则：

- 不把每个研究方法都硬编码进 Rust
- 优先通过 plugin / skill / Python-R bridge 扩展
- 保持 runtime 和方法层解耦

---

## 当前科研能力如何挂进 Rust 内核

当前最核心的插件是：

- `crates/plugins/bundled/research-survey/`

它提供：

- `survey_metadata`
- `survey_score`
- `survey_psychometrics`
- `survey_report`

治理与方法文档：

- [`../docs/research-method-registry.md`](../docs/research-method-registry.md)
- [`../docs/research-method-standards.md`](../docs/research-method-standards.md)

---

## 常用命令

```bash
./target/debug/claw --help
./target/debug/claw agents
./target/debug/claw skills
./target/debug/claw plugins list
./target/debug/claw system-prompt --cwd .. --date 2026-04-02
./target/debug/claw
```

---

## 配置说明

推荐先运行：

```bash
./target/debug/claw init --research survey
```

它会生成：

- `.claw.json`
- `.claw/settings.local.json`
- `.claw/artifacts/`
- `CLAW.md`

如果你手动配置 provider，通常改的是仓库根目录下：

- `.claw/settings.local.json`

---

## 前置要求

- Rust stable toolchain
- Cargo
- Python 3
- R
- 对应 provider 的 API key

---

## 当前重点不是做什么

不是去重写整套 runtime。

当前真正优先的是：

- 把已集成的研究方法链路打磨顺
- 把缺失方法的扩展路径理顺
- 让 skill / external plugin / bundled plugin 的边界清楚
