// PostToolUse hook: run cargo check after editing .rs files to verify compilation
'use strict';

const { execFileSync } = require('child_process');

let data = '';
process.stdin.on('data', (chunk) => (data += chunk));
process.stdin.on('end', () => {
  const input = JSON.parse(data);
  const filePath = input.tool_input?.file_path || '';

  if (/\.rs$/.test(filePath)) {
    try {
      execFileSync('cargo', ['check', '--all-targets'], {
        stdio: 'pipe',
        timeout: 60000,
      });
    } catch (e) {
      const stderr = e.stderr ? e.stderr.toString().trim() : e.message;
      if (stderr) {
        process.stderr.write(`[cargo check] Compilation errors:\n${stderr}\n`);
      }
    }
  }

  process.stdout.write(data);
});
