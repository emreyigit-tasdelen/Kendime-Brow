// kb://files/ — remote file browser over SFTP (task 4A.5). `?host=&port=&user=&vault=` connects.
import './base.css';
import './vault.css';
import { send } from './bridge';

interface Entry { name: string; dir: boolean; size: number; modified: number | null }
interface Target { host: string; port: number; user: string; vault_id?: string; password?: string; trust_new_host?: boolean }

const root = document.getElementById('root')!;
root.className = 'vault';

const files = async (op: string, args: Record<string, unknown> = {}): Promise<string> => send({ cmd: 'files', op, args });
const msg = (e: unknown) => String((e as Error).message ?? e).replace(/^\d+: /, '');

function el<K extends keyof HTMLElementTagNameMap>(tag: K, props: Record<string, unknown> = {}, ...kids: (Node | string)[]): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  const { class: cls, ...rest } = props;
  if (cls) e.className = String(cls);
  Object.assign(e, rest);
  e.append(...kids);
  return e;
}

const human = (n: number) => (n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1048576).toFixed(1)} MB`);
const join = (dir: string, name: string) => (dir.endsWith('/') ? dir + name : `${dir}/${name}`);
const parent = (p: string) => p.replace(/\/[^/]*\/?$/, '') || '/';

let sid = 0;
const status = el('div', { class: 'error' });

const params = new URLSearchParams(location.search);
const base: Target = {
  host: params.get('host') ?? '',
  port: Number(params.get('port') ?? 22) || 22,
  user: params.get('user') ?? '',
  vault_id: params.get('vault') ?? undefined,
};
const startProto = params.get('proto') ?? 'sftp';

function connectForm(): void {
  const host = el('input', { value: base.host });
  const port = el('input', { value: String(base.port) });
  const user = el('input', { value: base.user });
  const proto = el('select', {}, ...['sftp', 'ftps', 'ftp'].map((p) => el('option', { value: p, selected: p === startProto }, p === 'sftp' ? 'SFTP (SSH)' : p === 'ftps' ? 'FTPS (explicit TLS)' : 'FTP (unencrypted)')));
  const pw = el('input', { type: 'password', autocomplete: 'off', placeholder: base.vault_id ? 'using vault entry' : '' });
  pw.disabled = !!base.vault_id;
  const go = el('button', { class: 'primary', type: 'submit' }, 'Connect');
  const label = (t: string, i: HTMLElement) => el('label', {}, t, i);
  const form = el('form', {}, label('Protocol', proto), label('Host', host), label('Port', port), label('User', user), label('Password', pw), status, go);
  const attempt = async (trust: boolean) => {
    status.textContent = 'Connecting…';
    const t: Target = { host: host.value, port: Number(port.value) || 22, user: user.value, vault_id: base.vault_id, password: pw.value || undefined, trust_new_host: trust };
    const extra = { proto: proto.value };
    try {
      const r = JSON.parse(await files('connect', { ...t, ...extra })) as { sid: number; home: string };
      sid = r.sid;
      status.textContent = '';
      await browse(r.home);
    } catch (e) {
      const m = msg(e);
      if (m.startsWith('unknown-host:')) {
        status.textContent = `Unknown host. Fingerprint ${m.slice(13)}. `;
        const yes = el('button', { type: 'button' }, 'Trust and connect');
        yes.onclick = () => void attempt(true);
        status.append(yes);
      } else {
        status.textContent = m;
      }
    }
  };
  form.onsubmit = (ev) => {
    ev.preventDefault();
    void attempt(false);
  };
  root.replaceChildren(el('h1', {}, 'Remote files'), el('div', { class: 'card' }, form));
}

async function browse(path: string): Promise<void> {
  const r = JSON.parse(await files('list', { sid, path })) as { path: string; entries: Entry[] };
  const up = el('button', {}, '↑ Up');
  up.onclick = () => void browse(parent(r.path));
  const folder = el('input', { placeholder: 'new folder name' });
  const mk = el('button', {}, 'New folder');
  mk.onclick = async () => {
    const name = folder.value.trim();
    if (!name) return;
    await act(() => files('mkdir', { sid, path: join(r.path, name) }), r.path);
  };
  const pick = el('input', { type: 'file', hidden: true });
  pick.onchange = async () => {
    const f = pick.files?.[0];
    if (!f) return;
    status.textContent = `Uploading ${f.name}…`;
    const buf = new Uint8Array(await f.arrayBuffer());
    let bin = '';
    for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
    await act(() => files('upload', { sid, path: join(r.path, f.name), data: btoa(bin) }), r.path);
  };
  const upload = el('button', {}, 'Upload');
  upload.onclick = () => pick.click();
  const close = el('button', {}, 'Disconnect');
  close.onclick = async () => {
    await files('close', { sid });
    sid = 0;
    connectForm();
  };
  const card = el('div', { class: 'card' });
  if (!r.entries.length) card.append('Empty folder.');
  for (const e of r.entries) {
    const name = el('a', { href: '#' }, (e.dir ? '📁 ' : '') + e.name);
    name.onclick = (ev) => {
      ev.preventDefault();
      if (e.dir) void browse(join(r.path, e.name));
    };
    const actions = el('div', { class: 'actions' });
    if (!e.dir) {
      const dl = el('button', {}, 'Download');
      dl.onclick = async () => {
        status.textContent = `Downloading ${e.name}…`;
        await act(async () => {
          const s = JSON.parse(await files('download', { sid, path: join(r.path, e.name) })) as { saved: string };
          status.textContent = `Saved to ${s.saved}`;
          return '';
        });
      };
      actions.append(dl);
    }
    const del = el('button', {}, 'Delete');
    del.onclick = async () => {
      if (del.textContent !== 'Really?') {
        del.textContent = 'Really?';
        return;
      }
      await act(() => files('remove', { sid, path: join(r.path, e.name), dir: e.dir }), r.path);
    };
    actions.append(del);
    card.append(el('div', { class: 'row' }, el('div', {}, name, el('div', { class: 'meta' }, e.dir ? 'folder' : human(e.size))), actions));
  }
  root.replaceChildren(el('h1', {}, 'Remote files'), el('div', { class: 'meta' }, r.path), el('div', { class: 'bar' }, el('span', {}), el('div', { class: 'actions' }, up, folder, mk, upload, close, pick)), status, card);
}

/** Runs an operation, reports errors, then refreshes the folder. */
async function act(fn: () => Promise<string>, refresh?: string): Promise<void> {
  try {
    await fn();
    if (refresh) {
      const keep = status.textContent;
      await browse(refresh);
      status.textContent = keep;
    }
  } catch (e) {
    status.textContent = msg(e);
  }
}

connectForm();
