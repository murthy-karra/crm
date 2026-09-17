// Family-local fail-closed external sink and transparent realtime fault proxy.
// No CRM response is mocked; successful publication always reaches Centrifugo.
import http from 'node:http';
const providers = [];
if (['operator','calls'].includes(process.env.E2E_FAMILY)) providers.push((await import('./operator-provider.mjs')).handleProvider);
if (process.env.E2E_FAMILY === 'calls') providers.push((await import('./calls-provider.mjs')).handleProvider);
if (process.env.E2E_FAMILY === 'migration') providers.push((await import('./migration-provider.mjs')).handleProvider);
let failPublish = false;
const requests = [];
const send = (res, status, body) => {
  res.writeHead(status, { 'content-type': 'application/json' });
  res.end(JSON.stringify(body));
};
http.createServer(async (req, res) => {
  if (req.url === '/health') return send(res, 200, { ready: true });
  if (req.url === '/evidence') return send(res, 200, { project: process.env.E2E_PROJECT, requests });
  if (req.url === '/control' && req.method === 'POST') {
    let data = ''; for await (const chunk of req) data += chunk;
    const input = JSON.parse(data);
    if (typeof input.failPublish !== 'boolean') return send(res, 400, { error: 'invalid_control' });
    failPublish = input.failPublish;
    requests.push({ kind: 'control', failPublish, at: Date.now() });
    return send(res, 200, { failPublish });
  }
  if (providers.length && /^\/(operator-control|openai\/|call-control|call-provider\/|twirp\/|fub\/|migration-control)/.test(req.url)) {
    try {
      let raw = ''; for await (const chunk of req) raw += chunk;
      const body = raw ? JSON.parse(raw) : {};
      for (const provider of providers) if (await provider(req,res,body,requests)) return;
    } catch { return send(res,500,{error:'controlled_provider_error'}); }
  }
  if (req.url.startsWith('/api')) {
    const record = { kind: 'realtime', method: req.method, path: req.url, at: Date.now(), failed: failPublish };
    requests.push(record);
    if (failPublish) { req.resume(); return send(res, 503, { error: 'e2e_publication_failure' }); }
    const upstream = http.request({ hostname: 'centrifugo', port: 8000,
      path: req.url, method: req.method, headers: req.headers }, response => {
      record.status = response.statusCode;
      res.writeHead(response.statusCode, response.headers); response.pipe(res);
    });
    upstream.on('error', () => { if (!res.headersSent) send(res, 502, { error: 'upstream_unavailable' }); else res.destroy(); });
    req.pipe(upstream); return;
  }
  requests.push({ kind: 'external', method: req.method, path: req.url.split('?')[0], at: Date.now() });
  req.resume(); send(res, 503, { error: 'external_service_disabled_for_lead_family' });
}).listen(9000, '0.0.0.0');
