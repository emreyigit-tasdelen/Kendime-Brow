// Live check for net inspector edit-and-resend (task 4B.4). Start kendime-brow with KB_PROFILE_DIR=<empty dir>
// --remote-debugging-port=9222, then: node scripts/resend-smoke.mjs
import http from 'node:http';
const srv = http.createServer((q, r) => { let b = ''; q.on('data', d => b += d); q.on('end', () => { r.setHeader('x-seen', `${q.method} ${q.headers['x-edit'] ?? ''} cookie=${q.headers.cookie ?? ''}`); r.end('echo:' + b); }); }).listen(8768, '127.0.0.1');
const base = 'http://127.0.0.1:9222';
const pages = async () => (await (await fetch(base + '/json')).json()).filter(t => t.type === 'page');
const ev = (ws, expression) => new Promise(res => { const w = new WebSocket(ws); w.onopen = () => w.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); w.onmessage = e => { const m = JSON.parse(e.data); if (m.id === 1) { w.close(); res(m.result?.result?.value); } }; });
const q = (r) => `new Promise(x => window.kbQuery({request: JSON.stringify(${JSON.stringify(r)}), onSuccess: v => x(v), onFailure: (c, m) => x('FAIL ' + m)}))`;
const ui = (await pages()).find(p => p.url.startsWith('kb://ui'));
const call = (r) => ev(ui.webSocketDebuggerUrl, q(r));
console.log(await call({ cmd: 'net', op: 'resend', args: { method: 'post', url: 'http://127.0.0.1:8768/x', headers: [['X-Edit', 'yes'], ['Cookie', 'a=b']], body: 'hello' } }));
console.log(await call({ cmd: 'net', op: 'resend', args: { method: 'GET', url: 'file:///etc/hosts', headers: [], body: '' } }));
srv.close(); process.exit(0);
