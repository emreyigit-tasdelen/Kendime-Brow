// kb://vault/ — password vault UI (task 4A.1b). All crypto happens in the shell (kb-vault).
import './base.css';
import './vault.css';
import { send } from './bridge';

interface Status { exists: boolean; unlocked: boolean; count: number }
interface Item { id: string; kind: string; title: string }
interface Entry extends Item { username: string; password: string; url: string; notes: string }

const root = document.getElementById('root')!;
root.className = 'vault';

const call = (op: string, args: Record<string, string> = {}): Promise<string> =>
  send({ cmd: 'vault', op, args });

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Record<string, unknown> = {},
  ...kids: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  const { class: cls, ...rest } = props;
  if (cls) e.className = String(cls);
  Object.assign(e, rest);
  e.append(...kids);
  return e;
}

const field = (label: string, input: HTMLElement) => el('label', {}, label, input);
const msg = (e: unknown) => String((e as Error).message ?? e).replace(/^\d+: /, '');

async function render(): Promise<void> {
  const st = JSON.parse(await call('status')) as Status;
  root.replaceChildren(el('h1', {}, 'Vault'));
  if (!st.exists) return setup();
  if (!st.unlocked) return unlock();
  await list();
}

function authForm(title: string, hint: string, submit: string, confirm: boolean, run: (pw: string) => Promise<void>): void {
  const pw = el('input', { type: 'password', autocomplete: 'off' });
  const pw2 = el('input', { type: 'password', autocomplete: 'off' });
  const err = el('div', { class: 'error' });
  const form = el(
    'form',
    {},
    field('Master password', pw),
    ...(confirm ? [field('Repeat password', pw2)] : []),
    err,
    el('button', { class: 'primary', type: 'submit' }, submit),
  );
  form.onsubmit = async (ev) => {
    ev.preventDefault();
    if (confirm && pw.value !== pw2.value) {
      err.textContent = 'Passwords do not match';
      return;
    }
    err.textContent = 'Working…';
    try {
      await run(pw.value);
    } catch (e) {
      err.textContent = msg(e);
    }
  };
  root.append(el('div', { class: 'card' }, el('div', {}, title), el('p', { class: 'meta' }, hint), form));
  pw.focus();
}

function setup(): void {
  authForm('Create your vault', 'The master password cannot be recovered. Nothing leaves this device.', 'Create vault', true, async (pw) => {
    await call('create', { password: pw });
    await render();
  });
}

function unlock(): void {
  authForm('Vault is locked', 'It locks itself after 5 minutes of inactivity.', 'Unlock', false, async (pw) => {
    await call('unlock', { password: pw });
    await render();
  });
}

async function list(): Promise<void> {
  const items = JSON.parse(await call('list')) as Item[];
  const lock = el('button', {}, 'Lock');
  lock.onclick = async () => {
    await call('lock');
    await render();
  };
  const add = el('button', { class: 'primary' }, 'Add entry');
  add.onclick = () => editor();
  const note = el('div', { class: 'error' });
  const pick = el('input', { type: 'file', accept: '.csv,text/csv', hidden: true });
  pick.onchange = async () => {
    const f = pick.files?.[0];
    if (!f) return;
    try {
      const r = JSON.parse(await call('import-csv', { text: await f.text() })) as { imported: number };
      note.textContent = `Imported ${r.imported} login(s). Delete the CSV file: it holds your passwords in plain text.`;
      await render();
      root.prepend(note);
    } catch (e) {
      note.textContent = String((e as Error).message ?? e).replace(/^\d+: /, '');
    }
  };
  const kdbxPw = el('input', { type: 'password', placeholder: 'KeePass master password' });
  const pickKdbx = el('input', { type: 'file', accept: '.kdbx', hidden: true });
  pickKdbx.onchange = async () => {
    const f = pickKdbx.files?.[0];
    if (!f) return;
    const password = kdbxPw.value;
    try {
      const bytes = new Uint8Array(await f.arrayBuffer());
      let bin = '';
      for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
      const r = JSON.parse(await call('import-kdbx', { data: btoa(bin), password })) as { imported: number };
      note.textContent = `Imported ${r.imported} login(s) from KeePass.`;
      await render();
      root.prepend(note);
    } catch (e) {
      note.textContent = String((e as Error).message ?? e).replace(/^\d+: /, '');
    }
  };
  const impK = el('button', {}, 'Import KeePass');
  impK.onclick = () => pickKdbx.click();
  const imp = el('button', {}, 'Import CSV');
  imp.onclick = () => pick.click();
  const exp = el('button', {}, 'Export CSV');
  exp.onclick = async () => {
    if (exp.textContent !== 'Copy passwords as plain text?') {
      exp.textContent = 'Copy passwords as plain text?';
      return;
    }
    exp.textContent = 'Export CSV';
    try {
      const r = JSON.parse(await call('export-csv')) as { csv: string; count: number };
      await navigator.clipboard.writeText(r.csv);
      note.textContent = `${r.count} login(s) copied to the clipboard as plain text. Paste, then clear your clipboard.`;
    } catch (e) {
      note.textContent = String((e as Error).message ?? e).replace(/^\d+: /, '');
    }
  };
  root.append(el('div', { class: 'bar' }, el('span', {}, `${items.length} entries`), el('div', { class: 'actions' }, add, imp, kdbxPw, impK, exp, lock, pick, pickKdbx)), note);
  const card = el('div', { class: 'card' });
  if (!items.length) card.append('No entries yet.');
  for (const it of items) {
    const open = el('button', {}, 'Open');
    open.onclick = async () => editor(JSON.parse(await call('get', { id: it.id })) as Entry);
    card.append(el('div', { class: 'row' }, el('div', {}, it.title, el('div', { class: 'meta' }, it.kind)), el('div', { class: 'actions' }, open)));
  }
  root.append(card);
}

