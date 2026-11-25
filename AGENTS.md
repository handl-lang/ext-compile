# ext-compile

`hnl-ext-compile` — `hnl compile` as a dynamic `hnl` extension: the
compiler front packaged as a `libhnl_ext_compile.{so,dylib,dll}` cdylib
the dumb `hnl` host discovers at runtime.

## Layout

| path | role |
| --- | --- |
| `src/` | extension ABI exports + `compile` command wiring |
| `xtask/` | vendored gate crate (`fmt`, `clippy`, `test`, `doc`, `gate`) |
| `workflows/gate.yaml` | per-push gate — `handl-gates` rust campaign via bupha |

## Contract

- Commands: `hnl compile <js|llvm|handl|term|dot|sexp> <file.hnl>`
  `[--output] [--source-map] [--debug] [--format] [--to] [--emit]`.
- All six targets are linked — `compile-js` (web), `compile-llvm`
  (native), `compile-handl` (handl), `compile-term` (term),
  `compile-dot` (dot), `compile-sexp` (sexp): the host's target set is
  its dep list.
- The `js` target's `.hnl` import-graph walk (`js_module_graph`) is
  carried over verbatim from the old `host-cli` main.
- Exit codes: `0` ok, `1` compile dispatch errors, `2` usage/read/parse.

## Verification

```bash
cargo run --manifest-path xtask/Cargo.toml -- gate   # fmt → clippy → test → doc
```

<!-- myco:agent-awareness -->

## Agent awareness

- Session start / any work discussion: run `myco status --prompt`
  before proposing next steps — know the board first.
- User describes new work → offer to capture it (`myco todo add`,
  `issue new`, `proposal new`). Don't wait to be asked.
- Picking up a task: `myco resume <ID>` is the briefing.
- Wrapping up: `myco handoff <ID>` anything resumable; keep statuses
  honest (claimed → in-progress → blocked/done).

<!-- /myco:agent-awareness -->
