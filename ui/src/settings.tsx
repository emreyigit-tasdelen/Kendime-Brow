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

interface UpdateStatus {
  enabled: boolean;
  current: string;
  phase: string;
  version: string;
  error: string;
  canApply: boolean;
}

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
  const [upd, { mutate: setUpd }] = createResource(async () => {
    try {
      return JSON.parse(await send({ cmd: 'app-update', op: 'status' })) as UpdateStatus;
    } catch {
      return undefined;
    }
  });
  const updOp = async (op: string) => {
    try {
      setUpd(JSON.parse(await send({ cmd: 'app-update', op })) as UpdateStatus);
    } catch (e) {
      setUpd((u) => (u ? { ...u, phase: 'error', error: String(e) } : u));
    }
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
        <h2>Downloads</h2>
        <label class="row">
          <span>Download folder (empty = system Downloads)</span>
          <input
            class="search proxy"
            type="text"
            spellcheck={false}
            placeholder="C:\Users\you\Downloads"
            value={data()?.settings.download_dir ?? ''}
            onChange={(e) => void setSetting('download_dir', e.currentTarget.value.trim())}
          />
        </label>
        <label class="row">
          <span>Ask where to save each file before downloading</span>
          <input
            type="checkbox"
            checked={bool('download_ask')}
            onChange={(e) => void setSetting('download_ask', String(e.currentTarget.checked))}
          />
        </label>
        <label class="row">
          <span>Sort downloads into subfolders by type (Images, Documents, Video…)</span>
          <input
            type="checkbox"
            checked={bool('download_sort')}
            onChange={(e) => void setSetting('download_sort', String(e.currentTarget.checked))}
          />
        </label>
        <label class="row">
          <span>Open safe files automatically when the download finishes</span>
          <input
            type="checkbox"
            checked={bool('download_open_done')}
            onChange={(e) => void setSetting('download_open_done', String(e.currentTarget.checked))}
          />
        </label>
        <label class="row">
          <span>Programs and scripts (.exe, .msi, .bat, invoice.pdf.exe…)</span>
          <select
            value={data()?.settings.download_dangerous ?? 'ask'}
            onChange={(e) => void setSetting('download_dangerous', e.currentTarget.value)}
          >
            <option value="ask">Ask where to save (never opened automatically)</option>
            <option value="block">Block</option>
            <option value="allow">Allow</option>
          </select>
        </label>
        <label class="row">
          <span>Block downloads over insecure http://</span>
          <input
            type="checkbox"
            checked={bool('download_block_http')}
            onChange={(e) => void setSetting('download_block_http', String(e.currentTarget.checked))}
          />
        </label>
        <label class="row">
          <span>Mark files as downloaded from the internet (SmartScreen / Protected View)</span>
          <input
            type="checkbox"
            checked={data()?.settings.download_motw !== 'false'}
            onChange={(e) => void setSetting('download_motw', String(e.currentTarget.checked))}
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
        <h2>Updates</h2>
        <label class="row">
          <span>Check for updates at startup</span>
          <input
            type="checkbox"
            checked={bool('updates_auto_check')}
            onChange={(e) => void setSetting('updates_auto_check', String(e.currentTarget.checked))}
          />
        </label>
        <div class="row">
          <span class="muted">
            {!upd()?.enabled
              ? `Version ${upd()?.current ?? ''} · updates are not enabled in this build`
              : upd()?.phase === 'available'
                ? `Version ${upd()?.version} is available (current ${upd()?.current})`
                : upd()?.phase === 'staged'
                  ? `Version ${upd()?.version} is verified and ready`
                  : upd()?.phase === 'applying'
                    ? 'Close the browser to finish the update'
                    : upd()?.phase === 'error'
                      ? `Update error: ${upd()?.error}`
                      : upd()?.phase === 'checking' || upd()?.phase === 'downloading'
                        ? 'Working…'
                        : `Version ${upd()?.current ?? ''} · up to date`}
          </span>
          <Show when={upd()?.enabled}>
            <Show
              when={upd()?.phase === 'available'}
              fallback={
                <Show
                  when={upd()?.phase === 'staged' && upd()?.canApply}
                  fallback={
                    <button class="plain" onClick={() => void updOp('check')}>
                      Check now
                    </button>
                  }
                >
                  <button class="plain" onClick={() => void updOp('apply')}>
                    Install on exit
                  </button>
                </Show>
              }
            >
              <button class="plain" onClick={() => void updOp('download')}>
                Download
              </button>
            </Show>
          </Show>
        </div>
        <p class="muted">Updates come from the project's GitHub Releases and are checksum-verified before use.</p>
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
