import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { request } from '@playwright/test';
import { captureSecrets, remember, database, evidence } from './evidence.mjs';
export async function seedOperationalFamily({ version, setup = async () => ({}) }) {
const password = process.env.E2E_PASSWORD;
const actors = {};
const contexts = [];
const actorContexts = {};
async function context() {
  const ctx = await request.newContext({ baseURL: 'http://web:8080' }); contexts.push(ctx); return ctx;
}
async function post(ctx, path, data, status) {
  const response = await ctx.post(path, { data });
  await captureSecrets(response);
  assert.equal(response.status(), status, path);
  const body = await response.json();
  if (body.accept_path) remember(body.accept_path.split('/').at(-1));
  evidence('seed-http', { method: 'POST', path, status, response: body });
  return body;
}
async function accept(invitation, email, name) {
  const ctx = await context();
  const identity = await post(ctx, '/api/invitations/accept', {
    token: invitation.accept_path.split('/').at(-1), display_name: name, password,
  }, 200);
  assert.equal(identity.organization.workspace_mode, 'operational');
  assert.equal(identity.user.email, email);
  return { ctx, identity, email, name };
}
const db = await database();
try {
  const platform = await context();
  await post(platform, '/api/session', { email: 'platform@e2e.test', password }, 200);
  const org = await post(platform, '/api/platform/organizations', { name: 'Journey Realty' }, 201);
  const invite = await post(platform, '/api/platform/organizations/' + org.organization.id + '/invitations',
    { email: 'admin@e2e.test', role: 'admin' }, 201);
  const admin = await accept(invite, 'admin@e2e.test', 'Journey Admin');
  actors.admin = { ...admin.identity, email: admin.email, name: admin.name };
  actorContexts.admin = admin.ctx;
  for (const [key, name] of [['alice', 'Alice Agent'], ['blair', 'Blair Agent']]) {
    const email = key + '@e2e.test';
    const member = await accept(await post(admin.ctx, '/api/organization/invitations', { email, role: 'member' }, 201), email, name);
    actors[key] = { ...member.identity, email, name };
    actorContexts[key] = member.ctx;
  }
  const other = await post(platform, '/api/platform/organizations', { name: 'Other Realty' }, 201);
  const foreign = await accept(await post(platform, '/api/platform/organizations/' + other.organization.id + '/invitations',
    { email: 'casey@e2e.test', role: 'admin' }, 201), 'casey@e2e.test', 'Casey Other');
  actors.casey = { ...foreign.identity, email: foreign.email, name: foreign.name };
  actorContexts.casey = foreign.ctx;
  actorContexts.platform = platform;
  const stages = await (await admin.ctx.get('/api/stages')).json();
  assert.equal(stages.stages.length, 9);
  await db.check('assertion role cannot modify business tables', `SELECT
    has_table_privilege(current_user, 'person', 'SELECT') AS can_read,
    has_table_privilege(current_user, 'person', 'INSERT,UPDATE,DELETE,TRUNCATE') AS can_write`,
  [], rows => assert.deepEqual(rows[0], { can_read: true, can_write: false }));
  await db.check(version + ' operational prerequisites', `SELECT
    (SELECT count(*)::int FROM person) AS people,
    (SELECT count(*)::int FROM organization) AS organizations,
    (SELECT count(*)::int FROM organization_membership WHERE status='active') AS memberships,
    (SELECT count(*)::int FROM stage) AS stages,
    (SELECT count(*)::int FROM _sqlx_migrations WHERE success) AS migrations`, [], rows => {
      assert.deepEqual(rows[0], { people: 0, organizations: 2, memberships: 4, stages: 18,
        migrations: Number(process.env.E2E_MIGRATION_COUNT) });
    });
  const familyRequest = async (actorKey, method, path, data, status = 200) => {
    const response = await actorContexts[actorKey].fetch(path, { method, data });
    await captureSecrets(response);
    const body = response.status() === 204 ? null : await response.json();
    if (body?.accept_path) remember(body.accept_path.split('/').at(-1));
    evidence('seed-http', { actor: actorKey, method, path, request: data, status: response.status(), response: body });
    assert.equal(response.status(), status, method + ' ' + path);
    return body;
  };
  const extra = await setup({ actors, stages: stages.stages, request: familyRequest, db });
  const seed = { ...extra, version, actors, stages: stages.stages };
  writeFileSync('/private/seed.json', JSON.stringify(seed, null, 2), { mode: 0o600 });
  writeFileSync('/artifacts/seed.json', JSON.stringify(seed, null, 2));
  console.log(version + ' verified: isolated operational workspaces and family prerequisites.');
} finally { await db.close(); for (const ctx of contexts) await ctx.dispose(); }
}
