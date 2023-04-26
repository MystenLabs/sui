// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::monitored_scope;
use futures::FutureExt;
use parking_lot::Mutex;
use prometheus::{
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry, IntCounterVec,
    IntGaugeVec, Registry,
};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::time::Instant;
use tracing::{debug, error};

type Point = u64;
type HistogramMessage = (HistogramLabels, Point);

#[derive(Clone)]
pub struct Histogram {
    labels: HistogramLabels,
    channel: mpsc::Sender<HistogramMessage>,
}

pub struct HistogramTimerGuard<'a> {
    histogram: &'a Histogram,
    start: Instant,
}

#[derive(Clone)]
pub struct HistogramVec {
    channel: mpsc::Sender<HistogramMessage>,
}

struct HistogramCollector {
    reporter: Arc<Mutex<HistogramReporter>>,
    channel: mpsc::Receiver<HistogramMessage>,
    _name: String,
}

struct HistogramReporter {
    gauge: IntGaugeVec,
    sum: IntCounterVec,
    count: IntCounterVec,
    known_labels: HashSet<HistogramLabels>,
    percentiles: Vec<usize>,
}

type HistogramLabels = Arc<HistogramLabelsInner>;

struct HistogramLabelsInner {
    labels: Vec<String>,
    hash: u64,
}

/// Reports the histogram to the given prometheus gauge.
/// Unlike the histogram from prometheus crate, this histogram does not require to specify buckets
/// It works by calculating 'true' histogram by aggregating and sorting values.
///
/// The values are reported into prometheus gauge with requested labels and additional dimension
/// for the histogram percentile.
///
/// It worth pointing out that due to those more precise calculations, this Histogram usage
/// is somewhat more limited comparing to original prometheus Histogram.
///
/// On the bright side, this histogram exports less data to Prometheus comparing to prometheus::Histogram,
/// it exports each requested percentile into separate prometheus gauge, while original implementation creates
/// gauge per bucket.
/// It also exports _sum and _count aggregates same as original implementation.
///
/// It is ok to measure timings for things like network latencies and expensive crypto operations.
/// However as a rule of thumb this histogram should not be used in places that can produce very high data point count.
///
/// As a last round of defence this histogram emits error log when too much data is flowing in and drops data points.
///
/// This implementation puts great deal of effort to make sure the metric does not cause any harm to the code itself:
/// * Reporting data point is a non-blocking send to a channel
/// * Data point collections tries to clear the channel as fast as possible
/// * Expensive histogram calculations are done in a separate blocking tokio thread pool to avoid effects on main scheduler
/// * If histogram data is produced too fast, the data is dropped and error! log is emitted
impl HistogramVec {
    pub fn new_in_registry(name: &str, desc: &str, labels: &[&str], registry: &Registry) -> Self {
        Self::new_in_registry_with_percentiles(
            name,
            desc,
            labels,
            registry,
            vec![500usize, 950, 990],
        )
    }

    /// Allows to specify percentiles in 1/1000th, e.g. 90pct is specified as 900
    pub fn new_in_registry_with_percentiles(
        name: &str,
        desc: &str,
        labels: &[&str],
        registry: &Registry,
        percentiles: Vec<usize>,
    ) -> Self {
        let sum_name = format!("{}_sum", name);
        let count_name = format!("{}_count", name);
        let sum =
            register_int_counter_vec_with_registry!(sum_name, desc, labels, registry).unwrap();
        let count =
            register_int_counter_vec_with_registry!(count_name, desc, labels, registry).unwrap();
        let labels: Vec<_> = labels.iter().cloned().chain(["pct"].into_iter()).collect();
        let gauge = register_int_gauge_vec_with_registry!(name, desc, &labels, registry).unwrap();
        Self::new(gauge, sum, count, percentiles, name)
    }

    // Do not expose it to public interface because we need labels to have a specific format (e.g. add last label is "pct")
    fn new(
        gauge: IntGaugeVec,
        sum: IntCounterVec,
        count: IntCounterVec,
        percentiles: Vec<usize>,
        name: &str,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(1000);
        let reporter = HistogramReporter {
            gauge,
            sum,
            count,
            percentiles,
            known_labels: Default::default(),
        };
        let reporter = Arc::new(Mutex::new(reporter));
        let collector = HistogramCollector {
            reporter,
            channel: receiver,
            _name: name.to_string(),
        };
        Handle::current().spawn(collector.run());
        Self { channel: sender }
    }

