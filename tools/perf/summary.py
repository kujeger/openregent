#!/usr/bin/env python3
"""The summary of a perf run (tools/perf/run.sh), or two side by side.

Per case: wall s, CPU s (user + sys), peak RSS MB; what disturbed it:
thousands of involuntary context switches, major page faults, the busy
share of the pinned cores' SMT siblings (run.sh); instructions and
cycles (G, user space), then each of the game's timers (openregent/src/perf/mod.rs), ms:
mean / p50 / p99 / max, then the count of samples past 25 ms (perf::SPIKE)
where the log gives it: a stutter is the tail, not the mean. A drawn run reports every 600 drawn frames; its first
report (loading, shader builds) is left out, the rest merged: mean
weighted by samples (p50 likewise, an approximation: not the merged
median; left out when a report lacks it, as old logs do), p99 and max the largest of the reports', spikes
summed. The
renderer's series (perf/gpu.rs: gpu_*, draws_*, ...) follow by name,
then its GPU memory, MB, as last reported.

A run of repeats (tools/perf/quiet.sh: <run>/1, <run>/2, ...): each
figure the median of the repeats, then [min-max] of its first number.
Two runs: the second's cases, and their change from the first, of the
medians' first number. A case that failed in any repeat (run.sh's
`failed` mark) says FAILED: its figures are not to be compared. One
whose pinned core's SMT sibling was busy in any repeat (SIBLING_BUSY)
says DISTURBED: its wall time and cycles are not to be trusted, its
instructions are. So does one run.sh started on a busy machine (its
`disturbed` mark: cargo, rustc or openregent running, or a high load).
"""

import argparse
import re
import statistics
from pathlib import Path

# A pinned core's SMT sibling busier than this, in percent, marks the
# case DISTURBED: a sibling kept busy slowed a replay by 60% in wall time
# and cycles, its instructions the same. 5 is a judgement, not measured.
SIBLING_BUSY = 5.0

# The timers in the order shown; `steps` is a count, not ms.
TIMERS = ["main", "interval", "wait", "step", "steps", "publish", "asked", "clone", "fog", "deep", "snapshot", "fold", "henchmen", "ground"]

PERF = re.compile(r"maj_perf: (\w+) (\w+) n (\d+) mean ([\d.]+)(?: p50 ([\d.]+))? p99 ([\d.]+) max ([\d.]+)(?: over (\d+))?")
STARTUP = re.compile(r"maj_perf: startup (\w+) (\d+) ms")
MEMORY_GPU = re.compile(r"maj_perf: memory gpu allocated ([\d.]+) reserved ([\d.]+) .* images \d+ ([\d.]+)")
MEMORY_ART = re.compile(r"maj_perf: memory art sized \d+ ([\d.]+) plain \d+ ([\d.]+) map \d+ ([\d.]+)")
MEMORY = ["gpu alloc MB", "gpu resv MB", "images MB", "art sized MB", "art plain MB", "art map MB"]
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def case(dir: Path) -> dict:
    """One case's figures, by name."""
    figures = {}
    time = dir / "time.txt"
    if time.exists():
        fields = dict(re.findall(r"(\w+)=([\d.]+)", time.read_text()))
        if fields:
            figures["wall s"] = float(fields["wall"])
            figures["cpu s"] = float(fields["user"]) + float(fields["sys"])
            figures["rss MB"] = float(fields["rss_kb"]) / 1024
            # Disturbance: involuntary context switches, major page faults.
            if "ctx" in fields:
                figures["ctx k"] = float(fields["ctx"]) / 1000
                figures["faults"] = float(fields["faults"])
    siblings = dir / "siblings.txt"
    if siblings.exists():
        busy = re.search(r"busy=([\d.]+)%", siblings.read_text())
        if busy:
            figures["sibling %"] = float(busy.group(1))
    stat = dir / "stat.csv"
    if stat.exists():
        # One line per core type on a hybrid CPU: P and E cores summed.
        counts = [
            int(parts[0])
            for parts in (line.split(",") for line in stat.read_text().splitlines())
            if len(parts) > 2 and "instructions" in parts[2] and parts[0].isdigit()
        ]
        if counts:
            figures["instr G"] = sum(counts) / 1e9
        cycles = [
            int(parts[0])
            for parts in (line.split(",") for line in stat.read_text().splitlines())
            if len(parts) > 2 and "cycles" in parts[2] and parts[0].isdigit()
        ]
        if cycles:
            figures["cycles G"] = sum(cycles) / 1e9
    if (dir / "failed").exists():
        figures["failed"] = True
    if (dir / "disturbed").exists():
        figures["busy"] = True

    log = dir / "out.log"
    # Only the report lines, read a line at a time: a log can be gigabytes.
    text = ""
    if log.exists():
        with log.open(errors="replace") as lines:
            text = "".join(ANSI.sub("", line) for line in lines if "maj_perf" in line)
    reports = {}
    for what, timer, n, mean, p50, p99, top, over in PERF.findall(text):
        # Old logs have no p50: the mean stands in, and the figure is marked absent below.
        reports.setdefault(timer, []).append((what, int(n), float(mean), float(p99), float(top), over, p50))
    for timer, rows in reports.items():
        # A drawn run's first report covers its loading.
        if rows[0][0] == "drawn" and len(rows) > 1:
            rows = rows[1:]
        n = sum(r[1] for r in rows)
        mean = sum(r[1] * r[2] for r in rows) / n
        p50s = [r[6] for r in rows]
        # p50 shown only when every report has one.
        if all(p50s):
            p50 = sum(r[1] * float(r[6]) for r in rows) / n
            figures[timer] = (mean, p50, max(r[3] for r in rows), max(r[4] for r in rows))
        else:
            figures[timer] = (mean, max(r[3] for r in rows), max(r[4] for r in rows))
        if all(r[5] for r in rows):
            figures[timer] += (sum(int(r[5]) for r in rows),)
    for pattern, keys in ((MEMORY_GPU, MEMORY[:3]), (MEMORY_ART, MEMORY[3:])):
        found = pattern.findall(text)
        if found:
            figures.update(zip(keys, map(float, found[-1])))
    for what, ms in STARTUP.findall(text):
        figures[f"to {what} ms"] = float(ms)
    return figures


