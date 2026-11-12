// kb://pdf/?file=<path> — PDF viewer (task 3A.1): pages are rendered by the sandboxed kb-doc host
// and shown as images; pages are rendered lazily when they scroll into view.
import { render } from 'solid-js/web';
import { createEffect, createSignal, For, onMount, Show } from 'solid-js';
import { send } from './bridge';
import './base.css';
import './pdf.css';

interface DocInfo {
  pages: number;
  width: number;
  height: number;
}

const params = new URLSearchParams(location.search);
const file = params.get('file') ?? '';

function Page(props: { index: number; info: DocInfo; zoom: () => number }) {
  const [src, setSrc] = createSignal<string>('');
  const [error, setError] = createSignal('');
  let box: HTMLDivElement | undefined;
  let renderedAt = 0;
  const [visible, setVisible] = createSignal(false);

  const dpi = () => Math.round(96 * props.zoom());
  const draw = async () => {
    const wanted = dpi();
    if (renderedAt === wanted) return;
    renderedAt = wanted;
    try {
      const b64 = await send({ cmd: 'pdf-render', page: props.index, dpi: wanted });
      setSrc(`data:image/png;base64,${b64}`);
    } catch (e) {
      renderedAt = 0;
      setError(String(e));
    }
  };

  onMount(() => {
    const obs = new IntersectionObserver(
      (entries) => {
        for (const e of entries) setVisible(e.isIntersecting);
      },
      { rootMargin: '800px 0px' },
    );
    if (box) obs.observe(box);
  });
  // Render when the page becomes visible and again whenever the zoom (dpi) changes while visible.
  createEffect(() => {
    dpi();
    if (visible()) void draw();
  });

  const cssWidth = () => (props.info.width * (96 / 72) * props.zoom()).toFixed(0);
  const cssHeight = () => (props.info.height * (96 / 72) * props.zoom()).toFixed(0);

  return (
    <div class="page" ref={box} style={{ width: `${cssWidth()}px`, height: `${cssHeight()}px` }} data-page={props.index + 1}>
      <Show when={src()} fallback={<span class="muted">{error() || `Page ${props.index + 1}`}</span>}>
        <img src={src()} width={cssWidth()} height={cssHeight()} alt={`Page ${props.index + 1}`} onLoad={() => void 0} />
      </Show>
    </div>
  );
}

function Viewer() {
  const [info, setInfo] = createSignal<DocInfo | null>(null);
  const [error, setError] = createSignal('');
  const [zoom, setZoom] = createSignal(1);
  const [pages, setPages] = createSignal('');
  const [mergeList, setMergeList] = createSignal('');
  const [status, setStatus] = createSignal<{ text: string; path?: string }>({ text: '' });

  const run = async (label: string, command: Parameters<typeof send>[0]) => {
    setStatus({ text: `${label}…` });
    try {
      const { path } = JSON.parse(await send(command)) as { path: string };
      setStatus({ text: `${label}: saved`, path });
    } catch (e) {
      setStatus({ text: String(e) });
    }
  };
  const openResult = (path: string) =>
    void send({ cmd: 'new-tab', url: `kb://pdf/?file=${encodeURIComponent(path)}` });

  onMount(async () => {
    if (!file) return setError('No file given.');
    try {
      setInfo(JSON.parse(await send({ cmd: 'pdf-open', path: file })) as DocInfo);
    } catch (e) {
      setError(String(e));
    }
  });

  const changeZoom = (factor: number) => {
    setZoom((z) => Math.min(4, Math.max(0.25, +(z * factor).toFixed(2))));
  };

  const name = file.split(/[\\/]/).pop() ?? '';
  return (
    <div class="viewer">
      <header>
        <span class="title" title={file}>{name}</span>
        <Show when={info()}>
          <span class="muted">{info()!.pages} pages</span>
        </Show>
        <span class="spacer" />
        <button onClick={() => changeZoom(1 / 1.25)} title="Zoom out">−</button>
        <span class="zoom">{Math.round(zoom() * 100)}%</span>
        <button onClick={() => changeZoom(1.25)} title="Zoom in">+</button>
      </header>
      <Show when={info()}>
        <div class="tools">
          <button onClick={() => void run('Rotate', { cmd: 'pdf-rotate', path: file, degrees: 90 })}>Rotate all ↻</button>
          <button onClick={() => void run('Rotate', { cmd: 'pdf-rotate', path: file, degrees: 270 })}>Rotate all ↺</button>
          <input
            type="text"
            placeholder="Pages, e.g. 1-3,5,8-"
            value={pages()}
            onInput={(e) => setPages(e.currentTarget.value)}
          />
          <button disabled={!pages().trim()} onClick={() => void run('Extract', { cmd: 'pdf-extract', path: file, pages: pages() })}>
            Extract pages
          </button>
          <input
            type="text"
            class="wide"
            placeholder="Merge: other PDF paths, separated by | (this file goes first)"
            value={mergeList()}
            onInput={(e) => setMergeList(e.currentTarget.value)}
          />
          <button
            disabled={!mergeList().trim()}
            onClick={() =>
              void run('Merge', {
                cmd: 'pdf-merge',
                paths: [file, ...mergeList().split('|').map((p) => p.trim()).filter(Boolean)],
              })
            }
          >
            Merge
          </button>
          <Show when={status().text}>
            <span class="muted">{status().text}</span>
          </Show>
          <Show when={status().path}>
            <button onClick={() => openResult(status().path!)}>Open result</button>
          </Show>
        </div>
      </Show>
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>
      <main>
        <Show when={info()}>
          <For each={Array.from({ length: info()!.pages }, (_, i) => i)}>
            {(i) => <Page index={i} info={info()!} zoom={zoom} />}
          </For>
        </Show>
      </main>
    </div>
  );
}

render(() => <Viewer />, document.getElementById('root')!);
