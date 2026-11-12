// Live check for kb://term (task 4A.2). Run kendime-brow with --remote-debugging-port=9222, then: node scripts/term-smoke.mjs
const base = 'http://127.0.0.1:9222';
const pages = async () => (await (await fetch(base + '/json')).json()).filter(t => t.type === 'page');
const cdp = (wsUrl, method, params = {}) => new Promise(res => {
  const ws = new WebSocket(wsUrl);
  ws.onopen = () => ws.send(JSON.stringify({ id: 1, method, params }));
  ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id === 1) { ws.close(); res(m.result); } };
});
const ev = (ws, expression) => cdp(ws, 'Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }).then(r => r?.result?.value);
const sleep = ms => new Promise(r => setTimeout(r, ms));
const q = (req) => `new Promise(r => window.kbQuery({request: JSON.stringify(${JSON.stringify(req)}), onSuccess: x => r(x), onFailure: (c, m) => r('FAIL ' + c + ' ' + m)}))`;
const ui = (await pages()).find(p => p.url.startsWith('kb://ui'));
console.log('new-tab:', await ev(ui.webSocketDebuggerUrl, q({ cmd: 'new-tab', url: 'kb://term/' })));
await sleep(3000);
const term = (await pages()).find(p => p.url.startsWith('kb://term'));
if (!term) { console.log('FAIL: no term page'); process.exit(1); }
const text = () => ev(term.webSocketDebuggerUrl, `document.querySelector('.xterm-rows')?.innerText ?? 'no xterm'`);
console.log('initial:', JSON.stringify((await text()).slice(0, 200)));
await cdp(term.webSocketDebuggerUrl, 'Runtime.evaluate', { expression: `document.querySelector('.xterm-helper-textarea')?.focus()` });
// xterm reads pasted text from the helper textarea; a synthetic paste works without window focus.
await ev(term.webSocketDebuggerUrl, `(() => { const ta = document.querySelector('.xterm-helper-textarea'); const dt = new DataTransfer(); dt.setData('text/plain', 'echo kb-term-ok-$((6*7))\\r'); ta.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true })); })()`);
await sleep(1500);
const out = await text();
console.log('after typing has output:', out.includes('kb-term-ok-42'));
console.log('size:', await ev(term.webSocketDebuggerUrl, `innerWidth + 'x' + innerHeight + ' rows=' + document.querySelectorAll('.xterm-rows > div').length`));
