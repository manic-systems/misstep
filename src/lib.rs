//! Errors with context that stays outside the source chain.
//!
//! [`Report`] owns an error and its context strings. Context changes the
//! rendered report, not the source chain reached through [`Report::chain`].
//!
//! A `Report<E>` keeps the concrete error type so callers can match on it,
//! and converts into the erased `Report` through `?`.
//!
//! `Report` deliberately does not implement [`Error`], which is what lets the
//! blanket `From` conversion exist and makes `?` work on any error type.

#[cfg(feature = "backtrace")] use std::backtrace;
#[cfg(feature = "location")] use std::panic::Location;
use std::{
    error::Error,
    fmt::{
        Debug,
        Display,
        Formatter,
        Result as FmtResult,
    },
    iter::FusedIterator,
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::Deref,
    ptr::NonNull,
    result,
};

mod fmt;
#[macro_use]
mod macros;

/// A `Result` whose error defaults to [`Report`].
pub type Result<T, E = Report> = result::Result<T, E>;

/// Adds context to a [`Result`].
pub trait ResultExt<T> {
    /// The error type the resulting [`Report`] keeps.
    type Error;

    /// Adds context to an error result.
    ///
    /// # Errors
    ///
    /// Converts the contained error into a [`Report`] of the same type and
    /// returns it with the context.
    fn context<C>(self, context: C) -> Result<T, Report<Self::Error>>
    where
        C: Display;

    /// Adds context from a closure.
    ///
    /// # Errors
    ///
    /// Converts the contained error into a [`Report`] of the same type and
    /// returns it with the context.
    fn with_context<C, F>(self, context: F) -> Result<T, Report<Self::Error>>
    where
        C: Display,
        F: FnOnce() -> C;

    /// Forgets the error type, for returning from a function that owes an
    /// erased [`Report`].
    ///
    /// # Errors
    ///
    /// Returns the contained error as an erased [`Report`].
    fn erase(self) -> Result<T>;

    /// Turns an error of type `R` into a value.
    ///
    /// Only the error at the start of the chain is checked, since only that
    /// one can be taken by value.
    ///
    /// # Errors
    ///
    /// Returns any other error as an erased [`Report`] with its context.
    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    fn recover<R, F>(self, handler: F) -> Result<T>
    where
        Self: Sized,
        R: Error + Send + Sync + 'static,
        F: FnOnce(R) -> T,
    {
        match self.erase() {
            Ok(value) => Ok(value),
            Err(report) => report.downcast::<R>().map(handler),
        }
    }

    /// Turns an error of type `R` into a value when `handler` returns one.
    ///
    /// Only the error at the start of the chain is checked, matching
    /// [`ResultExt::recover`].
    ///
    /// # Errors
    ///
    /// Returns the error as an erased [`Report`] with its context when it is
    /// not an `R` or `handler` returns [`None`].
    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    fn recover_if<R, F>(self, handler: F) -> Result<T>
    where
        Self: Sized,
        R: Error + 'static,
        F: FnOnce(&R) -> Option<T>,
    {
        let report = match self.erase() {
            Ok(value) => return Ok(value),
            Err(report) => report,
        };

        report
            .error()
            .downcast_ref::<R>()
            .and_then(handler)
            .ok_or(report)
    }
}

impl<T, E> ResultExt<T> for Result<T, E>
where
    E: Error + Send + Sync + 'static,
{
    type Error = E;

    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    fn context<C>(self, context: C) -> Result<T, Report<E>>
    where
        C: Display,
    {
        match self {
            Ok(value) => Ok(value),
            Err(error) => Err(Report::wrap(error).context(context)),
        }
    }

    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    fn with_context<C, F>(self, context: F) -> Result<T, Report<E>>
    where
        C: Display,
        F: FnOnce() -> C,
    {
        match self {
            Ok(value) => Ok(value),
            Err(error) => Err(Report::wrap(error).with_context(context)),
        }
    }

    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    fn erase(self) -> Result<T> {
        match self {
            Ok(value) => Ok(value),
            Err(error) => Err(Report::new(error)),
        }
    }
}

