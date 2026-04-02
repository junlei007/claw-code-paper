# Research user playbook

这份文档站在**实际使用者**视角，回答三个最常见的问题：

1. 已经集成的分析方法怎么用？
2. 没有集成的分析方法怎么接进来？
3. skill 和 plugin 到底分别用来做什么？

---

## 先建立一个整体判断

你可以把当前项目理解成四层：

- **Rust core**：CLI、runtime、plugin 管理
- **bundled plugin**：已经集成进项目内核附近的方法能力
- **external plugin**：还没进核心，但已经有稳定计算契约的方法能力
- **skill**：把论文、SOP、codebook、经验流程沉淀成可复用工作流

所以一个实际项目通常会这样推进：

1. 先用现成能力跑通最小链路
2. 缺流程时，先补 skill
3. 缺稳定计算时，再补 external plugin
4. 真正高频、稳定、核心的方法，再考虑并入 bundled plugin

---

## 场景 1：已经集成的分析方法怎么用

当前最清晰的一条主链路是 survey：

```text
survey_metadata -> survey_score -> survey_psychometrics -> survey_report
```

### 最短启动方式

```bash
cd rust
~/.cargo/bin/cargo build -p claw-cli
./target/debug/claw init --research survey
```

这一步的作用是先把研究项目所需的基础配置和目录结构准备出来。

接下来重点看：

- [`./survey-minimal-walkthrough.md`](./survey-minimal-walkthrough.md)
- [`../rust/README.md`](../rust/README.md)
- [`../rust/crates/plugins/bundled/research-survey/README.md`](../rust/crates/plugins/bundled/research-survey/README.md)

### 推荐理解顺序

1. 先看有哪些输入文件和元数据要求
2. 再看 `survey_score` 的计分规则
3. 再看 psychometrics / CFA 这一层输出什么
4. 最后看报告和 artifact 怎么落地

### 适合什么用户

这一条链路目前最适合：

- 社科问卷研究
- 教育测量
- 心理量表处理
- 用户研究中的结构化问卷分析

---

## 场景 2：如果没有集成的方法，应该怎么做

不要先想着改 runtime，先判断你缺的到底是哪一类能力。

### A. 缺的是流程知识、SOP、方法说明

这类情况优先做 **skill**。

典型例子：

- 数据清洗 SOP
- 编码规范
- 访谈整理流程
- 某种分析前的人工检查清单
- 某篇论文方法在你项目里的落地流程

#### 为什么先做 skill

因为这类问题的核心价值通常不在“算”，而在“怎么做、按什么顺序做、用什么标准判断是否合格”。

#### 最小命令

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

进一步看：

- [`./project-skill-synthesis.md`](./project-skill-synthesis.md)
- [`./questionnaire-mediation-moderation-rehearsal.md`](./questionnaire-mediation-moderation-rehearsal.md)
- [`./research-extension-demo.md`](./research-extension-demo.md)

### B. 缺的是稳定计算方法

这类情况优先做 **external plugin**。

典型例子：

- regression
- 特定统计模型
- 一套稳定的数据变换工具
- 某种固定输入输出的自动评分/分析步骤

#### 为什么先做 external plugin

因为这类问题更适合被定义成一个稳定的 tool contract，而不是写进 skill 里反复靠自然语言执行。

#### 最小命令

```bash
cd rust
./target/debug/claw plugins install ../examples/external-plugins/research-regression
./target/debug/claw plugins list
```

进一步看：

- [`../examples/external-plugins/research-regression/README.md`](../examples/external-plugins/research-regression/README.md)
- [`./research-extension-demo.md`](./research-extension-demo.md)

---

## 场景 3：skill 和 plugin 怎么配合

一个好用的科研 agent，往往不是只靠 skill，也不是只靠 plugin，而是两者配合。

### skill 更像什么

skill 更像：

- 操作手册
- 方法 SOP
- 用户材料沉淀
- 解释框架
- 分析前后的检查流程

### plugin 更像什么

plugin 更像：

- 稳定工具
- 明确输入输出
- 固定计算逻辑
- 可以被反复复用的分析能力

### 推荐组合方式

比较自然的一条路径是：

1. 用户提供论文、SOP、经验材料
2. 先生成一个 skill 草稿
3. 运行一段时间后，发现某个计算步骤高度稳定
4. 再把这部分能力沉淀成 external plugin
5. 如果后续证明它跨项目高频复用，再考虑进入 bundled plugin

---

## 一个实际使用顺序建议

如果你现在就是使用者，可以照这个顺序走：

### 第一步：先跑通现有能力

目标不是一开始就“全自动”，而是先验证：

- 当前项目能否跑通你的基础分析流程
- 它的 artifact、报告、结构是否符合你的习惯

### 第二步：把你自己的方法知识沉淀成 skill

当你发现“项目缺的不是一个函数，而是一套流程经验”时，就开始建 skill。

### 第三步：把稳定计算能力沉淀成 plugin

当你发现某一步已经反复出现，而且输入输出清晰，就把它做成 external plugin。

### 第四步：再考虑是否升级为 bundled plugin

只有真正稳定、高频、通用的方法，才值得进入项目核心。

---

## 当前你最应该看的文档

如果你是第一次上手，建议按这个顺序读：

1. [`../README.md`](../README.md)
2. [`../rust/README.md`](../rust/README.md)
3. [`../rust/crates/plugins/bundled/research-survey/README.md`](../rust/crates/plugins/bundled/research-survey/README.md)
4. [`./research-method-registry.md`](./research-method-registry.md)
5. [`./project-skill-synthesis.md`](./project-skill-synthesis.md)
6. [`./research-extension-demo.md`](./research-extension-demo.md)

---

## 前置提醒

在实际运行 agent 或 provider 之前，你通常还需要准备：

- Rust toolchain
- Python 3
- R
- 对应 provider 的 API key

如果 provider 没配置好，CLI 能编译成功，但实际调用模型时仍然会报认证错误。
