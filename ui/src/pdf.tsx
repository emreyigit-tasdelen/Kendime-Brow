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
  const [batchList, setBatchList] = createSignal('');
  const ocr = async () => {
    setStatus({ text: 'Running OCR (this can take a while)…' });
    try {
      const r = JSON.parse(await send({ cmd: 'pdf-tool', op: 'ocr', args: { path: file } })) as { path: string };
      setStatus({ text: `Saved ${r.path}` });
    } catch (e) {
      setStatus({ text: String(e) });
    }
  };
  const verify = async () => {
    try {
      const r = JSON.parse(await send({ cmd: 'pdf-tool', op: 'verify', args: { path: file } })) as { signatures: { ok: boolean; signer?: string; error?: string }[] };
      setStatus({ text: r.signatures.length ? r.signatures.map((x) => (x.ok ? `Valid: ${x.signer}` : `INVALID ${x.error ?? ''}`)).join('; ') + ' (certificate trust not checked)' : 'No signatures found' });
    } catch (e) {
      setStatus({ text: String(e) });
    }
  };
  const [keyPath, setKeyPath] = createSignal('');
  const [certPath, setCertPath] = createSignal('');
  const sign = async () => {
    setStatus({ text: 'Signing…' });
    try {
      const r = JSON.parse(await send({ cmd: 'pdf-tool', op: 'sign', args: { path: file, key: keyPath(), cert: certPath() } })) as { path: string };
      setStatus({ text: `Saved ${r.path}` });
    } catch (e) {
      setStatus({ text: String(e) });
    }
  };
  const [toolPw, setToolPw] = createSignal('');
  const tool = async (op: 'encrypt' | 'linearize' | 'repair' | 'compress') => {
    setStatus({ text: `${op}…` });
    try {
      const r = JSON.parse(await send({ cmd: 'pdf-tool', op, args: { path: file, password: toolPw() } })) as { path: string };
      setStatus({ text: `Saved ${r.path}` });
    } catch (e) {
      setStatus({ text: String(e) });
    }
  };
  const batch = async (op: 'rotate' | 'extract') => {
    const paths = [file, ...batchList().split('|').map((p) => p.trim()).filter(Boolean)];
    setStatus({ text: `Batch ${op} on ${paths.length} files…` });
    try {
      const r = JSON.parse(await send({ cmd: 'pdf-batch', op, paths, pages: pages(), degrees: 90 })) as { results: { error?: string }[] };
      const failed = r.results.filter((x) => x.error).length;
      setStatus({ text: `Batch ${op}: ${r.results.length - failed} done, ${failed} failed (copies saved next to the originals)` });
    } catch (e) {
      setStatus({ text: String(e) });
    }
  };
  const [texts, setTexts] = createSignal<string[] | null>(null);
  const [query, setQuery] = createSignal('');
  const hits = () => {
    const q = query().trim().toLowerCase();
    const t = texts();
    if (!q || !t) return [] as number[];
    return t.flatMap((s, i) => (s.toLowerCase().includes(q) ? [i] : []));
  };
  const [hitIndex, setHitIndex] = createSignal(0);
  const ensureText = async () => {
    if (texts()) return;
    try {
      setTexts(JSON.parse(await send({ cmd: 'pdf-text' })) as string[]);
    } catch (e) {
      setError(String(e));
    }
  };
  const goTo = (page: number) =>
    document.querySelector(`[data-page="${page + 1}"]`)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  const step = (dir: number) => {
    const h = hits();
    if (!h.length) return;
    const i = (hitIndex() + dir + h.length) % h.length;
    setHitIndex(i);
    goTo(h[i]);
  };
  const copyTables = async () => {
    try {
      const t = JSON.parse(await send({ cmd: 'pdf-tables' })) as { page: number; csv: string }[];
      if (!t.length) {
        setError('No tables found in this document.');
        return;
      }
      await navigator.clipboard?.writeText(t.map((x) => `# page ${x.page}\r\n${x.csv}`).join('\r\n'));
    } catch (e) {
      setError(String(e));
    }
  };
  const copyAll = async () => {
    await ensureText();
    await navigator.clipboard?.writeText((texts() ?? []).join('\n\n')).catch(() => undefined);
    setStatus({ text: 'Text copied' });
  };
  interface FormField { page: number; name: string; kind: string; value: string; checked: boolean | null }
  const [fields, setFields] = createSignal<FormField[]>([]);
  const [values, setValues] = createSignal<Record<string, string>>({});
  const [formOpen, setFormOpen] = createSignal(false);
  const loadForm = async () => {
    try {
      setFields(JSON.parse(await send({ cmd: 'pdf-fields' })) as FormField[]);
    } catch (e) {
      setError(String(e));
    }
    setFormOpen(true);
  };
  const fillable = () => fields().filter((f) => f.kind === 'text' || f.kind === 'checkbox');
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
        <input
          class="find"
          type="search"
          placeholder="Find in document"
          value={query()}
          onFocus={() => void ensureText()}
          onInput={(e) => {
            setQuery(e.currentTarget.value);
            setHitIndex(0);
            const h = hits();
            if (h.length) goTo(h[0]);
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') step(e.shiftKey ? -1 : 1);
          }}
        />
        <Show when={query().trim()}>
          <span class="muted">{hits().length ? `${hitIndex() + 1}/${hits().length} pages` : 'no match'}</span>
        </Show>
        <button onClick={() => void copyAll()} title="Copy all text">⧉</button>
        <button onClick={() => void copyTables()} title="Copy detected tables as CSV">▦</button>
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
          <input
            type="text"
            class="wide"
            placeholder="Batch: more PDF paths, separated by | (this file included; uses the pages box above)"
            value={batchList()}
            onInput={(e) => setBatchList(e.currentTarget.value)}
          />
          <button onClick={() => void batch('rotate')}>Batch rotate 90°</button>
          <button disabled={!pages().trim()} onClick={() => void batch('extract')}>Batch extract pages</button>
          <input type="password" placeholder="password (for Encrypt)" value={toolPw()} onInput={(e) => setToolPw(e.currentTarget.value)} />
          <button disabled={!toolPw()} onClick={() => void tool('encrypt')}>Encrypt</button>
          <button onClick={() => void tool('linearize')}>Linearize</button>
          <button onClick={() => void tool('repair')}>Repair</button>
          <button onClick={() => void tool('compress')}>Compress (lossless)</button>
          <button onClick={() => void ocr()}>OCR (English)</button>
          <input type="text" placeholder="signing key (PKCS#8 PEM path)" value={keyPath()} onInput={(e) => setKeyPath(e.currentTarget.value)} />
          <input type="text" placeholder="certificate (PEM path)" value={certPath()} onInput={(e) => setCertPath(e.currentTarget.value)} />
          <button disabled={!keyPath() || !certPath()} onClick={() => void sign()}>Sign</button>
          <button onClick={() => void verify()}>Verify signatures</button>
          <button onClick={() => void loadForm()}>Fill form</button>
          <Show when={status().text}>
            <span class="muted">{status().text}</span>
          </Show>
          <Show when={status().path}>
            <button onClick={() => openResult(status().path!)}>Open result</button>
          </Show>
        </div>
      </Show>
      <Show when={formOpen()}>
        <div class="form">
          <Show when={fillable().length > 0} fallback={<span class="muted">This PDF has no fillable fields.</span>}>
            <For each={fillable()}>
              {(f) => (
                <label>
                  <span>{f.name || '(unnamed)'}</span>
                  {f.kind === 'checkbox' ? (
                    <input
                      type="checkbox"
                      checked={f.checked ?? false}
                      onChange={(e) => setValues({ ...values(), [f.name]: String(e.currentTarget.checked) })}
                    />
                  ) : (
                    <input
                      type="text"
                      value={f.value}
                      onInput={(e) => setValues({ ...values(), [f.name]: e.currentTarget.value })}
                    />
                  )}
                </label>
              )}
            </For>
            <button
              disabled={Object.keys(values()).length === 0}
              onClick={() => void run('Fill', { cmd: 'pdf-fill', path: file, values: values() })}
            >
              Save filled copy
            </button>
          </Show>
        </div>
      </Show>
      <Show when={error()}>
        <p class="error">{error()}</p>
        <Show when={/password/i.test(error())}>
          <form
            onSubmit={(ev) => {
              ev.preventDefault();
              const pw = (ev.currentTarget.elements.namedItem('pw') as HTMLInputElement).value;
              void send({ cmd: 'pdf-unlock', path: file!, password: pw })
                .then((r) => openResult((JSON.parse(r) as { path: string }).path))
                .catch((e) => setError(String(e)));
            }}
          >
            <input name="pw" type="password" placeholder="PDF password" autocomplete="off" />
            <button type="submit">Open unlocked copy</button>
          </form>
        </Show>
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
