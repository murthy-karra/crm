import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
import { seedOperationalFamily } from './operational-seed.mjs';
await seedOperationalFamily({ version: 'relationships-v1', setup: async ({ request, db }) => {
  const lead = await request('alice', 'POST', '/api/inquiries', { source: 'website', payload: {
    submission_id: randomUUID(), first_name: 'Rowan', last_name: process.env.E2E_PROJECT,
    email: 'rowan@example.test',
  } }, 201);
  await db.check('relationships seed creates one operational Person', 'SELECT count(*)::int AS count FROM person', [],
    rows => assert.equal(rows[0].count, 1));
  return { personId: lead.person_id };
} });
