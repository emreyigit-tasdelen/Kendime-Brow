// Performance check against the budgets of architecture §5.1 (task 2.11).
//
//   node scripts/perf.mjs [path/to/kendime-brow(.exe)]   (use a --release build for real numbers)
//
// Measures: cold start until the UI strip is loaded, and total memory of the process tree
// 10 s after start with one empty tab. Exit code 1 if a budget is exceeded (BUDGET_* env
// variables override; debug builds are expected to exceed them).

import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir, platform } from 'node:os';
import { join, resolve } from 'node:path';

const exe = resolve(
  process.argv[2] ?? join('target', 'release', platform() === 'win32' ? 'kendime-brow.exe' : 'kendime-brow'),
);
const budgetStartMs = Number(process.env.BUDGET_START_MS ?? 800);
const budgetRamMb = Number(process.env.BUDGET_RAM_MB ?? 250);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const profile = mkdtempSync(join(tmpdir(), 'kb-perf-'));
const t0 = performance.now();
const child = spawn(exe, ['--remote-debugging-port=9333', '--url=kb://newtab/'], {
  env: { ...process.env, KB_PROFILE_DIR: profile },
  stdio: 'ignore',
});

const pages = async () => {
  try {
    return (await (await fetch('http://127.0.0.1:9333/json')).json()).filter((t) => t.type === 'page');
  } catch {
    return [];
  }
};
let startMs = null;
while (startMs === null && performance.now() - t0 < 30000) {
  if ((await pages()).some((p) => p.url.startsWith('kb://ui'))) startMs = Math.round(performance.now() - t0);
  else await sleep(25);
}
await sleep(10000);

const ui = (await pages()).find((p) => p.url.startsWith('kb://ui'));
const tasks = await new Promise((resolveTasks) => {
  const ws = new WebSocket(ui.webSocketDebuggerUrl);
  const expr = `new Promise(r => kbQuery({request: '{"cmd":"tasks"}', onSuccess: r, onFailure: () => r('[]')}))`;
  ws.onopen = () => ws.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression: expr, awaitPromise: true, returnByValue: true } }));
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id === 1) resolveTasks(JSON.parse(m.result.result.value));
  };
});
const ramMb = Math.round(tasks.reduce((n, t) => n + t.memory, 0) / 1048576);

if (platform() === 'win32') {
  execFileSync('powershell', ['-NoProfile', '-Command', `(Get-Process -Id ${child.pid}).CloseMainWindow() | Out-Null`]);
} else child.kill('SIGTERM');
await sleep(3000);
child.kill('SIGKILL');
rmSync(profile, { recursive: true, force: true });

console.log(`cold start (UI loaded): ${startMs} ms (budget ${budgetStartMs})`);
console.log(`memory, ${tasks.length} processes, empty tab: ${ramMb} MB (budget ${budgetRamMb})`);
const failed = startMs === null || startMs > budgetStartMs || ramMb > budgetRamMb;
console.log(failed ? 'OVER BUDGET' : 'WITHIN BUDGET');
process.exit(failed ? 1 : 0);
