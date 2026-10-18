# ext-compile — `hnl compile` as a dynamic `hnl` extension

The compiler front packaged as a `libhnl_ext_compile.{so,dylib,dll}` shared
library for the dumb `hnl` host (ADR-0149). The host discovers it via
`--ext-dir`, `$HANDL_EXT_DIR`, `~/.handl/ext/`, or a dev `target/debug/` next
to the `hnl` binary.

## Commands

```text
hnl compile <js|llvm|handl|term|dot|sexp> <file.hnl>
            [--output <path>] [--source-map] [--debug]
            [--format <fmt>] [--to <sub>] [--emit json|pretty]
```

All six targets are linked — `compile-js` from the `web` repo, `compile-llvm`
from `native`, `compile-handl` from `handl`, `compile-term` from `term`,
`compile-dot` from `dot`, `compile-sexp` from `sexp` — the same set the old
`hnl` binary enabled by default (ADR-0128 §0.3: the host's target set is its
dep list). The `js` target's `.hnl` import-graph walk (`js_module_graph`,
per-statement `stmt.import` spec collection resolved through
`[dependencies]` names then `ModuleLoader` bases) is carried over verbatim
from the old `host-cli` main.

Exit codes: `0` ok, `1` compile dispatch errors, `2` usage/read/parse errors.

## Dev

```sh
cargo build        # target/debug/libhnl_ext_compile.so
cargo test         # unit tests link the rlib statically
HANDL_EXT_DIR=$PWD/target/debug ../host-cli/target/debug/hnl compile js probe.hnl
```

Sibling-repo deps resolve from sibling checkouts via the `[patch]` tables
(ADR-0128 §Resolution); without them the `branch = "main"` git sources are used.
