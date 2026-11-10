#!/usr/bin/env bash
#
# This machine's performance and efficiency cores, for taskset: sourced
# by run.sh and fps.sh, or run to print them.
#
#   tools/perf/cores.sh        all    the performance cores, e.g. 2,3,4,5,6,7,8,9
#                              one    the last of them, e.g. 9
#                              eff N  the last N efficiency cores, e.g. 0,1
#
# Found, in order:
#   Intel hybrid    /sys/devices/cpu_core/cpus (the P cores),
#                   /sys/devices/cpu_atom/cpus (the E cores)
#   ARM big.LITTLE  the cores of the highest cpu_capacity (M1 Pro: 2-9),
#                   of the lowest (M1 Pro: 0,1)
#   otherwise       none: every core alike, so `all` is empty (no pinning),
#                   `one` the last online core and `eff` empty
#
# The last, not the first: core 0 often takes the interrupts.
# $PERF_ONE overrides `one`: runs side by side each on their own core.

# The cores of the highest (`tail`) or lowest (`head`) cpu_capacity, a
# taskset list; empty when every core is alike or none says.
by_capacity() {
    local pick=$1
    local caps=(/sys/devices/system/cpu/cpu[0-9]*/cpu_capacity)

    if [[ -r ${caps[0]} && $(cat "${caps[@]}" | sort -u | wc -l) -gt 1 ]]; then
        local wanted
        wanted=$(cat "${caps[@]}" | sort -n | "$pick" -1)

        for cap in "${caps[@]}"; do
            [[ $(cat "$cap") == "$wanted" ]] && basename "$(dirname "$cap")" | tr -dc '0-9' && echo
        done | sort -n | paste -sd,
    fi
}

# The performance cores, a taskset list; empty when every core is alike.
perf_cores() {
    if [[ -r /sys/devices/cpu_core/cpus ]]; then
        cat /sys/devices/cpu_core/cpus
    else
        by_capacity tail
    fi
}

# The efficiency cores, a taskset list; empty when there are none.
eff_cores() {
    if [[ -r /sys/devices/cpu_atom/cpus ]]; then
        cat /sys/devices/cpu_atom/cpus
    else
        by_capacity head
    fi
}

# The last N efficiency cores, e.g. 2 of "16-23" -> 22,23; empty when
# there are fewer than N.
eff_last() {
    local n=$1
    local -a all
    mapfile -t all < <(eff_cores | tr ',' '\n' |
        awk -F- 'NF { for (i = $1; i <= (NF > 1 ? $2 : $1); i++) print i }')

    if [[ ${#all[@]} -ge $n ]]; then
        printf '%s\n' "${all[@]: -$n}" | paste -sd,
    fi
}

# One performance core: $PERF_ONE, else the last of perf_cores, else
# the last online.
perf_core() {
    if [[ -n ${PERF_ONE:-} ]]; then
        echo "$PERF_ONE"
        return
    fi

    local list
    list=$(perf_cores)
    list=${list:-$(cat /sys/devices/system/cpu/online)}

    # The last number of the list: "0-3,8-11" -> 11.
    echo "${list##*[,-]}"
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
    case ${1:-all} in
    all) perf_cores ;;
    one) perf_core ;;
    eff) eff_last "${2:-1}" ;;
    *)
        echo "usage: $0 [all|one|eff N]" >&2
        exit 2
        ;;
    esac
fi
