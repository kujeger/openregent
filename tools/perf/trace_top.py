#!/usr/bin/env python3
"""The spans of a timeline (run.sh's BUILD=trace cases) that cost most.

    tools/perf/trace_top.py <trace.json> [count] [match]

Per span name: total ms, ms per drawn frame, calls, and each thread's
share (main, render, pool), over the drawn frames after the first third
(loading left out). A drawn frame is one "update" span on the main
thread. Nested spans count in their own and their parents' rows; the
`match` argument keeps names containing it (e.g. `pass`, `batch`).
"""

import json
import sys
from collections import defaultdict


def spans(events):
    """(name, tid, start µs, ms) of every closed span."""
    open_ = defaultdict(list)
    for e in events:
        ph = e.get("ph")
        key = (e.get("pid"), e.get("tid"))
        if ph == "B":
            open_[key].append(e)
        elif ph == "E" and open_[key]:
            b = open_[key].pop()
            yield b["name"], e.get("tid"), b["ts"], (e["ts"] - b["ts"]) / 1e3
        elif ph == "X":
            yield e["name"], e.get("tid"), e["ts"], e.get("dur", 0) / 1e3


def main() -> None:
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    count = int(sys.argv[2]) if len(sys.argv) > 2 else 40
    match = sys.argv[3] if len(sys.argv) > 3 else ""

    with open(sys.argv[1]) as f:
        events = json.load(f)
    names = {e["tid"]: e["args"]["name"] for e in events if e.get("ph") == "M" and "args" in e}
    all_spans = list(spans(events))

    # The drawn frames: "update" spans; the first third skipped.
    updates = sorted(ts for name, _, ts, _ in all_spans if name == "update")
    if not updates:
        sys.exit("no update spans")
    since = updates[len(updates) // 3]
    frames = len(updates) - len(updates) // 3

    total = defaultdict(float)
    calls = defaultdict(int)
    threads = defaultdict(lambda: defaultdict(float))
    for name, tid, ts, ms in all_spans:
        if ts >= since and match in name:
            total[name] += ms
            calls[name] += 1
            threads[name][names.get(tid, str(tid))] += ms

    print(f"{frames} drawn frames; ms total, ms per frame, calls, threads\n")
    for name in sorted(total, key=total.get, reverse=True)[:count]:
        where = ", ".join(
            f"{t} {ms / total[name]:.0%}"
            for t, ms in sorted(threads[name].items(), key=lambda kv: -kv[1])[:3]
        )
        print(f"{total[name]:9.1f} {total[name] / frames:7.3f} {calls[name]:7d}  {name[:90]}  [{where}]")


if __name__ == "__main__":
    main()
