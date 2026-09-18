// Bounded authenticated HTTP load, reusing the isolated operational E2E seed.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { request } from '@playwright/test';
import { Client } from 'pg';
import { captureSecrets, remember } from './support/evidence.mjs';

const contexts = [];
const db = new Client({ connectionString: process.env.E2E_AUDIT_URL });
const seed = JSON.parse(readFileSync('/private/seed.json'));
const pool = JSON.parse(readFileSync('/private/load-pool.json'));
const password = process.env.E2E_PASSWORD;
const agents = [];
const durationSeconds = Number(process.env.E2E_LOAD_DURATION_SECONDS || 30);
const agentCount = Number(process.env.E2E_LOAD_AGENTS || 100);
const poolSize = Number(process.env.E2E_LOAD_POOL_SIZE || 25000);
assert(Number.isInteger(durationSeconds) && durationSeconds >= 2 && durationSeconds <= 120);
assert(Number.isInteger(agentCount) && agentCount >= 1 && agentCount <= 100);
assert.equal(pool.total_people, poolSize);
assert.equal(pool.person_ids.length, pool.target_pool_size);
async function context() {
  const ctx = await request.newContext({ baseURL: 'http://web:8080', timeout: 10000 });
  contexts.push(ctx); return ctx;
}
async function post(ctx, path, data, status) {
  const response = await ctx.post(path, { data });
  await captureSecrets(response);
  assert.equal(response.status(), status, path);
  const body = await response.json();
  if (body.accept_path) remember(body.accept_path.split('/').at(-1));
  await response.dispose(); return body;
}
async function snapshot() {
  const { rows } = await db.query(`SELECT
    (SELECT coalesce(sum(calls),0)::float8 FROM pg_stat_statements WHERE userid=(SELECT oid FROM pg_roles WHERE rolname='crm_app')) AS executions,
    (SELECT coalesce(sum(total_exec_time),0)::float8 FROM pg_stat_statements WHERE userid=(SELECT oid FROM pg_roles WHERE rolname='crm_app')) AS execution_ms,
    xact_commit::float8,xact_rollback::float8,blks_read::float8,blks_hit::float8,temp_bytes::float8,deadlocks::float8
    FROM pg_stat_database WHERE datname=current_database()`);
  return rows[0];
}
async function activity() {
  const { rows } = await db.query(`SELECT count(*)::int AS connections,
    count(*) FILTER (WHERE state='active')::int AS active,
    count(*) FILTER (WHERE state='idle in transaction')::int AS idle_in_transaction,
    count(*) FILTER (WHERE wait_event_type='Lock')::int AS lock_waiters
    FROM pg_stat_activity WHERE usename='crm_app'`);
  return { at_ms: Date.now(), ...rows[0] };
}
function percentile(values, p) { return values[Math.max(0, Math.ceil(values.length * p) - 1)] ?? null; }
function randomPersonIndex(state) {
  // Independent deterministic streams: reproducible random traversal without
  // serial cycles or one global RNG shared by all agents.
  state.value = (Math.imul(state.value, 1664525) + 1013904223) >>> 0;
  return state.value % pool.person_ids.length;
}
async function phase(name, selected, durationMs) {
  const before = await snapshot(), samples = [], requests = [];
  const started = Date.now(), deadline = performance.now() + durationMs;
  let monitoring = true;
  const monitor = (async () => {
    while (monitoring) { samples.push(await activity()); await new Promise(r => setTimeout(r, 500)); }
  })();
  await Promise.all(selected.map(async (agent, index) => {
    const state = { value: (0x9e3779b9 ^ ((index + 1) * 0x85ebca6b)) >>> 0 };
    while (performance.now() < deadline) {
      const personIndex = randomPersonIndex(state);
      const person = pool.person_ids[personIndex];
      const start = performance.now();
      let status = 0, valid = false, payloadBytes = 0;
      try {
        const response = await agent.ctx.get('/api/people/' + person);
        status = response.status();
        payloadBytes = Number(response.headers()['content-length'] || 0);
        if (status === 200) {
          const body = await response.json();
          valid = body.person?.id === person
            && body.contact_methods?.length === pool.expected_detail.contact_methods
            && body.tags?.length === pool.expected_detail.tags
            && body.tasks?.length === pool.expected_detail.tasks
            && body.history?.length === pool.expected_detail.history;
        }
        await response.dispose();
      } catch { /* Count transport/timeouts without logging request credentials. */ }
      requests.push({ agent: index + 1, person_index: personIndex + 1, status, valid,
        payload_bytes: payloadBytes, elapsed_ms: performance.now() - start });
    }
  }));
  const finished = Date.now();
  monitoring = false; await monitor;
  const after = await snapshot();
  const timings = requests.map(r => r.elapsed_ms).sort((a,b) => a-b);
  const payloads = requests.filter(r => r.payload_bytes > 0).map(r => r.payload_bytes).sort((a,b) => a-b);
  const result = { name, agents: selected.length, started_ms: started, finished_ms: finished,
    seconds: (finished-started)/1000, requests: requests.length,
    successful: requests.filter(r => r.status === 200 && r.valid).length,
    invalid_200: requests.filter(r => r.status === 200 && !r.valid).length,
    statuses: Object.fromEntries([...new Set(requests.map(r => r.status))].map(status => [status, requests.filter(r => r.status === status).length])),
    requests_per_second: requests.length/((finished-started)/1000),
    latency_ms: { p50: percentile(timings,.5), p95: percentile(timings,.95), p99: percentile(timings,.99), max: timings.at(-1) },
    payload_bytes: { p50: percentile(payloads,.5), max: payloads.at(-1) ?? null },
    postgres_delta: Object.fromEntries(Object.keys(before).map(key => [key, after[key]-before[key]])),
    activity: samples, samples: requests };
  writeFileSync('/artifacts/load-' + name + '.json', JSON.stringify(result, null, 2));
  console.log(JSON.stringify({ phase: name, agents: selected.length, requests: result.requests,
    successful: result.successful, statuses: result.statuses, rps: result.requests_per_second, latency_ms: result.latency_ms }));
  return result;
}

