import { test, expect } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { createJourney } from '../support/journey.mjs';
import { connected, mutation, propagated } from '../support/actions.mjs';
import { database, evidence, remember, captureSecrets } from '../support/evidence.mjs';
const names = JSON.parse(readFileSync(new URL('./registry.json', import.meta.url))).routing.steps;

test('routing-v1: unattended email → routing → unresolved workbench', async ({ browser }, testInfo) => {
  const seed = JSON.parse(readFileSync('/private/seed.json'));
  const journey = createJourney(browser, testInfo), db = await database();
  const { actors, allActors } = journey;
  const project = process.env.E2E_PROJECT, settings = '/api/organization/intake-settings';
  const people = [], rotations = [];
  let address, foreignAddress, defaultRaw, missingRaw, unresolvedId, discardAt;
  remember(process.env.E2E_INBOUND_SECRET);
  async function api(actor, method, path, data, status = 200) {
    const response = await actor.page.request.fetch(path, { method, data });
    await captureSecrets(response);
    const body = await response.json();
    if (body.address) {
      remember(body.address);
      const token = body.address.match(/-([a-z0-9]{8})@/i)?.[1]; if (token) remember(token);
    }
    evidence('http', { actor: actor.key, method, path, request: data, status: response.status(), response: body });
    expect(response.status(), method + ' ' + path).toBe(status); return body;
  }
  function mail(key, contact = true, suffix = '') {
    // The application's authored Cypress Bay pinned template, synthetic identities.
    // Recipient auth lives in the relay envelope, never this display header.
    return ['From: "Cypress Bay Realty" <forms@cypressbayrealty.com>',
      'To: synthetic@example.test', 'Subject: New contact form submission',
      'Message-ID: <' + project + '-' + key + suffix + '@example.test>',
      'Content-Type: text/plain; charset=utf-8', '',
      'Name: ' + key + ' ' + project, 'Email: ' + (contact ? key.toLowerCase() + '@example.test' : ''),
      'Phone:', 'Message: Synthetic routing sentinel ' + project + suffix, ''].join('\r\n');
  }
  async function deliver(raw, recipient = address, secret = process.env.E2E_INBOUND_SECRET, status = 200, outcome = 'accepted') {
    const response = await fetch('http://api:3001/inbound/email', { method: 'POST',
      headers: { 'content-type': 'application/json', authorization: 'Bearer ' + secret },
      body: JSON.stringify({ recipient, raw: Buffer.from(raw).toString('base64') }) });
    const body = await response.json();
    evidence('inbound', { status: response.status, response: body, bytes: Buffer.byteLength(raw),
      requestId: response.headers.get('x-request-id') });
    expect(response.status).toBe(status);
    expect(body).toEqual(status === 200 ? { status: outcome } : { error: 'unauthenticated' });
  }
  async function routed(key, assignee, strategy, org = seed.actors.alice.organization.id) {
    let person;
    await db.check(key + ' routing persisted', `SELECT p.id, p.assigned_user_id,
      (SELECT count(*)::int FROM inquiry WHERE person_id=p.id) AS inquiries,
      r.strategy, r.assignee_user_id, r.actor_kind, r.actor_user_id, r.on_behalf_of_user_id, r.origin
      FROM person p JOIN routing_decision r ON r.person_id=p.id
      WHERE p.organization_id=$1 AND p.first_name=$2 ORDER BY r.occurred_at DESC LIMIT 1`, [org, key], rows => {
      expect(rows).toHaveLength(1); person = rows[0].id;
      expect(rows[0]).toMatchObject({ assigned_user_id: assignee, assignee_user_id: assignee, strategy,
        actor_kind: 'system', actor_user_id: null, on_behalf_of_user_id: null, origin: 'webhook' });
    });
    if (!people.includes(person)) people.push(person); return person;
  }
  async function mode(label, value) {
    await actors.admin.page.goto('/manage/intake');
    await actors.admin.page.getByTestId('intake-routing-mode').click();
    await mutation(actors.admin, settings, () => actors.admin.page.getByRole('option', { name: label, exact: true }).click(),
      200, { intake_routing_mode: value }, 'PUT');
    await actors.admin.page.reload();
    await expect(actors.admin.page.getByTestId('intake-routing-mode')).toContainText(label);
  }
  async function ready(actor, url, readPath) {
    const since = Date.now();
    await actor.page.goto(url);
    await connected(actor, since);
    await expect.poll(() => actor.recorder.http.find(r => r.started >= since &&
      r.method === 'GET' && r.path === readPath && r.status === 200)).toBeTruthy();
  }
  async function unresolvedEvent(actor, since) {
    await expect.poll(() => actor.recorder.frames.find(f => f.at >= since &&
      f.message.push?.pub?.data?.type === 'intake.unresolved_changed' &&
      f.message.push.pub.data.data.raw_payload_id === unresolvedId)).toBeTruthy();
    const frame = actor.recorder.frames.find(f => f.at >= since && f.message.push?.pub?.data?.data?.raw_payload_id === unresolvedId);
    expect(Object.keys(frame.message.push.pub.data.data)).toEqual(['raw_payload_id']);
    await expect.poll(() => actor.recorder.http.find(r => r.started >= frame.at && r.path === '/api/intake/unresolved' && r.status === 200)).toBeTruthy();
  }
  const actions = [
    async () => {
      expect(seed.version).toBe('routing-v1');
      for (const key of ['admin', 'alice', 'blair', 'casey']) await journey.login(key, seed.actors[key]);
      address = (await api(actors.admin, 'GET', '/api/organization/intake-address')).address;
      foreignAddress = (await api(actors.casey, 'GET', '/api/organization/intake-address')).address;
      expect(address).not.toBe(foreignAddress);
      await actors.admin.page.goto('/manage/intake');
      await expect(actors.admin.page.getByTestId('intake-unassigned-warning')).toBeVisible();
      await actors.admin.page.getByTestId('intake-routing-mode').click();
      await actors.admin.page.getByRole('option', { name: 'Default assignee', exact: true }).click();
      await actors.admin.page.getByTestId('intake-default-assignee').click();
      await mutation(actors.admin, settings, () => actors.admin.page.getByRole('option', { name: 'Alice Agent', exact: true }).click(), 200,
        { intake_routing_mode: 'default_assignee', intake_default_assignee_user_id: actors.alice.identity.user.id }, 'PUT');
      expect(await api(actors.alice, 'PUT', settings, { intake_routing_mode: 'unassigned', intake_default_assignee_user_id: null }, 403)).toEqual({ error: 'forbidden' });
      expect((await api(actors.casey, 'GET', settings)).intake_routing_mode).toBe('unassigned');
    },
    async () => {
      defaultRaw = mail('Default'); const since = Date.now(); await deliver(defaultRaw);
      const id = await routed('Default', actors.alice.identity.user.id, 'organization_default');
      await propagated(actors.alice, 'inquiry_received', id, since, '/api/today');
      await expect(actors.alice.page.getByRole('row').filter({ hasText: 'Default ' + project })).toBeVisible();
      await expect(actors.blair.page.getByText('Default ' + project, { exact: true })).toHaveCount(0);
      await actors.blair.page.goto('/people/' + id);
      await expect(actors.blair.page.getByRole('heading', { name: 'Default ' + project, exact: true })).toBeVisible();
    },
    async () => {
      await deliver(defaultRaw); await deliver(defaultRaw, 'invalid-address', undefined, 200, 'rejected');
      await deliver(defaultRaw, address, 'incorrect-transport-secret', 401);
      await db.check('replay and invalid transport have no new writes', `SELECT
        (SELECT count(*)::int FROM person) AS people, (SELECT count(*)::int FROM inquiry) AS inquiries,
        (SELECT count(*)::int FROM raw_payload) AS raw`, [], rows => expect(rows).toEqual([{ people: 1, inquiries: 1, raw: 1 }]));
      // Same bytes in a second tenant are a separate delivery, a positive isolation control.
      await deliver(defaultRaw, foreignAddress);
      const other = await routed('Default', null, 'unassigned', actors.casey.identity.organization.id);
      expect(other).not.toBe(people[0]);
      await actors.casey.page.goto('/people/' + other);
      await expect(actors.casey.page.getByRole('heading', { name: 'Default ' + project, exact: true })).toBeVisible();
    },
    async () => {
      await mode('Unassigned', 'unassigned'); await ready(actors.blair, '/people', '/api/people');
      const since = Date.now(); await deliver(mail('Unassigned'));
      const id = await routed('Unassigned', null, 'unassigned');
      await propagated(actors.blair, 'inquiry_received', id, since, '/api/people');
      await expect(actors.blair.page.getByText('Unassigned ' + project, { exact: true })).toBeVisible();
      for (const key of ['admin', 'alice', 'blair']) expect((await api(actors[key], 'GET', '/api/today')).items.some(x => x.person.id === id)).toBe(false);
    },
    async () => {
      await mode('Round-robin', 'round_robin');
      await db.check('canonical active membership ordering', `SELECT user_id FROM organization_membership
        WHERE organization_id=$1 AND status='active' ORDER BY created_at,user_id`, [actors.admin.identity.organization.id],
      rows => rotations.push(...rows.map(r => r.user_id)));
      expect(rotations).toHaveLength(3);
      for (let index = 0; index < 4; index++) {
        const key = 'Rotation' + index, raw = mail(key); await deliver(raw);
        await routed(key, rotations[index % rotations.length], 'round_robin');
        await deliver(raw);
        await db.check('duplicate does not consume rotation slot ' + index,
          'SELECT last_assigned_user_id FROM intake_rotation WHERE organization_id=$1', [actors.admin.identity.organization.id],
        rows => expect(rows).toEqual([{ last_assigned_user_id: rotations[index % rotations.length] }]));
      }
      for (let index = 0; index < 3; index++) {
        const key = Object.keys(actors).find(k => actors[k].identity.user.id === rotations[index]);
        await actors[key].page.goto('/today');
        await expect(actors[key].page.getByRole('row').filter({ hasText: 'Rotation' + index + ' ' + project })).toBeVisible();
      }
    },
    async () => {
      await deliver(mail('Default', true, '-repeat'));
      await routed('Default', actors.alice.identity.user.id, 'kept_existing');
      await db.check('repeat inquiry preserves ownership and rotation cursor', `SELECT
        (SELECT count(*)::int FROM inquiry WHERE person_id=$1) AS inquiries,
        (SELECT count(*)::int FROM assignment_changed WHERE person_id=$1) AS assignments,
        (SELECT last_assigned_user_id FROM intake_rotation WHERE organization_id=$2) AS anchor`,
      [people[0], actors.admin.identity.organization.id], rows => expect(rows).toEqual([
        { inquiries: 2, assignments: 1, anchor: rotations[0] },
      ]));
    },
    async () => {
      for (const key of ['admin', 'alice']) await ready(actors[key], '/intake/unresolved', '/api/intake/unresolved');
      missingRaw = mail('Missing', false); const since = Date.now(); await deliver(missingRaw);
      const queue = await api(actors.alice, 'GET', '/api/intake/unresolved');
      expect(queue.items).toHaveLength(1); unresolvedId = queue.items[0].id;
      expect(queue.items[0].reason).toBe('no_contact_method');
      expect(JSON.stringify(queue)).not.toContain('Synthetic routing sentinel');
      await unresolvedEvent(actors.alice, since);
      await expect(actors.alice.page.getByRole('row').filter({ hasText: 'email' })).toBeVisible();
      for (const [method, suffix] of [['GET', ''], ['POST', '/retry'], ['POST', '/discard']]) {
        expect(await api(actors.alice, method, '/api/intake/unresolved/' + unresolvedId + suffix, undefined, 403)).toEqual({ error: 'forbidden' });
        expect(await api(actors.casey, method, '/api/intake/unresolved/' + unresolvedId + suffix, undefined, 404)).toEqual({ error: 'not_found' });
      }
      await actors.admin.page.getByRole('row').filter({ hasText: 'email' }).click();
      await expect(actors.admin.page.getByRole('dialog').locator('pre')).toContainText('Synthetic routing sentinel ' + project);
    },
    async () => {
      const base = '/api/intake/unresolved/' + unresolvedId;
      const retried = await mutation(actors.admin, base + '/retry', () => actors.admin.page.getByRole('button', { name: 'Try again', exact: true }).click());
      expect(retried.body).toMatchObject({ status: 'unresolved', reason: 'no_contact_method' });
      await expect(actors.admin.page.getByText('Still unresolved', { exact: false })).toBeVisible();
      await actors.admin.page.getByRole('button', { name: 'Discard', exact: true }).click();
      const since = Date.now();
      await mutation(actors.admin, base + '/discard', () => actors.admin.page.getByRole('dialog', { name: 'Discard this entry?', exact: true }).getByRole('button', { name: 'Discard', exact: true }).click());
      await unresolvedEvent(actors.alice, since);
      await expect(actors.alice.page.getByText('No unresolved leads', { exact: true })).toBeVisible();
      await db.check('discard attribution retained', `SELECT resolution, discarded_by_user_id, discarded_at::text FROM raw_payload WHERE id=$1`, [unresolvedId], rows => {
        expect(rows[0]).toMatchObject({ resolution: 'discarded', discarded_by_user_id: actors.admin.identity.user.id }); discardAt = rows[0].discarded_at;
      });
      await deliver(missingRaw);
      expect(await api(actors.admin, 'POST', base + '/discard')).toEqual({ status: 'discarded' });
      expect(await api(actors.admin, 'POST', base + '/retry', undefined, 409)).toEqual({ error: 'discarded' });
      expect(await api(actors.admin, 'GET', base, undefined, 404)).toEqual({ error: 'not_found' });
      await actors.alice.page.reload(); await expect(actors.alice.page.getByText('No unresolved leads', { exact: true })).toBeVisible();
      await db.check('discarded replay never resurrects or changes attribution', 'SELECT resolution, discarded_at::text FROM raw_payload WHERE id=$1',
        [unresolvedId], rows => expect(rows).toEqual([{ resolution: 'discarded', discarded_at: discardAt }]));
    },
    async () => {
      await db.check('exact routing tenant canaries', `SELECT
        (SELECT count(*)::int FROM person) AS people, (SELECT count(*)::int FROM inquiry) AS inquiries,
        (SELECT count(*)::int FROM routing_decision) AS routing,
        (SELECT count(*)::int FROM raw_payload) AS raw,
        (SELECT count(*)::int FROM raw_payload WHERE resolution='discarded') AS discarded,
        (SELECT count(*)::int FROM intake_rotation) AS rotations,
        (SELECT count(*)::int FROM person WHERE last_name<>$1) AS foreign_run_data`, [project],
      rows => expect(rows).toEqual([{ people: 7, inquiries: 8, routing: 8, raw: 9, discarded: 1, rotations: 1, foreign_run_data: 0 }]));
      await db.check('raw payload remains encrypted and preserved', `SELECT count(*)::int AS n FROM raw_payload
        WHERE octet_length(ciphertext)>0 AND octet_length(nonce)>0
          AND position(convert_to($1,'UTF8') IN ciphertext)=0 AND payload_format='rfc822_v1'`,
      ['Synthetic routing sentinel ' + project], rows => expect(rows[0].n).toBe(9));
      for (const actor of allActors) {
        await actor.recorder.flush(); expect(actor.recorder.errors).toEqual([]);
        for (const frame of actor.recorder.frames) {
          if (frame.message.connect?.subs) expect(Object.keys(frame.message.connect.subs)).toEqual(['org:' + actor.identity.organization.id]);
          if (frame.message.push?.pub) {
            expect(frame.message.push.pub.data.organization_id).toBe(actor.identity.organization.id);
            expect(JSON.stringify(frame.message)).not.toContain('Synthetic routing sentinel');
          }
        }
      }
      const mock = await (await fetch('http://mocks:9000/evidence')).json();
      expect(mock.project).toBe(project); expect(mock.requests.filter(r => r.kind === 'external')).toEqual([]); evidence('mocks', mock);
      writeFileSync('/artifacts/isolation.json', JSON.stringify({ project,
        organizations: [actors.admin.identity.organization.id, actors.casey.identity.organization.id], people, externalRequests: 0 }));
    },
  ];
  try { await journey.run(names, actions); } finally { await journey.close(); await db.close(); }
});
