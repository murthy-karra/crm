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
- Review-remediation `FieldCRMMobile006QA` `ModelTests` passed: 20 tests, 0 failures. It verifies matching sealed-catalog replacement labels, catalog mismatch refusal, storage-write rollback before predecessor supersession, and a deleted action requiring explicit exclusion before the valid replacement atomically saves. Log: `ios006-review-fix-m006-modeltests.log`.
- The final `FieldCRMMobile006QA` `StorageTests` rerun passed: 32 tests, 0 failures, including encrypted schema upgrade, atomic metadata envelope and covering-seal retirement. Log: `ios006-review-fix-storagetests.log`.
- Real Mobile006 conflict UI passed on the owned simulator. After an authorized terminal sealed download, the QA-only local conflict fixture selected an editable Person and opened Saved work. The installed UI displayed named baseline, proposed and current metadata values, saved the revised proposal, reconstructed controls, and exposed its normal `Save metadata proposal on device` action. The test retains `mobile006-metadata-three-way-values` and `mobile006-revised-metadata-proposal` screenshots in its xcresult. Log: `ios006-ui-metadata-conflict-round6.log`; 1 test, 0 failures in 33.035 seconds.

## In progress / failed evidence

- Earlier UI selector failures are retained in `ios006-ui-metadata.log`, `ios006-ui-metadata-rerun.log`, and `ios006-installed-upgrade*.log`. They occurred before metadata submission; the final replacement runs above passed.
- The conflict-QA iteration logs `ios006-ui-metadata-conflict-round1.log` through `round5.log` are retained. They show initial control timing, an unqualified first cached Person, a compile correction, and an off-screen save selector before the final round6 pass; no persisted server metadata mutation occurred in those failed runs.

## Final review recovery fix

Root integrated an explicit generation argument for metadata qualification: active
editing reads only the sealed generation, while reconciliation/promotion check the
staging generation. A shared same-Person-revision marker cannot revoke a sealed
baseline. `logs/integration/ios-sealed-metadata-storage-r2.log` passed 33/33 storage
tests, including interrupted refresh, same-revision staging, reopen and subsequent
valid seal (4.873s suite; new regression 0.218s). The comparison screen also omits
internal token counters; names and actual values remain unchanged.
