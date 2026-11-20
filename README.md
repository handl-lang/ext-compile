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
cargo xtask gate        # fmt --check → clippy -D warnings → test → doc
```

`cargo xtask` wraps the repo's `xtask/` crate (`fmt`, `clippy`, `test`, `doc`,
`check`, `gate`); `ci.sh` is the gate entry. The crate builds a `cdylib` +
`rlib`; `cargo test` links the `rlib` statically.

```sh
cargo build        # target/debug/libhnl_ext_compile.so
HANDL_EXT_DIR=$PWD/target/debug ../host-cli/target/debug/hnl compile js probe.hnl
```

Sibling-repo deps resolve from sibling checkouts via the `[patch]` tables;
without them the `branch = "main"` git sources are used.

## License

MIT OR Apache-2.0.
