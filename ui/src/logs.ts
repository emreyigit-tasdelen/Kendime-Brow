// kb://logs/ — encrypted terminal session logs (task 4A.9). Reading needs the vault unlocked.
import './base.css';
import './vault.css';
import { send } from './bridge';

const root = document.getElementById('root')!;
root.className = 'vault';
const logs = (op: string, args: Record<string, unknown> = {}) => send({ cmd: 'session-log', op, args });
const msg = (e: unknown) => String((e as Error).message ?? e).replace(/^\d+: /, '');

function el<K extends keyof HTMLElementTagNameMap>(tag: K, props: Record<string, unknown> = {}, ...kids: (Node | string)[]): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  const { class: cls, ...rest } = props;
  if (cls) e.className = String(cls);
  Object.assign(e, rest);
  e.append(...kids);
  return e;
}

// eslint-disable-next-line no-control-regex
const stripAnsi = (s: string) => s.replace(/\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07]*\x07/g, '');

async function view(name: string): Promise<void> {
  const err = el('div', { class: 'error' });
  const back = el('button', {}, 'Back');
  back.onclick = () => void list();
  const pre = el('pre', { style: 'white-space:pre-wrap;word-break:break-word' });
  root.replaceChildren(el('h1', {}, name), el('div', { class: 'bar' }, el('span', {}), el('div', { class: 'actions' }, back)), err, el('div', { class: 'card' }, pre));
  try {
    const entries = JSON.parse(await logs('read', { name })) as { d: 'in' | 'out'; t: string }[];
    pre.textContent = entries.map((e) => (e.d === 'in' ? `» ${e.t}\n` : stripAnsi(e.t))).join('');
  } catch (e) {
    err.textContent = msg(e);
  }
}

async function list(): Promise<void> {
  const err = el('div', { class: 'error' });
  const card = el('div', { class: 'card' });
  root.replaceChildren(el('h1', {}, 'Session logs'), el('div', { class: 'meta' }, 'Start or stop a log in a terminal tab with Ctrl+Shift+L. Logs are encrypted with a key kept in your vault.'), err, card);
  try {
    const items = JSON.parse(await logs('list')) as { name: string; bytes: number }[];
    if (!items.length) card.append('No logs yet.');
    for (const i of items) {
      const open = el('button', {}, 'Open');
      open.onclick = () => void view(i.name);
      card.append(el('div', { class: 'row' }, el('div', {}, i.name, el('div', { class: 'meta' }, `${(i.bytes / 1024).toFixed(1)} KB`)), el('div', { class: 'actions' }, open)));
    }
  } catch (e) {
    err.textContent = msg(e);
  }
}

void list();
