# 实施计划：将 memory_base_dir 从 "memory" 重命名为 "workspace"

## 需求重述

将 `apps/cli/src/setup.rs` 中 `memory_base_dir()` 的返回路径从 `~/.xclaw/memory` 改为 `~/.xclaw/workspace`。仅修改**顶层基目录名称**，不影响 `xclaw-memory` crate 内部的 `roles/<role>/memory/` 子目录结构。

## 实施步骤

### 阶段 1：代码变更（1 个文件）

1. **修改 `memory_base_dir` 函数**（`apps/cli/src/setup.rs:128-131`）
   - 注释：`~/.xclaw/memory.` → `~/.xclaw/workspace.`
   - 函数体：`.join("memory")` → `.join("workspace")`

2. **修改单元测试**（`apps/cli/src/setup.rs:157-163`）
   - 函数名：`memory_base_dir_ends_with_memory` → `memory_base_dir_ends_with_workspace`
   - 断言：`ends_with("memory")` → `ends_with("workspace")`

### 不需要变更的文件

以下 `join("memory")` 引用指的是 role 内部子目录（`roles/<role>/memory/`），语义不同，**保持不变**：

- `crates/xclaw-memory/src/role/daily.rs:46`
- `crates/xclaw-memory/src/role/manager.rs:70`
- `crates/xclaw-memory/tests/` 中的多处测试

## 风险

| 级别 | 风险 | 缓解 |
|------|------|------|
| 中 | 现有用户 `~/.xclaw/memory/` 下的数据变更后不可见 | 用户手动重命名目录，或设置 `XCLAW_DATA_DIR` 环境变量 |
| 低 | 测试断言不一致 | 同步更新测试 |

## 测试策略

- 单元测试：`cargo test -p xclaw-cli memory_base_dir`
- 集成测试：`cargo test -p xclaw-memory`（确认不受影响）
- 全量测试：`cargo test`

## 成功标准

- [ ] `memory_base_dir()` 返回以 "workspace" 结尾的路径
- [ ] 测试 `memory_base_dir_ends_with_workspace` 通过
- [ ] `cargo test` 全部通过，无回归
- [ ] `xclaw-memory` crate 内部的 `roles/<role>/memory/` 路径未被修改

## 复杂度：低
