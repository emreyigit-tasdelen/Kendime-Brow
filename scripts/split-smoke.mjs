// Live check for terminal split panes (task 4A.9). Start kendime-brow with KB_PROFILE_DIR=<empty dir>
// --remote-debugging-port=9222, then: node scripts/split-smoke.mjs
const base = 'http://127.0.0.1:9222';
const pages = async () => (await (await fetch(base + '/json')).json()).filter(t => t.type === 'page');
const ev = (ws, expression) => new Promise(res => { const w = new WebSocket(ws); w.onopen = () => w.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); w.onmessage = e => { const m = JSON.parse(e.data); if (m.id === 1) { w.close(); res(m.result?.result?.value); } }; });
const q = (r) => `new Promise(x => window.kbQuery({request: JSON.stringify(${JSON.stringify(r)}), onSuccess: v => x(v), onFailure: (c, m) => x('FAIL ' + m)}))`;
const sleep = ms => new Promise(r => setTimeout(r, ms));
const ui = (await pages()).find(p => p.url.startsWith('kb://ui'));
await ev(ui.webSocketDebuggerUrl, q({ cmd: 'new-tab', url: 'kb://term/' }));
await sleep(3000);
const t = (await pages()).find(p => p.url.startsWith('kb://term'));
const key = code => ev(t.webSocketDebuggerUrl, `window.dispatchEvent(new KeyboardEvent('keydown',{ctrlKey:true,shiftKey:true,code:'${code}',bubbles:true}))`);
const count = () => ev(t.webSocketDebuggerUrl, `document.querySelectorAll('.xterm').length`);
console.log('panes at start:', await count());
await key('KeyD'); await sleep(2000);
console.log('after split right:', await count());
await key('KeyE'); await sleep(2000);
console.log('after split down:', await count());
await key('KeyW'); await sleep(800);
console.log('after close:', await count());
process.exit(0);
