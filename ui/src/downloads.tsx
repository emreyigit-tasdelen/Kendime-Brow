// kb://downloads/ — download list with progress (task 1.13).
import { render } from 'solid-js/web';
import { createSignal, For, onCleanup, Show } from 'solid-js';
import { send, subscribe, type DownloadState, type State } from './bridge';
import './base.css';
import './settings.css';
import './downloads.css';

function size(bytes: number): string {
  if (bytes < 0) return '?';
  const units = ['B', 'KB', 'MB', 'GB'];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

function detail(d: DownloadState): string {
  switch (d.status) {
    case 'in-progress':
      return `${size(d.received)} of ${d.total > 0 ? size(d.total) : '?'} — ${size(d.speed)}/s`;
    case 'paused':
      return `Paused — ${size(d.received)}`;
    case 'complete':
      return size(d.total > 0 ? d.total : d.received);
    case 'canceled':
      return 'Canceled';
    default:
      return 'Failed';
  }
}

function Downloads() {
  const [state, setState] = createSignal<State>({ tabs: [], palette: false, downloads: [] });
  onCleanup(subscribe(setState));
  const list = () => [...state().downloads].reverse();
  const live = (d: DownloadState) => d.status === 'in-progress' || d.status === 'paused';

  return (
    <main class="settings">
      <div class="row">
        <h1>Downloads</h1>
        <button class="plain" onClick={() => void send({ cmd: 'clear-downloads' })}>
          Clear finished
        </button>
      </div>
      <Show when={list().length > 0} fallback={<p class="muted">No downloads in this session.</p>}>
        <ul class="list downloads">
          <For each={list()}>
            {(d) => (
              <li>
                <div class="info">
                  <span class="name">{d.fileName}</span>
                  <span class="muted url" title={d.url}>
                    {d.url}
                  </span>
                  <Show when={d.status === 'complete'}>
                    <span class="muted url" title="SHA-256">
                      {d.sha256 ? `SHA-256 ${d.sha256}` : 'SHA-256 …'}
                    </span>
                  </Show>
                  <Show when={live(d)}>
                    <progress max="100" value={d.percent >= 0 ? d.percent : undefined} />
                  </Show>
                  <span class="muted">{detail(d)}</span>
                </div>
                <div class="actions">
                  <Show when={d.status === 'in-progress'}>
                    <button class="plain" onClick={() => void send({ cmd: 'pause-download', id: d.id })}>
                      Pause
                    </button>
                  </Show>
                  <Show when={d.status === 'paused'}>
                    <button class="plain" onClick={() => void send({ cmd: 'resume-download', id: d.id })}>
                      Resume
                    </button>
                  </Show>
                  <Show when={live(d)}>
                    <button class="plain" onClick={() => void send({ cmd: 'cancel-download', id: d.id })}>
                      Cancel
                    </button>
                  </Show>
                  <Show when={d.status === 'complete'}>
                    <button class="plain" onClick={() => void send({ cmd: 'reveal-download', id: d.id })}>
                      Show in folder
                    </button>
                  </Show>
                </div>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </main>
  );
}

render(() => <Downloads />, document.getElementById('root')!);
