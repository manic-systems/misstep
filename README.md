# misstep

misstep is a lightweight Rust crate for error reporting. It attaches context to
errors without a required dependency, a global formatter hook, or any change to
the original error chain.

## Design

Adding context usually means wrapping the error. Wrapping changes what
`source()` returns, makes downcasting harder, and leaves the last handler to
unwind a chain of context layers before it reaches the real cause. Misstep takes
a different approach: a `Report` owns the error and keeps context strings beside
it. Context appears in the output, but it does not alter the source chain. Use
`Report::chain()` when you need to walk the original error chain.

`Report` does not implement `Error`. That is what allows the blanket
`From<E: Error + Send + Sync + 'static>` conversion, so `?` promotes any error
into a report. A boxed error goes through `Report::from_boxed()`, since a `From`
impl for it would overlap the blanket one. Going the other way, a report
converts into `Box<dyn Error>`, so `bail!` still works in a function that owes
one.

Formatting writes directly to the caller's buffer and holds no global state.
`Display` renders a compact one-line form, so a report can sit inside a log
line or a larger message. `{:#}` renders only the error, without context or
sources, for messages shown to users. `Debug` matches the compact form by
default. Under the `pretty` feature, `Debug` renders a multi-section form
instead, which is what Rust prints for an error returned from `main`. Feature
flags select the format at compile time, so there is no runtime formatter to
register and no hook other code can silently replace.

## Features

- `std`, on by default, is needed by `color`, `backtrace`, `spantrace`, and
  `panic-hook`. Without it misstep builds as `no_std` with `alloc`.
- `color` adds ANSI color with `yansi` when stderr is a terminal, honoring
  `NO_COLOR` and `CLICOLOR`.
- `backtrace` captures `std::backtrace::Backtrace` when a report is created.
- `spantrace` captures `tracing_error::SpanTrace` when a report is created.
- `pretty` renders `Debug` as error, cause, context, and available trace
  sections.
- `location` records where each report was created and where each context was
  added, and `pretty` prints them. It stores one pointer per entry instead of
  walking the stack, and works where backtraces do not, such as wasm.
- `panic-hook` adds `install_panic_hook()`. The hook prints a report, then calls
  the hook that was already installed.

> [!TIP]
> Misstep stays small by gating its optional dependencies behind feature flags,
> so the default feature set pulls in no dependencies at all. `pretty` works
> without `color`. `backtrace` and `spantrace` capture their data without
> `pretty`, but do not print it.

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
`OptionExt` turns a `None` into a report whose message is the context. Use
`with_context` when building the message should wait until an error actually
occurs:

```rust
use misstep::{OptionExt, Result, ResultExt};

fn parse_port(port: &str) -> Result<u16> {
    port.trim()
        .parse::<u16>()
        .with_context(|| format!("failed to parse {port:?}"))
        .erase()
}

fn find_port(configuration: &str) -> Result<u16> {
    let port = configuration
        .split_once('=')
        .map(|(_, port)| port)
        .context("no port in configuration")?;
    parse_port(port)
}
```

A report keeps the type of the error it was built from, so calling `context` on
a `Result<T, io::Error>` gives you a `Result<T, Report<io::Error>>`. The report
derefs to its error, so `report.kind()` below calls straight through, and
`into_inner` hands the error back by value. `?` turns a typed report into a
plain `Report` for you, but returning one directly skips that conversion.
`parse_port` above calls `erase` for that reason.

```rust
use std::{fs, io};

use misstep::{Report, Result, ResultExt};

fn read_configuration(path: &str) -> Result<String, Report<io::Error>> {
    fs::read_to_string(path).context("reading configuration")
}

fn configuration_or_default(path: &str) -> Result<String> {
    match read_configuration(path) {
        Ok(configuration) => Ok(configuration),
        Err(report) if report.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(report) => Err(report.into()),
    }
}
```

Some errors aren't really failures, like a missing config file you'd rather
treat as the defaults. `recover` is for those. Name the error type you expect
and give it a handler that turns that error into a value, and anything else
still comes back with its context attached. `recover_if` only borrows the
error, so the handler gets a look first and can return `None` to let it
through. Both check just the error at the start of the chain, since that's the
one `recover` can take by value.

```rust
use std::{fs, io};

use misstep::{Result, ResultExt};

fn read_port(path: &str) -> Result<u16> {
    let port = fs::read_to_string(path).context("reading the port")?;
    Ok(port.trim().parse()?)
}

fn port_or_default(path: &str) -> Result<u16> {
    read_port(path).recover_if::<io::Error, _>(|error| {
        (error.kind() == io::ErrorKind::NotFound).then_some(8080)
    })
}
```

The `report!` macro builds a report from a message or an existing error. Because
a report converts into `Box<dyn Error>`, a function returning a boxed error can
still use `bail!` and `report!`:

```rust
use misstep::{bail, report};

fn serve(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    if port == 0 {
        bail!("port 0 is not routable");
    }
    Err(report!("listening on port {} failed", port).into())
}
```

`ReportError` wraps a report in a type that implements `Error`, so a report can
be the source of your own error type. Its `Display` shows only the context and
its `source` is the original error:

```rust
use std::{error::Error, fmt};

use misstep::{ReportError, Result};

#[derive(Debug)]
struct StartupError(ReportError);

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("startup failed")
    }
}

impl Error for StartupError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

fn start(load: impl FnOnce() -> Result<()>) -> Result<(), StartupError> {
    load().map_err(|report| StartupError(report.into()))
}
```

Printing a report renders its context with the newest context first, followed by
the error and its source chain.

## License

<!-- markdownlint-disable MD059 -->

[here]: https://interoperable-europe.ec.europa.eu/sites/default/files/custom-page/attachment/eupl_v1.2_en.pdf

This project is made available under European Union Public Licence (EUPL)
version 1.2. See [LICENSE](LICENSE) for more details on the exact conditions. An
online copy is provided [here].

<!-- markdownlint-enable MD059 -->
