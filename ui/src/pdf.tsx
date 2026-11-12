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
