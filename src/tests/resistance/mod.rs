mod corrupt_message;
mod random_bytes;
mod stress_test;
mod zeroes;

use std::time::Duration;

const ITERATIONS: usize = 50;
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(5);
