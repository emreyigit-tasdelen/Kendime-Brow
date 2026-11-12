// kb://scripts/ — userscript manager (task 6.5): list, enable/disable, edit, delete.
import './base.css';
import './vault.css';
import { send } from './bridge';

interface Item { file: string; enabled: boolean; name: string; version: string; run_at: string; matches: string[] }

const root = document.getElementById('root')!;
root.className = 'vault';
const us = (op: string, args: Record<string, unknown> = {}) => send({ cmd: 'userscripts', op, args });
const msg = (e: unknown) => String((e as Error).message ?? e).replace(/^\d+: /, '');

function el<K extends keyof HTMLElementTagNameMap>(tag: K, props: Record<string, unknown> = {}, ...kids: (Node | string)[]): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  const { class: cls, ...rest } = props;
  if (cls) e.className = String(cls);
  Object.assign(e, rest);
  e.append(...kids);
  return e;
}

const TEMPLATE = `// ==UserScript==
// @name        My script
// @version     1.0
// @description What it does
// @match       https://example.com/*
// @run-at      document-end
// ==/UserScript==

GM_log('hello from', location.href);
`;

async function editor(file?: string): Promise<void> {
  const err = el('div', { class: 'error' });
  let source = TEMPLATE;
  if (file) {
    try {
      source = (JSON.parse(await us('get', { file })) as { source: string }).source;
    } catch (e) {
      err.textContent = msg(e);
    }
  }
  const area = el('textarea', { value: source, spellcheck: false, rows: 22, style: 'width:100%;font-family:monospace' });
  const save = el('button', { class: 'primary' }, 'Save');
  save.onclick = async () => {
    try {
      await us('save', { file, source: area.value });
      await list();
    } catch (e) {
      err.textContent = msg(e);
    }
  };
  const back = el('button', {}, 'Back');
  back.onclick = () => void list();
  root.replaceChildren(el('h1', {}, file ?? 'New script'), el('div', { class: 'card' }, area, err, el('div', { class: 'actions' }, save, back)));
}

async function list(): Promise<void> {
  const err = el('div', { class: 'error' });
  const card = el('div', { class: 'card' });
  const add = el('button', { class: 'primary' }, 'New script');
  add.onclick = () => void editor();
  root.replaceChildren(el('h1', {}, 'Userscripts'), el('div', { class: 'meta' }, 'Scripts run only on pages that match their @match / @include lines. GM_getValue/GM_setValue are stored per site.'), el('div', { class: 'bar' }, el('span', {}), el('div', { class: 'actions' }, add)), err, card);
  try {
    const items = JSON.parse(await us('list')) as Item[];
    if (!items.length) card.append('No scripts yet.');
    for (const i of items) {
      const toggle = el('button', {}, i.enabled ? 'Disable' : 'Enable');
      toggle.onclick = async () => {
        await us('toggle', { file: i.file });
        await list();
      };
      const edit = el('button', {}, 'Edit');
      edit.onclick = () => void editor(i.file);
      const del = el('button', {}, 'Delete');
      del.onclick = async () => {
        if (del.textContent !== 'Really?') {
          del.textContent = 'Really?';
          return;
        }
        await us('remove', { file: i.file });
        await list();
      };
      card.append(el('div', { class: 'row' }, el('div', {}, `${i.name || i.file} ${i.version}${i.enabled ? '' : ' (disabled)'}`, el('div', { class: 'meta' }, `${i.run_at} · ${i.matches.join(', ')}`)), el('div', { class: 'actions' }, toggle, edit, del)));
    }
  } catch (e) {
    err.textContent = msg(e);
  }
}

void list();
