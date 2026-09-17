import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { seedOperationalFamily } from './operational-seed.mjs';

await seedOperationalFamily({ version: 'tasks-v1', setup: async ({ request, db }) => {
  const result = await request('alice', 'POST', '/api/inquiries', { source: 'website', payload: {
    submission_id: randomUUID(), first_name: 'Task', last_name: process.env.E2E_PROJECT,
    email: 'task-person@example.test',
  } }, 201);
  await db.check('tasks seed contains one Person and no tasks', `SELECT
    (SELECT count(*)::int FROM person) AS people, (SELECT count(*)::int FROM task) AS tasks`, [],
  rows => assert.deepEqual(rows, [{ people: 1, tasks: 0 }]));
  return { personId: result.person_id, personName: 'Task ' + process.env.E2E_PROJECT };
} });