impl<T, E> ResultExt<T> for Result<T, Report<E>> {
    type Error = E;

    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    fn context<C>(self, context: C) -> Self
    where
        C: Display,
    {
        match self {
            Ok(value) => Ok(value),
            Err(report) => Err(report.context(context)),
        }
    }

    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    fn with_context<C, F>(self, context: F) -> Self
    where
        C: Display,
        F: FnOnce() -> C,
    {
        match self {
            Ok(value) => Ok(value),
            Err(report) => Err(report.with_context(context)),
        }
    }

    #[inline]
    fn erase(self) -> Result<T> {
        self.map_err(Report::erase)
    }
}

/// Turns a missing [`Option`] value into a [`Report`].
pub trait OptionExt<T> {
    /// Reports [`None`] with the context as the error message.
    ///
    /// # Errors
    ///
    /// Returns a [`Report`] carrying `context` when the option is [`None`].
    fn context<C>(self, context: C) -> Result<T>
    where
        C: Display;

    /// Reports [`None`] with a message from a closure.
    ///
    /// The closure runs only when the option is [`None`].
    ///
    /// # Errors
    ///
    /// Returns a [`Report`] carrying the closure's message when the option is
    /// [`None`].
    fn with_context<C, F>(self, context: F) -> Result<T>
    where
        C: Display,
        F: FnOnce() -> C;
}

impl<T> OptionExt<T> for Option<T> {
    #[inline]
    #[expect(
        clippy::option_if_let_else,
        reason = "a closure would hide the caller from track_caller"
    )]
    #[cfg_attr(feature = "location", track_caller)]
    fn context<C>(self, context: C) -> Result<T>
    where
        C: Display,
    {
        match self {
            Some(value) => Ok(value),
            None => Err(Report::msg(context)),
        }
    }

    #[inline]
    #[expect(
        clippy::option_if_let_else,
        reason = "a closure would hide the caller from track_caller"
    )]
    #[cfg_attr(feature = "location", track_caller)]
    fn with_context<C, F>(self, context: F) -> Result<T>
    where
        C: Display,
        F: FnOnce() -> C,
    {
        match self {
            Some(value) => Ok(value),
            None => Err(Report::msg(context())),
        }
    }
}

/// The error type of a [`Report`] that may hold any error.
#[derive(Debug)]
pub enum Dynamic {}

/// An error and the context collected while handling it.
///
/// Context does not wrap the error. Downcasts and [`Report::chain`] see only
/// the original error and its sources.
///
/// `Report<E>` holds an `E` and dereferences to it. The default, `Report`,
/// holds any error and supports runtime downcasting instead.
///
/// This type does not implement [`Error`]. Returning it from a function that
/// owes a `Box<dyn Error>` still works through the [`From`] impl below.
#[must_use]
pub struct Report<E = Dynamic> {
    /// The header of an [`Inner`] whose error type the header's vtable knows.
    /// One thin pointer keeps `Result<T, Report>` returnable in registers.
    header: NonNull<Header>,
    /// The error type, which the report owns through `header`.
    error:  PhantomData<E>,
}

// SAFETY: The allocation holds a `Header`, which is `Send + Sync`, and an
// error that is too. A typed report holds its `E`, and the erased constructors
// only accept `Send + Sync` errors.
unsafe impl<E> Send for Report<E> where E: Send + Sync {}
// SAFETY: See the `Send` impl.
unsafe impl<E> Sync for Report<E> where E: Send + Sync {}

const _: () = {
    const fn assert_send_sync<T>()
    where
        T: Send + Sync,
    {
    }
    assert_send_sync::<Header>();
};

/// The state every report has, whatever its error type.
struct Header {
    /// Operations that need the concrete error type.
    vtable:    &'static Vtable,
    /// Context in insertion order.
    contexts:  Vec<Context>,
    /// [`None`] for panic hook reports, whose message already names the panic
    /// site.
    #[cfg(feature = "location")]
    location:  Option<&'static Location<'static>>,
    /// The captured backtrace when capture succeeded.
    #[cfg(feature = "backtrace")]
    backtrace: Option<backtrace::Backtrace>,
    /// The captured span trace when capture succeeded.
    #[cfg(feature = "spantrace")]
    spantrace: Option<tracing_error::SpanTrace>,
}