try {
  await db.connect();
  const admin = await context();
  await post(admin, '/api/session', { email: seed.actors.admin.email, password }, 200);
  for (let index=0; index<agentCount; index++) {
    const ctx = await context();
    const email = 'load-agent-' + index + '@e2e.test';
    const invitation = await post(admin, '/api/organization/invitations', { email, role: 'member' }, 201);
    const identity = await post(ctx, '/api/invitations/accept', {
      token: invitation.accept_path.split('/').at(-1), display_name: 'Load Agent ' + index, password }, 200);
    assert.equal(identity.organization.id, seed.actors.admin.organization.id);
    agents.push({ ctx, user: identity.user.id });
  }
  assert.equal(new Set(agents.map(agent => agent.user)).size, agentCount);
  const foreign = await context();
  await post(foreign, '/api/session', { email: seed.actors.casey.email, password }, 200);
  const denied = await foreign.get('/api/people/' + pool.person_ids[0]);
  assert.equal(denied.status(), 404); await denied.dispose();
  for (const agent of agents) {
    const response = await agent.ctx.get('/api/people/' + pool.person_ids[0]);
    assert.equal(response.status(),200);
    const body = await response.json();
    assert.equal(body.history.length, pool.expected_detail.history);
    assert.equal(body.tasks.length, pool.expected_detail.tasks);
    await response.dispose();
  }
  const results = [];
  results.push(await phase('baseline', agents.slice(0,1), durationSeconds * 1000));
  results.push(await phase('hundred-agents', agents, durationSeconds * 1000));
  for (const result of results) {
    result.person_coverage = [];
    for (let index = 1; index <= result.agents; index++) {
      const visits = result.samples.filter(sample => sample.agent === index);
      const distinct = new Set(visits.map(sample => sample.person_index)).size;
      result.person_coverage.push({ agent: index, distinct_persons: distinct, requests: visits.length });
    }
  }
  const finalState = (await db.query(`SELECT (SELECT count(*)::int FROM person) AS people,
    (SELECT count(*)::int FROM inquiry) AS inquiries,
    (SELECT count(*)::int FROM inquiry_received) AS inquiry_facts,
    (SELECT count(*)::int FROM stage_changed) AS stage_facts,
    (SELECT count(*)::int FROM note) AS notes,
    (SELECT count(*)::int FROM task) AS tasks,
    (SELECT count(*)::int FROM contact_attempted) AS contact_facts,
    (SELECT count(*)::int FROM person_detail_projection) AS projections`)).rows[0];
  pool.expected_state.projections = pool.target_pool_size;
  assert.deepEqual(finalState, pool.expected_state);
  writeFileSync('/artifacts/load-summary.json', JSON.stringify({ fixture: `${agentCount} distinct member identities, one Organization, ${poolSize} Persons, ${pool.target_pool_size} ${pool.fixture_kind} random-access targets`,
    workload: 'Each agent uses an independent deterministic pseudo-random stream over the configured random-access target pool on every request. Closed-loop HTTP GET through nginx and the real API; one in-flight request per agent; no think time; no browser rendering or UI fanout',
    duration_seconds_per_condition: durationSeconds, pool_size: poolSize, agent_count: agentCount,
    sql_profile: false, foreign_tenant_denied: true, final_state: finalState,
    phases: results.map(({ samples, activity, ...rest }) => rest) }, null, 2));
} finally { await db.end(); for (const ctx of contexts) await ctx.dispose(); }
