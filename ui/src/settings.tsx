// kb://settings/ — settings, bookmarks and history (task 1.12).
import { render } from 'solid-js/web';
import { createResource, createSignal, For, Show } from 'solid-js';
import { profile, send } from './bridge';
import './base.css';
import './settings.css';
import './downloads.css';

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
          <span>Fingerprinting protection (fully applied after restart)</span>
          <select
            value={data()?.settings.fingerprint_protection ?? 'standard'}
            onChange={(e) => void setSetting('fingerprint_protection', e.currentTarget.value)}
          >
            <option value="standard">Standard</option>
            <option value="strict">Strict</option>
            <option value="off">Off</option>
          </select>
        </label>
        <label class="row">
          <span>HTTPS-Only mode (upgrade every connection)</span>
          <input
            type="checkbox"
            checked={bool('https_only')}
            onChange={(e) => void setSetting('https_only', String(e.currentTarget.checked))}
          />
        </label>
        <label class="row">
          <span>Low memory mode (software rendering, applies after restart)</span>
          <input
            type="checkbox"
            checked={bool('low_memory')}
            onChange={(e) => void setSetting('low_memory', String(e.currentTarget.checked))}
          />
        </label>
        <label class="row">
          <span>DNS-over-HTTPS</span>
          <select value={data()?.settings.doh ?? 'off'} onChange={(e) => void setSetting('doh', e.currentTarget.value)}>
            <option value="off">Off (system resolver)</option>
            <option value="quad9">Quad9</option>
            <option value="mullvad">Mullvad</option>
            <option value="cloudflare">Cloudflare</option>
          </select>
        </label>
        <For each={[['personal', 'Personal'], ['work', 'Work'], ['temporary', 'Private']]}>
          {([id, title]) => (
            <label class="row">
              <span>Proxy for {title} tabs</span>
              <input
                class="search proxy"
                type="text"
                spellcheck={false}
                placeholder="direct — or socks5://127.0.0.1:1080"
                value={data()?.settings[`proxy_${id}`] ?? ''}
                onChange={(e) => void setSetting(`proxy_${id}`, e.currentTarget.value.trim())}
              />
            </label>
          )}
        </For>
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
        <h2>Content blocking</h2>
        <label class="row">
          <span>Update filter lists automatically (daily)</span>
          <input
            type="checkbox"
            checked={bool('filters_auto_update')}
            onChange={(e) => void setSetting('filters_auto_update', String(e.currentTarget.checked))}
          />
        </label>
        <ul class="list">
          <For each={data()?.filters}>
            {(f) => (
              <li>
                <input
                  type="checkbox"
                  checked={f.enabled}
                  onChange={async (e) => {
                    await send({ cmd: 'set-filter-list', id: f.id, enabled: e.currentTarget.checked });
                    await refetch();
                  }}
                />
                <span class="name">{f.title}</span>
                <span class="muted url">
                  {f.error
                    ? `Error: ${f.error}`
                    : f.updated
                      ? `${f.rules.toLocaleString()} rules · updated ${formatDate(f.updated)}`
                      : 'Not downloaded yet'}
                </span>
              </li>
            )}
          </For>
        </ul>
        <div class="row">
          <span class="muted">Lists are downloaded from their publishers; nothing is bundled.</span>
          <button
            class="plain"
            onClick={async () => {
              await send({ cmd: 'update-filters' });
              setTimeout(() => void refetch(), 4000);
            }}
          >
            Update now
          </button>
        </div>
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
