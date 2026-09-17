import assert from 'node:assert/strict';
import { seedOperationalFamily } from './operational-seed.mjs';
await seedOperationalFamily({ version: 'routing-v1', setup: async ({ db }) => {
  await db.check('routing begins with no intake or rotation', `SELECT
    (SELECT count(*)::int FROM person) AS people,
    (SELECT count(*)::int FROM raw_payload) AS raw,
    (SELECT count(*)::int FROM intake_rotation) AS rotation`, [],
  rows => assert.deepEqual(rows, [{ people: 0, raw: 0, rotation: 0 }]));
  return {};
} });
