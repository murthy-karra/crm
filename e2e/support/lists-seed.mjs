import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { seedOperationalFamily } from './operational-seed.mjs';

await seedOperationalFamily({ version: 'lists-v1', setup: async ({ actors, request, db }) => {
  const people = {};
  for (const [key, actor, first] of [['avery', 'alice', 'Avery'], ['bailey', 'blair', 'Bailey'], ['foreign', 'casey', 'Foreign']]) {
    const result = await request(actor, 'POST', '/api/inquiries', { source: 'lists_e2e', payload: {
      first_name: first, last_name: process.env.E2E_PROJECT, email: key + '@example.test', submission_id: randomUUID(),
    } }, 201);
    people[key] = { id: result.person_id, name: first + ' ' + process.env.E2E_PROJECT };
  }
  const tag = (await request('admin', 'POST', '/api/tags', { name: 'Priority book' }, 201)).tag;
  await request('alice', 'PUT', `/api/people/${people.avery.id}/tags/${tag.id}`);
  await db.check('three seeded People retain tenant and assignment boundaries', `SELECT id, organization_id, assigned_user_id
    FROM person ORDER BY id`, [], rows => assert.deepEqual(rows, [
    { id: people.avery.id, organization_id: actors.alice.organization.id, assigned_user_id: actors.alice.user.id },
    { id: people.bailey.id, organization_id: actors.blair.organization.id, assigned_user_id: actors.blair.user.id },
    { id: people.foreign.id, organization_id: actors.casey.organization.id, assigned_user_id: actors.casey.user.id },
  ].sort((a, b) => a.id.localeCompare(b.id))));
  return { people, tag };
} });
