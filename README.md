# claw-code-paper

## 这个项目是干什么的

`claw-code-paper` 是一个面向 **科研数据分析** 的 Rust-first agent CLI 工作台。

它想解决的不是“再做一个通用代码助手”，而是把下面几件事放进同一套工作流里：

- 跑通已经集成的科研分析方法
- 让 agent 按规范调用 Python / R 分析能力
- 当现有方法不够时，快速从论文、SOP、codebook、内部笔记中生成 skill
- 当方法已经稳定时，用 external plugin 把计算能力接进来

当前最适合的场景包括：

- 问卷 / 量表数据处理
- 反向计分、均分 / 总分生成
- psychometrics 前置检查
- CFA（验证性因子分析）
- 分析结果整理与 Markdown 报告草稿
- 缺失方法的治理化扩展

---

## 这个项目有哪些特点

### 1. Rust 内核稳定，方法能力尽量外置

项目主张是：**内核稳定，方法扩展灵活**。

- Rust core 负责 CLI、runtime、session、plugin 管理
- Python / R 负责更贴近科研现场的数据处理和统计分析
- skill 负责 workflow knowledge / SOP / 用户可教给系统的方法流程
- plugin 负责稳定、可执行、可复用的计算能力

### 2. 已经有一条可落地的 survey 主链路

当前最核心的 bundled plugin 是 `research-survey`，已经覆盖：

```text
survey_metadata -> survey_score -> survey_psychometrics -> survey_report
```

也就是说，项目不是只有架子，已经有一条面向问卷研究的主链路。

### 3. 缺方法时，不是硬改内核，而是按规范扩展

这里把扩展分成三层：

- **skill**：适合流程知识、SOP、解释框架、用户材料沉淀
- **external plugin**：适合稳定计算方法、可执行工具契约
- **bundled plugin**：适合跨项目高复用、已经收敛的核心能力

这样做的好处是：既能扩展，又不会把 runtime 越改越乱。

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

这条路适合把“用户提供的资料 / 检索得到的材料 / 团队内部 SOP”沉淀成项目级 skill 草稿。

### 5. skill 兼容导出已经考虑到 OpenClaw / Claude 生态

内部 canonical draft 仍然是：

- `.claw/project-skills/<slug>/SKILL.md`
- `.claw/project-skills/<slug>/skill.json`

但现在也支持兼容导出到：

- **OpenClaw skill**：`skills/<slug>/SKILL.md`
- **Claude command**：`.claude/commands/<slug>.md`
- **Claude agent**：`.claude/agents/<slug>.md`

### 6. “自我进化”走的是受控自扩展，不是随意自我改写

项目当前强调的是 **controlled self-extension**：

- 先把资料转成 skill
- 再把稳定方法做成 plugin
- 最后才考虑是否进入 bundled core

相关规范见：

- [`docs/research-method-standards.md`](docs/research-method-standards.md)
- [`docs/research-method-registry.md`](docs/research-method-registry.md)

---

## 怎么快速使用

### 1. 先跑通已经集成的科研分析链路

```bash
cd rust
~/.cargo/bin/cargo build -p claw-cli
./target/debug/claw init --research survey
```

当前最短理解路径：

1. 看 [`rust/README.md`](rust/README.md)
2. 看 [`rust/crates/plugins/bundled/research-survey/README.md`](rust/crates/plugins/bundled/research-survey/README.md)
3. 按顺序使用 `survey_metadata -> survey_score -> survey_psychometrics -> survey_report`

### 2. 如果你手上有论文 / SOP / codebook / 内部说明，先生成一个项目 skill

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
```

说明文档：

- [`docs/project-skill-synthesis.md`](docs/project-skill-synthesis.md)

### 3. 如果你缺的是稳定计算方法，走 external plugin

```bash
cd rust
./target/debug/claw plugins install ../examples/external-plugins/research-regression
./target/debug/claw plugins list
```

示例插件：

- [`examples/external-plugins/research-regression/README.md`](examples/external-plugins/research-regression/README.md)

---

## 如果没有集成的方法，应该怎么判断走哪条路

优先按这个规则：

1. **主要是流程知识 / 操作经验 / 方法解释** → 先做 skill
2. **主要是稳定计算 / 可执行分析步骤** → 先做 external plugin
3. **已经跨项目高复用，而且契约稳定** → 再考虑 bundled plugin

如果你想看一条完整演示链路：

- [`docs/research-extension-demo.md`](docs/research-extension-demo.md)

这个 demo 会把这几步串起来：

1. 用材料生成 skill
2. 导出兼容格式
3. 安装 external plugin
4. 跑一个回归分析原型
5. 理解 skill 和 plugin 的分工边界

---

## 文档导航

### 面向使用者

- [`rust/README.md`](rust/README.md)
- [`rust/crates/plugins/bundled/research-survey/README.md`](rust/crates/plugins/bundled/research-survey/README.md)

### 面向方法治理

- [`docs/research-method-registry.md`](docs/research-method-registry.md)
- [`docs/research-method-standards.md`](docs/research-method-standards.md)

### 面向 skill / plugin 扩展

- [`docs/project-skill-synthesis.md`](docs/project-skill-synthesis.md)
- [`docs/research-extension-demo.md`](docs/research-extension-demo.md)
- [`examples/external-plugins/research-regression/README.md`](examples/external-plugins/research-regression/README.md)

---

## 仓库结构

```text
.
├── rust/                         # 主开发面（Rust workspace）
├── docs/                         # 方法治理 / 扩展规范 / demo
├── examples/external-plugins/    # external plugin 原型
├── tools/                        # 本地脚手架与辅助脚本
├── templates/                    # skill / doc 模板
├── src/                          # 早期 Python 面（历史 / 兼容参考）
├── tests/                        # Python 侧验证面
└── README.md
```
