#!/usr/bin/env bash
#
# The fixed perf runs (tools/perf/cases), each into <out-dir>/<case>/,
# then one summary table: <out-dir>/summary.txt (tools/perf/summary.py).
#
#   tools/perf/build.sh [trace|dist]          release, frame pointers
#   tools/perf/run.sh <out-dir> [case...]     every case, or those named
#   tools/perf/summary.py <before> <after>    two runs side by side
#   tools/perf/quiet.sh [case|group...]       repeated, kept, compared
#
#  per case: perf stat (instructions, cycles, task clock)
#              └── /usr/bin/time (wall, CPU, peak RSS)
#                    └── env <the case's NAME=value words> taskset -c <cores>
#                          setarch -R: no address randomising, one layout a run
#                          └── openregent, or tools/shot.sh openregent (hidden display)
#            MAJ_PERF=1: the game's own timers (openregent/src/perf/mod.rs) in the log
#            XDG data, config and cache under <out-dir>/<case>/: nothing
#            of the user's touched
#
# Cores: the case's CPUS=, else $PERF_CPUS, else the performance cores
# (cores.sh); CPUS=all or an empty PERF_CPUS pins nothing, CPUS=one pins
# one performance core (cores.sh one), CPUS=eff<N> the last N efficiency
# cores (cores.sh eff N): the case fails on a machine with fewer.
#
# perf stat needs the counters (perf_event_paranoid at most 2); without
# them a case runs under /usr/bin/time alone, with a warning.
#
# The binary is $PERF_BIN, else build.sh's; BUILD=trace cases' is
# $PERF_TRACE_BIN, else `build.sh trace`'s; BUILD=dist cases' is
# $PERF_DIST_BIN, else `build.sh dist`'s. The others' Bevy links
# dynamically: LD_LIBRARY_PATH takes their deps/ and the toolchain's lib/
# (the dist build is static: it needs none). Keep
# <out-dir> out of tracked folders: local/ (quiet.sh's default) is ignored.
#
# Busy machine (busy.sh: cargo, rustc, clippy-driver or openregent
# running, or a 1-minute load above its max_load) at start: a warning, and
# <out-dir>/disturbed; the summary marks the cases DISTURBED. PERF_GUARD=off
# skips the check (quiet.sh, which has its own).

set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
here=$root/tools/perf
bin=$(realpath "${PERF_BIN:-$root/target/perf/release/openregent}")
trace_bin=$(realpath -m "${PERF_TRACE_BIN:-$root/target/perf-trace/release/openregent}")
dist_bin=$(realpath -m "${PERF_DIST_BIN:-$root/target/perf-dist/dist/openregent}")

# This machine's performance cores (cores.sh; M1 Pro: 2-9). Unpinned,
# a run's timers turn bimodal as threads land on either type.
source "$here/cores.sh"
cpus=${PERF_CPUS-$(perf_cores)}

