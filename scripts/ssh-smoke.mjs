// Live check for SSH in kb://term (task 4A.3). Start `cargo run -p kb-ssh --example testserver -- 2222` and
// kendime-brow with KB_PROFILE_DIR=<empty dir> --remote-debugging-port=9222, then: node scripts/ssh-smoke.mjs
const base = 'http://127.0.0.1:9222';
const pages = async () => (await (await fetch(base + '/json')).json()).filter(t => t.type === 'page');
const ev = (ws, expression) => new Promise(res => { const w = new WebSocket(ws); w.onopen = () => w.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } })); w.onmessage = e => { const m = JSON.parse(e.data); if (m.id === 1) { w.close(); res(m.result?.result?.value); } }; });
const q = (r) => `new Promise(x => window.kbQuery({request: JSON.stringify(${JSON.stringify(r)}), onSuccess: v => x(v), onFailure: (c, m) => x('FAIL ' + m)}))`;
const sleep = ms => new Promise(r => setTimeout(r, ms));
const ui = (await pages()).find(p => p.url.startsWith('kb://ui'));
await ev(ui.webSocketDebuggerUrl, q({ cmd: 'new-tab', url: 'kb://term/?host=127.0.0.1&port=2222&user=tester' }));
await sleep(2500);
const term = (await pages()).find(p => p.url.includes('host=127.0.0.1'));
const w = term.webSocketDebuggerUrl;
const rows = () => ev(w, `document.querySelector('.xterm-rows')?.innerText ?? ''`);
const submitPassword = pw => ev(w, `(() => { const i = document.querySelector('.term-overlay input'); i.value = ${JSON.stringify(pw)}; i.form.requestSubmit(); })()`);
await submitPassword('pw');
await sleep(2500);
let t = await rows();
console.log('unknown host prompt shown:', t.includes('cannot be verified'));
await ev(w, `document.querySelector('.term-overlay button').click()`);
await sleep(3000);
t = await rows();
console.log('connected after trust:', t.includes('welcome to the kb-ssh test server'));
await ev(w, `(() => { const ta = document.querySelector('.xterm-helper-textarea'); const dt = new DataTransfer(); dt.setData('text/plain', 'hello-over-ssh'); ta.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true })); })()`);
await sleep(1500);
console.log('echo over ssh:', (await rows()).includes('hello-over-ssh'));
