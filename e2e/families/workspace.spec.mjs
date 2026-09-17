import { test, expect } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { createJourney } from '../support/journey.mjs';
import { connected, mutation, addLead, propagated } from '../support/actions.mjs';
import { database, evidence, remember, captureSecrets } from '../support/evidence.mjs';

const names = JSON.parse(readFileSync(new URL('./registry.json', import.meta.url))).workspace.steps;

test('workspace-v1: onboarding → membership → access recovery', async ({ browser }, testInfo) => {
  const seed = JSON.parse(readFileSync('/private/seed.json'));
  const journey = createJourney(browser, testInfo), db = await database();
  const { actors, allActors } = journey;
  const project = process.env.E2E_PROJECT;
  let org, foreignOrg, firstInvite, foreignInvite, personId, switchedAt;
  const orgName = 'Workspace ' + project, otherName = 'Other ' + project;
  const history = {};

  async function api(actor, method, path, data, status, code) {
    const response = await actor.page.request.fetch(path, { method, data });
    await captureSecrets(response);
    const body = response.status() === 204 ? null : await response.json();
    if (body?.accept_path) remember(body.accept_path.split('/').at(-1));
    evidence('http', { actor: actor.key, method, path, request: data, status: response.status(),
      response: body, responseHeaders: { requestId: response.headers()['x-request-id'] } });
    expect(response.status(), method + ' ' + path).toBe(status);
    if (code) expect(body).toEqual({ error: code });
    return body;
  }
  async function createOrg(name) {
    const actor = actors.platform;
    await actor.page.goto('/platform');
    await actor.page.getByRole('button', { name: 'New Organization', exact: true }).click();
    await actor.page.getByRole('dialog').getByLabel('Name', { exact: true }).fill(name);
    const result = await mutation(actor, '/api/platform/organizations', () =>
      actor.page.getByRole('dialog').getByRole('button', { name: 'Create', exact: true }).click(), 201, { name });
    const organization = result.body.organization;
    expect(organization).toMatchObject({ name, admin_count: 0, member_count: 0, state: 'needs_attention' });
    await actor.page.goto('/platform/organizations/' + organization.id);
    await expect(actor.page.getByRole('heading', { name, exact: true })).toBeVisible();
    return organization;
  }
  async function linkPanel(actor, result) {
    remember(result.accept_path.split('/').at(-1));
    const dialog = actor.page.getByRole('dialog');
    await expect(dialog.getByRole('heading', { name: 'Invitation sent' })).toBeVisible();
    await expect(dialog.locator('input[readonly]')).toHaveValue('http://web:8080' + result.accept_path);
    await expect(dialog.locator('input[readonly]')).toHaveCSS('-webkit-text-security', 'disc');
    await dialog.getByRole('button', { name: 'Done', exact: true }).click();
    await expect(dialog).toHaveCount(0);
    return result;
  }
  async function invite(actor, email, organization = null) {
    const path = organization ? '/api/platform/organizations/' + organization.id + '/invitations' : '/api/organization/invitations';
    await actor.page.goto(organization ? '/platform/organizations/' + organization.id : '/manage/members');
    await actor.page.getByRole('button', { name: organization ? 'Invite admin' : 'Invite', exact: true }).click();
    await actor.page.getByRole('dialog').getByLabel('Email', { exact: true }).fill(email);
    const result = await mutation(actor, path, () => actor.page.getByRole('dialog')
      .getByRole('button', { name: 'Send invite', exact: true }).click(), 201,
    { email, role: organization ? 'admin' : 'member' });
    return linkPanel(actor, result.body);
  }
  async function accept(key, invitation, displayName) {
    const actor = await journey.createActor(key);
    await actor.page.goto(invitation.accept_path);
    await expect(actor.page.getByText('has invited', { exact: false })).toBeVisible();
    await actor.page.getByLabel('Display name', { exact: true }).fill(displayName);
    await actor.page.getByLabel('Password', { exact: true }).fill(process.env.E2E_PASSWORD);
    const result = await mutation(actor, '/api/invitations/accept', () =>
      actor.page.getByRole('button', { name: 'Create account', exact: true }).click(), 200,
    { token: invitation.accept_path.split('/').at(-1), display_name: displayName });
    actor.identity = result.body;
    expect(actor.identity.user.email).toBe(invitation.invitation.email);
    expect(actor.identity.organization.id).toBe(invitation.invitation.organization_id ?? (key === 'casey' ? foreignOrg.id : org.id));
    history[key] = ['invitation'];
    await memberState(key, actor.identity.organization.role, 'active', actor.identity.user.id);
    await actor.page.waitForURL('**/today');
    await connected(actor);
    await expect(actor.page.getByText("You're all caught up", { exact: true })).toBeVisible();
    return actor;
  }
  async function memberState(key, role, status, changedBy = null, origin = 'web_session') {
    const target = actors[key].identity;
    await db.check(key + ' membership and history', `SELECT m.role, m.status,
      (SELECT array_agg(reason ORDER BY occurred_at, id) FROM membership_changed
       WHERE organization_id=m.organization_id AND user_id=m.user_id) AS reasons
      FROM organization_membership m WHERE m.organization_id=$1 AND m.user_id=$2`,
    [target.organization.id, target.user.id], rows => expect(rows).toEqual([{ role, status, reasons: history[key] }]));
    if (changedBy) await db.check(key + ' mutation actor attribution', `SELECT actor_user_id, organization_id, origin
      FROM membership_changed WHERE organization_id=$1 AND user_id=$2 ORDER BY occurred_at DESC, id DESC LIMIT 1`,
    [target.organization.id, target.user.id], rows => expect(rows).toEqual([
      { actor_user_id: changedBy, organization_id: target.organization.id, origin },
    ]));
  }
  async function change(actor, targetKey, action, status = 200, platform = false) {
    const target = actors[targetKey].identity;
    const role = ['Promote', 'Demote'].includes(action);
    const data = role ? { role: action === 'Promote' ? 'admin' : 'member' } : { status: action === 'Deactivate' ? 'inactive' : 'active' };
    await actor.page.goto(platform ? '/platform/organizations/' + target.organization.id : '/manage/members');
    await actor.page.getByRole('row').filter({ hasText: target.user.email })
      .getByRole('button', { name: action, exact: true }).click();
    const path = (platform ? '/api/platform/organizations/' + target.organization.id : '/api/organization') +
      '/members/' + target.user.id + (role ? '/role' : '/status');
    const result = await mutation(actor, path, () => actor.page.getByRole('dialog')
      .getByRole('button', { name: action, exact: true }).click(), status, data, 'PUT');
    if (status === 200) {
      history[targetKey].push(action.toLowerCase());
      await expect(actor.page.getByRole('dialog')).toHaveCount(0);
      expect(result.body.member).toMatchObject({ user_id: target.user.id, ...data });
      await memberState(targetKey, result.body.member.role, result.body.member.status,
        actor.identity.user.id, platform ? 'platform' : 'web_session');
    } else {
      expect(result.body).toEqual({ error: 'last_admin' });
      await expect(actor.page.getByRole('alert')).toHaveText('You are the last active admin. Promote someone else first.');
      await actor.page.getByRole('dialog').getByRole('button', { name: 'Cancel', exact: true }).click();
    }
  }
  async function deadLink(invitation, status, text) {
    const actor = actors.visitor ?? await journey.createActor('visitor');
    const response = actor.page.waitForResponse(r => new URL(r.url()).pathname === '/api/invitations/preview');
    await actor.page.goto(invitation.accept_path);
    expect((await response).status()).toBe(status);
    await expect(actor.page.getByText(text, { exact: true })).toBeVisible();
    await expect(actor.page.getByRole('button', { name: 'Create account', exact: true })).toHaveCount(0);
  }

  const actions = [
    async () => {
      expect(seed.version).toBe('workspace-v1');
      const actor = await journey.login('platform', seed.actors.platform);
      expect(actor.identity.organization).toBeNull();
      await expect(actor.page.getByText('No Organizations yet.', { exact: true })).toBeVisible();
      for (const path of ['/api/people', '/api/today', '/api/stages']) await api(actor, 'GET', path, undefined, 401, 'unauthenticated');
      await api(actor, 'POST', '/api/realtime/token', {}, 401, 'unauthenticated');
      expect(actor.recorder.sockets).toHaveLength(0);
    },
    async () => {
      org = await createOrg(orgName); foreignOrg = await createOrg(otherName);
      await db.check('workspace creation and default stages', `SELECT o.id, o.name,
        (SELECT count(*)::int FROM stage WHERE organization_id=o.id) AS stages,
        (SELECT count(*)::int FROM organization_created WHERE organization_id=o.id AND actor_user_id=$1 AND origin='platform') AS facts
        FROM organization o ORDER BY name`, [actors.platform.identity.user.id], rows => expect(rows).toEqual([
          { id: foreignOrg.id, name: otherName, stages: 9, facts: 1 }, { id: org.id, name: orgName, stages: 9, facts: 1 },
        ]));
      await api(actors.platform, 'POST', '/api/platform/organizations', { name: orgName }, 409, 'organization_name_taken');
      await db.check('duplicate creation has no effects', 'SELECT count(*)::int AS n FROM organization', [], rows => expect(rows[0].n).toBe(2));
    },
    async () => {
      firstInvite = await invite(actors.platform, 'admin@e2e.test', org);
      const pending = await api(actors.platform, 'GET', '/api/platform/organizations/' + org.id, undefined, 200);
      expect(pending.organization).toMatchObject({ state: 'pending_first_admin', pending_admin_invitations: 1 });
      await accept('admin', firstInvite, 'Workspace Admin');
      await deadLink(firstInvite, 409, 'This invitation has already been used.');
      await api(actors.visitor, 'POST', '/api/invitations/accept', { token: firstInvite.accept_path.split('/').at(-1),
        display_name: 'Replay', password: process.env.E2E_PASSWORD }, 409, 'invitation_used');
      await memberState('admin', 'admin', 'active');
      await actors.admin.page.reload();
      await expect(actors.admin.page.getByRole('link', { name: 'Members', exact: true })).toBeVisible();
    },
    async () => {
      for (const [key, name] of [['alice', 'Alice Agent'], ['blair', 'Blair Agent']]) {
        await accept(key, await invite(actors.admin, key + '@e2e.test'), name);
      }
      await api(actors.alice, 'POST', '/api/organization/invitations', { email: 'denied@e2e.test', role: 'admin' }, 403, 'forbidden');
      await api(actors.alice, 'PUT', '/api/organization/members/' + actors.alice.identity.user.id + '/role', { role: 'admin' }, 403, 'forbidden');
      await api(actors.admin, 'GET', '/api/platform/organizations', undefined, 403, 'forbidden');
      await db.check('denied invitations and promotions have no effects', `SELECT
        (SELECT count(*)::int FROM invitation) AS invitations,
        (SELECT count(*)::int FROM membership_changed) AS facts`, [], rows => expect(rows[0]).toEqual({ invitations: 3, facts: 3 }));
      await actors.alice.page.goto('/manage/members');
      await actors.alice.page.waitForURL('**/today');
    },
    async () => {
      await change(actors.admin, 'admin', 'Demote', 409);
      await change(actors.admin, 'admin', 'Deactivate', 409);
      await memberState('admin', 'admin', 'active');
      await api(actors.admin, 'GET', '/api/me', undefined, 200);
    },
    async () => {
      await change(actors.admin, 'alice', 'Promote');
      await actors.alice.page.goto('/manage/members');
      await expect(actors.alice.page.getByRole('button', { name: 'Invite', exact: true })).toBeVisible();
      await change(actors.admin, 'alice', 'Demote');
      await api(actors.alice, 'GET', '/api/organization/invitations', undefined, 403, 'forbidden');
      await actors.alice.page.reload(); await actors.alice.page.waitForURL('**/today');
      await memberState('alice', 'member', 'active');
    },
    async () => {
      const revoked = await invite(actors.admin, 'later@e2e.test');
      const row = actors.admin.page.getByRole('row').filter({ hasText: 'later@e2e.test' });
      await row.getByRole('button', { name: 'Revoke', exact: true }).click();
      await mutation(actors.admin, '/api/organization/invitations/' + revoked.invitation.id, () =>
        actors.admin.page.getByRole('dialog').getByRole('button', { name: 'Revoke', exact: true }).click(), 204, null, 'DELETE');
      await deadLink(revoked, 404, 'This invitation is no longer valid.');
      const pending = await invite(actors.admin, 'charlie@e2e.test');
      const result = await mutation(actors.admin, '/api/organization/invitations', () => actors.admin.page
        .getByRole('row').filter({ hasText: 'charlie@e2e.test' }).getByRole('button', { name: 'Re-issue', exact: true }).click(), 201);
      const replacement = await linkPanel(actors.admin, result.body);
      expect(replacement.invitation.id).not.toBe(pending.invitation.id);
      expect(replacement.accept_path).not.toBe(pending.accept_path);
      await deadLink(pending, 404, 'This invitation is no longer valid.');
      await accept('charlie', replacement, 'Charlie Member');
      await db.check('revoked and superseded links retain their resolution facts', `SELECT i.id, i.revoke_reason,
        (SELECT array_agg(outcome ORDER BY occurred_at) FROM invitation_resolved WHERE invitation_id=i.id) AS outcomes
        FROM invitation i WHERE i.id=ANY($1::uuid[]) ORDER BY i.revoke_reason`,
      [[revoked.invitation.id, pending.invitation.id]], rows => expect(rows).toEqual([
        { id: revoked.invitation.id, revoke_reason: 'revoked', outcomes: ['revoked'] },
        { id: pending.invitation.id, revoke_reason: 'superseded', outcomes: ['superseded'] },
      ]));
      await journey.closeActor(actors.charlie); await journey.closeActor(actors.visitor);
    },
    async () => {
      await actors.blair.page.goto('/people');
      const since = Date.now();
      const result = await addLead(actors.alice, { first: 'Retained', last: project, email: 'retained@example.test' });
      personId = result.body.person_id;
      await propagated(actors.blair, 'inquiry_received', personId, since, '/api/people');
      await expect(actors.blair.page.getByText('Retained ' + project, { exact: true })).toBeVisible();
      const deactivatedAt = Date.now();
      await change(actors.admin, 'alice', 'Deactivate');
      await expect.poll(() => actors.alice.recorder.sockets.some(s => s.closed >= deactivatedAt)).toBe(true);
      await actors.alice.page.reload(); await actors.alice.page.waitForURL('**/login**');
      await api(actors.alice, 'GET', '/api/people/' + personId, undefined, 401, 'unauthenticated');
      await journey.signIn(actors.alice, actors.alice.identity, 403);
      await expect(actors.alice.page.getByRole('alert')).toBeVisible();
      await db.check('deactivation revokes sessions and retains assigned data', `SELECT
        (SELECT count(*)::int FROM user_session WHERE user_id=$1 AND active_organization_id=$2 AND revoked_at IS NULL) AS live_sessions,
        (SELECT assigned_user_id FROM person WHERE id=$3) AS assignee,
        (SELECT count(*)::int FROM inquiry_received WHERE person_id=$3 AND actor_user_id=$1) AS facts`,
      [actors.alice.identity.user.id, org.id, personId], rows => expect(rows[0]).toEqual({ live_sessions: 0, assignee: actors.alice.identity.user.id, facts: 1 }));
      await change(actors.admin, 'alice', 'Reactivate');
      await api(actors.alice, 'GET', '/api/me', undefined, 401, 'unauthenticated');
      await journey.signIn(actors.alice, actors.alice.identity);
      await expect(actors.alice.page.getByText('Retained ' + project, { exact: true })).toBeVisible();
    },
    async () => {
      foreignInvite = await invite(actors.platform, 'casey@e2e.test', foreignOrg);
      await accept('casey', foreignInvite, 'Casey Other');
      await api(actors.admin, 'PUT', '/api/organization/members/' + actors.casey.identity.user.id + '/status', { status: 'inactive' }, 404, 'not_found');
      await api(actors.admin, 'DELETE', '/api/organization/invitations/' + foreignInvite.invitation.id, undefined, 404, 'not_found');
      await api(actors.casey, 'GET', '/api/people/' + personId, undefined, 404, 'not_found');
      await memberState('casey', 'admin', 'active');
      await db.check('foreign invitation untouched', 'SELECT accepted_user_id, revoked_at FROM invitation WHERE id=$1',
        [foreignInvite.invitation.id], rows => expect(rows).toEqual([{ accepted_user_id: actors.casey.identity.user.id, revoked_at: null }]));
    },
    async () => {
      await change(actors.platform, 'blair', 'Promote', 200, true);
      await actors.blair.page.goto('/manage/members');
      await expect(actors.blair.page.getByRole('button', { name: 'Invite', exact: true })).toBeVisible();
      const recovery = await invite(actors.platform, 'recovery@e2e.test', org);
      await accept('recovery', recovery, 'Recovery Admin');
      await api(actors.platform, 'GET', '/api/people/' + personId, undefined, 401, 'unauthenticated');
      await api(actors.platform, 'PUT', '/api/platform/organizations/' + org.id + '/members/' + actors.blair.identity.user.id + '/role',
        { role: 'member' }, 400, 'malformed_request');
      await memberState('blair', 'admin', 'active');
      await journey.closeActor(actors.recovery);
    },
    async () => {
      const actor = actors.alice;
      await actor.page.goto('/people/' + personId);
      await expect(actor.page.getByRole('heading', { name: 'Retained ' + project, exact: true })).toBeVisible();
      await mutation(actor, '/api/session', () => actor.page.getByRole('button', { name: 'Log out', exact: true }).click(), 204, null, 'DELETE');
      await actor.page.waitForURL('**/login**');
      await expect(actor.page.getByText('Retained ' + project, { exact: true })).toHaveCount(0);
      switchedAt = Date.now();
      await journey.signIn(actor, actors.casey.identity);
      await actor.page.goto('/people');
      await expect(actor.page.getByText('Retained ' + project, { exact: true })).toHaveCount(0);
      await api(actor, 'GET', '/api/people/' + personId, undefined, 404, 'not_found');
      expect(actor.identity.organization.id).toBe(foreignOrg.id);
      await actor.page.reload();
      await expect(actor.page.getByText(otherName, { exact: true })).toBeVisible();
    },
    async () => {
      for (const actor of allActors) {
        await actor.recorder.flush(); expect(actor.recorder.errors).toEqual([]);
        for (const frame of actor.recorder.frames) {
          const expectedOrg = actor.key === 'alice' && frame.at < switchedAt ? org.id : actor.identity?.organization?.id;
          if (frame.message.connect?.subs) expect(Object.keys(frame.message.connect.subs)).toEqual(['org:' + expectedOrg]);
          if (frame.message.push?.pub) expect(frame.message.push.pub.data.organization_id).toBe(expectedOrg);
        }
      }
      await db.check('exact family tenant and CRM canaries', `SELECT
        (SELECT array_agg(name ORDER BY name) FROM organization) AS organizations,
        (SELECT array_agg(last_name) FROM person) AS people,
        (SELECT count(*)::int FROM organization_membership) AS memberships,
        (SELECT count(*)::int FROM membership_changed) AS membership_facts,
        (SELECT count(*)::int FROM invitation) AS invitations,
        (SELECT count(*)::int FROM invitation_issued) AS issued,
        (SELECT count(*)::int FROM invitation_resolved) AS resolved`, [], rows => expect(rows[0]).toEqual({
          organizations: [otherName, orgName], people: [project], memberships: 6, membership_facts: 11,
          invitations: 8, issued: 8, resolved: 8,
        }));
      await db.check('acceptance and issue facts preserve tenant and actor attribution', `SELECT count(*)::int AS bad
        FROM invitation i JOIN invitation_issued issued ON issued.invitation_id=i.id
        LEFT JOIN invitation_resolved resolved ON resolved.invitation_id=i.id AND resolved.outcome='accepted'
        WHERE issued.organization_id<>i.organization_id OR issued.actor_user_id<>i.invited_by_user_id
           OR (resolved.id IS NOT NULL AND (resolved.actor_user_id<>i.accepted_user_id OR resolved.organization_id<>i.organization_id))`,
      [], rows => expect(rows[0].bad).toBe(0));
      const mock = await (await fetch('http://mocks:9000/evidence')).json();
      expect(mock.project).toBe(project);
      expect(mock.requests.filter(r => r.kind === 'external')).toEqual([]);
      evidence('mocks', mock);
      writeFileSync('/artifacts/isolation.json', JSON.stringify({ project,
        organizations: [org.id, foreignOrg.id], people: [personId], externalRequests: 0 }));
    },
  ];
  try { await journey.run(names, actions); } finally { await journey.close(); await db.close(); }
});
