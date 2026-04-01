# 实施计划：角色目录创建时自动拷贝 Bootstrap 模板文件

## 概览

当程序创建角色目录时，自动将 `docs/reference/template/bootstrap/` 下的模板文件写入新角色目录。模板在编译时通过 `include_str!` 嵌入二进制，运行时无需依赖源码目录。对已有角色目录（升级场景），补充缺失的模板文件。

## 需求

- `MemoryFileKind` 有 8 个变体，其中 7 个有对应模板（Heartbeat 无模板）
- 模板文件：AGENTS.md、BOOTSTRAP.md、SOUL.md、IDENTITY.md、USER.md、TOOLS.md、MEMORY.md
- 角色目录创建集中在 `FsRoleManager::create_role`
- 仅在文件不存在时写入（幂等），不覆盖已有文件
- 已有角色目录（升级场景）也需补充缺失模板

## 实施阶段

### 阶段 1：模板嵌入模块（1 个新文件 + 1 个修改）

1. **新建** `crates/xclaw-memory/src/workspace/templates.rs`
   - 用 `include_str!` 嵌入 7 个模板文件
   - 提供 `fn bootstrap_template(kind: MemoryFileKind) -> Option<&'static str>`，exhaustive match
   - 提供 `async fn ensure_bootstrap_templates(role_dir: &Path) -> Result<(), MemoryError>`
     - 遍历 `MemoryFileKind::all()`，对有模板的 Kind 写入文件（仅当不存在时）

2. **修改** `crates/xclaw-memory/src/workspace/mod.rs`
   - 导出 `templates` 模块

### 阶段 2：角色创建 + 默认角色处理（2 个文件修改）

3. **修改** `crates/xclaw-memory/src/role/manager.rs`
   - 在 `FsRoleManager::create_role` 中，创建目录和 `role.yaml` 后调用 `ensure_bootstrap_templates(role_dir)`

4. **修改** `crates/xclaw-memory/src/facade.rs`
   - 在 `ensure_default_role` 中，角色已存在时也调用 `ensure_bootstrap_templates(role_dir)` 补充缺失模板
   - 角色不存在时走 `create_role` 路径（已包含模板写入）

### 阶段 3：测试

5. **单元测试** `templates.rs` — 7 个返回 `Some`，Heartbeat 返回 `None`，内容非空
6. **单元测试** `manager.rs` — 创建角色后断言模板文件存在
7. **集成测试** — 验证 `ensure_default_role` 对已有角色补充缺失模板

## Default 角色处理

| 场景 | 行为 |
|------|------|
| 全新安装（首次运行） | `ensure_default_role` → `create_role` → 模板自动写入 |
| 已有 default 角色（升级） | `ensure_default_role` → 角色存在 → `ensure_bootstrap_templates` 补充缺失文件 |
| 手动创建新角色 | `create_role` → 模板自动写入 |

## 风险

| 风险 | 等级 | 缓解 |
|------|------|------|
| `include_str!` 路径在 CI 中失效 | 中 | 使用 `CARGO_MANIFEST_DIR` 相对路径 |
| 模板写入部分失败 | 低 | warning 日志，不中断角色创建 |
| 新增 Kind 忘记加模板 | 低 | exhaustive match，编译器强制 |

## 复杂度：低
