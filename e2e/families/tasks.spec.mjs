import { test, expect } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { createJourney } from '../support/journey.mjs';
import { mutation, propagated } from '../support/actions.mjs';
import { database, evidence, captureSecrets } from '../support/evidence.mjs';

const names = JSON.parse(readFileSync(new URL('./registry.json', import.meta.url))).tasks.steps;

test('tasks-v1: due work → completion → reopen → transfer → deletion', async ({ browser }, testInfo) => {
  const seed = JSON.parse(readFileSync('/private/seed.json'));
  const journey = createJourney(browser, testInfo), db = await database();
  const { actors, allActors } = journey;
  const project = process.env.E2E_PROJECT, id = seed.personId, path = '/api/people/' + id;
  let task, yesterday, originalPerson;
  const title = 'Prepare follow-up ' + project, revisedTitle = 'Transferred follow-up ' + project;
  const row = actor => actor.page.getByTestId('task-row').filter({ hasText: task.title });
  const taskPath = () => path + '/tasks/' + task.id;
  async function api(actor, method, url, data, status = 200) {
    const response = await actor.page.request.fetch(url, { method, data });
    await captureSecrets(response);
    const body = response.status() === 204 ? null : await response.json();
    evidence('http', { actor: actor.key, method, path: url, request: data, status: response.status(), response: body });
    expect(response.status(), method + ' ' + url).toBe(status);
    return body;
  }
  async function state(label, expected) {
    await db.check(label, `SELECT title, kind, assignee_user_id, created_by_user_id,
      completed_by_user_id, completed_at IS NOT NULL AS completed,
      deleted_at IS NOT NULL AS deleted, deleted_by_user_id, due_at::text
      FROM task WHERE id=$1 AND person_id=$2 AND organization_id=$3`,
    [task.id, id, actors.alice.identity.organization.id], rows => {
      expect(rows).toHaveLength(1); expect(rows[0]).toMatchObject(expected);
      expect(new Date(rows[0].due_at).getTime()).toBe(new Date(task.due_at).getTime());
    });
  }
  async function today(actor) {
    return api(actor, 'GET', '/api/today');
  }
  async function atPerson(actor) {
    await actor.page.goto('/people/' + id);
    await expect(actor.page.getByRole('heading', { name: seed.personName, exact: true })).toBeVisible();
  }
  const actions = [
    async () => {
      expect(seed.version).toBe('tasks-v1');
      for (const key of ['admin', 'alice', 'blair', 'casey']) await journey.login(key, seed.actors[key]);
      await expect(actors.alice.page.getByRole('row').filter({ hasText: seed.personName })).toBeVisible();
      await expect(actors.blair.page.getByTestId('tasks-panel')).toContainText('Nothing due');
      await atPerson(actors.admin);
      yesterday = await actors.admin.page.evaluate(() => {
        const d = new Date(); d.setDate(d.getDate() - 1);
        return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`;
      });
      await db.check('initial Person ownership and activity snapshot', `SELECT assigned_user_id, updated_at::text,
        last_inquiry_at::text, last_contact_at::text FROM person WHERE id=$1`, [id], rows => {
        originalPerson = rows[0]; expect(originalPerson.assigned_user_id).toBe(actors.alice.identity.user.id);
      });
    },
    async () => {
      const actor = actors.admin;
      await actor.page.getByTestId('task-add-title').fill(title);
      await actor.page.getByTestId('task-add-assignee').click();
      await actor.page.getByRole('option', { name: 'Blair Agent', exact: true }).click();
      await actor.page.getByTestId('task-add-date').fill(yesterday);
      const since = Date.now();
      const result = await mutation(actor, path + '/tasks', () => actor.page.getByTestId('task-add-submit').click(), 201,
        { title, kind: 'follow_up', assignee_user_id: actors.blair.identity.user.id });
      task = result.body.task;
      expect(task.can_manage).toBe(true);
      const localDue = await actor.page.evaluate(iso => {
        const d = new Date(iso); return [d.getHours(), d.getMinutes(), d.getSeconds()];
      }, task.due_at);
      expect(localDue).toEqual([23, 59, 59]);
      await propagated(actors.blair, 'task_changed', id, since, '/api/tasks');
      await expect(actors.blair.page.getByTestId('task-panel-group-overdue')).toContainText(title);
      await state('created task belongs to Blair, not Person owner', { title, kind: 'follow_up',
        assignee_user_id: actors.blair.identity.user.id, created_by_user_id: actors.admin.identity.user.id,
        completed: false, deleted: false });
      const work = (await today(actors.blair)).items.find(x => x.person.id === id);
      expect(work.priority).toBe('high');
      expect(work.reasons.some(r => r.code === 'task_overdue' && r.task_id === task.id)).toBe(true);
      expect((await api(actors.alice, 'GET', '/api/tasks?scope=mine')).tasks).toEqual([]);
    },
    async () => {
      await atPerson(actors.alice);
      await expect(row(actors.alice)).toBeVisible();
      for (const control of ['complete-task', 'edit-task', 'delete-task']) await expect(row(actors.alice).getByTestId(control)).toHaveCount(0);
      expect(await api(actors.alice, 'POST', taskPath() + '/complete', {}, 403)).toEqual({ error: 'forbidden' });
      expect(await api(actors.casey, 'POST', taskPath() + '/complete', {}, 404)).toEqual({ error: 'not_found' });
      expect(await api(actors.admin, 'POST', path + '/tasks', { title: 'Foreign assignee rejected',
        assignee_user_id: actors.casey.identity.user.id }, 422)).toEqual({ error: 'invalid_assignee' });
      expect((await api(actors.casey, 'GET', '/api/tasks?scope=mine')).tasks).toEqual([]);
      await state('denied task actions preserve open state', { title, completed: false, deleted: false });
      await db.check('denied creation leaves exactly one task', 'SELECT count(*)::int AS n FROM task', [], rows => expect(rows[0].n).toBe(1));
    },
    async () => {
      const since = Date.now();
      const result = await mutation(actors.blair, taskPath() + '/complete', () => actors.blair.page.getByTestId('task-panel-complete-' + task.id).click());
      task = result.body.task; expect(result.body.changed).toBe(true);
      await propagated(actors.admin, 'task_changed', id, since, path);
      await expect(actors.blair.page.getByTestId('task-panel-row-' + task.id)).toHaveCount(0);
      await expect(actors.admin.page.getByTestId('history-summary').filter({ hasText: 'Completed task: ' + title })).toBeVisible();
      await expect(actors.admin.page.getByTestId('task-completed-detail')).toHaveCount(1);
      await state('completion persists the acting assignee', { completed: true, completed_by_user_id: actors.blair.identity.user.id });
      const unchanged = await api(actors.blair, 'POST', taskPath() + '/complete', {});
      expect(unchanged.changed).toBe(false); expect(unchanged.task.completed_at).toBe(task.completed_at);
      const snoozeCompleted = await api(actors.blair, 'POST', taskPath() + '/snooze', { due_at: new Date(Date.now()+7*86400000).toISOString() });
      expect(snoozeCompleted.changed).toBe(false); expect(snoozeCompleted.task).toEqual(task);
      expect((await today(actors.blair)).items.some(x => x.person.id === id)).toBe(false);
    },
    async () => {
      const since = Date.now();
      const result = await mutation(actors.admin, taskPath() + '/reopen', () => actors.admin.page.getByTestId('reopen-task').click());
      task = result.body.task; expect(result.body.changed).toBe(true);
      await propagated(actors.blair, 'task_changed', id, since, '/api/tasks');
      await expect(actors.blair.page.getByTestId('task-panel-row-' + task.id)).toContainText(title);
      await expect(actors.admin.page.getByTestId('task-completed-detail')).toHaveCount(0);
      await actors.blair.page.reload();
      await expect(actors.blair.page.getByTestId('task-panel-row-' + task.id)).toContainText(title);
      await state('reopen clears completion columns durably', { completed: false, completed_by_user_id: null });
      expect((await api(actors.admin, 'POST', taskPath() + '/reopen', {})).changed).toBe(false);
    },
    async () => {
      const expectedDue = await actors.blair.page.evaluate(() => {
        const d = new Date(); return new Date(d.getFullYear(), d.getMonth(), d.getDate()+1, 23, 59, 59).toISOString();
      });
      const since = Date.now();
      const result = await mutation(actors.blair, taskPath() + '/snooze', () => actors.blair.page.getByTestId('task-panel-snooze-' + task.id).click());
      task = result.body.task;
      expect(new Date(task.due_at).toISOString()).toBe(expectedDue);
      await propagated(actors.admin, 'task_changed', id, since, path);
      const panel = await api(actors.blair, 'GET', '/api/tasks?scope=mine');
      const eligible = Date.parse(task.due_at) <= Date.parse(panel.generated_at) + 86400000;
      expect(panel.tasks.some(t => t.id === task.id)).toBe(eligible);
      await expect(actors.blair.page.getByTestId('task-panel-row-' + task.id)).toHaveCount(eligible ? 1 : 0);
      await state('snooze changes only the due instant', { title, completed: false, assignee_user_id: actors.blair.identity.user.id });
      expect((await api(actors.blair, 'POST', taskPath() + '/snooze', { due_at: task.due_at })).changed).toBe(false);
    },
    async () => {
      await atPerson(actors.blair); await actors.alice.page.goto('/today');
      await row(actors.blair).getByTestId('edit-task').click();
      await actors.blair.page.getByTestId('task-edit-title').fill(revisedTitle);
      await actors.blair.page.getByTestId('task-edit-date').fill(yesterday);
      await actors.blair.page.getByTestId('task-edit-assignee').click();
      await actors.blair.page.getByRole('option', { name: 'Alice Agent', exact: true }).click();
      const since = Date.now();
      const result = await mutation(actors.blair, taskPath(), () => actors.blair.page.getByTestId('task-edit-save').click(), 200,
        { title: revisedTitle, assignee_user_id: actors.alice.identity.user.id }, 'PUT');
      task = result.body.task; expect(task.can_manage).toBe(false);
      await propagated(actors.alice, 'task_changed', id, since, '/api/tasks');
      await expect(actors.alice.page.getByTestId('task-panel-row-' + task.id)).toContainText(revisedTitle);
      await expect(row(actors.blair).getByTestId('edit-task')).toHaveCount(0);
      expect(await api(actors.blair, 'POST', taskPath() + '/complete', {}, 403)).toEqual({ error: 'forbidden' });
      await state('handoff changes task responsibility', { title: revisedTitle, assignee_user_id: actors.alice.identity.user.id });
      const work = (await today(actors.alice)).items.find(x => x.person.id === id);
      expect(work.reasons.some(r => r.code === 'new_inquiry')).toBe(true);
      expect(work.reasons.some(r => r.code === 'task_overdue')).toBe(true);
    },
    async () => {
      const since = Date.now();
      const result = await mutation(actors.alice, taskPath() + '/complete', () => actors.alice.page.getByTestId('task-panel-complete-' + task.id).click());
      task = result.body.task;
      await propagated(actors.blair, 'task_changed', id, since, path);
      await expect(actors.alice.page.getByTestId('task-panel-row-' + task.id)).toHaveCount(0);
      await expect(actors.alice.page.getByRole('row').filter({ hasText: seed.personName })).toBeVisible();
      const work = (await today(actors.alice)).items.find(x => x.person.id === id);
      expect(work.reasons.some(r => r.code === 'new_inquiry')).toBe(true);
      expect(work.reasons.some(r => r.code.startsWith('task_'))).toBe(false);
      await state('completion keeps unrelated inquiry work', { completed: true, completed_by_user_id: actors.alice.identity.user.id });
    },
    async () => {
      await atPerson(actors.admin);
      await mutation(actors.admin, taskPath() + '/reopen', () => actors.admin.page.getByTestId('reopen-task').click());
      await row(actors.admin).getByTestId('delete-task').click();
      const since = Date.now();
      await mutation(actors.admin, taskPath(), () => actors.admin.page.getByRole('dialog').getByRole('button', { name: 'Delete', exact: true }).click(), 200, null, 'DELETE');
      await propagated(actors.blair, 'task_changed', id, since, path);
      await expect(actors.blair.page.getByTestId('task-row')).toHaveCount(0);
      await expect(actors.blair.page.getByTestId('task-completed-detail')).toHaveCount(0);
      await state('delete erases title and records tombstone actor', { title: '', completed: false, deleted: true,
        deleted_by_user_id: actors.admin.identity.user.id });
      expect(await api(actors.admin, 'DELETE', taskPath(), undefined, 404)).toEqual({ error: 'not_found' });
      await actors.blair.page.reload(); await expect(actors.blair.page.getByTestId('task-list-empty')).toBeVisible();
      const detail = await api(actors.admin, 'GET', path);
      expect(detail.tasks).toEqual([]); expect(JSON.stringify(detail)).not.toContain(revisedTitle);
    },
    async () => {
      await db.check('task lifecycle leaves Person ownership and activity unchanged', `SELECT assigned_user_id, updated_at::text,
        last_inquiry_at::text, last_contact_at::text FROM person WHERE id=$1`, [id], rows => expect(rows).toEqual([originalPerson]));
      await db.check('exact task family canaries', `SELECT
        (SELECT array_agg(last_name) FROM person) AS people,
        (SELECT count(*)::int FROM task) AS tasks,
        (SELECT count(*)::int FROM task WHERE title='' AND deleted_at IS NOT NULL) AS tombstones`, [],
      rows => expect(rows).toEqual([{ people: [project], tasks: 1, tombstones: 1 }]));
      for (const actor of allActors) {
        await actor.recorder.flush(); expect(actor.recorder.errors).toEqual([]);
        for (const frame of actor.recorder.frames) {
          if (frame.message.connect?.subs) expect(Object.keys(frame.message.connect.subs)).toEqual(['org:' + actor.identity.organization.id]);
          if (frame.message.push?.pub) {
            expect(frame.message.push.pub.data.organization_id).toBe(actor.identity.organization.id);
            expect(JSON.stringify(frame.message)).not.toContain(title);
            expect(JSON.stringify(frame.message)).not.toContain(revisedTitle);
          }
        }
      }
      const mock = await (await fetch('http://mocks:9000/evidence')).json();
      expect(mock.project).toBe(project); expect(mock.requests.filter(r => r.kind === 'external')).toEqual([]);
      evidence('mocks', mock);
      writeFileSync('/artifacts/isolation.json', JSON.stringify({ project,
        organizations: [actors.alice.identity.organization.id, actors.casey.identity.organization.id], people: [id], externalRequests: 0 }));
    },
  ];
  try { await journey.run(names, actions); } finally { await journey.close(); await db.close(); }
});
