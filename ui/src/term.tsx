// kb://term/ — terminal (tasks 4A.2/4A.3/4A.9): xterm.js panes on a local pseudo terminal or an
// SSH session, all owned by the shell. `?host=&user=&port=&vault=` opens an SSH session in the
// first pane. Ctrl+Shift+D / Ctrl+Shift+E split right / down, Ctrl+Shift+W closes the active pane.
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

const theme = matchMedia('(prefers-color-scheme: dark)').matches
  ? { background: '#1e1f23', foreground: '#e8e8ec' }
  : { background: '#ffffff', foreground: '#1d1d22', cursor: '#1d1d22', selectionBackground: '#b4d5fe' };
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

class Pane {
  readonly el = document.createElement('div');
  readonly term = new Terminal({
    cursorBlink: true,
    fontFamily: 'ui-monospace, Cascadia Mono, Consolas, monospace',
    fontSize: 14,
    scrollback: 5000,
    theme,
  });
  readonly fit = new FitAddon();
  sessionId = 0;
  private readonly decoder = new TextDecoder();
  private readonly observer: ResizeObserver;

  constructor() {
    this.el.className = 'term-pane';
    holder.append(this.el);
    this.term.loadAddon(this.fit);
    this.term.open(this.el);
    this.fit.fit();
    this.term.onData((data) => {
      if (this.sessionId) void send({ cmd: 'term-write', id: this.sessionId, data: b64.enc(data), all: broadcast });
    });
    this.term.textarea?.addEventListener('focus', () => setActive(this));
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(this.el);
  }

  resize(): void {
    this.fit.fit();
    if (this.sessionId) void send({ cmd: 'term-resize', id: this.sessionId, cols: this.term.cols, rows: this.term.rows });
  }

  /** Starts the local shell, or an SSH/telnet session when `ssh` is given. */
  connect(ssh?: SshParams): void {
    const term = this.term;
    if (!window.kbQuery) {
      term.write(`Terminal bridge unavailable.${NL}`);
      return;
    }
    window.kbQuery({
      request: JSON.stringify({ cmd: 'term-open', cols: term.cols, rows: term.rows, ssh }),
      persistent: true,
      onSuccess: (msg) => {
        if (msg.startsWith('d:')) term.write(this.decoder.decode(b64.dec(msg.slice(2)), { stream: true }));
        else if (msg.startsWith('x:')) term.write(`${NL}\x1b[90m[session ended: ${msg.slice(2)}]\x1b[0m${NL}`);
        else this.sessionId = (JSON.parse(msg) as { id: number }).id;
      },
      onFailure: (code, message) => {
        if (ssh && message.startsWith('unknown-host:')) {
          const fp = message.slice('unknown-host:'.length);
          term.write(`${NL}The authenticity of ${ssh.host} cannot be verified.${NL}Key fingerprint: ${fp}${NL}`);
          ask('Trust this host and connect', () => this.connect({ ...ssh, trust_new_host: true }));
        } else if (message.startsWith('host-key-changed:')) {
          term.write(`${NL}\x1b[31mWARNING: the host key changed (${message.slice(17)}). Connection refused.\x1b[0m${NL}`);
        } else {
          term.write(`${NL}\x1b[31m${ssh ? 'ssh' : 'terminal'} failed (${code}): ${message}\x1b[0m${NL}`);
        }
      },
    });
  }

  close(): void {
    if (this.sessionId) void send({ cmd: 'term-close', id: this.sessionId });
    this.observer.disconnect();
    this.term.dispose();
    this.el.remove();
  }
}

const panes: Pane[] = [];
let active: Pane;

function setActive(p: Pane): void {
  active = p;
  for (const q of panes) q.el.classList.toggle('active', q === p && panes.length > 1);
}

function split(direction: 'row' | 'column'): void {
  holder.style.flexDirection = direction;
  const p = new Pane();
  panes.push(p);
  setActive(p);
  p.connect();
  p.term.focus();
  for (const q of panes) q.resize();
}

function closeActive(): void {
  if (panes.length < 2) return;
  const i = panes.indexOf(active);
  active.close();
  panes.splice(i, 1);
  setActive(panes[Math.max(0, i - 1)]);
  active.term.focus();
  for (const q of panes) q.resize();
}

const first = new Pane();
panes.push(first);
active = first;
first.term.focus();

if (target?.telnet) {
  first.term.write(`\x1b[33mTelnet is unencrypted: everything you type, including passwords, is visible on the network.\x1b[0m${NL}`);
  first.connect(target);
} else if (target && !target.vault_id) {
  const pw = document.createElement('input');
  pw.type = 'password';
  pw.placeholder = `Password for ${target.user || '?'}@${target.host}`;
  const form = overlay('form', pw, button('Connect'));
  form.onsubmit = (ev) => {
    ev.preventDefault();
    const password = pw.value;
    form.remove();
    first.connect({ ...target, password });
    first.term.focus();
  };
  pw.focus();
} else {
  first.connect(target ?? undefined);
}

// Ctrl+Shift+B toggles broadcast: keystrokes go to every open terminal session.
let broadcast = false;
const badge = document.createElement('div');
badge.className = 'term-overlay';
badge.textContent = 'BROADCAST: input goes to ALL terminals (Ctrl+Shift+B to stop)';
badge.hidden = true;
root.append(badge);
window.addEventListener('keydown', (ev) => {
  if (!(ev.ctrlKey && ev.shiftKey)) return;
  if (ev.code === 'KeyB') {
    ev.preventDefault();
    broadcast = !broadcast;
    badge.hidden = !broadcast;
  } else if (ev.code === 'KeyD' || ev.code === 'KeyE') {
    ev.preventDefault();
    split(ev.code === 'KeyD' ? 'row' : 'column');
  } else if (ev.code === 'KeyW') {
    ev.preventDefault();
    closeActive();
  }
});

// Ctrl+Shift+L starts or stops an encrypted session log of the active pane (needs the vault unlocked).
const logging = new Set<number>();
const logBadge = document.createElement('div');
logBadge.className = 'term-overlay';
logBadge.hidden = true;
root.append(logBadge);
window.addEventListener('keydown', (ev) => {
  const id = active.sessionId;
  if (!(ev.ctrlKey && ev.shiftKey && ev.code === 'KeyL') || !id) return;
  ev.preventDefault();
  const want = !logging.has(id);
  void send({ cmd: 'session-log', op: want ? 'start' : 'stop', args: { id, label: target?.host ?? 'local' } })
    .then(() => {
      if (want) logging.add(id);
      else logging.delete(id);
      logBadge.textContent = 'LOGGING (encrypted). Ctrl+Shift+L to stop. View in kb://logs/';
      logBadge.hidden = logging.size === 0;
    })
    .catch((e) => {
      logBadge.textContent = `Cannot log: ${String((e as Error).message ?? e).replace(/^\d+: /, '')}`;
      logBadge.hidden = false;
    });
});
