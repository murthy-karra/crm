# Slice 019b release record

**DEPLOYED-AND-VERIFIED — shared development runtime, 2026-09-10.**
The user explicitly authorized “commit, deploy, merge with main and delete
loose branches.” The existing target is the Mac-hosted API and production
Web bundle exposed through the development Cloudflare tunnel at
[app.tarams.org](https://app.tarams.org); this is not a production-cluster rollout.

## Revision and source evidence

- Implementation: `a642987a307d995490af95381e30d80bb8f8ba8d`.
- Merge into local main: `237639d2c1b5554f9ad872f7c35100f76e80ee77`.
- Base: `6bad52a32376001d4082ac21aad29699462e3ceb`.
- Code-tree SHA-256 unchanged from the fully verified implementation:
  `a75ee70b9b3b51021ca00853109e71ff8c222ca46ad1f91c0937976fabf88e0a`.
- Final gates and the single benchmark are retained in the
  [implementation verification](SLICE_019b_VERIFICATION.md). The merge had
  no conflicts or source differences, so those checks remain applicable.
  No benchmark or complete test suite was repeated for the merge.

## Deployment procedure and checks

Read the existing README, `scripts/dev-api`, `scripts/dev-web-prod` and
`docs/prompts/07-deploy.md`. Retained the previous API binary and Web bundle
privately before replacement. Built from the merged main checkout:

```sh
cargo build --manifest-path backend/Cargo.toml -p crm-api --bin crm-api --locked
# From web; output staged privately before replacing the running bundle:
pnpm run build --outDir "$crm_release_dir/web-dist-new"
```

Both builds passed. Web reported its existing large LiveKit chunk advisory.
No migration was introduced or applied; no dependency or service configuration
changed. Existing root `.env` supplies the development configuration; secret
values were not recorded. Runtime data was preserved.

Revalidated the exact old listeners and their working directories, then sent
SIGTERM to Web PID 71217 and API PID 70740. Copied the staged bundle into
`web/dist`, started `./scripts/dev-api`, waited for readiness, and started
`pnpm exec vite preview` from `web`. This performs the documented
`dev-web-prod` build/preview steps separately so both builds finish before
replacing the runtime. No broad process kill or container reset was used.

- API PID **67757**, cwd `/Users/karrad/projects/crm`, port **3000**.
- Web preview PID **68235** (launcher 67769), cwd
  `/Users/karrad/projects/crm/web`, port **5173**.
- Loopback health/readiness: **200**, `ok` / `ready`, request ID present.
- Public app and API health: **200**; realtime upgrade: **101** through
  `./scripts/check-tunnel`.
- Both loopback and public HTML matched the built index byte for byte;
  six public assets, including People/Today rules chunks, matched their builds.
- Authenticated public smoke with the existing seeded administrator:
  login, identity, People, custom definitions, saved lists and Today all **200**.
  Each of the four new custom kinds accepted its wire shape and returned
  the expected **422 invalid_field** for a nonexistent field, confirming the
  new backend is serving. The temporary smoke session was revoked (**204**).
- This development Organization has no active custom definitions; valid
  custom membership is covered by the completed isolated browser walkthrough
  and DB gates. No fixture data was added to the shared runtime for this release.
- A separate short post-start observation confirmed the expected executable
  paths, healthy responses, and zero WARN/ERROR signals in the new API/Web logs.

The initial Python smoke helper could not use the public path: the system
Python TLS handshake failed, and newer Python's urllib client received an
edge 403. The supported native curl client passed, including authenticated
checks; TLS verification and edge configuration were unchanged.

[Build logs, smoke outcomes, artifact hashes and tunnel checks](../design/qa/slice-019b-2026-09-10/release/README.md)
are retained. No runtime credentials, session cookies or Person content appear
in those artifacts. This was a bounded release observation; no recurring
monitor was created.

## Cleanup and recovery

Deleted the fully merged `codex/slice-019b-custom-field-filters` branch using
`git branch -d` and removed `/Users/karrad/projects/crm-worktrees/019b` after
verifying no tracked or untracked changes remained. Its ignored configuration
was preserved privately; generated build/dependency directories were removed
with the worktree. Only local main remains. Remote inspection found only
`origin/main`, so no remote loose branch needed deletion. The unrelated
`notes.txt` in main was left untouched. The isolated QA database is retained.

Rollback artifacts are retained at `/private/tmp/crm-019b-release-emyyxbhd`:
`crm-api-before`, `web-dist-before`, and private process/build records. Recovery
would stop only these release listeners, restore the previous Web bundle and
launch the saved binary from the main root with the existing environment.
No rollback was needed. API and Web must move together; after users save new
custom-filter definitions, an older API will fail closed on them. Prefer a
forward correction rather than deleting or rewriting user criteria.

Local commit/merge/deployment/branch cleanup are complete. No push was requested
or performed: `origin/main` remains at `6bad52a`; main also contains the final
release-record commit. A future source push does not require another runtime
restart unless application code changes.
