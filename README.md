# claw-code-paper

一个面向 **科研数据分析** 的 Rust-first agent CLI 工作台。

它把这几件事放进同一套工作流里：

- 运行已经集成的科研分析方法
- 让 agent 按规范调用 Python / R 分析能力
- 从论文、SOP、codebook、内部笔记快速沉淀 skill
- 在方法稳定后，用 external plugin 承载可执行计算能力

---

## 这个项目是干什么的

这个项目不是在做一个泛化的“代码助手”，而是在做一个更适合科研分析场景的 agent 工作台。

它当前最适合的任务包括：

- 问卷 / 量表数据处理
- 反向计分、均分 / 总分生成
- psychometrics 前置检查
- CFA（验证性因子分析）
- 分析结果整理与 Markdown 报告草稿
- 缺失方法的治理化扩展

如果你把它当成一套科研分析操作台，可以简单理解为：

- **Rust core** 负责稳定内核
- **Python / R** 负责具体分析
- **skill** 负责 workflow knowledge / SOP
- **plugin** 负责稳定、可执行、可复用的方法能力

---

## 这个项目有哪些特点

### 1. 已经有一条可运行的 survey 主链路

当前最核心的 bundled plugin 是 `research-survey`，覆盖：

```text
survey_metadata -> survey_score -> survey_psychometrics -> survey_report
```

这意味着项目不只是架构想法，已经有一条可落地的问卷分析链路。

### 2. 内核稳定，方法能力尽量外置

项目的基本立场是：**Rust 内核尽量稳定，方法能力尽量通过 skill / plugin / Python-R bridge 外置扩展。**

这样可以避免把每一种研究方法都硬编码进 runtime。

### 3. 缺方法时，有明确扩展分层

- **skill**：流程知识、SOP、解释框架、用户材料沉淀
- **external plugin**：稳定计算方法、工具契约、可执行分析步骤
- **bundled plugin**：高复用、已收敛、适合进入核心的方法能力

### 4. 支持从材料快速生成 skill

现在已经有正式 CLI：

```bash
cd rust
./target/debug/claw project-skill init survey-cleaning-sop \
  --title "Survey Cleaning SOP" \
  --description "Draft workflow for local survey cleaning." \
  --domain survey \
  --use-when "Use before scoring." \
  --source ../docs/research-method-standards.md
```

### 5. skill 兼容 OpenClaw / Claude 生态

内部 canonical draft 仍然是：

- `.claw/project-skills/<slug>/SKILL.md`
- `.claw/project-skills/<slug>/skill.json`

但可以兼容导出到：

- `skills/<slug>/SKILL.md`
- `.claude/commands/<slug>.md`
- `.claude/agents/<slug>.md`

### 6. “自我进化”走的是受控自扩展

这里强调的是 **controlled self-extension**，不是让系统随意自我改写：

1. 先把资料沉淀成 skill
2. 再把稳定方法沉淀成 plugin
3. 最后才讨论是否进入 bundled core

### 7. provider 可切换，适合科研分析场景落地

`claw init --research survey` 现在会直接脚手架：

- DeepSeek / Kimi / Qwen / generic OpenAI-compatible provider profiles
- `.claw/helpers/plotting.py`
- `.claw/helpers/plotting.R`

所以你可以在一个会话里用 `/provider <profile>` 和 `/model <name>` 切换后端，同时让 agent 生成的 Python / R 作图脚本复用统一的中文绘图 helper。

---

## 怎么快速使用

### 路径 A：先跑通已经集成的方法

```bash
cd rust
~/.cargo/bin/cargo build -p claw-cli
./target/debug/claw init --research survey
```

初始化后的研究脚手架默认包含：

- `.claw/settings.local.json`：DeepSeek / Kimi / Qwen / OpenAI-compatible provider profiles
- `.claw/helpers/plotting.py`
- `.claw/helpers/plotting.R`

进入 REPL 后可以直接切换：

```text
/provider kimi
/model kimi-k2.5

/provider qwen
/model qwen-plus
```

然后按顺序使用：

```text
survey_metadata -> survey_score -> survey_psychometrics -> survey_report
```

详细说明：

