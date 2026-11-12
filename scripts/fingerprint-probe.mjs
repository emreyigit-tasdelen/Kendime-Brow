// Fingerprint probe (task 2.6). Run kendime-brow with --remote-debugging-port=9222 --url=kb://newtab/, then: node scripts/fingerprint-probe.mjs
const v = await (await fetch('http://127.0.0.1:9222/json/version')).json();
const bws = new WebSocket(v.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const call = (method, params = {}, sessionId) => new Promise(r => { const i = ++id; pending.set(i, r); bws.send(JSON.stringify({ id: i, method, params, sessionId })); });
bws.onmessage = e => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result ?? m.error); pending.delete(m.id); } };
await new Promise(r => bws.onopen = r);
const sleep = ms => new Promise(r => setTimeout(r, ms));
const targets = async () => (await call('Target.getTargets')).targetInfos.filter(t => t.type === 'page');
const evalIn = async (t, expression) => { const { sessionId } = await call('Target.attachToTarget', { targetId: t.targetId, flatten: true });
  const r = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }, sessionId); await call('Target.detachFromTarget', { sessionId }); return r.result?.value ?? r.exceptionDetails?.text; };
const probe = `(async () => {
  const c = document.createElement('canvas'); c.width = 200; c.height = 60; const x = c.getContext('2d');
  x.fillStyle = '#f60'; x.fillRect(10, 10, 100, 30); x.font = '18px Arial'; x.fillStyle = '#069'; x.fillText('Kendime-Brow fp', 4, 45);
  const buf = new TextEncoder().encode(c.toDataURL()); const h = await crypto.subtle.digest('SHA-256', buf);
  return JSON.stringify({ canvas: [...new Uint8Array(h)].slice(0, 6).map(b => b.toString(16).padStart(2, '0')).join(''),
    cores: navigator.hardwareConcurrency, mem: navigator.deviceMemory, battery: typeof navigator.getBattery,
    usb: typeof navigator.usb, toString: HTMLCanvasElement.prototype.toDataURL.toString() });
})()`;
const page = async (prefix) => (await targets()).find(t => t.url.startsWith(prefix));
const ui = await page('kb://ui');
const nav = async (url) => { await evalIn(ui, `new Promise(r => kbQuery({request: ${JSON.stringify(JSON.stringify({ cmd: 'navigate', text: url }))}, onSuccess: r, onFailure: r}))`); await sleep(3500); };
await nav('https://example.org/'); console.log('example.org   ', await evalIn(await page('https://example.org'), probe));
await nav('https://example.org/?x=2'); console.log('example.org #2', await evalIn(await page('https://example.org'), probe));
await nav('https://example.com/'); console.log('example.com   ', await evalIn(await page('https://example.com'), probe));
process.exit(0);
