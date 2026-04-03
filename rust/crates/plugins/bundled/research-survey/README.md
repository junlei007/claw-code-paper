# research-survey bundled plugin

面向**科研问卷/量表数据分析**的内置插件。

它不是一个通用 BI 工具，而是为以下典型场景准备的研究分析能力层：
- 问卷数据清洗与结构检查
- 量表条目核对、反向计分、总分/均分生成
- 信度、效度预检查
- 验证性因子分析（CFA）
- 分析结果整理与报告草稿生成

这个插件的设计目标，是把产品差异化能力尽量放在**插件 / skills / 文档模板 / Python + R 分析桥接**上，而不是深度耦合 Rust 核心运行时。这样后续即使上游内核继续演进，也更容易持续同步。

---

## 0. 最短使用路径

如果你是从一个全新研究项目开始，最短路径不是先手写配置，而是：

```bash
claw init --research survey
```

这会先帮你生成：

- `.claw/settings.local.json`
- `.claw/artifacts/`
- survey-aware 的 `CLAW.md`

然后再按这条链路工作：

```text
survey_metadata -> survey_score -> survey_psychometrics -> survey_report
```

如果你要看：
- 当前有哪些科研方法已经正式集成：
  [`../../../../../docs/research-method-registry.md`](../../../../../docs/research-method-registry.md)
- 某个新分析方法应该先做成 skill、外部插件还是内置插件：
  [`../../../../../docs/research-method-standards.md`](../../../../../docs/research-method-standards.md)

如果你是从插件开发/调试视角阅读本文，继续往下看各 tool 的输入输出和本地运行示例即可。

如果你想先跑一条最小可运行案例，再回来看各 tool 细节，先看：
- [`../../../../../docs/survey-minimal-walkthrough.md`](../../../../../docs/survey-minimal-walkthrough.md)

---

## 1. 适合什么任务

当前版本优先覆盖的是**问卷研究 / 社科 / 教育 / 心理 / 用户研究**中最常见的数据分析链路：

1. 读取数据集（CSV / Excel / SPSS `.sav` / `.dat`）
2. 检查字段、缺失、条目覆盖情况
3. 根据量表定义进行反向计分与量表得分计算
4. 进行信度分析、KMO / Bartlett 等效度前置检查
5. 按需运行 CFA
6. 生成结构化结果，再输出 Markdown 报告草稿

如果你希望把 claw-code 改造成“科研数据分析智能体”，这个插件就是目前最靠近该方向的一层领域能力封装。

---

## 2. 当前能力概览

### 工具列表

| Tool | 后端 | 作用 |
| --- | --- | --- |
| `survey_metadata` | Python | 读取数据并输出结构化元数据摘要 |
| `survey_score` | Python | 反向计分、量表均分/总分计算、可导出打分后数据集 |
| `survey_psychometrics` | R | 信度分析、效度前置检查、可选 CFA |
| `survey_report` | Python | 将结构化结果渲染为 Markdown 报告 |

### 附带资源

- `templates/survey-report.md`：报告模板
- `contracts/r-psychometrics.md`：R 后端输出约定
- `fixtures/mini_survey.csv`：本地最小测试数据

---

## 3. 为什么采用双后端

这个插件刻意采用 **Python + R 双后端**：

- **Python** 更适合：
  - 数据读取
  - 字段检查
  - 缺失率汇总
  - 反向计分
  - 生成中间产物
- **R** 更适合：
  - psych 指标
  - KMO / Bartlett
  - lavaan / CFA
  - 更贴近科研统计工作流

因此目前的推荐使用方式是：

- 用 `survey_metadata` 看清楚数据结构
- 用 `survey_score` 先把量表分数算出来
- 用 `survey_psychometrics` 做统计检验
- 用 `survey_report` 生成报告草稿

---

## 4. 支持的数据格式

优先支持：
- `csv`
- `xlsx`
- `sav`
- `dat`

说明：
- `survey_metadata` / `survey_score` / `survey_psychometrics` 都支持自动识别格式
- Excel 可通过 `sheetName` 指定工作表
- SPSS `.sav` 会尽量保留变量标签信息
- `.dat` 可作为分隔文本读取；`survey_psychometrics` 还支持固定宽度场景（`layout = "fixed-width"` + `widths` / `columnWidths`）
- Python 侧对复杂固定宽度 `.dat` 的支持目前不如 R 侧稳定，复杂场景建议优先走 psychometrics/R 通道

---

## 5. 推荐工作流

### 标准流程

1. 运行 `claw init --research survey`
2. 配置模型提供方（不限 Claude，可用 OpenAI-compatible / DeepSeek）
3. 检查 `.claw/settings.local.json` 中：
   - `research.profile = "survey"`
   - `research-survey@bundled = true`
   - `artifactDir = ".claw/artifacts"`
4. 先运行 `survey_metadata`
5. 再运行 `survey_score`
6. 然后运行 `survey_psychometrics`
7. 最后运行 `survey_report`

