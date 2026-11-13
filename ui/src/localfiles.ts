// kb://localfiles/ — dual-pane local file explorer (task 3C.6): browse, new folder, rename,
// copy to the other pane, delete (two clicks). Zip/Unzip go to the other pane (no overwrite). Drag and drop is not done yet.
import './base.css';
import './vault.css';
import { send } from './bridge';

interface Entry { name: string; dir: boolean; size: number; modified: number | null }

const root = document.getElementById('root')!;
root.className = 'vault';
const fs = (op: string, args: Record<string, unknown> = {}) => send({ cmd: 'local-fs', op, args });
const msg = (e: unknown) => String((e as Error).message ?? e).replace(/^\d+: /, '');
const human = (n: number) => (n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1048576).toFixed(1)} MB`);

function el<K extends keyof HTMLElementTagNameMap>(tag: K, props: Record<string, unknown> = {}, ...kids: (Node | string)[]): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  const { class: cls, ...rest } = props;
  if (cls) e.className = String(cls);
  Object.assign(e, rest);
  e.append(...kids);
  return e;
}

const status = el('div', { class: 'error' });
const sep = (p: string) => (p.includes('\\') ? '\\' : '/');
const join = (dir: string, name: string) => (dir.endsWith('/') || dir.endsWith('\\') ? dir + name : dir + sep(dir) + name);
const parentOf = (p: string) => {
  const s = sep(p);
  const parts = p.split(s).filter(Boolean);
  if (parts.length <= 1) return p.length > 1 && s === '\\' ? parts[0] + s : s;
  const head = parts.slice(0, -1).join(s);
  return (p.startsWith(s) ? s : '') + head + (parts.length === 2 && s === '\\' && /^[A-Za-z]:$/.test(head) ? s : '');
};

class Pane {
  path = '';
  selected: Entry | null = null;
  entries: Entry[] = [];
  readonly box = el('div', { class: 'card' });
  readonly head = el('div', { class: 'meta' });

  async go(path: string): Promise<void> {
    try {
      const r = JSON.parse(await fs('list', { path })) as { path: string; entries: Entry[] };
      this.path = r.path;
      this.selected = null;
      this.entries = r.entries;
      status.textContent = '';
      this.draw(r.entries);
    } catch (e) {
      status.textContent = msg(e);
    }
  }

  private draw(entries: Entry[]): void {
    this.head.textContent = this.path;
    const up = el('div', { class: 'row' }, el('div', {}, '↑ ..'));
    up.onclick = () => void this.go(parentOf(this.path));
    this.box.replaceChildren(up);
    for (const e of entries) {
      const row = el('div', { class: 'row' }, el('div', {}, `${e.dir ? '📁 ' : ''}${e.name}`, el('div', { class: 'meta' }, e.dir ? '' : human(e.size))));
      row.onclick = () => {
        if (e.dir) {
          void this.go(join(this.path, e.name));
          return;
        }
        this.selected = e;
        for (const r of this.box.children) (r as HTMLElement).style.background = '';
        row.style.background = 'rgba(128,128,255,.25)';
      };
      row.ondblclick = () => {
        if (e.dir) return;
        this.selected = e;
      };
      row.oncontextmenu = (ev) => {
        ev.preventDefault();
        this.selected = e;
        for (const r of this.box.children) (r as HTMLElement).style.background = '';
        row.style.background = 'rgba(128,128,255,.25)';
      };
      this.box.append(row);
    }
  }
}

const left = new Pane();
const right = new Pane();
let activePane = left;
for (const p of [left, right]) p.box.addEventListener('click', () => (activePane = p));
const other = () => (activePane === left ? right : left);

async function run(what: () => Promise<unknown>): Promise<void> {
  try {
    await what();
    status.textContent = '';
    await Promise.all([left.go(left.path), right.go(right.path)]);
  } catch (e) {
    status.textContent = msg(e);
  }
}

function button(label: string, f: () => void): HTMLButtonElement {
  const b = el('button', {}, label);
  b.onclick = f;
  return b;
}

const nameBox = el('input', { placeholder: 'name for New folder / Rename to' });
let confirmDelete = '';
const delBtn = button('Delete', () => {
  const s = activePane.selected;
  if (!s) {
    status.textContent = 'Select a file first (folders open on click).';
    return;
  }
  if (confirmDelete !== s.name) {
    confirmDelete = s.name;
    delBtn.textContent = `Really delete ${s.name}?`;
    return;
  }
  confirmDelete = '';
  delBtn.textContent = 'Delete';
  void run(() => fs('delete', { path: join(activePane.path, s.name), confirm: true }));
});

const bar = el(
  'div',
  { class: 'actions' },
  button('New folder', () => void run(() => fs('mkdir', { path: join(activePane.path, nameBox.value) }))),
  button('Rename', () => {
    const s = activePane.selected;
    if (s) void run(() => fs('rename', { from: join(activePane.path, s.name), to: join(activePane.path, nameBox.value) }));
  }),
  button('Copy → other pane', () => {
    const s = activePane.selected;
    if (s) void run(() => fs('copy', { from: join(activePane.path, s.name), to: join(other().path, s.name) }));
  }),
  button('Zip → other pane', () => {
    const s = activePane.selected;
    if (s) void run(() => fs('zip-create', { from: join(activePane.path, s.name), to: join(other().path, s.name + '.zip') }));
  }),
  button('Unzip → other pane', () => {
    const s = activePane.selected;
    if (s) void run(() => fs('zip-extract', { from: join(activePane.path, s.name), to: join(other().path, s.name.replace(/\.zip$/i, '')) }));
  }),
  button('Preview office file', () => {
    const s = activePane.selected;
    if (s && !s.dir) void run(async () => {
      const r = JSON.parse(await send({ cmd: 'pdf-tool', op: 'office-preview', args: { path: join(activePane.path, s.name) } })) as { path: string };
      await send({ cmd: 'new-tab', url: `kb://pdf/?file=${encodeURIComponent(r.path)}` });
    });
  }),
  button('Convert to PDF', () => {
    const s = activePane.selected;
    if (s && !s.dir) void run(() => send({ cmd: 'pdf-tool', op: 'office-convert', args: { path: join(activePane.path, s.name), format: 'pdf' } }));
  }),
  button('Convert folder office files → PDF', () => {
    const paths = activePane.entries.filter((e) => !e.dir && /\.(docx?|xlsx?|pptx?|odt|ods|odp|rtf)$/i.test(e.name)).map((e) => join(activePane.path, e.name));
    if (paths.length) void run(() => send({ cmd: 'pdf-tool', op: 'office-batch', args: { paths, format: 'pdf' } }));
  }),
  delBtn,
  nameBox,
);

const grid = el('div', { style: 'display:grid;grid-template-columns:1fr 1fr;gap:12px' }, el('div', {}, left.head, left.box), el('div', {}, right.head, right.box));
root.replaceChildren(el('h1', {}, 'Local files'), el('div', { class: 'meta' }, 'Click a folder to open it, a file to select it. Copy never overwrites; delete asks twice.'), bar, status, grid);
void left.go('');
void right.go('');