/// The single allocation behind a [`Report`].
///
/// `repr(C)` keeps `header` at offset zero for every `E`, so a pointer to an
/// `Inner<E>` is also a pointer to its [`Header`].
#[repr(C)]
struct Inner<E> {
    /// State that does not depend on `E`.
    header: Header,
    /// The original error.
    error:  E,
}

/// Operations on an [`Inner`] whose error type was erased.
struct Vtable {
    /// Returns the error as a trait object.
    error: unsafe fn(NonNull<Header>) -> NonNull<dyn Error + Send + Sync + 'static>,
    /// Drops the error and the header and frees the allocation.
    drop:  unsafe fn(NonNull<Header>),
    /// Whether the allocation is an `Inner<BoxedError>`.
    boxed: bool,
}

/// A caller's boxed error, stored as-is since `Box<dyn Error>` is not an
/// [`Error`].
struct BoxedError(Box<dyn Error + Send + Sync + 'static>);

/// Returns the error of an `Inner<E>`.
///
/// # Safety
///
/// `header` must point to the header of a live `Inner<E>`.
unsafe fn error_of<E>(header: NonNull<Header>) -> NonNull<dyn Error + Send + Sync + 'static>
where
    E: Error + Send + Sync + 'static,
{
    // SAFETY: The caller guarantees `header` starts a live `Inner<E>`.
    let error: &(dyn Error + Send + Sync + 'static) =
        unsafe { &(*header.cast::<Inner<E>>().as_ptr()).error };
    NonNull::from(error)
}

/// Returns the error inside an `Inner<BoxedError>`.
///
/// # Safety
///
/// `header` must point to the header of a live `Inner<BoxedError>`.
unsafe fn boxed_error_of(header: NonNull<Header>) -> NonNull<dyn Error + Send + Sync + 'static> {
    // SAFETY: The caller guarantees `header` starts a live `Inner<BoxedError>`.
    let boxed = unsafe { &(*header.cast::<Inner<BoxedError>>().as_ptr()).error };
    NonNull::from(boxed.0.as_ref())
}

/// Drops and frees an `Inner<E>`.
///
/// # Safety
///
/// `header` must point to the header of an `Inner<E>` allocated by
/// [`Report::from_inner`], and nothing may use it afterwards.
unsafe fn drop_inner<E>(header: NonNull<Header>) {
    // SAFETY: The caller guarantees the allocation is an `Inner<E>` from a
    // leaked `Box` and that this is its last use.
    drop(unsafe { Box::from_raw(header.cast::<Inner<E>>().as_ptr()) });
}

impl<E> Drop for Report<E> {
    #[inline]
    fn drop(&mut self) {
        let drop_inner = self.header().vtable.drop;
        // SAFETY: The vtable matches the allocation, and the report is gone
        // after this call.
        unsafe {
            drop_inner(self.header);
        }
    }
}

impl Report {
    /// Stores an error in a new report that may hold any error.
    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    pub fn new<E>(error: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Report::wrap(error).into()
    }

    /// Stores a boxed error in a new report.
    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    pub fn from_boxed(error: Box<dyn Error + Send + Sync + 'static>) -> Self {
        let vtable = const {
            &Vtable {
                error: boxed_error_of,
                drop:  drop_inner::<BoxedError>,
                boxed: true,
            }
        };

        // SAFETY: Both vtable entries are written for `BoxedError`.
        unsafe { Self::from_inner(BoxedError(error), vtable) }
    }

    /// Builds a report from a displayable message.
    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    pub fn msg<D>(message: D) -> Self
    where
        D: Display,
    {
        Self::new(OwnedMessageError(message.to_string()))
    }

    /// Takes the error out if it is a `T`.
    ///
    /// Only the error at the start of the chain can be taken by value. Use
    /// [`Report::downcast_ref`] to search its sources.
    ///
    /// # Errors
    ///
    /// Returns the report unchanged when the error is not a `T`.
    #[inline]
    pub fn downcast<T>(self) -> Result<T, Self>
    where
        T: Error + Send + Sync + 'static,
    {
        if !self.error().is::<T>() {
            return Err(self);
        }

        let boxed = self.header().vtable.boxed;
        let header = ManuallyDrop::new(self).header;

        if boxed {
            // SAFETY: `boxed` is only set by `Report::from_boxed`, which
            // allocates an `Inner<BoxedError>`. The report went into
            // `ManuallyDrop`, so this box is the allocation's only owner.
            let inner = unsafe { Box::from_raw(header.cast::<Inner<BoxedError>>().as_ptr()) };
            let error = Box::into_raw(inner.error.0).cast::<T>();
            // SAFETY: `is::<T>` passed above, so this box holds a `T`.
            return Ok(*unsafe { Box::from_raw(error) });
        }

        // SAFETY: Every other constructor allocates an `Inner<E>`, and
        // `is::<T>` passed above, so `E` is `T`. `ManuallyDrop` leaves this
        // box as the only owner here too.
        let inner = unsafe { Box::from_raw(header.cast::<Inner<T>>().as_ptr()) };
        Ok(inner.error)
    }
}

impl<E> Report<E>
where
    E: Error + Send + Sync + 'static,
{
    /// Stores an error in a new report of its own type.
    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    fn wrap(error: E) -> Self {
        let vtable = const {
            &Vtable {
                error: error_of::<E>,
                drop:  drop_inner::<E>,
                boxed: false,
            }
        };

        // SAFETY: Both vtable entries are instantiated for `E`.
        unsafe { Self::from_inner(error, vtable) }
    }

    /// Takes the error out, dropping the context.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> E {
        let header = ManuallyDrop::new(self).header;
        // SAFETY: Only `Report::wrap` builds a `Report<E>` for an error type,
        // and it allocates an `Inner<E>`. `ManuallyDrop` keeps
        // `Report::drop` from freeing it again.
        let inner = unsafe { Box::from_raw(header.cast::<Inner<E>>().as_ptr()) };
        inner.error
    }
}

