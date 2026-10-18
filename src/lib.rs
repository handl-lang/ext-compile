//! `hnl-ext-compile` — the `compile` extension for `hnl` (ADR-0149).
//!
//! The compile pipeline lives in `handl-plugin-ext-compile`
//! (`CompileFileRequest`/`CompileDispatcher`) driving the six target
//! modules linked from their own repos — `compile-js` (`web`),
//! `compile-llvm` (`native`), `compile-handl` (`handl`), `compile-term`
//! (`term`), `compile-dot` (`dot`), `compile-sexp` (`sexp`) — all six on,
//! matching the old `hnl` binary's default feature set (ADR-0128 §0.3).
//! This crate is the *packaging*: it exports the ADR-0149 ABI
//! (`handl_ext_manifest` / `handl_ext_dispatch` / `handl_ext_free`) and
//! implements `hnl compile <js|llvm|handl|term|dot|sexp> <file>
//! [--output|--source-map|--debug|--format|--to|--emit]` — byte-for-byte
//! the semantics the built-in `compile` arm had before the host went dumb,
//! including the `.hnl` import-graph walk for the `js` target
//! (`js_module_graph`, carried over from the old `host-cli` main.rs).

use std::path::{Path, PathBuf};
use std::rc::Rc;

use handl_ext::{ExtRequest, ExtResponse, ExtensionManifest};
use handl_kit_arrow_lower::lower_ast;
use handl_language::{AstRef, Language, Source};
use handl_plugin_engine_lex_pattern::DataLexerEngine;
use handl_plugin_engine_parse::{ParserEngine, ParserRequest};
use handl_plugin_engine_parse_pratt::PrattEngine;
use handl_runtime::{EngineRegistry, Runtime, RuntimeConfig};
use serde_json::Value;

handl_ext::export!(
    ExtensionManifest::new("compile")
        .version(env!("CARGO_PKG_VERSION"))
        .command(
            "compile",
            "compile <js|llvm|handl|term|dot|sexp> <file> [--output <path>] [--source-map] [--debug] [--format <fmt>] [--to <sub>] [--emit json|pretty]",
        ),
    dispatch
);

/// ABI dispatch: only the `compile` command is declared.
fn dispatch(cmd: &str, req: &ExtRequest) -> ExtResponse {
    match cmd {
        "compile" => run(&req.args, Path::new(&req.cwd)),
        other => ExtResponse::error(2, format!("compile: unknown command `{other}`\n")),
    }
}

// ---------------------------------------------------------------------------
// `compile <target> <file> [--output <path>] [--source-map] [--debug] [--format ...] [--to hughes] [--emit json|pretty]`
// ---------------------------------------------------------------------------

/// Output accumulator: the extension renders one `ExtResponse` instead of
/// printing, so stdout/stderr collect here and the code is returned.
#[derive(Default)]
struct Out {
    stdout: String,
    stderr: String,
}

impl Out {
    fn err(&mut self, msg: impl std::fmt::Display) {
        self.stderr.push_str(&format!("{msg}\n"));
    }
}

fn run(args: &[String], cwd: &Path) -> ExtResponse {
    let mut out = Out::default();
    let code = compile_command(args, cwd, &mut out);
    ExtResponse {
        code,
        stdout: out.stdout,
        stderr: out.stderr,
    }
}

