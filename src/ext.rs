//! Methods for adding context to `Result` errors.

use core::fmt::Display;
use std::error::Error;

use crate::{Report, Result};

/// Adds context to a [`Result`].
///
/// On `Err`, these methods create a [`Report`] and add the context. The report
/// keeps the original error and its source chain.
pub trait ResultExt<T> {
    /// Adds context to an error result.
    fn context<C>(self, context: C) -> Result<T>
    where
        C: Display;

    /// Adds context from a closure.
    ///
    /// The closure runs only for an error result.
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
