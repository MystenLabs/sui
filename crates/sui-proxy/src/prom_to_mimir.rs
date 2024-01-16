// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::remote_write;
use crate::var;
use itertools::Itertools;
use prometheus::proto::{Counter, Gauge, Histogram, Metric, MetricFamily, MetricType};
use protobuf::RepeatedField;
use tracing::{debug, error};

#[derive(Debug)]
pub struct Mimir<S> {
    state: S,
}

impl From<&Metric> for Mimir<RepeatedField<remote_write::Label>> {
    fn from(m: &Metric) -> Self {
        // we consume metric labels from an owned version so we can sort them
        let mut m = m.to_owned();
        let mut sorted = m.take_label();
        sorted.sort_by(|a, b| {
            (a.get_name(), a.get_value())
                .partial_cmp(&(b.get_name(), b.get_value()))
                .unwrap()
        });
        let mut r = RepeatedField::<remote_write::Label>::default();
        for label in sorted {
            let lp = remote_write::Label {
                name: label.get_name().into(),
                value: label.get_value().into(),
            };
            r.push(lp);
        }
        Self { state: r }
    }
}

impl IntoIterator for Mimir<RepeatedField<remote_write::Label>> {
    type Item = remote_write::Label;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.state.into_iter()
    }
}

impl From<&Counter> for Mimir<remote_write::Sample> {
    fn from(c: &Counter) -> Self {
        Self {
            state: remote_write::Sample {
                value: c.get_value(),
                ..Default::default()
            },
        }
    }
}
impl From<&Gauge> for Mimir<remote_write::Sample> {
    fn from(c: &Gauge) -> Self {
        Self {
            state: remote_write::Sample {
                value: c.get_value(),
                ..Default::default()
            },
        }
    }
}
impl Mimir<remote_write::Sample> {
    fn sample(self) -> remote_write::Sample {
        self.state
    }
}

/// TODO implement histogram
impl From<&Histogram> for Mimir<remote_write::Histogram> {
    fn from(_h: &Histogram) -> Self {
        Self {
            state: remote_write::Histogram::default(),
        }
    }
}
/// TODO implement histogram
impl Mimir<remote_write::Histogram> {
    #[allow(dead_code)]
    fn histogram(self) -> remote_write::Histogram {
        self.state
    }
}
impl From<Vec<MetricFamily>> for Mimir<Vec<remote_write::WriteRequest>> {
    fn from(metric_families: Vec<MetricFamily>) -> Self {
        // we may have more but we'll have at least this many timeseries
        let mut timeseries: Vec<remote_write::TimeSeries> =
            Vec::with_capacity(metric_families.len());

        for mf in metric_families {
            // TOOD add From impl
            let mt = match mf.get_field_type() {
                MetricType::COUNTER => remote_write::metric_metadata::MetricType::Counter,
                MetricType::GAUGE => remote_write::metric_metadata::MetricType::Gauge,
                MetricType::HISTOGRAM => remote_write::metric_metadata::MetricType::Histogram,
                MetricType::SUMMARY => remote_write::metric_metadata::MetricType::Summary,
                MetricType::UNTYPED => remote_write::metric_metadata::MetricType::Unknown,
            };

            // filter out the types we don't support
            match mt {
                remote_write::metric_metadata::MetricType::Counter
                | remote_write::metric_metadata::MetricType::Gauge => (),
                other => {
                    debug!("{:?} is not yet implemented, skipping metric", other);
                    continue;
                }
            }

            // TODO stop using state directly
            timeseries.extend(Mimir::from(mf.clone()).state);
        }

        Self {
            state: timeseries
                .into_iter()
                // the upstream remote_write should have a max sample size per request set to this number
                .chunks(var!("MIMIR_MAX_SAMPLE_SIZE", 500))
                .into_iter()
                .map(|ts| remote_write::WriteRequest {
                    timeseries: ts.collect(),
                    ..Default::default()
                })
                .collect_vec(),
        }
    }
}

