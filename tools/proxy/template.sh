#!/usr/bin/env bash
# A Blender modelling template for one building's shadow proxy:
#   tools/proxy/template.sh <CODE> [--set inactive] [--out DIR] [--reference MODEL.glb]
# Needs Blender and nightly cargo; see make_template.py.
set -euo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
exec blender -b -P "$here/make_template.py" -- "$@"
