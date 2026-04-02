# Self-extension evaluation checklist

这份清单用于给“自动进化 / controlled self-extension”加 gate。

目标不是让 agent 生成更多 skill，而是让它只沉淀**值得保留、可被其他 agent 正确使用、边界清楚**的 skill。

---

## 什么时候应该用这份清单

在下面这些动作之前，应该至少过一遍这份清单：

- 新生成一个 project skill draft
- 准备把 skill 从 `draft` 提升到 `project`
- 准备把 skill 对外导出到兼容格式
- 准备把 skill 的一部分能力升级成 external plugin

---

## Gate 1：这个能力真的应该是一个 skill 吗

先回答：

1. 它主要是在表达**流程知识 / SOP / 分析步骤**吗？
2. 它不是只包了一层很薄的低级调用吗？
3. 它没有和现有 skill 高度重复吗？
4. 它的边界能不能一句话说清楚？

如果这些问题答不清楚，就不要急着生成新 skill。

---

## Gate 2：名字和边界够不够清楚

检查：

- slug 是否同时体现了**领域 + 动作**
- title 是否让另一个 agent 一眼就知道什么时候该调用它
- `use_when` 是否明确
- skill 是否清楚声明了自己**不是**什么

好的命名例子：

- `survey-cleaning-sop`
- `regression-diagnostics-checklist`
- `interview-coding-consistency-review`

---

## Gate 3：agent contract 是否完整

一个可复用 skill 至少要让另一个 agent 能快速恢复下面这些信息：

- 它解决什么问题
- 它什么时候该被使用
- 它依赖什么输入
- 它会产出什么输出 / artifact
- 它有哪些 warning / limits
- 遇到什么情况应该停止或回退

当前建议至少补齐这些字段：

- `source_materials`
- `input_expectations`
- `workflow`
- `outputs`
- `limits`
- `failure_checks`
- `evaluation_examples`
- `verification_status`
- `held_out_validation_status`
- `maturity_level`

---

## Gate 4：有没有真实例子，不只是纸面上看起来对

至少要问：

1. 有没有一个**真实项目例子**证明它可用？
2. 有没有一个**held-out 例子**证明它不是只贴合生成它的源材料？
3. 失败时，它会不会给出明确 warning，而不是模糊话术？

推荐最低标准：

- `draft`：至少有结构化草稿
- `project`：真实例子 + held-out 例子都过
- `published`：已经在多个项目或可公开场景里稳定复用

---

## Gate 5：是否应该升级成 plugin

如果它已经明显变成下面这类能力，就要重新判断是不是还该停留在 skill：

- 输入输出稳定
- 重复计算很多
- 可以表达成清晰 tool contract
- 跨项目复用频率高

满足这些条件时，应该评估 external plugin，而不是继续把计算逻辑堆进 skill。

---

## Promotion 建议

### `draft` -> `project`

至少确认：

- 结构完整
- 边界清楚
- failure checks 已写明
- 真实例子已过
- held-out validation 已过

命令层面可以这样做：

```bash
cd rust
./target/debug/claw project-skill promote ./.claw/project-skills/<slug> \
  --to project \
  --held-out-validation passed
```

### `project` -> `published`

至少确认：

- 已有稳定复用记录
- warning / limits 足够明确
- 兼容导出不会丢失关键边界信息
- 没有更适合升级成 external plugin 的计算部分被硬塞在 skill 内

---

## 最后一句判断标准

如果一个 skill 不能让“另一个没参与起草的 agent”可靠使用，那它还不够成熟。
