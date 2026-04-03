# claw-code-paper

> **A research agent platform for turning analysis workflows into durable capabilities.**

一个面向科研数据分析的 agent 工作台：**稳定内核、可切换模型、可治理的方法能力、可演进的 skill / plugin 体系。**

它不是把“大模型 + 一堆脚本”临时拼起来，而是想把科研分析里最难长期维护的几件事放进同一套系统：

- 已集成方法可以直接跑
- Python / R 计算能力可以被 agent 稳定调用
- 论文、SOP、codebook、经验笔记可以沉淀成 skill
- 成熟方法可以升级成 external plugin / bundled plugin
- 整个系统能逐步形成 **受控自进化**，而不是失控堆逻辑

---

## Highlights

- **Research-first, not chat-first**
  - 从一开始就围绕问卷、量表、统计分析、方法沉淀来设计，而不是事后给通用 agent 打补丁
- **Stable Rust kernel**
  - runtime、配置、插件、会话、指令边界放在稳定内核里，避免方法逻辑四处散落
- **Python / R execution where it matters**
  - 用最合适的计算层做清洗、计分、psychometrics、CFA、作图、报告输出
- **Model-switchable by design**
  - DeepSeek、Kimi、Qwen、OpenAI-compatible 都能接进同一工作流，而不是被单一 provider 绑死
- **Skill / plugin growth loop**
  - 材料先沉淀成 skill，成熟方法再升级成 plugin，最终形成可复用能力资产
- **Built for Chinese research workflows**
  - 已开始补中文/CJK 图表输出、研究文档沉淀、方法治理这些真实使用中的关键细节

---

## Why this project is interesting

大多数 agent 项目擅长“会做事”，但不擅长“把研究方法长期沉淀成可维护能力”。

`claw-code-paper` 的长处在于，它不是只追求一次性回答，而是在做一套 **适合科研分析场景长期积累** 的 agent 基础设施：

- **Rust-first kernel**
  - 把 runtime、配置、插件、会话、指令注入这些底层能力做稳
- **Python / R as research engines**
  - 把真正的数据分析、统计检验、图表输出交给最合适的计算层
- **Skill-first knowledge capture**
  - 把流程知识、方法说明、研究规范、SOP 变成可复用 skill
- **Plugin-first executable methods**
  - 把稳定、重复、高价值的方法沉淀成工具契约与插件能力
- **Controlled self-extension**
  - 让系统“越用越会”，但通过规范、验证和分层治理来进化

换句话说，这个项目的目标不是“做一个能聊天的研究助手”，而是：

> **做一个能把科研分析能力逐步产品化、资产化、治理化的 agent 工作台。**

---

## The core idea

这个项目最核心的判断其实很简单：

- **研究知识** 不应该只留在 prompt 里
- **分析方法** 不应该只活在零散脚本里
- **用户材料** 不应该每次对话都重新解释一遍

更好的方式是把它们逐步沉淀为：

```text
materials -> skill -> plugin -> bundled capability
```

一旦这条链跑通，agent 不再只是“临时帮你完成任务”，而是在帮你积累一套越来越稳定的研究能力系统。

---

## What already works today

项目不是停留在架构想法上，已经有一条能跑起来的 survey 主链路：

```text
survey_metadata -> survey_score -> survey_psychometrics -> survey_report
```

它目前已经覆盖：

- 问卷 / 量表数据读取与结构理解
- 反向计分
- 均分 / 总分构造
- psychometrics 前置检查
- reliability / validity 基础分析
- CFA（验证性因子分析）
- Markdown 报告草稿生成

这意味着它已经具备一个很重要的特征：

> **它不是“以后也许可以做科研分析”，而是“现在已经可以把一类科研分析链路跑通”。**

---

## A better mental model

如果用一句更像产品介绍的话来描述它：

> **Claw Code Paper = research agent runtime + method execution layer + capability growth system**

其中：

- **runtime** 负责稳
- **execution layer** 负责把 Python / R 方法真正跑起来
- **growth system** 负责把一次次研究经验变成可复用 skill / plugin

这也是它和很多“会调工具的 agent”之间最大的差异。

---

## What makes it different

### 1. 它把“知识”和“计算”分开治理

很多系统会把方法说明、工具调用、统计逻辑、prompt 习惯全混在一起。

这里我们尽量分层：

| 层 | 负责什么 |
| --- | --- |
| **Rust core** | runtime、配置、插件、会话、稳定边界 |
| **Skill** | SOP、方法知识、解释框架、材料沉淀 |
| **External plugin** | 稳定计算方法、工具契约、执行入口 |
| **Bundled plugin** | 高复用、已收敛、适合核心内置的方法能力 |
| **Python / R** | 具体分析、统计、可视化、报告生成 |

