# Slice 011c — live browser evidence

These screenshots record the coordinator's real-browser walkthrough of the
uncommitted Slice 011c implementation on `codex/slice-011c-today-sources`,
based on `9d62e86`. They are intermediate checkpoints, not proof that all
acceptance criteria or final-tree gates passed. See the
[verification record](../../../tasks/SLICE_011c_VERIFICATION.md) for results,
outstanding work and the QA setup incident.

The browser used `http://crm-011c.localhost:51011/`, a dedicated Vite/API runtime
with generated synthetic test data and accounts. This hostname separated its
cookies from the existing development and deployed applications. No external
outreach or Operator request was sent in this walkthrough. Manual contact
records were synthetic facts entered through the normal application dialog.

| Artifact | Observed checkpoint |
|---|---|
| [Today after contact](today-after-contact.png) | Available work after logging contact: current list reasons can retain a Person after a built-in or activity-based reason clears. |
| [Narrow source manager before correction](sources-narrow.png) | I14 reproduction: header controls squeezed the update text. Retained as the original observation. |
| [Narrow source manager after correction](sources-narrow-fixed.png) | I14 correction: Today header controls stack below the title at narrow width. Requested viewport 390px; document content 375px, with no document-level horizontal overflow. |
| [Partial availability](today-partial.png) | An intentionally unsupported synthetic source produces a named notice while working sources and built-in work remain available. Its Remove control is retained. |
| [Session replacement observations](session-replacement-observations.txt) | Live two-tab Alice-to-Carol and Carol-to-Carol relogin checks clear the previous unsent Operator draft and identity. After navigation, the receiving tab shows the current actor and an empty drawer. |
| [Current normal Today](today-final-normal.jpg) | After the pending-attempt session implementation was reloaded and checked, Alice's Today shows three built-in People plus a fourth Person retained by the email list, with list reasons also attached to overlapping built-ins. |
| [Session reset while blocked](session-reset-blocked.png) | Coordinator takeover check, 2026-09-06 19:39 local, same synthetic origin, Alice signed in. A foreign never-settled auth-attempt record was written into browser storage to simulate a tab that closed mid-login. Reloading Today rendered the fail-closed copy ("A sign-in or sign-out started in another tab has not finished. If that tab is closed or stuck, reset to continue.") with only the Reset and continue control and no private navigation. Clicking it removed the record, published a settled marker, re-verified the session and restored Alice's four-item Today; only the settled marker remained in storage. Repeated once for the screenshot, then logged out. |

The unsupported definition was restored exactly by the sole database lane
after removal through the UI. The coordinator then re-enabled the valid list
and verified the notice cleared and matching reasons returned. The viewport
override was reset after the narrow-layout check.

[ARTIFACT_SHA256.txt](ARTIFACT_SHA256.txt) records the six unmodified images
and the session-observation text. Credentials, database URLs, private runtime handoffs and browser cookies
are excluded. Authenticated HTTP load measurements belong to a separate
performance archive; these screenshots do not establish latency or capacity.

The [retained check outputs](checks/README.md) separately preserve intermediate
automated test failures and reruns with provenance hashes.
