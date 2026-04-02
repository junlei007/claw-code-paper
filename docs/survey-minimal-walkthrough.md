# Survey minimal walkthrough

这份文档给你一条**最小可运行**的 survey 演示链路。

目标不是一上来就接模型，而是先验证这套科研分析能力在本地能不能跑通。

它会串起这 4 步：

```text
survey_metadata -> survey_score -> survey_psychometrics -> survey_report
```

---

## 这份 walkthrough 适合什么时候看

适合你在下面这些时候使用：

- 第一次拉下这个仓库，想确认 survey 主链路能不能运行
- 暂时不想配置 provider / API key，只想验证本地分析能力
- 想知道最小 fixture 跑出来大概是什么效果

这份示例默认在**仓库根目录**执行命令。

---

## 前置要求

你至少需要：

- Rust toolchain
- Python 3
- R
- R 侧依赖：`jsonlite`、`psych`、`lavaan`（如果缺依赖，psychometrics 步骤会失败）

先构建 CLI：

```bash
cd rust
~/.cargo/bin/cargo build -p claw-cli
cd ..
```

> 这份 walkthrough 里的 4 个分析步骤都直接调用本地 plugin 后端，**不依赖模型 provider，也不需要 API key**。

---

## Step 1：读取 fixture，看清楚数据结构

```bash
printf '%s' '{
  "datasetPath": "rust/crates/plugins/bundled/research-survey/fixtures/mini_survey.csv"
}' | \
env CLAW_TOOL_NAME=survey_metadata \
    CLAW_PLUGIN_ID=research-survey@bundled \
    CLAW_PLUGIN_ROOT=$(pwd)/rust/crates/plugins/bundled/research-survey \
    CLAW_WORKSPACE_ROOT=$(pwd) \
    python3 rust/crates/plugins/bundled/research-survey/tools/survey_tools.py
```

你应该看到的重点包括：

- 数据集有 5 行、6 列
- 题项列是 `q1` 到 `q4`
- `q3` 有 1 个缺失值
- 当前 fixture 里还没有声明量表定义，所以 `scaleCount = 0`

这一步的目的，是先确认数据、列名、缺失情况都符合预期。

---

## Step 2：做最小计分，写出 scored dataset

```bash
mkdir -p .claw/artifacts

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
  "idColumns": ["id"],
  "outputPath": ".claw/artifacts/mini_scored.csv"
}' | \
env CLAW_TOOL_NAME=survey_score \
    CLAW_PLUGIN_ID=research-survey@bundled \
    CLAW_PLUGIN_ROOT=$(pwd)/rust/crates/plugins/bundled/research-survey \
    CLAW_WORKSPACE_ROOT=$(pwd) \
    python3 rust/crates/plugins/bundled/research-survey/tools/survey_tools.py
```

你应该关注：

- `engagement_mean` 被成功计算出来
- 输出里会返回 preview rows
- artifact 会写到：`.claw/artifacts/mini_scored.csv`

在当前 fixture 上，验证时得到的 preview 大致是：

- id 1 → 4.5
- id 2 → 3.75
- id 3 → 2.6667
- id 4 → 5.0
- id 5 → 2.25

---

## Step 3：跑 psychometrics

```bash
printf '%s' '{
  "datasetPath": "rust/crates/plugins/bundled/research-survey/fixtures/mini_survey.csv",
  "analysisId": "mini",
  "scaleDefinitions": [{
    "name": "engagement",
    "items": ["q1", "q2", "q3", "q4"],
    "reverseItems": ["q3"]
  }],
  "reverseItems": ["q3"],
  "responseScale": {"min": 1, "max": 5},
  "cfaModel": "engagement =~ q1 + q2 + q3 + q4"
}' | \
env CLAW_TOOL_NAME=survey_psychometrics \
    CLAW_PLUGIN_ID=research-survey@bundled \
    CLAW_PLUGIN_ROOT=$(pwd)/rust/crates/plugins/bundled/research-survey \
    CLAW_WORKSPACE_ROOT=$(pwd) \
    Rscript rust/crates/plugins/bundled/research-survey/tools/survey_psychometrics.R
```

当前这个 toy fixture 很小，所以你看到的结果会更像“链路验证”，而不是“正式统计结论”。

验证时的关键现象是：

- `alpha` 能算出来，约为 `0.9662`
- `Bartlett` 会返回结果
- `KMO` 会被跳过
- `CFA` 会被跳过
- `warnings` 里会提示这个 fixture 存在 perfect correlations

这其实是正常的，因为这个最小数据集的目标是跑通链路，不是提供真实研究质量的数据结构。

---

## Step 4：生成最小 Markdown 报告草稿

```bash
printf '%s' '{
  "title": "Mini Survey Walkthrough Report",
  "datasetPath": "rust/crates/plugins/bundled/research-survey/fixtures/mini_survey.csv",
  "datasetSummary": {
    "dataset": {"path": "rust/crates/plugins/bundled/research-survey/fixtures/mini_survey.csv", "format": "csv", "rows": 5},
    "questionnaire": {"scaleCount": 1, "reverseItemCount": 1}
  },
  "results": {
    "alpha": 0.9662,
    "omega": "skipped",
    "kmo": "skipped",
    "bartlett": "chiSquare=32.87, df=6, p=0.0000111",
    "modelSpec": "engagement =~ q1 + q2 + q3 + q4",
    "cfi": "not-run",
    "tli": "not-run",
    "rmsea": "not-run",
    "srmr": "not-run"
  },
  "notes": [
    "Alpha was computed on 4 complete cases in the fixture dataset.",
    "CFA and KMO were skipped because the toy dataset contains perfect correlations."
  ],
  "outputPath": ".claw/artifacts/mini_report.md"
}' | \
env CLAW_TOOL_NAME=survey_report \
    CLAW_PLUGIN_ID=research-survey@bundled \
    CLAW_PLUGIN_ROOT=$(pwd)/rust/crates/plugins/bundled/research-survey \
    CLAW_WORKSPACE_ROOT=$(pwd) \
    python3 rust/crates/plugins/bundled/research-survey/tools/survey_tools.py
```

成功后会生成：

- `.claw/artifacts/mini_report.md`

这一步的作用不是替代研究者写最终报告，而是把前面结构化结果快速串成一个 Markdown 草稿。

---

## 跑完后你应该得到什么

至少会有这两个本地产物：

```text
.claw/artifacts/mini_scored.csv
.claw/artifacts/mini_report.md
```

如果这两步都能成功，基本说明：

- Python 侧数据读取和 scoring 正常
- R 侧 psychometrics 通道可用
- Markdown 报告生成可用
- survey 主链路已经能在本地最小运行

---

## 下一步怎么做

如果这条最小 walkthrough 能跑通，下一步建议是：

1. 用你自己的真实问卷数据替换 `mini_survey.csv`
2. 把量表定义补完整
3. 再决定哪些流程应该沉淀成 skill
4. 哪些稳定计算步骤应该沉淀成 external plugin

如果你想看更完整的使用者路径：

- [`./research-user-playbook.md`](./research-user-playbook.md)
- [`../rust/crates/plugins/bundled/research-survey/README.md`](../rust/crates/plugins/bundled/research-survey/README.md)
- [`./research-extension-demo.md`](./research-extension-demo.md)
