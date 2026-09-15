# Mobile006 iOS verification

## Scope and source

- iOS source: `9185cb3` plus this lane's changes on `codex/mobile-006-ios`.
- Frozen API source: `82d081e`, `http://127.0.0.1:3106`.
- Simulator: `CRM-Mobile006-QA`, `32978562-0A51-4E84-B55A-179BC5B28738`, iOS 26.5.
- Derived data: `/private/tmp/crm-mobile006-010f4-thyhauvv/ios006`.
- Package cache: `/private/tmp/crm-mobile006-010f4-thyhauvv/ios-spm`.

## Completed checks

- `FieldCRMMobile006QA` metadata store test passed: schema-8 to schema-9 encrypted migration preserves legacy immutable bytes; metadata envelope is atomic; invalid date/choice values are rejected; an accepted overlay remains until a later sealed metadata-qualified generation covers it, then retires. Log: `ios006-metadata-covering.log`.
- Full `StorageTests` passed: 32 tests, 0 failures. Log: `ios006-storage-full.log`.
- Full `ModelTests` passed: 16 tests, 0 failures. Log: `ios006-model-full.log`.
- Frozen live API test passed: lost metadata acknowledgement, byte-identical replay, current metadata read, and second-actor stale-revision conflict. Log: `ios006-live-metadata.log`.
- `FieldCRMMobile006UpgradeQA` build-for-testing passed with `CODE_SIGN_IDENTITY=-`. Log: `ios006-upgrade-build-for-testing.log`.
- A private QA-only Mobile005 working copy was created from the immutable archive. It is configured only to route its QA build to port 3106 and its archive hash remains separate from the modified copy. Its build-for-testing passed. Log: `ios005-upgrade-build-for-testing.log`.
- Real Mobile006 UI journey passed: offline tag/text/number proposal, process termination and relaunch, then successful sync. Log: `ios006-ui-metadata-final.log`.
- Real in-place upgrade passed. The private Mobile005 app populated and retained an installed encrypted store with an accepted receipt plus queued note, task and profile work; its UI journey passed in `ios005-installed-populate.log`. Before the current-app install, the SQLite/WAL/SHM hashes were recorded in `ios005-before-upgrade-hashes.txt`; CoreSimulator retained the same three hashes after replacing the executable (`ios006-before-launch-hashes.txt`). The current Mobile006 app then opened schema 9 with 100 people, 4 operations, 4 receipts and 1 draft, downloaded the metadata catalog, and saved a new post-upgrade metadata proposal. Log: `ios006-installed-upgrade-pass.log`.

## In progress / failed evidence

- Earlier UI selector failures are retained in `ios006-ui-metadata.log`, `ios006-ui-metadata-rerun.log`, and `ios006-installed-upgrade*.log`. They occurred before metadata submission; the final replacement runs above passed.
