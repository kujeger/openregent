#!/usr/bin/env bash
#
# One number to aim for: the frame rate of a busy game, drawn uncapped in a
# hidden display, printed as one line.
#
#   tools/fps.sh                          512-cell late game, 1920x1200
#   tools/fps.sh --size 2560x1440         another resolution
#   tools/fps.sh --small                  128-cell early game instead
#   tools/fps.sh -- --view-upscale fsr    more openregent arguments
#
#   tools/perf/build.sh (release, frame pointers), unless --no-build or $PERF_BIN
#     └── taskset <performance cores> tools/shot.sh openregent --frames N ...
#           MAJ_PERF=1 MAJ_PERF_UNCAPPED=1: no vsync, so the frame interval
#           is the frame's cost; the game logs it (openregent/src/perf)
#           └── "<game> <size>: <fps> fps (<mean> ms mean, <p99> ms p99)"
#
# The game is tools/perf/cases' g512-late (g128-early with --small), its
# GPU timers off (they cost about 3 ms a frame): map 7, the scripted
# player 5, warmed 14,000 game frames, the Palace selected; the shot's
# clock steps 1/60 s a drawn frame, so every run draws the same frames.
#
# The clock of this machine's cores drifts between runs: compare builds
# back to back, and on a quiet machine (no build or game running; the
# script warns). The log and last frame stay in the run directory,
# $PERF_RUNS (default local/perf-runs)/fps-<date>-<commit>.
#
# options:
#   --size <WxH>     window and frame size (default 1920x1200)
#   --frames <n>     drawn frames (default 1800). The game reports every 600
#                    (openregent's perf::WINDOW); the first 600 are left out,
#                    as art still loads then, so at least 1200; frames past
#                    the last full 600 are not reported
#   --small          the 128-cell early game
#   --no-build       use the release binary as it is
#   --cpus <list>    cores to pin to (default $PERF_CPUS, else the
#                    performance cores: tools/perf/cores.sh; "" pins nothing)

set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)

# Drawn frames a report of the game's timers covers (perf::WINDOW).
window=600

# The busy game and the small one: g512-late and g128-early.
large=(--map-seed 7 --side 512 --bot 5 --warm 14000 --running --select ABJ1)
small=(--map-seed 7 --side 128 --bot 5 --warm 600 --running --select ABJ1)

# What makes the machine busy, and the load above which it is
# (tools/perf/quiet.sh's).
busy_names=(cargo rustc clippy-driver openregent)
max_load=2.0

size=1920x1200
frames=1800
game=large
build=yes
source "$(dirname -- "${BASH_SOURCE[0]}")/perf/cores.sh"
cpus=${PERF_CPUS-$(perf_cores)}
extra=()

while [[ $# -gt 0 ]]; do
    case $1 in
    --size) size=$2; shift 2 ;;
    --frames) frames=$2; shift 2 ;;
    --small) game=small; shift ;;
    --no-build) build=no; shift ;;
    --cpus) cpus=$2; shift 2 ;;
    --) shift; extra=("$@"); break ;;
    -h | --help) sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option $1 (--help)" >&2; exit 2 ;;
    esac
done

if (( frames < 2 * window )); then
    echo "--frames $frames: at least $((2 * window)) (the first $window are left out)" >&2
    exit 2
fi

if [[ $game == large ]]; then
    args=("${large[@]}")
else
    args=("${small[@]}")
fi

# The binary: built now, or as it is.
if [[ -n ${PERF_BIN:-} ]]; then
    bin=$PERF_BIN
elif [[ $build == yes ]]; then
    bin=$("$root/tools/perf/build.sh" | tail -n 1)
else
    bin=$root/target/perf/release/openregent
fi

# A warning, not a refusal: one command should always give a number.
for name in "${busy_names[@]}"; do
    if pgrep -x "$name" >/dev/null; then
        echo "warning: $name is running; the number will be low" >&2
    fi
done

load=$(cut -d' ' -f1 /proc/loadavg)
if awk -v l="$load" -v m="$max_load" 'BEGIN { exit !(l > m) }'; then
    echo "warning: load $load is above $max_load; the number will be low" >&2
fi

commit=$(git -C "$root" rev-parse --short HEAD)
git -C "$root" diff --quiet HEAD -- crates || commit=$commit-dirty
runs=${PERF_RUNS:-$root/local/perf-runs}
out=$runs/fps-$(date +%Y%m%d-%H%M%S)-$commit
mkdir -p "$out"

pin=()
[[ -n $cpus ]] && pin=(taskset -c "$cpus")

SHOT_HOME=$out/shot MAJ_PERF=1 MAJ_PERF_UNCAPPED=1 RUST_LOG=warn,maj_perf=info \
    "${pin[@]}" "$root/tools/shot.sh" "$bin" "$out/frame.png" \
    --size "$size" --frames "$frames" "${args[@]}" "${extra[@]}" >"$out/shot.out" 2>&1 || {
    echo "the run failed; see $out/shot.out and $out/frame.png.log" >&2
    exit 1
}

# The game's frame interval, a line a window: "maj_perf: drawn interval n
# N mean M p99 P ...". All windows but the first: their frames' mean, and
# the worst window's p99.
summary=$(sed 's/\x1b\[[0-9;]*m//g' "$out/frame.png.log" |
    awk '/maj_perf: drawn interval n / {
            for (i = 1; i < NF; i++) {
                if ($i == "n") n = $(i + 1)
                if ($i == "mean") mean = $(i + 1)
                if ($i == "p99") p99 = $(i + 1)
            }
            windows++
            if (windows > 1) { frames += n; total += n * mean; if (p99 > worst) worst = p99 }
        }
        END {
            if (frames > 0) printf "%.0f %.2f %.2f %d\n", 1000 * frames / total, total / frames, worst, frames
        }')

if [[ -z $summary ]]; then
    echo "no frame intervals in $out/frame.png.log" >&2
    exit 1
fi

read -r fps mean p99 n <<<"$summary"

echo "$game $size $commit: $fps fps ($mean ms mean, $p99 ms p99, $n frames)" | tee "$out/fps.txt"
