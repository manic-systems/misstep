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
`Display` always renders a compact one-line form, so a report can sit inside a
log line or a larger message. `Debug` matches it by default. Under the `pretty`
feature, `Debug` renders a multi-section form instead, which is what Rust prints
for an error returned from `main`. Feature flags select the format at compile
time, so there is no runtime formatter to register and no hook other code can
silently replace.

## Features

- `color` adds ANSI color with `yansi`.
- `backtrace` captures `std::backtrace::Backtrace` when a report is created.
- `spantrace` captures `tracing_error::SpanTrace` when a report is created.
- `pretty` renders `Debug` as error, cause, context, and available trace
  sections.
- `panic-hook` adds `install_panic_hook()`. The hook prints a report, then calls
  the hook that was already installed.

> [!TIP]
> Misstep stays small by gating its optional dependencies behind feature flags,
> so every feature is opt-in and the default feature set pulls in no
> dependencies at all. `pretty` works without `color`. `backtrace` and
> `spantrace` capture their data without `pretty`, but do not print it.

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
}

fn find_port(configuration: &str) -> Result<u16> {
    let port = configuration.split('=').last().context("no port in configuration")?;
    parse_port(port)
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

Printing a report renders its context with the newest context first, followed by
the error and its source chain.

## License

<!-- markdownlint-disable MD059 -->

[here]: https://interoperable-europe.ec.europa.eu/sites/default/files/custom-page/attachment/eupl_v1.2_en.pdf

This project is made available under European Union Public Licence (EUPL)
version 1.2. See [LICENSE](LICENSE) for more details on the exact conditions. An
online copy is provided [here].

<!-- markdownlint-enable MD059 -->
