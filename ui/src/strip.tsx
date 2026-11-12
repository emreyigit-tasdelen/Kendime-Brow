// kb://ui/ — the window strip: tab row + toolbar with address bar (task 1.7).
import { render } from 'solid-js/web';
import { createMemo, createSignal, For, onCleanup, Show } from 'solid-js';
import { send, subscribe, type State, type TabState } from './bridge';
import { Palette } from './Palette';
import './base.css';
import './strip.css';

function tabLabel(t: TabState): string {
  if (t.title) return t.title;
  if (t.url) return t.url.replace(/^https?:\/\//, '');
  return 'New tab';
}

function Strip() {
  const [state, setState] = createSignal<State>({ tabs: [], palette: false, downloads: [] });
  const unsubscribe = subscribe(setState);
  onCleanup(unsubscribe);

  const active = createMemo(() => state().tabs.find((t) => t.active));
  const activeDownloads = createMemo(
    () => state().downloads.filter((d) => d.status === 'in-progress' || d.status === 'paused').length,
  );
  const [editing, setEditing] = createSignal(false);
  const [text, setText] = createSignal('');
  const shownUrl = () => {
    const url = active()?.url ?? '';
    return url.startsWith('kb://newtab') ? '' : url;
  };

  const navigate = (e: SubmitEvent) => {
    e.preventDefault();
    const value = text().trim();
    if (value) void send({ cmd: 'navigate', text: value });
    setEditing(false);
    (document.activeElement as HTMLElement | null)?.blur();
  };

  const onTabMouseDown = (e: MouseEvent, t: TabState) => {
    if (e.button === 1) {
      e.preventDefault();
      void send({ cmd: 'close-tab', id: t.id });
    } else if (e.button === 0 && !t.active) {
      void send({ cmd: 'activate-tab', id: t.id });
    }
  };

  return (
    <div class="strip">
      <div class="tabs" role="tablist">
        <For each={state().tabs}>
          {(t) => (
            <div
              class="tab"
              classList={{ active: t.active, loading: t.loading }}
              role="tab"
              aria-selected={t.active}
              title={t.url}
              onMouseDown={(e) => onTabMouseDown(e, t)}
            >
              <span class="spinner" aria-hidden="true" />
              <span class="label">{tabLabel(t)}</span>
              <button
                class="close"
                title="Close tab (Ctrl+W)"
                onMouseDown={(e) => e.stopPropagation()}
                onClick={() => void send({ cmd: 'close-tab', id: t.id })}
              >
                ×
              </button>
            </div>
          )}
        </For>
        <button class="new-tab" title="New tab (Ctrl+T)" onClick={() => void send({ cmd: 'new-tab' })}>
          +
        </button>
      </div>
      <form class="toolbar" onSubmit={navigate}>
        <button type="button" title="Back (Alt+←)" disabled={!active()?.canGoBack} onClick={() => void send({ cmd: 'back' })}>
          ←
        </button>
        <button
          type="button"
          title="Forward (Alt+→)"
          disabled={!active()?.canGoForward}
          onClick={() => void send({ cmd: 'forward' })}
        >
          →
        </button>
        <Show
          when={active()?.loading}
          fallback={
            <button type="button" title="Reload (F5)" onClick={() => void send({ cmd: 'reload' })}>
              ↻
            </button>
          }
        >
          <button type="button" title="Stop" onClick={() => void send({ cmd: 'stop' })}>
            ✕
          </button>
        </Show>
        <input
          class="address"
          type="text"
          spellcheck={false}
          autocomplete="off"
          placeholder="Search or enter address"
          value={editing() ? text() : shownUrl()}
          onFocus={(e) => {
            setText(shownUrl());
            setEditing(true);
            e.currentTarget.select();
          }}
          onBlur={() => setEditing(false)}
          onInput={(e) => setText(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              setEditing(false);
              e.currentTarget.blur();
            }
          }}
        />
        <button
          type="button"
          class="shield"
          classList={{ off: !active()?.shield }}
          title={
            active()?.shield
              ? `Content blocking on — ${active()?.blocked ?? 0} blocked. Click to turn off for this site.`
              : 'Content blocking is off for this site. Click to turn it on.'
          }
          disabled={!active() || !/^https?:/.test(active()?.url ?? '')}
          onClick={() => void send({ cmd: 'toggle-shield' })}
        >
          🛡
          <Show when={active()?.shield && (active()?.blocked ?? 0) > 0}>
            <span class="badge">{active()!.blocked > 99 ? '99+' : active()!.blocked}</span>
          </Show>
        </button>
        <button
          type="button"
          class="star"
          classList={{ on: !!active()?.bookmarked }}
          title={active()?.bookmarked ? 'Remove bookmark' : 'Bookmark this page'}
          disabled={!active() || shownUrl() === '' || shownUrl().startsWith('kb://')}
          onClick={() => void send({ cmd: 'toggle-bookmark' })}
        >
          {active()?.bookmarked ? '★' : '☆'}
        </button>
        <button
          type="button"
          class="downloads-button"
          title="Downloads"
          onClick={() => void send({ cmd: 'new-tab', url: 'kb://downloads/' })}
        >
          ↓
          <Show when={activeDownloads() > 0}>
            <span class="badge">{activeDownloads()}</span>
          </Show>
        </button>
        <button
          type="button"
          class="palette-button"
          title="Command palette (Ctrl+Shift+P)"
          onClick={() => void send({ cmd: 'set-palette', open: !state().palette })}
        >
          ⌘
        </button>
      </form>
      <Show when={state().palette}>
        <Palette />
      </Show>
    </div>
  );
}

render(() => <Strip />, document.getElementById('root')!);
