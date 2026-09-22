//! Macros for building and returning [`Report`](crate::Report) values.

/// Builds a [`Report`](crate::Report).
///
/// A single string literal becomes a message. Add arguments after that literal
/// to format it. Any other single expression becomes the wrapped error.
#[macro_export]
macro_rules! report {
    ($message:literal $(,)?) => {
        $crate::Report::msg(::core::format_args!($message))
    };
    ($format:literal, $($arguments:tt)+) => {
        $crate::Report::msg(::core::format_args!($format, $($arguments)+))
    };
    ($error:expr $(,)?) => {
        $crate::Report::from_error($error)
    };
}

/// Returns a report from the current function.
///
/// The macro accepts the same forms as [`report!`]. It formats arguments only
/// on this path.
#[macro_export]
macro_rules! bail {
    ($($arguments:tt)+) => {
        return ::core::result::Result::Err($crate::report!($($arguments)+).into())
    };
}

/// Returns a report unless a condition is true.
///
/// The condition runs once. Message formatting runs only when it is false.
#[macro_export]
macro_rules! ensure {
    ($condition:expr $(,)?) => {
        if !$condition {
            $crate::bail!("condition failed: {}", ::core::stringify!($condition));
        }
    };
    ($condition:expr, $($arguments:tt)+) => {
        if !$condition {
            $crate::bail!($($arguments)+);
        }
    };
}
