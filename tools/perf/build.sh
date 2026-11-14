#!/usr/bin/env bash
#
# The openregent the perf runs measure: release, Bevy linked dynamically
# (`dev`), frame pointers kept (so
# `perf record --call-graph fp` works; dwarf call graphs truncate in deep
# GPL recursion), in its own target directory so other builds' flags do
# not rebuild it.
#
#   tools/perf/build.sh            -> target/perf/release/openregent
#   tools/perf/build.sh trace      -> target/perf-trace/release/openregent
#   tools/perf/build.sh dist       -> target/perf-dist/dist/openregent
#
# `trace`: the timeline build (openregent's `trace` feature, Bevy's spans):
# for `run.sh`'s trace cases only, as spans cost even unrecorded.
# `dist`: what ships: the `dist` profile (thin LTO, Bevy linked
# statically: no `dev` feature), still with frame pointers and symbols.
# PERF_TARGET overrides the target directory.

set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)

features=(--features dev)
profile=release
target=${PERF_TARGET:-$root/target/perf}
case ${1:-} in
"") ;;
trace)
    features=(--features dev,trace)
    target=${PERF_TARGET:-$root/target/perf-trace}
    ;;
dist)
    features=()
    profile=dist
    target=${PERF_TARGET:-$root/target/perf-dist}
    ;;
*)
    echo "usage: $0 [trace|dist]" >&2
    exit 2
    ;;
esac

export RUSTFLAGS="-C force-frame-pointers=yes"
# Release strips its binaries; `perf record` needs the symbols.
export CARGO_PROFILE_RELEASE_STRIP=false
export CARGO_PROFILE_DIST_STRIP=false
export CARGO_INCREMENTAL=0

cd "$root"
cargo build --profile "$profile" -p openregent "${features[@]}" --target-dir "$target"
echo "$target/$profile/openregent"