def number(value: float) -> str:
    return f"{value:.2f}" if abs(value) < 100 else f"{value:.0f}"


def first(value) -> float:
    return value[0] if isinstance(value, tuple) else value


def middle(values: list):
    """The median of the repeats; of each number of a timer's triple."""
    if isinstance(values[0], tuple):
        return tuple(statistics.median(v) for v in zip(*values))
    return statistics.median(values)


def cell(values: list) -> str:
    """One figure of one run: "-", the value, or the median [min-max]."""
    values = [v for v in values if v is not None]
    if not values:
        return "-"
    median = middle(values)
    text = "/".join(map(number, median)) if isinstance(median, tuple) else number(median)
    if len(values) > 1:
        firsts = [first(v) for v in values]
        text += f" [{number(min(firsts))}-{number(max(firsts))}]"
    return text


def change(before: list, after: list) -> str:
    """The after median's first number against the before's, in percent."""
    before = [first(v) for v in before if v is not None]
    after = [first(v) for v in after if v is not None]
    if not before or not after or statistics.median(before) == 0:
        return ""
    return f"{statistics.median(after) / statistics.median(before) - 1:+.1%}"


def cases(dir: Path) -> dict:
    return {d.name: d for d in sorted(dir.iterdir()) if (d / "args.txt").exists()}


def rounds(dir: Path) -> list:
    """A run's repeats: its numbered subdirectories, else the run itself."""
    numbered = sorted((d for d in dir.iterdir() if d.name.isdigit() and d.is_dir()), key=lambda d: int(d.name))
    return numbered or [dir]


def run(dir: Path) -> dict:
    """Per case: its args line and each repeat's figures."""
    found = {}
    for repeat in rounds(dir):
        for name, case_dir in cases(repeat).items():
            args, figures = found.setdefault(name, [(case_dir / "args.txt").read_text().strip(), []])
            figures.append(case(case_dir))
    return found


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("run", type=Path, help="a run directory (run.sh's <out-dir>, or quiet.sh's)")
    parser.add_argument("after", type=Path, nargs="?", help="a second run, shown as the change from the first")
    options = parser.parse_args()
    paths = [p for p in (options.run, options.after) if p]
    for path in paths:
        if not path.is_dir():
            parser.error(f"{path}: not a directory")

    runs = [run(p) for p in paths]
    # The cases of the last run: a comparison shows those run now.
    names = list(runs[-1])
    order = ["wall s", "cpu s", "rss MB", "ctx k", "faults", "sibling %", "instr G", "cycles G", "to drawn ms", "to game ms"] + TIMERS
    known = set(order) | set(MEMORY) | {"failed", "busy"}
    named = sorted({k for r in runs for _, rs in r.values() for c in rs for k in c if k not in known})
    order += named + MEMORY
    width = max(len(k) for k in order)

    if len(runs) > 1:
        print(f"before: {paths[0]}\nafter:  {paths[1]}\n")
    repeats = sorted({len(rs) for r in runs for _, rs in r.values()})
    if repeats != [1]:
        print(f"repeats: {', '.join(map(str, repeats))}; median [min-max]")
    print("timers in ms, mean/[p50/]p99/max[/count past 25 ms] (steps: a count)\n")
    for name in names:
        args = next(r[name][0] for r in runs if name in r)
        failed = any(c.get("failed") for r in runs for c in r.get(name, ("", []))[1])
        disturbed = any(c.get("sibling %", 0) > SIBLING_BUSY for r in runs for c in r.get(name, ("", []))[1])
        busy = any(c.get("busy") for r in runs for c in r.get(name, ("", []))[1])
        marks = (
            ("  FAILED" if failed else "")
            + ("  DISTURBED (sibling busy)" if disturbed else "")
            + ("  DISTURBED (machine busy)" if busy else "")
        )
        print(f"{name}: {args}{marks}")
        for key in order:
            values = [[c.get(key) for c in r.get(name, ("", []))[1]] for r in runs]
            if any(v is not None for vs in values for v in vs):
                cells = "  ->  ".join(f"{cell(vs):<34}" for vs in values)
                if len(runs) > 1:
                    cells += f"  {change(*values)}"
                print(f"  {key:<{width}} {cells.rstrip()}")
        print()


if __name__ == "__main__":
    main()
