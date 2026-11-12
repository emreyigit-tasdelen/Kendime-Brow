// Live check for the network inspector (task 4B.1). Start kendime-brow with KB_PROFILE_DIR=<empty dir>
// --remote-debugging-port=9222, then: node scripts/net-smoke.mjs
import http from 'node:http';
const srv = http.createServer((q, r) => { r.setHeader('content-type', 'text/plain'); r.end('hello'); }).listen(8765, '127.0.0.1');
const base = 'http://127.0.0.1:9222';
const pages = async () => (await (await fetch(base + '/json')).json()).filter(t => t.type === 'page');
const ev = (ws, expression) => new Promise(res => { const w = new WebSocket(ws); w.onopen = () => w.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); w.onmessage = e => { const m = JSON.parse(e.data); if (m.id === 1) { w.close(); res(m.result?.result?.value); } }; });
const q = (r) => `new Promise(x => window.kbQuery({request: JSON.stringify(${JSON.stringify(r)}), onSuccess: v => x(v), onFailure: (c, m) => x('FAIL ' + m)}))`;
const sleep = ms => new Promise(r => setTimeout(r, ms));
const ui = (await pages()).find(p => p.url.startsWith('kb://ui'));
await ev(ui.webSocketDebuggerUrl, q({ cmd: 'new-tab', url: 'http://127.0.0.1:8765/probe' }));
await sleep(2500);
const rows = JSON.parse(await ev(ui.webSocketDebuggerUrl, q({ cmd: 'net', op: 'list', args: {} })));
const hit = rows.find(r => r.url.endsWith('/probe'));
console.log('rows:', rows.length, 'probe recorded:', !!hit, hit && `status=${hit.status} mime=${hit.mime} bytes=${hit.bytes}`);
const id = hit?.id;
if (id) console.log('curl:', JSON.parse(await ev(ui.webSocketDebuggerUrl, q({ cmd: 'net', op: 'curl', args: { id } }))).curl.split('\n')[0]);
const har = JSON.parse(await ev(ui.webSocketDebuggerUrl, q({ cmd: 'net', op: 'har', args: {} })));
console.log('har entries:', har.log.entries.length);
srv.close();
process.exit(hit ? 0 : 1);
