// kb://settings/ — settings, bookmarks and history (task 1.12).
import { render } from 'solid-js/web';
import { createResource, createSignal, For, Show } from 'solid-js';
import { profile, send } from './bridge';
import './base.css';
import './settings.css';

const SEARCH_ENGINES: [string, string][] = [
  ['DuckDuckGo', 'https://duckduckgo.com/?q='],
  ['Startpage', 'https://www.startpage.com/do/search?q='],
  ['Brave Search', 'https://search.brave.com/search?q='],
  ['Mojeek', 'https://www.mojeek.com/search?q='],
];

function formatDate(secs: number): string {
  return new Date(secs * 1000).toLocaleString();
}

function Settings() {
  const [query, setQuery] = createSignal('');
  const [data, { refetch }] = createResource(query, (q) => profile(q));

  const setSetting = async (key: string, value: string) => {
    await send({ cmd: 'set-setting', key, value });
    await refetch();
  };
  const bool = (key: string) => data()?.settings[key] === 'true';
  const open = (url: string) => void send({ cmd: 'new-tab', url });

  return (
    <main class="settings">
      <h1>Settings</h1>

      <section>
        <h2>General</h2>
        <label class="row">
          <span>Search engine</span>
          <select
            value={data()?.settings.search_url ?? ''}
            onChange={(e) => void setSetting('search_url', e.currentTarget.value)}
          >
            <For each={SEARCH_ENGINES}>{([name, url]) => <option value={url}>{name}</option>}</For>
          </select>
        </label>
        <label class="row">
          <span>Restore tabs from the last session</span>
          <input
            type="checkbox"
            checked={bool('restore_session')}
            onChange={(e) => void setSetting('restore_session', String(e.currentTarget.checked))}
          />
        </label>
        <label class="row">
          <span>Keep browsing history</span>
          <input
            type="checkbox"
            checked={bool('history_enabled')}
            onChange={(e) => void setSetting('history_enabled', String(e.currentTarget.checked))}
          />
        </label>
      </section>

      <section>
        <h2>Bookmarks</h2>
        <Show when={(data()?.bookmarks.length ?? 0) > 0} fallback={<p class="muted">No bookmarks yet.</p>}>
          <ul class="list">
            <For each={data()?.bookmarks}>
              {(b) => (
                <li>
                  <button class="link" title={b.url} onClick={() => open(b.url)}>
                    {b.title || b.url}
                  </button>
                  <span class="muted url">{b.url}</span>
                  <button
                    class="remove"
                    title="Delete bookmark"
                    onClick={async () => {
                      await send({ cmd: 'delete-bookmark', id: b.id });
                      await refetch();
                    }}
                  >
                    ×
                  </button>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </section>

      <section>
        <h2>History</h2>
        <div class="row">
          <input
            class="search"
            type="search"
            placeholder="Search history"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
          <button
            class="danger"
            onClick={async () => {
              await send({ cmd: 'clear-history' });
              await refetch();
            }}
          >
            Clear history
          </button>
        </div>
        <Show when={(data()?.history.length ?? 0) > 0} fallback={<p class="muted">Nothing here.</p>}>
          <ul class="list">
            <For each={data()?.history}>
              {(h) => (
                <li>
                  <button class="link" title={h.url} onClick={() => open(h.url)}>
                    {h.title || h.url}
                  </button>
                  <span class="muted url">{h.url}</span>
                  <span class="muted when">{formatDate(h.lastVisit)}</span>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </section>
    </main>
  );
}

render(() => <Settings />, document.getElementById('root')!);