如果你发现需要的方法不在当前四个 tool 里，不要直接把所有新方法塞进内置插件；先看：

- [`../../../../../docs/research-method-registry.md`](../../../../../docs/research-method-registry.md)
- [`../../../../../docs/research-method-standards.md`](../../../../../docs/research-method-standards.md)

### 为什么 `survey_score` 要放在前面

很多真实问卷数据分析里，真正进入统计分析之前，往往要先完成：
- 反向题处理
- 量表条目检查
- 均分/总分构造
- 缺失项阈值控制

所以 `survey_score` 不是附属能力，而是科研分析链路里的核心中间步骤。

---

## 6. 示例配置：使用 DeepSeek / Kimi / Kimi Code / Qwen / OpenAI-compatible 后端

```json
{
  "providers": {
    "default": "deepseek",
    "profiles": {
      "deepseek": {
        "type": "openai-compat",
        "providerName": "DeepSeek",
        "apiKeyEnv": "DEEPSEEK_API_KEY",
        "baseUrl": "https://api.deepseek.com/v1",
        "baseUrlEnv": "DEEPSEEK_BASE_URL",
        "defaultModel": "deepseek-chat"
      },
      "kimi": {
        "type": "openai-compat",
        "providerName": "Kimi",
        "apiKeyEnv": "MOONSHOT_API_KEY",
        "baseUrl": "https://api.moonshot.ai/v1",
        "baseUrlEnv": "MOONSHOT_BASE_URL",
        "defaultModel": "kimi-k2.5"
      },
      "kimi-code": {
        "type": "openai-compat",
        "providerName": "Kimi Code",
        "apiKeyEnv": "KIMI_CODE_API_KEY",
        "baseUrl": "https://api.kimi.com/coding/v1",
        "baseUrlEnv": "KIMI_CODE_BASE_URL",
        "defaultModel": "kimi-for-coding"
      },
      "qwen": {
        "type": "openai-compat",
        "providerName": "Qwen",
        "apiKeyEnv": "DASHSCOPE_API_KEY",
        "baseUrl": "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "baseUrlEnv": "DASHSCOPE_BASE_URL",
        "defaultModel": "qwen-plus"
      },
      "openai-compat": {
        "type": "openai-compat",
        "providerName": "OpenAI Compatible",
        "apiKeyEnv": "OPENAI_API_KEY",
        "baseUrl": "https://api.openai.com/v1",
        "baseUrlEnv": "OPENAI_BASE_URL",
        "defaultModel": "gpt-4o-mini"
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

这说明该研究插件并**不绑定 Claude 单一模型**，而是尽量通过 provider profile 保持模型后端可替换。

如果你已经进入 REPL，可以直接：

```text
/provider kimi
/model kimi-k2.5

/provider kimi-code
/model kimi-for-coding

/provider qwen
/model qwen-plus
```

另外，survey research bootstrap 现在会生成：

- `.claw/helpers/plotting.py`
- `.claw/helpers/plotting.R`

建议所有 agent 生成的 Python / R 作图脚本都先调用这些 helper，再画中文/CJK 图表。

---

## 7. 各工具说明

## `survey_metadata`

用于先看清楚数据，再决定后续分析。

### 适合做什么

- 检查行数、列数、字段名
- 查看每列类型、缺失率、样本值
- 检查量表定义里的题项是否存在
- 检查反向题声明是否有拼写错误
- 为后续 psychometrics / report 提供上下文

### 关键输入

- `datasetPath`：必填
- `format`：可选，`csv` / `xlsx` / `sav` / `dat` / `auto`
- `sheetName`：Excel 工作表
- `delimiter` / `encoding` / `naValues`：文本类文件可选
- `scaleDefinitions`：量表定义
- `reverseItems`：反向题列表

### 关键输出

- 数据集规模、格式、路径
- schema 摘要
- 缺失率摘要
- 问卷条目覆盖情况
- backend 建议

---

## `survey_score`

用于把问卷条目真正转成后续可分析的量表分数。

### 已支持能力

- 量表级 `mean` / `sum` 计分
- 反向题处理
- 顶层 `reverseItems` + scale 内 `reverseItems` 合并
- `responseScale.min/max` 指定反向计分边界
- `minValidItems` 控制最少有效题项数
- 自定义输出列名 `outputColumn`
- 输出预览行
- 可将打分后数据写出为 `.csv` / `.tsv` / `.xlsx`

### 关键输入

- `datasetPath`：必填
- `scaleDefinitions`：必填
- `reverseItems`：可选
- `responseScale`：可选，建议对 Likert 量表显式提供
- `idColumns`：可选，用于在预览中保留样本 ID
- `outputPath`：可选，写出打分后数据集

### 单个量表定义示例

```json
{
  "name": "engagement",
  "items": ["q1", "q2", "q3", "q4"],
  "reverseItems": ["q3"],
  "outputColumn": "engagement_mean",
  "method": "mean",
  "minValidItems": 3
}
```

### 输出会包含什么

- 每个量表的可用题项 / 缺失题项
- 实际应用了哪些反向题
- 有多少行成功得分
- 得分摘要（均值、标准差、最小值、最大值）
- 预览行
- 如果写出文件，则返回 artifact 信息

### 一个完整示例

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
  "idColumns": ["id"],
  "outputPath": ".claw/artifacts/mini_scored.csv"
}' | \
  env CLAW_TOOL_NAME=survey_score \
      CLAW_PLUGIN_ID=research-survey@bundled \
      CLAW_PLUGIN_ROOT=$(pwd)/rust/crates/plugins/bundled/research-survey \
      CLAW_WORKSPACE_ROOT=$(pwd) \
      python3 rust/crates/plugins/bundled/research-survey/tools/survey_tools.py
```

