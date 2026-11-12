// kb://connections/ — saved SSH connection profiles (task 4A.4). Secrets stay in the vault;
// a profile only references a vault entry.
import './base.css';
import './vault.css';
import { send } from './bridge';

interface Conn { id: string; name: string; host: string; port: number; user: string; vault_id: string }

// Port 23 profiles open as Telnet.
const isTelnet = (c: Conn) => c.port === 23;
interface VaultItem { id: string; kind: string; title: string }

const root = document.getElementById('root')!;
root.className = 'vault';

const conn = (op: string, args: Record<string, string> = {}) => send({ cmd: 'connections', op, args });
const msg = (e: unknown) => String((e as Error).message ?? e).replace(/^\d+: /, '');

function el<K extends keyof HTMLElementTagNameMap>(tag: K, props: Record<string, unknown> = {}, ...kids: (Node | string)[]): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  const { class: cls, ...rest } = props;
  if (cls) e.className = String(cls);
  Object.assign(e, rest);
  e.append(...kids);
  return e;
}

async function vaultItems(): Promise<VaultItem[]> {
  try {
    return JSON.parse(await send({ cmd: 'vault', op: 'list', args: {} })) as VaultItem[];
  } catch {
    return [];
  }
}

function openTerm(c: Conn): void {
  const q = new URLSearchParams({ host: c.host, port: String(c.port), user: c.user });
  if (c.vault_id) q.set('vault', c.vault_id);
  if (isTelnet(c)) q.set('telnet', '1');
  void send({ cmd: 'new-tab', url: `kb://term/?${q}` });
}

async function render(): Promise<void> {
  const list = JSON.parse(await conn('list')) as Conn[];
  const err = el('div', { class: 'error' });
  const add = el('button', { class: 'primary' }, 'New connection');
  add.onclick = () => void editor();
  const imp = el('button', {}, 'Import ~/.ssh/config');
  imp.onclick = async () => {
    try {
      const r = JSON.parse(await conn('import-ssh-config')) as { added: number };
      err.textContent = `Imported ${r.added} host(s)`;
      await render();
      root.prepend(err);
    } catch (e) {
      err.textContent = msg(e);
    }
  };
  const card = el('div', { class: 'card' });
  if (!list.length) card.append('No saved connections yet.');
  for (const c of list) {
    const go = el('button', { class: 'primary' }, 'Connect');
    go.onclick = () => openTerm(c);
    const files = el('button', {}, 'Files');
    files.onclick = () => {
      const q = new URLSearchParams({ host: c.host, port: String(c.port), user: c.user });
      if (c.vault_id) q.set('vault', c.vault_id);
      if (c.port === 21) q.set('proto', 'ftps');
      void send({ cmd: 'new-tab', url: `kb://files/?${q}` });
    };
    const tun = el('button', {}, 'Tunnel');
    tun.onclick = () => tunnelPanel(c);
    const edit = el('button', {}, 'Edit');
    edit.onclick = () => void editor(c);
    card.append(el('div', { class: 'row' }, el('div', {}, c.name, el('div', { class: 'meta' }, `${c.user ? c.user + '@' : ''}${c.host}:${c.port}`)), el('div', { class: 'actions' }, go, files, tun, edit)));
  }
  root.replaceChildren(el('h1', {}, 'Connections'), el('div', { class: 'bar' }, el('span', {}, `${list.length} saved`), el('div', { class: 'actions' }, add, imp)), err, card, await tunnelList());
}

