# Mobile 001 / 010e1 — Implementation coordination

**In progress — 2026-09-12.** D-074 and D-075 authorize implementation and isolated
synthetic checks. Neither slice is complete. Local integration branch:
`codex/mobile-migration-integration`; approved planning checkpoint `9eaeb0a`.
Main and the recorded shared-development release have not been changed.

| Lane | Worktree / branch | Ownership / test resources |
|---|---|---|
| Mobile backend | `/Users/karrad/projects/crm-worktrees/mobile-001-backend`, `codex/mobile-001-backend` | Mobile commands/routes/revisions and tests; schema versions 20260923…; isolated `crm_mobile_001`, loopback API3101 |
| Migration backend | `/Users/karrad/projects/crm-worktrees/migration-010e1`, `codex/migration-010e1` | Retained core comparison/report schema/routes/tests; schema versions 20260924…; isolated `crm_migration_010e1`, API3102/Web5202 |
| Migration Web subtask | Same migration worktree | Primary migration writer relinquished only `web/src/api/coreChangeReports.ts`, dedicated report components/tests to a second writer; backend/shared files remain excluded |
| Coordinator | Existing checkout | Shared module/router/worker/configuration/receipt-key and preflight wiring; shared Web navigation; integration and required gate serialization |

Both backend lanes freeze exact DTO/locking/schema details in their assigned
`MOBILE_001_CONTRACT.md` / `SLICE_010e1_CONTRACT.md`. Native implementation starts
from a locally integrated, reviewed foundation after its focused DB/API proof.
The broad regression gate continues concurrently and remains required for final
acceptance. Android may occupy the third worktree while the backend writer finishes
performance evidence; that backend worktree closes before iOS opens. This keeps
the three-worktree maximum and avoids leaving a native writer idle.

Each lane has private gitignored test configuration. Only local database and
Centrifugo test access plus synthetic session/raw/receipt keys were copied; no
FUB, AI or telephony credentials were supplied. Build artifacts were APFS-cloned
into separate target directories. Tests use distinct SQLx databases and bounded
parallelism. Shared `sqlx-prepare` / `check-db` names remain coordinator-serialized.
No shared service reset, production/customer processing, push or deployment.

Initial completed coordinator check: migration release-preflight tests **34 pass**,
including retained-report capability/schema/engine checks and read-only inventory.
The migration Web gate subsequently passed 1,181 tests, lint, type checking and
build. Mobile's eight focused database tests now pass, including the full HTTP
deadline and migration review compatibility correction. Full Rust/DB regression
gates and native verification remain in progress; this is not completion.

Native dependency research selected exact compatibility candidates for validation:
SQLCipher.swift/SQLCipher Android 4.19.0; Android AGP9.3.1, Gradle9.5.0, Kotlin and
Compose compiler2.4.20, Compose BOM2026.09.00, Room2.8.5, KSP2.3.12, SQLite2.7.1.
These are not app-build evidence. Existing JBR25 and installed API37 are expected
compatible; actual resolution/build/runtime checks belong to the native writers.
