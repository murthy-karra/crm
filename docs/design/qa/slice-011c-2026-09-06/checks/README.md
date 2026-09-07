# Slice 011c — retained check outputs

These are intermediate implementation checkpoints, including failures and
reruns. They do not establish final-tree acceptance. The
[verification record](../../../../tasks/SLICE_011c_VERIFICATION.md) explains
the commands, failures, fixes, overlapping counts and remaining gates.

[MANIFEST.json](MANIFEST.json) records each source and archived SHA-256.
All copied outputs are byte-for-byte except the explicitly identified
database-debug line in `today-source-termination-rerun.txt`, replaced with
its SQLSTATE and static permission failure. The source output remains private.
`today-source-db-nextest.txt` was already a reconstructed sanitized summary;
it must not be described as raw stdout. The earlier zero-selected-tests
output was overwritten before this archive and is unavailable; its command
and failure are recorded in the verification document.

The successful checkpoints include the 35-test source/baseline suite, the
strengthened eight-test snapshot/settings/deadline suite, the subsequent
300 ms early-budget assertion, and the clean 40-test router/AppShell/recovery
run. Earlier failing and interrupted outputs remain alongside them. Counts
overlap and cannot be added as unique tests.

Private credentials, database URLs, QA bootstrap/runtime logs and browser
handoffs are excluded. HTTP benchmark attempts belong in the separate
performance archive.
