---
description: 调用 architect agent 进行架构评审、设计讨论与技术决策。支持架构审查、设计提案和 ADR 生成。
---

# Archit 命令

该命令会调用 **architect** agent，进行架构评审、问题讨论、设计提案与技术决策记录。

## 这个命令会做什么

1. **分析范围** - 确定评审对象（crate、模块、功能、全系统）
2. **读取代码** - 通过 Read/Grep/Glob 工具深入分析现有架构
3. **产出结构化交付物** - 严格按照 agent 文档规范输出
4. **等待确认** - 涉及文档落盘时必须收到用户明确操作

## 何时使用

在以下场景使用 `/archit`：
- 架构评审：审查现有架构的健康度与问题
- 设计讨论：讨论新功能或重构的架构方案
- 技术决策：评估多个方案的取舍并产出 ADR
- 瓶颈分析：识别可扩展性与性能问题
- 一致性检查：确保新设计与现有架构模式一致

## 工作方式

architect agent 将会：

1. **明确任务类型**：判断是架构评审、设计提案还是 ADR
2. **现状分析**：审查现有架构、识别模式与约定、记录技术债务
3. **需求收集**：整理功能性与非功能性需求、集成点、数据流
4. **产出交付物**：根据任务类型选择对应的输出格式（见下方）
5. **等待确认**：展示结果并等待用户的明确操作（`modify` 或 `save`）

## 输出格式

architect agent 必须严格按以下模板输出，根据任务类型选择对应格式：

### 格式 A：架构评审

```markdown
# 架构评审：{评审范围}

## 现状分析
- 架构概述
- 已有模式与约定
- 技术债务
- 可扩展性评估

## 发现

### 优势
- {做得好的地方}

### 问题
- {发现的问题，按严重程度排序：CRITICAL > HIGH > MEDIUM > LOW}

## 建议
- {可操作的改进建议与理由}
```

### 格式 B：设计提案

```markdown
# 设计提案：{功能/变更}

## 背景
{问题描述与动机}

## 需求
- 功能性：{必须做什么}
- 非功能性：{性能、安全、可扩展性目标}

## 方案设计
- 组件职责划分
- 数据流
- API 契约 / trait 定义
- 集成点

## 取舍分析

| 决策点 | 方案 A | 方案 B | 选择 | 理由 |
|--------|--------|--------|------|------|
| ...    | ...    | ...    | ...  | ...  |

## 风险与缓解
- {风险}：{缓解措施}

## 实施阶段
1. 阶段 1：{范围}
2. 阶段 2：{范围}
```

### 格式 C：架构决策记录（ADR）

严格遵循 `docs/architecture/adr/` 下的 ADR 规范：

```markdown
# ADR-{序号}: {决策标题}

## 背景
{为什么需要做这个决策，当前面临的问题或需求}

## 决策
{选择了什么方案}

## 影响

### 正面影响
- {收益}

### 负面影响
- {代价}

### 备选方案
- **{方案}**：{简述及未选原因}

## 状态
Proposed / Accepted / Deprecated / Superseded by ADR-xxx

## 日期
YYYY-MM-DD
```

## 文档落盘规范

architect agent 在用户确认 `save` 后，按以下规范保存文件：

| 交付物类型 | 存储位置 | 命名规则 |
|-----------|---------|---------|
| 架构评审 | `docs/architecture/changelog/` | `YYYY-MM-DD-{英文简称}.md` |
| 设计提案 | `docs/architecture/` | 由 agent 根据内容决定 |
| ADR | `docs/architecture/adr/` | `ADR-{序号}-{英文简称}.md` |
| 架构主文档更新 | `docs/architecture/ARCHITECTURE.md` | 全量更新 |

## 使用示例

```
用户：/archit xclaw-memory
→ 启动 architect agent 对 xclaw-memory crate 进行架构评审
→ 返回：现状分析、发现、建议

用户：/archit 设计 headless server 模式
→ 启动 architect agent 创建设计提案
→ 返回：设计提案含取舍分析与实施阶段

用户：/archit ADR: 内存存储应该用 SQLite 还是 sled？
→ 启动 architect agent 产出 ADR
→ 返回：结构化 ADR 含备选方案与理由
```

## 项目上下文

architect agent 在评审时必须遵循以下 xClaw 架构原则：

1. **Trait 归属**：trait 属于定义该领域概念的 crate，不放在 xclaw-core
2. **AIOS 兼容**：角色定义对齐 AIOS agent config 规范
3. **文件优先持久化**：用户数据用 Markdown/YAML；结构化查询用 SQLite
4. **xclaw-config 只管配置**：不放业务逻辑
5. **不可变模式**：始终创建新对象，不改变已有对象
6. **多小文件**：高内聚低耦合；200-400 行常态，800 行上限
7. **Dyn-safety 意识**：多数 trait 用 `impl Future` 非 dyn-safe；仅 `Tool` trait 用 `#[async_trait]`
8. **Rust Edition 2024**

## 重要说明

**关键**：在你明确选择 `save` 之前，architect agent **不会** 写入任何文件。

若你希望修改，请回复：
- `modify: [你的修改内容]`
- `modify: [替代方案]`

若你确认保存，请回复：
- `save`

## 与其他命令的集成

完成架构设计后：
- 使用 `/plan` 制定详细实施计划
- 使用 `/tdd` 以测试驱动方式实现
- 使用 `/code-review` 做复查

## 相关 Agent

该命令调用项目下的 `architect` agent（`.claude/agents/architect.md`）。
