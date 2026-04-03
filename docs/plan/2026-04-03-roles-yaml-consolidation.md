# 实施计划：角色配置统一存储（roles.yaml）

## 概览

将分散在各角色目录中的 `role.yaml` 文件统一为工作区根目录下的单一 `roles.yaml` 文件。新文件采用 YAML map 格式，key 为 `role_id`，value 包含原有 `RoleConfig` 字段加上新增的 `memory_dir` 字段。

## 需求

- 所有角色配置保存在 `{base_dir}/roles.yaml` 单一文件中
- 文件格式为 YAML map，key 为 `role_id`（snake_case 字符串）
- 每个 value 包含现有 `RoleConfig` 字段（name、description、system_prompt、tools、meta）加上新增 `memory_dir` 字段
- `memory_dir` 字段指向该角色的记忆目录路径（相对于 base_dir）
- `RoleManager` trait 的方法签名不变，仅实现变化
- 删除角色后，角色目录（含 memory 子目录、bootstrap 模板等）仍然保留，仅从 `roles.yaml` 中移除条目
- 角色的 memory 文件（AGENTS.md、SOUL.md 等）和 daily memory 仍存储在 `roles/{name}/` 目录下（不变）

## 新 `roles.yaml` 文件结构

```yaml
secretary:
  name: secretary
  description:
    - "日程管理"
    - "邮件处理"
  system_prompt: "你是私人秘书"
  tools:
    - shell
    - file_read
  meta:
    author: user
    version: "1.0.0"
    license: private
    created_at: "2026-03-25"
  memory_dir: roles/secretary

default:
  name: default
  description:
    - "Default AI assistant"
  system_prompt: ""
  tools: []
  meta:
    author: user
    version: "1.0.0"
    license: private
  memory_dir: roles/default
```

## 实施阶段

### 阶段 1：数据模型变更（`crates/xclaw-memory/src/role/config.rs`）

1. **扩展 `RoleConfig` 结构体**
   - 新增字段 `pub memory_dir: String`，加 `#[serde(default)]`
   - 修改 `default_config()` 方法，设 `memory_dir: "roles/default".to_string()`
   - 新增类型 `pub type RolesFile = BTreeMap<String, RoleConfig>;`
   - 新增辅助函数：`parse_roles_file(content: &str) -> Result<RolesFile, MemoryError>` 和 `serialize_roles_file(roles: &RolesFile) -> Result<String, MemoryError>`

2. **更新测试**
   - 更新 `default_config_has_expected_fields` 断言 `memory_dir == "roles/default"`
   - 更新 `round_trip_yaml` 加入 `memory_dir` 字段
   - 新增 `roles_file_roundtrip` 测试
   - 新增 `roles_file_with_multiple_entries` 测试
   - 验证旧格式 YAML（无 `memory_dir`）仍可解析

### 阶段 2：重写 `FsRoleManager`（`crates/xclaw-memory/src/role/manager.rs`）

3. **重写读写逻辑**
   - 新增 `fn roles_yaml_path(&self) -> PathBuf` → `self.base_dir.join("roles.yaml")`
   - 新增 `async fn load_roles_file(&self) -> Result<RolesFile, MemoryError>`
   - 新增 `async fn save_roles_file(&self, roles: &RolesFile) -> Result<(), MemoryError>`（原子写入）
   - 重写 `create_role`：加载 roles.yaml → 检查重复 → 设置 `memory_dir` 默认值 → 创建目录 → 写回
   - 重写 `get_role`：加载 roles.yaml → 按 key 查找
   - 重写 `list_roles`：加载 roles.yaml → 返回所有 value
   - 重写 `delete_role`：加载 roles.yaml → 移除 key → 写回（不删除目录）
   - 删除旧 `role_yaml_path` 方法
   - `role_dir` 方法保持不变

4. **更新测试**
   - 文件路径断言从 `roles/{name}/role.yaml` 改为 `roles.yaml`
   - 新增 `roles_yaml_persists_multiple_roles` 测试
   - 新增 `create_role_sets_memory_dir_default` 测试

### 阶段 3：上游集成

5. **更新 `facade.rs`**
   - `ensure_default_role` 适配新的 roles.yaml 存储方式

6. **更新 `StubRoleManager`**（`crates/xclaw-agent/src/test_support.rs`）
   - `get_role` 返回值加入 `memory_dir: "roles/default".to_string()`

7. **更新集成测试**（`crates/xclaw-memory/tests/memory_system_integration.rs`）
   - 断言路径从 `roles/default/role.yaml` 改为 `roles.yaml`

### 阶段 4：文档

8. **更新 `CLAUDE.md`**
   - 注明角色配置统一存储在 `roles.yaml`

## 不变的部分

- `RoleManager` trait 方法签名（包括 `role_dir`）
- `MemoryFileLoader` / `FsMemoryFileLoader`
- `DailyMemory` / `FsDailyMemory`
- bootstrap templates 机制
- `LoopAgent` / `AgentConfig` / `SystemPromptBuilder`

## 风险与缓解

| 风险 | 级别 | 缓解 |
|------|------|------|
| 并发写入 `roles.yaml` 数据损坏 | 高 | tempfile + rename 原子写入 |
| `delete_role` 不删目录可能困惑 | 低 | 日志明确提示 |
| `memory_dir` 向后兼容 | 低 | `serde(default)` |

## 成功标准

- [ ] `roles.yaml` 单一文件正确存储所有角色配置
- [ ] `RoleConfig` 包含 `memory_dir` 字段
- [ ] CRUD 操作全部通过新实现工作正常
- [ ] memory/daily/bootstrap 功能不受影响
- [ ] 所有测试通过
- [ ] `cargo clippy -- -D warnings` 和 `cargo fmt --check` 通过