impl<E> Report<E> {
    /// Allocates an `Inner<T>` and keeps a pointer to its header.
    ///
    /// # Safety
    ///
    /// Every entry of `vtable` must be written for an `Inner<T>`, and `T` must
    /// be `E` unless `E` is [`Dynamic`].
    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    unsafe fn from_inner<T>(error: T, vtable: &'static Vtable) -> Self
    where
        T: Send + Sync + 'static,
    {
        let inner = Box::new(Inner {
            header: Header {
                vtable,
                contexts: Vec::new(),
                #[cfg(feature = "location")]
                location: Some(Location::caller()),
                #[cfg(feature = "backtrace")]
                backtrace: capture_backtrace(),
                #[cfg(feature = "spantrace")]
                spantrace: capture_spantrace(),
            },
            error,
        });

        Self {
            header: NonNull::from(Box::leak(inner)).cast(),
            error:  PhantomData,
        }
    }

    /// Forgets the error type.
    #[inline]
    pub fn erase(self) -> Report {
        Report {
            header: ManuallyDrop::new(self).header,
            error:  PhantomData,
        }
    }

    /// Borrows the header.
    #[inline]
    const fn header(&self) -> &Header {
        // SAFETY: The report owns a live allocation that starts with a header.
        unsafe { self.header.as_ref() }
    }

    /// Mutably borrows the header.
    #[inline]
    const fn header_mut(&mut self) -> &mut Header {
        // SAFETY: The report owns a live allocation that starts with a header,
        // and `&mut self` makes this borrow unique.
        unsafe { self.header.as_mut() }
    }

    /// Adds context without changing the source chain.
    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    pub fn context<D>(mut self, context: D) -> Self
    where
        D: Display,
    {
        self.header_mut().contexts.push(Context {
            message:                               context.to_string(),
            #[cfg(feature = "location")]
            location:                              Location::caller(),
        });
        self
    }

    /// Adds context returned by `context`.
    ///
    /// This method calls the closure immediately. [`ResultExt::with_context`]
    /// calls its closure only for an error result.
    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    pub fn with_context<C, F>(self, context: F) -> Self
    where
        C: Display,
        F: FnOnce() -> C,
    {
        self.context(context())
    }

    /// Returns the error at the start of the source chain.
    #[inline]
    #[must_use]
    pub fn error(&self) -> &(dyn Error + 'static) {
        let error_of = self.header().vtable.error;
        // SAFETY: The vtable matches the allocation.
        let error = unsafe { error_of(self.header) };
        // SAFETY: The error lives inside the allocation, which `self` keeps
        // alive and does not mutate while borrowed.
        unsafe { error.as_ref() }
    }

    /// Returns context in the order callers added it.
    ///
    /// Formatters print the newest context first.
    #[inline]
    #[must_use]
    pub fn contexts(&self) -> &[Context] {
        &self.header().contexts
    }

    /// Walks the error and each of its sources.
    ///
    /// Context is not part of this iterator.
    #[inline]
    pub fn chain(&self) -> Chain<'_> {
        Chain {
            current: Some(self.error()),
        }
    }

