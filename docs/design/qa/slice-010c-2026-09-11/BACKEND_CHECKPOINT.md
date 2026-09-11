# Slice 010c backend checkpoint — before review round 1

2026-09-11. Worktree `codex/slice-010c-people-import`, baseline `c6c5930`.
This freezes the backend boundary for the first bounded implementation review.
It is not a final-gate, browser, deployment or customer-data readiness claim.

## Implemented boundary

- Additive workspace mode, durable import binding and Operator admission, with
  workspace-first shared/exclusive transaction guards. Current admin review reads
  and ordinary operational writes are distinct; no activation/reset API exists.
- Direct domain, HTTP, intake/extraction, Operator and call paths enforce the
  same hold. Existing terminal call cleanup remains allowed. API/CLI startup and
  first confirmation enforce server/operator-owned release compatibility evidence.
- Retained raw-qualified People/stage/user preparation, immutable bounded plan
  revisions, explicit mapping choices, scoped exact field segments, staged
  execution, separate source identities, per-Person atomic results/provenance and
  ID-only Migration facts. No source requests, Inquiry creation or notifications.
- Import-owned exact byte accounting against the shared source ledger, current
  deployment ceiling clamps, lease fencing, cancellation capacity retained from
  proposal and explicit retry after restored capacity.
- Nullable import contact ordering across all shared primary readers, with
  ordinary ordering preserved. HTTP/session contract is exported in
  `docs/specs/SLICE_010c_CONTRACT.md`.

## Focused verification

All Rust commands used the isolated worktree and `SQLX_OFFLINE=true`.
DB tests additionally sourced its private provider-disabled configuration and
set `DATABASE_URL="$MIGRATION_DATABASE_URL"` targeting the disposable
`crm_slice010c_test_master` harness. Each SQLx test applied the current migration
in a fresh isolated test database. Shared `crm_dev` and the main runtime were
not used. No live FUB/provider call was made.

| Command (Cargo manifest: backend/Cargo.toml) | Result | Retained log |
|---|---|---|
| `cargo test -p crm-app import_ --lib --locked` | 17 passed; 0 failed; 0.57s test / 14.69s build | `/private/tmp/crm-010c-import-unit-final.log` |
| `cargo test -p crm-api --test all --features test-support --locked db_import_ -- --ignored --nocapture --test-threads=1` | 23 passed; 0 failed; 95.11s test / 18.88s build | `/private/tmp/crm-010c-import-gate-3.log` |
| `cargo test -p crm-api --test all --features test-support --locked db_workspace_ -- --ignored --nocapture --test-threads=1` | 9 passed; 0 failed; 23.89s test / 22.14s build | `/private/tmp/crm-010c-workspace-background-2.log` |
| `cargo clippy -p crm-api --all-targets --features test-support --locked -- -D warnings` | passed; 31.58s | `/private/tmp/crm-010c-scoped-clippy-3.log` |
| `cargo fmt --all --check` | passed | `/private/tmp/crm-010c-backend-fmt.log` |
| `python3 -B -m unittest discover -s scripts/tests -p test_migration_release_preflight.py` | 14 passed; 0 failed; mocked database only | `/private/tmp/crm-010c-preflight-tests.log` |
| `bash -n scripts/check`; `git diff --check` | passed | command output was empty |

The 23 import tests comprise nine actual gate/concurrency/accounting cases,
five HTTP cases, eight raw-source integration cases and one direct-reader case.
The nine workspace tests comprise five HTTP/domain matrices and four actual
Operator/inbound/call background cases. These include a real advisory waiter,
current-role revocation, actor adoption, exact retained-column accounting,
>2 MiB per-item admission and provenance, primary-before-dedup behavior and
separate People with shared contacts. No test-only import permission is supplied
by a tenant: release readiness is injected only through `test-support`.

The reader test retained three actual `EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON)`
records for list, by-ID and task-only statements in the import gate log, marked
`CRM_010C_READER_PLAN`. Its final fixture contains 25,002 People and 100,012
contacts, including dense shared values. It requires indexed contact relation
nodes with at most three returned rows per Person/kind invocation. List/search/
by-ID/detail and zero-Inquiry task-only ordering assertions passed.

