import { test } from 'node:test';
import assert from 'node:assert/strict';
import { clean, remember, observe } from './evidence.mjs';

test('diagnostics redact nested credentials while retaining meaningful evidence', () => {
  remember('test-session-credential');
  const value = clean({ status: 200, headers: { hasCookie: true, cookie: 'test-session-credential' },
    request: { password: 'anything', email: 'alice@e2e.test' },
    items: ['prefix test-session-credential suffix', 'eyJabc.abcdefgh.abcdefgh'] });
  assert.deepEqual(value, { status: 200, headers: { hasCookie: true, cookie: '[REDACTED]' },
    request: { password: '[REDACTED]', email: 'alice@e2e.test' },
    items: ['prefix [REDACTED] suffix', '[REDACTED_JWT]'] });
});


test('navigation-aborted response inspection settles without losing failure evidence', async () => {
  const { EventEmitter } = await import('node:events');
  const page = new EventEmitter(), records = [];
  const recorder = observe(page, 'alice', (kind, record) => records.push({ kind, ...record }));
  const request = { url: () => 'http://web:8080/api/people', headers: () => ({}),
    postDataJSON: () => null, method: () => 'GET', failure: () => ({ errorText: 'net::ERR_ABORTED' }) };
  page.emit('request', request);
  page.emit('response', { request: () => request, status: () => 200, headers: () => ({}), headersArray: () => new Promise(() => {}) });
  page.emit('requestfailed', request);
  await recorder.flush();
  assert.equal(recorder.http[0].failure, 'net::ERR_ABORTED');
  assert.equal(records.at(-1).failure, 'net::ERR_ABORTED');
  assert.deepEqual(recorder.errors, []);
});


test('response inspection timeout cannot produce a passing final flush', async t => {
  const { EventEmitter } = await import('node:events');
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const page = new EventEmitter(), records = [];
  const recorder = observe(page, 'alice', (kind, record) => records.push({ kind, ...record }));
  const request = { url: () => 'http://web:8080/api/people', headers: () => ({}),
    postDataJSON: () => null, method: () => 'GET' };
  page.emit('request', request);
  page.emit('response', { request: () => request, status: () => 200, headers: () => ({}), headersArray: () => new Promise(() => {}) });
  const failure = assert.rejects(recorder.flush(), /Response evidence timed out: \/api\/people/);
  t.mock.timers.tick(15_000);
  await failure;
  assert.equal(records.at(-1).kind, 'recorder');
});


test('navigation retains response status while explicitly marking unavailable body', async () => {
  const { EventEmitter } = await import('node:events');
  const page = new EventEmitter(), records = [], frame = {};
  page.mainFrame = () => frame;
  const recorder = observe(page, 'alice', (kind, record) => records.push({ kind, ...record }));
  const request = { url: () => 'http://web:8080/api/people', headers: () => ({}),
    postDataJSON: () => null, method: () => 'GET', allHeaders: async () => ({}) };
  page.emit('request', request);
  page.emit('response', { request: () => request, status: () => 200, headers: () => ({}),
    headersArray: async () => [], json: () => new Promise(() => {}) });
  page.emit('framenavigated', frame);
  await recorder.flush();
  assert.equal(records.at(-1).status, 200);
  assert.equal(records.at(-1).responseUnavailable, 'navigation_interrupted_inspection');
  assert.equal(records.at(-1).response, undefined);
  assert.deepEqual(recorder.errors, []);
});

test('completed response evidence retains its body across later navigation', async () => {
  const { EventEmitter } = await import('node:events');
  const page = new EventEmitter(), records = [], frame = {};
  page.mainFrame = () => frame;
  const recorder = observe(page, 'alice', (kind, record) => records.push({ kind, ...record }));
  const request = { url: () => 'http://web:8080/api/people', headers: () => ({}),
    postDataJSON: () => null, method: () => 'GET', allHeaders: async () => ({}) };
  page.emit('request', request);
  page.emit('response', { request: () => request, status: () => 200, headers: () => ({}),
    headersArray: async () => [], json: async () => ({ people: [] }) });
  await recorder.flush();
  page.emit('framenavigated', frame);
  assert.deepEqual(records.at(-1).response, { people: [] });
  assert.equal(records.at(-1).responseUnavailable, undefined);
  assert.deepEqual(recorder.errors, []);
});
