// "Zero unwanted connections" check (architecture §4.1, task 2.1).
//
//   node scripts/check-connections.mjs [path/to/kendime-brow(.exe)] [idle seconds = 60]
//
// Starts the browser with a fresh temporary profile on kb://newtab/, stays idle, closes it
// gracefully and inspects the NetLog: every network destination outside ALLOWLIST fails the
// check (exit code 1). The architecture target is 5 minutes idle; CI uses a shorter window.

import { spawn, execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir, platform } from 'node:os';
import { join, resolve } from 'node:path';

const exe = resolve(
  process.argv[2] ?? join('target', 'debug', platform() === 'win32' ? 'kendime-brow.exe' : 'kendime-brow'),
);
const idleSeconds = Number(process.argv[3] ?? 60);

// Hosts the browser may contact without a user action. Empty on purpose: update checks and
// filter-list downloads (the documented exceptions) do not exist yet.
const ALLOWLIST = [];

const profile = mkdtempSync(join(tmpdir(), 'kb-netcheck-'));
const netlog = join(profile, 'netlog.json');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

console.log(`exe: ${exe}\nprofile: ${profile}\nidle: ${idleSeconds}s`);
const child = spawn(exe, ['--url=kb://newtab/', `--log-net-log=${netlog}`, '--net-log-capture-mode=Default'], {
  env: { ...process.env, KB_PROFILE_DIR: profile },
  stdio: 'ignore',
});
await sleep(idleSeconds * 1000);

// Graceful close so the NetLog file is finalized.
if (platform() === 'win32') {
  execFileSync('powershell', [
    '-NoProfile',
    '-Command',
    `(Get-Process -Id ${child.pid}).CloseMainWindow() | Out-Null`,
  ]);
} else {
  child.kill('SIGTERM');
}
const exited = await Promise.race([new Promise((r) => child.on('exit', () => r(true))), sleep(15000).then(() => false)]);
if (!exited) {
  child.kill('SIGKILL');
  console.error('browser did not exit in time');
}

if (!existsSync(netlog)) {
  console.error('no NetLog written');
  process.exit(2);
}
let text = readFileSync(netlog, 'utf8').trim();
// A NetLog cut short by a crash may lack the closing brackets.
if (!text.endsWith('}')) text = text.replace(/,\s*$/, '') + ']}';
const log = JSON.parse(text);

// Hosts blocked by `--host-resolver-rules` (single source: crates/kb-shell/src/privacy_flags.rs).
// Requests to them never leave the machine (no DNS query, no socket), so they are reported
// as "blocked attempts", not failures.
const rs = readFileSync(join(import.meta.dirname, '..', 'crates', 'kb-shell', 'src', 'privacy_flags.rs'), 'utf8');
const blockedPatterns = [...rs.slice(rs.indexOf('BLOCKED_HOSTS')).split('];')[0].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
const isBlocked = (h) => blockedPatterns.some((p) => (p.startsWith('*.') ? h.endsWith(p.slice(1)) : h === p));

const eventName = Object.fromEntries(Object.entries(log.constants?.logEventTypes ?? {}).map(([k, v]) => [v, k]));
const hostOf = (url) => {
  try {
    const u = new URL(url);
    return ['http:', 'https:', 'ws:', 'wss:'].includes(u.protocol) ? u.hostname.replace(/^\[|\]$/g, '') : null;
  } catch {
    return null;
  }
};
// Loopback and private networks (system DNS resolver, proxy auto-config) are not destinations
// chosen by the browser.
const local = (h) =>
  h === 'localhost' || h === 'wpad' || h.startsWith('127.') || h === '::1' ||
  /^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|169\.254\.|fe80:)/i.test(h);

const failures = new Map();
const info = new Map();
const note = (map, key, why) => map.has(key) || map.set(key, why);
for (const e of log.events ?? []) {
  const p = e.params ?? {};
  const type = eventName[e.type] ?? '';
  if (typeof p.url === 'string') {
    const h = hostOf(p.url);
    if (h && !local(h)) {
      if (isBlocked(h)) note(info, `blocked: ${h}`, p.url.slice(0, 120));
      else if (!ALLOWLIST.includes(h)) note(failures, h, `request ${p.url.slice(0, 120)}`);
    }
  }
  // Real connections: a socket connect that succeeded (no net_error) to a public address.
  if (/(TCP_CONNECT|SOCKET_CONNECT|UDP_CONNECT)$/.test(type) && typeof p.address === 'string') {
    const h = hostOf(`https://${p.address}`);
    if (!h || local(h)) continue;
    if (p.net_error !== undefined) {
      note(info, `probe/failed: ${h}`, `${type} ${p.address} net_error=${p.net_error}`);
    } else if (type === 'UDP_CONNECT' && /^(2001:4860:4860::8888|8\.8\.8\.8)$/.test(h)) {
      // Chromium's reachability probe: UDP connect only picks a source address, sends nothing.
      note(info, `probe: ${h}`, `${type} ${p.address}`);
    } else {
      note(failures, h, `${type} ${p.address}`);
    }
  }
}

rmSync(profile, { recursive: true, force: true });
console.log(`events: ${(log.events ?? []).length}`);
for (const [k, why] of info) console.log(`  info  ${k}   (${why})`);
if (failures.size) {
  console.error('UNWANTED CONNECTIONS:');
  for (const [h, why] of failures) console.error(`  FAIL  ${h}   (${why})`);
  process.exit(1);
}
console.log('OK: no unwanted connections');
