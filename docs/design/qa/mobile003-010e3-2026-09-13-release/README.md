# Mobile 003 / 010e3 release evidence — 2026-09-13

The [release record](../../../tasks/MOBILE_003_010e3_RELEASE.md) owns scope and
interpretation. Runtime source: `b37a480c5bc4cd5c711dceba287cc5bed70a71d0`.

- `release-summary.json`: executed build, backup, preservation, HTTP/mobile,
  browser, compatibility, configuration and cleanup results.
- `build-sha256.json`, `web-build-sha256.json`: verified runtime and served assets.
- `migration-checksums.json`: all 45 applied schema versions and SHA384 hashes.
- `release-source.json`: all 1,100 source-file SHA256 hashes used for the build.
- `browser-desktop.png`, `browser-390.png`: inspected empty-workspace admission UI.

No credentials, cookies, customer payloads or database backup are included.
Protected raw evidence and recovery artifacts remain in the private release
directory referenced by the release record. Backup catalog verification is not a
restore test. Populated admission and native contact writes remain attributed to
implementation acceptance; this release performed no source or business writes.
