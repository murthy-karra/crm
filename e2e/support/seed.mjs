const seeds = Object.fromEntries(['leads', 'workspace', 'tasks', 'relationships', 'lists', 'routing', 'correspondence', 'operator', 'calls', 'migration'].map(name => [name, './' + name + '-seed.mjs']));
const path = seeds[process.env.E2E_FAMILY];
if (!path) throw Error('No seed registered for family');
await import(path);
