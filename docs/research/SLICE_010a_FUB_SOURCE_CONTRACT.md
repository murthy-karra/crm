# Slice 010a — Public FUB source contract qualification

Checked 2026-09-10 against official public reference pages and the embedded
`oasDefinition` OpenAPI JSON on those pages. This is documentation evidence,
not a live account test. The user explicitly deferred live validation because
no synthetic FUB test account is available yet.

## Profile contract

| Endpoint | Qualified response / limitation |
|---|---|
| [`/identity`](https://docs.followupboss.com/reference/identity) | `account.id` is an integer; `account.domain` a display/domain string; `account.owner` contains name/email. `user` contains integer ID, name and email. The published schema contains **no role**. Require valid account/user IDs; do not derive ownership from matching names or emails. |
| [`/people`](https://docs.followupboss.com/reference/people-get) | Collection `people`; `_metadata.collection = "people"`; example total is a JSON integer. `limit=1`, `fields=id`, and explicit `includeTrash` are documented. The two Trash scopes overlap. |
| [`/users`](https://docs.followupboss.com/reference/users-get) | Collection `users`; metadata includes total and nullable continuation. Example user has `id`, `role`, `isOwner`, `status`. Example role is the illustrative string `Captain`; it is not a closed authorization enum. `limit=1` does not guarantee the connected user is returned. |
| [`/stages`](https://docs.followupboss.com/reference/stages-get) | Collection `stages`; metadata includes collection/offset/limit/total. No source-to-destination stage mapping is implied. |
| [`/customFields`](https://docs.followupboss.com/reference/customfields-get) | Path has capital F, but response collection and metadata collection are lowercase **`customfields`**. The example total is the string **`"4"`**. Accept strictly validated nonnegative integral totals encoded as either JSON numbers or decimal strings. |

The users/people documentation examples contain literal ellipses and are not
valid JSON documents as published. Implementation fixtures must be minimal
synthetic examples with these documented keys, not copied ellipses or invented
claims of captured live responses. Missing, malformed, fractional, negative or
inconsistent totals remain unknown. `/identity` alone yields unknown role.
The requested first page may identify its own user; only explicit evidence for
that same user can refine the access scope, and scope never proves full coverage.

## Authentication, pacing and versions

[Authentication](https://docs.followupboss.com/reference/authentication) specifies
HTTPS Basic authentication with the API key as username and an empty password;
source permissions follow the key's user. Expired-account lockdown can cause
403 even if the key still authenticates.

[Identification](https://docs.followupboss.com/reference/identification) specifies
registered `X-System` and `X-System-Key` request headers. They are deployment
configuration, not browser input; the system key is secret. No registration or
provider permission was obtained in this task.

[Rate limiting](https://docs.followupboss.com/reference/rate-limiting) documents
`X-RateLimit-Limit`, `X-RateLimit-Remaining`, `X-RateLimit-Window` (seconds), and
`X-RateLimit-Context`. `Retry-After` gives seconds before another request; honor
429 even when Remaining is positive. Limits can change, so do not encode the
published example allowance as the application's budget.

Record request API path version `v1`, assessment profile version and capture
time. OpenAPI document `info.version = "1.0"` describes the public schema;
it is not proof of a live server software version. Preserve any selected safe
response version metadata if actually present; otherwise it is unknown.

## Local evidence identifiers

The extracted public OpenAPI JSON files are in
`/private/tmp/crm-010a-source/`. SHA-256 of the exact formatted local files:

| File | SHA-256 |
|---|---|
| identity-oas.json | `9071c5aae7a662180a9ab26de96075b4b7ecfc98a1f8240f0ca92942aa257b68` |
| people-get-oas.json | `687d37cc80114fcd5b1acfbca10140d82007e3becfa7e594d202bbc5d1bba736` |
| users-get-oas.json | `8c8b9b5beb47aa01098b665b25467cce7e789f4380923bbf9d05258adba006f4` |
| stages-get-oas.json | `5ff0e5564868873519f6db1383740b55c82221bfeccd62428ad79488b94fb34a` |
| customfields-get-oas.json | `89de68c170804c2e323d3a994ae1d320394a80c9bfa7c42109e6f22b9e3a8291` |

These hashes identify this research capture, not a vendor-supported immutable
schema release. Refer to the linked public pages when refreshing the contract.
