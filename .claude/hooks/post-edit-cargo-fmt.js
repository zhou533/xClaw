// PostToolUse hook: auto-format .rs files with cargo fmt after edits
'use strict';

const { execFileSync } = require('child_process');

let data = '';
process.stdin.on('data', (chunk) => (data += chunk));
process.stdin.on('end', () => {
  const input = JSON.parse(data);
  const filePath = input.tool_input?.file_path || '';

  if (/\.rs$/.test(filePath)) {
    try {
      execFileSync('cargo', ['fmt', '--', filePath], {
        stdio: 'pipe',
        timeout: 15000,
      });
      process.stderr.write(`[cargo fmt] Formatted ${filePath}\n`);
    } catch (e) {
      process.stderr.write(
        `[cargo fmt] Warning: format failed for ${filePath}: ${e.message}\n`
      );
    }
  }

  process.stdout.write(data);
});
