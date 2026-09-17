import { expect } from '@playwright/test';
export async function connected(actor, since = 0) {
  await expect.poll(() => actor.recorder.frames.filter(f => f.at >= since &&
    f.message.connect?.subs?.['org:' + actor.identity.organization.id]).length,
  { message: actor.key + ' has a real server-authorized subscription' }).toBeGreaterThan(0);
}
export async function mutation(actor, path, action, status = 200, expectedBody = null, method = 'POST') {
  const responsePromise = actor.page.waitForResponse(r =>
    new URL(r.url()).pathname === path && r.request().method() === method);
  await action();
  const response = await responsePromise;
  expect(response.status(), path).toBe(status);
  if (response.request().postData() !== null) expect(response.request().headers()['content-type']).toContain('application/json');
  expect(response.headers()['x-request-id']).toBeTruthy();
  if (expectedBody) expect(response.request().postDataJSON()).toMatchObject(expectedBody);
  const body = response.status() === 204 ? null : await response.json();
  return { body, request: response.request().postDataJSON() };
}
export async function form(actor, values, source = 'website') {
  await actor.page.goto('/intake/new');
  await actor.page.getByLabel('Source', { exact: true }).fill(source);
  await actor.page.getByLabel('First name', { exact: true }).fill(values.first);
  await actor.page.getByLabel('Last name', { exact: true }).fill(values.last);
  await actor.page.getByLabel('Email', { exact: true }).fill(values.email);
  if (values.phone) await actor.page.getByLabel('Phone', { exact: true }).fill(values.phone);
}
export async function addLead(actor, values, source = 'website') {
  await form(actor, values, source);
  const result = await mutation(actor, '/api/inquiries',
    () => actor.page.getByRole('button', { name: 'Add lead', exact: true }).click(), 201,
    { source, payload: { first_name: values.first, last_name: values.last, email: values.email } });
  expect(result.request.payload.submission_id).toMatch(/^[a-f0-9-]{36}$/);
  await actor.page.waitForURL('**/people/' + result.body.person_id);
  await expect(actor.page.getByRole('heading', { name: values.first + ' ' + values.last, exact: true })).toBeVisible();
  return result;
}
export async function propagated(actor, change, id, since, readPath) {
  await expect.poll(() => actor.recorder.frames.find(f => f.at >= since &&
    f.message.push?.pub?.data?.data?.person_id === id &&
    f.message.push.pub.data.data.change === change),
  { message: 'real Centrifugo ' + change + ' reaches ' + actor.key }).toBeTruthy();
  const frame = actor.recorder.frames.find(f => f.at >= since &&
    f.message.push?.pub?.data?.data?.person_id === id &&
    f.message.push.pub.data.data.change === change);
  expect(frame.message.push.channel).toBe('org:' + actor.identity.organization.id);
  expect(frame.message.push.pub.data.organization_id).toBe(actor.identity.organization.id);
  expect(Object.keys(frame.message.push.pub.data.data).sort()).toEqual(['change', 'person_id']);
  await expect.poll(() => actor.recorder.http.find(r => r.started >= frame.at &&
    r.method === 'GET' && r.path === readPath && r.status === 200),
  { timeout: 8_000, message: 'event causes authoritative refetch before the 60-second poll' }).toBeTruthy();
}

export async function select(actor, label, option) {
  await actor.page.getByRole('combobox', { name: label, exact: true }).first().click();
  await actor.page.getByRole('option', { name: option, exact: true }).click();
}
