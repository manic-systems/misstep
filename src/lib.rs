//! Errors with context that stays outside the source chain.
//!
//! [`Report`] owns an error and its context strings. Context changes the rendered
//! report, not the source chain reached through [`Report::chain`].
//!
//! `Report` deliberately does not implement [`Error`], which is what lets the
//! blanket `From` conversion exist and makes `?` work on any error type.

#[cfg(feature = "backtrace")]
use std::backtrace;
use std::{
    error::Error,
    fmt::{Debug, Display, Formatter, Result as FmtResult},
    iter::FusedIterator,
    result,
};

mod fmt;
#[macro_use]
mod macros;

/// A `Result` whose error defaults to [`Report`].
pub type Result<T, E = Report> = result::Result<T, E>;

/// Adds context to a [`Result`].
pub trait ResultExt<T> {
    /// Adds context to an error result.
    ///
    /// # Errors
    ///
    /// Converts the contained error into a [`Report`] and returns it with the context.
    fn context<C>(self, context: C) -> Result<T>
    where
        C: Display;

    /// Adds context from a closure.
    ///
    /// # Errors
    ///
    /// Converts the contained error into a [`Report`] and returns it with the context.
    fn with_context<C, F>(self, context: F) -> Result<T>
    where
        C: Display,
        F: FnOnce() -> C;
}

impl<T, E> ResultExt<T> for Result<T, E>
where
    E: Error + Send + Sync + 'static,
{
    #[inline]
    fn context<C>(self, context: C) -> Result<T>
    where
        C: Display,
    {
        self.map_err(|error| Report::from_error(error).context(context))
    }

    #[inline]
    fn with_context<C, F>(self, context: F) -> Result<T>
    where
        C: Display,
        F: FnOnce() -> C,
    {
        self.map_err(|error| Report::from_error(error).with_context(context))
    }
}

impl<T> ResultExt<T> for Result<T, Report> {
    #[inline]
    fn context<C>(self, context: C) -> Self
    where
        C: Display,
    {
        self.map_err(|report| report.context(context))
    }

    #[inline]
    fn with_context<C, F>(self, context: F) -> Self
    where
        C: Display,
        F: FnOnce() -> C,
    {
        self.map_err(|report| report.with_context(context))
    }
}

/// Turns a missing [`Option`] value into a [`Report`].
pub trait OptionExt<T> {
    /// Reports [`None`] with the context as the error message.
    ///
    /// # Errors
    ///
    /// Returns a [`Report`] carrying `context` when the option is [`None`].
    fn context<C>(self, context: C) -> Result<T>
    where
        C: Display;

    /// Reports [`None`] with a message from a closure.
    ///
    /// The closure runs only when the option is [`None`].
    ///
    /// # Errors
    ///
    /// Returns a [`Report`] carrying the closure's message when the option is [`None`].
    fn with_context<C, F>(self, context: F) -> Result<T>
    where
        C: Display,
        F: FnOnce() -> C;
}

impl<T> OptionExt<T> for Option<T> {
    #[inline]
    fn context<C>(self, context: C) -> Result<T>
    where
        C: Display,
    {
        self.ok_or_else(|| Report::msg(context))
    }

    #[inline]
    fn with_context<C, F>(self, context: F) -> Result<T>
    where
        C: Display,
        F: FnOnce() -> C,
    {
        self.ok_or_else(|| Report::msg(context()))
    }
}

/// An error and the context collected while handling it.
///
/// Context does not wrap the error. Downcasts and [`Report::chain`] see only the
/// original error and its sources.
///
/// This type does not implement [`Error`]. Returning it from a function that
/// owes a `Box<dyn Error>` still works through the [`From`] impl below.
#[must_use]
pub struct Report {
    /// The original error.
    error: Box<dyn Error + Send + Sync + 'static>,
    /// Context strings in insertion order.
    contexts: Vec<String>,
    #[cfg(feature = "backtrace")]
    /// The captured backtrace when capture succeeded.
    backtrace: Option<Box<backtrace::Backtrace>>,
    #[cfg(feature = "spantrace")]
    /// The captured span trace when capture succeeded.
    spantrace: Option<Box<tracing_error::SpanTrace>>,
}

impl Report {
    /// Stores an error in a new report.
    #[inline]
    pub fn new<E>(error: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self::from_boxed(Box::new(error))
    }

    /// Calls [`Report::new`].
    #[inline]
    pub fn from_error<E>(error: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self::new(error)
    }

    /// Stores a boxed error in a new report.
    #[inline]
    pub fn from_boxed(error: Box<dyn Error + Send + Sync + 'static>) -> Self {
        Self {
            error,
            contexts: Vec::new(),
            #[cfg(feature = "backtrace")]
            backtrace: capture_backtrace(),
            #[cfg(feature = "spantrace")]
            spantrace: capture_spantrace(),
        }
    }

    /// Builds a report from a displayable message.
    #[inline]
    pub fn msg<D>(message: D) -> Self
    where
        D: Display,
    {
        Self::new(OwnedMessageError(message.to_string()))
    }

    /// Adds context without changing the source chain.
    #[inline]
    pub fn context<D>(mut self, context: D) -> Self
    where
        D: Display,
    {
        self.contexts.push(context.to_string());
        self
    }

    /// Adds context returned by `context`.
    ///
    /// This method calls the closure immediately. [`ResultExt::with_context`] calls
    /// its closure only for an error result.
    #[inline]
    pub fn with_context<C, F>(self, context: F) -> Self
    where
        C: Display,
        F: FnOnce() -> C,
    {
        self.context(context())
    }

