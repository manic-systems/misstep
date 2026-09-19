use std::cell::Cell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use misstep::{Report, Result, ResultExt};

#[derive(Debug)]
enum NestedError {
    Leaf,
    Wrap {
        label: &'static str,
        source: Box<NestedError>,
    },
}

impl NestedError {
    fn leaf() -> Self {
        Self::Leaf
    }

    fn wrap(label: &'static str, source: Self) -> Self {
        Self::Wrap {
            label,
            source: Box::new(source),
        }
    }
}

impl Display for NestedError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Leaf => formatter.write_str("leaf"),
            Self::Wrap { label, .. } => formatter.write_str(label),
        }
    }
}

impl Error for NestedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Leaf => None,
            Self::Wrap { source, .. } => Some(source.as_ref()),
        }
    }
}

#[derive(Debug)]
struct TypedError;

impl Display for TypedError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("typed failure")
    }
}

impl Error for TypedError {}

#[test]
fn display_and_debug_render_reports_identically() {
    let report = Report::msg("failure").context("while processing");

    assert_eq!(format!("{report}"), format!("{report:?}"));
}

#[cfg(feature = "pretty")]
#[test]
fn pretty_formatting_renders_error_cause_and_context_sections() {
    let report = Report::new(NestedError::wrap("root", NestedError::leaf()))
        .context("while processing input");

    let display = format!("{report}");
    let debug = format!("{report:?}");

    assert_eq!(display, debug);
    assert!(display.contains("Error"));
    assert!(display.contains("Caused by"));
    assert!(display.contains("Context"));
    assert!(display.contains("root"));
    assert!(display.contains("leaf"));
    assert!(display.contains("while processing input"));
}

#[cfg(feature = "color")]
#[test]
fn color_formatting_uses_ansi_and_matches_debug() {
    let report = Report::msg("failure");

    let display = format!("{report}");
    let debug = format!("{report:?}");

    assert_eq!(display, debug);
    assert!(display.contains("\x1b["));
}

#[test]
fn chain_iterates_nested_sources_without_context_entries() {
    let report = Report::new(NestedError::wrap(
        "root",
        NestedError::wrap("middle", NestedError::leaf()),
    ))
    .context("diagnostic context");

    let chain = report.chain().map(ToString::to_string).collect::<Vec<_>>();

    assert_eq!(chain, ["root", "middle", "leaf"]);
}

#[test]
fn context_is_rendered_and_original_error_can_be_downcast() {
    let report = Report::new(TypedError).context("while reading configuration");

    let rendered = report.to_string();
    assert!(rendered.contains("while reading configuration"));
    assert!(rendered.contains("typed failure"));
    assert!(report.downcast_ref::<TypedError>().is_some());
}

#[test]
fn result_ext_with_context_is_lazy() {
    let calls = Cell::new(0);

    let success: std::result::Result<_, TypedError> = Ok(7);
    let value = success.with_context(|| {
        calls.set(calls.get() + 1);
        "unused context"
    });
    assert_eq!(value.expect("success should remain successful"), 7);
    assert_eq!(calls.get(), 0);

    let failure: std::result::Result<(), TypedError> = Err(TypedError);
    let report = failure
        .with_context(|| {
            calls.set(calls.get() + 1);
            "operation failed"
        })
        .expect_err("failure should become a report");
    assert_eq!(calls.get(), 1);
    assert!(report.to_string().contains("operation failed"));
}

fn bail_result() -> Result<(), Report> {
    misstep::bail!("explicit failure");
}

fn ensure_result(calls: &Cell<u8>) -> Result<(), Report> {
    misstep::ensure!(
        {
            calls.set(calls.get() + 1);
            false
        },
        "condition failed",
    );
    Ok(())
}

fn bail_into_boxed_error() -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
    misstep::bail!("boxed failure");
}

fn ensure_into_boxed_error() -> std::result::Result<(), Box<dyn Error + Send + Sync>> {
    misstep::ensure!(false, "boxed condition failed");
    Ok(())
}

#[test]
fn bail_and_ensure_convert_reports_to_boxed_errors() {
    let bail_error = bail_into_boxed_error().expect_err("bail should return a boxed error");
    assert!(bail_error.to_string().contains("boxed failure"));

    let ensure_error = ensure_into_boxed_error().expect_err("ensure should return a boxed error");
    assert!(ensure_error.to_string().contains("boxed condition failed"));
}

#[test]
fn result_ext_context_accepts_borrowed_string_slice() {
    let context = String::from("borrowed operation context");
    let failure: std::result::Result<(), TypedError> = Err(TypedError);

    let report = failure
        .context(context.as_str())
        .expect_err("failure should become a report");

    assert!(report.to_string().contains("borrowed operation context"));
}

#[test]
fn bail_and_ensure_return_reports_and_ensure_evaluates_once() {
    let bail_error = bail_result().expect_err("bail should return an error");
    assert!(bail_error.to_string().contains("explicit failure"));

    let calls = Cell::new(0);
    let ensure_error = ensure_result(&calls).expect_err("ensure should return an error");
    assert_eq!(calls.get(), 1);
    assert!(ensure_error.to_string().contains("condition failed"));
}
