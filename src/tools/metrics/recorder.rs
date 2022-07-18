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