    pub fn with_label_values(&self, labels: &[&str]) -> Histogram {
        let labels = labels.iter().map(ToString::to_string).collect();
        let labels = HistogramLabelsInner::new(labels);
        Histogram {
            labels,
            channel: self.channel.clone(),
        }
    }
}

impl HistogramLabelsInner {
    pub fn new(labels: Vec<String>) -> HistogramLabels {
        // Not a crypto hash
        let mut hasher = DefaultHasher::new();
        labels.hash(&mut hasher);
        let hash = hasher.finish();
        Arc::new(Self { labels, hash })
    }
}

impl PartialEq for HistogramLabelsInner {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl Eq for HistogramLabelsInner {}

impl Hash for HistogramLabelsInner {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.hash.hash(state)
    }
}

impl Histogram {
    pub fn new_in_registry(name: &str, desc: &str, registry: &Registry) -> Self {
        HistogramVec::new_in_registry(name, desc, &[], registry).with_label_values(&[])
    }

    pub fn report(&self, v: Point) {
        match self.channel.try_send((self.labels.clone(), v)) {
            Ok(()) => {}
            Err(TrySendError::Closed(_)) => {
                // can happen during runtime shutdown
            }
            Err(TrySendError::Full(_)) => debug!("Histogram channel is full, dropping data"),
        }
    }

    pub fn start_timer(&self) -> HistogramTimerGuard {
        HistogramTimerGuard {
            histogram: self,
            start: Instant::now(),
        }
    }
}

impl HistogramCollector {
    pub async fn run(mut self) {
        let mut deadline = Instant::now();
        loop {
            // We calculate deadline here instead of just using sleep inside cycle to avoid accumulating error
            #[cfg(test)]
            const HISTOGRAM_WINDOW_SEC: u64 = 1;
            #[cfg(not(test))]
            const HISTOGRAM_WINDOW_SEC: u64 = 60;
            deadline += Duration::from_secs(HISTOGRAM_WINDOW_SEC);
            if self.cycle(deadline).await.is_err() {
                return;
            }
        }
    }

    async fn cycle(&mut self, deadline: Instant) -> Result<(), ()> {
        let mut labeled_data: HashMap<HistogramLabels, Vec<Point>> = HashMap::new();
        let mut count = 0usize;
        let mut timeout = tokio::time::sleep_until(deadline).boxed();
        const MAX_POINTS: usize = 500_000;
        loop {
            tokio::select! {
             _ = &mut timeout => {
                break;
            },
                point = self.channel.recv() => {
                    count += 1;
                    if count > MAX_POINTS {
                        continue;
                    }
                    if let Some((label, point)) = point {
                        let values = labeled_data.entry(label).or_default();
                        values.push(point);
                    } else {
                        // Histogram no longer exists
                        return Err(());
                    }
                }
                }
        }
        if count > MAX_POINTS {
            error!(
                "Too many data points for histogram, dropping {} points",
                count - MAX_POINTS
            );
        }
        if Arc::strong_count(&self.reporter) != 1 {
            #[cfg(not(debug_assertions))]
            error!("Histogram data overflow - we receive histogram data for {} faster then can process. Some histogram data is dropped", self._name);
        } else {
            let reporter = self.reporter.clone();
            Handle::current().spawn_blocking(move || reporter.lock().report(labeled_data));
        }
        Ok(())
    }
}

impl HistogramReporter {
    pub fn report(&mut self, labeled_data: HashMap<HistogramLabels, Vec<Point>>) {
        let _scope = monitored_scope("HistogramReporter::report");
        let mut reset_labels = self.known_labels.clone();
        for (label, mut data) in labeled_data {
            self.known_labels.insert(label.clone());
            reset_labels.remove(&label);
            assert!(!data.is_empty());
            data.sort_unstable();
            for pct1000 in self.percentiles.iter() {
                let index = Self::pct1000_index(data.len(), *pct1000);
                let point = *data.get(index).unwrap();
                let pct_str = Self::format_pct1000(*pct1000);
                let labels = Self::gauge_labels(&label, &pct_str);
                let metric = self.gauge.with_label_values(&labels);
                metric.set(point as i64);
            }
            let mut sum = 0u64;
            let count = data.len() as u64;
            for point in data {
                sum += point;
            }
            let labels: Vec<_> = label.labels.iter().map(|s| &s[..]).collect();
            self.sum.with_label_values(&labels).inc_by(sum);
            self.count.with_label_values(&labels).inc_by(count);
        }

        for reset_label in reset_labels {
            for pct1000 in self.percentiles.iter() {
                let pct_str = Self::format_pct1000(*pct1000);
                let labels = Self::gauge_labels(&reset_label, &pct_str);
                let metric = self.gauge.with_label_values(&labels);
                metric.set(0);
            }
        }
    }