impl IntoIterator for Mimir<Vec<remote_write::WriteRequest>> {
    type Item = remote_write::WriteRequest;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.state.into_iter()
    }
}

impl Mimir<RepeatedField<remote_write::TimeSeries>> {
    pub fn repeated(self) -> RepeatedField<remote_write::TimeSeries> {
        self.state
    }
}

impl From<MetricFamily> for Mimir<Vec<remote_write::TimeSeries>> {
    fn from(mf: MetricFamily) -> Self {
        let mut timeseries = vec![];
        for metric in mf.get_metric() {
            let mut ts = remote_write::TimeSeries::default();
            ts.labels.extend(vec![
                // mimir requires that we use __name__ as a key that points to a value
                // of the metric name
                remote_write::Label {
                    name: "__name__".into(),
                    value: mf.get_name().into(),
                },
            ]);
            ts.labels
                .extend(Mimir::<RepeatedField<remote_write::Label>>::from(metric));

            // assumption here is that since a MetricFamily will have one MetricType, we'll only need
            // to look for one of these types.  Setting two different types on Metric at the same time
            // in a way that is conflicting with the MetricFamily type will result in undefined mimir
            // behavior, probably an error.
            if metric.has_counter() {
                let mut s = Mimir::<remote_write::Sample>::from(metric.get_counter()).sample();
                s.timestamp = metric.get_timestamp_ms();
                ts.samples.push(s);
            } else if metric.has_gauge() {
                let mut s = Mimir::<remote_write::Sample>::from(metric.get_gauge()).sample();
                s.timestamp = metric.get_timestamp_ms();
                ts.samples.push(s);
            } else if metric.has_histogram() {
                // TODO implement
                // ts.mut_histograms()
                //     .push(Mimir::<remote_write::Histogram>::from(metric.get_histogram()).histogram());
            } else if metric.has_summary() {
                // TODO implement
                error!("summary is not implemented for a metric type");
            }
            timeseries.push(ts);
        }
        Self { state: timeseries }
    }
}

impl Mimir<remote_write::TimeSeries> {
    pub fn timeseries(self) -> remote_write::TimeSeries {
        self.state
    }
}

#[cfg(test)]
pub mod tests {
    use crate::prom_to_mimir::Mimir;
    use crate::remote_write;
    use prometheus::proto;
    use protobuf::RepeatedField;

    // protobuf stuff
    pub fn create_metric_family(
        name: &str,
        help: &str,
        field_type: Option<proto::MetricType>,
        metric: RepeatedField<proto::Metric>,
    ) -> proto::MetricFamily {
        // no public fields, cannot use literals
        let mut mf = proto::MetricFamily::default();
        mf.set_name(name.into());
        mf.set_help(help.into());
        // TODO remove the metric type serialization if we still don't use it
        // after implementing histogram and summary
        if let Some(ft) = field_type {
            mf.set_field_type(ft);
        }
        mf.set_metric(metric);
        mf
    }
    #[allow(dead_code)]
    fn create_metric_gauge(
        labels: RepeatedField<proto::LabelPair>,
        gauge: proto::Gauge,
    ) -> proto::Metric {
        let mut m = proto::Metric::default();
        m.set_label(labels);
        m.set_gauge(gauge);
        m.set_timestamp_ms(12345);
        m
    }

    pub fn create_metric_counter(
        labels: RepeatedField<proto::LabelPair>,
        counter: proto::Counter,
    ) -> proto::Metric {
        let mut m = proto::Metric::default();
        m.set_label(labels);
        m.set_counter(counter);
        m.set_timestamp_ms(12345);
        m
    }

    pub fn create_metric_histogram(
        labels: RepeatedField<proto::LabelPair>,
        histogram: proto::Histogram,
    ) -> proto::Metric {
        let mut m = proto::Metric::default();
        m.set_label(labels);
        m.set_histogram(histogram);
        m.set_timestamp_ms(12345);
        m
    }

