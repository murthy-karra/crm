import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { seedOperationalFamily } from './operational-seed.mjs';

await seedOperationalFamily({ version: 'correspondence-v1', setup: async ({ actors, request, db }) => {
  const people = {};
  for (const [key, actor, first, email] of [
    ['main', 'alice', 'Avery', 'client@example.test'],
    ['second', 'alice', 'Bailey', 'second@example.test'],
    ['foreign', 'casey', 'Foreign', 'client@example.test'],
  ]) {
    const result = await request(actor, 'POST', '/api/inquiries', { source: 'correspondence_e2e', payload: {
      first_name: first, last_name: process.env.E2E_PROJECT, email, submission_id: randomUUID(),
    } }, 201);
    people[key] = { id: result.person_id, name: first + ' ' + process.env.E2E_PROJECT, email };
  }
  await db.check('correspondence seed owns three People and no captured mail', `SELECT
    (SELECT count(*)::int FROM person) AS people,
    (SELECT count(*)::int FROM correspondence_raw) AS raw,
    (SELECT count(*)::int FROM correspondence_captured) AS facts,
    (SELECT count(*)::int FROM capture_message) AS held`, [], rows =>
    assert.deepEqual(rows, [{ people: 3, raw: 0, facts: 0, held: 0 }]));
  return { people };
} });
