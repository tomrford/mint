#!/usr/bin/env sh
# Run manually with a licensed TriCore compiler; this check is outside Nix CI.
# TRICORE_GCC selects the compiler (default: tricore-gcc on PATH).
# TRICORE_FLAGS supplies extra flags, for example -mtc162.
set -eu

repo="$(cd "$(dirname "$0")/../.." && pwd)"
tricore_gcc="${TRICORE_GCC:-tricore-gcc}"
if ! command -v "$tricore_gcc" >/dev/null 2>&1; then
  echo "error: TriCore compiler not found: $tricore_gcc" >&2
  echo "Set TRICORE_GCC to a licensed tricore-gcc." >&2
  exit 1
fi
workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

for layout in doc/examples/block.toml tests/abi/pack.toml; do
  target="$workdir/$(basename "$layout")"
  sed 's/abi = "generic-le"/abi = "tricore-eabi-le"/' "$repo/$layout" > "$target"
  grep -q 'abi = "tricore-eabi-le"' "$target"
done

cargo run --quiet --release --manifest-path "$repo/Cargo.toml" -p mint-cli -- \
  header "$workdir/block.toml" -o "$workdir/mint_abi.h"
cargo run --quiet --release --manifest-path "$repo/Cargo.toml" -p mint-cli -- \
  header "$workdir/pack.toml" -o "$workdir/mint_pack.h"
cargo run --quiet --release --manifest-path "$repo/Cargo.toml" -p mint-cli -- \
  header "$repo/tests/abi/bytes.toml" -o "$workdir/mint_bytes.h"

# shellcheck disable=SC2086
"$tricore_gcc" -std=c11 -ffreestanding -Wall -Wextra -Werror -pedantic \
  ${TRICORE_FLAGS:-} -DMINT_TRICORE \
  -I"$workdir" -c "$repo/tests/abi/compiler-probe.c" -o "$workdir/probe.o"

commit="$(git -C "$repo" rev-parse --short HEAD)"
version="$("$tricore_gcc" -dumpversion)"
machine="$("$tricore_gcc" -dumpmachine)"
echo "tricore-eabi-le ABI check passed"
echo "compiler: $tricore_gcc ($version, $machine)"
echo "flags: -std=c11 -ffreestanding -Wall -Wextra -Werror -pedantic ${TRICORE_FLAGS:-} -DMINT_TRICORE"
echo "commit: $commit"
echo "layouts: doc/examples/block.toml tests/abi/pack.toml tests/abi/bytes.toml"
