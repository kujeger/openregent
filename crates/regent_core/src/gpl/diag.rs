//! Renders lexer errors against their source.
//!
//! Thin wrapper, so `ariadne` can be swapped without touching callers.

use std::io::Write;

use ariadne::{Color, Label, Report, ReportKind, Source};

use super::lexer::{Lexed, MAX_ERRORS};

/// Write a caret-annotated report for each error to `out`.
///
/// `path` heads the report. `src` must be the exact text that was lexed.
///
/// At the error cap, a closing line gives the suppressed count and asks
/// whether the file is GPL at all.
///
/// # Errors
///
/// Only if writing to `out` fails.
pub fn report(
    out: &mut impl Write,
    path: &str,
    src: &str,
    lexed: &Lexed<'_>,
) -> std::io::Result<()> {
    for e in &lexed.errors {
        writeln!(out, "{path}:")?;

        let mut buf = Vec::new();

        Report::build(ReportKind::Error, e.span.clone())
            .with_message(e.to_string())
            .with_label(
                Label::new(e.span.clone())
                    .with_color(Color::Red)
                    .with_message("not a GPL token"),
            )
            .finish()
            .write(Source::from(src), &mut buf)?;

        out.write_all(&buf)?;
    }

    if lexed.is_flooded() {
        writeln!(
            out,
            "{path}: {} more errors suppressed ({} total) — is this GPL source?",
            lexed.suppressed,
            lexed.error_count()
        )?;
    }

    Ok(())
}

/// Record the error `make` builds, or past [`MAX_ERRORS`] only count it: the
/// lexer, parser and `.dat` reader all stop recording there.
pub(crate) fn capped<E>(errors: &mut Vec<E>, suppressed: &mut usize, make: impl FnOnce() -> E) {
    if errors.len() < MAX_ERRORS {
        errors.push(make());
    } else {
        *suppressed += 1;
    }
}

/// The 1-based line and column of a byte offset, for compact listings.
pub fn location(src: &str, offset: usize) -> (usize, usize) {
    let upto = &src[..offset.min(src.len())];
    let line = upto.matches('\n').count() + 1;
    let col = upto
        .rsplit('\n')
        .next()
        .map_or(1, |l| l.chars().count() + 1);

    (line, col)
}
