// kb://net/ — network inspector (task 4B.1): request list from the shell's DevTools observer,
// with filter, cURL copy and HAR copy.
import './base.css';
import './vault.css';
import { send } from './bridge';

interface Row { id: string; url: string; method: string; status: number | null; mime: string; ms: number | null; bytes: number | null; error: string | null; type: string; protocol: string | null; ip: string | null }

const root = document.getElementById('root')!;
root.className = 'vault';
const net = (op: string, args: Record<string, unknown> = {}) => send({ cmd: 'net', op, args });
const msg = (e: unknown) => String((e as Error).message ?? e).replace(/^\d+: /, '');

function el<K extends keyof HTMLElementTagNameMap>(tag: K, props: Record<string, unknown> = {}, ...kids: (Node | string)[]): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  const { class: cls, ...rest } = props;
  if (cls) e.className = String(cls);
  Object.assign(e, rest);
  e.append(...kids);
  return e;
}

const human = (n: number | null) => (n === null ? '–' : n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1048576).toFixed(1)} MB`);
let filter = '';
const status = el('div', { class: 'error' });
const list = el('div', { class: 'card' });

async function copy(text: string, what: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    status.textContent = `${what} copied`;
  } catch (e) {
    status.textContent = msg(e);
  }
}

async function refresh(): Promise<void> {
  let rows: Row[];
  try {
    rows = JSON.parse(await net('list')) as Row[];
  } catch (e) {
    status.textContent = msg(e);
    return;
  }
  const f = filter.toLowerCase();
  const shown = rows.filter((r) => !f || `${r.method} ${r.url} ${r.status ?? ''} ${r.mime} ${r.type}`.toLowerCase().includes(f)).slice(-500).reverse();
  list.replaceChildren();
  if (!shown.length) list.append('No requests recorded yet. Browse in another tab, then refresh.');
  for (const r of shown) {
    const curl = el('button', {}, 'cURL');
    curl.onclick = async () => {
      try {
        await copy((JSON.parse(await net('curl', { id: r.id })) as { curl: string }).curl, 'cURL');
      } catch (e) {
        status.textContent = msg(e);
      }
    };
    const code = r.error ?? (r.status === null ? '…' : String(r.status));
    const meta = [r.type, r.mime, r.protocol, r.ip, r.ms === null ? null : `${r.ms.toFixed(0)} ms`, human(r.bytes)].filter(Boolean).join(' · ');
    list.append(el('div', { class: 'row' }, el('div', {}, `${r.method} ${code}  ${r.url.length > 110 ? r.url.slice(0, 110) + '…' : r.url}`, el('div', { class: 'meta' }, meta)), el('div', { class: 'actions' }, curl)));
  }
}

const search = el('input', { placeholder: 'Filter (method, url, status, type)…', type: 'search' });
search.oninput = () => {
  filter = search.value;
  void refresh();
};
const reload = el('button', { class: 'primary' }, 'Refresh');
reload.onclick = () => void refresh();
const har = el('button', {}, 'Copy HAR');
har.onclick = async () => {
  try {
    await copy(await net('har'), 'HAR');
  } catch (e) {
    status.textContent = msg(e);
  }
};
const clear = el('button', {}, 'Clear');
clear.onclick = async () => {
  await net('clear');
  await refresh();
};

root.replaceChildren(el('h1', {}, 'Network'), el('div', { class: 'bar' }, search, el('div', { class: 'actions' }, reload, har, clear)), status, list);
void refresh();
