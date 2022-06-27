//! Tables to display metrics.

use histogram::Histogram;
use tabled::{object::Segment, Alignment, Modify, Style, Table, Tabled};
use tokio::time::Duration;

/// Provides a simplified interface to produce a well-formatted table for latency statistics.
///
/// Table can be displayed by `println!("{}", table)`
#[derive(Default)]
pub struct RequestsTable {
    rows: Vec<RequestStats>,
}

/// Commonly used request statistics.
#[derive(Tabled)]
pub struct RequestStats {
    #[tabled(rename = " peers ")]
    peers: u16,
    #[tabled(rename = " requests ")]
    requests: u16,
    #[tabled(rename = " min (ms) ")]
    latency_min: u16,
    #[tabled(rename = " max (ms) ")]
    latency_max: u16,
    #[tabled(rename = " std dev (ms) ")]
    latency_std_dev: u16,
    #[tabled(rename = " 10% (ms) ")]
    latency_percentile_10: u16,
    #[tabled(rename = " 50% (ms) ")]
    latency_percentile_50: u16,
    #[tabled(rename = " 75% (ms) ")]
    latency_percentile_75: u16,
    #[tabled(rename = " 90% (ms) ")]
    latency_percentile_90: u16,
    #[tabled(rename = " 99% (ms) ")]
    latency_percentile_99: u16,
    #[tabled(rename = " completion % ")]
    #[tabled(display_with = "table_float_display")]
    completion: f64,
    #[tabled(rename = " time (s) ")]
    #[tabled(display_with = "table_float_display")]
    time: f64,
    #[tabled(rename = " requests/s ")]
    #[tabled(display_with = "table_float_display")]
    throughput: f64,
}

impl RequestStats {
    pub fn new(peers: u16, requests: u16, latencies: Histogram, time: f64) -> Self {
        Self {
            peers,
            requests,
            completion: (latencies.entries() as f64) / (peers as f64 * requests as f64) * 100.00,
            latency_min: latencies.minimum().unwrap() as u16,
            latency_max: latencies.maximum().unwrap() as u16,
            latency_std_dev: latencies.stddev().unwrap() as u16,
            latency_percentile_10: latencies.percentile(10.0).unwrap() as u16,
            latency_percentile_50: latencies.percentile(50.0).unwrap() as u16,
            latency_percentile_75: latencies.percentile(75.0).unwrap() as u16,
            latency_percentile_90: latencies.percentile(90.0).unwrap() as u16,
            latency_percentile_99: latencies.percentile(99.0).unwrap() as u16,
            time,
            throughput: latencies.entries() as f64 / time,
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
        f.write_str(&fmt_table(Table::new(&self.rows)))
    }
}

/// Formats `f64` with 2 decimal points.
pub fn table_float_display(x: &f64) -> String {
    format!("{0:.2}", x)
}

/// Returns the duration converted to milliseconds.
pub fn duration_as_ms(duration: Duration) -> f64 {
    duration.as_millis() as f64
}

/// Formats a table with our custom style.
///
/// Modifications:
///  - [pseudo style](https://docs.rs/tabled/0.2.1/tabled/style/struct.Style.html#method.pseudo)
///  - centered headers
///  - right aligned data
pub fn fmt_table(table: Table) -> String {
    // table with pseudo style, right aligned data and center aligned headers
    table
        .with(Style::modern())
        .with(Modify::new(Segment::all()).with(Alignment::right()))
        .to_string()
}
