// Controlled retained source evidence at the existing FubReader seam.
// This is deliberately not proof of the production FUB HTTPS adapter.
let phase = 1, executeRefresh = false;
export async function handleProvider(req, res, body, requests) {
  const url = new URL(req.url, 'http://mocks:9000');
  if (url.pathname === '/fub/control') {
    if (req.method === 'POST') {
      const control = typeof body === 'string' || Buffer.isBuffer(body) ? JSON.parse(String(body)) : body;
      if (control.phase !== undefined) phase = Number(control.phase);
      if (control.executeRefresh !== undefined) executeRefresh = !!control.executeRefresh;
    }
    res.setHeader('content-type', 'application/json'); res.end(JSON.stringify({ phase, executeRefresh })); return true;
  }
  if (!url.pathname.startsWith('/fub/v1/')) return false;
  requests.push({ kind: 'fub', method: req.method, path: url.pathname, query: url.search, at: Date.now(), project: process.env.E2E_PROJECT });
  res.setHeader('content-type', 'application/json');
  if (req.method !== 'GET') { res.writeHead(405); res.end(JSON.stringify({ error: 'read_only_fixture' })); return true; }
  const project = process.env.E2E_PROJECT;
  const collections = {
    people: [101, 102].map((id, i) => ({ id, firstName: i ? 'ImportedTwo' : 'ImportedOne', lastName: project,
      stage: 'Lead', assignedUserId: 7, emails: [{ value: 'shared-import@example.test', type: 'home' }],
      phones: [{ value: '+12025550123', type: 'mobile' }], created: '2026-09-01T12:00:00Z',
      source: 'Synthetic source', tags: phase === 1 ? ['Imported tag'] : [], customE2EArea: phase === 1 ? 'North' : 'South', addresses: [], relationships: [] })),
    users: [{ id: 7, name: 'Alice Agent', email: 'alice@e2e.test', role: 'Admin', status: 'Active', timezone: 'America/Los_Angeles' }],
    stages: [{ id: 11, name: 'Lead', orderWeight: 1, peopleCount: 2 }],
    customfields: [{ id: 21, name: 'customE2EArea', label: 'Imported area', type: 'text', isRecurring: false }],
    notes: [{ id: 201, personId: 101, createdById: 7, type: 'Note', body: phase === 1 ? '<p>Imported note original</p>' : '<p>Imported note revised</p>',
      isHtml: true, created: '2026-09-01T12:00:00Z', updated: phase === 1 ? '2026-09-01T12:00:00Z' : '2026-09-02T12:00:00Z' }],
    tasks: [{ id: 301, personId: 101, name: 'Imported call task', type: 'Call', createdById: 7, assignedUserId: 7,
      isCompleted: phase === 2, dueDate: '2026-09-20', created: '2026-09-01T12:00:00Z', updated: '2026-09-02T12:00:00Z',
      ...(phase === 2 ? { completed: '2026-09-02T12:00:00Z' } : {}) }],
    events: [{ id: 401, personId: 101, type: phase === 1 ? 'Inquiry' : 'Property Inquiry', created: '2026-09-01T10:00:00Z', description: 'Retained history content never exposed' }],
    calls: [], textmessages: [],
  };
  const name = url.pathname.slice('/fub/v1/'.length);
  if (name === 'identity') {
    res.end(JSON.stringify({ account: { id: 101, domain: 'Synthetic Migration ' + project }, user: { id: 7, name: 'Alice Agent' } })); return true;
  }
  if (name === 'notes/201') { res.end(JSON.stringify(collections.notes[0])); return true; }
  const collection = name === 'customFields' ? 'customfields' : name === 'textMessages' ? 'textmessages' : name;
  if (!Object.hasOwn(collections, collection)) { res.writeHead(404); res.end(JSON.stringify({ error: 'unqualified_fixture_endpoint' })); return true; }
  const records = collection === 'tasks' ? collections.tasks.filter(t => String(t.isCompleted) === url.searchParams.get('isCompleted')) : collections[collection], limit = Number(url.searchParams.get('limit') || 100), offset = Number(url.searchParams.get('offset') || 0);
  const selected = records.slice(offset, offset + limit);
  const more = offset + limit < records.length;
  res.end(JSON.stringify({ [collection]: selected,
    _metadata: { collection, offset, limit, total: records.length,
      next: more ? offset + limit : null, nextLink: more ? '/v1/' + name + '?offset=' + (offset + limit) + '&limit=' + limit : null } }));
  return true;
}
