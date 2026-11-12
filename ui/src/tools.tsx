// kb://tools/ — developer utilities (task 6.4): JSON, regex, hash, base64, URL, JWT. All local.
import { render } from 'solid-js/web';
import { createSignal, For, Match, Switch } from 'solid-js';
import {
  base64Decode, base64Encode, decodeJwt, digest, formatJson, minifyJson, queryJson, regexMatches, urlDecode, urlEncode,
} from './toolkit';
import './base.css';
import './tools.css';

const TABS = ['JSON', 'Regex', 'Hash', 'Base64', 'URL', 'JWT'] as const;
type Tab = (typeof TABS)[number];

/** Runs `fn`, returning its text or the error message (shown in red by the caller). */
function attempt(fn: () => string): { ok: boolean; text: string } {
  try {
    return { ok: true, text: fn() };
  } catch (e) {
    return { ok: false, text: e instanceof Error ? e.message : String(e) };
  }
}

function Output(props: { ok: boolean; text: string }) {
  return <pre class="out" classList={{ bad: !props.ok }}>{props.text}</pre>;
}

function JsonTool() {
  const [text, setText] = createSignal('{"name":"Kendime-Brow","tags":["a","b"],"n":{"x":1}}');
  const [path, setPath] = createSignal('');
  const [mode, setMode] = createSignal<'format' | 'minify' | 'query'>('format');
  const result = () =>
    attempt(() =>
      mode() === 'format'
        ? formatJson(text())
        : mode() === 'minify'
          ? minifyJson(text())
          : JSON.stringify(queryJson(text(), path()), null, 2),
    );
  return (
    <>
      <textarea value={text()} onInput={(e) => setText(e.currentTarget.value)} spellcheck={false} />
      <div class="row">
        <For each={['format', 'minify', 'query'] as const}>
          {(m) => <button classList={{ on: mode() === m }} onClick={() => setMode(m)}>{m}</button>}
        </For>
        <input
          type="text"
          placeholder="Path, e.g. n.x or tags[1]"
          value={path()}
          onInput={(e) => {
            setPath(e.currentTarget.value);
            setMode('query');
          }}
        />
      </div>
      <Output {...result()} />
    </>
  );
}

function RegexTool() {
  const [pattern, setPattern] = createSignal('(\\w+)@(\\w+)\\.com');
  const [flags, setFlags] = createSignal('g');
  const [input, setInput] = createSignal('mail ali@example.com and veli@example.org');
  const result = () =>
    attempt(() => {
      const m = regexMatches(pattern(), flags(), input());
      if (!m.length) return 'No matches';
      return m
        .map((x, i) => `#${i + 1} @${x.index}: ${JSON.stringify(x.text)}` +
          (x.groups.length ? `\n   groups: ${JSON.stringify(x.groups)}` : '') +
          (Object.keys(x.named).length ? `\n   named: ${JSON.stringify(x.named)}` : ''))
        .join('\n');
    });
  return (
    <>
      <div class="row">
        <input class="grow" type="text" value={pattern()} onInput={(e) => setPattern(e.currentTarget.value)} spellcheck={false} />
        <input class="flags" type="text" value={flags()} onInput={(e) => setFlags(e.currentTarget.value)} title="Flags: g i m s u y" />
      </div>
      <textarea value={input()} onInput={(e) => setInput(e.currentTarget.value)} spellcheck={false} />
      <Output {...result()} />
    </>
  );
}

function HashTool() {
  const [text, setText] = createSignal('Kendime-Brow');
  const [rows, setRows] = createSignal<[string, string][]>([]);
  const run = async () =>
    setRows(await Promise.all((['SHA-1', 'SHA-256', 'SHA-384', 'SHA-512'] as const).map(async (a) => [a, await digest(a, text())] as [string, string])));
  void run();
  return (
    <>
      <textarea value={text()} onInput={(e) => { setText(e.currentTarget.value); void run(); }} spellcheck={false} />
      <dl class="hashes">
        <For each={rows()}>{([a, h]) => (<><dt>{a}</dt><dd>{h}</dd></>)}</For>
      </dl>
    </>
  );
}

function ConvertTool(props: { encode: (s: string) => string; decode: (s: string) => string; extra?: () => string }) {
  const [text, setText] = createSignal('Merhaba dünya ş/ğ?');
  const [mode, setMode] = createSignal<'encode' | 'decode'>('encode');
  const result = () => attempt(() => (mode() === 'encode' ? props.encode(text()) : props.decode(text())));
  return (
    <>
      <textarea value={text()} onInput={(e) => setText(e.currentTarget.value)} spellcheck={false} />
      <div class="row">
        <For each={['encode', 'decode'] as const}>
          {(m) => <button classList={{ on: mode() === m }} onClick={() => setMode(m)}>{m}</button>}
        </For>
      </div>
      <Output {...result()} />
    </>
  );
}

function JwtTool() {
  const [token, setToken] = createSignal('');
  const result = () =>
    token().trim()
      ? attempt(() => {
          const j = decodeJwt(token());
          return JSON.stringify(j, null, 2) + '\n\n⚠ Signature is NOT verified.';
        })
      : { ok: true, text: 'Paste a JWT above.' };
  return (
    <>
      <textarea value={token()} onInput={(e) => setToken(e.currentTarget.value)} spellcheck={false} placeholder="eyJhbGciOi…" />
      <Output {...result()} />
    </>
  );
}

function Tools() {
  const [tab, setTab] = createSignal<Tab>('JSON');
  return (
    <main class="tools">
      <h1>Tools</h1>
      <nav>
        <For each={TABS}>{(t) => <button classList={{ on: tab() === t }} onClick={() => setTab(t)}>{t}</button>}</For>
      </nav>
      <section>
        <Switch>
          <Match when={tab() === 'JSON'}><JsonTool /></Match>
          <Match when={tab() === 'Regex'}><RegexTool /></Match>
          <Match when={tab() === 'Hash'}><HashTool /></Match>
          <Match when={tab() === 'Base64'}><ConvertTool encode={(s) => base64Encode(s)} decode={base64Decode} /></Match>
          <Match when={tab() === 'URL'}><ConvertTool encode={urlEncode} decode={urlDecode} /></Match>
          <Match when={tab() === 'JWT'}><JwtTool /></Match>
        </Switch>
      </section>
    </main>
  );
}

render(() => <Tools />, document.getElementById('root')!);
