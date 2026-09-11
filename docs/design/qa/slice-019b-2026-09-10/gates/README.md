# Slice 019b final gate logs

Exact captured stdout/stderr and shell exit statuses from the delivered source
tree, code SHA-256 `a75ee70b9b3b51021ca00853109e71ff8c222ca46ad1f91c0937976fabf88e0a`.
All final gates passed. Logs include the first formatting failure for completeness.

| Command | Log | Exit status |
|---|---|---|
| `./scripts/check` | [check.log](check.log) | [0](check.status) |
| `./scripts/sqlx-prepare` | [sqlx.log](sqlx.log) | [0](sqlx.status) |
| `./scripts/check-db` | [db.log](db.log) | [0](db.status) |
| Single authenticated performance test | [perf.log](perf.log) | [0](perf.status) |
| Initial check attempt, formatting stopped | [check-format-attempt.log](check-format-attempt.log) | [1](check-format-attempt.status) |

The workspace formatter corrected the initial failure before the successful
full code gate. Performance ran once. See the [verification record](../../../../tasks/SLICE_019b_VERIFICATION.md)
for commands, test counts, review dispositions and limitations.