    pub fn create_histogram() -> proto::Histogram {
        let mut h = proto::Histogram::default();
        h.set_sample_count(1);
        h.set_sample_sum(1.0);
        let mut b = proto::Bucket::default();
        b.set_cumulative_count(1);
        b.set_upper_bound(1.0);
        h.mut_bucket().push(b);
        h
    }

    pub fn create_labels(labels: Vec<(&str, &str)>) -> Vec<proto::LabelPair> {
        labels
            .into_iter()
            .map(|(key, value)| {
                let mut lp = proto::LabelPair::default();
                lp.set_name(key.into());
                lp.set_value(value.into());
                lp
            })
            .collect()
    }
    #[allow(dead_code)]
    fn create_gauge(value: f64) -> proto::Gauge {
        let mut g = proto::Gauge::default();
        g.set_value(value);
        g
    }

    pub fn create_counter(value: f64) -> proto::Counter {
        let mut c = proto::Counter::default();
        c.set_value(value);
        c
    }

    // end protobuf stuff

    // mimir stuff
    fn create_timeseries_with_samples(
        labels: Vec<remote_write::Label>,
        samples: Vec<remote_write::Sample>,
    ) -> remote_write::TimeSeries {
        remote_write::TimeSeries {
            labels,
            samples,
            ..Default::default()
        }
    }
    // end mimir stuff

    #[test]
    fn metricfamily_to_timeseries() {
        let tests: Vec<(proto::MetricFamily, Vec<remote_write::TimeSeries>)> = vec![
            (
                create_metric_family(
                    "test_gauge",
                    "i'm a help message",
                    Some(proto::MetricType::GAUGE),
                    RepeatedField::from(vec![create_metric_gauge(
                        RepeatedField::from_vec(create_labels(vec![
                            ("host", "local-test-validator"),
                            ("network", "unittest-network"),
                        ])),
                        create_gauge(2046.0),
                    )]),
                ),
                vec![create_timeseries_with_samples(
                    vec![
                        remote_write::Label {
                            name: "__name__".into(),
                            value: "test_gauge".into(),
                        },
                        remote_write::Label {
                            name: "host".into(),
                            value: "local-test-validator".into(),
                        },
                        remote_write::Label {
                            name: "network".into(),
                            value: "unittest-network".into(),
                        },
                    ],
                    vec![remote_write::Sample {
                        value: 2046.0,
                        timestamp: 12345,
                    }],
                )],
            ),
            (
                create_metric_family(
                    "test_counter",
                    "i'm a help message",
                    Some(proto::MetricType::GAUGE),
                    RepeatedField::from(vec![create_metric_counter(
                        RepeatedField::from_vec(create_labels(vec![
                            ("host", "local-test-validator"),
                            ("network", "unittest-network"),
                        ])),
                        create_counter(2046.0),
                    )]),
                ),
                vec![create_timeseries_with_samples(
                    vec![
                        remote_write::Label {
                            name: "__name__".into(),
                            value: "test_counter".into(),
                        },
                        remote_write::Label {
                            name: "host".into(),
                            value: "local-test-validator".into(),
                        },
                        remote_write::Label {
                            name: "network".into(),
                            value: "unittest-network".into(),
                        },
                    ],
                    vec![remote_write::Sample {
                        value: 2046.0,
                        timestamp: 12345,
                    }],
                )],
            ),
        ];
        for (mf, expected_ts) in tests {
            // TODO stop using state directly
            for (actual, expected) in Mimir::from(mf).state.into_iter().zip(expected_ts) {
                assert_eq!(actual.labels, expected.labels);
                for (actual_sample, expected_sample) in
                    actual.samples.into_iter().zip(expected.samples)
                {
                    assert_eq!(
                        actual_sample.value, expected_sample.value,
                        "sample values do not match"
                    );

                    // timestamps are injected on the sui-node and we copy it to our sample
                    // make sure that works
                    assert_eq!(
                        actual_sample.timestamp, expected_sample.timestamp,
                        "timestamp should be non-zero"
                    );
                }
            }
        }
    }
}
