#!/bin/zsh
set -eu
test "${QA_RUNTIME_AUTHORIZED:-}" = yes || { print -u2 'Await root authorization after gates, measurements and fresh context.'; exit 1; }
qa_dir=/private/tmp/crm-010f2-qa-694szdwe
qa_node=/Users/karrad/.nvm/versions/node/v24.16.0/bin/node
mkdir -p "$qa_dir/browser-phase-logs"
chmod 700 "$qa_dir/browser-phase-logs"
run_phase() {
  local qa_case=$1
  local qa_phase=$2
  print "Starting synthetic browser phase: $qa_case / $qa_phase"
  "$qa_node" "$qa_dir/browser_driver.mjs" --authorize-runtime --case="$qa_case" --phase="$qa_phase" > "$qa_dir/browser-phase-logs/$qa_case-$qa_phase.log" 2>&1
  print "Passed synthetic browser phase: $qa_case / $qa_phase"
}
run_phase complete pre-child
run_phase cancel pre-child
run_phase budget pre-child
run_phase complete plan
run_phase complete disconnect
run_phase complete complete-confirm-lost
run_phase complete complete-drain
run_phase complete native
run_phase complete access
run_phase cancel plan
run_phase cancel cancel-partial
run_phase cancel cancel-revision-and-stop
run_phase cancel native
run_phase budget plan
run_phase budget budget-queue
print 'Root handoff required: budget child queued with worker frozen; lower deployment ceiling and restart API.'
