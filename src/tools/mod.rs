//! Utilities for network testing.

pub mod crawler;
pub mod fuzzing;
pub mod message_filter;
pub mod metrics;
pub mod synthetic_node;

use std::time::Duration;

/// Default timeout for connection operations in seconds.
/// TODO: move to config file.
pub const LONG_TIMEOUT: Duration = Duration::from_secs(10);
/// Default timeout for response-specific reads in seconds.
pub const RECV_TIMEOUT: Duration = Duration::from_millis(100);

/// Waits until an expression is true or times out.
///
/// Uses polling to cut down on time otherwise used by calling `sleep` in tests.
#[macro_export]
macro_rules! wait_until {
    ($wait_limit: expr, $condition: expr $(, $sleep_duration: expr)?) => {
        let now = std::time::Instant::now();
        loop {
            if $condition {
                break;
            }

            // Default timeout.
            let sleep_duration = std::time::Duration::from_millis(10);
            // Set if present in args.
            $(let sleep_duration = $sleep_duration;)?
            tokio::time::sleep(sleep_duration).await;
            if now.elapsed() > $wait_limit {
                panic!("timed out!");
            }
        }
    };
}
