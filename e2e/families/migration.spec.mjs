import { test, expect } from '@playwright/test';
import { readFileSync, writeFileSync } from 'node:fs';
import { createJourney } from '../support/journey.mjs';
import { mutation } from '../support/actions.mjs';
import { database, evidence, remember } from '../support/evidence.mjs';
const names = JSON.parse(readFileSync(new URL('./registry.json', import.meta.url))).migration.steps;
test('migration-v1: source evidence → frozen plan → administrator review', async ({ browser }, testInfo) => {
  test.setTimeout(600000);
  const seed = JSON.parse(readFileSync('/private/seed.json'));
  const journey = createJourney(browser, testInfo), db = await database();
  const { actors, allActors } = journey, root = '/api/migrations/fub';
  const project = process.env.E2E_PROJECT, credential = 'synthetic-migration-e2e'; remember(credential);
  let connection, assessment, snapshot, preview, imported, initialPlan, readyPlan, callsBeforeImport;
  let metadataId, activityId, historyCapture, historyImport, newerSnapshot, reportId, bundleId;
  const people = [];
  const panel = () => actors.admin.page.getByTestId('people-import-panel');
  async function api(actor, method, path, data, status = 200) {
    const response = await actor.page.request.fetch(path, { method, data });
    const body = response.status() === 204 ? null : await response.json();
    if (path.endsWith('/reconciliation')) expect(response.headers()['cache-control']).toBe('no-store');
    evidence('http', { actor: actor.key, method, path, request: data, status: response.status(), response: body });
    expect(response.status(), method + ' ' + path).toBe(status); return body;
  }
  async function settled(path, select, expected) {
    let body;
    await expect.poll(async () => { body = await api(actors.admin, 'GET', path); return select(body); },
      { timeout: 90000, intervals: [250, 500, 1000], message: path + ' reaches ' + expected }).toBe(expected);
    return body;
  }
  async function control(value) {
    const response = await fetch('http://mocks:9000/fub/control', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(value) });
    expect(response.ok).toBe(true);
  }
  async function checkAll(scope) {
    const boxes = scope.getByRole('checkbox');
    await expect(boxes.first()).toBeVisible();
    for (const box of await boxes.all()) await box.check();
  }
  async function openMigration() { await actors.admin.page.goto('/manage/migration'); }
  async function captureHistory() {
    await openMigration();
    const p = actors.admin.page.getByTestId('history-capture-panel');
    await p.getByLabel('People import for historical capture', { exact: true }).selectOption(imported);
    const proposed = await mutation(actors.admin, root + '/history-captures', () => p.getByRole('button', { name: 'Prepare historical capture', exact: true }).click(), 201);
    const id = proposed.body.capture_id;
    await checkAll(p);
    await p.getByRole('button', { name: 'Review historical capture confirmation', exact: true }).click();
    await mutation(actors.admin, root + '/history-captures/' + id + '/confirm', () => actors.admin.page.getByRole('dialog').getByRole('button', { name: 'Start historical capture', exact: true }).click(), 202);
    await settled(root + '/history-captures/' + id, b => b.state, 'completed_with_gaps');
    return id;
  }
  async function refreshReady() {
    let detail;
    await expect.poll(async () => {
      detail = await api(actors.admin, 'GET', root + '/family-refreshes/' + bundleId);
      return detail.families.every(f => ['ready', 'completed', 'cancelled'].includes(f.state));
    }, { timeout: 90000, intervals: [250, 500, 1000] }).toBe(true);
    return detail;
  }
  async function confirmRefresh() {
    const p = actors.admin.page.getByTestId('family-refresh');
    const detail = await refreshReady();
    await p.getByRole('button', { name: 'Refresh review', exact: true }).click();
    const selected = p.getByRole('checkbox', { name: 'Include in confirmation', exact: true });
    await expect(selected).toHaveCount(detail.families.length);
    for (const box of await selected.all()) await box.check();
    await p.getByRole('checkbox', { name: /^I reviewed every selected family/ }).check();
    await p.getByRole('button', { name: 'Review confirmation', exact: true }).click();
    return mutation(actors.admin, root + '/family-refreshes/' + bundleId + '/confirm', () => actors.admin.page.getByRole('dialog').getByRole('button', { name: 'Apply reviewed refresh', exact: true }).click(), 200);
  }
  const actions = [
    async () => {
      expect(seed.version).toBe('migration-v1');
      for (const key of ['admin', 'alice', 'casey']) await journey.login(key, seed.actors[key]);
      await actors.admin.page.goto('/manage/migration');
      await actors.admin.page.getByTestId('fub-api-key').fill(credential);
      const response = await mutation(actors.admin, root + '/connections', () => actors.admin.page.getByTestId('save-fub-credential').click(), 201);
      connection = response.body.connection;
      expect(connection.status).toBe('connected'); expect(connection.source_account_id).toBe(101);
      await expect(actors.admin.page.getByTestId('fub-api-key')).toHaveValue('');
      expect(await api(actors.alice, 'GET', root + '/', undefined, 403)).toEqual({ error: 'forbidden' });
      expect((await api(actors.casey, 'GET', root + '/')).connection).toBeNull();
      await db.check('source connection does not populate CRM', 'SELECT count(*)::int AS n FROM person', [], r => expect(r[0].n).toBe(0));
    },
    async () => {
      const result = await mutation(actors.admin, root + '/assessments', () => actors.admin.page.getByTestId('assess-fub').click(), 202);
      assessment = result.body.assessment.id;
      const report = await settled(root + '/assessments/' + assessment, b => b.assessment.state, 'completed');
      expect(report.assessment.checks.length).toBeGreaterThanOrEqual(6);
      expect(report.assessment.checks.some(c => c.check_key === 'people_including_trash')).toBe(true);
      await actors.admin.page.reload();
      await expect(actors.admin.page.getByTestId('assessment-report')).toBeVisible();
      expect((await api(actors.admin, 'GET', root + '/assessments/' + assessment)).assessment.id).toBe(assessment);
    },
    async () => {
      const result = await mutation(actors.admin, root + '/snapshots', () => actors.admin.page.getByTestId('snapshot-prepare').click(), 201);
      snapshot = result.body.snapshot.id;
      await mutation(actors.admin, root + '/snapshots/' + snapshot + '/confirm', () => actors.admin.page.getByRole('dialog').getByRole('button', { name: 'Start source capture', exact: true }).click(), 202);
      const capture = await settled(root + '/snapshots/' + snapshot, b => b.snapshot.state, 'completed');
      expect(capture.streams.find(s => s.stream === 'people').distinct_ids).toBe('2');
      expect(capture.coverage.length).toBeGreaterThan(0);
      expect(Number(capture.snapshot.accepted_captures)).toBeGreaterThan(0);
      await db.check('retained capture performs no CRM import', 'SELECT count(*)::int AS n FROM person', [], r => expect(r[0].n).toBe(0));
    },
    async () => {
      const result = await mutation(actors.admin, root + '/snapshots/' + snapshot + '/previews', () => actors.admin.page.getByTestId('snapshot-generate-preview').click(), 202);
      preview = result.body.preview_id;
      await settled(root + '/snapshots/' + snapshot + '/previews/' + preview, b => b.preview.state, 'completed');
      const groups = await api(actors.admin, 'GET', root + '/snapshots/' + snapshot + '/previews/' + preview + '/overlap-groups?limit=50');
      expect(groups.groups.length).toBeGreaterThan(0);
      await panel().getByRole('button', { name: 'Refresh import status', exact: true }).click();
      await panel().getByLabel('Retained snapshot', { exact: true }).selectOption(snapshot);
      await panel().getByLabel('Completed preview', { exact: true }).selectOption(preview);
      const planned = await mutation(actors.admin, root + '/imports', () => panel().getByRole('button', { name: 'Plan People import', exact: true }).click(), 201);
      imported = planned.body.import_id;
      const plan = await settled(root + '/imports/' + imported, b => b.plan.state, 'ready'); initialPlan = plan.plan.id;
      expect(plan.counts.source_people).toBe('2');
      callsBeforeImport = (await (await fetch('http://mocks:9000/evidence')).json()).requests.filter(r => r.kind === 'fub').length;
    },
    async () => {
      const stage = seed.stages.find(s => s.name === 'Lead');
      await actors.admin.page.getByRole('button', { name: 'Stage mappings', exact: true }).click();
      await actors.admin.page.locator('select[aria-label^="Stage choice for source"]').selectOption('existing:' + stage.id);
      await actors.admin.page.getByRole('button', { name: 'Assignment mappings', exact: true }).click();
      await actors.admin.page.locator('select[aria-label^="Assignment choice for source"]').selectOption('member:' + actors.alice.identity.user.id);
      await mutation(actors.admin, root + '/imports/' + imported + '/plans', () => actors.admin.page.getByRole('button', { name: /Apply \d+ mapping changes/ }).click(), 202);
      readyPlan = await settled(root + '/imports/' + imported, b => b.plan.state, 'ready');
      expect(readyPlan.plan.id).not.toBe(initialPlan);
      expect(readyPlan.counts.eligible_people).toBe('2'); expect(readyPlan.counts.held_people).toBe('0');
      expect(readyPlan.counts.overlap_people).toBe('2');
      await db.check('mapping revisions are immutable before business writes', `SELECT
        (SELECT count(*)::int FROM person) AS people,
        (SELECT state FROM migration_import_plan WHERE id=$1) AS prior`, [initialPlan], r => expect(r).toEqual([{ people: 0, prior: 'superseded' }]));
    },
    async () => {
      await panel().getByLabel(/I acknowledge .* held People/).check();
      await panel().getByLabel('Keep this Organization for administrator review only.', { exact: true }).check();
      await panel().getByLabel('I acknowledge the remaining data is not imported.', { exact: true }).check();
      await panel().getByRole('button', { name: 'Review People import', exact: true }).click();
      const result = await mutation(actors.admin, root + '/imports/' + imported + '/confirm', () => actors.admin.page.getByRole('dialog').getByRole('button', { name: 'Confirm People import', exact: true }).click(), 202);
      expect(result.request.plan_id).toBe(readyPlan.plan.id);
      const completed = await settled(root + '/imports/' + imported, b => b.state, 'completed');
      expect(completed.counts.imported_people).toBe('2'); expect(completed.workspace.mode).toBe('migration_review');
      await db.check('exact People are imported under permanent review hold', `SELECT
        (SELECT count(*)::int FROM person WHERE organization_id=$1) AS people,
        (SELECT workspace_mode FROM organization WHERE id=$1) AS mode,
        (SELECT count(*)::int FROM inquiry) AS inquiries,
        (SELECT count(*)::int FROM contact_attempted) AS attempts`, [actors.admin.identity.organization.id],
      r => expect(r).toEqual([{ people: 2, mode: 'migration_review', inquiries: 0, attempts: 0 }]));
    },
    async () => {
      const results = await api(actors.admin, 'GET', root + '/imports/' + imported + '/results?limit=50');
      evidence('migration-results', results);
      await db.check('source identities preserve overlapping People separately', `SELECT source_id, target_id FROM migration_import_identity
        WHERE organization_id=$1 AND family='people' ORDER BY source_id`, [actors.admin.identity.organization.id], rows => {
        expect(rows.map(r => r.source_id)).toEqual(['101', '102']); people.push(...rows.map(r => r.target_id)); expect(new Set(people).size).toBe(2);
      });
      await db.check('shared email survives as two scoped imported contacts', `SELECT count(*)::int AS n, count(DISTINCT person_id)::int AS people
        FROM contact_method WHERE normalized_value='shared-import@example.test'`, [], r => expect(r).toEqual([{ n: 2, people: 2 }]));
      for (let i = 0; i < people.length; i++) {
        await actors.admin.page.goto('/people/' + people[i]);
        await expect(actors.admin.page.getByRole('heading', { name: (i ? 'ImportedTwo' : 'ImportedOne') + ' ' + project, exact: true })).toBeVisible();
        await expect(actors.admin.page.getByText('FUB Person ' + (101+i), { exact: false })).toBeVisible();
        const provenance = await api(actors.admin, 'GET', '/api/people/' + people[i] + '/import-provenance');
        expect(provenance.source_id).toBe(String(101+i));
      }
    },
    async () => {
      for (const actor of [actors.admin, actors.alice]) {
        const today = await api(actor, 'GET', '/api/today', undefined, 409);
        expect(today.error).toBe('workspace_in_migration_review');
        const denied = await api(actor, 'POST', '/api/people/' + people[0] + '/tasks', { title: 'Must never execute' }, 409);
        expect(denied.error).toBe('workspace_in_migration_review');
      }
      expect((await api(actors.alice, 'GET', '/api/people/' + people[0], undefined, 409)).error).toBeTruthy();
      expect(await api(actors.casey, 'GET', '/api/people/' + people[0], undefined, 404)).toEqual({ error: 'not_found' });
      expect(await api(actors.casey, 'GET', root + '/imports/' + imported, undefined, 404)).toEqual({ error: 'not_found' });
      await db.check('hold denials leave no tasks or native contact credit', `SELECT
        (SELECT count(*)::int FROM task) AS tasks, (SELECT count(*)::int FROM contact_attempted) AS contacts`, [],
      r => expect(r).toEqual([{ tasks: 0, contacts: 0 }]));
    },
    async () => {
      await openMigration(); const p = actors.admin.page.getByTestId('metadata-import-panel');
      await p.getByLabel('Completed People import', { exact: true }).selectOption(imported);
      const prepared = await mutation(actors.admin, root + '/metadata-imports', () => p.getByRole('button', { name: 'Prepare tags and fields plan', exact: true }).click(), 201);
      metadataId = prepared.body.import.id;
      await settled(root + '/metadata-imports/' + metadataId, b => b.latest_plan?.state, 'ready');
      for (const kind of ['Tags', 'Fields']) {
        await p.getByRole('button', { name: kind, exact: true }).click();
        await p.locator('select[aria-label*="choice for"]').selectOption('create_matching');
      }
      await mutation(actors.admin, root + '/metadata-imports/' + metadataId + '/plans', () => p.getByRole('button', { name: /Apply \d+ mapping changes/ }).click(), 202);
      const ready = await settled(root + '/metadata-imports/' + metadataId, b => b.latest_plan?.state, 'ready');
      expect(ready.latest_plan.counts.held_count).toBe('0');
      await checkAll(p); await p.getByRole('button', { name: 'Review metadata confirmation', exact: true }).click();
      await mutation(actors.admin, root + '/metadata-imports/' + metadataId + '/confirm', () => actors.admin.page.getByRole('dialog').getByRole('button', { name: 'Confirm metadata import', exact: true }).click(), 202);
      await settled(root + '/metadata-imports/' + metadataId, b => b.state, 'completed');
      await db.check('imported metadata uses both separate People', `SELECT
        (SELECT count(*)::int FROM person_tag) AS links,
        (SELECT count(*)::int FROM person_custom_field_value WHERE text_value='North') AS values`, [],
      r => expect(r).toEqual([{ links: 2, values: 2 }]));
    },
    async () => {
      await openMigration(); const p = actors.admin.page.getByTestId('activity-import-panel');
      await p.getByLabel('People import for notes and tasks', { exact: true }).selectOption(imported);
      const prepared = await mutation(actors.admin, root + '/activity-imports', () => p.getByRole('button', { name: 'Prepare notes and tasks plan', exact: true }).click(), 201);
      activityId = prepared.body.import.id;
      await settled(root + '/activity-imports/' + activityId, b => b.latest_plan?.state, 'ready');
      for (const role of ['Note author', 'Task creator', 'Task assignee', 'Task kind']) {
        await p.locator('[aria-label="Activity mapping role"]').getByRole('button', { name: new RegExp('^' + role + '$', 'i') }).click();
        if (role === 'Task kind') await p.locator('select[aria-label*="choice for"]').selectOption('map_kind:call');
        else {
          await p.getByRole('button', { name: 'Choose CRM user', exact: true }).click();
          await actors.admin.page.getByRole('dialog').getByRole('button', { name: 'Use Alice Agent', exact: true }).click();
        }
      }
      await p.getByLabel('IANA source timezone', { exact: true }).fill('America/Los_Angeles');
      await p.getByLabel('I confirm this source timezone applies to the selected date-only records.', { exact: true }).check();
      await mutation(actors.admin, root + '/activity-imports/' + activityId + '/plans', () => p.getByRole('button', { name: 'Apply activity choices', exact: true }).click(), 202);
      const ready = await settled(root + '/activity-imports/' + activityId, b => b.latest_plan?.state, 'ready');
      expect(ready.latest_plan.counts.notes.eligible).toBe('1'); expect(ready.latest_plan.counts.tasks.eligible).toBe('1');
      await checkAll(p); await p.getByRole('button', { name: 'Review notes and tasks confirmation', exact: true }).click();
      await mutation(actors.admin, root + '/activity-imports/' + activityId + '/confirm', () => actors.admin.page.getByRole('dialog').getByRole('button', { name: 'Confirm notes and tasks import', exact: true }).click(), 202);
      await settled(root + '/activity-imports/' + activityId, b => b.state, 'completed');
      await db.check('HTML is readable text and explicit source timezone fixes due instant', `SELECT
        (SELECT body FROM note) AS body, (SELECT due_at::text FROM task) AS due,
        (SELECT completed_at FROM task) AS completed`, [], r => {
        expect(r[0].body.trim()).toBe('Imported note original'); expect(new Date(r[0].due).toISOString()).toBe('2026-09-21T06:59:59.000Z'); expect(r[0].completed).toBeNull();
      });
    },
    async () => {
      historyCapture = await captureHistory();
      await openMigration(); const p = actors.admin.page.getByTestId('history-import-panel');
      await p.getByLabel('People import for historical records', { exact: true }).selectOption(imported);
      await p.getByLabel('Retained historical capture to import', { exact: true }).selectOption(historyCapture);
      const prepared = await mutation(actors.admin, root + '/history-imports', () => p.getByRole('button', { name: 'Prepare history import', exact: true }).click(), 201);
      historyImport = prepared.body.import_id;
      await settled(root + '/history-imports/' + historyImport, b => b.state, 'ready');
      await checkAll(p); await p.getByRole('button', { name: 'Review confirmation', exact: true }).click();
      await mutation(actors.admin, root + '/history-imports/' + historyImport + '/confirm', () => actors.admin.page.getByRole('dialog').getByRole('button', { name: 'Import records', exact: true }).click(), 202);
      await settled(root + '/history-imports/' + historyImport, b => b.state, 'completed');
      await db.check('imported historical fact does not create native contact credit', `SELECT
        (SELECT count(*)::int FROM fub_event_record_imported) AS imported,
        (SELECT count(*)::int FROM inquiry) AS inquiries,
        (SELECT count(*)::int FROM contact_attempted) AS attempts`, [], r => expect(r).toEqual([{ imported: 1, inquiries: 0, attempts: 0 }]));
      await actors.admin.page.goto('/people/' + people[0]);
      await actors.admin.page.getByLabel('History family', { exact: true }).selectOption('events');
      await expect(actors.admin.page.getByText('Retained history content never exposed', { exact: false })).toHaveCount(0);
    },
    async () => {
      await control({ phase: 2 }); await openMigration();
      const proposed = await mutation(actors.admin, root + '/snapshots', () => actors.admin.page.getByTestId('snapshot-prepare').click(), 201);
      newerSnapshot = proposed.body.snapshot.id;
      await mutation(actors.admin, root + '/snapshots/' + newerSnapshot + '/confirm', () => actors.admin.page.getByRole('dialog').getByRole('button', { name: 'Start source capture', exact: true }).click(), 202);
      await settled(root + '/snapshots/' + newerSnapshot, b => b.snapshot.state, 'completed');
      await openMigration(); const p = actors.admin.page.getByTestId('core-change-reports');
      await p.getByLabel('Completed People import', { exact: true }).selectOption(imported);
      await p.getByLabel('Newer retained core capture', { exact: true }).selectOption(newerSnapshot);
      const compared = await mutation(actors.admin, root + '/core-change-reports', () => p.getByRole('button', { name: 'Assess changes', exact: true }).click(), 201);
      reportId = compared.body.report_id; await settled(root + '/core-change-reports/' + reportId, b => b.state, 'completed');
      historyCapture = await captureHistory();
      callsBeforeImport = (await (await fetch('http://mocks:9000/evidence')).json()).requests.filter(r => r.kind === 'fub').length;
      await db.check('later capture and comparison do not overwrite imported values', `SELECT
        (SELECT count(*)::int FROM person_tag) AS links,
        (SELECT count(*)::int FROM person_custom_field_value WHERE text_value='North') AS values,
        (SELECT completed_at FROM task) AS completed`, [], r => expect(r).toEqual([{ links: 2, values: 2, completed: null }]));
    },
    async () => {
      await openMigration(); const p = actors.admin.page.getByTestId('family-refresh');
      await p.getByLabel('Original People import', { exact: true }).selectOption(imported);
      await p.getByLabel('Completed core comparison', { exact: true }).selectOption(reportId);
      await p.getByLabel('Retained history capture', { exact: true }).selectOption(historyCapture);
      for (const label of ['Tags and custom fields', 'Notes and tasks', 'Historical facts']) await p.getByRole('checkbox', { name: label, exact: true }).check();
      const prepared = await mutation(actors.admin, root + '/family-refreshes', () => p.getByRole('button', { name: 'Prepare family refresh', exact: true }).click(), 201);
      bundleId = prepared.body.bundle_id; await refreshReady();
      for (const family of ['Tags and custom fields', 'Notes and tasks']) {
        await p.locator('[aria-label="Refresh family"]').getByRole('button', { name: family, exact: true }).click();
        const selects = p.locator('[aria-label="Refresh mappings"] select[aria-label^="Mapping choice for"]');
        await expect(selects.first()).toBeVisible();
        for (const select of await selects.all()) {
          const row = select.locator('xpath=ancestor::li');
          if (/task kind/i.test(await row.innerText())) await select.selectOption('kind:call');
          else {
            await row.getByRole('button', { name: 'Choose existing destination', exact: true }).click();
            const picker = p.locator('[aria-label="Choose existing refresh destination"]');
            await picker.getByRole('button', { name: family === 'Notes and tasks' ? 'Choose Alice Agent' : /^Choose /, exact: family === 'Notes and tasks' }).first().click();
          }
        }
        if (family === 'Notes and tasks') await p.getByPlaceholder('America/Los_Angeles').fill('America/Los_Angeles');
        await mutation(actors.admin, root + '/family-refreshes/' + bundleId + '/plans', () => p.getByRole('button', { name: 'Apply mapping choices', exact: true }).click(), 201);
        await refreshReady();
      }
      const ready = await refreshReady();
      expect(Number(ready.families.find(f => f.family === 'metadata').counts.tag_removals)).toBe(2);
      expect(Number(ready.families.find(f => f.family === 'activity').counts.task_completions)).toBe(1);
      expect(Number(ready.families.find(f => f.family === 'history').counts.history_corrections)).toBe(1);
    },
    async () => {
      await confirmRefresh(); const p = actors.admin.page.getByTestId('family-refresh');
      await p.locator('[aria-label="Refresh family"]').getByRole('button', { name: 'Notes and tasks', exact: true }).click();
      await mutation(actors.admin, root + '/family-refreshes/' + bundleId + '/cancel', () => p.getByRole('button', { name: 'Cancel Notes and tasks', exact: true }).click(), 200);
      await control({ executeRefresh: true });
      const done = await settled(root + '/family-refreshes/' + bundleId, b => b.bundle.state, 'cancelled');
      expect(done.families.find(f => f.family === 'activity').state).toBe('cancelled');
      expect(done.families.filter(f => f.family !== 'activity').every(f => f.state === 'completed')).toBe(true);
      await db.check('cancelled activity is untouched while other families settle', `SELECT
        (SELECT count(*)::int FROM person_tag) AS links,
        (SELECT count(*)::int FROM person_custom_field_value WHERE text_value='South') AS values,
        (SELECT completed_at FROM task) AS completed,
        (SELECT count(*)::int FROM fub_event_record_imported) AS original,
        (SELECT count(*)::int FROM fub_event_record_corrected) AS corrections`, [],
      r => expect(r).toEqual([{ links: 0, values: 2, completed: null, original: 1, corrections: 1 }]));
    },
    async () => {
      const p = actors.admin.page.getByTestId('family-refresh');
      const remainder = await mutation(actors.admin, root + '/family-refreshes/' + bundleId + '/remainder', () => p.getByRole('button', { name: 'Prepare exact remainder', exact: true }).click(), 201);
      bundleId = remainder.body.bundle_id; const ready = await refreshReady();
      expect(ready.families.map(f => f.family)).toEqual(['activity']);
      await confirmRefresh(); await settled(root + '/family-refreshes/' + bundleId, b => b.bundle.state, 'completed');
      await db.check('exact remainder applies activity and retains immutable original history', `SELECT
        (SELECT body FROM note) AS body,
        (SELECT completed_at IS NOT NULL FROM task) AS completed,
        (SELECT version::int FROM migration_family_refresh_history_head) AS version,
        (SELECT workspace_mode FROM organization WHERE id=$1) AS mode`, [actors.admin.identity.organization.id], r => {
        expect(r[0].body.trim()).toBe('Imported note revised'); expect(r[0]).toMatchObject({ completed: true, version: 2, mode: 'migration_review' });
      });
    },
    async () => {
      await actors.admin.page.goto('/manage/migration');
      await panel().getByLabel('Existing People import', { exact: true }).selectOption(imported);
      await expect(panel()).toContainText(/administrator review/i);
      const final = await api(actors.admin, 'GET', root + '/imports/' + imported);
      expect(final.state).toBe('completed'); expect(final.counts.imported_people).toBe('2');
      const reconciliation = actors.admin.page.getByTestId('migration-reconciliation');
      await reconciliation.getByLabel('People import for reconciliation', { exact: true }).selectOption(imported);
      await expect(reconciliation).toContainText(/standalone tag/i);
      await expect(reconciliation).toContainText(/unqualified/i);
      const reconciled = await api(actors.admin, 'GET', root + '/imports/' + imported + '/reconciliation');
      expect(reconciled.original_import_id).toBe(imported);
      expect(reconciled.review_hold).toBe(true);
      const originalPeople = reconciled.families.find(f => f.coverage.family === 'people_contacts');
      expect(originalPeople.unit).toBe('people');
      expect(originalPeople.cohorts.find(c => c.cohort_origin === 'original').result_totals.applied).toBe('2');
      for (const [family, count] of [['embedded_person_tags', '2'], ['custom_fields', '2'], ['notes', '1'], ['tasks', '1'], ['historical_events', '1']]) {
        const outcome = reconciled.families.find(f => f.coverage.family === family).cohorts.find(c => c.cohort_origin === 'original');
        expect(outcome.result_totals.applied, family + ' initial outcomes').toBe(count);
        expect(outcome.latest_bundle.state, family + ' latest family state').toBe('completed');
        expect(outcome.latest_bundle.outcome_totals.applied, family + ' latest outcomes').toBe(count);
        if (family === 'notes' || family === 'tasks') expect(outcome.latest_bundle.bundle_id).toBe(bundleId);
        else expect(outcome.latest_bundle.bundle_id).not.toBe(bundleId);
      }
      const unsupportedCatalog = reconciled.families.find(f => f.coverage.family === 'standalone_tag_catalog');
      expect(unsupportedCatalog.coverage.path).toBe('unqualified_source');
      expect(unsupportedCatalog.cohorts).toEqual([]);
      const desktopViewport = actors.admin.page.viewportSize();
      await actors.admin.page.setViewportSize({ width: 390, height: 844 });
      const peopleEvidence = reconciliation.locator('[data-family="people_contacts"] details').filter({ hasText: 'Evidence references' });
      await peopleEvidence.getByText('Evidence references', { exact: true }).click();
      await expect(peopleEvidence).toContainText(imported);
      await reconciliation.getByRole('heading', { name: 'Migration reconciliation', exact: true }).scrollIntoViewIfNeeded();
      expect(await reconciliation.evaluate(element => element.scrollWidth <= element.clientWidth + 1)).toBe(true);
      await testInfo.attach('migration-reconciliation-mobile', {
        body: await actors.admin.page.screenshot(), contentType: 'image/png',
      });
      await peopleEvidence.scrollIntoViewIfNeeded();
      await testInfo.attach('migration-reconciliation-evidence-mobile', {
        body: await actors.admin.page.screenshot(), contentType: 'image/png',
      });
      if (desktopViewport) await actors.admin.page.setViewportSize(desktopViewport);
      await api(actors.alice, 'GET', root + '/imports/' + imported + '/reconciliation', undefined, 403);
      await api(actors.casey, 'GET', root + '/imports/' + imported + '/reconciliation', undefined, 404);
      for (const actor of allActors) {
        await actor.recorder.flush(); expect(actor.recorder.errors).toEqual([]);
        for (const f of actor.recorder.frames) if (f.message.push?.pub) expect(f.message.push.pub.data.organization_id).toBe(actor.identity.organization.id);
      }
      const mock = await (await fetch('http://mocks:9000/evidence')).json();
      expect(mock.project).toBe(project); expect(mock.requests.filter(r => r.kind === 'external')).toEqual([]);
      expect(mock.requests.filter(r => r.kind === 'fub')).toHaveLength(callsBeforeImport);
      expect(mock.requests.filter(r => r.kind === 'fub').every(r => r.method === 'GET')).toBe(true);
      evidence('mocks', mock);
      writeFileSync('/artifacts/isolation.json', JSON.stringify({ project,
        organizations: [actors.admin.identity.organization.id, actors.casey.identity.organization.id], people, externalRequests: 0 }));
    },
  ];
  try { await journey.run(names, actions); } finally { await journey.close(); await db.close(); }
});
