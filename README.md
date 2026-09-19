# misstep

Light weight crate reports Rust errors without a required dependency or a global
formatter hook.

`Report` owns the error and keeps context strings beside it. Context appears in
the output, but it does not alter `Error::source()`. Use `Report::chain()` when
you need to walk the original error chain.

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
use misstep::{bail, ensure, Result, ResultExt};

fn read_configuration(path: &str) -> Result<String> {
    let configuration = std::fs::read_to_string(path)
        .context("reading configuration")?;
    ensure!(!configuration.is_empty(), "configuration is empty");
    Ok(configuration)
}

fn run() -> Result<()> {
    let configuration = read_configuration("config.toml")?;
    if configuration.starts_with("disabled") {
        bail!("configuration disabled this service");
    }
    Ok(())
}
```
