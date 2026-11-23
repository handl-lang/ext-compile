# ext-compile

`hnl-ext-compile` — `hnl compile` as a dynamic `hnl` extension.

An `ext-*` crate: the compiler front packaged as a
`libhnl_ext_compile.{so,dylib,dll}` shared library for the dumb `hnl` host.
The host discovers it via `--ext-dir`, `$HANDL_EXT_DIR`, `~/.handl/ext/`, or a
dev `target/debug/` next to the `hnl` binary.

## Commands

```text
hnl compile <js|llvm|handl|term|dot|sexp> <file.hnl>
            [--output <path>] [--source-map] [--debug]
            [--format <fmt>] [--to <sub>] [--emit json|pretty]
```

All six targets are linked — `compile-js` from the `web` repo, `compile-llvm`
from `native`, `compile-handl` from `handl`, `compile-term` from `term`,
`compile-dot` from `dot`, `compile-sexp` from `sexp` — the same set the old
`hnl` binary enabled by default: the host's target set is its dep list. The
`js` target's `.hnl` import-graph walk (`js_module_graph`, per-statement
`stmt.import` spec collection resolved through `[dependencies]` names then
`ModuleLoader` bases) is carried over verbatim from the old `host-cli` main.

Exit codes: `0` ok, `1` compile dispatch errors, `2` usage/read/parse errors.

## Development

```bash
cargo run --manifest-path xtask/Cargo.toml -- gate   # fmt → clippy → test → doc
```

`xtask/` provides the gate tasks (`fmt`, `clippy`, `test`, `doc`, `check`,
`gate`); `workflows/gate.yaml` is the per-push gate — the `handl-gates` bundle's rust campaign (fmt → clippy → test → doc), run by bupha/the fleet runner via `peregrin campaign run`. The crate
builds a `cdylib` + `rlib`; `cargo test` links the `rlib` statically.

```sh
cargo build        # target/debug/libhnl_ext_compile.so
HANDL_EXT_DIR=$PWD/target/debug ../host-cli/target/debug/hnl compile js probe.hnl
```

Deps are git deps on `branch = "main"`; locally the workspace-root
`.cargo/config.toml` patches them to sibling checkouts (dev-only, never
committed).

## License

MIT.
