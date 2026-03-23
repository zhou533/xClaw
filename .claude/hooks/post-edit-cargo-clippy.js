// PostToolUse hook: run cargo clippy after editing .rs files (async, non-blocking)
'use strict';

const { execFileSync } = require('child_process');

let data = '';
process.stdin.on('data', (chunk) => (data += chunk));
process.stdin.on('end', () => {
  const input = JSON.parse(data);
  const filePath = input.tool_input?.file_path || '';

  if (/\.rs$/.test(filePath)) {
    try {
      const result = execFileSync(
        'cargo',
        ['clippy', '--all-targets', '--', '-D', 'warnings'],
        { stdio: 'pipe', timeout: 60000 }
      );
      const output = result.toString().trim();
      if (output) {
        process.stderr.write(`[cargo clippy] ${output}\n`);
      }
    } catch (e) {
      const stderr = e.stderr ? e.stderr.toString().trim() : e.message;
      if (stderr) {
        process.stderr.write(`[cargo clippy] Warnings/errors:\n${stderr}\n`);
      }
    }
  }

  process.stdout.write(data);
});
