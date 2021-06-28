mod connections;
mod getdata_blocks;
mod ping_pong;

use histogram::Histogram;
use tabled::{Alignment, Modify, Style, Table, Tabled};
use tokio::time::Duration;

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
    #[header(" completion % ")]
    #[field(display_with = "table_float_display")]
    completion: f64,
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

/// Formats f64 with 2 decimal points
pub fn table_float_display(x: &f64) -> String {
    format!("{0:.2}", x)
}

pub fn duration_as_ms(duration: Duration) -> f64 {
    duration.as_millis() as f64
}

/// Formats a [Table] with our style:
///  - [pseudo style](Style) (todo - fix this link)
///  - centered headers
///  - right aligned data
pub fn fmt_table(table: Table) -> String {
    // table with pseudo style, right aligned data and center aligned headers
    table
        .with(Style::pseudo())
        .with(Modify::new(tabled::Full).with(Alignment::right()))
        .with(Modify::new(tabled::Head).with(Alignment::center_horizontal()))
        .to_string()
}
