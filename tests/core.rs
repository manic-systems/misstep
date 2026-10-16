use std::{
    cell::Cell,
    error::Error,
    fmt::{
        self,
        Display,
        Formatter,
    },
    result,
};

use misstep::{
    OptionExt as _,
    Report,
    Result,
    ResultExt as _,
};

#[derive(Debug)]
enum NestedError {
    Leaf,
    Wrap {
        label:  &'static str,
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

#[cfg(not(feature = "color"))]
#[test]
fn display_renders_compact_report_with_newest_context_first() {
    let report = Report::new(NestedError::wrap("root", NestedError::leaf()))
        .context("inner")
        .context("outer");

    assert_eq!(format!("{report}"), "outer: inner: root: leaf");
}

#[cfg(all(not(feature = "pretty"), not(feature = "color")))]
#[test]
fn debug_matches_compact_display_without_pretty_formatting() {
    let report = Report::msg("failure").context("while processing");

    assert_eq!(format!("{report:?}"), "while processing: failure");
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
    assert!(!debug.lines().next().unwrap_or_default().contains("Error"));
    assert!(debug.contains("Caused by"));
    assert!(debug.contains("Context"));
    assert!(debug.contains("root"));
    assert!(debug.contains("leaf"));
    assert!(debug.contains("while processing input"));
}

#[cfg(all(
    feature = "pretty",
    not(feature = "color"),
    not(feature = "backtrace"),
    not(feature = "spantrace"),
    not(feature = "location")
))]
#[test]
fn pretty_debug_renders_sections_in_order() {
    let report = Report::new(NestedError::wrap("root", NestedError::leaf()))
        .context("inner")
        .context("outer");

    assert_eq!(
        format!("{report:?}"),
        "root\n\nCaused by:\n  0: leaf\n\nContext:\n  0: outer\n  1: inner"
    );
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

#[test]
fn option_ext_with_context_is_lazy() {
    let calls = Cell::new(0_u8);

    let value = Some(7_i32).with_context(|| {
        calls.set(calls.get() + 1_u8);
        "unused context"
    });
    assert_eq!(value.ok(), Some(7_i32));
    assert_eq!(calls.get(), 0_u8);

    let missing: Option<i32> = None;
    let result = missing.with_context(|| {
        calls.set(calls.get() + 1_u8);
        "missing value"
    });
    assert!(result.is_err(), "absence should become a report");
    let Err(report) = result else { return };

    assert_eq!(calls.get(), 1_u8);
    assert!(report.to_string().contains("missing value"));
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

#[test]
fn downcast_takes_the_root_error_or_returns_the_report() {
    let report = Report::new(TypedError).context("kept");

    let unchanged = report
        .downcast::<fmt::Error>()
        .expect_err("not a fmt::Error");
    assert_eq!(unchanged.contexts().len(), 1);

    unchanged
        .downcast::<TypedError>()
        .expect("root is a TypedError");
}

#[test]
fn boxed_reports_expose_and_release_the_boxed_error() {
    let boxed: Box<dyn Error + Send + Sync> = Box::new(TypedError);
    let report = Report::from_boxed(boxed).context("while loading");

    assert_eq!(report.contexts()[0].message(), "while loading");
    assert!(report.downcast_ref::<TypedError>().is_some());

    let unchanged = report
        .downcast::<fmt::Error>()
        .expect_err("not a fmt::Error");
    unchanged
        .downcast::<TypedError>()
        .expect("boxed error is a TypedError");
}

fn typed_bail() -> Result<(), Report<TypedError>> {
    misstep::bail!(TypedError);
}

#[test]
fn typed_reports_keep_their_error_through_context_and_bail() {
    let failure: result::Result<(), TypedError> = Err(TypedError);
    let report: Report<TypedError> = failure.context("typed context").expect_err("failed");

    assert_eq!(report.contexts()[0].message(), "typed context");
    assert_eq!((*report).to_string(), "typed failure");

    let TypedError = report.into_inner();
    let TypedError = typed_bail().expect_err("bail returns").into_inner();
}

#[test]
fn erasing_keeps_context_and_allows_downcasting() {
    let failure: result::Result<(), TypedError> = Err(TypedError);
    let report = failure
        .context("before erasing")
        .erase()
        .expect_err("failed");

    assert_eq!(report.contexts()[0].message(), "before erasing");
    report
        .downcast::<TypedError>()
        .expect("erasing keeps the error");
}

#[test]
fn context_on_a_report_appends_instead_of_wrapping() {
    let failure: Result<()> = Err(Report::new(TypedError).context("inner"));
    let report = failure.context("outer").expect_err("failed");

    let contexts = report
        .contexts()
        .iter()
        .map(misstep::Context::message)
        .collect::<Vec<_>>();
    assert_eq!(contexts, ["inner", "outer"]);
    assert_eq!(report.chain().count(), 1);
}

fn read_typed(fail: bool) -> Result<u8> {
    if fail {
        return Err(Report::new(TypedError).context("reading"));
    }
    Ok(1)
}

#[test]
fn recover_replaces_only_the_requested_error() {
    assert_eq!(
        read_typed(true).recover::<TypedError, _>(|_| 2).ok(),
        Some(2)
    );
    assert_eq!(
        read_typed(false).recover::<TypedError, _>(|_| 2).ok(),
        Some(1)
    );

    let report = read_typed(true)
        .recover::<fmt::Error, _>(|_| 2)
        .expect_err("other type");
    assert!(report.downcast_ref::<TypedError>().is_some());
    assert_eq!(report.contexts()[0].message(), "reading");
}

#[test]
fn recover_if_passes_the_report_through_when_declined() {
    assert_eq!(
        read_typed(true)
            .recover_if::<TypedError, _>(|_| Some(3))
            .ok(),
        Some(3)
    );

    let declined = read_typed(true)
        .recover_if::<TypedError, _>(|_| None)
        .expect_err("declined");
    assert!(declined.downcast_ref::<TypedError>().is_some());
    assert_eq!(declined.contexts()[0].message(), "reading");

    let other = read_typed(true)
        .recover_if::<fmt::Error, _>(|_| Some(3))
        .expect_err("other");
    assert_eq!(other.contexts().len(), 1);
}

#[cfg(not(feature = "color"))]
#[test]
fn alternate_display_renders_only_the_root_error() {
    let report = Report::new(NestedError::wrap("root", NestedError::leaf())).context("hidden");

    assert_eq!(format!("{report:#}"), "root");

    let boxed: Box<dyn Error> = report.into();
    assert_eq!(format!("{boxed:#}"), "root");
}

#[test]
fn boxed_conversion_exposes_the_original_error_as_source() {
    let boxed: Box<dyn Error + Send + Sync> = Report::new(TypedError).context("ctx").into();

    let source = boxed.source().expect("report source");
    assert!(source.is::<TypedError>());
}

#[test]
fn option_ext_context_becomes_the_message() {
    let report = None::<u8>.context("nothing here").expect_err("absent");

    assert_eq!(report.error().to_string(), "nothing here");
    assert!(report.contexts().is_empty());
}

#[cfg(feature = "location")]
#[test]
fn location_points_at_the_caller() {
    let failure: result::Result<(), TypedError> = Err(TypedError);
    let report = failure.context("located").expect_err("failed");

    let location = report.location().expect("created outside the panic hook");
    assert!(location.file().ends_with("core.rs"));
    assert_eq!(report.contexts()[0].location().line(), location.line());
}
