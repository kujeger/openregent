# Common commands; `just` lists them.
#
# Test tiers, by the data they need:
#   test          none: a clean clone passes
#   test-install  the game's files, set in `.env` (see `.env.example`)
#
# `dev` links Bevy dynamically: fast rebuilds, not for handing out.

# List the recipes.
default:
    @just --list

# Typecheck everything.
check:
    cargo check --workspace --all-targets --features dev

# Build everything.
build:
    cargo build --workspace --features dev

# Build the game to hand out: Bevy linked statically.
release:
    cargo build --release -p openregent

# Build the stripped Windows exe to hand out, with mingw64-gcc.
release-windows:
    cargo build --release -p openregent --target x86_64-pc-windows-gnu

# Remove the build output.
clean:
    cargo clean

# Format the code.
fmt:
    cargo fmt

# Lint everything, the tests of every tier included.
lint:
    cargo clippy --workspace --all-targets --features dev,install

# Tests needing no game data.
test *args:
    cargo test --workspace --features dev {{args}}

# Also the tests needing the game's files.
test-install *args:
    cargo test --workspace --features dev,install {{args}}

# Addresses, labels and disassembly in tracked files: fails on any.
addresses:
    cargo test -p regent_core --test addresses -- --nocapture

# What CI runs: no game data.
ci:
    cargo fmt --check
    cargo clippy --workspace --all-targets
    cargo test --workspace

# Run the game.
run *args:
    cargo run -p openregent --features dev -- {{args}}

# Run the data reader.
cli *args:
    cargo run -p regent_cli -- {{args}}

# The recorded replays timed on a quiet machine, repeated, compared with the last run.
perf-replay *args:
    tools/perf/quiet.sh -n 5 replay {{args}}
