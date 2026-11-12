// kb://office/?file=<path> — spreadsheet preview (task 3B.1): xlsx / xls / xlsb / ods / csv.
import { render } from 'solid-js/web';
import { createSignal, For, onMount, Show } from 'solid-js';
import { send } from './bridge';
import './base.css';
import './office.css';

interface Sheet {
  name: string;
  rows: string[][];
  totalRows: number;
  totalCols: number;
}

const file = new URLSearchParams(location.search).get('file') ?? '';

function colName(i: number): string {
  let s = '';
  for (let n = i + 1; n > 0; n = Math.floor((n - 1) / 26)) s = String.fromCharCode(65 + ((n - 1) % 26)) + s;
  return s;
}

function Viewer() {
  const [sheets, setSheets] = createSignal<Sheet[]>([]);
  const [active, setActive] = createSignal(0);
  const [error, setError] = createSignal('');

  onMount(async () => {
    try {
      setSheets((JSON.parse(await send({ cmd: 'sheet-preview', path: file })) as { sheets: Sheet[] }).sheets);
    } catch (e) {
      setError(String(e));
    }
  });

  const sheet = () => sheets()[active()];
  const cols = () => Math.max(0, ...(sheet()?.rows.map((r) => r.length) ?? [0]));

  return (
    <div class="sheet-viewer">
      <header>
        <span class="title" title={file}>{file.split(/[\\/]/).pop()}</span>
        <Show when={sheet()}>
          <span class="muted">
            {sheet()!.totalRows.toLocaleString()} rows × {sheet()!.totalCols} columns
            {sheet()!.rows.length < sheet()!.totalRows ? ` (showing first ${sheet()!.rows.length})` : ''}
          </span>
        </Show>
      </header>
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>
      <main>
        <Show when={sheet()}>
          <table>
            <thead>
              <tr>
                <th />
                <For each={Array.from({ length: cols() }, (_, i) => i)}>{(i) => <th>{colName(i)}</th>}</For>
              </tr>
            </thead>
            <tbody>
              <For each={sheet()!.rows}>
                {(row, r) => (
                  <tr>
                    <th>{r() + 1}</th>
                    <For each={Array.from({ length: cols() }, (_, i) => row[i] ?? '')}>{(c) => <td>{c}</td>}</For>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </Show>
      </main>
      <Show when={sheets().length > 1}>
        <nav>
          <For each={sheets()}>
            {(s, i) => (
              <button classList={{ on: i() === active() }} onClick={() => setActive(i())}>
                {s.name}
              </button>
            )}
          </For>
        </nav>
      </Show>
    </div>
  );
}

render(() => <Viewer />, document.getElementById('root')!);
