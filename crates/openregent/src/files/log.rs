//! The session's log: tracing's lines, openregent's, the libraries' and
//! Bevy's, to the terminal and to a file. Bevy's `LogPlugin` is off: this
//! subscriber is set once at start, so headless runs log too.
//!
//! ```text
//!  $XDG_DATA_HOME/openregent/play.log     (~/.local/share/... by default)
//!    each session overwrites it; the previous is kept as play.log.1
//!  error! ──> stderr and the file, from every crate
//!  RUST_LOG, when set, filters both instead (EnvFilter syntax).
//!  feature `trace`: every info span, a Chrome trace (timeline)
//! ```

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use bevy::log::error;
use bevy::log::tracing_subscriber::fmt::MakeWriter;
use bevy::log::tracing_subscriber::prelude::*;
use bevy::log::tracing_subscriber::{EnvFilter, Registry, fmt};

/// The log in [`crate::files::env::OWN`].
const FILE: &str = "play.log";

/// The previous session's log.
const PREVIOUS: &str = "play.log.1";

/// The open log; unset when it could not be made.
static LOG: OnceLock<Mutex<File>> = OnceLock::new();

/// Whether a session keeps its log file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    /// `play.log`, the last moved to `play.log.1`.
    File,
    /// The terminal only: a screenshot leaves the user's log alone.
    Terminal,
}

/// [`crate::files::env::OWN`] under the data directory, made; `None` with none.
pub fn dir() -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join(crate::files::env::OWN);

    std::fs::create_dir_all(&dir).ok().map(|()| dir)
}

/// Start this session's log: the file as `keep` asks, then the global
/// subscriber. A file that will not open is logged, and play goes on.
pub fn start(keep: Keep) {
    let opened = match keep {
        Keep::File => open(),
        Keep::Terminal => Ok(()),
    };

    let terminal = fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(EnvFilter::from_default_env());

    let file = LOG.get().map(|_| {
        fmt::layer()
            .with_ansi(false)
            .with_writer(Sink)
            .with_filter(EnvFilter::from_default_env())
    });

    // Set once; should one be set already, it stays.
    let _ = Registry::default()
        .with(timeline())
        .with(terminal)
        .with(file)
        .try_init();

    if let Err(e) = opened {
        error!("no log: {e}");
    }
}

/// A layer of the subscriber, boxed: present or not by build.
type Boxed = Box<dyn bevy::log::tracing_subscriber::Layer<Registry> + Send + Sync>;

/// The run's timeline, built with the `trace` feature: every info span
/// (Bevy's systems, render passes, ours) as a Chrome trace, for
/// ui.perfetto.dev. Written to `$TRACE_CHROME`, else
/// `./trace-<time>.json`, as the process ends ([`finish`]).
#[cfg(feature = "trace")]
#[allow(clippy::unnecessary_wraps, reason = "None without the trace feature")]
fn timeline() -> Option<Boxed> {
    use bevy::log::tracing_subscriber::fmt::FormattedFields;
    use bevy::log::tracing_subscriber::fmt::format::DefaultFields;
    use tracing_chrome::EventOrSpan;

    // A span named with its fields, as Bevy's own timeline:
    // "system: name=openregent::game::clock::tick", not "system".
    let mut builder = tracing_chrome::ChromeLayerBuilder::new()
        .include_locations(false)
        .name_fn(Box::new(|event_or_span| match event_or_span {
            EventOrSpan::Event(event) => event.metadata().name().into(),
            EventOrSpan::Span(span) => {
                match span.extensions().get::<FormattedFields<DefaultFields>>() {
                    Some(fields) if !fields.fields.is_empty() => {
                        format!("{}: {}", span.metadata().name(), fields.fields)
                    }
                    _ => span.metadata().name().into(),
                }
            }
        }));
    if let Some(path) = std::env::var_os(TRACE_FILE) {
        builder = builder.file(path);
    }
    let (layer, guard) = builder.build();

    if let Ok(mut kept) = TIMELINE.lock() {
        *kept = Some(guard);
    }
    Some(Box::new(
        Fields
            .and_then(layer)
            .with_filter(EnvFilter::new(TRACE_LEVEL)),
    ))
}

/// Each new span's fields kept with it, formatted, for the timeline's
/// names: the terminal's layer keeps them only for spans it shows.
#[cfg(feature = "trace")]
struct Fields;

#[cfg(feature = "trace")]
impl<S> bevy::log::tracing_subscriber::Layer<S> for Fields
where
    S: bevy::log::tracing::Subscriber
        + for<'a> bevy::log::tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &bevy::log::tracing::span::Attributes<'_>,
        id: &bevy::log::tracing::span::Id,
        ctx: bevy::log::tracing_subscriber::layer::Context<'_, S>,
    ) {
        use bevy::log::tracing_subscriber::fmt::FormattedFields;
        use bevy::log::tracing_subscriber::fmt::format::{DefaultFields, FormatFields};

        if let Some(span) = ctx.span(id) {
            let mut extensions = span.extensions_mut();

            if extensions
                .get_mut::<FormattedFields<DefaultFields>>()
                .is_none()
            {
                let mut fields = FormattedFields::<DefaultFields>::new(String::new());

                if DefaultFields::default()
                    .format_fields(fields.as_writer(), attrs)
                    .is_ok()
                {
                    extensions.insert(fields);
                }
            }
        }
    }
}

#[cfg(not(feature = "trace"))]
fn timeline() -> Option<Boxed> {
    None
}

/// The timeline's file, as Bevy's own `trace_chrome` names it.
#[cfg(feature = "trace")]
const TRACE_FILE: &str = "TRACE_CHROME";

/// The spans the timeline keeps: info and up; wgpu's own are many.
#[cfg(feature = "trace")]
const TRACE_LEVEL: &str = "info,wgpu=warn,naga=warn";

/// The timeline's writer: dropped, the file is finished.
#[cfg(feature = "trace")]
static TIMELINE: Mutex<Option<tracing_chrome::FlushGuard>> = Mutex::new(None);

/// The timeline, if any, written out: once, as the process ends.
#[allow(
    clippy::missing_const_for_fn,
    reason = "empty only without the trace feature"
)]
pub fn finish() {
    #[cfg(feature = "trace")]
    if let Ok(mut kept) = TIMELINE.lock() {
        drop(kept.take());
    }
}

/// The file, the last one moved to [`PREVIOUS`].
fn open() -> Result<(), String> {
    let dir = dir().ok_or_else(|| "no data directory".to_owned())?;
    let path = dir.join(FILE);
    let _ = std::fs::rename(&path, dir.join(PREVIOUS));
    let file = File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let _ = LOG.set(Mutex::new(file));

    Ok(())
}

/// The file as a tracing writer.
struct Sink;

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match LOG.get().map(Mutex::lock) {
            Some(Ok(mut f)) => f.write(buf),
            _ => Ok(buf.len()),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match LOG.get().map(Mutex::lock) {
            Some(Ok(mut f)) => f.flush(),
            _ => Ok(()),
        }
    }
}

impl MakeWriter<'_> for Sink {
    type Writer = Sink;

    fn make_writer(&self) -> Sink {
        Sink
    }
}
