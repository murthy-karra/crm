# Slice 010f2 shared-development release evidence

Source `fc5a8757bcb2591a43976544e282722b3616039f`, implementation
`9f457bc8db8707fa4ce361aac485fd6bf928bf34`, is deployed to the existing Mac-hosted
API and production Web bundle. [The release record](../../../tasks/SLICE_010f2_RELEASE.md)
describes executed checks, cleanup, recovery and remaining scope.

- [Build hashes](build-sha256.json), [Web hashes](web-build-sha256.json),
  [backup result](backup-result.json) and [migration result](migration-result.json).
- [Observed workload inventory](process-inventory.json), [known builds](known-builds.json),
  [candidates](candidates.json), [launch preflight](preflight-launch.json) and
  [confirmation preflight](preflight-confirm.json). Confirmation passed ordinary,
  metadata and activity readiness; the report expires at 2026-09-12 05:18:49 UTC.
- [HTTP smoke](smoke-results.json): 50 actual passing health/auth/asset checks.
  [The initial attempt](http-attempt-1/smoke-results.json) preserves the excessive
  metadata no-store assertion; no product fix was needed.
- [Qualified browser result](browser-release-audit.json): all eight functional
  checks completed, with ten screenshots and revoked sessions. The
  [raw result](browser-attempt-1/browser-results.json) retains its final assertion
  failure on four deliberately blocked Cloudflare Insights GETs. Exact telemetry
  qualification was offline; no second browser run is claimed.
- [Visual inspection](visual-inspection.json), including
  [desktop activity](activity-import-empty-desktop.png) and
  [390px activity](activity-import-empty-390px.png). These are responsive Web;
  no native mobile work occurred. Member denial has HTTP/URL/DOM evidence only.
- [Final database counts/workspaces](database-after-browser.json),
  [bounded observation](observation.json), [clock qualification](observation-timing-qualification.json)
  and [tunnel check](tunnel.log): stable artifacts, unchanged business/workspace
  state, empty migration/admission tables and public 200/200/101 routing.
- [Integration](git-integration.json), [cleanup](implementation-cleanup.json),
  [retired executables](retired-artifacts.json) and [evidence inventory](evidence-inventory.json).
  The inventory records post-copy hashes and a known-private-value scan.
- [Independent release audit](release-audit.json): live identities, source/build
  hashes, retained recovery artifacts and recorded release evidence verified.

Private backup/configuration/binaries are outside Git. The backup catalog was
checked, not restored. The evidence uses existing synthetic development accounts
and creates no FUB connection, import, customer-data processing or activation.
Earlier [implementation proof](../slice-010f2-2026-09-11/README.md) remains separate.
