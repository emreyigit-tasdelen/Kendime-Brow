// After term-smoke: closes every kb://term tab and prints the remaining shell count (run `tasklist` before/after).
const base = 'http://127.0.0.1:9222';
const pages = async () => (await (await fetch(base + '/json')).json()).filter(t => t.type === 'page');
const ev = (ws, expression) => new Promise(res => { const w = new WebSocket(ws); w.onopen = () => w.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); w.onmessage = e => { const m = JSON.parse(e.data); if (m.id === 1) { w.close(); res(m.result?.result?.value); } }; });
const q = (r) => `new Promise(x => window.kbQuery({request: JSON.stringify(${JSON.stringify(r)}), onSuccess: v => x(v), onFailure: (c, m) => x('FAIL ' + m)}))`;
const ui = (await pages()).find(p => p.url.startsWith('kb://ui'));
const st = JSON.parse(await ev(ui.webSocketDebuggerUrl, q({ cmd: 'state' })));
for (const t of st.tabs.filter(t => t.url.startsWith('kb://term'))) console.log('close', t.id, await ev(ui.webSocketDebuggerUrl, q({ cmd: 'close-tab', id: t.id })));
