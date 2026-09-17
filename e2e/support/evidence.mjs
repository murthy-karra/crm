import { appendFileSync, readFileSync, writeFileSync } from 'node:fs';
import { Client } from 'pg';

const secrets = new Set([process.env.E2E_PASSWORD].filter(Boolean));
try { for (const value of JSON.parse(readFileSync('/private/observed-secrets.json'))) secrets.add(value); } catch {}
export function remember(value) {
  if (typeof value !== 'string' || value.length < 8) return;
  secrets.add(value);
  if (process.env.E2E_PROJECT) writeFileSync('/private/observed-secrets.json', JSON.stringify([...secrets]), { mode: 0o600 });
}
export function clean(value) {
  if (typeof value === 'string') {
    for (const secret of secrets) value = value.replaceAll(secret, '[REDACTED]');
    return value.replace(/eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g, '[REDACTED_JWT]');
  }
  if (Array.isArray(value)) return value.map(clean);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, v]) =>
    [key, typeof v !== 'boolean' && /password|token|cookie|authorization|secret|accept_path/i.test(key) ? '[REDACTED]' : clean(v)]));
  return value;
}
export function evidence(file, value) {
  appendFileSync('/artifacts/' + file + '.jsonl', JSON.stringify(clean(value)) + '\n');
}
export async function captureSecrets(response) {
  const headers = await response.headersArray();
  for (const h of headers) if (h.name.toLowerCase() === 'set-cookie') {
    remember(h.value.split(';')[0].split('=').slice(1).join('='));
  }
}
export function observe(page, actor, recordEvidence = evidence) {
  const http = [], frames = [], errors = [], sockets = [];
  const pending = new Set();
  const byRequest = new Map();
  const cancellations = new Map();
  page.on('request', req => {
    const url = new URL(req.url());
    if (!url.pathname.startsWith('/api/')) return;
    const headers = req.headers();
    for (const pair of (headers.cookie || '').split(';')) remember(pair.trim().split('=').slice(1).join('='));
    let body = null; try { body = req.postDataJSON(); } catch {}
    const record = { actor, method: req.method(), path: url.pathname, query: url.search,
      started: Date.now(), headers: { contentType: headers['content-type'],
        origin: headers.origin, requestId: headers['x-request-id'], hasCookie: !!headers.cookie },
      request: clean(body) };
    http.push(record); byRequest.set(req, record);
    let cancel;
    const promise = new Promise(resolve => { cancel = resolve; });
    cancellations.set(req, { promise, cancel });
  });
  page.on('response', response => {
    const record = byRequest.get(response.request());
    if (!record || record.failure) return;
    const cancellation = cancellations.get(response.request());
    cancellation.capturing = true;
    Object.assign(record, { status: response.status(), responseHeaders: {
      contentType: response.headers()['content-type'], requestId: response.headers()['x-request-id'] } });
    // Chromium can leave headers/body inspection pending when navigation aborts
    // a response. Its requestfailed event is the authoritative terminal outcome.
    // Preserve that failure record and let diagnostics finish on that event.
    const capture = (async () => {
      await captureSecrets(response);
      const headers = await response.request().allHeaders();
      for (const pair of (headers.cookie || '').split(';')) remember(pair.trim().split('=').slice(1).join('='));
      Object.assign(record.headers, { hasCookie: !!headers.cookie, origin: headers.origin });
      let body = null; try { body = await response.json(); } catch {}
      if (record.path === '/api/realtime/token') remember(body?.token);
      if (body?.accept_path) remember(body.accept_path.split('/').at(-1));
      if (record.failure || record.responseUnavailable) return;
      Object.assign(record, { finished: Date.now(), status: response.status(),
        response: clean(body), responseHeaders: { contentType: response.headers()['content-type'],
          requestId: response.headers()['x-request-id'] } });
      recordEvidence('http', record);
    })().catch(error => { if (!record.failure && !record.responseUnavailable) errors.push({ actor, kind: 'recorder', message: String(error) }); });
    let timer;
    const deadline = new Promise(resolve => {
      timer = setTimeout(() => {
        const item = { actor, kind: 'recorder', message: 'Response evidence timed out', path: record.path };
        errors.push(item); recordEvidence('console', item); resolve();
      }, 15_000);
    });
    const promise = Promise.race([capture, cancellation.promise, deadline]).finally(() => {
      clearTimeout(timer); pending.delete(promise); cancellations.delete(response.request());
    });
    pending.add(promise);
  });
  page.on('framenavigated', frame => {
    if (frame !== page.mainFrame()) return;
    for (const [req, cancellation] of cancellations) {
      if (!cancellation.capturing) continue;
      const record = byRequest.get(req);
      // A response can be abandoned during navigation without requestfailed.
      // Keep its actual HTTP status, but never invent an unavailable body.
      record.responseUnavailable = 'navigation_interrupted_inspection';
      record.finished = Date.now(); recordEvidence('http', record);
      cancellation.cancel(); cancellations.delete(req);
    }
  });
  page.on('requestfailed', req => {
    const record = byRequest.get(req);
    if (record) { record.failure = req.failure()?.errorText; record.finished = Date.now(); recordEvidence('http', record); }
    cancellations.get(req)?.cancel();
    cancellations.delete(req);
  });
  page.on('websocket', socket => {
    const connection = { actor, opened: Date.now(), closed: null }; sockets.push(connection);
    socket.on('close', () => { connection.closed = Date.now(); recordEvidence('websocket-connections', connection); });
    socket.on('framesent', ({ payload }) => {
      try { for (const line of String(payload).split('\n')) remember(JSON.parse(line)?.connect?.token); } catch {}
    });
    socket.on('framereceived', ({ payload }) => {
      for (const line of String(payload).split('\n')) {
        try {
          const message = JSON.parse(line);
          const frame = { actor, at: Date.now(), message: clean(message) };
          frames.push(frame); recordEvidence('realtime', frame);
        } catch { /* heartbeat/non-JSON frame is not application evidence */ }
      }
    });
  });
  page.on('pageerror', error => { const item = { actor, kind: 'pageerror', message: String(error) }; errors.push(item); recordEvidence('console', item); });
  page.on('console', message => {
    if (!['error', 'warning'].includes(message.type())) return;
    const item = { actor, kind: message.type(), message: message.text(), location: message.location() };
    // Browser network errors are recorded separately; denied access and dropped
    // responses are intentional steps. Framework-caught JS exceptions still fail.
    if (message.type() === 'error' && !message.text().startsWith('Failed to load resource:') &&
        !message.text().startsWith('WebSocket connection to ')) errors.push(item);
    recordEvidence('console', item);
  });
  return { http, frames, sockets, errors, flush: async () => {
    await Promise.all([...pending]);
    const failure = errors.find(error => error.kind === 'recorder');
    if (failure) throw Error(failure.message + (failure.path ? ': ' + failure.path : ''));
  } };
}

export async function database() {
  const client = new Client({ connectionString: process.env.E2E_AUDIT_URL });
  await client.connect();
  return {
    async check(name, sql, values, verify) {
      if (!/^\s*SELECT\b/i.test(sql) || sql.includes(';')) throw Error('DB assertions must be a single SELECT');
      await client.query('BEGIN READ ONLY');
      try {
        const { rows } = await client.query(sql, values);
        try { await verify(rows); evidence('database', { name, sql, values, rows, status: 'passed' }); }
        catch (error) { evidence('database', { name, sql, values, rows, status: 'failed' }); throw error; }
        return rows;
      } finally { await client.query('ROLLBACK'); }
    },
    close: () => client.end(),
  };
}
