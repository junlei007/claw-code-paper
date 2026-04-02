# claw-code-paper

一个面向**科研数据分析**的 Rust-first agent CLI 工作台。

它的重点不是再造一个通用代码助手，而是把 agent runtime、research workflow、plugin/tool、Python/R 分析桥接组合成一个更适合下面这些任务的工作流：

- 问卷 / 量表数据处理
- 反向计分、均分 / 总分生成
- 信度 / 效度前置检查
- CFA（验证性因子分析）
- 分析结果整理与 Markdown 报告草稿
- 缺失方法的 skill / plugin 扩展

---

## 你先看哪条路径

### 1. 我只是想先跑通已经集成的分析链路

按这个顺序看：

1. 当前 README
2. [`rust/README.md`](rust/README.md)
3. [`rust/crates/plugins/bundled/research-survey/README.md`](rust/crates/plugins/bundled/research-survey/README.md)

最短命令：

```bash
cd rust
~/.cargo/bin/cargo build -p claw-cli
./target/debug/claw init --research survey
```

标准链路：

```text
survey_metadata -> survey_score -> survey_psychometrics -> survey_report
```

---

### 2. 我想知道现在已经有哪些科研方法

看这里：

- [`docs/research-method-registry.md`](docs/research-method-registry.md)

它回答的是：

- 已集成的方法是什么
- 每个方法怎么用
- 预期会产生什么 artifact
- 当前还没集成的方法有哪些

---

### 3. 我缺一个方法，应该怎么扩展

看这里：

- [`docs/research-method-standards.md`](docs/research-method-standards.md)

核心规则：

- **workflow knowledge** → 先做 skill
- **stable computation** → 先做 external plugin
- **高复用核心能力** → 再考虑 bundled plugin

---

### 4. 我想从资料 / SOP / 用户材料快速生成一个 skill

看这里：

- [`docs/project-skill-synthesis.md`](docs/project-skill-synthesis.md)

现在已经有正式 CLI 入口：

```bash
cd rust
./target/debug/claw project-skill init survey-cleaning-sop \
  --title "Survey Cleaning SOP" \
  --description "Draft workflow for local survey cleaning." \
  --domain survey \
  --use-when "Use before scoring." \
  --source ../docs/research-method-standards.md
```

校验：

```bash
./target/debug/claw project-skill validate ../.claw/project-skills/survey-cleaning-sop
```

---

### 5. 我想走 external plugin 路线

看这里：

- [`examples/external-plugins/research-regression/README.md`](examples/external-plugins/research-regression/README.md)

现在已经有正式 CLI 入口：

```bash
cd rust
./target/debug/claw plugins install ../examples/external-plugins/research-regression
./target/debug/claw plugins list
```

---

## 当前项目的设计立场

一句话：

> 用 Rust 保持内核稳定，把科研差异化能力尽量放在 plugin、skill、文档规范、Python/R 分析层。

这也是为什么目前主线不是深改 runtime，而是优先建设：

- research bootstrap
- survey bundled plugin
- method registry / standards
- project-skill synthesis
- external plugin path

---

## 当前最重要的能力面

### 已集成主链路：survey

当前最核心的 bundled plugin 是：

- [`rust/crates/plugins/bundled/research-survey/`](rust/crates/plugins/bundled/research-survey/)

它提供：

- `survey_metadata`
- `survey_score`
- `survey_psychometrics`
- `survey_report`

适合：

- 社科 / 教育 / 心理 / 用户研究问卷数据
- 量表计分
- psychometrics 前置分析
- 报告草稿生成

---

### 已落地的扩展示例

#### A. 项目级 skill 草稿

- canonical draft：`.claw/project-skills/<slug>/`

#### B. compatibility exports

自动生成的 skill 现在按“三层结构”组织：

1. **canonical**：我们自己的 `SKILL.md + skill.json`
2. **OpenClaw-compatible**：`skills/<skill>/SKILL.md` 形态
3. **Claude-compatible**：
   - workflow 型 → `.claude/commands/*.md`
   - specialist / persona 型 → `.claude/agents/*.md`

当前脚手架已经支持导出这些兼容目标。

#### C. external plugin prototype

- regression 原型：[`examples/external-plugins/research-regression/`](examples/external-plugins/research-regression/)

---

## 技术架构

### Rust core

- `rust/crates/claw-cli`：CLI 入口
- `rust/crates/runtime`：配置、session、prompt、权限、provider/runtime glue
- `rust/crates/plugins`：插件发现、启停、tool 聚合
- `rust/crates/commands`：slash/plugin 管理命令
- `rust/crates/tools`：tool 执行桥接

### Research substrate

- **Python**：数据读取、清洗、scoring、artifact 导出
- **R**：psychometrics、CFA、更贴近科研统计工作流的分析

### Model layer

不是只绑定 Claude。

当前支持：

- OpenAI-compatible provider profiles
- DeepSeek 等可替换模型后端

---

## 文档导航

### 面向用户

- [`rust/README.md`](rust/README.md)
- [`rust/crates/plugins/bundled/research-survey/README.md`](rust/crates/plugins/bundled/research-survey/README.md)

### 面向方法治理

- [`docs/research-method-registry.md`](docs/research-method-registry.md)
- [`docs/research-method-standards.md`](docs/research-method-standards.md)
- [`docs/project-skill-synthesis.md`](docs/project-skill-synthesis.md)

### 面向扩展者

- [`examples/external-plugins/research-regression/README.md`](examples/external-plugins/research-regression/README.md)

---

## 仓库结构

```text
.
├── rust/                         # 主开发面（Rust workspace）
├── docs/                         # 方法治理 / skill synthesis 文档
├── examples/external-plugins/    # external plugin 原型
├── tools/                        # 本地脚手架与辅助脚本
├── templates/                    # skill / doc 模板
├── src/                          # 早期 Python 面（历史 / 兼容参考）
├── tests/                        # Python 侧验证面
└── README.md
```

---

## 当前状态

现在已经具备：

- survey bootstrap
- survey bundled plugin
- project-skill CLI scaffold
- direct plugin CLI surface
- regression external plugin prototype
- method governance docs

如果你现在要继续推进，最自然的路径就是：

1. 先用 integrated methods 跑通项目
2. 缺方法时判断 skill 还是 plugin
3. 用 project-skill / external plugin 路线扩展
