import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { request } from '@playwright/test';
import { captureSecrets, database, evidence } from './evidence.mjs';

const ctx = await request.newContext({ baseURL: 'http://web:8080' });
const db = await database();
try {
  const response = await ctx.post('/api/session', { data: {
    email: 'platform@e2e.test', password: process.env.E2E_PASSWORD,
  } });
  await captureSecrets(response);
  assert.equal(response.status(), 200);
  const platform = await response.json();
  assert.equal(platform.platform_admin, true);
  assert.equal(platform.organization, null);
  evidence('seed-http', { method: 'POST', path: '/api/session', status: 200, response: platform });
  await db.check('workspace-v1 starts before workspace creation', `SELECT
    (SELECT count(*)::int FROM app_user) AS users,
    (SELECT count(*)::int FROM platform_admin) AS platform_admins,
    (SELECT count(*)::int FROM organization) AS organizations,
    (SELECT count(*)::int FROM organization_membership) AS memberships,
    (SELECT count(*)::int FROM person) AS people,
    (SELECT count(*)::int FROM _sqlx_migrations WHERE success) AS migrations`, [], rows =>
    assert.deepEqual(rows[0], { users: 1, platform_admins: 1, organizations: 0,
      memberships: 0, people: 0, migrations: Number(process.env.E2E_MIGRATION_COUNT) }));
  const seed = { version: 'workspace-v1', actors: { platform } };
  writeFileSync('/private/seed.json', JSON.stringify(seed), { mode: 0o600 });
  writeFileSync('/artifacts/seed.json', JSON.stringify(seed));
  console.log('workspace-v1 verified: platform identity only, no Organizations or members.');
} finally { await ctx.dispose(); await db.close(); }