- [`rust/README.md`](rust/README.md)
- [`docs/survey-minimal-walkthrough.md`](docs/survey-minimal-walkthrough.md)
- [`rust/crates/plugins/bundled/research-survey/README.md`](rust/crates/plugins/bundled/research-survey/README.md)

### 路径 B：如果你手上已经有论文 / SOP / codebook，先生成 skill

```bash
cd rust
./target/debug/claw project-skill init survey-cleaning-sop \
  --title "Survey Cleaning SOP" \
  --description "Draft workflow for local survey cleaning." \
  --domain survey \
  --use-when "Use before scoring." \
  --source ../docs/research-method-standards.md \
  --source ../docs/research-method-registry.md

./target/debug/claw project-skill validate ./.claw/project-skills/survey-cleaning-sop

./target/debug/claw project-skill doctor ./.claw/project-skills/survey-cleaning-sop

./target/debug/claw project-skill promote ./.claw/project-skills/survey-cleaning-sop \
  --to project \
  --held-out-validation passed
```

`validate` 现在会补充 warning 和 remediation 建议，便于在 promotion 前做最后检查。
`doctor` 适合看非阻断质量问题，比如 placeholder、泛化输出、未解析 source。
这些治理命令也支持 `--output-format json`，便于后续 agent 自动消费。

详细说明：

- [`docs/project-skill-synthesis.md`](docs/project-skill-synthesis.md)
- [`docs/questionnaire-mediation-moderation-rehearsal.md`](docs/questionnaire-mediation-moderation-rehearsal.md)
- [`docs/research-extension-demo.md`](docs/research-extension-demo.md)

### 路径 C：如果你缺的是稳定计算方法，走 external plugin

```bash
cd rust
./target/debug/claw plugins install ../examples/external-plugins/research-regression
./target/debug/claw plugins list
```

详细说明：

- [`examples/external-plugins/research-regression/README.md`](examples/external-plugins/research-regression/README.md)
- [`docs/research-extension-demo.md`](docs/research-extension-demo.md)

---

## 如果没有集成的方法，怎么判断该怎么做

优先按这个规则：

1. **主要是流程知识 / 方法说明 / 操作经验** → 先做 skill
2. **主要是稳定计算 / 工具调用 / 分析契约** → 先做 external plugin
3. **已经跨项目高复用，而且契约稳定** → 再考虑 bundled plugin

如果你想直接按“使用者视角”看操作顺序：

- [`docs/research-user-playbook.md`](docs/research-user-playbook.md)

---

## 文档导航

### 面向使用者

- [`docs/research-user-playbook.md`](docs/research-user-playbook.md)
- [`rust/README.md`](rust/README.md)
- [`rust/crates/plugins/bundled/research-survey/README.md`](rust/crates/plugins/bundled/research-survey/README.md)

### 面向方法治理

- [`docs/research-method-registry.md`](docs/research-method-registry.md)
- [`docs/research-method-standards.md`](docs/research-method-standards.md)

### 面向 skill / plugin 扩展

- [`docs/project-skill-synthesis.md`](docs/project-skill-synthesis.md)
- [`docs/questionnaire-mediation-moderation-rehearsal.md`](docs/questionnaire-mediation-moderation-rehearsal.md)
- [`docs/self-extension-evaluation-checklist.md`](docs/self-extension-evaluation-checklist.md)
- [`docs/research-extension-demo.md`](docs/research-extension-demo.md)
- [`examples/project-skills/questionnaire-mediation-moderation-sop/README.md`](examples/project-skills/questionnaire-mediation-moderation-sop/README.md)
- [`examples/external-plugins/research-regression/README.md`](examples/external-plugins/research-regression/README.md)

---

## 仓库结构

```text
.
├── rust/                         # 主开发面（Rust workspace）
├── docs/                         # 方法治理 / 扩展规范 / 用户指南 / demo
├── examples/external-plugins/    # external plugin 原型
├── tools/                        # 本地脚手架与辅助脚本
├── templates/                    # skill / doc 模板
├── src/                          # 早期 Python 面（历史 / 兼容参考）
├── tests/                        # Python 侧验证面
└── README.md
```
