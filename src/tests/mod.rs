mod conformance;
mod resistance;

/// Shorthand for `assert!(matches!(val, pattern), args..)`
///
/// Useful for asserting a enum matches a specific variant.
///
/// Usage:
///     `assert_matches!(value, MyEnum::Variant(..));`
///
/// or with additional context:
///     `assert_matches!(value, MyEnum::Variant(..), "Additional {}", "context");`
#[macro_export]
macro_rules! assert_matches {
    ($value:expr, $pattern:pat $(,)?) => {
        assert_matches!($value, $pattern, "");
    };
    ($value:expr, $pattern:pat, $($arg:tt)*) => {{
        assert!(matches!($value, $pattern),
r#"assert_matches!({}, {})
left: {:?}
right: {} : {}"#,
            stringify!($value), stringify!($pattern), $value, stringify!($pattern), $($arg)*);
    }}
}
