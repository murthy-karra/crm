import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
import { seedOperationalFamily } from './operational-seed.mjs';
await seedOperationalFamily({ version: 'operator-v1', setup: async ({ actors, request, db }) => {
  const personName = 'Morgan ' + process.env.E2E_PROJECT;
  const lead = await request('alice', 'POST', '/api/inquiries', { source: 'website', payload: {
    submission_id: randomUUID(), first_name: 'Morgan', last_name: process.env.E2E_PROJECT, email: 'morgan@example.test',
  } }, 201);
  const foreign = await request('casey', 'POST', '/api/inquiries', { source: 'website', payload: {
    submission_id: randomUUID(), first_name: 'Private', last_name: process.env.E2E_PROJECT, email: 'private@example.test',
  } }, 201);
  const denied = await request('admin', 'POST', '/api/people/' + lead.person_id + '/tasks', {
    title: 'Blair exclusive follow-up', assignee_user_id: actors.blair.user.id,
  }, 201);
  await db.check('Operator seed contains only two Persons and one protected task', `SELECT
    (SELECT count(*)::int FROM person) AS people, (SELECT count(*)::int FROM task) AS tasks`, [],
  rows => assert.deepEqual(rows[0], { people: 2, tasks: 1 }));
  return { personId: lead.person_id, personName, foreignPersonId: foreign.person_id, deniedTaskId: denied.task.id };
} });
