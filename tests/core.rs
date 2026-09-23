use std::cell::Cell;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::result;

use misstep::{Report, Result, ResultExt as _};

#[derive(Debug)]
enum NestedError {
    Leaf,
    Wrap {
        label: &'static str,
        source: Box<Self>,
    },
}

impl NestedError {
    const fn leaf() -> Self {
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
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Leaf => f.write_str("leaf"),
            Self::Wrap { label, .. } => f.write_str(label),
        }
    }
}

impl Error for NestedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match *self {
            Self::Leaf => None,
            Self::Wrap { ref source, .. } => Some(source.as_ref()),
        }
    }
}

#[derive(Debug)]
struct TypedError;

impl Display for TypedError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("typed failure")
    }
}

impl Error for TypedError {}

#[cfg(not(feature = "pretty"))]
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

    assert!(!display.contains('\n'));
    assert!(display.contains("while processing input"));
    assert!(debug.contains("Error"));
    assert!(debug.contains("Caused by"));
    assert!(debug.contains("Context"));
    assert!(debug.contains("root"));
    assert!(debug.contains("leaf"));
    assert!(debug.contains("while processing input"));
}

#[cfg(feature = "color")]
#[test]
fn color_formatting_uses_ansi() {
    let report = Report::msg("failure");

    assert!(format!("{report}").contains("\x1b["));
    assert!(format!("{report:?}").contains("\x1b["));
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
    let calls = Cell::new(0_u8);

    let success: result::Result<_, TypedError> = Ok(7_i32);
    let value = success.with_context(|| {
        calls.set(calls.get() + 1_u8);
        "unused context"
    });
    assert_eq!(value.ok(), Some(7_i32));
    assert_eq!(calls.get(), 0_u8);

    let failure: result::Result<(), TypedError> = Err(TypedError);
    let result = failure.with_context(|| {
        calls.set(calls.get() + 1_u8);
        "operation failed"
    });
    assert!(result.is_err(), "failure should become a report");
    let Err(report) = result else { return };

    assert_eq!(calls.get(), 1_u8);
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

fn bail_into_boxed_error() -> result::Result<(), Box<dyn Error + Send + Sync>> {
    misstep::bail!("boxed failure");
}

fn ensure_into_boxed_error() -> result::Result<(), Box<dyn Error + Send + Sync>> {
    misstep::ensure!(false, "boxed condition failed");
    Ok(())
}

#[test]
fn bail_and_ensure_convert_reports_to_boxed_errors() {
    let bail_result = bail_into_boxed_error();
    assert!(bail_result.is_err(), "bail should return a boxed error");
    let Err(bail_error) = bail_result else { return };

    assert!(bail_error.to_string().contains("boxed failure"));

    let ensure_result = ensure_into_boxed_error();
    assert!(ensure_result.is_err(), "ensure should return a boxed error");
    let Err(ensure_error) = ensure_result else {
        return;
    };

    assert!(ensure_error.to_string().contains("boxed condition failed"));
}

#[test]
fn result_ext_context_accepts_borrowed_string_slice() {
    let context = String::from("borrowed operation context");
    let failure: result::Result<(), TypedError> = Err(TypedError);

    let result = failure.context(context.as_str());
    assert!(result.is_err(), "failure should become a report");
    let Err(report) = result else { return };

    assert!(report.to_string().contains("borrowed operation context"));
}

#[test]
fn bail_and_ensure_return_reports_and_ensure_evaluates_once() {
    let bail_result = bail_result();
    assert!(bail_result.is_err(), "bail should return an error");
    let Err(bail_error) = bail_result else { return };

    assert!(bail_error.to_string().contains("explicit failure"));

    let calls = Cell::new(0_u8);
    let ensure_result = ensure_result(&calls);
    assert!(ensure_result.is_err(), "ensure should return an error");
    let Err(ensure_error) = ensure_result else {
        return;
    };

    assert_eq!(calls.get(), 1_u8);
    assert!(ensure_error.to_string().contains("condition failed"));
}
