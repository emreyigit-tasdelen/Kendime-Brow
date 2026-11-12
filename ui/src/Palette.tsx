// Command palette (task 1.11): searchable list of registered actions.
import { createResource, createSignal, For, onMount, Show } from 'solid-js';
import { actions, send } from './bridge';

export function Palette() {
  const [query, setQuery] = createSignal('');
  const [selected, setSelected] = createSignal(0);
  const [items] = createResource(query, (q) => actions(q));
  let input: HTMLInputElement | undefined;

  onMount(() => input?.focus());

  const close = () => void send({ cmd: 'set-palette', open: false });
  const run = (id: string) => void send({ cmd: 'run-action', id });

  const onKeyDown = (e: KeyboardEvent) => {
    const list = items() ?? [];
    if (e.key === 'Escape') {
      e.preventDefault();
      close();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      setSelected((i) => Math.min(i + 1, list.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setSelected((i) => Math.max(i - 1, 0));
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const item = list[selected()];
      if (item) run(item.id);
    }
  };

  return (
    <div class="palette" role="dialog" aria-label="Command palette">
      <input
        ref={input}
        class="palette-input"
        type="text"
        spellcheck={false}
        autocomplete="off"
        placeholder="Type a command…"
        value={query()}
        onInput={(e) => {
          setQuery(e.currentTarget.value);
          setSelected(0);
        }}
        onKeyDown={onKeyDown}
        onBlur={close}
      />
      <ul class="palette-list" role="listbox">
        <For each={items() ?? []}>
          {(item, i) => (
            <li
              role="option"
              aria-selected={i() === selected()}
              classList={{ selected: i() === selected() }}
              onMouseEnter={() => setSelected(i())}
              onMouseDown={(e) => {
                e.preventDefault();
                run(item.id);
              }}
            >
              <span class="title">{item.title}</span>
              <span class="id">{item.id}</span>
            </li>
          )}
        </For>
        <Show when={(items() ?? []).length === 0}>
          <li class="empty">No matching command</li>
        </Show>
      </ul>
    </div>
  );
}
