# AGENTS.md

These are the rules for agents working in this repository. For the structure and the design background, see [README.md](README.md) and [docs/architecture.md](docs/architecture.md); for where each feature's code and tests live, see [docs/feature-map.md](docs/feature-map.md).

## Structure

- Crate dependencies go one way: `noa-gpui-scene` ← `noa-gpui-text` / `noa-gpui-layout` ← `noa-gpui-core`, `noa-gpui-scene` / `noa-gpui-text` ← `noa-gpui-render-wgpu`, all of them ← `noa-gpui-platform` ← `apps/*`. Do not add dependencies in the reverse direction (see each `Cargo.toml`)
- `noa-gpui-core` and below (`noa-gpui-scene` / `noa-gpui-text` / `noa-gpui-layout` / `noa-gpui-core`) do not depend on the GPU or windows. Keep wgpu inside `noa-gpui-render-wgpu`, and winit and OS integration inside `noa-gpui-platform`
- On the Web, WebGL2 is the lowest common denominator. Do not make Compute Shaders or Storage Buffers mandatory in the rendering pipeline (docs/architecture.md §2)
- Put new Widgets in `crates/ui-core/src/widgets/`, add tests to the matching group in `crates/ui-core/tests/`, and add a catalog example to the matching group in `apps/demo/src/catalog/`
- Change only what the task needs. Put unrelated fixes in a separate pull request

## Coding

- Add tests for new behavior. For `noa-gpui-core` Widgets, send events to a `UiTree` and check the result (the same as the existing `crates/ui-core/tests/*.rs`)
- Do not delete existing tests or mark them `#[ignore]` without human agreement
- Fix warnings and clippy findings. Do not silence them with `#[allow(...)]` or by changing `RUSTFLAGS`
- Commit messages follow Conventional Commits (`feat(noa-gpui-core): ...`, `fix(demo): ...`). The scope is a crate name or `demo`

## Verification

- Run `./scripts/verify.sh` after a change. Run `./scripts/verify.sh --full` (same as CI) before a pull request
- Failed checks are reported by name. The full output is in `.verify/<check-name>.log`
- The `gpu` check (`crates/ui-render-wgpu/tests/render.rs`) needs a GPU adapter. In a sandbox without an adapter it always fails with `a GPU adapter is required for render tests`; in that case, ask a human to run it with `! ./scripts/verify.sh` and include the result in the report
- When the sandbox cannot write to `~/.cargo`, run with `CARGO_HOME="$TMPDIR/cargo-home"`
- The command that runs only one feature's tests is in the rightmost column of docs/feature-map.md
- Check screen changes in the demo: `cargo run -p desktop --release -- --scene=catalog` (or `--scene=minios`). It needs a GPU and a window, so when an agent cannot run it, say in the report that it was not verified

## Safety

- Do not print or commit secrets or credentials. Do not write them to files either
- Ask a human before changing the following: adding or updating dependencies (`[workspace.dependencies]` in `Cargo.toml` and `Cargo.lock`), changing `rust-version`, changing `scripts/verify.sh` and `.github/`

## Pull requests

- Fill in every section of `.github/pull_request_template.md`. In "Verification", write the commands run and their results
- Write what was not verified and what problems could occur
- Do not merge a pull request whose required checks are failing

## Discipline — what no command enforces

Neither `scripts/verify.sh` nor CI checks the following. Review is what enforces them. (The direction of crate dependencies, and that `noa-gpui-core` and below do not depend on the GPU or windows, are checked from `Cargo.toml` by `crates/ui-core/tests/architecture.rs`)

- Treating WebGL2 as the lowest common denominator (the wasm clippy may pass, but runtime limits are not checked)
- Adding tests for new behavior; not deleting tests or marking them `#[ignore]`
- Not silencing with `#[allow(...)]`
- Asking a human before changing dependencies, `rust-version`, or the verification gates (there is no CODEOWNERS)
- Not merging until the required checks pass
- The commit message format
