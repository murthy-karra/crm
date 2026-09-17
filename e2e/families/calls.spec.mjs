import { test, expect } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash, createHmac } from 'node:crypto';
import { createJourney } from '../support/journey.mjs';
import { mutation, propagated } from '../support/actions.mjs';
import { database, evidence, captureSecrets, remember } from '../support/evidence.mjs';
const names = JSON.parse(readFileSync(new URL('./registry.json', import.meta.url))).calls.steps;
test('Browser and Operator calling preserve authoritative contact and outcome state', async ({ browser }, testInfo) => {
  const seed = JSON.parse(readFileSync('/private/seed.json'));
  const journey = createJourney(browser, testInfo), db = await database();
  const path = '/api/people/' + seed.personId;
  let a, b, other, main, busy, microphone, operatorCall, rootAttempt;
  async function api(actor, method, url, data, status = 200) {
    const response = await actor.context.request.fetch(url, { method, ...(data === undefined ? {} : { data }) });
    await captureSecrets(response); const body = await response.json();
    if (body.join?.token) remember(body.join.token);
    evidence('http', { actor: actor.key, method, path: url, request: data, status: response.status(), response: body });
    expect(response.status(), url).toBe(status); return body;
  }
  async function control(data) { await api(a, 'POST', 'http://mocks:9000/call-control', data); }
  async function person(actor) { await actor.page.goto('/people/' + seed.personId); await expect(actor.page.getByRole('heading', { name: seed.personName })).toBeVisible(); }
  async function start(mode = 'answered') {
    await control({ mode }); await person(a);
    const result = await mutation(a, path + '/calls', () => a.page.getByTestId('call-button').click(), 201,
      { contact_method_id: seed.phoneId });
    remember(result.body.join.token); return result.body.call;
  }
  async function attempts(callId, outcomes) {
    return db.check('immutable attempts for call ' + callId, `SELECT id, outcome, corrects_id, actor_user_id, organization_id FROM contact_attempted
      WHERE causation_id=$1 ORDER BY recorded_at,id`, [callId], rows => {
      expect(rows.map(r => r.outcome)).toEqual(outcomes);
      for (const row of rows) { expect(row.actor_user_id).toBe(seed.actors.alice.user.id); expect(row.organization_id).toBe(seed.actors.alice.organization.id); }
    });
  }
  async function waitStatus(call, status) {
    await expect.poll(async () => (await api(a, 'GET', '/api/calls/' + call.id)).call.status).toBe(status);
  }
  async function choosePanel(call, label) {
    await expect(a.page.getByTestId('call-outcome-save')).toBeDisabled();
    await expect(a.page.getByRole('radio', { checked: true })).toHaveCount(0);
    await a.page.getByRole('radio', { name: label, exact: true }).click();
    return mutation(a, '/api/calls/' + call.id + '/outcome', () => a.page.getByTestId('call-outcome-save').click());
  }
  async function countCalls(n) { await db.check('exact call count ' + n, 'SELECT count(*)::int AS count FROM call', [], rows => expect(rows[0].count).toBe(n)); }
  try { await journey.run(names, [
    async () => {
      expect(seed.version).toBe('calls-v1');
      a = await journey.login('alice', seed.actors.alice); b = await journey.login('blair', seed.actors.blair); other = await journey.login('casey', seed.actors.casey);
      await person(a); await person(b); await expect(a.page.getByTestId('call-button')).toBeEnabled(); await countCalls(0);
    },
    async () => {
      main = await start(); await expect(a.page.getByTestId('call-status')).toContainText('Ringing');
      await waitStatus(main, 'ringing'); await attempts(main.id, []);
      await api(a, 'POST', path + '/calls', { contact_method_id: seed.phoneId }, 409);
      await api(a, 'POST', '/api/calls/' + main.id + '/dial', undefined, 409); await countCalls(1);
      await expect.poll(() => a.recorder.http.find(r => r.path === '/api/calls/' + main.id + '/dial' && r.status === 202)).toBeTruthy();
    },
    async () => {
      const since = Date.now(); await control({ action: 'answer', room: 'call:' + main.id });
      await expect(a.page.getByTestId('call-status')).toContainText('Connected'); await waitStatus(main, 'answered');
      await propagated(b, 'contact_attempted', seed.personId, since, path);
      rootAttempt = (await attempts(main.id, ['reached']))[0].id;
      await api(b, 'GET', '/api/calls/' + main.id);
      await api(b, 'POST', '/api/calls/' + main.id + '/hangup', undefined, 403);
      await api(other, 'GET', '/api/calls/' + main.id, undefined, 404);
      await api(other, 'POST', '/api/calls/' + main.id + '/hangup', undefined, 404);
      await mutation(a, '/api/calls/' + main.id + '/hangup', () => a.page.getByTestId('call-hangup').click());
      await waitStatus(main, 'ended'); await expect(a.page.getByTestId('call-outcome-prompt')).toBeVisible();
      await expect(a.page.getByTestId('call-outcome-save')).toBeDisabled();
      await expect(a.page.getByRole('radio', { checked: true })).toHaveCount(0);
      await expect(a.page.getByRole('button', { name: 'Skip', exact: true })).toHaveCount(0);
    },
    async () => {
      await a.page.reload(); await a.page.goto('/today');
      const caller = await api(a, 'GET', '/api/today'); const owner = await api(b, 'GET', '/api/today');
      const work = caller.items.find(i => i.person.id === seed.personId);
      expect(work.priority).toBe('low'); expect(work.recommended_action).toBe('set_outcome');
      expect(work.reasons.some(r => r.code === 'call_outcome_needed' && r.call_id === main.id)).toBe(true);
      expect(owner.items.some(i => i.reasons.some(r => r.code === 'call_outcome_needed' && r.call_id === main.id))).toBe(false);
      await a.page.getByRole('button', { name: 'Set outcome', exact: true }).click();
      await expect(a.page.getByRole('dialog')).toBeVisible(); await expect(a.page.getByTestId('change-outcome-save')).toBeDisabled();
      await expect(a.page.getByRole('radio', { checked: true })).toHaveCount(0);
    },
    async () => {
      await a.page.getByRole('radio', { name: 'Voicemail', exact: true }).click(); let since = Date.now();
      const result = await mutation(a, '/api/calls/' + main.id + '/outcome', () => a.page.getByTestId('change-outcome-save').click());
      expect(result.body.attempt.corrects_id).toBe(rootAttempt); await propagated(b, 'contact_attempted', seed.personId, since, path);
      await attempts(main.id, ['reached', 'left_message']);
      expect((await api(a, 'GET', '/api/today')).items.some(i => i.reasons.some(r => r.code === 'call_outcome_needed' && r.call_id === main.id))).toBe(false);
      await api(b, 'POST', '/api/calls/' + main.id + '/outcome', { outcome: 'reached' }, 403);
      await a.page.getByTestId('change-outcome').click(); await a.page.getByRole('radio', { name: 'Talked to them', exact: true }).click();
      const corrected = await mutation(a, '/api/calls/' + main.id + '/outcome', () => a.page.getByTestId('change-outcome-save').click());
      expect(corrected.body.attempt.corrects_id).toBe(result.body.attempt.id);
      await attempts(main.id, ['reached', 'left_message', 'reached']);
      expect((await api(a, 'POST', '/api/calls/' + main.id + '/outcome', { outcome: 'reached' })).changed).toBe(false);
      // Signed duplicate terminal provider deliveries must not append facts twice.
      const body = JSON.stringify({ event: 'room_finished', room: { name: 'call:' + main.id } });
      const header = Buffer.from(JSON.stringify({ alg: 'HS256', typ: 'JWT' })).toString('base64url');
      const payload = Buffer.from(JSON.stringify({ iss: process.env.E2E_LIVEKIT_KEY, exp: Math.floor(Date.now()/1000)+60,
        sha256: createHash('sha256').update(body).digest('base64') })).toString('base64url');
      const token = header + '.' + payload + '.' + createHmac('sha256', process.env.E2E_LIVEKIT_SECRET).update(header + '.' + payload).digest('base64url'); remember(token);
      for (let i=0;i<2;i++) { const response = await a.context.request.post('http://api:3001/webhooks/livekit', { data: body, headers: { authorization: token, 'content-type': 'application/json' } }); expect(response.status()).toBe(200); evidence('provider-webhooks', { event: 'room_finished', callId: main.id, status: response.status(), duplicate: i===1 }); }
      await attempts(main.id, ['reached', 'left_message', 'reached']);
      await db.check('duplicate terminal signals retain one completion fact', 'SELECT count(*)::int AS count FROM call_completed WHERE call_id=$1', [main.id], rows => expect(rows[0].count).toBe(1));
    },
    async () => {
      busy = await start('busy'); await waitStatus(busy, 'failed');
      expect((await api(a, 'GET', '/api/calls/' + busy.id)).call.failure_reason).toBe('busy');
      await expect(a.page.getByTestId('call-outcome-prompt')).toBeVisible(); await attempts(busy.id, ['no_answer']);
      await choosePanel(busy, 'Busy'); await attempts(busy.id, ['no_answer', 'busy']);
      await a.page.getByTestId('call-dismiss').click();
    },
    async () => {
      await control({ mode: 'provider_error' }); await person(a);
      const failed = await mutation(a, path + '/calls', () => a.page.getByTestId('call-button').click(), 503);
      expect(failed.body.error).toBe('telephony_unavailable'); await expect(a.page.getByTestId('call-error')).toBeVisible();
      await countCalls(3);
      await db.check('room-creation failure gets no contact attempt', `SELECT (SELECT count(*)::int FROM contact_attempted a WHERE a.causation_id=c.id) AS attempts FROM call c WHERE failure_reason='provider_error'`, [], rows => expect(rows).toEqual([{ attempts: 0 }]));
      microphone = await start('microphone_denied'); await waitStatus(microphone, 'failed');
      await expect(a.page.getByTestId('call-error')).toBeVisible(); await attempts(microphone.id, []);
      await expect(a.page.getByTestId('call-outcome-prompt')).toHaveCount(0); await a.page.getByTestId('call-dismiss').click();
    },
    async () => {
      await control({ mode: 'answered' }); await person(a);
      async function ask() {
        await api(a, 'POST', 'http://mocks:9000/operator-control', { scenario: 'call', personId: seed.personId });
        if (!await a.page.getByTestId('operator-input').isVisible()) await a.page.getByTestId('ask-toggle').click();
        await a.page.getByTestId('operator-input').fill('Call Cameron.');
        return (await mutation(a, '/api/operator/turns', () => a.page.getByTestId('operator-send').click())).body;
      }
      const dismissed = await ask(); expect(dismissed.proposal.kind).toBe('start_call'); await countCalls(4);
      await a.page.getByTestId('operator-proposal-dismiss').last().click(); await countCalls(4);
      const result = await ask(); const proposalId = result.proposal.id;
      const confirmed = await mutation(a, '/api/operator/proposals/' + proposalId + '/confirm', () => a.page.getByTestId('operator-proposal-confirm').last().click());
      operatorCall = confirmed.body.call; remember(confirmed.body.join.token);
      await a.page.getByTestId('operator-close').click(); await expect(a.page.getByTestId('call-status')).toContainText('Ringing');
      await api(a, 'POST', '/api/operator/proposals/' + proposalId + '/confirm', {}, 409); await countCalls(5);
      await control({ action: 'answer', room: 'call:' + operatorCall.id }); await waitStatus(operatorCall, 'answered');
      await mutation(a, '/api/calls/' + operatorCall.id + '/hangup', () => a.page.getByTestId('call-hangup').click());
      await waitStatus(operatorCall, 'ended'); await choosePanel(operatorCall, 'Talked to them');
      await attempts(operatorCall.id, ['reached', 'reached']);
      await db.check('confirmed Operator call preserves proposal correlation', 'SELECT origin, correlation_id FROM call WHERE id=$1', [operatorCall.id], rows => expect(rows).toEqual([{ origin: 'operator', correlation_id: result.turn_id }]));
    },
    async () => {
      await a.page.reload(); await person(b);
      await db.check('five terminal calls and no unrelated Person mutation', `SELECT
        (SELECT count(*)::int FROM call WHERE status IN ('ended','failed')) AS terminal,
        (SELECT count(*)::int FROM call_completed) AS completed,
        (SELECT count(*)::int FROM contact_attempted) AS attempts,
        (SELECT assigned_user_id FROM person WHERE id=$1) AS owner,
        (SELECT count(*)::int FROM person) AS people`, [seed.personId], rows => expect(rows).toEqual([{ terminal: 5, completed: 5, attempts: 7, owner: seed.actors.blair.user.id, people: 1 }]));
      const mock = await (await a.context.request.get('http://mocks:9000/evidence')).json();
      expect(mock.project).toBe(process.env.E2E_PROJECT); expect(mock.requests.filter(r => r.kind === 'external')).toEqual([]);
      expect(mock.requests.some(r => r.kind === 'livekit' && r.method === 'CreateSIPParticipant')).toBe(true);
      expect(mock.requests.some(r => r.kind === 'livekit_browser' && r.action === 'join')).toBe(true); evidence('mocks', mock);
      for (const actor of journey.allActors) { await actor.recorder.flush(); expect(actor.recorder.errors).toEqual([]);
        for (const f of actor.recorder.frames.filter(f => f.message.push?.pub)) {
          expect(f.message.push.channel).toBe('org:' + actor.identity.organization.id);
          expect(JSON.stringify(f.message)).not.toContain('+12025550148');
        }
      }
      writeFileSync('/artifacts/isolation.json', JSON.stringify({ project: process.env.E2E_PROJECT,
        organizations: [seed.actors.alice.organization.id, seed.actors.casey.organization.id], people: [seed.personId], externalRequests: 0 }));
    },
  ]); } finally { await journey.close(); await db.close(); }
});
