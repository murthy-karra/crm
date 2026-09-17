import { test, expect } from '@playwright/test';
import { writeFileSync, renameSync } from 'node:fs';
import { observe } from './evidence.mjs';
import { connected, mutation } from './actions.mjs';

export function createJourney(browser, testInfo) {
  const actors = {}, allActors = [];
  async function createActor(key, identity = null) {
    const context = await browser.newContext({ baseURL: 'http://web:8080',
      timezoneId: 'America/Los_Angeles', locale: 'en-US',
      recordVideo: { dir: '/artifacts/videos', size: { width: 1280, height: 720 } } });
    // Prevent invitation credentials appearing in video/screenshots. This only
    // masks their rendered text; the actual input value remains testable.
    await context.addInitScript(() => {
      const install = () => {
        if (!document.documentElement) return false;
        const style = document.createElement('style');
        style.textContent = '[role="dialog"] input[readonly], [data-testid="intake-address"], [data-testid="capture-address"], [data-testid="capture-signature-snippet"] { -webkit-text-security: disc !important; }';
        document.documentElement.appendChild(style);
        return true;
      };
      if (!install()) {
        const observer = new MutationObserver(() => { if (install()) observer.disconnect(); });
        observer.observe(document, { childList: true });
      }
    });
    await context.tracing.start({ screenshots: true, snapshots: true, sources: true });
    const page = await context.newPage();
    const actor = { context, page, recorder: observe(page, key), key, identity };
    actors[key] = actor; allActors.push(actor);
    return actor;
  }
  async function signIn(actor, identity, status = 200) {
    await actor.page.goto('/login');
    await actor.page.getByLabel('Email', { exact: true }).fill(identity.email ?? identity.user.email);
    await actor.page.getByLabel('Password', { exact: true }).fill(process.env.E2E_PASSWORD);
    const result = await mutation(actor, '/api/session',
      () => actor.page.getByRole('button', { name: 'Sign in', exact: true }).click(), status);
    if (status === 200) {
      actor.identity = result.body;
      await actor.page.waitForURL(result.body.organization ? '**/today' : '**/platform');
      if (result.body.organization) await connected(actor);
    }
    return result.body;
  }
  async function login(key, identity) {
    const actor = await createActor(key, identity);
    await signIn(actor, identity);
    return actor;
  }
  async function closeActor(actor) {
    if (actor.closed) return;
    let recordingError;
    try { await actor.recorder.flush(); } catch (error) { recordingError = error; }
    const label = actor.key + '-' + allActors.indexOf(actor);
    await actor.context.tracing.stop({ path: '/artifacts/' + label + '-trace.zip' });
    await actor.context.close(); actor.closed = true;
    const path = '/artifacts/videos/' + label + '.webm';
    renameSync(await actor.page.video().path(), path);
    await testInfo.attach('video-' + label, { path, contentType: 'video/webm' });
    if (recordingError) throw recordingError;
  }
  async function run(names, actions) {
    expect(actions).toHaveLength(names.length);
    const steps = names.map(name => ({ name, status: 'blocked', reason: 'prerequisite has not completed' }));
    const save = () => writeFileSync('/artifacts/steps.json', JSON.stringify(steps, null, 2));
    save();
    try {
      for (let i = 0; i < actions.length; i++) {
        await test.step(names[i], async () => {
          steps[i] = { name: names[i], status: 'running', started: Date.now() }; save();
          try {
            if (Number(process.env.E2E_FAIL_STEP) === i + 1) throw Error('intentional harness diagnostics self-check');
            await actions[i](); steps[i].status = 'passed';
          } catch (error) {
            steps[i].status = 'failed'; steps[i].error = String(error);
            for (const actor of allActors.filter(a => !a.closed)) {
              await actor.page.screenshot({ path: '/artifacts/failure-' + actor.key + '.png', fullPage: true }).catch(() => {});
            }
            throw error;
          } finally { steps[i].finished = Date.now(); save(); }
        });
      }
    } finally { await testInfo.attach('family-steps', { path: '/artifacts/steps.json', contentType: 'application/json' }); }
  }
  return { actors, allActors, createActor, signIn, login, closeActor, run,
    close: async () => {
      const errors = [];
      for (const actor of allActors) { try { await closeActor(actor); } catch (error) { errors.push(error); } }
      if (errors.length) throw new AggregateError(errors, 'Actor recording failed');
    } };
}
