# Research extension demo

这份文档给出一条**从材料到可复用扩展**的完整演示链路：

1. 用已有材料生成项目级 skill 草稿
2. 导出 OpenClaw / Claude 兼容格式
3. 安装一个 external plugin
4. 跑一个稳定计算方法原型
5. 明确 skill 和 plugin 的职责边界

它对应的目标不是“让系统随意自我改写”，而是让 agent 能在治理框架下做 **controlled self-extension**。

---

## 适用场景

当用户在交互中提供：

- 论文
- SOP
- codebook
- 内部分析说明
- 检索得到的方法资料

我们希望 agent 能先把这些材料沉淀成一个可复用的 skill；如果后续发现某个计算环节已经足够稳定，再把它收敛成 external plugin。

---

## 这条 demo 要说明什么

- **skill** 解决“怎么做”
- **plugin** 解决“怎么稳定地算”
- **canonical draft** 是内部真源
- **OpenClaw / Claude** 兼容导出只是适配层

---

## 前置准备

先构建 CLI：

```bash
cd rust
~/.cargo/bin/cargo build -p claw-cli
```

后面的命令默认仍在 `rust/` 目录下执行，除非命令里单独说明。

---

## Step 1：把材料转成一个项目级 skill 草稿

这里用仓库内已有文档做演示输入。实际使用时，你可以把 `--source` 替换成自己的论文、SOP、访谈整理、团队规范等材料路径。

```bash
./target/debug/claw project-skill init survey-cleaning-sop \
  --title "Survey Cleaning SOP" \
  --description "Draft workflow for local survey cleaning." \
  --domain survey \
  --use-when "Use before scoring." \
  --source ../docs/research-method-standards.md \
  --source ../docs/research-method-registry.md \
  --target openclaw \
  --target claude-command \
  --target claude-agent \
  --openclaw-root .. \
  --claude-root ..
```

这一步会同时生成：

### 内部 canonical draft

```text
./.claw/project-skills/survey-cleaning-sop/
├── SKILL.md
├── skill.json
└── README.md
```

### 兼容导出

```text
../skills/survey-cleaning-sop/SKILL.md
../.claude/commands/survey-cleaning-sop.md
../.claude/agents/survey-cleaning-sop.md
```

这表示：

- 我们自己的 canonical draft 仍然是源头
- OpenClaw / Claude 的文件只是导出适配，不反向成为真源

---

## Step 2：校验 skill 草稿结构

```bash
./target/debug/claw project-skill validate ./.claw/project-skills/survey-cleaning-sop
```

建议把这一步当成最小门槛：

1. 先验证结构完整
2. 再人工补充上下文、限制条件、失败检查
3. 最后再决定是否进入更广泛复用

---

## Step 3：安装一个 external plugin 原型

如果你发现“方法说明”已经够了，但某个计算环节还需要稳定可执行的 tool contract，那么下一步就是 external plugin。

这里用 regression 原型做演示：

```bash
./target/debug/claw plugins install ../examples/external-plugins/research-regression
./target/debug/claw plugins list
```

这个原型表示的是：

- 工作流知识不一定要进 Rust core
- 统计计算也不一定先做 bundled plugin
- 可以先在 external plugin 里把契约和边界打磨清楚

如果你要走 **R/lavaan 的 CFA / SEM** 路线，也可以参考：

- [`../examples/external-plugins/research-sem/README.md`](../examples/external-plugins/research-sem/README.md)
- [`./questionnaire-sem-rehearsal.md`](./questionnaire-sem-rehearsal.md)

如果你要走 **PROCESSv50 / mediation / moderation** 路线，也可以参考：

- [`../examples/external-plugins/research-processv50/README.md`](../examples/external-plugins/research-processv50/README.md)
- [`../examples/project-skills/questionnaire-processv50-sop/`](../examples/project-skills/questionnaire-processv50-sop/)

---

## Step 4：直接调用 regression 原型工具

下面这个例子从仓库根目录执行，用示例 CSV 跑一个最小 OLS 回归。

```bash
cd ..
printf '%s' '{
  "datasetPath": "examples/external-plugins/research-regression/fixtures/regression_demo.csv",
  "outcome": "performance",
  "predictors": ["study_hours", "sleep_quality"],
  "includeIntercept": true
}' | \
env CLAW_TOOL_NAME=regression_ols \
    CLAW_PLUGIN_ID=research-regression@example \
    CLAW_PLUGIN_ROOT=$(pwd)/examples/external-plugins/research-regression \
    CLAW_WORKSPACE_ROOT=$(pwd) \
    python3 examples/external-plugins/research-regression/tools/regression_tools.py
```

如果你希望把结果写成 artifact，可以额外补一个输出字段：

```json
{
  "outputPath": ".claw/artifacts/regression-summary.json"
}
```

---

## Step 5：如何理解这条链路

这条演示链路想表达的其实只有一句话：

> 先把方法知识沉淀成 skill，再把已经稳定的计算环节沉淀成 plugin。

更具体地说：

### 什么时候优先做 skill

当问题主要是这些内容时：

- 操作流程
- 数据检查 SOP
- 方法解释框架
- 用户材料整理
- 项目局部工作流

### 什么时候优先做 external plugin

当问题主要是这些内容时：

- 稳定输入/输出结构
- 可重复执行的计算逻辑
- 可以被 tool contract 表达的方法
- 后续可能跨项目复用的分析步骤

### 什么时候再考虑 bundled plugin

只有当方法已经：

- 契约稳定
- 复用频率高
- 足够核心
- 不适合继续停留在 external plugin 层

再考虑推进到 bundled core。

---

## 推荐使用顺序

如果你是一个实际使用者，我建议按这个顺序操作：

1. 先用当前集成的 survey 链路跑通一个项目
2. 缺流程时，先沉淀 skill
3. 缺稳定计算时，再补 external plugin
4. 真正高频、稳定、通用的方法，再讨论是否并入 bundled plugin

---

## 相关文档

- [`./project-skill-synthesis.md`](./project-skill-synthesis.md)
- [`./research-method-standards.md`](./research-method-standards.md)
- [`./research-method-registry.md`](./research-method-registry.md)
- [`./questionnaire-sem-rehearsal.md`](./questionnaire-sem-rehearsal.md)
- [`../examples/external-plugins/research-sem/README.md`](../examples/external-plugins/research-sem/README.md)
- [`../examples/external-plugins/research-processv50/README.md`](../examples/external-plugins/research-processv50/README.md)
- [`../examples/external-plugins/research-regression/README.md`](../examples/external-plugins/research-regression/README.md)
