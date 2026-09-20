# misstep

Light weight crate reports Rust errors without a required dependency or a global
formatter hook.

`Report` owns the error and keeps context strings beside it. Context appears in
the output, but it does not alter the source chain. Use `Report::chain()` when
you need to walk the original error chain.

`Report` does not implement `Error`. That is what allows the blanket
`From<E: Error + Send + Sync + 'static>` conversion, so `?` promotes any error
into a report. A boxed error goes through `Report::from_boxed()`, since a `From`
impl for it would overlap the blanket one. Going the other way, a report
converts into `Box<dyn Error>`, so `bail!` still works in a function that owes
one.

## Features

- `color` adds ANSI color with `yansi`.
- `backtrace` captures `std::backtrace::Backtrace` when a report is created.
- `spantrace` captures `tracing_error::SpanTrace` when a report is created.
- `pretty` prints error, cause, context, and available trace sections.
- `panic-hook` adds `install_panic_hook()`. The hook prints a report, then calls
  the hook that was already installed.

Misstep aims to stay small by gating dependencies (which there aren't too many)
behind feature flags. Which means for you that every feature is optional and
opt-in. `pretty` works without `color`. `backtrace` and `spantrace` capture
their data without `pretty`, but do not print it.

## Usage

```rust
use misstep::{bail, ensure, OptionExt, Result, ResultExt};

fn read_configuration(path: &str) -> Result<String> {
    let configuration = std::fs::read_to_string(path)
        .context("reading configuration")?;
    ensure!(!configuration.is_empty(), "configuration is empty");
    Ok(configuration)
}

fn run() -> Result<()> {
    let configuration = read_configuration("config.toml")
        .context("starting the service")?;

    let port: u16 = configuration
        .strip_prefix("port=")
        .context("configuration has no port")?
        .parse()?;

    if port == 0 {
        bail!("port 0 is not routable");
    }
    Ok(())
}
```

`ResultExt` adds context to any `Result`. When the error is already a `Report`
the context joins the existing list rather than wrapping a report in a report.
`OptionExt` turns a `None` into a report whose message is the context.
