// Smoke test for the kb:// bridge (task 1.8). Run kendime-brow with --remote-debugging-port=9222 --url=https://example.org, then: node scripts/bridge-smoke.mjs
const base = 'http://127.0.0.1:9222';
const pages = async () => (await (await fetch(base + '/json')).json()).filter(t => t.type === 'page');
const evalIn = (wsUrl, expression) => new Promise(res => {
  const ws = new WebSocket(wsUrl);
  ws.onopen = () => ws.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } }));
  ws.onmessage = e => { const m = JSON.parse(e.data); if (m.id === 1) { ws.close(); res(m.result?.result?.value ?? m.result); } };
});
const q = (req) => `new Promise(r => window.kbQuery({request: JSON.stringify(${JSON.stringify(req)}), onSuccess: x => r('OK ' + x), onFailure: (c, m) => r('FAIL ' + c + ' ' + m)}))`;
const sleep = ms => new Promise(r => setTimeout(r, ms));
let ps = await pages();
const ui = ps.find(p => p.url.startsWith('kb://ui'));
const web = ps.find(p => p.url.startsWith('https://'));
console.log('web has kbQuery:', await evalIn(web.webSocketDebuggerUrl, 'typeof window.kbQuery'));
console.log('ui has kbQuery:', await evalIn(ui.webSocketDebuggerUrl, 'typeof window.kbQuery'));
console.log('state:', await evalIn(ui.webSocketDebuggerUrl, q({ cmd: 'state' })));
console.log('new-tab:', await evalIn(ui.webSocketDebuggerUrl, q({ cmd: 'new-tab', url: 'example.com' })));
await sleep(2500);
console.log('state:', await evalIn(ui.webSocketDebuggerUrl, q({ cmd: 'state' })));
console.log('navigate:', await evalIn(ui.webSocketDebuggerUrl, q({ cmd: 'navigate', text: 'kb://newtab' })));
await sleep(1500);
console.log('bad:', await evalIn(ui.webSocketDebuggerUrl, q({ cmd: 'rm-rf' })));
const st = JSON.parse((await evalIn(ui.webSocketDebuggerUrl, q({ cmd: 'state' }))).slice(3));
console.log('tabs:', st.tabs.map(t => `${t.id}:${t.active ? '*' : ''}${t.url}`).join(' '));
console.log('close-tab:', await evalIn(ui.webSocketDebuggerUrl, q({ cmd: 'close-tab', id: st.tabs.find(t => t.active).id })));
await sleep(1500);
console.log('pages:', (await pages()).map(p => p.url).join(' | '));
