//! Errors with context that stays outside the source chain.
//!
//! [`Report`] owns an error and its context strings. Context changes the rendered
//! report, not [`Error::source`].

use std::{
    error::Error,
    fmt::{Debug, Display, Formatter, Result as FmtResult},
    iter::FusedIterator,
};

mod ext;
mod fmt;
#[macro_use]
mod macros;

pub use ext::ResultExt;

/// A `Result` whose error defaults to [`Report`].
pub type Result<T, E = Report> = std::result::Result<T, E>;

/// An error and the context collected while handling it.
///
/// Context does not wrap the error. Downcasts and [`Report::chain`] see only the
/// original error and its sources.
#[must_use]
pub struct Report {
    error: Box<dyn Error + Send + Sync + 'static>,
    contexts: Vec<String>,
    #[cfg(feature = "backtrace")]
    backtrace: Option<std::backtrace::Backtrace>,
    #[cfg(feature = "spantrace")]
    spantrace: Option<tracing_error::SpanTrace>,
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
    pub fn msg(message: impl Display) -> Self {
        Self::new(OwnedMessageError(message.to_string()))
    }

    /// Adds context without changing the source chain.
    #[inline]
    pub fn context(mut self, context: impl Display) -> Self {
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
    pub fn error(&self) -> &(dyn Error + 'static) {
        self.error.as_ref()
    }

    /// Returns context strings in the order callers added them.
    ///
    /// Formatters print the newest context first.
    #[inline]
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
    pub fn downcast_ref<T>(&self) -> Option<&T>
    where
        T: Error + 'static,
    {
        self.chain().find_map(|error| error.downcast_ref::<T>())
    }

    /// Returns the captured backtrace, if capture was enabled and succeeded.
    #[cfg(feature = "backtrace")]
    #[inline]
    pub fn backtrace(&self) -> Option<&std::backtrace::Backtrace> {
        self.backtrace.as_ref()
    }

    /// Returns the captured span trace, if one exists.
    #[cfg(feature = "spantrace")]
    #[inline]
    pub fn spantrace(&self) -> Option<&tracing_error::SpanTrace> {
        self.spantrace.as_ref()
    }
}

impl Display for Report {
    #[inline]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        crate::fmt::render_report(self, formatter)
    }
}

impl Debug for Report {
    #[inline]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        crate::fmt::render_report(self, formatter)
    }
}

impl Error for Report {
    #[inline]
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.error())
    }
}

impl From<Box<dyn Error + Send + Sync + 'static>> for Report {
    #[inline]
    fn from(error: Box<dyn Error + Send + Sync + 'static>) -> Self {
        Self::from_boxed(error)
    }
}

/// A non-allocating iterator over an error and its sources.
#[derive(Clone)]
#[must_use]
pub struct Chain<'a> {
    current: Option<&'a (dyn Error + 'static)>,
}

impl<'a> Iterator for Chain<'a> {
    type Item = &'a (dyn Error + 'static);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let error = self.current?;
        self.current = error.source();
        Some(error)
    }
}

impl FusedIterator for Chain<'_> {}

#[derive(Debug)]
struct OwnedMessageError(String);

impl Display for OwnedMessageError {
    #[inline]
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(&self.0)
    }
}

impl Error for OwnedMessageError {}

#[cfg(feature = "backtrace")]
fn capture_backtrace() -> Option<std::backtrace::Backtrace> {
    let backtrace = std::backtrace::Backtrace::capture();
    (backtrace.status() == std::backtrace::BacktraceStatus::Captured).then_some(backtrace)
}

#[cfg(feature = "spantrace")]
fn capture_spantrace() -> Option<tracing_error::SpanTrace> {
    let spantrace = tracing_error::SpanTrace::capture();
    (spantrace.status() == tracing_error::SpanTraceStatus::CAPTURED).then_some(spantrace)
}

/// Installs a panic hook that writes a report before the prior hook runs.
///
/// This function is available with the `panic-hook` feature. It changes the
/// process-wide panic hook, so call it once during startup.
#[cfg(feature = "panic-hook")]
pub mod panic_hook {
    use std::{io::Write, panic::PanicHookInfo};

    /// Installs the reporting hook and then calls the hook it replaced.
    pub fn install() {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let report = crate::Report::msg(panic_message(info));
            {
                let mut stderr = std::io::stderr().lock();
                let _ = writeln!(stderr, "{report:?}");
            }
            previous(info);
        }));
    }

    fn panic_message(info: &PanicHookInfo<'_>) -> String {
        let payload = if let Some(message) = info.payload().downcast_ref::<&str>() {
            *message
        } else if let Some(message) = info.payload().downcast_ref::<String>() {
            message.as_str()
        } else {
            "non-string panic payload"
        };

        match info.location() {
            Some(location) => format!(
                "panic at {}:{}:{}: {payload}",
                location.file(),
                location.line(),
                location.column()
            ),
            None => format!("panic: {payload}"),
        }
    }
}

/// Installs the `panic-hook` feature's process-wide panic hook.
#[cfg(feature = "panic-hook")]
pub use panic_hook::install as install_panic_hook;
