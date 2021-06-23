pub mod synthetic_peers;

use std::time::Duration;

// Default timeout for connection reads in seconds.
pub const TIMEOUT: Duration = Duration::from_secs(10);

pub fn enable_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};

    fmt()
        .with_test_writer()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}

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

            // Default timout.
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