fn compile_command(args: &[String], cwd: &Path, out: &mut Out) -> i32 {
    let request = match handl_plugin_ext_compile::CompileFileRequest::from_args(args) {
        Ok(r) => r,
        Err(e) => {
            out.err(format!("compile: {e}"));
            return 2;
        }
    };

    let disk = resolve(cwd, &request.path);
    let src = match std::fs::read_to_string(&disk) {
        Ok(s) => s,
        Err(e) => {
            out.err(format!(
                "compile: failed to read {}: {e}",
                request.path.display()
            ));
            return 2;
        }
    };

    let data_kit = match handl_kit_data_canonical::assemble() {
        Ok(k) => k,
        Err(e) => {
            out.err(format!("compile: failed to assemble canonical language: {e}"));
            return 2;
        }
    };

    let mut registry = EngineRegistry::new();
    registry.register_lex("data", Box::new(DataLexerEngine::new()));
    registry.register_parse("pratt", PrattEngine::new());

    let mut runtime_config = RuntimeConfig::new().with_lex("data").with_parse("pratt");
    runtime_config.eval.clear();
    runtime_config.typecheck.clear();

    let lang = data_kit.lang().clone();
    let runtime = match Runtime::from_registry(lang, &Rc::new(registry), &runtime_config) {
        Ok(r) => r,
        Err(e) => {
            out.err(format!("compile: failed to build runtime: {e}"));
            return 2;
        }
    };

    let source = Source::new(src.clone());
    let tokens = match runtime.lex(&source) {
        Ok(t) => t,
        Err(e) => {
            out.err(format!("compile: lex error: {e}"));
            return 2;
        }
    };
    let ast = match runtime.parse(&tokens) {
        Ok(a) => a,
        Err(e) => {
            out.err(format!("compile: parse error: {e}"));
            return 2;
        }
    };

    if ast.is_empty() {
        out.err("compile: empty AST");
        return 2;
    }

    let root = AstRef::new(ast.len() - 1);
    let term = match lower_ast(&ast, runtime.lang(), root) {
        Ok(t) => t,
        Err(e) => {
            out.err(format!("compile: lower error: {e}"));
            return 2;
        }
    };

    // The compile-target set is this extension's dep list (ADR-0128 §0.3):
    // each arm is compiled only when the target's repo module is linked in.
    // Built before `CompilerInput` so the js arm can resolve the entry
    // file's import graph from `src` (`src` moves into the input below).
    let config: Box<dyn handl_plugin_compile::CompilerConfig> = match request.target.as_str() {
        "js" => build_js_config(&request, runtime.lang(), &src, out),
        "llvm" => build_llvm_config(&request),
        "handl" => build_handl_config(&request),
        "term" => build_term_config(&request),
        "dot" => build_dot_config(&request),
        "sexp" => build_sexp_config(&request),
        other => {
            out.err(format!("compile: unsupported target `{other}`"));
            return 2;
        }
    };

    let report = handl_plugin_engine_typecheck::TypecheckReport::default();
    let input = handl_plugin_compile::CompilerInput::new(
        src,
        ast,
        root,
        runtime.lang().clone(),
        term,
        report,
    );

    let dispatcher = handl_plugin_ext_compile::CompileDispatcher::new();
    let output = match dispatcher.compile(&request.target, input, config.as_ref()) {
        Ok(o) => o,
        Err(e) => {
            out.err(format!("compile: {e}"));
            return 1;
        }
    };

    if let Some(path) = &request.output {
        let out_path = resolve(cwd, path);
        if let Err(e) = std::fs::write(&out_path, output.primary_output().unwrap_or_default()) {
            out.err(format!("compile: failed to write {}: {e}", path.display()));
            return 2;
        }
    } else {
        out.stdout
            .push_str(&String::from_utf8_lossy(output.primary_output().unwrap_or_default()));
    }

    0
}

