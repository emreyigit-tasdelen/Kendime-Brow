// kb://term/ — local terminal (task 4A.2): xterm.js on a pseudo terminal owned by the shell.
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

if (!window.kbQuery) {
  term.write('Terminal bridge unavailable.\r\n');
} else {
  window.kbQuery({
    request: JSON.stringify({ cmd: 'term-open', cols: term.cols, rows: term.rows }),
    persistent: true,
    onSuccess: (msg) => {
      if (msg.startsWith('d:')) term.write(decoder.decode(b64.dec(msg.slice(2)), { stream: true }));
      else if (msg.startsWith('x:')) term.write(`\r\n\x1b[90m[process exited with code ${msg.slice(2)}]\x1b[0m\r\n`);
      else sessionId = (JSON.parse(msg) as { id: number }).id;
    },
    onFailure: (code, message) => term.write(`\r\n\x1b[31mterminal failed (${code}): ${message}\x1b[0m\r\n`),
  });
}

term.onData((data) => {
  if (sessionId) void send({ cmd: 'term-write', id: sessionId, data: b64.enc(data) });
});

const onResize = () => {
  fit.fit();
  if (sessionId) void send({ cmd: 'term-resize', id: sessionId, cols: term.cols, rows: term.rows });
};
new ResizeObserver(onResize).observe(holder);
