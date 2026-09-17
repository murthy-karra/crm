import { test, expect } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { createJourney } from '../support/journey.mjs';
import { mutation, propagated } from '../support/actions.mjs';
import { database, evidence, captureSecrets } from '../support/evidence.mjs';
const names = JSON.parse(readFileSync(new URL('./registry.json', import.meta.url))).operator.steps;
test('Operator tools share authorized application commands', async ({ browser }, testInfo) => {
  const seed = JSON.parse(readFileSync('/private/seed.json'));
  const journey = createJourney(browser, testInfo), db = await database();
  let a, b, proposal, dismissed, task, completedTurn;
  const path = '/api/people/' + seed.personId, title = 'Operator synthetic follow-up';
  const calls = (name, args) => ({ name, arguments: args });
  const getPerson = calls('get_person', { person_id: seed.personId });
  const createTask = calls('create_task', { person_id: seed.personId, title, kind: 'follow_up', assignee: 'me', due_date: '2026-01-01' });
  async function api(actor, method, url, data, status = 200) {
    const response = await actor.context.request.fetch(url, { method, ...(data === undefined ? {} : { data }) });
    await captureSecrets(response); const body = await response.json();
    evidence('http', { actor: actor.key, method, path: url, status: response.status(), request: data, response: body });
    expect(response.status(), url).toBe(status); return body;
  }
  async function turn(scenario, toolCalls, message, options = {}) {
    const control = await a.context.request.post('http://mocks:9000/operator-control', { data: {
      scenario, calls: toolCalls, reply: options.reply, failure: options.failure ?? false,
    } }); expect(control.status()).toBe(200);
    if (!await a.page.getByTestId('operator-input').isVisible()) await a.page.getByTestId('ask-toggle').click();
    await a.page.getByTestId('operator-input').fill(message);
    return (await mutation(a, '/api/operator/turns', () => a.page.getByTestId('operator-send').click(), options.status ?? 200)).body;
  }
  async function countTasks(n) {
    await db.check('exact persisted task count ' + n, 'SELECT count(*)::int AS count FROM task', [], rows => expect(rows[0].count).toBe(n));
  }
  async function taskState(done) {
    await db.check('Operator task durable completion ' + done, `SELECT title, organization_id, assignee_user_id,
      created_by_user_id, completed_at IS NOT NULL AS completed, origin FROM task WHERE id=$1`, [task.id], rows =>
      expect(rows).toEqual([{ title, organization_id: seed.actors.alice.organization.id,
        assignee_user_id: seed.actors.alice.user.id, created_by_user_id: seed.actors.alice.user.id, completed: done, origin: 'operator' }]));
  }
  try { await journey.run(names, [
    async () => {
      expect(seed.version).toBe('operator-v1');
      a = await journey.login('alice', seed.actors.alice); b = await journey.login('blair', seed.actors.blair);
      await b.page.goto('/people/' + seed.personId); await expect(b.page.getByRole('heading', { name: seed.personName })).toBeVisible();
      const result = await turn('search', [calls('search_people', { query: 'Morgan' })], 'Find Morgan for this synthetic journey.');
      expect(result.references.people.map(p => p.id)).toEqual([seed.personId]);
      expect(result.tool_calls).toEqual([expect.objectContaining({ name: 'search_people', outcome: 'ok' })]);
      await expect(a.page.getByTestId('operator-person-card')).toContainText(seed.personName);
      await a.page.getByTestId('operator-person-card').click(); await a.page.waitForURL('**/people/' + seed.personId);
      await expect(a.page.getByRole('heading', { name: seed.personName })).toBeVisible();
    },
    async () => {
      const result = await turn('explain', [getPerson, calls('explain_priority', { person_id: seed.personId })], 'Why is Morgan on my Today queue?', { reply: 'Morgan has a new inquiry in the authoritative Today results.' });
      expect(result.tool_calls.map(t => [t.name, t.outcome])).toEqual([['get_person', 'ok'], ['explain_priority', 'ok']]);
      const today = await api(a, 'GET', '/api/today');
      expect(today.items.find(i => i.person.id === seed.personId).reasons.some(r => r.code === 'new_inquiry')).toBe(true);
      await expect(a.page.getByTestId('operator-assistant').last()).toContainText('new inquiry'); await countTasks(1);
    },
    async () => {
      const result = await turn('dismiss', [getPerson, createTask], 'Add a follow-up task for Morgan, due January 1 2026.');
      dismissed = result.proposal; expect(dismissed.kind).toBe('create_task');
      await expect(a.page.getByTestId('operator-task-proposal').last()).toContainText(title); await countTasks(1);
      await a.page.getByTestId('operator-task-proposal-dismiss').last().click();
      await expect(a.page.getByTestId('operator-task-proposal-confirm')).toBeDisabled(); await countTasks(1);
      await db.check('dismissed proposal remains unconfirmed and does not write a task', 'SELECT status, task_id FROM operator_proposal WHERE id=$1', [dismissed.id ?? dismissed.proposal_id], rows => expect(rows).toEqual([{ status: 'proposed', task_id: null }]));
    },
    async () => {
      const result = await turn('confirm', [getPerson, createTask], 'Add the same follow-up task for Morgan, due January 1 2026.');
      proposal = result.proposal; const proposalId = proposal.id ?? proposal.proposal_id;
      const since = Date.now();
      const confirmed = await mutation(a, '/api/operator/proposals/' + proposalId + '/confirm', () => a.page.getByTestId('operator-task-proposal-confirm').last().click(), 201);
      task = confirmed.body.task;
      await propagated(b, 'task_changed', seed.personId, since, path);
      await expect(b.page.getByTestId('task-row').filter({ hasText: title })).toBeVisible(); await countTasks(2); await taskState(false);
      await api(a, 'POST', '/api/operator/proposals/' + proposalId + '/confirm', {}, 409); await countTasks(2);
      const today = await api(a, 'GET', '/api/today'); expect(today.items.find(i => i.person.id === seed.personId).reasons.some(r => r.task_id === task.id)).toBe(true);
    },
    async () => {
      let since = Date.now();
      const result = await turn('complete', [getPerson, calls('complete_task', { person_id: seed.personId, task_id: task.id })], 'Complete my synthetic follow-up task for Morgan.');
      completedTurn = result.turn_id; expect(result.receipt).toBeTruthy();
      await expect(a.page.getByTestId('operator-receipt').last()).toContainText(title);
      await propagated(b, 'task_changed', seed.personId, since, path); await taskState(true);
      await expect(b.page.getByTestId('history-summary').filter({ hasText: 'Completed task: ' + title })).toBeVisible();
      since = Date.now(); await mutation(a, path + '/tasks/' + task.id + '/reopen', () => a.page.getByTestId('operator-receipt-undo').last().click());
      await propagated(b, 'task_changed', seed.personId, since, path); await taskState(false);
      await expect(a.page.getByTestId('operator-receipt-message').last()).toContainText('Reopened');
    },
    async () => {
      let result = await turn('denied-complete', [getPerson, calls('complete_task', { person_id: seed.personId, task_id: seed.deniedTaskId })], 'Complete Blair exclusive follow-up.');
      expect(result.receipt).toBeNull();
      await db.check('task ownership denial has no effect', 'SELECT completed_at IS NULL AS open, assignee_user_id FROM task WHERE id=$1', [seed.deniedTaskId], rows => expect(rows).toEqual([{ open: true, assignee_user_id: seed.actors.blair.user.id }]));
      result = await turn('foreign', [calls('get_person', { person_id: seed.foreignPersonId })], 'Try a foreign Person identifier.');
      expect(result.references.people).toEqual([]); expect(result.tool_calls).toEqual([expect.objectContaining({ name: 'get_person', outcome: 'not_found' })]);
      expect(JSON.stringify(result)).not.toContain('private@example.test'); await countTasks(2);
    },
    async () => {
      const failed = await turn('provider-failure', [], 'Synthetic failure should not create a task.', { failure: true, status: 503 });
      expect(failed).toEqual({ error: 'operator_unavailable' });
      await expect(a.page.getByTestId('operator-error')).toBeVisible(); await countTasks(2); await taskState(false);
      await db.check('provider failure is recorded without message content', "SELECT count(*)::int AS count FROM operator_turn WHERE outcome='provider_error'", [], rows => expect(rows[0].count).toBe(1));
    },
    async () => {
      await a.page.reload(); await expect(a.page.getByTestId('operator-receipt')).toHaveCount(0);
      await expect(a.page.getByTestId('task-row').filter({ hasText: title })).toBeVisible(); await taskState(false);
      await db.check('ledger contains only scoped metadata and the completed typed tool', `SELECT t.organization_id, t.actor_user_id, t.origin, c.tool_name, c.outcome
        FROM operator_turn t JOIN operator_tool_call c ON c.turn_id=t.id WHERE t.id=$1 AND c.tool_name='complete_task'`, [completedTurn], rows => expect(rows).toEqual([{
          organization_id: seed.actors.alice.organization.id, actor_user_id: seed.actors.alice.user.id, origin: 'operator', tool_name: 'complete_task', outcome: 'ok',
        }]));
      await db.check('audit rows never persist synthetic conversation content', 'SELECT row_to_json(t)::text AS metadata FROM operator_turn t UNION ALL SELECT row_to_json(c)::text FROM operator_tool_call c', [], rows => {
        expect(rows.length).toBeGreaterThan(8); for (const row of rows) expect(row.metadata).not.toMatch(/Morgan|synthetic follow-up|private@example|Why is|Add a follow-up/);
      });
      const mock = await (await a.context.request.get('http://mocks:9000/evidence')).json();
      expect(mock.project).toBe(process.env.E2E_PROJECT); expect(mock.requests.filter(r => r.kind === 'external')).toEqual([]);
      expect(mock.requests.some(r => r.kind === 'inference' && r.toolResultCount > 0)).toBe(true);
      expect(mock.requests.filter(r => r.kind === 'inference' && r.status === 429)).toHaveLength(1); evidence('mocks', mock);
      for (const actor of journey.allActors) {
        await actor.recorder.flush(); expect(actor.recorder.errors).toEqual([]);
        for (const f of actor.recorder.frames.filter(f => f.message.push?.pub)) {
          expect(f.message.push.channel).toBe('org:' + actor.identity.organization.id);
          expect(JSON.stringify(f.message)).not.toContain(title);
        }
      }
      await db.check('exact family Person book', 'SELECT id FROM person ORDER BY id', [], rows => expect(rows.map(r => r.id).sort()).toEqual([seed.personId, seed.foreignPersonId].sort()));
      writeFileSync('/artifacts/isolation.json', JSON.stringify({ project: process.env.E2E_PROJECT,
        organizations: [seed.actors.alice.organization.id, seed.actors.casey.organization.id], people: [seed.personId, seed.foreignPersonId], externalRequests: 0 }));
    },
  ]); } finally { await journey.close(); await db.close(); }
});