fn build_js_config(
    req: &handl_plugin_ext_compile::CompileFileRequest,
    lang: &Language,
    src: &str,
    out: &mut Out,
) -> Box<dyn handl_plugin_compile::CompilerConfig> {
    use handl_plugin_compile_js::{
        CommonJs, Deno, Esm, Iife, Inline, JsConfig, JsOutputFormat, JsTarget,
    };

    let target: Box<dyn JsTarget> = Box::new(Deno);
    let format: Box<dyn JsOutputFormat> = match req.format.as_deref().unwrap_or("esm") {
        "esm" => Box::new(Esm),
        "commonjs" => Box::new(CommonJs),
        "iife" => Box::new(Iife),
        "inline" => Box::new(Inline),
        other => {
            // Unsupported format is treated like the default `esm`; the
            // dispatcher will still run and the output can be inspected.
            out.err(format!("compile: unknown js format `{other}`, defaulting to esm"));
            Box::new(Esm)
        }
    };

    let (modules, entry_imports) = js_module_graph(&req.path, src, lang);

    Box::new(JsConfig {
        target,
        format,
        debug: req.debug,
        no_inline_foreign: false,
        source_map: req.source_map,
        modules,
        entry_imports,
    })
}

/// The `.hnl` import graph for `hnl compile js` (ADR-0128 Phase 5):
/// walk the entry file's top-level `stmt.import` specs — the same
/// statement granularity compile-js's module emit sees — resolve each
/// with [`resolve_spec_path`] (`[dependencies]` name first, then the
/// `ModuleLoader` bases: the importing file's directory, the module
/// root, the working directory), and recurse into each resolved file.
///
/// The result rides to the compiler in `JsConfig::modules` +
/// `entry_imports`; the compiler itself never touches the filesystem.
/// Specs that resolve to no file (host modules like `js`/`str`, or
/// plain missing paths) produce no edge and keep their
/// `__handl_rt`/ES-import emit.
fn js_module_graph(
    entry: &Path,
    entry_src: &str,
    lang: &Language,
) -> (Vec<handl_plugin_compile_js::JsModule>, Vec<(String, String)>) {
    let deps = handl_platform::discover_dependencies(entry).unwrap_or_default();
    let module_root = handl_platform::discover_manifest_dir(entry);
    let mut seen = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::new();

    // The entry file is marked seen but never queued: a dep importing
    // it back is a circular import (the compiler errors on the missing
    // edge), not a second module.
    if let Ok(canon) = std::fs::canonicalize(entry) {
        seen.insert(canon);
    }

    let mut entry_imports = Vec::new();
    let entry_bases = spec_resolution_bases(entry, module_root.as_deref());
    for spec in module_import_specs(lang, entry_src) {
        if let Some(path) = resolve_spec_path(&spec, &entry_bases, &deps) {
            let canon = std::fs::canonicalize(&path).unwrap_or(path);
            entry_imports.push((spec, canon.to_string_lossy().into_owned()));
            if seen.insert(canon.clone()) {
                queue.push_back(canon);
            }
        }
    }

    let mut modules = Vec::new();
    while let Some(path) = queue.pop_front() {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        // Nested imports resolve against the dep file's own directory
        // first — `ModuleLoader`'s `dir_stack` order.
        let bases = spec_resolution_bases(&path, module_root.as_deref());
        let mut imports = Vec::new();
        for spec in module_import_specs(lang, &src) {
            if let Some(p) = resolve_spec_path(&spec, &bases, &deps) {
                let canon = std::fs::canonicalize(&p).unwrap_or(p);
                imports.push((spec, canon.to_string_lossy().into_owned()));
                if seen.insert(canon.clone()) {
                    queue.push_back(canon);
                }
            }
        }
        modules.push(handl_plugin_compile_js::JsModule {
            path: path.to_string_lossy().into_owned(),
            source: src,
            imports,
        });
    }
    (modules, entry_imports)
}