async function editor(c?: Conn): Promise<void> {
  const items = await vaultItems();
  const name = el('input', { value: c?.name ?? '' });
  const host = el('input', { value: c?.host ?? '' });
  const port = el('input', { value: String(c?.port ?? 22) });
  const user = el('input', { value: c?.user ?? '' });
  const vault = el('select', {}, el('option', { value: '' }, 'Ask for a password each time'), ...items.map((i) => el('option', { value: i.id, selected: i.id === c?.vault_id }, `${i.title} (${i.kind})`)));
  const err = el('div', { class: 'error' });
  const label = (t: string, i: HTMLElement) => el('label', {}, t, i);
  const save = el('button', { class: 'primary', type: 'submit' }, 'Save');
  const back = el('button', { type: 'button' }, 'Back');
  back.onclick = () => void render();
  const form = el('form', {}, label('Name', name), label('Host', host), label('Port', port), label('User', user), label('Credentials (vault entry)', vault), err, el('div', { class: 'actions' }, save, back));
  form.onsubmit = async (ev) => {
    ev.preventDefault();
    try {
      await conn('put', { id: c?.id ?? '', name: name.value, host: host.value, port: port.value, user: user.value, vault_id: vault.value });
      await render();
    } catch (e) {
      err.textContent = msg(e);
    }
  };
  root.replaceChildren(el('h1', {}, c ? 'Edit connection' : 'New connection'), el('div', { class: 'card' }, form));
  if (c) {
    const del = el('button', {}, 'Delete');
    del.onclick = async () => {
      if (del.textContent !== 'Really delete?') {
        del.textContent = 'Really delete?';
        return;
      }
      await conn('remove', { id: c.id });
      await render();
    };
    root.append(del);
  }
}


interface ActiveTunnel { id: number; label: string; port: number }

const tun = (op: string, args: Record<string, unknown> = {}) => send({ cmd: 'tunnels', op, args });

async function tunnelList(): Promise<HTMLElement> {
  const items = JSON.parse(await tun('list')) as ActiveTunnel[];
  const card = el('div', { class: 'card' }, el('div', {}, 'Active tunnels'));
  if (!items.length) card.append(el('div', { class: 'meta' }, 'None. Use Tunnel on a connection.'));
  for (const t of items) {
    const stop = el('button', {}, 'Stop');
    stop.onclick = async () => {
      await tun('stop', { id: t.id });
      await render();
    };
    card.append(el('div', { class: 'row' }, el('div', {}, `127.0.0.1:${t.port}`, el('div', { class: 'meta' }, t.label)), el('div', { class: 'actions' }, stop)));
  }
  return card;
}

function tunnelPanel(c: Conn): void {
  const mode = el('select', {}, el('option', { value: 'local' }, 'Local forward (-L)'), el('option', { value: 'socks' }, 'SOCKS5 proxy (-D)'));
  const bind = el('input', { placeholder: 'local port (0 = any)', value: '0' });
  const rhost = el('input', { placeholder: 'remote host, e.g. 10.0.0.5' });
  const rport = el('input', { placeholder: 'remote port' });
  const pw = el('input', { type: 'password', autocomplete: 'off', placeholder: c.vault_id ? 'using vault entry' : 'password' });
  pw.disabled = !!c.vault_id;
  const trust = el('input', { type: 'checkbox' });
  const err = el('div', { class: 'error' });
  const label = (t: string, i: HTMLElement) => el('label', {}, t, i);
  const start = el('button', { class: 'primary', type: 'submit' }, 'Start tunnel');
  const back = el('button', { type: 'button' }, 'Back');
  back.onclick = () => void render();
  const form = el('form', {}, label('Type', mode), label('Local port', bind), label('Remote host (local forward)', rhost), label('Remote port (local forward)', rport), label('Password', pw), label('Trust a new host key', trust), err, el('div', { class: 'actions' }, start, back));
  form.onsubmit = async (ev) => {
    ev.preventDefault();
    err.textContent = 'Connecting…';
    try {
      await tun('start', { host: c.host, port: c.port, user: c.user, vault_id: c.vault_id || undefined, password: pw.value || undefined, trust_new_host: trust.checked, mode: mode.value, bind_port: bind.value, remote_host: rhost.value, remote_port: rport.value });
      await render();
    } catch (e) {
      const m = msg(e);
      err.textContent = m.startsWith('unknown-host:') ? `Unknown host key ${m.slice(13)}. Tick "Trust a new host key" to accept it.` : m;
    }
  };
  root.replaceChildren(el('h1', {}, `Tunnel via ${c.name}`), el('div', { class: 'card' }, form));
}

void render().catch((e) => root.append(el('div', { class: 'error' }, msg(e))));
