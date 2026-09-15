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

## In progress / failed evidence

- The first real M006 UI run reached a qualified person, tag picker, text and number custom-field controls, but its XCTest selector treated SwiftUI's date control as `XCUIElementTypeDatePicker`; iOS 26.5 exposes it differently. The test stopped before submitting metadata. Log: `ios006-ui-metadata.log`.
- The selector was replaced with stable typed-field section labels. The corrected UI journey, private Mobile005 populated-store run, in-place current-app install, and post-upgrade metadata proof remain to run in the next serialized API slot. No claim of success is made for those pending checks.
