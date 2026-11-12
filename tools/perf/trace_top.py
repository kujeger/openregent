#!/usr/bin/env python3
"""The spans of a timeline (run.sh's BUILD=trace cases) that cost most.

    tools/perf/trace_top.py <trace.json> [count] [match]
    tools/perf/trace_top.py <trace.json> [count] --schedule <Name>

Per span name (a span's `name` and its `args.name`, as `system: name="..."`):
total ms, ms per drawn frame, calls, and each thread's share (main, render,
pool), over the drawn frames of the game. A drawn frame is one "update" span
on the main thread. The window starts SETTLE frames after the first span of
the game's clock (`openregent::game::clock::tick`), so loading is left out;
a trace with no game (a menu) takes the frames after the first third. The
output says which. Nested spans count in their own and their parents' rows;
the `match` argument keeps names containing it (e.g. `pass`, `batch`).

--schedule <Name> (e.g. PostUpdate, Update, ExtractSchedule, Render): the
schedule's wall ms per drawn frame, then the spans that ran inside it, ms and
calls per frame, `count` rows. A span belongs to the schedule when it
- ran on the schedule's own thread inside its interval, outermost only (a span
  within a counted span is in that one's row), or
- is a `system:` span on a task-pool thread ("Task Pool" in its name) that
  began and ended inside the interval (a worker running the schedule's
  systems).
Limit: a pool worker's system of another schedule running at the same time
(e.g. the render thread's) cannot be told apart and counts too. The last line
is the remainder: wall time not covered by any counted span, i.e. the
executor's own cost and waiting. Rows sum past the wall time when systems
run in parallel.
"""

import argparse
import json
import sys
from bisect import bisect_left
from collections import defaultdict

# Drawn frames skipped after the game's first tick: its systems warm up.
SETTLE = 10

# The span whose first appearance is the game starting.
GAME_SPAN = "openregent::game::clock::tick"

# Part of a task-pool thread's name.
POOL = "Task Pool"

# The prefix of a system's span name.
SYSTEM = "system: "


def label(e) -> str:
    """`system: name="x"` for a span `system` with `args.name`."""
    args = e.get("args") or {}
    return f"{e['name']}: name={args['name']}" if "name" in args else e["name"]


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
            yield label(b), e.get("tid"), b["ts"], (e["ts"] - b["ts"]) / 1e3
        elif ph == "X":
            yield label(e), e.get("tid"), e["ts"], e.get("dur", 0) / 1e3


def window(all_spans):
    """(start µs, frames, rule) of the drawn frames measured."""
    updates = sorted(ts for name, _, ts, _ in all_spans if name == "update")
    if not updates:
        sys.exit("no update spans")
    game = min((ts for name, _, ts, _ in all_spans if GAME_SPAN in name), default=None)
    later = [i for i, ts in enumerate(updates) if game is not None and ts >= game]
    if later:
        first = min(later[0] + SETTLE, len(updates) - 1)
        rule = f"from {SETTLE} frames after the game's first tick"
    else:
        first = len(updates) // 3
        rule = "no game in the trace: after the first third"
    return updates[first], len(updates) - first, rule


class Rows:
    """One thread's spans (start, end, name), sorted by start, longest first."""

    def __init__(self, rows):
        self.rows = sorted(rows, key=lambda r: (r[0], -r[1]))
        self.starts = [r[0] for r in self.rows]

    def outermost(self, lo, hi, skip=None):
        """The spans starting in [lo, hi) and ending by hi, in no other such
        span; `skip` itself left out."""
        found = []
        reach = lo
        for row in self.rows[bisect_left(self.starts, lo) :]:
            if row[0] >= hi:
                break
            if row[0] >= reach and row[1] <= hi and row != skip:
                found.append(row)
                reach = row[1]
        return found


