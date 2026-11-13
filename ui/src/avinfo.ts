// kb://avinfo/?file=<absolute path> — audio/video info (ffprobe) and conversion/trim (ffmpeg),
// tasks 3C.1 and 3C.2.
import './base.css';
import './vault.css';
import { send } from './bridge';

const root = document.getElementById('root')!;
root.className = 'vault';
const file = new URLSearchParams(location.search).get('file') ?? '';
const media = (op: string, args: Record<string, unknown>) => send({ cmd: 'media', op, args });
const msg = (e: unknown) => String((e as Error).message ?? e).replace(/^\d+: /, '');

function el<K extends keyof HTMLElementTagNameMap>(tag: K, props: Record<string, unknown> = {}, ...kids: (Node | string)[]): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  const { class: cls, ...rest } = props;
  if (cls) e.className = String(cls);
  Object.assign(e, rest);
  e.append(...kids);
  return e;
}

interface Stream { type: string; codec: string; width?: number; height?: number; fps?: string; channels?: number; sample_rate?: string; bit_rate?: string }
interface Info { format: string; duration: string; size: string; bit_rate: string; streams: Stream[] }

const status = el('div', { class: 'error' });
const info = el('div', { class: 'card' }, 'Reading…');
const out = el('div', { class: 'meta' });

const preset = el('select', {}, ...['mp4', 'webm', 'mp3', 'm4a', 'copy'].map((p) => el('option', { value: p }, p === 'copy' ? 'copy streams (mkv, no re-encode)' : p)));
const start = el('input', { placeholder: 'trim start (s)', size: 10 });
const end = el('input', { placeholder: 'trim end (s)', size: 10 });
const go = el('button', { class: 'primary' }, 'Convert');
go.onclick = async () => {
  const num = (i: HTMLInputElement) => (i.value.trim() === '' ? null : Number(i.value));
  go.disabled = true;
  out.textContent = 'Converting… this can take a while.';
  status.textContent = '';
  try {
    const r = JSON.parse(await media('convert', { path: file, preset: preset.value, start: num(start), end: num(end) })) as { path: string };
    out.textContent = `Saved ${r.path}`;
  } catch (e) {
    out.textContent = '';
    status.textContent = msg(e);
  } finally {
    go.disabled = false;
  }
};

root.replaceChildren(
  el('h1', {}, file.split(/[\\/]/).pop() ?? 'Media'),
  el('div', { class: 'meta' }, 'Runs FFmpeg on this computer. Open only media you trust.'),
  status,
  info,
  el('div', { class: 'card' }, el('div', { class: 'actions' }, preset, start, end, go), out),
);

void (async () => {
  try {
    const i = JSON.parse(await media('probe', { path: file })) as Info;
    const secs = Number(i.duration);
    const lines = [`${i.format}`, `${Number.isFinite(secs) ? secs.toFixed(1) + ' s' : ''} · ${Math.round(Number(i.size) / 1024)} KB · ${Math.round(Number(i.bit_rate) / 1000)} kb/s`];
    info.replaceChildren(
      ...lines.map((l) => el('div', {}, l)),
      ...i.streams.map((s) => el('div', { class: 'meta' }, `${s.type}: ${s.codec}${s.width ? ` ${s.width}×${s.height}` : ''}${s.fps && s.fps !== '0/0' ? ` @ ${s.fps}` : ''}${s.channels ? ` ${s.channels}ch ${s.sample_rate} Hz` : ''}`)),
    );
  } catch (e) {
    info.textContent = '';
    status.textContent = msg(e);
  }
})();