### 使用建议

- 对反向题尽量显式提供 `responseScale.min/max`
- 如果你希望避免缺失题过多导致分数失真，请设置 `minValidItems`
- 如果要进入后续报告或进一步分析，建议总是写出 `outputPath`

---

## `survey_psychometrics`

这个工具走 R 后端，负责更偏科研统计的一段。

### 适合做什么

- Cronbach's alpha
- McDonald's omega（条件允许时）
- KMO
- Bartlett 球形检验
- 可选 CFA（基于 lavaan）

### 关键输入

- `datasetPath`：必填
- `scaleDefinitions`：强烈建议提供
- `reverseItems`：可选
- `responseScale`：用于反向计分边界
- `cfaModel`：可选，不提供则只做信度/效度前置检查
- `estimator` / `missingHandling`：可按需要指定
- 固定宽度 `.dat` 可配 `layout = "fixed-width"` + `widths` / `columnWidths`

### 关键输出

- `reliability`
- `validity`
- `cfa`
- `warnings`

### 设计约束

R 后端目标是**只输出 JSON**。如果出现收敛问题、奇异矩阵、缺失题项等情况，会尽量进入 `warnings`，避免把原始 R 噪声直接泄漏到 stdout/stderr。

### 示例

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

---

## `survey_report`

用于把结构化分析结果快速整理成 Markdown 报告草稿。

### 适合做什么

- 先生成分析初稿
- 用于人工复核
- 作为后续 Quarto / PDF / 正式报告的基础

### 关键输入

- `title`
- `datasetPath`
- `datasetSummary`
- `results`
- `notes`
- `outputPath`

### 关键输出

- 渲染后的 Markdown 文本
- 可选 Markdown artifact
- 可选 `report.input.json` / `report.review.json` 审查配套 artifact

这个工具现在仍然是**报告草稿入口**，但已经开始输出配套的 review artifact，用于检查：

- 结构是否接近 SSCI / Results 风格
- 文字是否仍然带有 scaffold / tool-like 痕迹
- 后续是否需要 targeted revision

当前还会在 review 失败时自动做**一轮有界定向修订**，然后把修订后的结果重新写回最终 markdown 和 review artifact。

---

## 8. 依赖要求

### Python 侧

基础依赖：
- `pandas`

按格式附加：
- `openpyxl`：读取 / 写入 `.xlsx`
- `pyreadstat`：读取 `.sav`

### R 侧

- `jsonlite`
- `psych`
- `lavaan`
- `readxl`：读取 `.xlsx`
- `haven`：读取 `.sav`

如果缺少依赖，工具会尽量返回**结构化 JSON 错误**，而不是只抛出无法解析的原始异常。

---

## 9. 当前边界与注意事项

当前版本仍然是**科研分析能力雏形**，不是完整统计平台。已知边界包括：

- `survey_score` 当前聚焦常见量表计分，不覆盖复杂 IRT / 多层模型 / 高级插补流程
- Python 侧对复杂固定宽度 `.dat` 的支持仍偏基础
- `survey_report` 目前是 Markdown 草稿，不是最终发表级排版
- CFA 质量仍依赖研究者提供合理模型、样本量与数据质量
- 统计结果应由研究者复核，不应直接视为可发表结论

---

## 10. 推荐下一步演进

如果要继续把 claw-code 往“科研数据分析智能体”方向推进，下一批高价值能力通常会是：

1. 更完整的量表 scoring 模板库
2. EFA / 因子提取与旋转
3. 分组差异检验 / 回归 / 中介调节分析
4. 自动生成结果表格（APA 风格）
5. Quarto / Word / PDF 输出
6. 面向论文写作的结果解释与报告 skills

---

## 11. 总结

这个插件当前已经形成了一个比较清晰的科研分析基础链路：

- **模型层可替换**：不绑定 Claude，可接 OpenAI-compatible / DeepSeek
- **内核层尽量轻耦合**：重点把能力放在插件与桥接层
- **分析层双后端**：Python 负责数据处理，R 负责统计分析
- **流程层完整闭环**：metadata → scoring → psychometrics → report

如果你的目标是把这个项目做成一个专门用于**问卷科研数据分析**的 CLI 智能体，这个 README 对应的插件就是当前最核心的一块能力基座。