if [[ $# -lt 1 ]]; then
    echo "usage: $0 <out-dir> [case...]" >&2
    exit 2
fi

out=$(realpath -m "$1")
shift
mkdir -p "$out"
rm -f "$out/disturbed"

case $out/ in
"$root"/*)
    echo "$out: inside the repository; give a scratch directory" >&2
    exit 2
    ;;
esac

# .env's game paths, as tools/shot.sh reads them.
env_file="$root/.env"
if [[ -f $env_file ]]; then
    while IFS='=' read -r key value; do
        [[ $key == \#* || -z $key ]] && continue
        key=${key// /}
        value=${value%\"}
        value=${value#\"}
        value=${value/#\~/$HOME}
        [[ $value != /* ]] && value="$root/$value"
        [[ -z ${!key:-} ]] && export "$key=$value"
    done <"$env_file"
fi

toolchain=$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/lib
export LD_LIBRARY_PATH=$(dirname "$bin")/deps:$toolchain
export MAJ_PERF=1 RUST_LOG=warn,maj_perf=info

# Mesa's software Vulkan (lavapipe), for the cases' VK_ICD_FILENAMES.
lavapipe=/usr/share/vulkan/icd.d/lvp_icd.$(uname -m).json

# The integrated GPU's name, for the cases' WGPU_ADAPTER_NAME: $PERF_IGPU,
# else the last parenthesis of the first integrated Vulkan adapter's
# deviceName ("... (RADV RAPHAEL_MENDOCINO)" -> "RADV RAPHAEL_MENDOCINO");
# empty when there is none.
igpu=${PERF_IGPU-$(vulkaninfo --summary 2>/dev/null |
    awk '/deviceType/ { integrated = /INTEGRATED_GPU/ } /deviceName/ && integrated { print; exit }' |
    sed -n 's/.*(\([^()]*\))[^()]*$/\1/p' || true)}

# The LAN case's port.
port=47613

# `perf stat` and `time` about a command, into $dir.
counted=no
if perf stat -e instructions:u -o /dev/null -- true 2>/dev/null; then
    counted=yes
else
    echo "perf stat: no counters here; wall and CPU time only" >&2
fi

measured() {
    # ctx: involuntary context switches, faults: major page faults (swap,
    # cold files); either high marks a repeat another load disturbed.
    local -a timed=(/usr/bin/time -f "wall=%e user=%U sys=%S rss_kb=%M ctx=%c faults=%F" -o "$dir/time.txt" "$@")

    if [[ $counted == yes ]]; then
        LC_ALL=C perf stat -x, -e instructions:u,cycles:u,task-clock -o "$dir/stat.csv" -- "${timed[@]}"
    else
        "${timed[@]}"
    fi
}

# A taskset list's cores, one a line: "0-2,7" -> 0 1 2 7.
expand() {
    local part
    for part in ${1//,/ }; do
        seq "${part%-*}" "${part#*-}"
    done
}

# The SMT siblings of a taskset list's cores outside the list: on one
# physical core with the case, their load slows it. "7" -> 15 here.
siblings() {
    local cpu
    for cpu in $(expand "$1"); do
        expand "$(cat "/sys/devices/system/cpu/cpu$cpu/topology/thread_siblings_list")"
    done | sort -nu | grep -vxF -f <(expand "$1") || true
}

# Busy and total jiffies of the cores named, from /proc/stat: busy is
# all but idle and iowait.
jiffies() {
    local cpus=" $* "
    awk -v cpus="$cpus" '
        /^cpu[0-9]/ && index(cpus, " " substr($1, 4) " ") {
            for (i = 2; i <= NF; i++) total += $i
            busy += $2 + $3 + $4 + $7 + $8 + $9
        }
        END { print busy + 0, total + 0 }' /proc/stat
}

# XDG directories of one process, under $dir.
isolated() {
    export XDG_DATA_HOME=$dir/$1/data XDG_CONFIG_HOME=$dir/$1/config XDG_CACHE_HOME=$dir/$1/cache
    mkdir -p "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME"
}

run_case() {
    local name=$1 kind=$2
    shift 2
    dir=$out/$name
    rm -rf "$dir"
    mkdir -p "$dir"
    echo "$kind $*" >"$dir/args.txt"
    [[ -f $out/disturbed ]] && cp "$out/disturbed" "$dir/disturbed"
    echo "== $name: $kind $*"

    # Leading NAME=value words: the case's environment. CPUS=<list> pins
    # it to those cores (taskset), CPUS=all to none; BUILD=trace runs build.sh's timeline
    # build, its trace to <case>/trace.json; BUILD=dist the shipped profile's.
    local game=$bin
    local -a with=(env) pin=()
    [[ -n $cpus ]] && pin=(taskset -c "$cpus")
    while [[ ${1:-} =~ ^[A-Z_][A-Z0-9_]*= ]]; do
        case $1 in
        CPUS=all) pin=() ;;
        CPUS=one) pin=(taskset -c "$(perf_core)") ;;
        CPUS=eff*)
            local some
            some=$(eff_last "${1#CPUS=eff}")
            if [[ -z $some ]]; then
                echo "$name: fewer than ${1#CPUS=eff} efficiency cores" >&2
                return 1
            fi
            pin=(taskset -c "$some")
            ;;
        CPUS=*) pin=(taskset -c "${1#CPUS=}") ;;
        BUILD=trace)
            game=$trace_bin
            with+=("TRACE_CHROME=$dir/trace.json" "LD_LIBRARY_PATH=$(dirname "$game")/deps:$toolchain")
            ;;
        BUILD=dist) game=$dist_bin ;;
        WGPU_ADAPTER_NAME=)
            echo "$name: no integrated GPU here (PERF_IGPU names one)" >&2
            return 1
            ;;
        *) with+=("$1") ;;
        esac
        shift
    done
    with+=("${pin[@]}" setarch "$(uname -m)" -R)

    # The pinned cores' SMT siblings, watched while the case runs.
    local -a sibs=()
    [[ ${#pin[@]} -gt 0 ]] && mapfile -t sibs < <(siblings "${pin[2]}")
    local before
    before=$(jiffies "${sibs[@]}")

    case $kind in
    headless)
        (isolated run && cd "$root" && measured "${with[@]}" "$game" --headless "$@" >"$dir/out.log" 2>&1)
        ;;
    drawn | startup)
        (SHOT_HOME=$dir/shot measured "${with[@]}" "$root/tools/shot.sh" "$game" "$dir/shot.png" "$@" \
            >"$dir/shot.out" 2>&1)
        cp "$dir/shot.png.log" "$dir/out.log"
        ;;
    lan)
        (isolated host && cd "$root" &&
            measured "${with[@]}" "$game" --server "$port" --headless "$@" >"$dir/out.log" 2>&1) &
        local host=$!
        sleep 2
        local frames
        frames=$(grep -o -- '--frames [0-9]*' <<<"$*")
        (isolated guest && cd "$root" &&
            "${with[@]}" "$game" --client "127.0.0.1:$port" --headless $frames >"$dir/guest.log" 2>&1)
        wait "$host"
        ;;
    *)
        echo "$name: unknown kind $kind" >&2
        return 1
        ;;
    esac
    local status=$?

    # The siblings' busy share while the case ran, e.g. "cpus=15 busy=0.4%".
    if [[ ${#sibs[@]} -gt 0 ]]; then
        read -r busy total <<<"$(paste -d' ' <(echo "$before") <(jiffies "${sibs[@]}") |
            awk '{ print $3 - $1, $4 - $2 }')"
        awk -v cpus="$(paste -sd, <(printf '%s\n' "${sibs[@]}"))" -v b="$busy" -v t="$total" \
            'BEGIN { printf "cpus=%s busy=%.1f%%\n", cpus, t ? 100 * b / t : 0 }' >"$dir/siblings.txt"
    fi

    return $status
}

wanted=("$@")

# A busy machine skews every case: warn, and mark the run for the summary.
if [[ ${PERF_GUARD:-on} != off ]]; then
    source "$here/busy.sh"
    found=$(busy)
    if [[ -n $found ]]; then
        printf 'WARNING: machine busy; timings skewed (marked DISTURBED):\n%s\n' "$found" >&2
        echo "$found" >"$out/disturbed"
    fi
fi

while read -r name kind args; do
    [[ -z $name || $name == \#* ]] && continue
    if [[ ${#wanted[@]} -eq 0 || " ${wanted[*]} " == *" $name "* ]]; then
        # The line's $VARIABLES expanded, the rest split on spaces.
        eval "set -- $args"
        # A failed case (e.g. a replay's digest differs) is marked: the
        # summary flags it.
        if ! run_case "$name" "$kind" "$@"; then
            echo "$name: failed" >&2
            touch "$out/$name/failed"
        fi
    fi
done <"$here/cases"

git -C "$root" log -1 --format='%h %s' >"$out/commit.txt" 2>/dev/null || true
"$here/summary.py" "$out" | tee "$out/summary.txt"
