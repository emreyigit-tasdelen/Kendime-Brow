// Live check for encrypted session logs (task 4A.9). Start kendime-brow with KB_PROFILE_DIR=<empty dir>
// --remote-debugging-port=9222, then: node scripts/seclog-smoke.mjs
const base = 'http://127.0.0.1:9222';
const pages = async () => (await (await fetch(base + '/json')).json()).filter(t => t.type === 'page');
const ev = (ws, expression) => new Promise(res => { const w = new WebSocket(ws); w.onopen = () => w.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); w.onmessage = e => { const m = JSON.parse(e.data); if (m.id === 1) { w.close(); res(m.result?.result?.value); } }; });
const q = (r) => `new Promise(x => window.kbQuery({request: JSON.stringify(${JSON.stringify(r)}), onSuccess: v => x(v), onFailure: (c, m) => x('FAIL ' + m)}))`;
const sleep = ms => new Promise(r => setTimeout(r, ms));
const ui = (await pages()).find(p => p.url.startsWith('kb://ui'));
const call = (r) => ev(ui.webSocketDebuggerUrl, q(r));
console.log('log without vault:', await call({ cmd: 'session-log', op: 'start', args: { id: 1, label: 'x' } }));
console.log('vault create:', await call({ cmd: 'vault', op: 'create', args: { password: 'correct horse battery' } }));
await call({ cmd: 'vault', op: 'unlock', args: { password: 'correct horse battery' } });
await call({ cmd: 'new-tab', url: 'kb://term/' });
await sleep(2500);
// Session 1 is the one the kb://term page opened itself (its xterm answers ConPTY's cursor query).
const id = { id: 1 };
console.log('start:', await call({ cmd: 'session-log', op: 'start', args: { id: id.id, label: 'smoke' } }));
await call({ cmd: 'term-write', id: id.id, data: Buffer.from('echo seclog-marker-42\r\n').toString('base64') });
await sleep(2500);
const list = JSON.parse(await call({ cmd: 'session-log', op: 'list', args: {} }));
console.log('logs:', list.length);
const entries = JSON.parse(await call({ cmd: 'session-log', op: 'read', args: { name: list[0].name } }));
const text = entries.filter(e => e.d === 'out').map(e => e.t).join('').replace(/[[0-9;?]*[ -/]*[@-~]/g, '');
console.log('out chars:', text.length, 'has marker:', text.includes('seclog-marker-42'));
console.log('typed recorded:', entries.some(e => e.d === 'in' && e.t.includes('seclog-marker-42')), 'output recorded:', entries.some(e => e.d === 'out' && e.t.includes('seclog-marker-42')));
const fs = await import('node:fs'); const dir = process.env.KB_PROFILE_DIR;
if (dir) console.log('plaintext on disk:', fs.readFileSync(`${dir}/session-logs/${list[0].name}`).includes('seclog-marker-42'));
process.exit(0);
