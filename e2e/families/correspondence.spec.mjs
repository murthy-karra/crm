import { test, expect } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { createJourney } from '../support/journey.mjs';
import { connected, mutation, propagated } from '../support/actions.mjs';
import { database, evidence, remember, captureSecrets } from '../support/evidence.mjs';

const names = JSON.parse(readFileSync(new URL('./registry.json', import.meta.url))).correspondence.steps;

test('correspondence-v1: exchange → reply work → held resolution → rotation', async ({ browser }, testInfo) => {
  const seed = JSON.parse(readFileSync('/private/seed.json'));
  const journey = createJourney(browser, testInfo), db = await database();
  const { actors } = journey, { people } = seed, project = process.env.E2E_PROJECT;
  let address, oldAddress, heldLink, heldDismiss, contactBeforeForward;
  let expectedFacts = 0, expectedContacts = 0;
  remember(process.env.E2E_INBOUND_SECRET);
  function rememberAddress(value) { remember(value); remember(value.match(/^save-([^@]+)@/)?.[1]); return value; }
  async function api(actor, method, path, data, status = 200) {
    const response = await actor.page.request.fetch(path, { method, data });
    await captureSecrets(response);
    const body = await response.json();
    if (body.address) rememberAddress(body.address);
    evidence('http', { actor: actor.key, method, path, request: data, status: response.status(), response: body });
    expect(response.status(), method + ' ' + path).toBe(status); return body;
  }
  async function ready(actor, url, readPath) {
    const since = Date.now(); await actor.page.goto(url); await connected(actor, since);
    await expect.poll(() => actor.recorder.http.find(r => r.started >= since && r.path === readPath && r.status === 200)).toBeTruthy();
  }
  function mail(key, from, to, extra = [], body = 'Synthetic correspondence body sentinel ' + project) {
    return [`From: ${from}`, `To: ${to}`, `Message-ID: <${project}-${key}@example.test>`,
      'Subject: Synthetic correspondence subject sentinel', 'Content-Type: text/plain; charset=utf-8',
      ...extra, '', body, ''].join('\r\n');
  }
  async function deliver(raw, recipient = address, outcome = 'accepted') {
    const response = await fetch('http://api:3001/inbound/email', { method: 'POST',
      headers: { 'content-type': 'application/json', authorization: 'Bearer ' + process.env.E2E_INBOUND_SECRET },
      body: JSON.stringify({ recipient, raw: Buffer.from(raw).toString('base64') }) });
    const body = await response.json();
    evidence('inbound', { status: response.status, response: body, bytes: Buffer.byteLength(raw),
      requestId: response.headers.get('x-request-id') });
    expect(response.status).toBe(200); expect(body).toEqual({ status: outcome });
  }
  async function counts(label) {
    await db.check(label, `SELECT
      (SELECT count(*)::int FROM correspondence_captured WHERE person_id=$1) AS facts,
      (SELECT count(*)::int FROM contact_attempted WHERE person_id=$1) AS contacts,
      (SELECT count(*)::int FROM correspondence_captured WHERE organization_id=$2) AS foreign_facts,
      (SELECT count(*)::int FROM person) AS people`, [people.main.id, seed.actors.casey.organization.id], rows =>
      expect(rows).toEqual([{ facts: expectedFacts, contacts: expectedContacts, foreign_facts: 0, people: 3 }]));
  }
  async function detail(actor) {
    const result = await api(actor, 'GET', '/api/people/' + people.main.id);
    const entries = result.history.filter(entry => entry.kind === 'correspondence');
    expect(entries).toHaveLength(expectedFacts);
    for (const entry of entries) {
      expect(Object.keys(entry.detail).sort()).toEqual(['agent', 'backdated', 'captured_at', 'direction', 'via']);
      expect(entry.detail.agent.id).toBe(seed.actors.alice.user.id);
      expect(JSON.stringify(entry.detail)).not.toContain('sentinel');
    }
    return entries;
  }
  async function todayMain(expectedReason = null) {
    const result = await api(actors.alice, 'GET', '/api/today');
    const item = result.items.find(item => item.person.id === people.main.id);
    if (expectedReason) expect(item.reasons).toContainEqual(expect.objectContaining({ code: expectedReason }));
    else expect(item).toBeUndefined();
  }
  async function terminal(id, status) {
    await db.check('held row terminal state strips counterparty', `SELECT status, counterparty_email FROM capture_message WHERE id=$1`,
      [id], rows => expect(rows).toEqual([{ status, counterparty_email: null }]));
  }
  try {
    await journey.run(names, [
      async () => {
        expect(seed.version).toBe('correspondence-v1');
        for (const key of ['alice', 'blair', 'admin', 'casey']) await journey.login(key, seed.actors[key]);
        address = rememberAddress((await api(actors.alice, 'GET', '/api/capture/address')).address);
        await ready(actors.alice, '/email-capture', '/api/capture/address');
        await expect(actors.alice.page.getByTestId('capture-address')).toHaveText(address);
        await expect(actors.alice.page.getByTestId('capture-address')).toHaveCSS('-webkit-text-security', 'disc');
        await expect(actors.alice.page.getByTestId('capture-unmatched-empty')).toBeVisible();
        await ready(actors.alice, '/today', '/api/today');
        await expect(actors.alice.page.getByRole('row').filter({ hasText: people.main.name })).toBeVisible();
        await ready(actors.blair, '/people/' + people.main.id, '/api/people/' + people.main.id);
        await counts('empty capture history');
      },
      async () => {
        const since = Date.now();
        await deliver(mail('outbound-1', 'alice@e2e.test', people.main.email, ['Cc: ' + address]));
        expectedFacts++; expectedContacts++;
        await propagated(actors.alice, 'correspondence_captured', people.main.id, since, '/api/today');
        await propagated(actors.blair, 'correspondence_captured', people.main.id, since, '/api/people/' + people.main.id);
        await expect(actors.blair.page.getByText('Outbound email — Alice Agent', { exact: true })).toBeVisible();
        await expect(actors.alice.page.getByRole('row').filter({ hasText: people.main.name })).toHaveCount(0);
        await todayMain(); await counts('outbound fact and automatic attempt');
        await db.check('automatic attempt is caused by captured correspondence', `SELECT c.actor_kind, c.actor_user_id,
          c.on_behalf_of_user_id, c.agent_user_id, c.direction, a.channel, a.outcome,
          a.actor_kind AS attempt_actor, a.on_behalf_of_user_id AS attempt_agent,
          a.occurred_at=c.occurred_at AS same_time FROM correspondence_captured c
          JOIN contact_attempted a ON a.causation_id=c.id WHERE c.person_id=$1`, [people.main.id], rows =>
          expect(rows).toEqual([{ actor_kind: 'system', actor_user_id: null, on_behalf_of_user_id: seed.actors.alice.user.id,
            agent_user_id: seed.actors.alice.user.id, direction: 'outbound', channel: 'email', outcome: 'sent',
            attempt_actor: 'system', attempt_agent: seed.actors.alice.user.id, same_time: true }]));
      },
      async () => {
        const since = Date.now();
        await deliver(mail('reply-1', people.main.email, 'alice@e2e.test', ['Cc: ' + address,
          `References: <${project}-outbound-1@example.test>`]));
        expectedFacts++;
        await propagated(actors.alice, 'correspondence_captured', people.main.id, since, '/api/today');
        await propagated(actors.blair, 'correspondence_captured', people.main.id, since, '/api/people/' + people.main.id);
        await expect(actors.alice.page.getByRole('row').filter({ hasText: people.main.name })).toContainText('Client replied');
        await expect(actors.blair.page.getByText('Inbound email — Alice Agent', { exact: true })).toBeVisible();
        await todayMain('client_replied'); await counts('reply adds a fact without a contact attempt');
      },
      async () => {
        const since = Date.now();
        // Envelope delivery models BCC too: the capture address need not occur in headers.
        const raw = mail('outbound-2', 'alice@e2e.test', people.main.email);
        await deliver(raw); expectedFacts++; expectedContacts++;
        await propagated(actors.alice, 'correspondence_captured', people.main.id, since, '/api/today');
        await expect(actors.alice.page.getByRole('row').filter({ hasText: people.main.name })).toHaveCount(0);
        await deliver(raw); // Relay redelivery is acknowledged without duplicate effects.
        await todayMain(); await counts('response and identical retry clear work once');
        await db.check('remember current contact maximum before retroactive import',
          'SELECT last_contact_at FROM person WHERE id=$1', [people.main.id], rows => { contactBeforeForward = rows[0].last_contact_at.toISOString(); });
      },
      async () => {
        const inner = ['---------- Forwarded message ---------', 'From: Client <client@example.test>',
          'Date: Mon, Aug 3, 2020 at 9:00 AM', 'Subject: Older synthetic exchange', 'To: alice@e2e.test', '', 'Older body sentinel'].join('\r\n');
        const refs = [`References: <thread-root@example.test> <${project}-original-old@example.test>`];
        await deliver(mail('forward-1', 'alice@e2e.test', address, refs, inner)); expectedFacts++;
        await deliver(mail('forward-2', 'alice@e2e.test', address, refs, inner));
        await counts('different forwards deduplicate on original message id'); await todayMain();
        await ready(actors.blair, '/people/' + people.main.id, '/api/people/' + people.main.id);
        await expect(actors.blair.page.getByText('Inbound email — Alice Agent (forwarded)', { exact: true })).toBeVisible();
        const entries = await detail(actors.blair);
        const backdated = entries.find(entry => entry.detail.backdated);
        expect(new Date(backdated.occurred_at).toISOString()).toBe('2020-08-03T09:00:00.000Z');
        await db.check('backdated fact preserves latest activity', `SELECT p.last_contact_at, c.occurred_at, c.via, c.direction,
          c.backdated FROM person p JOIN correspondence_captured c ON c.person_id=p.id
          WHERE p.id=$1 AND c.backdated`, [people.main.id], rows => {
          expect(rows).toHaveLength(1);
          expect(rows[0].last_contact_at.toISOString()).toBe(contactBeforeForward);
          expect(rows[0].occurred_at.toISOString()).toBe('2020-08-03T09:00:00.000Z');
          expect(rows[0]).toMatchObject({ via: 'forward', direction: 'inbound', backdated: true });
        });
      },
      async () => {
        await deliver(mail('held-link', 'alias@example.test', 'alice@e2e.test'));
        await deliver(mail('held-dismiss', 'alice@e2e.test', 'unwanted@example.test'));
        await ready(actors.alice, '/email-capture', '/api/capture/unmatched');
        const held = await api(actors.alice, 'GET', '/api/capture/unmatched');
        expect(held.items).toHaveLength(2);
        heldLink = held.items.find(row => row.counterparty_email === 'alias@example.test').id;
        heldDismiss = held.items.find(row => row.counterparty_email === 'unwanted@example.test').id;
        await expect(actors.alice.page.getByTestId('capture-unmatched-row-' + heldLink)).toContainText('alias@example.test');
        for (const key of ['blair', 'admin', 'casey']) {
          expect((await api(actors[key], 'GET', '/api/capture/unmatched')).items).toEqual([]);
          for (const id of [heldLink, heldDismiss]) {
            expect(await api(actors[key], 'POST', `/api/capture/unmatched/${id}/link`,
              { person_id: people.main.id, add_contact_method: true }, 404)).toEqual({ error: 'not_found' });
            expect(await api(actors[key], 'POST', `/api/capture/unmatched/${id}/dismiss`, undefined, 404)).toEqual({ error: 'not_found' });
          }
        }
        expect(await api(actors.alice, 'POST', `/api/capture/unmatched/${heldLink}/link`,
          { person_id: people.foreign.id, add_contact_method: true }, 404)).toEqual({ error: 'not_found' });
        await counts('unmatched and denied resolutions never create People or history');
      },
      async () => {
        const a = actors.alice;
        await a.page.getByTestId('link-' + heldLink).click();
        await a.page.getByTestId('link-person-select-' + heldLink).click();
        await a.page.getByRole('option', { name: people.main.name, exact: true }).click();
        const since = Date.now();
        await mutation(a, `/api/capture/unmatched/${heldLink}/link`, () => a.page.getByTestId('confirm-link-' + heldLink).click(),
          200, { person_id: people.main.id, add_contact_method: true }); expectedFacts++;
        await propagated(actors.blair, 'correspondence_captured', people.main.id, since, '/api/people/' + people.main.id);
        await expect(a.page.getByTestId('capture-unmatched-row-' + heldLink)).toHaveCount(0);
        await terminal(heldLink, 'linked');
        await db.check('linked address belongs only to the chosen Person', `SELECT person_id FROM contact_method
          WHERE kind='email' AND normalized_value=$1`, ['alias@example.test'], rows => expect(rows).toEqual([{ person_id: people.main.id }]));
        expect(await api(a, 'POST', `/api/capture/unmatched/${heldLink}/link`,
          { person_id: people.main.id, add_contact_method: true })).toEqual({ status: 'linked' });
        expect(await api(a, 'POST', `/api/capture/unmatched/${heldLink}/link`,
          { person_id: people.second.id, add_contact_method: true }, 409)).toEqual({ error: 'capture_conflict' });
        await mutation(a, `/api/capture/unmatched/${heldDismiss}/dismiss`, () => a.page.getByTestId('dismiss-' + heldDismiss).click());
        await expect(a.page.getByTestId('capture-unmatched-empty')).toBeVisible(); await terminal(heldDismiss, 'dismissed');
        expect(await api(a, 'POST', `/api/capture/unmatched/${heldDismiss}/dismiss`)).toEqual({ status: 'dismissed' });
        expect(await api(a, 'POST', `/api/capture/unmatched/${heldDismiss}/link`,
          { person_id: people.main.id, add_contact_method: true }, 409)).toEqual({ error: 'capture_conflict' });
        await counts('link once and dismiss preserve terminal history');
      },
      async () => {
        const a = actors.alice; oldAddress = address;
        await a.page.getByTestId('rotate-capture-address').click();
        const rotated = await mutation(a, '/api/capture/address/rotate', () =>
          a.page.getByRole('dialog').getByRole('button', { name: 'Rotate', exact: true }).click());
        address = rememberAddress(rotated.body.address); expect(address).not.toBe(oldAddress);
        await expect(a.page.getByTestId('capture-address')).toHaveText(address);
        const raw = mail('after-rotation', 'alice@e2e.test', people.main.email);
        await deliver(raw, oldAddress, 'rejected'); await counts('retired credential cannot capture');
        await deliver(raw, address); expectedFacts++; expectedContacts++;
        await counts('new credential captures immediately'); await todayMain();
        await a.page.reload(); await expect(a.page.getByTestId('capture-address')).toHaveText(address);
        await db.check('rotation is attributed to its owner', `SELECT actor_kind, actor_user_id, organization_id
          FROM capture_token_rotated`, [], rows => expect(rows).toEqual([{ actor_kind: 'user',
          actor_user_id: seed.actors.alice.user.id, organization_id: seed.actors.alice.organization.id }]));
      },
      async () => {
        await detail(actors.blair); await detail(actors.admin);
        expect(await api(actors.casey, 'GET', '/api/people/' + people.main.id, undefined, 404)).toEqual({ error: 'not_found' });
        await db.check('capture stores sealed raw outside the intake pipeline', `SELECT
          (SELECT count(*)::int FROM raw_payload) AS intake_raw,
          (SELECT count(*)::int FROM correspondence_raw) AS capture_raw,
          (SELECT bool_and(processed AND octet_length(nonce)>0 AND octet_length(ciphertext)>byte_len)
             FROM correspondence_raw) AS sealed_processed,
          (SELECT count(*)::int FROM correspondence_raw WHERE organization_id=$1) AS foreign_raw,
          (SELECT count(*)::int FROM capture_message WHERE status='held') AS held`,
          [seed.actors.casey.organization.id], rows => expect(rows).toEqual([{ intake_raw: 3, capture_raw: 8,
            sealed_processed: true, foreign_raw: 0, held: 0 }]));
        await counts('final immutable facts and People count');
        const mocks = await (await actors.alice.page.request.get('http://mocks:9000/evidence')).json();
        expect(mocks.project).toBe(project); expect(mocks.requests.filter(row => row.kind === 'external')).toEqual([]);
        for (const actor of journey.allActors) {
          await actor.recorder.flush(); expect(actor.recorder.errors).toEqual([]);
          for (const frame of actor.recorder.frames) if (frame.message.push?.pub) {
            expect(frame.message.push.channel).toBe('org:' + actor.identity.organization.id);
            expect(Object.keys(frame.message.push.pub.data.data).sort()).toEqual(['change', 'person_id']);
          }
        }
        writeFileSync('/artifacts/isolation.json', JSON.stringify({ project,
          organizations: [seed.actors.alice.organization.id, seed.actors.casey.organization.id],
          people: Object.values(people).map(person => person.id), externalRequests: 0 }, null, 2));
      },
    ]);
  } finally { await journey.close(); await db.close(); }
});
