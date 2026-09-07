#!/usr/bin/env python3
"""Independently summarize raw WaveCapture JSON; this is not full acceptance.

Usage: python3 recompute_requests.py raw-case.json [raw-case-2.json ...]
The harness wraps waves in objects carrying static `case` and `series` labels.
No network/database access. Only known safe telemetry fields are summarized;
response bodies and arbitrary caller metadata are never emitted.
"""

import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path


def waves(value, fallback, series=None, case=None):
    if isinstance(value, list):
        for child in value:
            yield from waves(child, fallback, series, case)
    elif isinstance(value, dict):
        series = value.get("series", series)
        case = value.get("case", case)
        if all(key in value for key in ("arm", "phase", "concurrency", "wave")) and isinstance(value.get("attempts"), list):
            yield str(series or fallback), str(case or fallback), value
        else:
            for child in value.values():
                if isinstance(child, (dict, list)):
                    yield from waves(child, fallback, series, case)


def late_captures(value):
    if isinstance(value, list):
        for child in value:
            yield from late_captures(child)
    elif isinstance(value, dict):
        for key, child in value.items():
            if key == "late_capture_reconciliations":
                if not isinstance(child, list):
                    raise ValueError("invalid late capture collection")
                yield from child
            elif isinstance(child, (dict, list)):
                yield from late_captures(child)


def metadata_for(record):
    metadata = record["outcome"].get("detail", {}).get("metadata")
    return metadata if isinstance(metadata, dict) else record.get("_reconciled_join_telemetry")


def attach_late_captures(records_by_id, captures):
    attached = set()
    for capture in captures:
        identity = (capture["series"], capture["case"], capture["arm"], capture["attempt"])
        if identity in attached:
            raise ValueError("duplicate late capture")
        attached.add(identity)
        record = records_by_id.get(identity)
        if record is None or record["outcome"]["kind"] != "join_failure":
            raise ValueError("late capture does not identify a retained failed join")
        if any(record["context"][key] != capture[key] for key in ("phase", "wave", "arm")):
            raise ValueError("late capture context disagrees with failed join")
        record["_reconciled_join_telemetry"] = capture
    return len(attached)


def duration_ns(duration):
    secs, nanos = duration["secs"], duration["nanos"]
    if not isinstance(secs, int) or not isinstance(nanos, int) or secs < 0 or not 0 <= nanos < 1_000_000_000:
        raise ValueError("invalid serialized Duration")
    return secs * 1_000_000_000 + nanos


def nanoseconds(timing):
    kind = timing["kind"]
    if kind == "unavailable":
        return None
    if kind not in ("exact", "join_observed_upper_bound"):
        raise ValueError("unknown timing kind")
    return duration_ns(timing["duration"])


def milliseconds(value):
    return None if value is None else round(value / 1_000_000, 6)


