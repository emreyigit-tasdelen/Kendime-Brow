// kb://term/ — terminal (tasks 4A.2/4A.3): xterm.js on a local pseudo terminal or an SSH
// session, both owned by the shell. `?host=&user=&port=&vault=` opens an SSH session.
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import './base.css';
import './term.css';
import { send } from './bridge';

const b64 = {
  enc: (s: string): string => {
    const bytes = new TextEncoder().encode(s);
    let bin = '';
    for (const b of bytes) bin += String.fromCharCode(b);
    return btoa(bin);
  },
  dec: (s: string): Uint8Array => Uint8Array.from(atob(s), (c) => c.charCodeAt(0)),
};

const root = document.getElementById('root')!;
root.className = 'term-root';
const holder = document.createElement('div');
holder.className = 'term-holder';
root.append(holder);

const term = new Terminal({
  cursorBlink: true,
  fontFamily: 'ui-monospace, Cascadia Mono, Consolas, monospace',
  fontSize: 14,
  scrollback: 5000,
  theme: matchMedia('(prefers-color-scheme: dark)').matches
    ? { background: '#1e1f23', foreground: '#e8e8ec' }
    : { background: '#ffffff', foreground: '#1d1d22', cursor: '#1d1d22', selectionBackground: '#b4d5fe' },
});
const fit = new FitAddon();
term.loadAddon(fit);
term.open(holder);
fit.fit();
term.focus();

let sessionId = 0;
const decoder = new TextDecoder();
const NL = '\r\n';

interface SshParams {
  host: string;
  port: number;
  user: string;
  vault_id?: string;
  password?: string;
  trust_new_host?: boolean;
  telnet?: boolean;
}

const params = new URLSearchParams(location.search);
const target: SshParams | null = params.get('host')
  ? {
      host: params.get('host')!,
      port: Number(params.get('port') ?? 22) || 22,
      user: params.get('user') ?? '',
      vault_id: params.get('vault') ?? undefined,
      telnet: params.get('telnet') === '1',
    }
  : null;

function overlay(tag: 'div' | 'form', ...kids: HTMLElement[]): HTMLElement {
  const box = document.createElement(tag);
  box.className = 'term-overlay';
  box.append(...kids);
  root.append(box);
  return box;
}

function button(label: string): HTMLButtonElement {
  const b = document.createElement('button');
  b.textContent = label;
  return b;
}

function ask(label: string, yes: () => void): void {
  const ok = button(label);
  const no = button('Cancel');
  const box = overlay('div', ok, no);
  ok.onclick = () => {
    box.remove();
    yes();
  };
  no.onclick = () => box.remove();
}

function connect(ssh?: SshParams): void {
  if (!window.kbQuery) {
    term.write(`Terminal bridge unavailable.${NL}`);
    return;
  }
  window.kbQuery({
    request: JSON.stringify({ cmd: 'term-open', cols: term.cols, rows: term.rows, ssh }),
    persistent: true,
    onSuccess: (msg) => {
      if (msg.startsWith('d:')) term.write(decoder.decode(b64.dec(msg.slice(2)), { stream: true }));
      else if (msg.startsWith('x:')) term.write(`${NL}\x1b[90m[session ended: ${msg.slice(2)}]\x1b[0m${NL}`);
      else sessionId = (JSON.parse(msg) as { id: number }).id;
    },
    onFailure: (code, message) => {
      if (ssh && message.startsWith('unknown-host:')) {
        const fp = message.slice('unknown-host:'.length);
        term.write(`${NL}The authenticity of ${ssh.host} cannot be verified.${NL}Key fingerprint: ${fp}${NL}`);
        ask('Trust this host and connect', () => connect({ ...ssh, trust_new_host: true }));
      } else if (message.startsWith('host-key-changed:')) {
        term.write(`${NL}\x1b[31mWARNING: the host key changed (${message.slice(17)}). Connection refused.\x1b[0m${NL}`);
      } else {
        term.write(`${NL}\x1b[31m${ssh ? 'ssh' : 'terminal'} failed (${code}): ${message}\x1b[0m${NL}`);
      }
    },
  });
}

if (target?.telnet) {
  term.write(`\x1b[33mTelnet is unencrypted: everything you type, including passwords, is visible on the network.\x1b[0m${NL}`);
  connect(target);
} else if (target && !target.vault_id) {
  const pw = document.createElement('input');
  pw.type = 'password';
  pw.placeholder = `Password for ${target.user || '?'}@${target.host}`;
  const form = overlay('form', pw, button('Connect'));
  form.onsubmit = (ev) => {
    ev.preventDefault();
    const password = pw.value;
    form.remove();
    connect({ ...target, password });
    term.focus();
  };
  pw.focus();
} else {
  connect(target ?? undefined);
}

// Ctrl+Shift+B toggles broadcast: keystrokes go to every open terminal tab.
let broadcast = false;
const badge = document.createElement('div');
badge.className = 'term-overlay';
badge.textContent = 'BROADCAST: input goes to ALL terminals (Ctrl+Shift+B to stop)';
badge.hidden = true;
root.append(badge);
window.addEventListener('keydown', (ev) => {
  if (ev.ctrlKey && ev.shiftKey && ev.code === 'KeyB') {
    ev.preventDefault();
    broadcast = !broadcast;
    badge.hidden = !broadcast;
  }
});

// Ctrl+Shift+L starts or stops an encrypted session log (needs the vault unlocked).
let logging = false;
const logBadge = document.createElement('div');
logBadge.className = 'term-overlay';
logBadge.hidden = true;
root.append(logBadge);
window.addEventListener('keydown', (ev) => {
  if (!(ev.ctrlKey && ev.shiftKey && ev.code === 'KeyL') || !sessionId) return;
  ev.preventDefault();
  const want = !logging;
  void send({ cmd: 'session-log', op: want ? 'start' : 'stop', args: { id: sessionId, label: target?.host ?? 'local' } })
    .then(() => {
      logging = want;
      logBadge.textContent = 'LOGGING (encrypted). Ctrl+Shift+L to stop. View in kb://logs/';
      logBadge.hidden = !want;
    })
    .catch((e) => {
      logBadge.textContent = `Cannot log: ${String((e as Error).message ?? e).replace(/^\d+: /, '')}`;
      logBadge.hidden = false;
    });
});

term.onData((data) => {
  if (sessionId) void send({ cmd: 'term-write', id: sessionId, data: b64.enc(data), all: broadcast });
});

const onResize = () => {
  fit.fit();
  if (sessionId) void send({ cmd: 'term-resize', id: sessionId, cols: term.cols, rows: term.rows });
};
new ResizeObserver(onResize).observe(holder);
