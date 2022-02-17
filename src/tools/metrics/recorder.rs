//! Metrics recording types and utilities.

use std::{collections::HashMap, sync::Arc};

use metrics::{GaugeValue, Key, SetRecorderError, Unit};
use parking_lot::Mutex;

/// A counter metric.
#[derive(Default)]
pub struct Counter {
    pub value: u64,
    pub unit: Option<Unit>,
    pub description: Option<String>,
}

/// A gauge metric.
#[derive(Default)]
pub struct Gauge {
    pub value: f64,
    pub unit: Option<Unit>,
    pub description: Option<String>,
}

/// A histogram metric.
#[derive(Default)]
pub struct Histogram {
    pub value: histogram::Histogram,
    pub unit: Option<Unit>,
    pub description: Option<String>,
}

/// A simple [`metrics::Recorder`](https://docs.rs/metrics/0.16.0/metrics/trait.Recorder.html)
/// singleton implementation, that stores metrics for all registered counters, gauges and
/// histograms.
///
/// Attempts to update unregistered metrics are ignored and logged to `std::err`. These metrics can
/// then be retrieved via the [`counters`], [`gauges`] and [`histograms`] getters. This recorder is
/// enabled by calling [`enable_simple_recorder`].
#[derive(Default)]
pub struct SimpleRecorder {
    counters: Arc<Mutex<HashMap<Key, Counter>>>,
    gauges: Arc<Mutex<HashMap<Key, Gauge>>>,
    histograms: Arc<Mutex<HashMap<Key, Histogram>>>,
}

impl metrics::Recorder for SimpleRecorder {
    fn register_counter(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let new_counter = Counter {
            value: 0,
            unit,
            description: description.map(|str| str.to_owned()),
        };
        self.counters.lock().insert(key.clone(), new_counter);
    }

    fn register_gauge(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let new_gauge = Gauge {
            value: 0f64,
            unit,
            description: description.map(|str| str.to_owned()),
        };
        self.gauges.lock().insert(key.clone(), new_gauge);
    }

    fn register_histogram(&self, key: &Key, unit: Option<Unit>, description: Option<&'static str>) {
        let new_histogram = Histogram {
            value: histogram::Histogram::new(),
            unit,
            description: description.map(|str| str.to_owned()),
        };
        self.histograms.lock().insert(key.clone(), new_histogram);
    }

    fn increment_counter(&self, key: &Key, value: u64) {
        if let Some(counter) = self.counters.lock().get_mut(key) {
            match counter.value.checked_add(value) {
                Some(new_value) => counter.value = new_value,
                None => {
                    counter.value = u64::MAX;
                    eprintln!("Warning: counter {} saturated!", key);
                }
            }
        } else {
            eprintln!("Warning: counter {} not registered!", key);
        }
    }

    fn update_gauge(&self, key: &Key, value: GaugeValue) {
        if let Some(gauge) = self.gauges.lock().get_mut(key) {
            match value {
                GaugeValue::Absolute(new_value) => gauge.value = new_value,
                GaugeValue::Increment(inc) => gauge.value += inc,
                GaugeValue::Decrement(dec) => gauge.value -= dec,
            }
        } else {
            eprintln!("Warning: gauge {} not registered!", key);
        }
    }

    fn record_histogram(&self, key: &Key, value: f64) {
        assert!(value.is_sign_positive());
        assert!(value.is_finite());

        if let Some(histogram) = self.histograms.lock().get_mut(key) {
            // We know it cannot be negative, NaN or infinite so this is safe (albeit lossy)
            let value = value.round() as u64;
            // Can't pass on the Error here, so we will have to see if this becomes an issue.
            histogram.value.increment(value).unwrap();
        } else {
            eprintln!("Warning: histogram {} not registered!", key);
        }
    }
}

lazy_static::lazy_static! {
    static ref SIMPLE_RECORDER: SimpleRecorder = SimpleRecorder::default();
}

/// Enables the [`SimpleRecorder`] as the
/// [`metrics::Recorder`](https://docs.rs/metrics/0.16.0/metrics/trait.Recorder.html) sink.
pub fn enable_simple_recorder() -> Result<(), SetRecorderError> {
    // FIXME: This is a work-around while we don't have a test-runner
    //        which can set this globally. Currently we are calling this
    //        from every test which requires metrics. This will cause an
    //        error when called multiple times.
    //
    //        The correct implementation will pass on the result of metric::set_recorder
    //        instead of masking it.
    let _ = metrics::set_recorder(&*SIMPLE_RECORDER);
    Ok(())
}

/// Map of all counters recorded.
pub fn counters() -> Arc<Mutex<HashMap<Key, Counter>>> {
    SIMPLE_RECORDER.counters.clone()
}

/// Map of all gauges recorded.
pub fn gauges() -> Arc<Mutex<HashMap<Key, Gauge>>> {
    SIMPLE_RECORDER.gauges.clone()
}

/// Map of all histograms recorded.
pub fn histograms() -> Arc<Mutex<HashMap<Key, Histogram>>> {
    SIMPLE_RECORDER.histograms.clone()
}

/// Removes all previously registered metrics.
pub fn clear() {
    SIMPLE_RECORDER.counters.lock().clear();
    SIMPLE_RECORDER.gauges.lock().clear();
    SIMPLE_RECORDER.histograms.lock().clear();
}