    fn gauge_labels<'a>(label: &'a HistogramLabels, pct_str: &'a str) -> Vec<&'a str> {
        let labels = label.labels.iter().map(|s| &s[..]).chain([pct_str]);
        labels.collect()
    }

    /// Returns value in range [0; len)
    fn pct1000_index(len: usize, pct1000: usize) -> usize {
        len * pct1000 / 1000
    }

    fn format_pct1000(pct1000: usize) -> String {
        format!("{}", (pct1000 as f64) / 10.)
    }
}

impl<'a> Drop for HistogramTimerGuard<'a> {
    fn drop(&mut self) {
        self.histogram
            .report(self.start.elapsed().as_millis() as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus::proto::MetricFamily;

    #[test]
    fn pct_index_test() {
        assert_eq!(200, HistogramReporter::pct1000_index(1000, 200));
        assert_eq!(100, HistogramReporter::pct1000_index(500, 200));
        assert_eq!(1800, HistogramReporter::pct1000_index(2000, 900));
        // Boundary checks
        assert_eq!(21, HistogramReporter::pct1000_index(22, 999));
        assert_eq!(0, HistogramReporter::pct1000_index(1, 999));
        assert_eq!(0, HistogramReporter::pct1000_index(1, 100));
        assert_eq!(0, HistogramReporter::pct1000_index(1, 1));
    }

    #[test]
    fn format_pct1000_test() {
        assert_eq!(HistogramReporter::format_pct1000(999), "99.9");
        assert_eq!(HistogramReporter::format_pct1000(990), "99");
        assert_eq!(HistogramReporter::format_pct1000(900), "90");
    }

    #[tokio::test]
    async fn histogram_test() {
        let registry = Registry::new();
        let histogram = HistogramVec::new_in_registry_with_percentiles(
            "test",
            "xx",
            &["lab"],
            &registry,
            vec![500, 900],
        );
        let a = histogram.with_label_values(&["a"]);
        let b = histogram.with_label_values(&["b"]);
        a.report(1);
        a.report(2);
        a.report(3);
        a.report(4);
        b.report(10);
        b.report(20);
        b.report(30);
        b.report(40);
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let gather = registry.gather();
        let gather: HashMap<_, _> = gather
            .into_iter()
            .map(|f| (f.get_name().to_string(), f))
            .collect();
        let hist = gather.get("test").unwrap();
        let sum = gather.get("test_sum").unwrap();
        let count = gather.get("test_count").unwrap();
        let hist = aggregate_gauge_by_label(hist);
        let sum = aggregate_counter_by_label(sum);
        let count = aggregate_counter_by_label(count);
        assert_eq!(Some(3.), hist.get("::a::50").cloned());
        assert_eq!(Some(4.), hist.get("::a::90").cloned());
        assert_eq!(Some(30.), hist.get("::b::50").cloned());
        assert_eq!(Some(40.), hist.get("::b::90").cloned());

        assert_eq!(Some(10.), sum.get("::a").cloned());
        assert_eq!(Some(100.), sum.get("::b").cloned());

        assert_eq!(Some(4.), count.get("::a").cloned());
        assert_eq!(Some(4.), count.get("::b").cloned());
    }

    fn aggregate_gauge_by_label(family: &MetricFamily) -> HashMap<String, f64> {
        family
            .get_metric()
            .iter()
            .map(|m| {
                let value = m.get_gauge().get_value();
                let mut key = String::new();
                for label in m.get_label() {
                    key.push_str("::");
                    key.push_str(label.get_value());
                }
                (key, value)
            })
            .collect()
    }

    fn aggregate_counter_by_label(family: &MetricFamily) -> HashMap<String, f64> {
        family
            .get_metric()
            .iter()
            .map(|m| {
                let value = m.get_counter().get_value();
                let mut key = String::new();
                for label in m.get_label() {
                    key.push_str("::");
                    key.push_str(label.get_value());
                }
                (key, value)
            })
            .collect()
    }
}
