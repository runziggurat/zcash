//! Metrics recording types and utilities.

use std::collections::HashMap;

use histogram::Histogram;
use metrics::Key;
use metrics_util::{
    debugging::{DebugValue, DebuggingRecorder, Snapshotter},
    CompositeKey, MetricKind,
};

// This is a false positive, `CompositeKey` does not depend on any interior mutable
// types for its hashing implementation. Therefore, it is safe to use in our context.
#[allow(clippy::mutable_key_type)]
pub struct Snapshot(HashMap<CompositeKey, MetricVal>);

impl Snapshot {
    pub fn get_counter(&self, metric: &'static str) -> u64 {
        let key = CompositeKey::new(MetricKind::Counter, Key::from_name(metric));
        if let MetricVal::Counter(val) = *self.0.get(&key).unwrap() {
            val
        } else {
            0
        }
    }

    pub fn get_gauge(&self, metric: &'static str) -> f64 {
        let key = CompositeKey::new(MetricKind::Gauge, Key::from_name(metric));
        if let MetricVal::Gauge(val) = *self.0.get(&key).unwrap() {
            val
        } else {
            0.0
        }
    }

    pub fn get_histogram(&self, metric: &'static str) -> Option<Vec<f64>> {
        let key = CompositeKey::new(MetricKind::Histogram, Key::from_name(metric));
        if let MetricVal::Histogram(vals) = self.0.get(&key).unwrap() {
            Some(vals.to_vec())
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

pub struct TestMetrics(Snapshotter);

impl TestMetrics {
    fn new() -> TestMetrics {
        let recorder = DebuggingRecorder::new();
        let snapshotter = recorder.snapshotter();
        let _ = recorder.install();

        TestMetrics(snapshotter)
    }
}

impl Default for TestMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl TestMetrics {
    #[allow(clippy::mutable_key_type)]
    pub fn take_snapshot(&self) -> Snapshot {
        let mut snapshot = HashMap::new();

        for (key, val) in self.0.snapshot().into_hashmap().into_iter() {
            match val.2 {
                DebugValue::Counter(val) => snapshot.insert(key, MetricVal::Counter(val)),
                DebugValue::Gauge(val) => snapshot.insert(key, MetricVal::Gauge(val.into_inner())),
                DebugValue::Histogram(vals) => snapshot.insert(
                    key,
                    MetricVal::Histogram(vals.iter().map(|val| val.into_inner()).collect()),
                ),
            };
        }

        Snapshot(snapshot)
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
    const METRIC_NAME_ALT: &str = "test_metrics_alt";
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

        let snapshot = metrics.take_snapshot();

        assert_eq!(snapshot.get_counter(METRIC_NAME), COUNTER_INC);
    }

    #[test]
    #[ignore]
    fn can_get_gauge_value() {
        let metrics = TestMetrics::default();
        let gauge = register_gauge!(METRIC_NAME);

        gauge.set(1000.0);
        gauge.decrement(500.0);
        gauge.increment(25.0);

        let snapshot = metrics.take_snapshot();

        assert_eq!(snapshot.get_gauge(METRIC_NAME), 525.0);
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

        let snapshot = metrics.take_snapshot();

        assert_eq!(snapshot.get_histogram(METRIC_NAME), Some(values));
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

        let snapshot = metrics.take_snapshot();
        let constructed_histogram = snapshot.construct_histogram(METRIC_NAME).unwrap();

        assert!(constructed_histogram.entries() == 4);
        assert_eq!(constructed_histogram.percentile(50.0).unwrap(), 5);
        assert_eq!(constructed_histogram.percentile(90.0).unwrap(), 9);
    }

    #[test]
    #[ignore]
    fn can_construct_multiple_histograms() {
        let metrics = TestMetrics::default();
        let histogram = register_histogram!(METRIC_NAME);
        histogram.record(1.0);

        let histogram2 = register_histogram!(METRIC_NAME_ALT);
        histogram2.record(1.0);

        let snapshot = metrics.take_snapshot();
        let constructed_histogram = snapshot.construct_histogram(METRIC_NAME).unwrap();
        let constructed_histogram2 = snapshot.construct_histogram(METRIC_NAME_ALT).unwrap();

        assert!(constructed_histogram.entries() == 1);
        assert!(constructed_histogram2.entries() == 1);
    }
}
