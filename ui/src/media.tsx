// kb://media/?file=<path> — image viewer with metadata (task 3C.4): shows EXIF grouped by
// privacy relevance (GPS / device / time) and removes it from JPEGs losslessly.
import { render } from 'solid-js/web';
import { createSignal, For, onMount, Show } from 'solid-js';
import { send } from './bridge';
import './base.css';
import './media.css';

interface Entry {
  tag: string;
  value: string;
  group: 'gps' | 'device' | 'time' | 'other';
}
interface Meta {
  entries: Entry[];
  has_gps: boolean;
}

const file = new URLSearchParams(location.search).get('file') ?? '';
const GROUPS: [Entry['group'], string][] = [
  ['gps', 'Location'],
  ['device', 'Device and author'],
  ['time', 'Date and time'],
  ['other', 'Other'],
];

function Viewer() {
  const [src, setSrc] = createSignal('');
  const [meta, setMeta] = createSignal<Meta | null>(null);
  const [message, setMessage] = createSignal<{ text: string; path?: string }>({ text: '' });

  onMount(async () => {
    try {
      setMeta(JSON.parse(await send({ cmd: 'image-meta', path: file })) as Meta);
    } catch (e) {
      setMessage({ text: String(e) });
    }
    try {
      setSrc(await send({ cmd: 'image-data', path: file }));
    } catch {
      /* formats the viewer cannot display still show their metadata */
    }
  });

  const strip = async () => {
    try {
      const { path } = JSON.parse(await send({ cmd: 'image-strip', path: file })) as { path: string };
      setMessage({ text: 'Saved a copy without metadata:', path });
    } catch (e) {
      setMessage({ text: String(e) });
    }
  };

  const convert = async (format: string) => {
    try {
      const { path } = JSON.parse(await send({ cmd: 'image-convert', path: file, format })) as { path: string };
      setMessage({ text: `Saved a ${format.toUpperCase()} copy:`, path });
    } catch (e) {
      setMessage({ text: String(e) });
    }
  };

  const edit = async (ops: object[], label: string) => {
    const format = /\.(png|webp)$/i.test(file) ? file.slice(file.lastIndexOf('.') + 1).toLowerCase() : 'png';
    try {
      const { path } = JSON.parse(await send({ cmd: 'image-edit', path: file, ops: JSON.stringify(ops), format })) as { path: string };
      setMessage({ text: `Saved an edited copy (${label}):`, path });
    } catch (e) {
      setMessage({ text: String(e) });
    }
  };

  const [size, setSize] = createSignal('');
  const [crop, setCrop] = createSignal('');
  const nums = (t: string) => t.split(/[x, ]+/).filter(Boolean).map(Number);

  const isJpeg = /\.jpe?g$/i.test(file);
  return (
    <div class="media">
      <header>
        <span class="title" title={file}>{file.split(/[\\/]/).pop()}</span>
        <span class="spacer" />
        <For each={['png', 'jpg', 'webp']}>{(f) => <button onClick={() => void convert(f)}>Save as {f.toUpperCase()}</button>}</For>
        <button onClick={() => void edit([{ op: 'rotate', degrees: 90 }], 'rotate 90°')}>Rotate 90°</button>
        <button onClick={() => void edit([{ op: 'flip-h' }], 'mirror')}>Mirror</button>
        <button onClick={() => void edit([{ op: 'grayscale' }], 'grayscale')}>Grayscale</button>
        <input placeholder="resize WxH" size={9} value={size()} onInput={(e) => setSize(e.currentTarget.value)} />
        <button disabled={nums(size()).length !== 2} onClick={() => { const [width, height] = nums(size()); void edit([{ op: 'resize', width, height }], `resize ${width}×${height}`); }}>Resize</button>
        <input placeholder="crop x,y,w,h" size={12} value={crop()} onInput={(e) => setCrop(e.currentTarget.value)} />
        <button disabled={nums(crop()).length !== 4} onClick={() => { const [x, y, w, h] = nums(crop()); void edit([{ op: 'crop', x, y, w, h }], 'crop'); }}>Crop</button>
        <Show when={meta() && meta()!.entries.length > 0 && isJpeg}>
          <button onClick={() => void strip()}>Remove metadata (save copy)</button>
        </Show>
      </header>
      <Show when={message().text}>
        <p class="message">
          {message().text} <Show when={message().path}><code>{message().path}</code></Show>
        </p>
      </Show>
      <div class="body">
        <div class="picture">
          <Show when={src()} fallback={<span class="muted">No preview</span>}>
            <img src={src()} alt="" />
          </Show>
        </div>
        <aside>
          <Show when={meta()?.has_gps}>
            <p class="warn">This image contains GPS location data.</p>
          </Show>
          <Show when={meta()} fallback={<span class="muted">Reading metadata…</span>}>
            <Show when={meta()!.entries.length > 0} fallback={<span class="muted">No EXIF metadata.</span>}>
              <For each={GROUPS}>
                {([group, title]) => (
                  <Show when={meta()!.entries.some((e) => e.group === group)}>
                    <h3>{title}</h3>
                    <dl>
                      <For each={meta()!.entries.filter((e) => e.group === group)}>
                        {(e) => (
                          <>
                            <dt>{e.tag}</dt>
                            <dd>{e.value}</dd>
                          </>
                        )}
                      </For>
                    </dl>
                  </Show>
                )}
              </For>
            </Show>
          </Show>
        </aside>
      </div>
    </div>
  );
}

render(() => <Viewer />, document.getElementById('root')!);
