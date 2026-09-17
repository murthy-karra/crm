import { test, expect } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { createJourney } from '../support/journey.mjs';
import { mutation, propagated, select } from '../support/actions.mjs';
import { database, evidence } from '../support/evidence.mjs';
const seed = JSON.parse(readFileSync('/private/seed.json', 'utf8'));
const names = JSON.parse(readFileSync(new URL('./registry.json', import.meta.url))).relationships.steps;

test('Shared relationship context: notes, tags and typed fields', async ({ browser }, testInfo) => {
  const journey = createJourney(browser, testInfo), db = await database();
  const person = seed.personId, path = '/api/people/' + person;
  let a, b, admin, other, noteId, tagId;
  const fields = {};
  async function api(actor, method, url, data, status = 200) {
    const response = await actor.context.request.fetch(url, { method, ...(data === undefined ? {} : { data }) });
    const body = await response.json();
    evidence('http', { actor: actor.key, method, path: url, status: response.status(), request: data, response: body });
    expect(response.status(), url).toBe(status); return body;
  }
  async function detail(actor) {
    await actor.page.goto('/people/' + person);
    await expect(actor.page.getByRole('heading', { name: 'Rowan ' + process.env.E2E_PROJECT, exact: true })).toBeVisible();
  }
  async function change(actor, url, action, method = 'PUT', status = 200) {
    return (await mutation(actor, url, action, status, null, method)).body;
  }
  async function sync(changeToken, since) { await propagated(b, changeToken, person, since, path); }
  async function values(count) {
    return db.check('typed values remain scoped and durable (' + count + ')',
      'SELECT field_type, text_value, number_value::text, date_value::text, option_id, organization_id, updated_by_user_id FROM person_custom_field_value WHERE person_id=$1 ORDER BY field_type', [person], rows => {
        expect(rows).toHaveLength(count);
        for (const row of rows) expect(row.organization_id).toBe(seed.actors.alice.organization.id);
      });
  }
  async function input(label, value) {
    await a.page.getByLabel(label, { exact: true }).fill(value);
    const since = Date.now();
    const result = await change(a, path + '/custom-fields/' + fields[label].id,
      () => a.page.getByLabel(label, { exact: true }).press('Enter'));
    expect(result.changed).toBe(true); await sync('custom_field_changed', since);
  }
  try { await journey.run(names, [
    async () => {
      expect(seed.version).toBe('relationships-v1');
      a = await journey.login('alice', seed.actors.alice); b = await journey.login('blair', seed.actors.blair);
      admin = await journey.login('admin', seed.actors.admin); other = await journey.login('casey', seed.actors.casey);
      await detail(a); await detail(b);
      await db.check('clean relationship state', 'SELECT (SELECT count(*)::int FROM note) AS notes, (SELECT count(*)::int FROM tag) AS tags, (SELECT count(*)::int FROM custom_field) AS fields', [], rows => expect(rows[0]).toEqual({ notes: 0, tags: 0, fields: 0 }));
    },
    async () => {
      await a.page.getByLabel('Add a note', { exact: true }).fill('Prefers afternoon viewings');
      let since = Date.now();
      const result = await change(a, path + '/notes', () => a.page.getByRole('button', { name: 'Add note', exact: true }).click(), 'POST', 201);
      noteId = result.note.id; await sync('note_changed', since);
      await expect(b.page.getByTestId('note-body')).toHaveText('Prefers afternoon viewings');
      await a.page.getByRole('button', { name: 'Edit note', exact: true }).click();
      await a.page.getByLabel('Edit note', { exact: true }).fill('Prefers weekend viewings'); since = Date.now();
      await change(a, path + '/notes/' + noteId, () => a.page.getByTestId('note-edit-save').click());
      await sync('note_changed', since); await expect(b.page.getByTestId('note-body')).toHaveText('Prefers weekend viewings');
      await db.check('note CRUD attributed to author', 'SELECT body, author_user_id, origin FROM note WHERE id=$1', [noteId], rows => expect(rows[0]).toEqual({ body: 'Prefers weekend viewings', author_user_id: seed.actors.alice.user.id, origin: 'web_session' }));
    },
    async () => {
      await expect(b.page.getByRole('button', { name: 'Edit note', exact: true })).toHaveCount(0);
      await api(b, 'PUT', path + '/notes/' + noteId, { body: 'Forbidden overwrite' }, 403);
      await api(other, 'DELETE', path + '/notes/' + noteId, undefined, 404);
      await detail(admin); await admin.page.getByRole('button', { name: 'Edit note', exact: true }).click();
      await admin.page.getByLabel('Edit note', { exact: true }).fill('Admin clarified availability');
      await change(admin, path + '/notes/' + noteId, () => admin.page.getByTestId('note-edit-save').click());
      await expect(b.page.getByTestId('note-body')).toHaveText('Admin clarified availability');
      await admin.page.getByRole('button', { name: 'Delete note', exact: true }).click(); const since = Date.now();
      await change(admin, path + '/notes/' + noteId, () => admin.page.getByRole('dialog').getByRole('button', { name: 'Delete', exact: true }).click(), 'DELETE');
      await sync('note_changed', since); await expect(b.page.getByTestId('note-body')).toHaveCount(0);
      await b.page.reload(); await expect(b.page.getByTestId('note-body')).toHaveCount(0);
      await db.check('deleted note tombstone erases its body', 'SELECT body, deleted_at IS NOT NULL AS deleted, deleted_by_user_id FROM note WHERE id=$1', [noteId], rows => expect(rows[0]).toEqual({ body: '', deleted: true, deleted_by_user_id: seed.actors.admin.user.id }));
    },
    async () => {
      await a.page.getByRole('button', { name: 'Add tag', exact: true }).click();
      await a.page.getByLabel('Search tags').fill('Investor'); const since = Date.now();
      const created = await change(a, '/api/tags', () => a.page.getByTestId('add-tag-create-row').click(), 'POST', 201);
      tagId = created.tag.id; await sync('tags_changed', since);
      await expect(b.page.getByTestId('person-tag-chip')).toContainText('Investor');
      const duplicate = await api(b, 'POST', '/api/tags', { name: ' investor ' });
      expect(duplicate.created).toBe(false); expect(duplicate.tag.id).toBe(tagId);
      expect((await api(b, 'PUT', path + '/tags/' + tagId)).changed).toBe(false);
      await db.check('normalized tag has one definition and one link', 'SELECT (SELECT count(*)::int FROM tag) AS tags, (SELECT count(*)::int FROM person_tag) AS links', [], rows => expect(rows[0]).toEqual({ tags: 1, links: 1 }));
    },
    async () => {
      await api(a, 'PUT', '/api/tags/' + tagId, { name: 'Not allowed while used' }, 403);
      await a.page.goto('/manage/tags');
      await expect(a.page.getByRole('row').filter({ hasText: 'Investor' }).getByTestId('rename-tag')).toBeDisabled();
      await admin.page.goto('/manage/tags');
      await admin.page.getByTestId('rename-tag').click(); await admin.page.getByLabel('Rename Investor').fill('Buyer');
      await change(admin, '/api/tags/' + tagId, () => admin.page.getByTestId('save-tag-rename').click());
      await detail(a); await detail(b); const since = Date.now();
      await change(a, path + '/tags/' + tagId, () => a.page.getByRole('button', { name: 'Remove tag Buyer', exact: true }).click(), 'DELETE');
      await sync('tags_changed', since); await expect(b.page.getByTestId('person-tag-chip')).toHaveCount(0);
      await a.page.goto('/manage/tags'); await a.page.getByTestId('rename-tag').click();
      await a.page.getByLabel('Rename Buyer').fill('Unused buyer');
      await change(a, '/api/tags/' + tagId, () => a.page.getByTestId('save-tag-rename').click());
      await a.page.getByTestId('delete-tag').click();
      await change(a, '/api/tags/' + tagId, () => a.page.getByRole('dialog').getByRole('button', { name: 'Delete', exact: true }).click(), 'DELETE');
      await db.check('creator can delete unused tag', 'SELECT count(*)::int AS count FROM tag', [], rows => expect(rows[0].count).toBe(0));
    },
    async () => {
      await admin.page.goto('/manage/fields');
      for (const [label, type, option] of [['Referrer', 'text', 'Text'], ['Budget', 'number', 'Number'], ['Anniversary', 'date', 'Date'], ['Temperature', 'choice', 'Single choice']]) {
        await admin.page.getByTestId('new-field-label').fill(label);
        await select(admin, 'Type', option);
        if (type === 'choice') {
          await admin.page.getByLabel('Option 1', { exact: true }).fill('Warm');
          await admin.page.getByTestId('add-new-field-option').click();
          await admin.page.getByLabel('Option 2', { exact: true }).fill('Hot');
        }
        fields[label] = (await change(admin, '/api/custom-fields', () => admin.page.getByTestId('create-field').click(), 'POST', 201)).field;
      }
      await detail(a); await detail(b);
      await expect(a.page.getByTestId('custom-field-row')).toHaveCount(4);
    },
    async () => {
      await input('Referrer', 'Neighbour referral'); await input('Budget', '425000.50');
      await input('Anniversary', '2026-06-12');
      const since = Date.now(); await change(a, path + '/custom-fields/' + fields.Temperature.id, () => select(a, 'Temperature', 'Warm'));
      await sync('custom_field_changed', since);
      await input('Referrer', 'Open house referral');
      await expect(b.page.getByLabel('Referrer', { exact: true })).toHaveValue('Open house referral');
      await expect(b.page.getByLabel('Budget', { exact: true })).toHaveValue('425000.5');
      await expect(b.page.getByLabel('Anniversary', { exact: true })).toHaveValue('2026-06-12');
      const rows = await values(4);
      expect(rows.find(r => r.field_type === 'number').number_value).toBe('425000.5000');
      expect(rows.find(r => r.field_type === 'choice').option_id).toBe(fields.Temperature.options[0].id);
    },
    async () => {
      await api(a, 'POST', '/api/custom-fields', { label: 'Unauthorized', field_type: 'text' }, 403);
      await api(a, 'PUT', path + '/custom-fields/' + fields.Budget.id, { value: { text: 'wrong type' } }, 422);
      await api(a, 'PUT', path + '/custom-fields/' + fields.Budget.id, { value: { number: '1e5' } }, 422);
      await api(other, 'PUT', path + '/custom-fields/' + fields.Referrer.id, { value: { text: 'foreign' } }, 404);
      await api(other, 'POST', path + '/notes', { body: 'foreign note' }, 404);
      await api(other, 'GET', path, undefined, 404);
      expect((await api(other, 'GET', '/api/custom-fields')).fields).toHaveLength(0);
      await values(4); await a.page.reload();
      await expect(a.page.getByLabel('Budget', { exact: true })).toHaveValue('425000.5');
      await db.check('denied writes create no notes or fields', 'SELECT (SELECT count(*)::int FROM note) AS notes, (SELECT count(*)::int FROM custom_field) AS fields', [], rows => expect(rows[0]).toEqual({ notes: 1, fields: 4 }));
    },
    async () => {
      await admin.page.goto('/manage/fields');
      await admin.page.getByRole('row').filter({ hasText: 'Referrer' }).getByTestId('archive-field').click();
      await change(admin, '/api/custom-fields/' + fields.Referrer.id, () => admin.page.getByRole('dialog').getByRole('button', { name: 'Archive', exact: true }).click());
      await b.page.reload(); await expect(b.page.getByLabel('Referrer', { exact: true })).toHaveCount(0); await values(4);
      await api(a, 'PUT', path + '/custom-fields/' + fields.Referrer.id, { value: { text: 'Rejected archived edit' } }, 409);
      await change(admin, '/api/custom-fields/' + fields.Referrer.id, () => admin.page.getByTestId('restore-field').click());
      await b.page.reload(); await expect(b.page.getByLabel('Referrer', { exact: true })).toHaveValue('Open house referral');
      await admin.page.getByRole('row').filter({ hasText: 'Temperature' }).getByTestId('manage-field-options').click();
      const optionPath = '/api/custom-fields/' + fields.Temperature.id + '/options/' + fields.Temperature.options[0].id;
      await change(admin, optionPath, () => admin.page.getByTestId('field-option-row').filter({ hasText: 'Warm' }).getByTestId('archive-option').click());
      await b.page.reload(); await expect(b.page.getByRole('combobox', { name: 'Temperature', exact: true })).toContainText('Warm (archived — keep current)');
      await api(a, 'PUT', path + '/custom-fields/' + fields.Temperature.id, { value: { option_id: fields.Temperature.options[0].id } }, 422);
      await change(admin, optionPath, () => admin.page.getByTestId('restore-option').click());
      await values(4);
    },
    async () => {
      await detail(a); await detail(b);
      await a.page.getByLabel('Anniversary', { exact: true }).fill(''); const since = Date.now();
      await change(a, path + '/custom-fields/' + fields.Anniversary.id, () => a.page.getByLabel('Anniversary', { exact: true }).press('Enter'), 'DELETE');
      await sync('custom_field_changed', since); await values(3);
      await journey.closeActor(b); b = await journey.login('blair-fresh', seed.actors.blair); await detail(b);
      await expect(b.page.getByLabel('Anniversary', { exact: true })).toHaveValue('');
      await expect(b.page.getByLabel('Referrer', { exact: true })).toHaveValue('Open house referral');
      await expect(b.page.getByTestId('note-body')).toHaveCount(0); await expect(b.page.getByTestId('person-tag-chip')).toHaveCount(0);
      const mocks = await (await a.context.request.get('http://mocks:9000/evidence')).json();
      expect(mocks.project).toBe(process.env.E2E_PROJECT); expect(mocks.requests.filter(r => r.kind === 'external')).toHaveLength(0);
      for (const actor of journey.allActors) {
        expect(actor.recorder.errors).toEqual([]);
        for (const frame of actor.recorder.frames.filter(f => f.message.push?.pub)) {
          expect(frame.message.push.channel).toBe('org:' + actor.identity.organization.id);
          expect(JSON.stringify(frame.message)).not.toMatch(/viewings|referral|425000|Forbidden overwrite/);
        }
      }
      await db.check('relationship CRUD adds no immutable facts or contact credit', `SELECT
        (SELECT count(*)::int FROM inquiry_received WHERE person_id=$1) AS inquiries,
        (SELECT count(*)::int FROM routing_decision WHERE person_id=$1) AS routing,
        (SELECT count(*)::int FROM assignment_changed WHERE person_id=$1) AS assignments,
        (SELECT count(*)::int FROM stage_changed WHERE person_id=$1) AS stages,
        (SELECT count(*)::int FROM contact_attempted WHERE person_id=$1) AS contacts`, [person],
      rows => expect(rows[0]).toEqual({ inquiries: 1, routing: 1, assignments: 1, stages: 1, contacts: 0 }));
      await db.check('family owns exactly the seeded Person', 'SELECT id FROM person', [], rows => expect(rows.map(r => r.id)).toEqual([person]));
      writeFileSync('/artifacts/isolation.json', JSON.stringify({ project: process.env.E2E_PROJECT, organizations: [seed.actors.alice.organization.id, seed.actors.casey.organization.id], people: [person], externalRequests: 0 }, null, 2));
    },
  ]); } finally { await journey.close(); await db.close(); }
});
