import assert from 'node:assert/strict';
import { seedOperationalFamily } from './operational-seed.mjs';
await seedOperationalFamily({ version: 'migration-v1', setup: async ({ db }) => {
  await db.check('migration starts empty and operational', `SELECT
    (SELECT count(*)::int FROM person) AS people,
    (SELECT count(*)::int FROM organization WHERE workspace_mode='operational') AS operational`, [],
  rows => assert.deepEqual(rows, [{ people: 0, operational: 2 }]));
  return {};
} });