def covered(intervals) -> float:
    """µs covered by the union of (start, end) pairs."""
    total = 0.0
    reach = None
    for start, end in sorted(intervals):
        if reach is None or start >= reach:
            total += end - start
            reach = end
        elif end > reach:
            total += end - reach
            reach = end
    return total


def schedule(all_spans, names, since, frames, wanted, count) -> None:
    """The per-schedule breakdown (--schedule)."""
    own_name = f"schedule: name={wanted}"
    runs = sorted((ts, ts + ms * 1e3, tid) for name, tid, ts, ms in all_spans if name == own_name and ts >= since)
    if not runs:
        found = sorted({n for n, *_ in all_spans if n.startswith("schedule: ")})
        sys.exit(f"no {own_name} spans in the window; schedules: {', '.join(found)}")

    every = defaultdict(list)
    systems = defaultdict(list)
    for name, tid, ts, ms in all_spans:
        if ts >= since:
            every[tid].append((ts, ts + ms * 1e3, name))
            if name.startswith(SYSTEM):
                systems[tid].append((ts, ts + ms * 1e3, name))
    every = {tid: Rows(rows) for tid, rows in every.items()}
    systems = {tid: Rows(rows) for tid, rows in systems.items()}

    total = defaultdict(float)
    calls = defaultdict(int)
    wall = 0.0
    gap = 0.0
    for start, end, tid in runs:
        # The schedule's own span is not its child.
        own = every[tid].outermost(start, end, skip=(start, end, own_name))
        others = [r for t, rows in systems.items() if t != tid and POOL in names.get(t, "") for r in rows.outermost(start, end)]
        picked = own + others
        for s, e, name in picked:
            total[name] += (e - s) / 1e3
            calls[name] += 1
        wall += (end - start) / 1e3
        gap += (end - start - covered([(s, e) for s, e, _ in picked])) / 1e3

    thread = names.get(runs[0][2], str(runs[0][2]))
    print(f"{wanted}: {len(runs)} runs in {frames} drawn frames, on thread {thread}")
    print("spans: same thread, outermost; pool threads' `system:` spans inside the interval (may include other schedules')")
    print(f"{wall / frames:9.3f} ms/frame wall\n")
    print("ms/frame  calls/frame  span")
    for name in sorted(total, key=total.get, reverse=True)[:count]:
        print(f"{total[name] / frames:9.3f} {calls[name] / frames:12.2f}  {name[:110]}")
    print(f"{gap / frames:9.3f} {'':12}  unattributed: wall minus the time the spans above cover")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("trace", help="trace.json")
    parser.add_argument("count", type=int, nargs="?", default=40, help="rows to show")
    parser.add_argument("match", nargs="?", default="", help="keep names containing this")
    parser.add_argument("--schedule", metavar="NAME", help="break this schedule down")
    options = parser.parse_args()

    with open(options.trace) as f:
        events = json.load(f)
    names = {e["tid"]: e["args"]["name"] for e in events if e.get("ph") == "M" and "args" in e}
    all_spans = list(spans(events))
    del events

    since, frames, rule = window(all_spans)
    print(f"window: {rule}; {frames} drawn frames")

    if options.schedule:
        schedule(all_spans, names, since, frames, options.schedule, options.count)
    else:
        total = defaultdict(float)
        calls = defaultdict(int)
        threads = defaultdict(lambda: defaultdict(float))
        for name, tid, ts, ms in all_spans:
            if ts >= since and options.match in name:
                total[name] += ms
                calls[name] += 1
                threads[name][names.get(tid, str(tid))] += ms

        print("ms total, ms per frame, calls, threads\n")
        for name in sorted(total, key=total.get, reverse=True)[: options.count]:
            where = ", ".join(
                f"{t} {ms / total[name]:.0%}"
                for t, ms in sorted(threads[name].items(), key=lambda kv: -kv[1])[:3]
            )
            print(f"{total[name]:9.1f} {total[name] / frames:7.3f} {calls[name]:7d}  {name[:90]}  [{where}]")


if __name__ == "__main__":
    main()