    /// Finds `T` in the source chain.
    #[inline]
    #[must_use]
    pub fn downcast_ref<T>(&self) -> Option<&T>
    where
        T: Error + 'static,
    {
        self.chain().find_map(|error| error.downcast_ref::<T>())
    }

    /// Returns where the report was created.
    ///
    /// Reports from the panic hook have no location here, since the panic
    /// message already carries it.
    #[cfg(feature = "location")]
    #[inline]
    #[must_use]
    pub const fn location(&self) -> Option<&'static Location<'static>> {
        self.header().location
    }

    /// Returns the captured backtrace, if capture was enabled and succeeded.
    #[cfg(feature = "backtrace")]
    #[inline]
    #[must_use]
    pub const fn backtrace(&self) -> Option<&backtrace::Backtrace> {
        self.header().backtrace.as_ref()
    }

    /// Returns the captured span trace, if one exists.
    #[cfg(feature = "spantrace")]
    #[inline]
    #[must_use]
    pub const fn spantrace(&self) -> Option<&tracing_error::SpanTrace> {
        self.header().spantrace.as_ref()
    }
}

impl<E> Deref for Report<E>
where
    E: Error + Send + Sync + 'static,
{
    type Target = E;

    #[inline]
    fn deref(&self) -> &E {
        // SAFETY: Only `Report::wrap` builds a `Report<E>` for an error type,
        // and it allocates an `Inner<E>` that `self` keeps alive.
        unsafe { &(*self.header.cast::<Inner<E>>().as_ptr()).error }
    }
}

impl<E> Display for Report<E> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        if f.alternate() {
            return fmt::write_error(f, self.error());
        }

        fmt::render_compact(self, f)
    }
}

impl<E> Debug for Report<E> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        #[cfg(feature = "pretty")]
        {
            fmt::render_pretty(self, f)
        }

        #[cfg(not(feature = "pretty"))]
        fmt::render_compact(self, f)
    }
}

impl<E> From<E> for Report
where
    E: Error + Send + Sync + 'static,
{
    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    fn from(error: E) -> Self {
        Self::new(error)
    }
}

impl<E> From<E> for Report<E>
where
    E: Error + Send + Sync + 'static,
{
    #[inline]
    #[cfg_attr(feature = "location", track_caller)]
    fn from(error: E) -> Self {
        Self::wrap(error)
    }
}

impl<E> From<Report<E>> for Report
where
    E: Error + Send + Sync + 'static,
{
    #[inline]
    fn from(report: Report<E>) -> Self {
        report.erase()
    }
}

impl<E> From<Report<E>> for Box<dyn Error + Send + Sync + 'static>
where
    E: Send + Sync + 'static,
{
    #[inline]
    fn from(report: Report<E>) -> Self {
        Box::new(BoxedReport(report))
    }
}

impl<E> From<Report<E>> for Box<dyn Error + 'static>
where
    E: Send + Sync + 'static,
{
    #[inline]
    fn from(report: Report<E>) -> Self {
        Box::new(BoxedReport(report))
    }
}

