#!/usr/bin/env bash
#
# The perf cases on a quiet machine, repeated, kept and compared: run by
# hand when no build or game runs, as other load skews the timings.
#
#   tools/perf/quiet.sh [options] [case|group...]
#   tools/perf/quiet.sh                     the default groups, 3 times
#   tools/perf/quiet.sh -n 5 lan h512-bot5  a group and a case, 5 times
#   tools/perf/quiet.sh --list              groups, cases, expected time
#
#   build.sh (and `build.sh trace` for trace cases)
#     └── refuse if busy: cargo, rustc or openregent running, or the
#         1-minute load above $max_load (after our build: wait for it)
#           └── run.sh <run>/1 <cases>, run.sh <run>/2 <cases>, ...
#                 (pinned to the performance cores: run.sh's PERF_CPUS)
#                 └── summary.py <run>            -> <run>/summary.txt
#                     summary.py <previous> <run> -> <run>/compare.txt
#
# Groups: the cases file's `#= <group>` lines; `all` is every group but
# trace (its own build). A run goes to <runs>/<date>-<commit>[-dirty]
# (dirty: changes under crates/ or tools/perf/), <runs> local/perf-runs or
# $PERF_RUNS. The previous run is the newest finished one there.
#
# options:
#   -n <count>         repeats (default 3)
#   --force            run even when busy
#   --no-build         use the binaries as they are
#   --cpus <list>      cores to pin to (run.sh's PERF_CPUS; "" none)
#   --runs <dir>       where runs go
#   --out <dir>        this run's directory
#   --against <run>    compare with this run (a name in <runs>, or a
#                      path); `none` for no comparison
#   --list             print groups and cases, then exit

set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
here=$root/tools/perf

# The 1-minute load average above which the machine is busy, whatever
# its core count: one 4-job cargo build lifts the load to about 4, an idle
# desktop stays near or below 1 (**Inferred**: neither measured alone).
# 2 is between: room for a browser, not a build.
max_load=2.0

# What makes the machine busy: builds and other games.
busy_names=(cargo rustc clippy-driver openregent)

# The groups run when none is named: about 6 minutes a repeat (weak:
# lavapipe, about 9; trace: its own build).
default_groups=(headless replay drawn camera startup lan gpu)

# After our own build, how long to wait for its load to fade.
settle_s=300

count=3
force=no
build=yes
runs=${PERF_RUNS:-$root/local/perf-runs}
out=
against=
list=no
wanted=()

while [[ $# -gt 0 ]]; do
    case $1 in
    -n) count=$2; shift ;;
    --force) force=yes ;;
    --no-build) build=no ;;
    --cpus) export PERF_CPUS=$2; shift ;;
    --runs) runs=$2; shift ;;
    --out) out=$2; shift ;;
    --against) against=$2; shift ;;
    --list) list=yes ;;
    -h | --help) sed -n '3,/^$/s/^# \{0,1\}//p' "$0"; exit 0 ;;
    -*) echo "$1: unknown option (--help)" >&2; exit 2 ;;
    *) wanted+=("$1") ;;
    esac
    shift
done

if ! [[ $count =~ ^[1-9][0-9]*$ ]]; then
    echo "-n $count: not a count" >&2
    exit 2
fi

