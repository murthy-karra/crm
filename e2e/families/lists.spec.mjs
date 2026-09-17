import { test, expect } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { createJourney } from '../support/journey.mjs';
import { mutation, propagated } from '../support/actions.mjs';
import { database, evidence, captureSecrets } from '../support/evidence.mjs';

const names = JSON.parse(readFileSync(new URL('./registry.json', import.meta.url))).lists.steps;

test('lists-v1: personal work source → shared criteria → broken source repair', async ({ browser }, testInfo) => {
  const seed = JSON.parse(readFileSync('/private/seed.json'));
  const journey = createJourney(browser, testInfo), db = await database();
  const { actors } = journey, { people, tag } = seed;
  const personalName = 'Priority work', sharedName = 'My assigned book', copyName = 'My private copy';
  let personal, shared, copy;
  const path = list => '/api/saved-lists/' + list.id;
  const tagged = { version: 1, clauses: [{ kind: 'tags', tag_ids: [tag.id] }] };
  const mine = { version: 1, clauses: [{ kind: 'assigned_to', assignees: ['me'] }] };
  async function api(actor, method, url, data, status = 200) {
    const response = await actor.page.request.fetch(url, { method, data });
    await captureSecrets(response);
    const body = await response.json();
    evidence('http', { actor: actor.key, method, path: url, request: data, status: response.status(), response: body,
      responseHeaders: { requestId: response.headers()['x-request-id'] } });
    expect(response.status(), method + ' ' + url).toBe(status);
    return body;
  }
  async function open(actor, list) {
    await actor.page.goto('/lists/' + list.id);
    await expect(actor.page.getByRole('heading', { name: list.name, exact: true })).toBeVisible();
  }
  async function visiblePeople(actor, expected) {
    for (const person of Object.values(people)) {
      const row = actor.page.getByRole('row').filter({ hasText: person.name });
      if (expected.includes(person.id)) await expect(row).toHaveCount(1);
      else await expect(row).toHaveCount(0);
    }
  }
  async function saveAs(actor, name, scope = 'personal', duplicate = false) {
    await actor.page.getByRole('button', { name: duplicate ? 'Duplicate' : 'Save as list', exact: true }).click();
    const dialog = actor.page.getByRole('dialog');
    await dialog.getByTestId('saved-list-name').fill(name);
    if (scope === 'shared') await dialog.getByRole('radio', { name: /Shared lists/ }).check();
    const result = await mutation(actor, '/api/saved-lists', () => dialog.getByRole('button', {
      name: duplicate ? 'Duplicate list' : 'Save list', exact: true }).and(dialog.locator('button[type="button"]')).click(), 201, { name, scope });
    await actor.page.waitForURL('**/lists/' + result.body.list.id);
    return result.body.list;
  }
  async function persisted(list, owner, filter, revision = list.revision, sort = null) {
    await db.check('saved definition ' + list.name, `SELECT organization_id, created_by_user_id, scope, name, filter,
      revision::int, sort_key, sort_direction, deleted_at FROM saved_list WHERE id=$1`, [list.id], rows => expect(rows).toEqual([{
      organization_id: owner.identity.organization.id, created_by_user_id: owner.identity.user.id,
      scope: list.scope, name: list.name, filter, revision, sort_key: sort ? sort.split('.')[0] : null,
      sort_direction: sort ? sort.split('.')[1] : null, deleted_at: null,
    }]));
  }
  async function source(actor, list) {
    const result = await mutation(actor, '/api/today/sources/' + list.id, () =>
      actor.page.getByRole('button', { name: 'Use as a Today source', exact: true }).click(), 200,
    { expected_list_revision: list.revision }, 'PUT');
    expect(result.body).toEqual({ enabled: true, changed: true });
    await db.check('source belongs to its viewer', `SELECT user_id FROM today_work_source WHERE list_id=$1`,
      [list.id], rows => expect(rows).toEqual([{ user_id: actor.identity.user.id }]));
  }
  async function today(actor) {
    const response = actor.page.waitForResponse(r => new URL(r.url()).pathname === '/api/today' && r.status() === 200);
    await actor.page.goto('/today');
    return (await response).json();
  }
  function reasonFor(result, person, listId) {
    return result.items.filter(item => item.person.id === person).flatMap(item => item.reasons)
      .filter(reason => reason.code === 'list_member' && reason.list_id === listId);
  }
  try {
    await journey.run(names, [
      async () => {
        expect(seed.version).toBe('lists-v1');
        for (const key of ['alice', 'blair', 'admin', 'casey']) await journey.login(key, seed.actors[key]);
        await actors.alice.page.goto('/people');
        await visiblePeople(actors.alice, [people.avery.id, people.bailey.id]);
        await db.check('no prebuilt lists or source preferences', `SELECT
          (SELECT count(*)::int FROM saved_list) AS lists, (SELECT count(*)::int FROM today_work_source) AS sources`,
        [], rows => expect(rows).toEqual([{ lists: 0, sources: 0 }]));
      },
      async () => {
        const a = actors.alice;
        await a.page.getByTestId('filter-add').click();
        await a.page.getByTestId('filter-add-tags').click();
        // Committing ad-hoc criteria updates the URL and can unmount the
        // popover. Assert the committed chip instead of check()'s old input.
        await a.page.getByTestId('filter-option-' + tag.id).click();
        await expect(a.page.getByTestId('filter-chip-tags')).toBeVisible();
        await a.page.keyboard.press('Escape');
        await visiblePeople(a, [people.avery.id]);
        await a.page.getByRole('button', { name: 'Sort by Name, ascending', exact: true }).click();
        personal = await saveAs(a, personalName);
        await persisted(personal, a, tagged, 1, 'name.asc');
        await a.page.reload();
        await expect(a.page.getByRole('columnheader', { name: /Sort by Name/ })).toHaveAttribute('aria-sort', 'ascending');
        await visiblePeople(a, [people.avery.id]);
      },
      async () => {
        const a = actors.alice;
        await source(a, personal);
        let result = await today(a);
        expect(reasonFor(result, people.avery.id, personal.id)).toHaveLength(1);
        expect(result.items.filter(item => item.person.id === people.avery.id)).toHaveLength(1);
        await expect(a.page.getByRole('row').filter({ hasText: people.avery.name })).toContainText(personalName);
        await open(a, personal);
        await a.page.getByTestId('filter-chip-remove-tags').click();
        await expect(a.page.getByText('Unsaved changes', { exact: true })).toBeVisible();
        await visiblePeople(a, [people.avery.id, people.bailey.id]);
        await persisted(personal, a, tagged, 1, 'name.asc');
        result = await api(a, 'GET', '/api/today');
        expect(reasonFor(result, people.bailey.id, personal.id)).toHaveLength(0);
        await a.page.getByRole('button', { name: 'Reset changes', exact: true }).click();
        await visiblePeople(a, [people.avery.id]);
      },
      async () => {
        const a = actors.alice, b = actors.blair;
        await a.page.goto('/lists');
        await expect(a.page.getByLabel('1 matches', { exact: true })).toBeVisible();
        await b.page.goto('/people/' + people.bailey.id);
        await b.page.getByRole('button', { name: 'Add tag', exact: true }).click();
        const since = Date.now();
        await mutation(b, `/api/people/${people.bailey.id}/tags/${tag.id}`, () =>
          b.page.getByRole('option', { name: tag.name, exact: true }).click(), 200, null, 'PUT');
        await propagated(a, 'tags_changed', people.bailey.id, since, path(personal) + '/count');
        await expect(a.page.getByLabel('2 matches', { exact: true })).toBeVisible();
        await open(a, personal);
        await visiblePeople(a, [people.avery.id, people.bailey.id]);
        const result = await today(a);
        expect(reasonFor(result, people.bailey.id, personal.id)).toHaveLength(1);
        await expect(a.page.getByRole('row').filter({ hasText: people.bailey.name })).toContainText(personalName);
        await db.check('tag mutation persisted across actors', `SELECT person_id FROM person_tag WHERE tag_id=$1 ORDER BY person_id`,
          [tag.id], rows => expect(rows.map(row => row.person_id)).toEqual([people.avery.id, people.bailey.id].sort()));
      },
      async () => {
        const admin = actors.admin;
        await admin.page.goto('/people');
        await admin.page.getByTestId('filter-trigger-assigned_to').click();
        await admin.page.getByTestId('filter-assignee-me').click();
        await expect(admin.page.getByTestId('filter-chip-assigned_to')).toBeVisible();
        await admin.page.keyboard.press('Escape');
        shared = await saveAs(admin, sharedName, 'shared');
        await persisted(shared, admin, mine);
        for (const [key, person] of [['alice', people.avery], ['blair', people.bailey]]) {
          await open(actors[key], shared);
          await visiblePeople(actors[key], [person.id]);
          await expect(actors[key].page.getByRole('button', { name: 'Save', exact: true })).toHaveCount(0);
          await expect(actors[key].page.getByRole('button', { name: 'Delete', exact: true })).toHaveCount(0);
        }
        copy = await saveAs(actors.alice, copyName, 'personal', true);
        await persisted(copy, actors.alice, mine);
        await actors.alice.page.getByRole('textbox', { name: 'List name', exact: true }).fill(copyName + ' edited');
        const result = await mutation(actors.alice, path(copy), () => actors.alice.page.getByRole('button', { name: 'Save', exact: true }).click(),
          200, { expected_revision: 1, name: copyName + ' edited' }, 'PUT');
        copy = result.body.list;
        await persisted(copy, actors.alice, mine, 2);
        await persisted(shared, admin, mine);
      },
      async () => {
        for (const key of ['admin', 'blair', 'casey']) {
          const actor = actors[key];
          const index = await api(actor, 'GET', '/api/saved-lists');
          expect(index.lists.some(list => [personal.id, copy.id].includes(list.id))).toBe(false);
          for (const list of [personal, copy]) {
            expect(await api(actor, 'GET', path(list), undefined, 404)).toEqual({ error: 'not_found' });
            expect(await api(actor, 'GET', path(list) + '/count?revision=' + list.revision, undefined, 404)).toEqual({ error: 'not_found' });
            expect(await api(actor, 'DELETE', path(list), { expected_revision: list.revision }, 404)).toEqual({ error: 'not_found' });
          }
          await actor.page.goto('/lists/' + personal.id);
          await expect(actor.page.getByText('This saved list is not available.', { exact: true })).toBeVisible();
          await expect(actor.page.getByText(personalName, { exact: true })).toHaveCount(0);
        }
        expect(await api(actors.casey, 'GET', path(shared), undefined, 404)).toEqual({ error: 'not_found' });
        expect(await api(actors.blair, 'PUT', path(shared), { expected_revision: 1, name: 'forbidden', filter: mine }, 403)).toEqual({ error: 'forbidden' });
        await persisted(personal, actors.alice, tagged, 1, 'name.asc');
        await persisted(shared, actors.admin, mine);
      },
      async () => {
        const a = actors.alice, admin = actors.admin;
        await open(a, shared); await source(a, shared);
        await admin.page.goto('/manage/tags');
        await admin.page.getByRole('row').filter({ hasText: tag.name }).getByTestId('delete-tag').click();
        const deleted = await mutation(admin, '/api/tags/' + tag.id, () =>
          admin.page.getByRole('dialog').getByRole('button', { name: 'Delete', exact: true }).click(), 200, null, 'DELETE');
        expect(deleted.body).toEqual({ deleted: true, removed_from_people: 2 });
        const result = await today(a);
        expect(result.sources.status).toBe('partial');
        expect(result.sources.issues).toContainEqual(expect.objectContaining({ list_id: personal.id, error: 'invalid_tag' }));
        expect(reasonFor(result, people.avery.id, shared.id)).toHaveLength(1);
        expect(reasonFor(result, people.bailey.id, personal.id)).toHaveLength(0);
        await expect(a.page.getByText('Some Today rules or sources could not load. Available work is shown.', { exact: false })).toBeVisible();
        await expect(a.page.getByRole('row').filter({ hasText: people.avery.name })).toHaveCount(1);
        expect((await api(a, 'GET', path(personal))).filter_error).toBe('invalid_tag');
        expect(await api(a, 'GET', path(personal) + '/count?revision=1', undefined, 422)).toEqual({ error: 'invalid_tag' });
        await persisted(personal, a, tagged, 1, 'name.asc');
      },
      async () => {
        const a = actors.alice;
        await open(a, personal);
        await a.page.getByTestId('filter-chip-remove-tags').click();
        await a.page.getByTestId('filter-trigger-assigned_to').click();
        await a.page.getByTestId('filter-assignee-me').click();
        await expect(a.page.getByTestId('filter-chip-assigned_to')).toBeVisible();
        await a.page.keyboard.press('Escape');
        const result = await mutation(a, path(personal), () => a.page.getByRole('button', { name: 'Save', exact: true }).click(),
          200, { expected_revision: 1, filter: mine }, 'PUT');
        personal = result.body.list;
        await persisted(personal, a, mine, 2, 'name.asc');
        expect(await api(a, 'PUT', path(personal), { expected_revision: 1, name: 'stale overwrite', filter: tagged, sort: 'name.asc' }, 409))
          .toEqual({ error: 'saved_list_conflict' });
        const feed = await today(a);
        expect(feed.sources.status).toBe('complete');
        expect(reasonFor(feed, people.avery.id, personal.id)).toHaveLength(1);
        expect(reasonFor(feed, people.avery.id, shared.id)).toHaveLength(1);
        expect(feed.items.filter(item => item.person.id === people.avery.id)).toHaveLength(1);
        await a.page.reload();
        await expect(a.page.getByRole('row').filter({ hasText: people.avery.name })).toContainText(personalName);
      },
      async () => {
        const a = actors.alice;
        await open(a, personal);
        await a.page.getByRole('button', { name: 'Delete', exact: true }).click();
        await mutation(a, path(personal), () => a.page.getByRole('dialog').getByRole('button', { name: 'Delete list', exact: true }).click(),
          200, { expected_revision: 2 }, 'DELETE');
        await a.page.waitForURL('**/lists');
        await db.check('deletion clears criteria and removes source atomically', `SELECT name, filter, sort_key, sort_direction,
          revision::int, deleted_at IS NOT NULL AS deleted,
          (SELECT count(*)::int FROM today_work_source WHERE list_id=$1) AS sources FROM saved_list WHERE id=$1`,
        [personal.id], rows => expect(rows).toEqual([{ name: null, filter: null, sort_key: null, sort_direction: null, revision: 3, deleted: true, sources: 0 }]));
        const feed = await today(a);
        expect(feed.sources.status).toBe('complete');
        expect(reasonFor(feed, people.avery.id, personal.id)).toHaveLength(0);
        expect(reasonFor(feed, people.avery.id, shared.id)).toHaveLength(1);
        expect(await api(a, 'GET', path(personal), undefined, 404)).toEqual({ error: 'not_found' });
        await db.check('deleting a list does not delete People', 'SELECT id FROM person ORDER BY id', [], rows =>
          expect(rows.map(row => row.id)).toEqual(Object.values(people).map(person => person.id).sort()));
        const mocks = await (await a.page.request.get('http://mocks:9000/evidence')).json();
        expect(mocks.project).toBe(process.env.E2E_PROJECT);
        expect(mocks.requests.filter(item => item.kind === 'external')).toHaveLength(0);
        for (const actor of journey.allActors) {
          await actor.recorder.flush(); expect(actor.recorder.errors).toEqual([]);
          for (const frame of actor.recorder.frames) if (frame.message.push?.pub) {
            expect(frame.message.push.channel).toBe('org:' + actor.identity.organization.id);
            expect(frame.message.push.pub.data.organization_id).toBe(actor.identity.organization.id);
            expect(Object.keys(frame.message.push.pub.data.data).sort()).toEqual(['change', 'person_id']);
          }
        }
        writeFileSync('/artifacts/isolation.json', JSON.stringify({ project: process.env.E2E_PROJECT,
          organizations: [seed.actors.alice.organization.id, seed.actors.casey.organization.id],
          people: Object.values(people).map(person => person.id), externalRequests: 0 }, null, 2));
      },
    ]);
  } finally { await journey.close(); await db.close(); }
});