Earlier targeted attempts are retained separately, not represented as passes:
`crm-010c-import-gate-2.log` was 20/23 (three invalid test boundary assumptions,
corrected by preserving stored-limit floors and reaching actual admission);
`crm-010c-workspace-background.log` was 7/9 (intake rotation HTTP error mapping
corrected; call fixture aligned to existing `failed{cancelled}` contract).
The final lint pass follows six test-only style fixes. Formatting and explicit
Rust auto-deref/expect_err changes after DB execution do not change behavior.

## Remaining coordinated work

First bounded backend review/fixes, then Web implementation and second bounded
review. Final sequential SQLx metadata preparation/full service-free and DB gates,
comprehensive hot-statement EXPLAIN inventory, the single paired contact-reader
regression, release-preflight integration checks and production-build synthetic
browser walkthrough remain pending. The new `db_import_plans.rs` is still a
support-owned Step 6 fixture, is not registered or executed at this checkpoint,
and is explicitly excluded from the frozen manifest below. No known test
failure remains in the checkpoint's registered focused suites.

The dedicated macro master was migrated before later additive migration edits;
its recorded checksum/schema is not the final migration authority. Final SQLx
preparation must use a fresh current isolated schema. Existing generated metadata
is included in this hash freeze for reproducibility, not certified as final.
Live authorized FUB validation remains deferred by the user. No commit, push,
merge, deployment, activation or main-worktree/runtime mutation was performed.

## Retained log integrity

- `6a7cf20db88cf58500a11cb3f6b4f8e4eeba8d2ef6db75fda49d2885bc88e73e` — `/private/tmp/crm-010c-import-unit-final.log`
- `3c7c0adba716bf44942a76b94ea6ef1002b3d26c954f16878c82b58afe3649be` — `/private/tmp/crm-010c-import-gate-3.log`
- `dbb412537a1e762bd9ff215e9f8c3e5a8ea46eacfe478282487df54cca7d6327` — `/private/tmp/crm-010c-workspace-background-2.log`
- `f196bef0973ff999dcbcf679ca035393cbb4be84e582dd7f9d09005e1f656ac4` — `/private/tmp/crm-010c-preflight-tests.log`
- `eb06a38e4ab14a80cd84cda5a868759882af4febe0f33a59608edd4aa81d76fe` — `/private/tmp/crm-010c-scoped-clippy-3.log`
- `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` — `/private/tmp/crm-010c-backend-fmt.log`

## Frozen backend source manifest

147 changed backend/config/script/contract files; paths are repository-relative.

