// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::{
    core::{Collector, Desc},
    proto::{Counter, Gauge, LabelPair, Metric, MetricFamily, MetricType, Summary},
};

use crate::stats::DbConnectionStats;

/// Collects information about the database connection pool.
pub struct DbConnectionStatsCollector<T: DbConnectionStats> {
    stats_provider: T,
    desc: Vec<(MetricType, Desc)>,
}

impl<T: DbConnectionStats> DbConnectionStatsCollector<T> {
    pub fn new(prefix: Option<&str>, stats_provider: T) -> Self {
        let prefix = prefix.unwrap_or("db");
        let name = |n| format!("{prefix}_{n}");

        let desc = vec![
            (
                MetricType::GAUGE,
                desc(
                    name("connections"),
                    "Number of connections currently being managed by the pool",
                ),
            ),
            (
                MetricType::GAUGE,
                desc(
                    name("idle_connections"),
                    "Number of idle connections in the pool",
                ),
            ),
            (
                MetricType::COUNTER,
                desc(
                    name("connect_direct"),
                    "Connections that did not have to wait",
                ),
            ),
            (
                MetricType::SUMMARY,
                desc(name("connect_waited"), "Connections that had to wait"),
            ),
            (
                MetricType::COUNTER,
                desc(
                    name("connect_timed_out"),
                    "Connections that timed out waiting for a connection",
                ),
            ),
            (
                MetricType::COUNTER,
                desc(
                    name("connections_created"),
                    "Connections that have been created in the pool",
                ),
            ),
            (
                MetricType::COUNTER,
                desc_with_labels(
                    name("connections_closed"),
                    "Total connections that were closed",
                    &["reason"],
                ),
            ),
        ];

        Self {
            stats_provider,
            desc,
        }
    }
}

impl<T: DbConnectionStats> Collector for DbConnectionStatsCollector<T> {
    fn desc(&self) -> Vec<&Desc> {
        self.desc.iter().map(|d| &d.1).collect()
    }

    fn collect(&self) -> Vec<MetricFamily> {
        let stats = self.stats_provider.get_connection_stats();

        vec![
            gauge(&self.desc[0].1, stats.connections as f64),
            gauge(&self.desc[1].1, stats.idle_connections as f64),
            counter(&self.desc[2].1, stats.get_direct as f64),
            summary(
                &self.desc[3].1,
                stats.get_wait_time_ms as f64,
                stats.get_waited + stats.get_timed_out,
            ),
            counter(&self.desc[4].1, stats.get_timed_out as f64),
            counter(&self.desc[5].1, stats.connections_created as f64),
            counter_with_labels(
                &self.desc[6].1,
                &[
                    ("reason", "broken", stats.connections_closed_broken as f64),
                    ("reason", "invalid", stats.connections_closed_invalid as f64),
                    (
                        "reason",
                        "max_lifetime",
                        stats.connections_closed_max_lifetime as f64,
                    ),
                    (
                        "reason",
                        "idle_timeout",
                        stats.connections_closed_idle_timeout as f64,
                    ),
                ],
            ),
        ]
    }
}

fn desc(name: String, help: &str) -> Desc {
    desc_with_labels(name, help, &[])
}

fn desc_with_labels(name: String, help: &str, labels: &[&str]) -> Desc {
    Desc::new(
        name,
        help.to_string(),
        labels.iter().map(|s| s.to_string()).collect(),
        Default::default(),
    )
    .expect("Bad metric description")
}

fn gauge(desc: &Desc, value: f64) -> MetricFamily {
    let mut g = Gauge::default();
    let mut m = Metric::default();
    let mut mf = MetricFamily::new();

    g.set_value(value);
    m.set_gauge(g);

    mf.mut_metric().push(m);
    mf.set_name(desc.fq_name.clone());
    mf.set_help(desc.help.clone());
    mf.set_field_type(MetricType::GAUGE);
    mf
}

fn counter(desc: &Desc, value: f64) -> MetricFamily {
    let mut c = Counter::default();
    let mut m = Metric::default();
    let mut mf = MetricFamily::new();

    c.set_value(value);
    m.set_counter(c);

    mf.mut_metric().push(m);
    mf.set_name(desc.fq_name.clone());
    mf.set_help(desc.help.clone());
    mf.set_field_type(MetricType::COUNTER);
    mf
}

fn counter_with_labels(desc: &Desc, values: &[(&str, &str, f64)]) -> MetricFamily {
    let mut mf = MetricFamily::new();

    for (name, label, value) in values {
        let mut c = Counter::default();
        let mut l = LabelPair::default();
        let mut m = Metric::default();

        c.set_value(*value);
        l.set_name(name.to_string());
        l.set_value(label.to_string());

        m.set_counter(c);
        m.mut_label().push(l);
        mf.mut_metric().push(m);
    }

    mf.set_name(desc.fq_name.clone());
    mf.set_help(desc.help.clone());
    mf.set_field_type(MetricType::COUNTER);
    mf
}

fn summary(desc: &Desc, sum: f64, count: u64) -> MetricFamily {
    let mut s = Summary::default();
    let mut m = Metric::default();
    let mut mf = MetricFamily::new();

    s.set_sample_sum(sum);
    s.set_sample_count(count);
    m.set_summary(s);

    mf.mut_metric().push(m);
    mf.set_name(desc.fq_name.clone());
    mf.set_help(desc.help.clone());
    mf.set_field_type(MetricType::SUMMARY);
    mf
}
