#!/usr/bin/env python3
"""Render the harness's retained Phase B artifact as Markdown tables.

Usage: python3 summarize_run.py run-<nanos>.json
Reads only the harness's safe summary fields (counts, typed outcomes, timing
percentiles, acceptance flags). No network or database access; it does not
recompute acceptance and does not replace the harness's own gate.
"""
import json
import sys


def ms(value):
    if value is None:
        return None
    if isinstance(value, dict):
        return value.get("secs", 0) * 1000 + value.get("nanos", 0) / 1e6
    return value


def fmt(value):
    return "–" if value is None else f"{value:,.0f}"


def flag(ok):
    return "yes" if ok else "**NO**"


def main(path):
    data = json.load(open(path))
    fixture = data["fixture"]
    print(f"Protocol `{data['protocol']}`; fixture people={fixture['people']:,} inquiries={fixture['inquiries']:,} "
          f"contact facts={fixture['contact_facts']:,} corrections={fixture['contact_corrections']:,} "
          f"inbound={fixture['inbound_records']:,}; clock {fixture['fixed_clock']}; "
          f"source hash `{fixture['source_hash'][:12]}…`; build hash `{fixture['build_hash'][:12]}…`.")
    print(f"Series: {len(data['series'])}; late capture reconciliations: {len(data['late_capture_reconciliations'])}; "
          f"sentinel safety: {data['sentinel_safety']}.")
    print()
    print("| Case | Series | Arm | Attempts exp/rec/complete | Source evals | c1 p95 / cap | c10 p95 / cap | c20 p95 / cap | Whole-source p95 / max | Enum max | Auth acq p95 / max | Feed acq p95 / max | Feed headroom |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for s in data["series"]:
        sm = s["summary"]
        nc = sm["normal_completion"]
        cells = []
        for c in ("1", "10", "20"):
            v = sm["request_by_concurrency"].get(c)
            if v is None:
                cells.append("–")
            else:
                cells.append(f"{fmt(ms(v['request_timing']['p95']))} / {fmt(ms(v['request_p95_limit']))}{'' if v['p95_within_limit'] else ' **NO**'}")
        ws = sm["whole_source_timing"]
        en = sm["source_enumeration_timing"]
        aa = sm["authentication_pool_acquisition_timing"]
        fa = sm["feed_pool_acquisition_timing"]
        print(f"| {s['case']} | {s['series']} | {s['arm']} | {nc['expected_attempts']}/{nc['recorded_attempts']}/{nc['normal_complete_attempts']}{'' if nc['passed'] else ' **NO**'} | "
              f"{nc['whole_source_evaluations']} | {cells[0]} | {cells[1]} | {cells[2]} | "
              f"{fmt(ms(ws.get('p95')))} / {fmt(ms(ws.get('max')))}{'' if sm['whole_source_p95_within_limit'] else ' **NO**'} | "
              f"{fmt(ms(en.get('max')))}{'' if sm['source_enumeration_max_within_budget'] else ' **NO**'} | "
              f"{fmt(ms(aa.get('p95')))} / {fmt(ms(aa.get('max')))} | {fmt(ms(fa.get('p95')))} / {fmt(ms(fa.get('max')))} | "
              f"{fmt(ms(sm.get('feed_pool_headroom_to_two_seconds')))} |")
    print()
    print("| Paired zero-source case | Comparison | Payload parity | Hash parity | c1 orig→final (allowed) | c10 orig→final (allowed) | c20 orig→final (allowed) |")
    print("|---|---|---|---|---|---|---|")
    for r in data["paired_zero_source_parity"]:
        cells = []
        for c in ("1", "10", "20"):
            v = r["p95_by_concurrency"].get(c)
            cells.append("–" if v is None else f"{fmt(ms(v['original_p95']))}→{fmt(ms(v['final_p95']))} ({fmt(ms(v['allowed_final_p95']))}){'' if v['p95_within_regression_limit'] else ' **NO**'}")
        print(f"| {r['case']} | {flag(r['comparison_present'])} | {flag(r['exact_normalized_payload_parity'])} | {flag(r['normalized_hash_parity'])} | {cells[0]} | {cells[1]} | {cells[2]} |")
    final = [s for s in data["series"] if s["arm"] == "final"]
    orig = [s for s in data["series"] if s["arm"] == "frozen_original"]
    def measured(series):
        return sum(len(w["wave"]["attempts"]) for s in series for w in s["measured"])
    def warm(series):
        return sum(len(w["wave"]["attempts"]) for s in series for w in s["warmups"])
    print()
    print(f"Measured attempts: final {measured(final)}, original {measured(orig)}; warm-up attempts {warm(data['series'])}; "
          f"final whole-source evaluations {sum(s['summary']['normal_completion']['whole_source_evaluations'] for s in final)}.")
    print(f"Series passing the harness's normal-completion gate: {sum(1 for s in data['series'] if s['summary']['normal_completion']['passed'])} of {len(data['series'])}.")


if __name__ == "__main__":
    main(sys.argv[1])
