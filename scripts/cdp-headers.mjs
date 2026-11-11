// Dev tool: prints response headers of kb:// requests via the DevTools protocol.
// Usage: run kendime-brow with --remote-debugging-port=9222, then: node scripts/cdp-headers.mjs kb://ui/
const [, , url] = process.argv;
const list = await (await fetch('http://127.0.0.1:9222/json')).json();
const page = list.find(t => t.type === 'page');
const ws = new WebSocket(page.webSocketDebuggerUrl);
let id = 0; const send = (method, params = {}) => ws.send(JSON.stringify({ id: ++id, method, params }));
ws.onopen = () => { send('Network.enable'); send('Page.navigate', { url }); };
ws.onmessage = e => {
  const m = JSON.parse(e.data);
  if (m.method === 'Network.responseReceived' && m.params.response.url.startsWith('kb:')) {
    console.log(m.params.response.url, m.params.response.status, JSON.stringify(m.params.response.headers));
  }
};
setTimeout(() => process.exit(0), 3000);