这让系统不会因为某一种研究方法的变化就污染整个内核。

### 2. 它不是只“调用模型”，而是支持模型可替换

`claw init --research survey` 现在会直接脚手架这些 provider profile：

- `deepseek`
- `kimi`
- `qwen`
- `openai-compat`

并且可以在同一会话里直接切换：

```text
/provider kimi
/model kimi-k2.5

/provider qwen
/model qwen-plus
```

这对科研场景很重要，因为：

- 不同模型擅长不同任务
- 成本、速度、风格都可能不同
- 后续上线时不能被单一 provider 锁死

### 3. 它开始具备“自我进化”的正确形态

这里说的自我进化，不是让 agent 随机重写自己。

而是：

1. 用户在交互里提供资料、论文、SOP、codebook
2. agent 快速抽取并形成可复用 skill
3. 高复用、强契约的方法再升级成 plugin
4. 真正成熟后，才考虑进入 bundled core

这条路线更像 **研究能力资产化流水线**，而不是 prompt 级 improvisation。

### 4. 它在中文科研场景上开始补齐细节

现在 survey research bootstrap 会额外生成：

- `.claw/helpers/plotting.py`
- `.claw/helpers/plotting.R`

agent 生成的 Python / R 图表脚本，会被引导优先复用统一的 `configure_cjk_plotting()` helper，减少中文/CJK 标签乱码、负号异常、不同脚本各写一套字体配置的问题。

这类细节看起来小，但对真实科研交付非常重要。

---

## Current product shape

如果把它当成一个产品，而不是一堆仓库文件，可以把它理解成下面这套组合：

### Research analysis cockpit

- 一个 Rust-first CLI / agent runtime
- 一套可扩展的 research workflow
- 一组可治理的 method assets

### Method execution surface

- Python：清洗、计分、可视化、报告拼装
- R：psychometrics、CFA、统计检验

### Capability growth loop

- 材料 → skill
- skill → external plugin
- external plugin → bundled plugin

这也是这个项目最值得展示的点：

> **它天然适合把“零散经验”变成“稳定能力”。**

---

## Quick start

### 1. Build the CLI

```bash
cd rust
~/.cargo/bin/cargo build -p claw-cli
```

### 2. Bootstrap a survey research workspace

```bash
./target/debug/claw init --research survey
```

这会生成：

- `.claw.json`
- `.claw/settings.local.json`
- `.claw/artifacts/`
- `.claw/helpers/plotting.py`
- `.claw/helpers/plotting.R`
- `CLAW.md`

### 3. Configure your provider

在 `.claw/settings.local.json` 里配置 API key 对应的环境变量，然后启动：

```bash
./target/debug/claw
```

进入 REPL 后可以直接切换模型与 provider：

```text
/provider deepseek
/model deepseek-chat

/provider kimi
/model kimi-k2.5

/provider qwen
/model qwen-plus
```

### 4. Run the survey workflow

核心链路：

```text
survey_metadata -> survey_score -> survey_psychometrics -> survey_report
```

详细文档：

- [`rust/README.md`](rust/README.md)
- [`docs/survey-minimal-walkthrough.md`](docs/survey-minimal-walkthrough.md)
- [`rust/crates/plugins/bundled/research-survey/README.md`](rust/crates/plugins/bundled/research-survey/README.md)

---

## From materials to reusable skills

这个项目另一个强项，是把“研究材料”转成“可执行工作流记忆”。

已经有正式 CLI 支持 project skill 生命周期：

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

兼容目标也已经明确：

- `.claw/project-skills/<slug>/SKILL.md`
- `.claw/project-skills/<slug>/skill.json`
- `skills/<slug>/SKILL.md`
- `.claude/commands/<slug>.md`
- `.claude/agents/<slug>.md`

所以它不是封闭生态，而是尽量往现有 skill 生态兼容。

---

## Extension philosophy

这个项目的扩展，不是“想到什么就往 core 里塞什么”，而是有明确分层：

### 用 skill 的场景

- 方法说明
- 研究规范
- 解释框架
- 操作流程
- 用户提供材料的沉淀

### 用 external plugin 的场景

- 稳定计算方法
- 明确输入输出契约
- 可重复调用的分析步骤

### 进入 bundled plugin 的场景

- 跨项目高复用
- 契约稳定
- 已经证明值得进核心分发面

这套分层，决定了它更像一个 **research capability platform**，而不是一个一次性脚本仓库。

---

## Documentation

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

## Repository layout

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

---

## In one sentence

**`claw-code-paper` 想做的，不只是“帮你跑一次分析”，而是把科研分析方法逐步沉淀成一个可复用、可扩展、可治理、可演进的 agent 系统。**
