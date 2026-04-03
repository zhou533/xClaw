# Plan: Memory Module Tool Description Clarity

## Overview

xclaw-memory crate has 9 LLM-callable tools with vague or incomplete descriptions. This plan improves each description so LLMs can accurately determine when to call each tool and what to expect.

## Changes

### Phase 1: Daily Memory Tools (`memory_tools.rs`)

1. **`memory_daily_append` description**
   - Current: "Append an entry to today's daily memory"
   - New: "Append a Markdown entry to today's daily memory file. Each day has a separate file under the role's memory directory. The entry is appended to the end of the file."
   - Parameter `entry`: "Memory entry to append" -> "Markdown text to append to today's daily memory"

2. **`memory_daily_read` description**
   - Current: "Read daily memory for a specific date"
   - New: "Read the full content of a daily memory file for a given date (YYYY-MM-DD). Returns the entire day's entries as Markdown text, or an empty string if no entries exist for that date."

3. **Shared parameter `role`**: "Role name (default: 'default')" -> "Role identifier in snake_case (default: 'default')"

### Phase 2: Memory File Tools (`memory_file_tools.rs`)

4. **`memory_file_read` description**
   - Current: "Read a memory file (MEMORY.md, SOUL.md, AGENTS.md, etc.) for a role"
   - New: "Read a role's memory file by kind. Supported files: AGENTS.md (collaboration rules), SOUL.md (AI persona), TOOLS.md (tool guidance), IDENTITY.md (self-identity), USER.md (user preferences), HEARTBEAT.md (action reference), BOOTSTRAP.md (workspace bootstrap), MEMORY.md (long-term knowledge). Returns the file content or a message if it does not exist."

5. **`memory_file_write` description**
   - Current: "Write a memory file (MEMORY.md, SOUL.md, AGENTS.md, etc.) for a role"
   - New: "Write (overwrite) a role's memory file by kind. The entire file content is replaced. Creates the file if it does not exist. Supported kinds: agents, soul, tools, identity, user, heartbeat, bootstrap, long_term."
   - Parameter `content`: "File content to write" -> "Markdown content to write. Replaces the entire file."

6. **`memory_file_delete` description**
   - Current: "Delete a memory file (BOOTSTRAP.md, etc.) for a role"
   - New: "Delete a role's memory file by kind. Primarily used to remove BOOTSTRAP.md after workspace bootstrap is complete. Other memory file kinds can also be deleted."

7. **Shared parameter `kind`**: "Memory file kind" -> "Memory file kind. agents=collaboration rules, soul=AI persona, tools=tool guidance, identity=self-identity, user=user preferences, heartbeat=action reference, bootstrap=workspace bootstrap, long_term=distilled knowledge (MEMORY.md)"

8. **Shared parameter `role`**: "Role name (default: 'default')" -> "Role identifier in snake_case (default: 'default')"

### Phase 3: Role Tools (`role_tools.rs`)

9. **`role_create` description**
   - Current: "Create a new role with configuration (name, description, system_prompt, tools)"
   - New: "Create a new role with its configuration and initialize its memory directory with bootstrap templates. Role name must be snake_case."

10. **`role_list` description**
    - Current: "List all available roles"
    - New: "List all available roles. Returns a JSON array of role names."

11. **`role_get` description**
    - Current: "Get configuration details of a specific role"
    - New: "Get the full configuration of a specific role. Returns YAML with name, description, system_prompt, tools, and memory_dir fields."

12. **`role_delete` description**
    - Current: "Delete a role (cannot delete 'default')"
    - New: "Delete a role and all its memory files. The 'default' role cannot be deleted."

13. **Shared parameter `role`/`name`**: Unify to "Role identifier in snake_case"

### Phase 4: Verification

14. `cargo test -p xclaw-memory`
15. `cargo clippy -p xclaw-memory -- -D warnings`
16. `cargo fmt --check`