/// Top-level import specifiers in `src`: `stmt.import` nodes' `path`
/// payload (`from "<spec>" …`) or string child (bare `import "<spec>"`),
/// at the same per-statement granularity compile-js's module emit uses.
/// Best-effort: a chunk that fails to lex/parse yields no spec — the
/// compiler's own statement pass reports the real error.
fn module_import_specs(lang: &Language, src: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for chunk in handl_plugin_ext_diag::split_statements(src) {
        let Ok(tokens) = handl_plugin_engine_lex_pattern::lex(lang, &chunk.text) else {
            continue;
        };
        if tokens.as_slice().is_empty() {
            continue;
        }
        let req = ParserRequest::new(tokens);
        let Ok(ast) = PrattEngine::new().parse(lang, &req) else {
            continue;
        };
        let Some(node) = ast.get(AstRef::new(ast.len() - 1)) else {
            continue;
        };
        if node.kind != "stmt.import" {
            continue;
        }
        let spec = node
            .payload
            .get("path")
            .and_then(Value::as_str)
            .map(String::from)
            .or_else(|| {
                node.payload
                    .get("children")
                    .and_then(|c| c.as_array())
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_u64())
                    .and_then(|i| ast.get(AstRef::new(i as usize)))
                    .and_then(|n| n.payload.as_str().map(String::from))
            });
        if let Some(spec) = spec {
            specs.push(spec);
        }
    }
    specs
}

fn build_llvm_config(
    req: &handl_plugin_ext_compile::CompileFileRequest,
) -> Box<dyn handl_plugin_compile::CompilerConfig> {
    use handl_plugin_compile_llvm::{LlvmConfig, X86_64};

    let mut cfg = LlvmConfig::new(Box::new(X86_64));
    cfg.debug = req.debug;
    cfg.emit_object = req.format.as_deref().map(|f| f == "o").unwrap_or(false);
    Box::new(cfg)
}

fn build_handl_config(
    req: &handl_plugin_ext_compile::CompileFileRequest,
) -> Box<dyn handl_plugin_compile::CompilerConfig> {
    use handl_plugin_compile_handl::HandlConfig;

    let to = req.to.as_deref().unwrap_or("hughes").to_string();
    Box::new(HandlConfig::new(to))
}

fn build_term_config(
    req: &handl_plugin_ext_compile::CompileFileRequest,
) -> Box<dyn handl_plugin_compile::CompilerConfig> {
    use handl_plugin_compile_term::TermConfig;

    let emit = req.emit.as_deref().unwrap_or("json").to_string();
    Box::new(TermConfig::new(emit))
}

fn build_dot_config(
    _req: &handl_plugin_ext_compile::CompileFileRequest,
) -> Box<dyn handl_plugin_compile::CompilerConfig> {
    use handl_plugin_compile_dot::DotConfig;

    Box::new(DotConfig::new())
}

fn build_sexp_config(
    _req: &handl_plugin_ext_compile::CompileFileRequest,
) -> Box<dyn handl_plugin_compile::CompilerConfig> {
    use handl_plugin_compile_sexp::SexpConfig;

    Box::new(SexpConfig::new())
}

// ---------------------------------------------------------------------------
// Import-specifier resolution (shared with `run`/`typecheck`/`lint`)
// ---------------------------------------------------------------------------