# The cases file: "<group> <name>" per case.
mapfile -t table < <(awk '
    /^#= / { g = $2; next }
    /^#/ || NF == 0 { next }
    { print g, $1 }' "$here/cases")
groups=$(printf '%s\n' "${table[@]}" | cut -d' ' -f1 | uniq | paste -sd' ')

# Seconds a repeat of the named cases takes: each case's wall in the
# newest run under $runs that has it. A case never run adds nothing and
# marks the total "+": more than that; "?" when no case was run.
expected() {
    local total=0 mark= name wall dir
    for name in "$@"; do
        wall=
        for dir in $(ls -td "$runs"/*/ 2>/dev/null); do
            if [[ -f $dir/1/$name/time.txt ]]; then
                wall=$(sed -n 's/.*wall=\([0-9.]*\).*/\1/p' "$dir/1/$name/time.txt")
                break
            fi
        done
        if [[ -z $wall ]]; then
            mark='+'
        else
            total=$(awk -v a="$total" -v b="$wall" 'BEGIN { print a + b }')
        fi
    done
    if [[ $total == 0 && -n $mark ]]; then
        echo "?"
    else
        echo "${total%.*}$mark s"
    fi
}

if [[ $list == yes ]]; then
    echo "groups: $groups (all: every one but trace; default: ${default_groups[*]})"
    echo "a repeat, as last run under $runs (+: some cases never run there; ?: none):"
    for group in $groups; do
        names=()
        for row in "${table[@]}"; do
            [[ ${row%% *} == "$group" ]] && names+=("${row#* }")
        done
        printf '  %-9s %-7s %s\n' "$group" "$(expected "${names[@]}")" "${names[*]}"
    done
    exit 0
fi

# The case names, from the case and group words.
[[ ${#wanted[@]} -eq 0 ]] && wanted=("${default_groups[@]}")
cases=()
traced=no
for word in "${wanted[@]}"; do
    found=no
    for row in "${table[@]}"; do
        read -r group name _ <<<"$row"
        if [[ $word == "$name" || $word == "$group" || ($word == all && $group != trace) ]]; then
            [[ " ${cases[*]} " == *" $name "* ]] || cases+=("$name")
            [[ $group == trace ]] && traced=yes
            found=yes
        fi
    done
    if [[ $found == no ]]; then
        echo "$word: no such case or group (groups: $groups all)" >&2
        exit 2
    fi
done

# What makes the machine busy now, one line each; nothing when quiet.
busy() {
    local name load
    for name in "${busy_names[@]}"; do
        pgrep -a -x "$name" | cut -c1-120 | sed 's/^/  running: /' || true
    done
    load=$(cut -d' ' -f1 /proc/loadavg)
    if awk -v a="$load" -v b="$max_load" 'BEGIN { exit !(a > b) }'; then
        echo "  load: $load (over $max_load)"
    fi
}

# Refuses to go on while busy, unless --force.
quiet_or_stop() {
    local found
    found=$(busy)
    if [[ -n $found ]]; then
        echo "busy:"
        echo "$found"
        if [[ $force == yes ]]; then
            echo "--force: running anyway; timings may be skewed"
        else
            echo "wait, or --force"
            exit 1
        fi
    fi
}

# This run's directory, named by date and commit.
if [[ -z $out ]]; then
    commit=$(git -C "$root" rev-parse --short HEAD)
    [[ -n $(git -C "$root" status --porcelain -- crates tools/perf) ]] && commit+=-dirty
    out=$runs/$(date +%F)-$commit
    n=2
    base=$out
    while [[ -e $out ]]; do
        out=$base-$n
        n=$((n + 1))
    done
fi
out=$(realpath -m "$out")

# The run to compare with: named, or the newest finished one.
case $against in
none) previous= ;;
"")
    previous=
    for dir in $(ls -td "$runs"/*/ 2>/dev/null); do
        dir=$(realpath "$dir")
        if [[ $dir != "$out" && -f $dir/summary.txt ]]; then
            previous=$dir
            break
        fi
    done
    ;;
*)
    previous=$against
    [[ -d $previous ]] || previous=$runs/$against
    if [[ ! -d $previous ]]; then
        echo "--against $against: no such run" >&2
        exit 2
    fi
    ;;
esac

quiet_or_stop

if [[ $build == yes ]]; then
    "$here/build.sh"
    [[ $traced == yes ]] && "$here/build.sh" trace

    # Our build's load fades over a minute or two.
    waited=0
    while [[ -n $(busy | grep '^  load') && $waited -lt $settle_s ]]; do
        echo "after the build: load $(cut -d' ' -f1 /proc/loadavg), waiting"
        sleep 10
        waited=$((waited + 10))
    done
    quiet_or_stop
fi

mkdir -p "$out"

{
    echo "cases: ${cases[*]}"
    echo "repeats: $count, each about $(expected "${cases[@]}")"
    cores=${PERF_CPUS-run.sh default}
    echo "cores: ${cores:-all}"
    git -C "$root" log -1 --format='commit: %h %s'
} | tee "$out/run.txt"

# Rounds of every case, not a case's repeats in a row: a drift in the
# machine spreads over all cases.
for ((i = 1; i <= count; i++)); do
    echo "== repeat $i of $count, load $(cut -d' ' -f1 /proc/loadavg)"
    echo "$i $(cat /proc/loadavg)" >>"$out/load.txt"
    "$here/run.sh" "$out/$i" "${cases[@]}" >"$out/$i.log" 2>&1 || echo "repeat $i: run.sh failed ($out/$i.log)" >&2
    grep -E 'failed' "$out/$i.log" >&2 || true
done
echo "end $(cat /proc/loadavg)" >>"$out/load.txt"

"$here/summary.py" "$out" >"$out/summary.txt"
if [[ -n $previous ]]; then
    "$here/summary.py" "$previous" "$out" | tee "$out/compare.txt"
else
    cat "$out/summary.txt"
fi
echo "run: $out"