function editor(e?: Entry): void {
  root.replaceChildren(el('h1', {}, e ? 'Edit entry' : 'New entry'));
  const kind = el('select', {}, ...['login', 'ssh-key', 'note', 'token'].map((k) => el('option', { value: k, selected: k === (e?.kind ?? 'login') }, k)));
  const title = el('input', { value: e?.title ?? '' });
  const user = el('input', { value: e?.username ?? '', autocomplete: 'off' });
  const pass = el('input', { type: 'password', value: e?.password ?? '', autocomplete: 'off' });
  const url = el('input', { value: e?.url ?? '' });
  const notes = el('textarea', { rows: 4, value: e?.notes ?? '' });
  const err = el('div', { class: 'error' });
  const show = el('button', { type: 'button' }, 'Show');
  show.onclick = () => {
    pass.type = pass.type === 'password' ? 'text' : 'password';
  };
  const copy = el('button', { type: 'button' }, 'Copy password');
  copy.onclick = async () => {
    await navigator.clipboard.writeText(pass.value);
    err.textContent = 'Copied; clipboard is cleared in 30 s';
    setTimeout(() => void navigator.clipboard.writeText('').catch(() => {}), 30_000);
  };
  const gen = el('button', { type: 'button' }, 'Generate');
  gen.onclick = () => {
    const chars = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!@#$%^&*-_';
    const buf = crypto.getRandomValues(new Uint32Array(24));
    pass.value = Array.from(buf, (n) => chars[n % chars.length]).join('');
    pass.type = 'text';
  };
  const save = el('button', { class: 'primary', type: 'submit' }, 'Save');
  const back = el('button', { type: 'button' }, 'Back');
  back.onclick = () => void render();
  const form = el(
    'form',
    {},
    field('Type', kind),
    field('Title', title),
    field('Username', user),
    field('Password / secret', pass),
    el('div', { class: 'actions' }, show, copy, gen),
    field('URL', url),
    field('Notes', notes),
    err,
    el('div', { class: 'actions' }, save, back),
  );
  form.onsubmit = async (ev) => {
    ev.preventDefault();
    try {
      await call('put', {
        id: e?.id ?? '',
        kind: kind.value,
        title: title.value,
        username: user.value,
        password: pass.value,
        url: url.value,
        notes: notes.value,
      });
      await render();
    } catch (x) {
      err.textContent = msg(x);
    }
  };
  root.append(el('div', { class: 'card' }, form));
  if (e) {
    const del = el('button', {}, 'Delete entry');
    del.onclick = async () => {
      if (del.textContent !== 'Really delete?') {
        del.textContent = 'Really delete?';
        return;
      }
      await call('remove', { id: e.id });
      await render();
    };
    root.append(del);
  }
}

void render().catch((e) => root.append(el('div', { class: 'error' }, msg(e))));
