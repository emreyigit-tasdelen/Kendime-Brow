// Container isolation check (task 2.5). Run kendime-brow with --remote-debugging-port=9222 --url=https://example.org, then: node scripts/container-isolation.mjs
const v = await (await fetch('http://127.0.0.1:9222/json/version')).json();
const bws = new WebSocket(v.webSocketDebuggerUrl);
let id = 0; const pending = new Map();
const call = (method, params = {}, sessionId) => new Promise(r => { const i = ++id; pending.set(i, r); bws.send(JSON.stringify({ id: i, method, params, sessionId })); });
bws.onmessage = e => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m.result ?? m.error); pending.delete(m.id); } };
await new Promise(r => bws.onopen = r);
const sleep = ms => new Promise(r => setTimeout(r, ms));
const targets = async () => (await call('Target.getTargets')).targetInfos.filter(t => t.type === 'page');
const evalIn = async (t, expression) => {
  const { sessionId } = await call('Target.attachToTarget', { targetId: t.targetId, flatten: true });
  const r = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }, sessionId);
  await call('Target.detachFromTarget', { sessionId });
  return r.result?.value;
};
const ui = async () => (await targets()).find(t => t.url.startsWith('kb://ui'));
const q = async (cmd) => evalIn(await ui(), `new Promise(r => kbQuery({request: ${JSON.stringify(JSON.stringify(cmd))}, onSuccess: r, onFailure: (c, m) => r('FAIL ' + m)}))`);
const ex = async () => (await targets()).filter(t => t.url.startsWith('https://example.org'));

const [personal] = await ex();
console.log('personal ctx', personal.browserContextId?.slice(0, 8), 'set:', await evalIn(personal, "document.cookie='who=personal; max-age=3600'; document.cookie"));
await q({ cmd: 'new-tab', url: 'https://example.org/', container: 'temporary' }); await sleep(3000);
const priv = (await ex()).find(t => t.targetId !== personal.targetId);
console.log('private ctx', priv.browserContextId?.slice(0, 8), 'sees:', JSON.stringify(await evalIn(priv, 'document.cookie')));
await evalIn(priv, "document.cookie='who=private; max-age=3600'");
console.log('personal sees:', await evalIn(personal, 'document.cookie'));
await q({ cmd: 'new-tab', url: 'https://example.org/', container: 'work' }); await sleep(3000);
const work = (await ex()).find(t => ![personal.targetId, priv.targetId].includes(t.targetId));
console.log('work ctx', work.browserContextId?.slice(0, 8), 'sees:', JSON.stringify(await evalIn(work, 'document.cookie')));
process.exit(0);
