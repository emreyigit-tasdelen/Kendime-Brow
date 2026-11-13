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

// Edit & resend: raw "METHOD URL / headers / blank line / body" text, sent fresh (no cookies, not logged).
async function editor(id: string): Promise<void> {
  const out = el('pre', { style: 'white-space:pre-wrap;max-height:40vh;overflow:auto' });
  const NL = String.fromCharCode(10);
  const hdrs = (h: [string, string][]) => h.map(([k, v]) => `${k}: ${v}`).join(NL);
  let raw = '';
  try {
    const g = JSON.parse(await net('get', { id })) as { method: string; url: string; headers: [string, string][]; body: string | null };
    raw = `${g.method} ${g.url}${NL}${hdrs(g.headers)}${NL}${NL}${g.body ?? ''}`;
  } catch (e) {
    status.textContent = msg(e);
    return;
  }
  const area = el('textarea', { value: raw, spellcheck: false, rows: 14, style: 'width:100%;font-family:monospace' });
  const send = el('button', { class: 'primary' }, 'Send');
  send.onclick = async () => {
    const text = area.value.split(String.fromCharCode(13)).join('');
    const cut = text.indexOf(NL + NL);
    const head = cut < 0 ? text : text.slice(0, cut);
    const body = cut < 0 ? '' : text.slice(cut + 2);
    const lines = head.split(NL);
    const [method, ...u] = (lines.shift() ?? '').trim().split(' ').filter(Boolean);
    const headers = lines.filter(Boolean).map((l) => [l.slice(0, l.indexOf(':')).trim(), l.slice(l.indexOf(':') + 1).trim()]);
    try {
      const r = JSON.parse(await net('resend', { method, url: u.join(' '), headers, body })) as { status: number; headers: [string, string][]; body: string };
      out.textContent = `${r.status}${NL}${hdrs(r.headers)}${NL}${NL}${r.body}`;
    } catch (e) {
      out.textContent = msg(e);
    }
  };
  const back = el('button', {}, 'Back');
  back.onclick = () => void refresh();
  list.replaceChildren(area, el('div', { class: 'actions' }, send, back), out);
}

async function refresh(): Promise<void> {
  let rows: Row[];
  try {
    rows = JSON.parse(await net('list', { filter })) as Row[];
    status.textContent = '';
  } catch (e) {
    status.textContent = msg(e);
    return;
  }
  const shown = rows.slice(-500).reverse();
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
    const edit = el('button', {}, 'Edit & resend');
    edit.onclick = () => void editor(r.id);
    const code = r.error ?? (r.status === null ? '…' : String(r.status));
    const meta = [r.type, r.mime, r.protocol, r.ip, r.ms === null ? null : `${r.ms.toFixed(0)} ms`, human(r.bytes)].filter(Boolean).join(' · ');
    list.append(el('div', { class: 'row' }, el('div', {}, `${r.method} ${code}  ${r.url.length > 110 ? r.url.slice(0, 110) + '…' : r.url}`, el('div', { class: 'meta' }, meta)), el('div', { class: 'actions' }, edit, curl)));
  }
}

const search = el('input', { title: 'Examples: status>=400   host:example.com   method:post   size>10k   time>500   -error   "exact text"', placeholder: 'Filter: status>=400 host:example.com -type:image', type: 'search' });
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
