import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
import { seedOperationalFamily } from './operational-seed.mjs';
await seedOperationalFamily({ version: 'calls-v1', setup: async ({ actors, request, db }) => {
  const lead = await request('blair', 'POST', '/api/inquiries', { source: 'website', payload: {
    submission_id: randomUUID(), first_name: 'Cameron', last_name: process.env.E2E_PROJECT,
    email: 'cameron@example.test', phone: '+12025550148',
  } }, 201);
  const detail = await request('alice', 'GET', '/api/people/' + lead.person_id);
  await db.check('call prerequisites have separate owner and no existing calls', `SELECT assigned_user_id,
    (SELECT count(*)::int FROM call) AS calls FROM person WHERE id=$1`, [lead.person_id],
  rows => assert.deepEqual(rows[0], { assigned_user_id: actors.blair.user.id, calls: 0 }));
  return { personId: lead.person_id, personName: 'Cameron ' + process.env.E2E_PROJECT,
    phoneId: detail.contact_methods.find(c => c.kind === 'phone').id };
} });
