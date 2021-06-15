mod blocks;
mod connections;
mod ping;

use tokio::time::Duration;
use histogram::Histogram;
use tabled::{table, Alignment, Style, Tabled};

/// Provides a simplified interface to producde a well-formatted
/// table for latency statistics. Table can be displayed by `println!("{}", table)`
#[derive(Default)]
pub struct RequestsTable {
    rows: Vec<RequestStats>,
}

#[derive(Tabled)]
pub struct RequestStats {
    #[header(" peers ")]
    peers: u16,
    #[header(" requests ")]
    requests: u16,
    #[header(" min (ms) ")]
    latency_min: u16,
    #[header(" max (ms) ")]
    latency_max: u16,
    #[header(" std dev (ms) ")]
    latency_std_dev: u16,
    #[header(" 10% (ms) ")]
    latency_percentile_10: u16,
    #[header(" 50% (ms) ")]
    latency_percentile_50: u16,
    #[header(" 75% (ms) ")]
    latency_percentile_75: u16,
    #[header(" 90% (ms) ")]
    latency_percentile_90: u16,
    #[header(" 99% (ms) ")]
    latency_percentile_99: u16,
    #[header(" time (s) ")]
    #[field(display_with = "table_float_display")]
    time: f64,
    #[header(" requests/s ")]
    #[field(display_with = "table_float_display")]
    throughput: f64,
}

impl RequestStats {
    pub fn new(peers: u16, requests: u16, latencies: Histogram, time: f64) -> Self {
        Self {
            peers,
            requests,
            latency_min: latencies.minimum().unwrap() as u16,
            latency_max: latencies.maximum().unwrap() as u16,
            latency_std_dev: latencies.stddev().unwrap() as u16,
            latency_percentile_10: latencies.percentile(10.0).unwrap() as u16,
            latency_percentile_50: latencies.percentile(50.0).unwrap() as u16,
            latency_percentile_75: latencies.percentile(75.0).unwrap() as u16,
            latency_percentile_90: latencies.percentile(90.0).unwrap() as u16,
            latency_percentile_99: latencies.percentile(99.0).unwrap() as u16,
            time,
            throughput: requests as f64 * peers as f64 / time,
        }
    }
}

impl RequestsTable {
    pub fn add_row(&mut self, row: RequestStats) {
        self.rows.push(row);
    }
}

impl std::fmt::Display for RequestsTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&table!(
            &self.rows,
            Style::pseudo(),
            Alignment::center_vertical(tabled::Full),
            Alignment::right(tabled::Column(..)),
            Alignment::center_horizontal(tabled::Head),
        ))
    }
}

/// Formats f64 with 2 decimal points
pub fn table_float_display(x: &f64) -> String {
    format!("{0:.2}", x)
}


pub fn duration_as_ms(duration: Duration) -> f64 {
    duration.as_millis() as f64
}