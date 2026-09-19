//! Formatting shared by [`core::fmt::Display`] and [`core::fmt::Debug`].
//!
//! The formatter writes directly to the caller's buffer. It has no global state.

use std::{error::Error, fmt};

#[cfg(feature = "color")]
use yansi::Paint;

use crate::Report;

/// Formats a report for both `Display` and `Debug`.
///
/// Both traits call this function, so they produce the same text.
pub(crate) fn render_report(report: &Report, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    #[cfg(feature = "pretty")]
    {
        return render_pretty(report, formatter);
    }

    #[cfg(not(feature = "pretty"))]
    render_compact(report, formatter)
}

/// Writes the compact report format.
#[cfg(not(feature = "pretty"))]
fn render_compact(report: &Report, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    // The newest context describes the outermost operation.
    for context in report.contexts().iter().rev() {
        write!(formatter, "{context}: ")?;
    }

    write_error(formatter, report.error())?;

    let mut cause = report.error().source();
    while let Some(error) = cause {
        write!(formatter, ": {error}")?;
        cause = error.source();
    }

    Ok(())
}

/// Writes the multi-section report format.
#[cfg(feature = "pretty")]
fn render_pretty(report: &Report, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write_heading(formatter, "Error")?;
    formatter.write_str(":\n  ")?;
    write_error(formatter, report.error())?;

    let mut cause = report.error().source();
    if cause.is_some() {
        formatter.write_str("\n\n")?;
        write_heading(formatter, "Caused by")?;
        formatter.write_str(":")?;

        let mut index = 0;
        while let Some(error) = cause {
            write!(formatter, "\n  {index}: {error}")?;
            cause = error.source();
            index += 1;
        }
    }

    if !report.contexts().is_empty() {
        formatter.write_str("\n\n")?;
        write_heading(formatter, "Context")?;
        formatter.write_str(":")?;

        for (index, context) in report.contexts().iter().rev().enumerate() {
            write!(formatter, "\n  {index}: {context}")?;
        }
    }

    #[cfg(feature = "backtrace")]
    render_backtrace(report, formatter)?;

    #[cfg(feature = "spantrace")]
    render_spantrace(report, formatter)?;

    Ok(())
}

/// Writes the captured backtrace section.
#[cfg(all(feature = "pretty", feature = "backtrace"))]
fn render_backtrace(report: &Report, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let Some(backtrace) = report.backtrace() else {
        return Ok(());
    };

    // `Backtrace::capture` returns a value when backtraces are off. Omit it.
    if backtrace.status() != std::backtrace::BacktraceStatus::Captured {
        return Ok(());
    }

    formatter.write_str("\n\n")?;
    write_heading(formatter, "Backtrace")?;
    formatter.write_str(":\n")?;
    write!(formatter, "{backtrace}")
}

/// Writes the captured span trace section.
#[cfg(all(feature = "pretty", feature = "spantrace"))]
fn render_spantrace(report: &Report, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let Some(spantrace) = report.spantrace() else {
        return Ok(());
    };

    formatter.write_str("\n\n")?;
    write_heading(formatter, "Span trace")?;
    formatter.write_str(":\n")?;
    write!(formatter, "{spantrace}")
}

/// Writes the root error, with color when enabled.
fn write_error(formatter: &mut fmt::Formatter<'_>, error: &(dyn Error + 'static)) -> fmt::Result {
    #[cfg(feature = "color")]
    {
        return write!(formatter, "{}", error.red());
    }

    #[cfg(not(feature = "color"))]
    write!(formatter, "{error}")
}

/// Writes a section heading, with color when enabled.
#[cfg(feature = "pretty")]
fn write_heading(formatter: &mut fmt::Formatter<'_>, heading: &str) -> fmt::Result {
    #[cfg(feature = "color")]
    {
        return write!(formatter, "{}", heading.bold());
    }

    #[cfg(not(feature = "color"))]
    formatter.write_str(heading)
}
