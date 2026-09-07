#!/usr/bin/env node
// Records the "create a list" walkthrough shown by CreateListGuide.vue and
// writes web/public/guides/create-list.mp4 (H.264, 2x device scale, still
// frames held about two seconds each) plus create-list-poster.png.
//
// Requirements: the dev stack running (./scripts/dev-services, ./scripts/dev-api,
// ./scripts/dev-web), a seeded organization, an installed Google Chrome
// (playwright-core launches it via the `chrome` channel; no browser download),
// and ffmpeg on PATH. Credentials come only from the environment:
//   GUIDE_EMAIL (default alice@acme.test)  GUIDE_PASSWORD (required)
//   GUIDE_BASE_URL (default http://127.0.0.1:5173)
// The list created during the recording is deleted again at the end.
import { mkdtempSync, mkdirSync, rmSync, copyFileSync, statSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'
import { chromium } from 'playwright-core'

const baseUrl = process.env.GUIDE_BASE_URL ?? 'http://127.0.0.1:5173'
const email = process.env.GUIDE_EMAIL ?? 'alice@acme.test'
const password = process.env.GUIDE_PASSWORD
if (!password) {
  console.error('GUIDE_PASSWORD is not set')
  process.exit(2)
}
const listName = 'Zillow leads not yet contacted'
const outDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'public', 'guides')
const frames = mkdtempSync(join(tmpdir(), 'create-list-guide-'))
mkdirSync(outDir, { recursive: true })

const browser = await chromium.launch({ channel: 'chrome', headless: true })
const page = await browser.newPage({ viewport: { width: 1120, height: 700 }, deviceScaleFactor: 2 })
let frame = 0
const shoot = async (label) => {
  await page.waitForTimeout(400)
  const file = join(frames, `frame-${String(frame).padStart(2, '0')}.png`)
  await page.screenshot({ path: file })
  console.log(`frame ${frame}: ${label}`)
  frame += 1
  return file
}

let createdListId = null
let createdListRevision = 1
try {
  await page.goto(`${baseUrl}/login`)
  await page.getByRole('textbox', { name: 'Email' }).fill(email)
  await page.getByRole('textbox', { name: 'Password' }).fill(password)
  await page.getByRole('button', { name: 'Sign in' }).click()
  await page.waitForURL(/\/today/)

  await page.goto(`${baseUrl}/lists`)
  await page.getByRole('link', { name: 'Create a list' }).first().waitFor()
  const poster = await shoot('Lists page')

  await page.getByRole('link', { name: 'Create a list' }).first().click()
  await page.getByTestId('create-list-guide').waitFor()
  await shoot('People with the guide')

  // Apply a two-clause filter through the URL the FilterBar itself writes,
  // so the frame shows real chips without depending on chip internals.
  const stages = await page.evaluate(async () => {
    const response = await fetch('/api/stages', { credentials: 'same-origin' })
    return (await response.json()).stages
  })
  const lead = stages.find((s) => /lead/i.test(s.name)) ?? stages[0]
  const filter = JSON.stringify({
    version: 1,
    clauses: [
      { kind: 'stage', stage_ids: [lead.id] },
      { kind: 'last_contact', age: { op: 'never' } },
    ],
  })
  await page.goto(`${baseUrl}/people?guide=create-list&filter=${encodeURIComponent(filter)}`)
  await page.getByTestId('create-list-guide').waitFor()
  await page.getByRole('button', { name: 'Save as list' }).waitFor()
  await shoot('Filter applied')

  await page.getByRole('button', { name: 'Save as list' }).click()
  await page.getByRole('dialog').waitFor()
  await page.getByRole('dialog').getByRole('textbox', { name: /name/i }).fill(listName)
  await shoot('Save as list dialog')

  const [created] = await Promise.all([
    page.waitForResponse((r) => r.url().includes('/api/saved-lists') && r.request().method() === 'POST'),
    // The footer's primary action is the dialog's last button (Cancel precedes it).
    page.getByRole('dialog').locator('button').last().click(),
  ])
  const createdList = (await created.json()).list ?? null
  createdListId = createdList?.id ?? null
  createdListRevision = createdList?.revision ?? 1
  await page.waitForURL(/\/lists\//)
  await page.getByRole('heading', { name: listName }).waitFor({ timeout: 10_000 }).catch(() => {})
  await shoot('New list page')

  // Hold the last frame a little longer by repeating it once.
  copyFileSync(join(frames, `frame-${String(frame - 1).padStart(2, '0')}.png`), join(frames, `frame-${String(frame).padStart(2, '0')}.png`))
  frame += 1

  // Poster at CSS size (the video itself keeps the 2x frames).
  const posterOut = spawnSync('ffmpeg', ['-y', '-loglevel', 'error', '-i', poster, '-vf', 'scale=1120:-1:flags=lanczos', join(outDir, 'create-list-poster.png')], { stdio: 'inherit' })
  if (posterOut.status !== 0) throw new Error(`ffmpeg (poster) exited with ${posterOut.status}`)
  const video = join(outDir, 'create-list.mp4')
  const ffmpeg = spawnSync('ffmpeg', [
    '-y', '-loglevel', 'error',
    '-framerate', '1/2.4', '-i', join(frames, 'frame-%02d.png'),
    '-vf', 'scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p',
    '-c:v', 'libx264', '-preset', 'slow', '-crf', '20', '-r', '30', '-tune', 'stillimage',
    '-movflags', '+faststart', video,
  ], { stdio: 'inherit' })
  if (ffmpeg.status !== 0) throw new Error(`ffmpeg exited with ${ffmpeg.status}`)
  console.log(`wrote ${video} (${Math.round(statSync(video).size / 1024)} KB)`)
} finally {
  if (createdListId) {
    // The delete command needs the saved revision (SLICE_011b §"Delete").
    await page.evaluate(async ({ id, revision }) => {
      await fetch(`/api/saved-lists/${encodeURIComponent(id)}`, {
        method: 'DELETE',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ expected_revision: revision }),
      })
    }, { id: createdListId, revision: createdListRevision }).catch(() => {})
  }
  await browser.close()
  rmSync(frames, { recursive: true, force: true })
}
