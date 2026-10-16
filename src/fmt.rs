//! Formatting for [`core::fmt::Display`] and [`core::fmt::Debug`].
//!
//! `Display` writes the compact form so a report can sit inside a larger
//! message, or only the error under `{:#}`. `Debug` writes the multi-section
//! form under the `pretty` feature. The formatter writes directly to the
//! caller's buffer. It has no global state.

#[cfg(feature = "color")]
use std::io::{
    self,
    IsTerminal as _,
};
#[cfg(all(feature = "pretty", feature = "location"))]
use std::iter;
use std::{
    error::Error,
    fmt,
};

#[cfg(feature = "color")]
use yansi::{
    Condition,
    Paint as _,
};

use crate::Report;

/// Colors only on a terminal stderr that `NO_COLOR` and `CLICOLOR` allow.
///
/// `&` instead of `&&` keeps branch coverage from depending on whether the
/// tests run on a terminal.
#[cfg(feature = "color")]
const COLOR: Condition =
    Condition::from(|| io::stderr().is_terminal() & Condition::clicolor() & Condition::no_color());

/// Writes the compact report format.
pub fn render_compact<E>(report: &Report<E>, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    // The newest context describes the outermost operation.
    for context in report.contexts().iter().rev() {
        write!(formatter, "{context}: ")?;
    }

    write_error(formatter, report.error())?;

    for error in report.chain().skip(1) {
        write!(formatter, ": {error}")?;
    }

    Ok(())
}

/// Writes the multi-section report format.
#[cfg(feature = "pretty")]
pub fn render_pretty<E>(report: &Report<E>, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    write_error(formatter, report.error())?;

    let mut causes = report.chain().skip(1).peekable();
    if causes.peek().is_some() {
        formatter.write_str("\n\n")?;
        write_heading(formatter, "Caused by")?;
        formatter.write_str(":")?;

        for (index, error) in causes.enumerate() {
            write!(formatter, "\n  {index}: {error}")?;
        }
    }

    if !report.contexts().is_empty() {
        formatter.write_str("\n\n")?;
        write_heading(formatter, "Context")?;
        formatter.write_str(":")?;

        for (index, context) in report.contexts().iter().rev().enumerate() {
            write!(formatter, "\n  {index}: {context}")?;

            #[cfg(feature = "location")]
            {
                let width =
                    iter::successors(Some(index), |rest| (*rest >= 10).then(|| rest / 10)).count();
                write!(formatter, "\n  {:width$}  at {}", "", context.location())?;
            }
        }
    }

    #[cfg(feature = "location")]
    if let Some(location) = report.location() {
        formatter.write_str("\n\n")?;
        write_heading(formatter, "Location")?;
        write!(formatter, ":\n  {location}")?;
    }

    #[cfg(feature = "backtrace")]
    render_backtrace(report, formatter)?;

    #[cfg(feature = "spantrace")]
    render_spantrace(report, formatter)?;

    Ok(())
}

/// Writes the captured backtrace section.
#[cfg(all(feature = "pretty", feature = "backtrace"))]
fn render_backtrace<E>(report: &Report<E>, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let Some(backtrace) = report.backtrace() else {
        return Ok(());
    };

    formatter.write_str("\n\n")?;
    write_heading(formatter, "Backtrace")?;
    formatter.write_str(":\n")?;
    write!(formatter, "{backtrace}")
}

/// Writes the captured span trace section.
#[cfg(all(feature = "pretty", feature = "spantrace"))]
fn render_spantrace<E>(report: &Report<E>, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    let Some(spantrace) = report.spantrace() else {
        return Ok(());
    };

    formatter.write_str("\n\n")?;
    write_heading(formatter, "Span trace")?;
    formatter.write_str(":\n")?;
    write!(formatter, "{spantrace}")
}

/// Writes the root error, with color when enabled.
pub fn write_error(
    formatter: &mut fmt::Formatter<'_>,
    error: &(dyn Error + 'static),
) -> fmt::Result {
    #[cfg(feature = "color")]
    {
        write!(formatter, "{}", error.red().whenever(COLOR))
    }

    #[cfg(not(feature = "color"))]
    write!(formatter, "{error}")
}

/// Writes a section heading, with color when enabled.
#[cfg(feature = "pretty")]
fn write_heading(formatter: &mut fmt::Formatter<'_>, heading: &str) -> fmt::Result {
    #[cfg(feature = "color")]
    {
        write!(formatter, "{}", heading.bold().whenever(COLOR))
    }

    #[cfg(not(feature = "color"))]
    formatter.write_str(heading)
}