/// Resolution bases for `import "<spec>"` specifiers in a file at `path`,
/// mirroring `ModuleLoader` order: the file's directory, the module root,
/// then the process working directory.
fn spec_resolution_bases(path: &Path, module_root: Option<&Path>) -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = Vec::new();
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => bases.push(dir.to_path_buf()),
        _ => bases.push(PathBuf::from(".")),
    }
    if let Some(root) = module_root {
        if !bases.iter().any(|b| b.as_path() == root) {
            bases.push(root.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if !bases.iter().any(|b| *b == cwd) {
            bases.push(cwd);
        }
    }
    bases
}

/// Resolve a `[dependencies]` specifier: when the first `/`-separated
/// segment of `spec` names a declared dependency, the remainder resolves
/// inside that package's directory (`dep/lib/file` →
/// `<deps["dep"]>/lib/file`; a bare `dep` name resolves the package dir's
/// `index.hnl`/`<name>.hnl` main file). Returns `None` for non-dep specs.
fn resolve_dep_spec_path(
    spec: &str,
    deps: &std::collections::HashMap<String, PathBuf>,
) -> Option<PathBuf> {
    let (first, rest) = match spec.split_once('/') {
        Some((f, r)) => (f, Some(r)),
        None => (spec, None),
    };
    let dep_dir = deps.get(first)?;
    let joined = match rest {
        Some(r) if !r.is_empty() => dep_dir.join(r),
        _ => dep_dir.clone(),
    };
    file_candidate(&joined, spec)
}

/// Resolve `<spec>` to the file `ModuleLoader` would load: dependency-name
/// resolution first, then `bases` (same order as [`spec_resolves_to_file`]).
fn resolve_spec_path(
    spec: &str,
    bases: &[PathBuf],
    deps: &std::collections::HashMap<String, PathBuf>,
) -> Option<PathBuf> {
    if Path::new(spec).is_absolute() {
        return file_candidate(Path::new(spec), spec);
    }
    resolve_dep_spec_path(spec, deps)
        .or_else(|| bases.iter().find_map(|base| file_candidate(&base.join(spec), spec)))
}

/// Whether `<spec>` resolves to an existing file — first as a
/// `.handl.toml` `[dependencies]` specifier (a dep name shadows a
/// same-named relative directory, mirroring `ModuleLoader::resolve_path`),
/// then under any of `bases` (checked in order — mirroring `ModuleLoader`
/// relative resolution: importing module's directory, module root, working
/// directory).
///
/// A directory is treated as a package if it contains an `index.hnl` or
/// a `<dir>/<name>.hnl` main file; a bare name without `.hnl` is tried
/// with `.hnl` appended.
#[allow(dead_code)]
fn spec_resolves_to_file(
    spec: &str,
    bases: &[PathBuf],
    deps: &std::collections::HashMap<String, PathBuf>,
) -> bool {
    resolve_spec_path(spec, bases, deps).is_some()
}

fn file_candidate(joined: &Path, spec: &str) -> Option<PathBuf> {
    if joined.is_dir() {
        let index = joined.join("index.hnl");
        if index.is_file() {
            return Some(index);
        }
        if let Some(name) = joined.file_name().and_then(|s| s.to_str()) {
            let main = joined.join(format!("{}.hnl", name));
            if main.is_file() {
                return Some(main);
            }
        }
    }
    if !spec.ends_with(".hnl") {
        let with_ext = joined.with_extension("hnl");
        if with_ext.is_file() {
            return Some(with_ext);
        }
    }
    if joined.is_file() {
        Some(joined.to_path_buf())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve `path` (possibly relative) against the request's `cwd`.
fn resolve(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    /// A unique temp dir per test.
    fn tmpdir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("hnl-ext-compile-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn no_args_is_usage_error() {
        let res = run(&[], Path::new("."));
        assert_eq!(res.code, 2);
        assert!(res.stderr.contains("usage: compile"));
    }

    #[test]
    fn missing_file_is_read_error() {
        let dir = tmpdir("missing");
        let res = run(&["js".into(), "nope.hnl".into()], &dir);
        assert_eq!(res.code, 2);
        assert!(res.stderr.contains("compile: failed to read"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_target_is_usage_error() {
        let dir = tmpdir("target");
        let file = dir.join("p.hnl");
        std::fs::write(&file, "x = 1 + 2;").unwrap();
        let res = run(
            &["bogus".into(), file.to_string_lossy().into_owned()],
            &dir,
        );
        assert_eq!(res.code, 2);
        assert!(res.stderr.contains("compile: unsupported target `bogus`"));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The `export!`-generated manifest symbol, called directly.
    #[test]
    fn manifest_declares_compile() {
        let raw = handl_ext_manifest();
        assert!(!raw.is_null());
        let json = unsafe { CStr::from_ptr(raw) }
            .to_string_lossy()
            .into_owned();
        unsafe { handl_ext_free(raw) };
        let manifest: ExtensionManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(manifest.name, "compile");
        let names: Vec<&str> = manifest.commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["compile"]);
    }
}
