#!/usr/bin/env bash
# One command for a person, an agent and CI (.github/workflows/ci.yml runs `--full`).
# Exit 0: every check below passed. Exit 1: the last line names those that did not,
# and each one's full output is in .verify/<name>.log.
set -uo pipefail
cd "$(dirname "$0")/.."
mkdir -p .verify
rm -f .verify/*.log

# CI denies warnings for every crate; do the same locally unless told otherwise.
: "${RUSTFLAGS:=-D warnings}"
export RUSTFLAGS

failed=()
run() { # run <name> <command> [args...]
  local name=$1
  shift
  local log=".verify/$name.log"
  printf '== %s\n' "$name"
  if "$@" >"$log" 2>&1; then
    printf '   ok\n'
  else
    printf '   FAILED: see %s\n' "$log"
    tail -n 40 "$log"
    failed+=("$name")
  fi
}

run fmt    cargo fmt --all --check
run clippy cargo clippy --workspace --exclude web --features demo/native --all-targets
run test   cargo test --workspace --exclude web --exclude noa-gpui-render-wgpu --features demo/native
# Renders offscreen and reads pixels back: needs a GPU adapter (CI uses Mesa lavapipe).
run gpu    cargo test -p noa-gpui-render-wgpu --features native

if [ "${1:-}" = "--full" ]; then
  run clippy-webgpu cargo clippy -p web -p demo --target wasm32-unknown-unknown --features web/webgpu
  run clippy-webgl2 cargo clippy -p web -p demo --target wasm32-unknown-unknown --features web/webgl2
  run bench env BENCH_ROWS=2000 cargo bench -p noa-gpui-core --bench ui_tree
  # Each fails when any frame takes longer than 16 ms.
  run bench-table   cargo bench -p noa-gpui-core --bench table
  run bench-desktop cargo bench -p noa-gpui-core --bench desktop
fi

echo "not covered: release builds per OS and the wasm-release builds (CI matrix jobs only), the MSRV (rust-version 1.90), running the web build in a browser, secret scanning"
if [ "${#failed[@]}" -gt 0 ]; then
  printf 'verify: FAILED: %s\n' "${failed[*]}"
  exit 1
fi
echo "verify: ok"
