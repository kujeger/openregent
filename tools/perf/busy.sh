#!/usr/bin/env bash
#
# Whether the machine is busy, for timings: sourced by quiet.sh and
# run.sh. `busy` prints one line per cause, nothing when quiet.

# The 1-minute load average above which the machine is busy, whatever
# its core count: one 4-job cargo build lifts the load to about 4, an idle
# desktop stays near or below 1 (**Inferred**: neither measured alone).
# 2 is between: room for a browser, not a build.
max_load=2.0

# What makes the machine busy: builds and other games.
busy_names=(cargo rustc clippy-driver openregent)

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
