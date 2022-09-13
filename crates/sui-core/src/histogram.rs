use futures::FutureExt;
use prometheus::{
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry, IntCounterVec,
    IntGaugeVec, Registry,
};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::error;

type Point = u64;
type HistogramMessage = (HistogramLabels, Point);

#[derive(Clone)]
pub struct Histogram {
    labels: HistogramLabels,
    channel: mpsc::UnboundedSender<HistogramMessage>,
}

pub struct HistogramTimerGuard<'a> {
    histogram: &'a Histogram,
    start: Instant,
}

#[derive(Clone)]
pub struct HistogramVec {
    channel: mpsc::UnboundedSender<HistogramMessage>,
}

struct HistogramReporter {
    gauge: IntGaugeVec,
    sum: IntCounterVec,
    count: IntCounterVec,
    percentiles: Arc<Vec<usize>>,
    channel: mpsc::UnboundedReceiver<HistogramMessage>,
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
        Self::new(gauge, sum, count, percentiles)
    }

    // Do not expose it to public interface because we need labels to have a specific format (e.g. add last label is "pct")
    fn new(
        gauge: IntGaugeVec,
        sum: IntCounterVec,
        count: IntCounterVec,
        percentiles: Vec<usize>,
    ) -> Self {
        // The processing task is very fast and should not have realistic scenario where this channel overflows
        // If the channel is growing too fast you will start seeing error! from the
        // data points being dropped way before it will produce any measurable memory pressure
        #[allow(clippy::disallowed_methods)]
        let (sender, receiver) = mpsc::unbounded_channel();
        let percentiles = Arc::new(percentiles);
        let reporter = HistogramReporter {
            gauge,
            sum,
            count,
            percentiles,
            channel: receiver,
        };
        Handle::current().spawn(reporter.run());
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
    pub fn report(&self, v: Point) {
        if self.channel.send((self.labels.clone(), v)).is_err() {
            unreachable!("Histogram reporting task do not stop when Histogram instance exists");
        }
    }

    pub fn start_timer(&self) -> HistogramTimerGuard {
        HistogramTimerGuard {
            histogram: self,
            start: Instant::now(),
        }
    }
}

impl HistogramReporter {
    pub async fn run(mut self) {
        let mut deadline = Instant::now();
        loop {
            // We calculate deadline here instead of just using sleep inside cycle to avoid accumulating error
            deadline += Duration::from_secs(1);
            if self.cycle(deadline).await.is_err() {
                return;
            }
        }
    }

    async fn cycle(&mut self, deadline: Instant) -> Result<(), ()> {
        let mut labeled_data: HashMap<HistogramLabels, Vec<Point>> = HashMap::new();
        let mut timeout = tokio::time::sleep_until(deadline).boxed();
        loop {
            tokio::select! {
             _ = &mut timeout => {
                break;
            },
                point = self.channel.recv() => {
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
        if labeled_data.is_empty() {
            return Ok(());
        }
        if Arc::strong_count(&self.percentiles) != 1 {
            // Not processing new data point if we have not finished processing previous
            error!("Histogram data overflow - we receive histogram data faster then can process. Some histogram data is dropped")
        } else {
            let percentiles = self.percentiles.clone();
            let gauge = self.gauge.clone();
            let sum = self.sum.clone();
            let count = self.count.clone();
            // Histogram calculation can be CPU intensive, running in tokio blocking thread pool
            Handle::current()
                .spawn_blocking(move || Self::report(percentiles, gauge, sum, count, labeled_data));
        }
        Ok(())
    }

    fn report(
        percentiles: Arc<Vec<usize>>,
        gauge: IntGaugeVec,
        sum_counter: IntCounterVec,
        count_counter: IntCounterVec,
        labeled_data: HashMap<HistogramLabels, Vec<Point>>,
    ) {
        for (label, mut data) in labeled_data {
            assert!(!data.is_empty());
            data.sort_unstable();
            for pct1000 in percentiles.iter() {
                let index = Self::pct1000_index(data.len(), *pct1000);
                let point = *data.get(index).unwrap();
                let pct_str = Self::format_pct1000(*pct1000);
                let labels = label.labels.iter().map(|s| &s[..]).chain([&pct_str[..]]);
                let labels: Vec<_> = labels.collect();
                let metric = gauge.with_label_values(&labels);
                metric.set(point as i64);
            }
            let mut sum = 0u64;
            let count = data.len() as u64;
            for point in data {
                sum += point;
            }
            let labels: Vec<_> = label.labels.iter().map(|s| &s[..]).collect();
            sum_counter.with_label_values(&labels).inc_by(sum);
            count_counter.with_label_values(&labels).inc_by(count);
        }
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
}
