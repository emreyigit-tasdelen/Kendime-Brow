// Live check for userscripts (task 6.5). Start kendime-brow with KB_PROFILE_DIR=<empty dir>
// --remote-debugging-port=9222, then: node scripts/userscript-smoke.mjs
import http from 'node:http';
const srv = http.createServer((q, r) => { r.setHeader('content-type', 'text/html'); r.end('<title>plain</title><body>x</body>'); }).listen(8766, '127.0.0.1');
const base = 'http://127.0.0.1:9222';
const pages = async () => (await (await fetch(base + '/json')).json()).filter(t => t.type === 'page');
const ev = (ws, expression) => new Promise(res => { const w = new WebSocket(ws); w.onopen = () => w.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); w.onmessage = e => { const m = JSON.parse(e.data); if (m.id === 1) { w.close(); res(m.result?.result?.value); } }; });
const q = (r) => `new Promise(x => window.kbQuery({request: JSON.stringify(${JSON.stringify(r)}), onSuccess: v => x(v), onFailure: (c, m) => x('FAIL ' + m)}))`;
const sleep = ms => new Promise(r => setTimeout(r, ms));
const ui = (await pages()).find(p => p.url.startsWith('kb://ui'));
const call = (r) => ev(ui.webSocketDebuggerUrl, q(r));
const src = '// ==UserScript==\n// @name Smoke\n// @version 1\n// @match http://127.0.0.1:8766/*\n// ==/UserScript==\nGM_setValue("n", 41); document.title = "script-ran-" + (GM_getValue("n", 0) + 1);';
console.log('bad header:', await call({ cmd: 'userscripts', op: 'save', args: { source: 'alert(1)' } }));
console.log('save:', await call({ cmd: 'userscripts', op: 'save', args: { source: src } }));
await call({ cmd: 'new-tab', url: 'http://127.0.0.1:8766/a' });
await sleep(3000);
let tab = (await pages()).find(p => p.url.includes('8766'));
console.log('title with script:', await ev(tab.webSocketDebuggerUrl, 'document.title'));
const list = JSON.parse(await call({ cmd: 'userscripts', op: 'list', args: {} }));
await call({ cmd: 'userscripts', op: 'toggle', args: { file: list[0].file } });
await call({ cmd: 'new-tab', url: 'http://127.0.0.1:8766/b' });
await sleep(3000);
tab = (await pages()).find(p => p.url.includes('8766/b'));
console.log('title when disabled:', await ev(tab.webSocketDebuggerUrl, 'document.title'));
srv.close(); process.exit(0);