```text
7ff5e7fa664b4cd1c7f082770e95c9c406e3b42784b2335cfc145cb9f870ee94  .env.example
5ac95cc4b7af23714b5561bc28cee08dbd6cc6c1a859456e6a51900208b9a14a  backend/.sqlx/query-0c5a84f414ccc57edf4e55d7314c807247bd6b18d99a1a3c2db27e0ba91f6bc3.json
5269f6c830fe14ccfc9c3ce3d75ad6e193ef23284f4eb572047a9a7795f13414  backend/.sqlx/query-24c3432391a34488bd1fa020c9757cec0dafcc95f4bbce953dbbaf480344c6dd.json
2a227c2f74b95ac848bed8d9636238e0903f93b2d003cb503fc6f8de73b953b0  backend/.sqlx/query-30986a899b1fb0a32f4d92e454c0b1abf9596ce02916bb14e5d932544a0750bc.json
ad693c2775e75b944982a3b397d7d68deeb6d9f58e165701caf3f756e2890fd6  backend/.sqlx/query-35100d9f49886a0439e7b9e01fdff4274bc60d39a2a0e8308e29cf75a003fd7c.json
a31e1667d98abab878911618882bf2f194802f97c827934bac53a93b3de5dbe3  backend/.sqlx/query-3c2dfc76f3b5667cf2620f51d158d0b59135a4266462db8f209e832707a0711c.json
020d2d8ced75bdfc4e6cfdd08e1c3d620bd784f576dc69fa421147da8900be93  backend/.sqlx/query-44952452ca800343ba34474294ddc92ffa656ab7fb9d637b3b902b4ff7415c60.json
292cfef8b910f4595daf7ae4eb5657d4df8037104e0296c7bfed3f75355ea588  backend/.sqlx/query-59c283e05ace9f184e4fe2740592fa843bc81b224b6f5b419aae0830b79a9e57.json
18a62eeb76eeb7ea32d4aff8dac565296dab060815ab84033c163e4d6fd398e7  backend/.sqlx/query-63f37a19805c6ec956c1da4e2242d72cb55a5f14469bce324dd4ba9e1378e18d.json
d2702936a61a6573d954878e0ebf36d802bbaee30dcb444b85f4b9d22193e9d6  backend/.sqlx/query-66608de6ee2ee7b6513542c530b40070469100b1da34426b2c6125d1ff6a1c00.json
6ce8314a51bcfdf32b063bf1752c967ecbe088f6a991ecb0c78b092a0b52459e  backend/.sqlx/query-7e5ed3c4ad8ccab94c9da152c16bfc61a73d05e556946a886b7bf348df6bf66e.json
52d806af5d79c812f3587532a8f5ec6a72d25e3f72a3e0a3399b864ecac26961  backend/.sqlx/query-851fa5bd39633130d7bb835716850f25746eba1c4c9ce731858485702638b5d8.json
b3711b8ea931b019e04a0a069b39df4a67a21b0f32940d312534e265aaf2d149  backend/.sqlx/query-854d6a0d23cc7d0baaba7114592f4605c908ffe5bc961c9676188fe12b3dbe6e.json
d343384ee03a3b19309c94308784e2acb813ca3a30050f77a6ffe9e6b57bcd56  backend/.sqlx/query-86f2df85e71a35a74ef2cb0347f3d5e6cb97e7b2fdf0cc171fe71bc71b635859.json
f56a12b7cb1b70921e6f7c867e0076f62452c7aecd46e17c9c8625e90796e520  backend/.sqlx/query-8bf2b13ed011caa12c840a015a82369bd7958f231dc3eb84d0669c135673095c.json
8ac621567b66a7756b6151a5e40320f9182388ada781fbfe23df31d2d840b7d9  backend/.sqlx/query-d9a37b516ab11598778d0b94a281095eb718497fec218337f7fca540a42f70e9.json
fc5c7f38141af895f9762018defb2484b3200f259597c2b2a41018eac99768f7  backend/.sqlx/query-e70277cec0bedbbdf5382c1288f7d60983e13f9819763b31f1f5c056933ae852.json
e67d04b7b804490c0c7085cea0d660428ef2422d99081891d7c91165454efaf3  backend/.sqlx/query-ebb6e7743e5e969b34fef8b852a25980cdec047ba8412435b312e01f7708719d.json
403713f38fa138449a7bf6b51e8a5ed3757a5202a71be1a25f880041af7cb4cb  backend/.sqlx/query-ffb5d47d81a8f8e4a63bb380f5d33587f233e3fd63d823c4dce643933d212c9b.json
841f08c07111bd7dc2e356ebb1f9e012ef869cf13057929ac2fec67be8ffc081  backend/crates/crm-api/Cargo.toml
0c5d85ece14bbcf24bed0223624c2012c368893aae9276fba5dfdeb20f2b1536  backend/crates/crm-api/examples/import_qa.rs
716fc5e4adf32886ddbb9e59fcc6bbc308b8d271ba220225c8643eccac6a779b  backend/crates/crm-api/migrations/20260918000001_fub_people_import.sql
7c2c6783a6fe8057bab3d361ff3b501a5b0caef060ecab3bd01298010b268a65  backend/crates/crm-api/src/auth/mod.rs
148acf11813ca87c2bcab6558b58e904a18634f4dc735940170dc563fa8564d7  backend/crates/crm-api/src/auth/session.rs
868ffa5a7c1bb483842fab777cf80deb8582594561514734aa147a1a3810dc69  backend/crates/crm-api/src/auth/workspace_http.rs
4ce9c40e1d08ad4646685c527f8e8364d537e030c6089e365ab1d19cbf3e1527  backend/crates/crm-api/src/bin/crm-admin.rs
e95b59321927ce8417daf9cbb13dbf6f2d40b0a747ab6024ce485acc0dc362e3  backend/crates/crm-api/src/error.rs
11b1acba07e0f6135a48b870580bf5a9fd2c5e7170e7b72a3a66cd06d91fa39f  backend/crates/crm-api/src/lib.rs
c1ce9052a2caa18150100d100a12d50c018caabef3a7b63c7a5a94d1a7cc8319  backend/crates/crm-api/src/operator/backend.rs
a9a052ff77e013df9e8e22dbfceadd98d9f70bed716d1c9f44c15fab3f43f8bb  backend/crates/crm-api/src/routes/calls.rs
11e2028fe2239324a3e542a04802395be256de3fea7ecdc165414fa89c63cd35  backend/crates/crm-api/src/routes/capture.rs
0e34cae6100c0a715f762b4c0e66226f5221f80f50546ff1a1ac756dc98fd49e  backend/crates/crm-api/src/routes/custom_fields.rs
33543d9badb683113860851ba4e5f7e003d2c685a1d48c532822bb0b3765b8c8  backend/crates/crm-api/src/routes/inbound_email.rs
61924d920259aa4441c44279cf50993cc705b6989c1d7a218ed1552c96ec4aad  backend/crates/crm-api/src/routes/inquiry_sources.rs
27370ae0a30c3d0e726311a49aace2f042971fbffbd2e77be01f060aee985530  backend/crates/crm-api/src/routes/intake.rs
fe206c0c39c1ddcdb96aac26a684d48d685b23a605b3944b69790b73bcfdc732  backend/crates/crm-api/src/routes/invitations.rs
f318cbcab4a96797160c24a4abe84ba772eaff31fe2499ff674dc14310eb11bb  backend/crates/crm-api/src/routes/livekit_webhook.rs
e033ca3ccc4f663800d221b6a5ce5675cd509878dae1f988c536836b37b9119a  backend/crates/crm-api/src/routes/migration_imports.rs
5514a311f407017d8430777c3b7a6fc2cf1635c7d74021b32eada070a88659f6  backend/crates/crm-api/src/routes/mod.rs
1230b4988b0b45e29240df407497f4a1b6c73d105aefdbb0b3bd115e28acf1df  backend/crates/crm-api/src/routes/operator.rs
cc09c9d3d9b3715400b62bd0fbbdb42799108236db4a7e185f0f44d3c4ce14df  backend/crates/crm-api/src/routes/organization.rs
3b92c6b79fa09c27f22886c9e36db59e5e2eb132cb116d1ec6b8dae05e95cfc0  backend/crates/crm-api/src/routes/people.rs
b6eb1fc471202587160ee303dbf875b7427870c7f4475db315202fd6a0adc8f7  backend/crates/crm-api/src/routes/platform.rs
5e2a6e4aa996f7670a5231d74e5983fe10b4d107a369fff31e5fa73fe66128b1  backend/crates/crm-api/src/routes/realtime.rs
54aff1bf8695f6f95e31d6175f92b27326eed18cb8a2091155e09c8c644074c9  backend/crates/crm-api/src/routes/saved_lists.rs
1776ad935dabfd0c4f4c36ec7ded737b6c3b8a908a1877d017f8da99111a5fa3  backend/crates/crm-api/src/routes/session.rs
2b2416732b66bfd6d8a398bfa0e7082594791e99da4e82c7a9e9149499713369  backend/crates/crm-api/src/routes/stages.rs
e5f91025dd901a99757cf1c0478066db0e56c7ac14dbf7473285a6285bfd702d  backend/crates/crm-api/src/routes/tags.rs
48614d3f7422c50a3aec8fe5ffd2a9588eeab4acd0c511fade0638e3fd313701  backend/crates/crm-api/src/routes/tasks.rs
c9f5b6e592aefb99e880fe36a624933ed80365e9aa5a7d8f6b1e5abf8bbb3884  backend/crates/crm-api/src/routes/today.rs
7ae447ba75d7008568f1ce0e35bf48fb9091d16ac5be2a89a84a2adcb1882987  backend/crates/crm-api/src/routes/today_feeds.rs
138c00d250893ad790b32f70a555f87fd9686b2edbed7c39ba224cb188556493  backend/crates/crm-api/src/state.rs
3a4d444c9e5cb0ce31e276fa69dde0a3396df09374e7689f511a7853ac03df7a  backend/crates/crm-api/tests/all.rs
05c50a2e466a9b3d1e67c54c0ceb4db628b96495124abe39b5b8a920960209f4  backend/crates/crm-api/tests/db_import_contact_perf.rs
4abda11552604910d445913b3a2f50f843904591515f98d909e35e441378e659  backend/crates/crm-api/tests/db_import_gate.rs
fa05c4182366908600a5882cfb87d322e642925c252dac2edfb5998f23b1d126  backend/crates/crm-api/tests/db_import_http.rs
db701a354638df9f7d3f4d0406e12dc03cf13dd8cd4f005b4c3e3525f38eb7de  backend/crates/crm-api/tests/db_import_readers.rs
40ced08a6b40bf86816298416c6d985b19b28bcb118324d68478aa612a40676c  backend/crates/crm-api/tests/db_import_source.rs
1fc5a347159560bff89c8716f37924efd51e9e2192cf1832459109c5fe4db2dc  backend/crates/crm-api/tests/db_workspace_background.rs
4ffcbc2862883951c888f74143cad207a28acb8039564398c90999c2a8dc7d33  backend/crates/crm-api/tests/db_workspace_http.rs
4d78841bc1e50590b3e35451a8b447b2631c86f0541a8a3e87c48c322eb3c4a0  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/README.md
89842eb584f00b3fcb6e5e207e75114e34d1d436f3ec8ac325bb5dbe02778953  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/manifest.json
94ae5569b322f0898e26b6c693f5b070dac5d091a676a119ca90d55a7c43c009  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/mod.rs
c16b41d19b9be2d1c5332bf30516a24eff6c8b80a0478a4d09cbb20ce9a5d63c  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/person_sql.rs
ed5c1680476bfa6f41ebef270b53e0e5d5ac1dfd60481965c48cf96b718b5657  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/call_membership.sql
3a686f3e1abb293ba831fe3b7427f120a4657c508d6db2ba09f7c2a46e2ace3e  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/call_only.sql
fc38a85fb2124820fac12e18db7b33740ffdd4229d4f9bfde302612990611105  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/count_filtered_matches.sql
d42edfa7ac0efa2557c633bcff72b56b52b4d2bd1e61eaf8e75889cd3b80558f  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/filtered_summaries.sql
27d0a146cab352203f545a5855f6976be7edc58bd4587bc6ec14c81ede54a178  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/filtered_summaries_assignee_asc.sql
41fcb1cdee6fb2e6b69d765a52f21dca4849e7479715e7e8f3aac4e6124a0e06  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/filtered_summaries_assignee_desc.sql
08d88882eff99adba22eba8053f599b9c028369d3985058426d316c3d34d336d  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/filtered_summaries_created_asc.sql
adb8c050821121dca973ffe69340c977c3bc54d14d439d33ab4518db2d0afef4  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/filtered_summaries_name_asc.sql
deefe638489f357490c9c4490aa7115cf5167ce3027dac21662c4b76dfe6c756  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/filtered_summaries_name_desc.sql
737649b460693cb8147de65ba6cfc1017139e9024fa2be44156ca29087db244b  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/filtered_summaries_stage_asc.sql
efa2979afc11994589ee09ef48e56a483cc007d30bed88df611c902712eb6522  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/filtered_summaries_stage_desc.sql
582305813704b72be89ec008a4cbcbb1ae6fbae2962856bd7f567b001d23d41f  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/person_state.sql
9f301ed0828fe7a1463e53174133b4f1cedc5c3e7b20238761df4f6e9051cd1e  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/source_candidates.sql
651ccc7b21ef81d3b7d39a00f273018dfcd88274732e10e935725a04be6ac854  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/sql/source_membership.sql
ed65dda760d747959522e3fac20f86e4d72cc7a107bcf4b90d2b6bb540cfe5de  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/system_feeds_sql.rs
3c7a76e33fa2ef22f0f22ca70b6d106bcf48508efccd8a5051a55d5923efb466  backend/crates/crm-api/tests/fixtures/import_contact_c6c5930/today_sql.rs
18299304338db09520b9f419e93dd079493c5fcc048c614f179855e528909475  backend/crates/crm-api/tests/fixtures/import_support.rs
61e090528d8b416fd4cb159632bdd18bf32a4929f0ea594ed57105b4cdda0cb8  backend/crates/crm-app/src/auth/mod.rs
829fef1e8f466b8ad8d2fa9400473ae7e091be4574c589d2ebc720844c7787ca  backend/crates/crm-app/src/auth/workspace.rs
3e4c4d34ae35a4614ae7a9d1660bee58b282b112771c6c393920f02f6f23269c  backend/crates/crm-app/src/domain/admin/queries.rs
07fbc77eeed6a421473e2a057ee86320db2cfc515e4f945aaebf7d3b0b256c0a  backend/crates/crm-app/src/domain/capture/address.rs
0058e6d2fb21aee954778c0251af1929dfc2529338fdfb732291401ed71dcc91  backend/crates/crm-app/src/domain/capture/commands.rs
ccbf1f4c0ac72e71cfd8ef779b65c9a2858d0a7688e63683f10587ad7e533791  backend/crates/crm-app/src/domain/capture/receive.rs
e5300e583fb6c3b53c6f01b3df1b61eb8872cb32d3e44a95bd0602ff0a9b6e34  backend/crates/crm-app/src/domain/capture/store.rs
dd6e0c77f39d8bb19d4a1e8391e60816d0d488a61dbcc14923862cf433400e4d  backend/crates/crm-app/src/domain/commands/assign_person.rs
dfca7fc339e6cbe83a799f354886e32afdc5698711d27f9a0a94ff7e2dde9fe3  backend/crates/crm-app/src/domain/commands/change_person_stage.rs
aaf3627103e2e78d9aea05adbf3d03c61ed09d8c2f2975f1170bde21080915ee  backend/crates/crm-app/src/domain/commands/correct_call_outcome.rs
6a2e708982c84ba7c649500dc3a59f4e6f2666bb9695410bbe4e2293b75d12ae  backend/crates/crm-app/src/domain/commands/dial_call.rs
e6fa2ba60eb66683901fc9795aaa2d536ca5ff4bdc15a12694afbd45da3ce900  backend/crates/crm-app/src/domain/commands/hangup_call.rs
918ae0b7a5641cbd465ddb540fd244944a8974d0408554b8ea99f7bc10c1d635  backend/crates/crm-app/src/domain/commands/log_contact_attempt.rs
92226824ff3803445f7228d3799bd431ef7f7f4a277f5402635f636e77a03580  backend/crates/crm-app/src/domain/commands/mod.rs
1becd98010d66cff30b86cacb0ea007f87a41d7431382ae428a73a8c91356727  backend/crates/crm-app/src/domain/commands/receive_inquiry.rs
8d883106bdb4dc692bc5719f1a2c0b3c0bb924350c8a60907fd43e46f65b12c3  backend/crates/crm-app/src/domain/commands/start_call.rs
a6c4e5e73edf3917e6bc5c1c54888af74ac932ced68355967a8ceabb4b955104  backend/crates/crm-app/src/domain/commands/update_intake_settings.rs
94d73cbfa3109410da0a957670df1f8120930b63eadd771489c1b931df94a4c2  backend/crates/crm-app/src/domain/contact.rs
d046e0bcfef4ae6a98c7e5cedc97fdf9cfff99a1edda824133df1de35ea292d9  backend/crates/crm-app/src/domain/custom_field/commands.rs
929df3b457b1160e88965e86657ed2a0fc83b7f856476e59fed762510eaf94db  backend/crates/crm-app/src/domain/custom_field/queries.rs
e7fcc9a710997011068d5fc7027236ad9a79103f712eb79360c3a3932ea875e0  backend/crates/crm-app/src/domain/facts.rs
c47cc52acbdb56214736f63c28e71444cc1e4aad15b36ff53119a488a1a4d45a  backend/crates/crm-app/src/domain/inquiry/queries.rs
51977210284a63ccc8fbb07b5f9deafda118188c9032223557845d4e594f3a06  backend/crates/crm-app/src/domain/intake/extraction/worker.rs
f5e43def0110c8fc386e6348a7201f80c4fdba8edb0c7a6899caa212715db0ba  backend/crates/crm-app/src/domain/intake/rotate.rs
b6d5ce3e39038e636da5d852720eea762ed80dd3cfaa4c2c06afe6edcf00961d  backend/crates/crm-app/src/domain/intake/workbench.rs
6c0bb3d4aebe9e9dd40606ee5ce8eb714abed030b7a376b0c10ee93f01b9b774  backend/crates/crm-app/src/domain/migration/import_display.rs
29b6d685212cf763630a93c5a603087b885d87f16cecbfad07562b8ab22c7075  backend/crates/crm-app/src/domain/migration/import_source.rs
599426990b2e3bd2dd8a1644a79178c35e07964ee2da1fb001f8c241b304b81f  backend/crates/crm-app/src/domain/migration/import_worker.rs
fe86431f9f44ecdb85ba653942b791235062aedd81e9a92fa5efcd942182ba81  backend/crates/crm-app/src/domain/migration/imports.rs
5df11d757eeae4bc0315e942fb2e0213ad6f06458d8cd90c8db00df4edf62800  backend/crates/crm-app/src/domain/migration/mod.rs
2bf667f86d174d487df419dfc3d0ab6270a5fa47c3adc8b8ac1722af43222fa4  backend/crates/crm-app/src/domain/migration/snapshot_source.rs
7576f896e23724b6cf8c04e79085a36975c5c6560bca23a447a07e15e6e22620  backend/crates/crm-app/src/domain/migration/store.rs
6813c31ad959ec8d2cac041416bfebccf5d63890bf7d121b3d7401457b79027f  backend/crates/crm-app/src/domain/note/commands.rs
7878e95f9f258ab047c04529f43d40b6ec50e7a415b67bcb9ff606277c42f99f  backend/crates/crm-app/src/domain/note/queries.rs
b4ec0adbb337d926f7ca9055d07a64fb3ce0d88fd41d11461e623b8651bad438  backend/crates/crm-app/src/domain/person/queries.rs
30986a899b1fb0a32f4d92e454c0b1abf9596ce02916bb14e5d932544a0750bc  backend/crates/crm-app/src/domain/person/sql/filtered_summaries.sql
63f37a19805c6ec956c1da4e2242d72cb55a5f14469bce324dd4ba9e1378e18d  backend/crates/crm-app/src/domain/person/sql/filtered_summaries_assignee_asc.sql
35100d9f49886a0439e7b9e01fdff4274bc60d39a2a0e8308e29cf75a003fd7c  backend/crates/crm-app/src/domain/person/sql/filtered_summaries_assignee_desc.sql
86f2df85e71a35a74ef2cb0347f3d5e6cb97e7b2fdf0cc171fe71bc71b635859  backend/crates/crm-app/src/domain/person/sql/filtered_summaries_created_asc.sql
66608de6ee2ee7b6513542c530b40070469100b1da34426b2c6125d1ff6a1c00  backend/crates/crm-app/src/domain/person/sql/filtered_summaries_name_asc.sql
d9a37b516ab11598778d0b94a281095eb718497fec218337f7fca540a42f70e9  backend/crates/crm-app/src/domain/person/sql/filtered_summaries_name_desc.sql
0c5a84f414ccc57edf4e55d7314c807247bd6b18d99a1a3c2db27e0ba91f6bc3  backend/crates/crm-app/src/domain/person/sql/filtered_summaries_stage_asc.sql
e70277cec0bedbbdf5382c1288f7d60983e13f9819763b31f1f5c056933ae852  backend/crates/crm-app/src/domain/person/sql/filtered_summaries_stage_desc.sql
6e51e13a2112ea7e751a7f39d9404058e6e65a9a7b39c1cb25fd33d2ac7e858c  backend/crates/crm-app/src/domain/raw_payload/store.rs
f961a3590053fa4fafe0f8608e3522fc9bdbcbc3e32c1ab7e14d69ab7718648d  backend/crates/crm-app/src/domain/saved_list/commands.rs
f367d29734175e707c3fdbc3b307a9f3ec4c78fd4328e34e533babba99617777  backend/crates/crm-app/src/domain/saved_list/queries.rs
4e876f79439c65e2552329685c66066fb910a83580881ddfae51f53d35e05755  backend/crates/crm-app/src/domain/stage.rs
d8b14b753a8e7ac01845302f353477863c86ccc09e1dd13fefa7ce229490bf0b  backend/crates/crm-app/src/domain/tag/commands.rs
1d2747ef05ce00b66ffcb1dff9cb5aa2981520dc1eb9044ec5b00f1000fa72fd  backend/crates/crm-app/src/domain/tag/queries.rs
68b058f1de84139f046cbdfae3bb2174533f5235ca63d006cb5a1cbc60d50b75  backend/crates/crm-app/src/domain/task/commands.rs
b294c8682733e15f423c059cd17c3e3e4e426b08a0a2a64fe703c8df37c58c4f  backend/crates/crm-app/src/domain/task/queries.rs
5fe84f699235fb6af7b2283b6ab2039b56c524eca658b03670358785b5c00309  backend/crates/crm-app/src/domain/telephony/queries.rs
1f7233908d3041f368284b0daffd937ba118c6b9774369da386c9f4e143e3bca  backend/crates/crm-app/src/domain/telephony/settle.rs
d34308fb3945f5be2a88154deb956c7016f901cc540a7b66ccf74296bcb7727f  backend/crates/crm-app/src/domain/today/mod.rs
8bf2b13ed011caa12c840a015a82369bd7958f231dc3eb84d0669c135673095c  backend/crates/crm-app/src/domain/today/source_candidates.sql
38200a38d66bcb1a3a18100d268fe75fb17b85f0a6765529594293510059c851  backend/crates/crm-app/src/domain/today/sources.rs
3c2dfc76f3b5667cf2620f51d158d0b59135a4266462db8f209e832707a0711c  backend/crates/crm-app/src/domain/today/sql/task_only.sql
3f03ebc1a234bd110456083e793b3e4aa0f2aca415f8bf68d45dbf291d5cb041  backend/crates/crm-app/src/domain/today/system_feeds/commands.rs
0b937f752f68d52638417cda8ffe9e4f7855bf768b4f3fea3b08ff25e3227650  backend/crates/crm-app/src/domain/today/system_feeds/queries.rs
ffb5d47d81a8f8e4a63bb380f5d33587f233e3fd63d823c4dce643933d212c9b  backend/crates/crm-app/src/domain/today/system_feeds/sql/call_only.sql
851fa5bd39633130d7bb835716850f25746eba1c4c9ce731858485702638b5d8  backend/crates/crm-app/src/domain/today/system_feeds/sql/person_state.sql
b540bee22cca30d717c3556999bdeb6e283c010c3735799fae742fc3049ccb3e  backend/crates/crm-operator/src/service.rs
e628171b5f6d355eb8c6dc53dd9f412d32f89696e1bf1d77012e6238664a97f2  docs/specs/SLICE_010c_CONTRACT.md
7217d698c2e9ba9d9659ad14aaddb3029ea2802c07329aabe0b271a4bdb356b4  scripts/check
c78bc7f39dbcbd1d81f6a1714be9882135b51f6b04e56b32f7548d82a0fbf1da  scripts/migration-release-preflight
d8965fb9c94bb82bc8df0f798023849ea0b31e6c21231411954cec2bc3f687bb  scripts/tests/test_migration_release_preflight.py
```
