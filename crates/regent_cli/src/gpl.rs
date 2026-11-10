//! Commands over GPL source: lex, parse, resolve, dat, check-gpl,
//! compile.

use std::io;

use regent_core::gpl::ast::Item;
use regent_core::gpl::resolve::Report;
use regent_core::gpl::{dat, diag, lex, parse};

use crate::util::{load, report_errors};

pub fn cmd_lex(paths: &[String]) -> Result<bool, String> {
    let mut ok = true;

    for (path, src) in load(paths, "gpl")? {
        let lexed = lex(&src);

        println!("\n== {}", path.display());

        for t in &lexed.tokens {
            let (line, col) = diag::location(&src, t.span.start);
            println!("  {line:>5}:{col:<3} {:<12} {:?}", t.node.kind(), t.node);
        }

        if !lexed.is_ok() {
            ok = false;
            let mut err = io::stderr();
            diag::report(&mut err, &path.display().to_string(), &src, &lexed)
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(ok)
}

pub fn cmd_parse(paths: &[String]) -> Result<bool, String> {
    let files = load(paths, "gpl")?;
    let mut ok = true;
    let (mut funcs, mut protos, mut consts, mut stmts) = (0, 0, 0, 0);

    for (path, src) in &files {
        let parsed = parse(src);

        if !parsed.is_ok() {
            ok = false;
            report_errors(path, src, &parsed.errors, parsed.suppressed);
        }

        for item in &parsed.script.items {
            match item {
                Item::Function(_) => funcs += 1,
                Item::Prototype(_) => protos += 1,
                Item::Constant(_) => consts += 1,
            }
        }

        parsed.script.visit_stmts(&mut |_| stmts += 1);
    }

    println!(
        "parsed {} files: {funcs} functions, {protos} prototypes, {consts} constants, {stmts} statements",
        files.len()
    );

    Ok(ok)
}

pub fn cmd_check_gpl(paths: &[String]) -> Result<bool, String> {
    let files = load(paths, "gpl")?;
    let mut failed = 0;
    let mut errors = 0;

    for (path, src) in &files {
        let parsed = parse(src);

        if !parsed.is_ok() {
            failed += 1;
            errors += parsed.error_count();
            report_errors(path, src, &parsed.errors, parsed.suppressed);
        }
    }

    print!("checked {} files, {failed} failed", files.len());
    println!(
        "{}",
        if failed > 0 {
            format!(", {errors} errors")
        } else {
            String::new()
        }
    );

    Ok(failed == 0)
}

/// Parse everything, then check that every `$Name` refers to something real.
pub fn cmd_resolve(paths: &[String]) -> Result<bool, String> {
    let files = load(paths, "gpl")?;
    let mut report = Report::default();
    let mut ok = true;

    for (path, src) in &files {
        let parsed = parse(src);

        if !parsed.is_ok() {
            ok = false;
            report_errors(path, src, &parsed.errors, parsed.suppressed);
        }

        report.add(&parsed.script);
    }

    let (script, builtin, unresolved) = report.counts();

    println!("files              {}", files.len());
    println!("functions defined  {}", report.defined.len());
    println!("prototypes         {}", report.prototypes.len());
    println!("distinct $names    {}", report.referenced.len());
    println!("call sites         {script} script, {builtin} builtin, {unresolved} unresolved");

    // Large by design: the Mx tree redefines most base names.
    println!(
        "redefined names    {} (base game vs expansion)",
        report.duplicate_definitions.len()
    );

    let missing = report.unresolved();

    if missing.is_empty() {
        println!("\nevery $name resolves.");
    } else {
        ok = false;
        println!("\nunresolved ({}):", missing.len());

        for (name, n) in &missing {
            println!("  {name:<34} x{n}");
        }
    }

    let unused = report.unused_builtins();
    println!("\ndocumented builtins never called: {}", unused.len());

    for name in unused.chunks(4) {
        println!("  {}", name.join(", "));
    }

    println!(
        "\ncorpus functions never referenced: {} (engine entry points)",
        report.uncalled().len()
    );

    Ok(ok)
}

pub fn cmd_dat(paths: &[String]) -> Result<bool, String> {
    let files = load(paths, "dat")?;
    let mut failed = 0;
    let mut protos = 0;
    let mut pairs = 0;

    for (path, src) in &files {
        let parsed = dat::parse(src);

        if !parsed.is_ok() {
            failed += 1;
            report_errors(path, src, &parsed.errors, parsed.suppressed);
        }

        protos += parsed.prototypes.len();
        pairs += parsed
            .prototypes
            .iter()
            .map(|p| p.values.len())
            .sum::<usize>();
    }

    println!(
        "parsed {} .dat files: {protos} prototypes, {pairs} values, {failed} failed",
        files.len()
    );

    Ok(failed == 0)
}

/// Five of the SDK's seven projects compile byte-identical to the shipped
/// images: `a_whole_bcd_is_compiled_from_its_project`.
pub fn cmd_compile(project: &str, out: &str) -> Result<bool, String> {
    let bytes = regent_core::gpl::emit::compile_project(std::path::Path::new(project))
        .map_err(|e| e.to_string())?;

    std::fs::write(out, &bytes).map_err(|e| format!("{out}: {e}"))?;
    println!("wrote {out}: {} bytes", bytes.len());

    Ok(true)
}
