# Mobile007 / 010d3 — Shared-development release

**DEPLOYED-AND-VERIFIED — 2026-09-15.** Mobile007 Find and Save People and
Slice 010d3 admitted-People history are deployed to the Mac-hosted shared
development environment after the user's explicit release request.

## Publication and runtime

- `main` is pushed at `f6557eef462c790978264299cf1861a50193edcb`.
- The API binary was built from that revision in an isolated target and is
  running on `127.0.0.1:3000`.
- API SHA-256:
  `096088bf2a884130d94d6c2b8b294429f50adda3724878b82a673701be0d657b`.
- The Web production build was regenerated from the same revision and is
  serving on `127.0.0.1:5173`.
- Migration workers use one-second idle polling intervals. Existing native test
  APIs on ports 3101 and 3106 were left unchanged.

## Database and release gate

- Migration `20261005000001_fub_admitted_history` applied successfully.
- No existing business data was reset or restored.
- Fresh operator-owned release evidence passed launch and confirmation checks,
  including `admitted_history_confirmation_ready=true` and database `crm_dev`.
- The report is held outside the repository at
  `/private/tmp/crm-mobile007-release-20260915/`.

## Smoke checks

- `GET /api/health`: 200 and `{"status":"ok"}`.
- `GET /internal/ready`: 200 and `{"status":"ready"}`.
- Unauthenticated `POST /api/mobile/v1/people/search`: 401, as required.
- Unauthenticated `GET /api/migrations/fub/admitted-history-imports`: 401, as
  required.
- Web root on port 5173: 200 with the newly generated asset manifest.
- Isolated locked Rust build, formatting check, Web production build and SQLx
  migration application passed.

Implementation acceptance and native/browser evidence remain in
[MOBILE_007_010d3_FINAL_VERIFICATION.md](MOBILE_007_010d3_FINAL_VERIFICATION.md).
Production Kubernetes deployment, live FUB/customer processing, activation and
native distribution remain separate scopes.
