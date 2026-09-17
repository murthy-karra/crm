import { test, expect } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { database, evidence } from '../support/evidence.mjs';
import { createJourney } from '../support/journey.mjs';
import { connected, mutation, form, addLead, propagated, select } from '../support/actions.mjs';

const names = JSON.parse(readFileSync(new URL('./registry.json', import.meta.url))).leads.steps;

test('leads-v1: new lead → Today → contact → repeat inquiry → handoff', async ({ browser }, testInfo) => {
  const seed = JSON.parse(readFileSync('/private/seed.json'));
  const db = await database();
  const journey = createJourney(browser, testInfo);
  const { actors, allActors, closeActor } = journey;
  const login = key => journey.login(key, seed.actors[key]);
  let personId, inquiryId, originalSubmission, foreignPerson, retryPerson;
  let stageFacts = 1;
  const project = process.env.E2E_PROJECT;
  const lead = { first: 'Avery', last: project, email: 'avery@example.test', phone: '2025550148' };
  const personName = lead.first + ' ' + lead.last;

  async function checkpoint(name, { inquiries = 2, contacts = 1, assignments = 2,
    stages = stageFacts, assignee = seed.actors.blair.user.id, stage = 'Hot Prospect' } = {}) {
    return db.check(name, `SELECT p.id, p.organization_id, p.assigned_user_id, s.name AS stage,
      (SELECT count(*)::int FROM inquiry WHERE person_id=p.id) AS inquiries,
      (SELECT count(*)::int FROM inquiry_received WHERE person_id=p.id) AS inquiry_facts,
      (SELECT count(*)::int FROM routing_decision WHERE person_id=p.id) AS routing_facts,
      (SELECT count(*)::int FROM contact_attempted WHERE person_id=p.id) AS contacts,
      (SELECT count(*)::int FROM assignment_changed WHERE person_id=p.id) AS assignments,
      (SELECT count(*)::int FROM stage_changed WHERE person_id=p.id) AS stages,
      p.last_inquiry_at = (SELECT max(received_at) FROM inquiry WHERE person_id=p.id) AS inquiry_max_exact,
      p.last_contact_at IS NOT DISTINCT FROM (SELECT max(occurred_at) FROM contact_attempted WHERE person_id=p.id) AS contact_max_exact
      FROM person p JOIN stage s ON s.id=p.stage_id WHERE p.id=$1`, [personId], rows => {
        expect(rows).toEqual([{ id: personId, organization_id: seed.actors.alice.organization.id,
          assigned_user_id: assignee, stage, inquiries, inquiry_facts: inquiries, routing_facts: inquiries,
          contacts, assignments, stages, inquiry_max_exact: true, contact_max_exact: true }]);
      });
  }
  const actions = [
    async () => {
      expect(seed.version).toBe('leads-v1');
      for (const key of ['alice', 'blair', 'casey']) {
        const actor = await login(key);
        await expect(actor.page.getByText("You're all caught up", { exact: true })).toBeVisible();
      }
      await db.check('browser begins on verified empty book', 'SELECT count(*)::int AS n FROM person', [], rows => expect(rows[0].n).toBe(0));
      await actors.blair.page.goto('/people'); await actors.casey.page.goto('/people');
    },
    async () => {
      const since = Date.now();
      const { body, request } = await addLead(actors.alice, lead);
      expect(body).toMatchObject({ status: 'resolved', person_created: true, duplicate: false,
        assigned_user_id: seed.actors.alice.user.id, routing_strategy: 'actor_default' });
      personId = body.person_id; inquiryId = body.inquiry_id; originalSubmission = request.payload.submission_id;
      await propagated(actors.blair, 'inquiry_received', personId, since, '/api/people');
      await expect(actors.blair.page.getByText(personName, { exact: true })).toBeVisible();
      await checkpoint('initial inquiry and four facts committed', { inquiries: 1, contacts: 0,
        assignments: 1, assignee: seed.actors.alice.user.id, stage: 'Lead' });
      await db.check('contact methods retain the Person and tenant relationships',
        'SELECT kind, normalized_value, organization_id FROM contact_method WHERE person_id=$1 ORDER BY kind',
        [personId], rows => expect(rows).toEqual([
          { kind: 'email', normalized_value: lead.email, organization_id: seed.actors.alice.organization.id },
          { kind: 'phone', normalized_value: '+12025550148', organization_id: seed.actors.alice.organization.id },
        ]));
      await actors.alice.page.goto('/today');
      await expect(actors.alice.page.getByText(personName, { exact: true })).toBeVisible();
      await expect(actors.alice.page.getByText('New inquiry', { exact: true }).first()).toBeVisible();
    },
    async () => {
      await actors.blair.page.goto('/people/' + personId);
      await expect(actors.blair.page.getByRole('heading', { name: personName, exact: true })).toBeVisible();
      await actors.alice.page.getByRole('button', { name: 'Log contact', exact: true }).click();
      const since = Date.now();
      await mutation(actors.alice, '/api/people/' + personId + '/contact-attempts',
        () => actors.alice.page.getByRole('dialog').getByRole('button', { name: 'Log contact', exact: true }).click(),
        201, { channel: 'call', outcome: 'no_answer' });
      await propagated(actors.blair, 'contact_attempted', personId, since, '/api/people/' + personId);
      await expect(actors.alice.page.getByText(personName, { exact: true })).toHaveCount(0);
      await checkpoint('contact fact changes Today', { inquiries: 1, assignments: 1,
        assignee: seed.actors.alice.user.id, stage: 'Lead' });
    },
    async () => {
      const since = Date.now();
      const result = await addLead(actors.alice, lead, 'referral');
      expect(result.body).toMatchObject({ person_id: personId, person_created: false, duplicate: false,
        routing_strategy: 'kept_existing' });
      expect(result.body.inquiry_id).not.toBe(inquiryId);
      expect(result.request.payload.submission_id).not.toBe(originalSubmission);
      await propagated(actors.blair, 'inquiry_received', personId, since, '/api/people/' + personId);
      await db.check('original source survives repeat inquiry',
        'SELECT id, source FROM inquiry WHERE person_id=$1 ORDER BY received_at, id', [personId],
        rows => expect(rows).toEqual([{ id: inquiryId, source: 'website' }, { id: result.body.inquiry_id, source: 'referral' }]));
      await checkpoint('repeat inquiry preserves initial assignment/stage', { assignments: 1,
        assignee: seed.actors.alice.user.id, stage: 'Lead' });
      await actors.alice.page.goto('/today');
      await expect(actors.alice.page.getByText(personName, { exact: true })).toBeVisible();
    },
    async () => {
      await actors.blair.page.goto('/today');
      await expect(actors.blair.page.getByText(personName, { exact: true })).toHaveCount(0);
      await actors.alice.page.goto('/people/' + personId);
      const stage = seed.stages.find(s => s.name === 'Hot Prospect');
      let since = Date.now();
      await mutation(actors.alice, '/api/people/' + personId + '/stage',
        () => select(actors.alice, 'Stage', stage.name), 200, { stage_id: stage.id });
      stageFacts++;
      await propagated(actors.blair, 'stage_changed', personId, since, '/api/today');
      await checkpoint('stage persists before handoff', { assignments: 1, assignee: seed.actors.alice.user.id });
      since = Date.now();
      await mutation(actors.alice, '/api/people/' + personId + '/assignment',
        () => select(actors.alice, 'Assignee', 'Blair Agent'), 200, { assigned_user_id: seed.actors.blair.user.id });
      await propagated(actors.blair, 'assignment_changed', personId, since, '/api/today');
      await expect(actors.blair.page.getByText(personName, { exact: true })).toBeVisible();
      await checkpoint('handoff changes responsibility');
      await actors.alice.page.goto('/today');
      await expect(actors.alice.page.getByText(personName, { exact: true })).toHaveCount(0);
    },
    async () => {
      await actors.blair.page.reload();
      await expect(actors.blair.page.getByText(personName, { exact: true })).toBeVisible();
      await closeActor(actors.alice); await login('alice');
      await actors.alice.page.goto('/people/' + personId);
      await expect(actors.alice.page.getByRole('heading', { name: personName, exact: true })).toBeVisible();
      await expect(actors.alice.page.getByRole('combobox', { name: 'Assignee', exact: true }).first()).toContainText('Blair Agent');
      await checkpoint('fresh sessions see durable shared Person');
    },
    async () => {
      const actor = actors.casey;
      const denied = actor.page.waitForResponse(r => new URL(r.url()).pathname === '/api/people/' + personId);
      await actor.page.goto('/people/' + personId); expect((await denied).status()).toBe(404);
      await expect(actor.page.getByRole('heading', { name: personName, exact: true })).toHaveCount(0);
      const response = await actor.page.request.post('/api/people/' + personId + '/assignment',
        { data: { assigned_user_id: actor.identity.user.id } });
      expect(response.status()).toBe(404);
      evidence('http', { actor: 'casey', method: 'POST', path: '/api/people/' + personId + '/assignment',
        status: response.status(), request: { assigned_user_id: actor.identity.user.id }, response: await response.json() });
      await checkpoint('foreign write has zero effects');
      const since = Date.now();
      const own = await addLead(actor, { first: 'Foreign', last: project, email: 'foreign@example.test' });
      foreignPerson = own.body.person_id;
      await expect.poll(() => actor.recorder.frames.some(f => f.at >= since &&
        f.message.push?.pub?.data?.data?.person_id === foreignPerson)).toBe(true);
      await db.check('foreign canary is stored only in its own tenant',
        'SELECT organization_id FROM person WHERE id=$1', [foreignPerson],
        rows => expect(rows[0].organization_id).toBe(seed.actors.casey.organization.id));
    },
    async () => {
      const actor = actors.alice, bodyList = [];
      await actors.blair.page.goto('/people');
      await form(actor, { first: 'Retry', last: project, email: 'retry@example.test' }, 'manual');
      let committed;
      await actor.page.route('**/api/inquiries', async route => {
        bodyList.push(route.request().postDataJSON());
        if (bodyList.length === 1) {
          const response = await route.fetch();
          expect(response.status()).toBe(201); committed = await response.json();
          evidence('http', { actor: 'alice', method: 'POST', path: '/api/inquiries',
            request: bodyList[0], status: 201, response: committed, fault: 'response lost after real commit' });
          await route.abort('failed');
        } else await route.continue();
      });
      await actor.page.getByRole('button', { name: 'Add lead', exact: true }).click();
      await expect(actor.page.getByRole('alert')).toBeVisible();
      expect(committed.person_created).toBe(true); retryPerson = committed.person_id;
      await db.check('lost response already committed once',
        'SELECT count(*)::int AS n FROM inquiry WHERE person_id=$1', [retryPerson], rows => expect(rows[0].n).toBe(1));
      const result = await mutation(actor, '/api/inquiries',
        () => actor.page.getByRole('button', { name: 'Add lead', exact: true }).click(), 200);
      expect(result.body).toMatchObject({ duplicate: true, person_id: retryPerson, inquiry_id: committed.inquiry_id });
      expect(bodyList).toHaveLength(2); expect(bodyList[0]).toEqual(bodyList[1]);
      await actor.page.unroute('**/api/inquiries');
      await db.check('same identity has no duplicate inquiry or initial facts',
        `SELECT (SELECT count(*)::int FROM inquiry WHERE person_id=$1) AS inquiries,
        (SELECT count(*)::int FROM inquiry_received WHERE person_id=$1) AS facts,
        (SELECT count(*)::int FROM assignment_changed WHERE person_id=$1) AS assignments`,
        [retryPerson], rows => expect(rows[0]).toEqual({ inquiries: 1, facts: 1, assignments: 1 }));
      await expect(actors.blair.page.getByRole('link', { name: 'Retry ' + project, exact: true })).toBeVisible();
    },
    async () => {
      const observer = actors.blair;
      await observer.page.goto('/people/' + personId);
      await expect(observer.page.getByRole('combobox', { name: 'Stage', exact: true }).first()).toContainText('Hot Prospect');
      const disconnectSince = Date.now();
      await observer.context.setOffline(true);
      await expect.poll(() => observer.recorder.sockets.some(s => s.closed >= disconnectSince)).toBe(true);
      await actors.alice.page.goto('/people/' + personId);
      await mutation(actors.alice, '/api/people/' + personId + '/stage',
        () => select(actors.alice, 'Stage', 'Closed'));
      stageFacts++;
      await checkpoint('missed event still commits', { stage: 'Closed' });
      const faultSince = Date.now();
      await fetch('http://mocks:9000/control', { method: 'POST', body: JSON.stringify({ failPublish: true }) });
      await mutation(actors.alice, '/api/people/' + personId + '/stage',
        () => select(actors.alice, 'Stage', 'Nurture'));
      stageFacts++;
      await checkpoint('publication failure does not roll back command', { stage: 'Nurture' });
      await expect.poll(async () => (await (await fetch('http://mocks:9000/evidence')).json()).requests.some(
        r => r.kind === 'realtime' && r.failed && r.at >= faultSince)).toBe(true);
      await fetch('http://mocks:9000/control', { method: 'POST', body: JSON.stringify({ failPublish: false }) });
      const reconnectSince = Date.now();
      await observer.context.setOffline(false);
      await connected(observer, reconnectSince);
      await expect(observer.page.getByRole('combobox', { name: 'Stage', exact: true }).first()).toContainText('Nurture');
      await expect.poll(() => observer.recorder.http.some(r => r.started >= reconnectSince &&
        r.path === '/api/people/' + personId && r.status === 200)).toBe(true);
      expect(observer.recorder.frames.filter(f => f.at >= disconnectSince &&
        f.message.push?.pub?.data?.data?.person_id === personId)).toHaveLength(0);
    },
    async () => {
      const admin = await login('admin'), target = actors.blair;
      await admin.page.goto('/manage/members');
      await admin.page.getByRole('row').filter({ hasText: 'blair@e2e.test' })
        .getByRole('button', { name: 'Deactivate', exact: true }).click();
      const since = Date.now();
      await mutation(admin, '/api/organization/members/' + target.identity.user.id + '/status',
        () => admin.page.getByRole('dialog').getByRole('button', { name: 'Deactivate', exact: true }).click(),
        200, { status: 'inactive' }, 'PUT');
      await expect.poll(() => target.recorder.sockets.some(s => s.closed >= since)).toBe(true);
      await target.page.reload(); await target.page.waitForURL('**/login**');
      const response = await target.page.request.post('/api/inquiries', { data: {
        source: 'manual', payload: { email: 'forbidden@example.test', submission_id: '00000000-0000-4000-8000-000000000099' } } });
      expect(response.status()).toBe(401);
      evidence('http', { actor: 'blair-revoked', method: 'POST', path: '/api/inquiries', status: 401,
        response: await response.json() });
      await db.check('revocation retains attribution and rejects new writes', `SELECT
        (SELECT status FROM organization_membership WHERE user_id=$1 AND organization_id=$2) AS status,
        (SELECT count(*)::int FROM person) AS people,
        (SELECT assigned_user_id FROM person WHERE id=$3) AS assignee`,
      [target.identity.user.id, target.identity.organization.id, personId],
      rows => expect(rows[0]).toEqual({ status: 'inactive', people: 3, assignee: target.identity.user.id }));
      await checkpoint('deactivation does not erase Person history', { stage: 'Nurture' });
    },
    async () => {
      const mock = await (await fetch('http://mocks:9000/evidence')).json();
      expect(mock.project).toBe(project);
      expect(mock.requests.filter(r => r.kind === 'external')).toEqual([]);
      evidence('mocks', mock);
      for (const actor of allActors) {
        await actor.recorder.flush();
        expect(actor.recorder.errors).toEqual([]);
        for (const frame of actor.recorder.frames) {
          if (frame.message.push?.pub) expect(frame.message.push.pub.data.organization_id).toBe(actor.identity.organization.id);
          if (frame.message.connect?.subs) expect(Object.keys(frame.message.connect.subs)).toEqual(['org:' + actor.identity.organization.id]);
        }
      }
      await db.check('all immutable facts have the real actor and Organization', `SELECT actor_user_id, organization_id FROM inquiry_received WHERE person_id=$1
        UNION ALL SELECT actor_user_id, organization_id FROM routing_decision WHERE person_id=$1
        UNION ALL SELECT actor_user_id, organization_id FROM assignment_changed WHERE person_id=$1
        UNION ALL SELECT actor_user_id, organization_id FROM stage_changed WHERE person_id=$1
        UNION ALL SELECT actor_user_id, organization_id FROM contact_attempted WHERE person_id=$1`,
      [personId], rows => { expect(rows).toHaveLength(11); for (const row of rows) expect(row).toEqual({
        actor_user_id: seed.actors.alice.user.id, organization_id: seed.actors.alice.organization.id }); });
      await db.check('exactly this family canary and no peer book', 'SELECT first_name, last_name FROM person ORDER BY first_name',
        [], rows => expect(rows).toEqual([{ first_name: 'Avery', last_name: project },
          { first_name: 'Foreign', last_name: project }, { first_name: 'Retry', last_name: project }]));
      writeFileSync('/artifacts/isolation.json', JSON.stringify({ project,
        organizations: [seed.actors.alice.organization.id, seed.actors.casey.organization.id],
        people: [personId, foreignPerson, retryPerson], externalRequests: 0 }));
    },
  ];
  try { await journey.run(names, actions); } finally { await journey.close(); await db.close(); }
});