    /// Returns the error at the start of the source chain.
    #[inline]
    #[must_use]
    pub fn error(&self) -> &(dyn Error + 'static) {
        self.error.as_ref()
    }

    /// Returns context strings in the order callers added them.
    ///
    /// Formatters print the newest context first.
    #[inline]
    #[must_use]
    pub fn contexts(&self) -> &[String] {
        &self.contexts
    }

    /// Walks the error and each of its sources.
    ///
    /// Context is not part of this iterator.
    #[inline]
    pub fn chain(&self) -> Chain<'_> {
        Chain {
            current: Some(self.error()),
        }
    }

    /// Finds `T` in the source chain.
    #[inline]
    #[must_use]
    pub fn downcast_ref<T>(&self) -> Option<&T>
    where
        T: Error + 'static,
    {
        self.chain().find_map(|error| error.downcast_ref::<T>())
    }

    /// Returns the captured backtrace, if capture was enabled and succeeded.
    #[cfg(feature = "backtrace")]
    #[inline]
    #[must_use]
    pub fn backtrace(&self) -> Option<&backtrace::Backtrace> {
        self.backtrace.as_deref()
    }

    /// Returns the captured span trace, if one exists.
    #[cfg(feature = "spantrace")]
    #[inline]
    #[must_use]
    pub fn spantrace(&self) -> Option<&tracing_error::SpanTrace> {
        self.spantrace.as_deref()
    }
}

impl Display for Report {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        fmt::render_compact(self, f)
    }
}

impl Debug for Report {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        #[cfg(feature = "pretty")]
        {
            fmt::render_pretty(self, f)
        }

        #[cfg(not(feature = "pretty"))]
        fmt::render_compact(self, f)
    }
}

impl<E> From<E> for Report
where
    E: Error + Send + Sync + 'static,
{
    #[inline]
    fn from(error: E) -> Self {
        Self::new(error)
    }
}

impl From<Report> for Box<dyn Error + Send + Sync + 'static> {
    #[inline]
    fn from(report: Report) -> Self {
        Box::new(BoxedReport(report))
    }
}

impl From<Report> for Box<dyn Error + 'static> {
    #[inline]
    fn from(report: Report) -> Self {
        Box::new(BoxedReport(report))
    }
}

/// Gives a [`Report`] an [`Error`] impl for the boxing conversions.
#[derive(Debug)]
struct BoxedReport(Report);

impl Display for BoxedReport {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Display::fmt(&self.0, f)
    }
}

impl Error for BoxedReport {
    #[inline]
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.error())
    }
}

/// A non-allocating iterator over an error and its sources.
#[derive(Clone)]
#[must_use]
pub struct Chain<'error> {
    /// The next error returned by the iterator.
    current: Option<&'error (dyn Error + 'static)>,
}

impl<'error> Iterator for Chain<'error> {
    type Item = &'error (dyn Error + 'static);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let error = self.current?;
        self.current = error.source();
        Some(error)
    }
}

impl FusedIterator for Chain<'_> {}

#[derive(Debug)]
/// An owned string exposed as an error.
struct OwnedMessageError(String);

impl Display for OwnedMessageError {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(&self.0)
    }
}

impl Error for OwnedMessageError {}

#[cfg(feature = "backtrace")]
/// Captures an enabled backtrace.
fn capture_backtrace() -> Option<Box<backtrace::Backtrace>> {
    let backtrace = backtrace::Backtrace::capture();
    (backtrace.status() == backtrace::BacktraceStatus::Captured).then(|| Box::new(backtrace))
}

#[cfg(feature = "spantrace")]
/// Captures an enabled span trace.
fn capture_spantrace() -> Option<Box<tracing_error::SpanTrace>> {
    let spantrace = tracing_error::SpanTrace::capture();
    (spantrace.status() == tracing_error::SpanTraceStatus::CAPTURED).then(|| Box::new(spantrace))
}

/// Installs a panic hook that writes a report before the prior hook runs.
///
/// This function is available with the `panic-hook` feature. It changes the
/// process-wide panic hook, so call it once during startup.
#[cfg(feature = "panic-hook")]
pub mod panic_hook {
    use std::{
        io::{self, Write as _},
        panic::{self, PanicHookInfo},
    };

    /// Installs the reporting hook and then calls the hook it replaced.
    #[inline]
    #[expect(
        clippy::use_debug,
        reason = "a panic prints the full report, which `pretty` renders through Debug"
    )]
    pub fn install() {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let report = crate::Report::msg(panic_message(info));
            let mut stderr = io::stderr().lock();
            drop(writeln!(stderr, "{report:?}"));
            drop(stderr);

            previous(info);
        }));
    }

    /// Formats a panic payload and its source location.
    fn panic_message(info: &PanicHookInfo<'_>) -> String {
        let payload = info.payload().downcast_ref::<&str>().map_or_else(
            || {
                info.payload()
                    .downcast_ref::<String>()
                    .map_or("non-string panic payload", |message| message.as_str())
            },
            |message| *message,
        );

        info.location().map_or_else(
            || format!("panic: {payload}"),
            |location| {
                format!(
                    "panic at {}:{}:{}: {payload}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            },
        )
    }
}

/// Installs the `panic-hook` feature's process-wide panic hook.
#[cfg(feature = "panic-hook")]
#[inline]
pub fn install_panic_hook() {
    panic_hook::install();
}
