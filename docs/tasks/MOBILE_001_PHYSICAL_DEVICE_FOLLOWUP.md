# Mobile 001 — Deferred physical-device follow-up

**USER-DEFERRED — 2026-09-12.** After trying the iOS simulator, the user reported
that the app and sync seem to work, deferred mobile design/system-information
cleanup, and requested continued development. Asked which phones could be made
available, the user selected **"Leave physical-phone testing for later"**.
Do not make phone availability a dependency for the next migration slice.

## Current evidence and prerequisites

The [iOS verification](MOBILE_001_IOS_VERIFICATION.md),
[Android verification](MOBILE_001_ANDROID_VERIFICATION.md) and
[milestone release](MOBILE_001_010e1_RELEASE.md) remain the completed evidence.
The user's successful simulator walkthrough is useful feedback, not physical
device or cellular test evidence.

Read-only discovery during this follow-up found no known physical iOS device
through Xcode's device tooling and only the existing Android emulator through
ADB. No physical phone was installed, paired, reconfigured or tested.

There are also concrete application prerequisites:

- [iOS sign-in](../../ios/FieldCRM/FieldModel.swift) currently runs only in
  Debug + Simulator, using loopback API3101. Device and Release sign-in fail
  closed until a trusted HTTPS environment is configured. Existing passcode-bound
  storage policy must remain in effect; the synthetic Simulator key namespace
  cannot be enabled on phones.
- [Android builds](../../android/build.gradle) use the emulator address
  `http://10.0.2.2:3101` in Debug and an empty Release API origin. A phone needs
  an explicitly configured reachable API origin and appropriate local signing.
- Existing API3101 is loopback-only. Real cellular testing needs a reachable
  synthetic HTTPS environment. Configuring that environment, device signing and
  local installation should be made concrete when this work resumes. Do not
  weaken TLS or point devices at customer data to make a test pass.

These are configuration/workflow prerequisites, not evidence that on-device
storage or synchronization failed. No new origin, certificate, distribution
channel, signing identity or phone-support policy is selected by this record.

## Bounded sequence when resumed

1. Select test iPhone/Android phone, configure trusted synthetic HTTPS access
   and prepare platform development builds with production device protections.
   Record build/source, device/OS and environment without publishing credentials
   or unnecessary device identifiers. Preserve installed pending work on upgrades.
2. Download the existing representative bounded synthetic book. On each phone,
   commit a note, task and dependent completion offline; verify saved indicators,
   background/terminate/relaunch, screen lock and ordinary reboot. Reauthorize
   online after reboot as required, then verify exact queue/content preservation.
3. Exercise actual Wi-Fi loss, cellular connection changes and a complete outage.
   Separately label controlled latency/loss tests; a network simulator is not
   evidence of real cellular behavior. Foreground recovery must converge without
   claiming guaranteed background execution.
4. Drop a response after actual server acceptance, retry with the original
   operation identity, and reconcile one business effect. Interrupt a download
   and confirm the previous complete cache and pending work survive. Check a
   task conflict without silently replacing the saved proposal.
5. Verify device passcode/key-access enforcement and same-account protected-work
   recovery. Do not remove a personal phone's passcode or erase its app storage
   as an informal test. Scope any destructive key-loss case to a disposable,
   explicitly designated test device.
6. Record observed results independently for each platform. Repeat only failed
   or affected cases after fixes; reuse prior backend/native synthetic evidence
   instead of rerunning all repository suites. This does not constitute customer
   readiness, App Store/Play distribution or a new runtime deployment.

## Separate design follow-up

The user found system information that may be unnecessary in the mobile app.
Later design work should decide which diagnostics belong behind a details/support
view and make the primary field workflow clearer. Keep essential saved-work,
sync-failure and access-lock feedback understandable. No UI change is made or
design direction approved in this checkpoint.
