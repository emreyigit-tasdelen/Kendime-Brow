// Live check for autofill (task 6.2). Start kendime-brow with KB_PROFILE_DIR=<empty dir>
// --remote-debugging-port=9222, then: node scripts/autofill-smoke.mjs
import http from 'node:http';
const html = '<title>login</title><form><input id=u type=text><input id=p type=password></form>';
const mk = port => http.createServer((q, r) => { r.setHeader('content-type', 'text/html'); r.end(html); }).listen(port, '127.0.0.1');
const servers = [mk(8766), mk(8767)];
const base = 'http://127.0.0.1:9222';
const pages = async () => (await (await fetch(base + '/json')).json()).filter(t => t.type === 'page');
const ev = (ws, expression) => new Promise(res => { const w = new WebSocket(ws); w.onopen = () => w.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); w.onmessage = e => { const m = JSON.parse(e.data); if (m.id === 1) { w.close(); res(m.result?.result?.value); } }; });
const q = (r) => `new Promise(x => window.kbQuery({request: JSON.stringify(${JSON.stringify(r)}), onSuccess: v => x(v), onFailure: (c, m) => x('FAIL ' + m)}))`;
const sleep = ms => new Promise(r => setTimeout(r, ms));
const ui = (await pages()).find(p => p.url.startsWith('kb://ui'));
const call = (r) => ev(ui.webSocketDebuggerUrl, q(r));
await call({ cmd: 'vault', op: 'create', args: { password: 'correct horse battery' } });
await call({ cmd: 'vault', op: 'unlock', args: { password: 'correct horse battery' } });
console.log('import:', await call({ cmd: 'vault', op: 'import-csv', args: { text: 'name,url,username,password\nlocal,http://127.0.0.1:8766/,alice,s3cret-pw\n' } }));
for (const port of [8766, 8767]) {
  await call({ cmd: 'new-tab', url: `http://127.0.0.1:${port}/` });
  await sleep(2500);
  const tab = (await pages()).find(p => p.url.includes(String(port)));
  await call({ cmd: 'autofill' });
  await sleep(800);
  console.log(port, await ev(tab.webSocketDebuggerUrl, 'JSON.stringify([u.value, p.value, document.body.innerText])'));
}
servers.forEach(s => s.close()); process.exit(0);
