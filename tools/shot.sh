#!/usr/bin/env bash
#
# One openregent screenshot in a hidden display: no window on the desktop.
#
#   tools/shot.sh <openregent binary> <out.png> [openregent args...]
#   tools/shot.sh target/debug/openregent local/shots/x.png --menu main
#
#  cage (wlroots, headless backend: no output on screen)
#    └── wlr-randr: the output HEADLESS-1 to the shot's size (1280 x 720
#        otherwise), then openregent on Wayland, --screenshot <out.png>
#
# Pixel-identical to a shot in a desktop X11 session (Barren Waste day 6).
#
# Scratch data, cache and config under $SHOT_HOME (default local/shot-scratch),
# so the user's play.input, log, settings and caches are untouched. The log
# is <out.png>.log.

set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
bin=$(realpath "$1")
out=$(realpath "$2")
shift 2

# .env says where the game files are, filling only what the environment
# has not set.
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
    done < "$env_file"
fi

home=${SHOT_HOME:-$root/local/shot-scratch}
mkdir -p "$home"
export XDG_DATA_HOME=$home/data XDG_CACHE_HOME=$home/cache XDG_CONFIG_HOME=$home/config

# A dev build links Bevy dynamically: its library is in deps/, or beside
# the binary (cargo names it by version: hashed there, plain here).
toolchain=$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/lib
export LD_LIBRARY_PATH=$(dirname "$bin")/deps:$(dirname "$bin"):$toolchain

export WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1
unset DISPLAY

# The output as large as the window the shot asks: --size, else the game's
# default.
size=1920x1200
args=("$@")
for i in "${!args[@]}"; do
    [[ ${args[$i]} == --size ]] && size=${args[$((i + 1))]}
done

timeout 300 cage -d -- sh -c \
    'wlr-randr --output HEADLESS-1 --custom-mode "$0" && exec "$@"' \
    "$size" "$bin" "$@" --screenshot "$out" > "$out.log" 2>&1
