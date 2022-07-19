//! Metrics recording types and utilities.

use histogram::Histogram;
use metrics::Key;
use metrics_util::{
    debugging::{DebugValue, DebuggingRecorder, Snapshotter},
    CompositeKey, MetricKind,
};

pub fn initialize() -> Snapshotter {
    let recorder = DebuggingRecorder::new();
    let snapshotter = recorder.snapshotter();
    let _ = recorder.install();

    snapshotter
}

pub struct TestMetrics(Snapshotter);

impl Default for TestMetrics {
    fn default() -> Self {
        Self(initialize())
    }
}

impl TestMetrics {
    pub fn get_val_for(&self, kind: MetricKind, metric: &'static str) -> MetricVal {
        let key = CompositeKey::new(kind, Key::from_name(metric));

        match &self.0.snapshot().into_hashmap().get(&key).unwrap().2 {
            DebugValue::Counter(val) => MetricVal::Counter(*val),
            DebugValue::Gauge(val) => MetricVal::Gauge(val.into_inner()),
            DebugValue::Histogram(vals) => {
                MetricVal::Histogram(vals.iter().map(|val| val.into_inner()).collect())
            }
        }
    }

    pub fn get_counter(&self, metric: &'static str) -> u64 {
        if let MetricVal::Counter(val) = self.get_val_for(MetricKind::Counter, metric) {
            val
        } else {
            0
        }
    }

    pub fn get_gauge(&self, metric: &'static str) -> f64 {
        if let MetricVal::Gauge(val) = self.get_val_for(MetricKind::Gauge, metric) {
            val
        } else {
            0.0
        }
    }

    pub fn get_histogram(&self, metric: &'static str) -> Option<Vec<f64>> {
        if let MetricVal::Histogram(vals) = self.get_val_for(MetricKind::Histogram, metric) {
            Some(vals)
        } else {
            None
        }
    }

    pub fn construct_histogram(&self, metric: &'static str) -> Option<Histogram> {
        if let Some(metric_histogram) = self.get_histogram(metric) {
            let mut histogram = Histogram::new();

            for value in metric_histogram.iter() {
                let _ = histogram.increment(value.round() as u64);
            }

            Some(histogram)
        } else {
            None
        }
    }
}

impl Drop for TestMetrics {
    fn drop(&mut self) {
        // Clear the recorder to avoid the global state bleeding into other tests.
        // Safety: this is ok since it is only ever used in tests that are to be run sequentially
        // on one thread.
        unsafe {
            metrics::clear_recorder();
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum MetricVal {
    Counter(u64),
    Gauge(f64),
    Histogram(Vec<f64>),
}

#[cfg(test)]
mod tests {
    use metrics::{register_counter, register_gauge, register_histogram};

    use super::*;

    const METRIC_NAME: &str = "test_metrics";
    const COUNTER_INC: u64 = 25;
    const HISTOGRAM_SIZE: usize = 50;

    #[test]
    #[ignore]
    fn can_initialize_metrics() {
        let _ = TestMetrics::default();
    }

    #[test]
    #[ignore]
    fn can_get_counter_value() {
        let metrics = TestMetrics::default();
        let counter = register_counter!(METRIC_NAME);

        counter.increment(COUNTER_INC);

        assert_eq!(metrics.get_counter(METRIC_NAME), COUNTER_INC);
    }

    #[test]
    #[ignore]
    fn can_get_gauge_value() {
        let metrics = TestMetrics::default();
        let gauge = register_gauge!(METRIC_NAME);

        gauge.set(1000.0);
        gauge.decrement(500.0);
        gauge.increment(25.0);

        assert_eq!(metrics.get_gauge(METRIC_NAME), 525.0);
    }

    #[test]
    #[ignore]
    fn can_get_histogram_values() {
        let metrics = TestMetrics::default();
        let histogram = register_histogram!(METRIC_NAME);

        let mut values = Vec::with_capacity(HISTOGRAM_SIZE);
        for i in 0..HISTOGRAM_SIZE {
            histogram.record(i as f64);
            values.push(i as f64);
        }

        assert_eq!(metrics.get_histogram(METRIC_NAME), Some(values));
    }

    #[test]
    #[ignore]
    fn can_construct_histogram() {
        let metrics = TestMetrics::default();
        let histogram = register_histogram!(METRIC_NAME);

        histogram.record(1.0);
        histogram.record(3.0);
        histogram.record(5.0);
        histogram.record(9.0);

        let constructed_histogram = metrics.construct_histogram(METRIC_NAME).unwrap();

        assert!(constructed_histogram.entries() == 4);
        assert_eq!(constructed_histogram.percentile(50.0).unwrap(), 5);
        assert_eq!(constructed_histogram.percentile(90.0).unwrap(), 9);
    }
}