/// Gives a [`Report`] an [`Error`] impl for the boxing conversions.
struct BoxedReport<E>(Report<E>);

impl<E> Debug for BoxedReport<E> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Debug::fmt(&self.0, f)
    }
}

impl<E> Display for BoxedReport<E> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        Display::fmt(&self.0, f)
    }
}

impl<E> Error for BoxedReport<E> {
    #[inline]
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.error())
    }
}

/// A context message attached to a [`Report`].
#[derive(Debug)]
pub struct Context {
    /// The rendered message.
    message:  String,
    /// Where the context was added.
    #[cfg(feature = "location")]
    location: &'static Location<'static>,
}

impl Context {
    /// Returns the rendered message.
    #[inline]
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns where the context was added.
    #[cfg(feature = "location")]
    #[inline]
    #[must_use]
    pub const fn location(&self) -> &'static Location<'static> {
        self.location
    }
}

impl Display for Context {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(&self.message)
    }
}

/// A non-allocating iterator over an error and its sources.
#[derive(Clone, Debug)]
#[must_use]
pub struct Chain<'error> {
    /// The next error returned by the iterator.
    current: Option<&'error (dyn Error + 'static)>,
}

impl<'error> Iterator for Chain<'error> {
    type Item = &'error (dyn Error + 'static);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let error = self.current?;
        self.current = error.source();
        Some(error)
    }
}

impl FusedIterator for Chain<'_> {}

/// An owned string exposed as an error.
#[derive(Debug)]
struct OwnedMessageError(String);

impl Display for OwnedMessageError {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.write_str(&self.0)
    }
}

impl Error for OwnedMessageError {}

/// Captures an enabled backtrace.
#[cfg(feature = "backtrace")]
fn capture_backtrace() -> Option<backtrace::Backtrace> {
    let backtrace = backtrace::Backtrace::capture();
    (backtrace.status() == backtrace::BacktraceStatus::Captured).then_some(backtrace)
}

/// Captures an enabled span trace.
#[cfg(feature = "spantrace")]
fn capture_spantrace() -> Option<tracing_error::SpanTrace> {
    let spantrace = tracing_error::SpanTrace::capture();
    (spantrace.status() == tracing_error::SpanTraceStatus::CAPTURED).then_some(spantrace)
}

/// Installs a panic hook that writes a report before the prior hook runs.
///
/// This function is available with the `panic-hook` feature. It changes the
/// process-wide panic hook, so call it once during startup.
#[cfg(feature = "panic-hook")]
pub mod panic_hook {
    use std::{
        io::{
            self,
            Write as _,
        },
        panic::{
            self,
            PanicHookInfo,
        },
    };

    /// Installs the reporting hook and then calls the hook it replaced.
    #[inline]
    #[expect(
        clippy::use_debug,
        reason = "a panic prints the full report, which `pretty` renders through Debug"
    )]
    pub fn install() {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let report = panic_report(info);
            let mut stderr = io::stderr().lock();
            drop(writeln!(stderr, "{report:?}"));
            drop(stderr);

            previous(info);
        }));
    }

    /// Builds a report from a panic payload and its source location.
    fn panic_report(info: &PanicHookInfo<'_>) -> crate::Report {
        let payload = info.payload().downcast_ref::<&str>().map_or_else(
            || {
                info.payload()
                    .downcast_ref::<String>()
                    .map_or("non-string panic payload", String::as_str)
            },
            |message| *message,
        );

        let report = info.location().map_or_else(
            || crate::Report::msg(format_args!("panic: {payload}")),
            |location| {
                crate::Report::msg(format_args!(
                    "panic at {}:{}:{}: {payload}",
                    location.file(),
                    location.line(),
                    location.column()
                ))
            },
        );

        #[cfg(feature = "location")]
        {
            let mut unlocated = report;
            unlocated.header_mut().location = None;
            unlocated
        }

        #[cfg(not(feature = "location"))]
        report
    }
}

/// Installs the `panic-hook` feature's process-wide panic hook.
#[cfg(feature = "panic-hook")]
#[inline]
pub fn install_panic_hook() {
    panic_hook::install();
}