def percentile(values, percent):
    if not values:
        return None
    ordered = sorted(values)
    return ordered[(percent * len(ordered) + 99) // 100 - 1]


def telemetry(records, field):
    values, outcomes, filter_kinds = [], Counter(), Counter()
    missing_metadata, missing_field = 0, 0
    allowed_filters = {"stage", "assigned_to", "source", "created", "last_inquiry",
                       "last_contact", "last_inbound", "has_replied", "has_phone", "has_email"}
    source = field == "source_evaluations"
    enumeration = field == "source_enumeration"
    if source:
        allowed_outcomes = {"complete", "invalid_filter", "unavailable", "timed_out"}
    elif enumeration:
        allowed_outcomes = {"complete", "unavailable", "timed_out"}
    else:
        allowed_outcomes = {"acquired", "timed_out", "failed"}
    for record in records:
        metadata = metadata_for(record)
        if not isinstance(metadata, dict):
            missing_metadata += 1
            continue
        events = metadata.get(field)
        if not isinstance(events, list):
            missing_field += 1
            continue
        for event in events:
            outcome = event["outcome"]
            if outcome not in allowed_outcomes:
                raise ValueError("unknown telemetry outcome")
            outcomes[outcome] += 1
            values.append(duration_ns(event["duration"]))
            if source:
                kinds = event["filter_kinds"]
                if not isinstance(kinds, list) or any(kind not in allowed_filters for kind in kinds):
                    raise ValueError("unknown static filter kind")
                filter_kinds.update(kinds)
    result = {
        "events": len(values), "outcomes": dict(outcomes),
        "attempts_without_metadata": missing_metadata,
        "attempts_missing_event_field": missing_field,
        "p95_ms": milliseconds(percentile(values, 95)),
        "max_ms": milliseconds(max(values)) if values else None,
    }
    if source:
        result["static_filter_kinds"] = dict(filter_kinds)
    elif not enumeration:
        result["headroom_from_observed_max_to_2s_ms"] = milliseconds(2_000_000_000 - max(values)) if values else None
    return result


def summarize(records):
    kinds, statuses, results, timing_kinds = Counter(), Counter(), Counter(), Counter()
    terminals, initial_terminals = Counter(), Counter()
    values, complete, source_evaluations = [], 0, 0
    for record in records:
        metadata = metadata_for(record)
        if isinstance(metadata, dict):
            for field, target in (("capture_terminal", terminals), ("initial_capture_terminal", initial_terminals)):
                terminal = metadata.get(field)
                if terminal is not None:
                    if terminal not in {"complete_body", "client_failure_quiescent", "client_failure_drain_timed_out"}:
                        raise ValueError("unknown capture terminal")
                    target[terminal] += 1
        timing_kinds[record["timing"]["kind"]] += 1
        duration = nanoseconds(record["timing"])
        if duration is not None:
            values.append(duration)
        outcome = record["outcome"]
        kinds[outcome["kind"]] += 1
        if outcome["kind"] == "response":
            detail = outcome["detail"]
            statuses[str(detail["status"])] += 1
            results[detail["result"]] += 1
            complete += detail["status"] == 200 and detail["result"] == "complete"
            source_evaluations += detail["whole_source_evaluations"]
        elif outcome["kind"] not in ("client_failure", "join_failure"):
            raise ValueError("unknown attempt outcome")
    return {
        "attempts": len(records), "complete_200": complete,
        "outcome_counts": dict(kinds), "status_counts": dict(statuses),
        "result_counts": dict(results), "timing_counts": dict(timing_kinds),
        "response_source_evaluations": source_evaluations,
        "reconciled_failed_joins": sum("_reconciled_join_telemetry" in record for record in records),
        "capture_terminal_counts": dict(terminals),
        "initial_capture_terminal_counts": dict(initial_terminals),
        "request_p50_ms": milliseconds(percentile(values, 50)),
        "request_p95_ms": milliseconds(percentile(values, 95)),
        "request_max_ms": milliseconds(max(values)) if values else None,
        "telemetry": {field: telemetry(records, field) for field in (
            "source_enumeration", "source_evaluations",
            "authentication_pool_acquisitions", "feed_pool_acquisitions")},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", type=Path, nargs="+")
    args = parser.parse_args()
    groups, records_by_id, captures = defaultdict(list), {}, []
    wave_count = 0
    for path in args.files:
        document = json.loads(path.read_text())
        captures.extend(late_captures(document))
        for series, case, wave in waves(document, path.stem):
            wave_count += 1
            arm, phase, concurrency = wave["arm"], wave["phase"], wave["concurrency"]
            if len(wave["attempts"]) != concurrency:
                raise ValueError("wave attempt count does not match concurrency")
            slots = set()
            for record in wave["attempts"]:
                context = record["context"]
                if any(context[key] != wave[key] for key in ("arm", "phase", "wave")):
                    raise ValueError("attempt context disagrees with containing wave")
                identity = (series, case, arm, context["id"])
                if identity in records_by_id:
                    raise ValueError("duplicate attempt ID within a series/case/arm")
                records_by_id[identity] = record
                slots.add(context["slot"])
            if slots != set(range(concurrency)):
                raise ValueError("wave slots are duplicated or missing")
            groups[series, case, arm, phase, concurrency].extend(wave["attempts"])
    if not wave_count:
        raise ValueError("no raw WaveCapture records found")
    reconciled = attach_late_captures(records_by_id, captures)
    output = []
    for (series, case, arm, phase, concurrency), records in sorted(groups.items()):
        output.append({
            "series": series, "case": case, "arm": arm, "phase": phase,
            "concurrency": concurrency, **summarize(records),
        })
    print(json.dumps({
        "scope": "Raw request and known safe telemetry recount. Matrix completeness, body parity, trace sentinel checks and final acceptance require separate checks.",
        "waves": wave_count, "reconciled_failed_joins": reconciled, "groups": output,
    }, indent=2))


if __name__ == "__main__":
    main()
