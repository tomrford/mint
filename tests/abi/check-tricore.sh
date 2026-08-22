#!/usr/bin/env sh
# Validate the tricore-eabi-le ABI profile against a real TriCore compiler.
#
# The TriCore toolchains are proprietary and license-managed, so this check
# cannot run in CI alongside the nix-based ABI checks. Run it manually from
# an environment that provides the compiler and its license:
#
#   TRICORE_GCC=/path/to/tricore-gcc tests/abi/check-tricore.sh
#
# TRICORE_GCC defaults to `tricore-gcc` on PATH. Pass extra flags such as a
# core selection (e.g. -mtc162) via TRICORE_FLAGS.
#
# The script compiles the same generated headers as the Nix ABI probe
# (doc/examples/block.toml and tests/abi/pack.toml) plus the byte-only
# aggregate fixture tests/abi/bytes.toml, which proves the EABI minimum
# two-octet aggregate alignment. It also checks a hand-authored Neo schema
# against compiler sizeof, alignment and offset results.
set -eu

repo="$(cd "$(dirname "$0")/../.." && pwd)"
tricore_gcc="${TRICORE_GCC:-tricore-gcc}"
if ! command -v "$tricore_gcc" >/dev/null 2>&1; then
  echo "error: TriCore compiler not found: $tricore_gcc" >&2
  echo "Set TRICORE_GCC to a licensed tricore-gcc." >&2
  exit 1
fi
for tool in sfw jq; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: required tool not found: $tool" >&2
    exit 1
  fi
done
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

for layout in doc/examples/block.toml tests/abi/pack.toml; do
  target="$workdir/$(basename "$layout")"
  sed 's/abi = "generic-le"/abi = "tricore-eabi-le"/' "$repo/$layout" > "$target"
  grep -q 'abi = "tricore-eabi-le"' "$target"
done

sfw cargo run --quiet --release --locked --manifest-path "$repo/Cargo.toml" -p mint-cli -- \
  header "$workdir/block.toml" -o "$workdir/mint_abi.h"
sfw cargo run --quiet --release --locked --manifest-path "$repo/Cargo.toml" -p mint-cli -- \
  header "$workdir/pack.toml" -o "$workdir/mint_pack.h"
sfw cargo run --quiet --release --locked --manifest-path "$repo/Cargo.toml" -p mint-cli -- \
  header "$repo/tests/abi/bytes.toml" -o "$workdir/mint_bytes.h"
sed 's/@mint abi generic-le/@mint abi tricore-eabi-le/' \
  "$repo/tests/abi/neo-schema.h" > "$workdir/mint_neo.h"
grep -q '@mint abi tricore-eabi-le' "$workdir/mint_neo.h"
sfw cargo run --quiet --release --locked --manifest-path "$repo/Cargo.toml" -p mint-neo -- \
  inspect "$workdir/mint_neo.h" --format json > "$workdir/mint_neo_layout.json"
jq -r -f "$repo/tests/abi/neo-expect.jq" \
  "$workdir/mint_neo_layout.json" > "$workdir/mint_neo_expect.h"

# shellcheck disable=SC2086 # TRICORE_FLAGS is intentionally word-split
"$tricore_gcc" -std=c11 -ffreestanding -Wall -Wextra -Werror -pedantic \
  ${TRICORE_FLAGS:-} -DMINT_TRICORE \
  -I"$workdir" -c "$repo/tests/abi/compiler-probe.c" -o "$workdir/probe.o"
# shellcheck disable=SC2086 # TRICORE_FLAGS is intentionally word-split
"$tricore_gcc" -std=c11 -ffreestanding -Wall -Wextra -Werror -pedantic \
  ${TRICORE_FLAGS:-} \
  -I"$workdir" -c "$repo/tests/abi/neo-compiler-probe.c" -o "$workdir/neo-probe.o"

commit="$(git -C "$repo" rev-parse --short HEAD)"
version="$("$tricore_gcc" -dumpversion)"
machine="$("$tricore_gcc" -dumpmachine)"
echo "tricore-eabi-le ABI check passed"
echo "compiler: $tricore_gcc ($version, $machine)"
echo "flags: -std=c11 -ffreestanding -Wall -Wextra -Werror -pedantic ${TRICORE_FLAGS:-} -DMINT_TRICORE"
echo "commit: $commit"
echo "layouts: doc/examples/block.toml tests/abi/pack.toml tests/abi/bytes.toml tests/abi/neo-schema.h"
