// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Copyright 2014 The Prometheus Authors
// Copyright 2019 TiKV Project Authors. Licensed under Apache-2.0.

//! This library implements a `ClosureMetric` for crate `prometheus` whose value is computed at
//! the time of collection by a provided closure.

// TODO: add example usage once constructor macros are implemented.
// (For now, look at tests for an example.)

use anyhow::anyhow;
use anyhow::Result;
use prometheus::core;
use prometheus::proto;

/// A Prometheus metric whose value is computed at collection time by the provided closure.
///
/// WARNING: The provided closure must be fast (~milliseconds or faster), since it blocks
/// metric collection.
#[derive(Debug)]
pub struct ClosureMetric<F> {
    desc: core::Desc,
    f: F,
    value_type: ValueType,
    label_pairs: Vec<proto::LabelPair>,
}

impl<F, T> ClosureMetric<F>
where
    F: Fn() -> T + Sync + Send,
    T: core::Number,
{
    pub fn new<D: core::Describer>(
        describer: D,
        value_type: ValueType,
        f: F,
        label_values: &[&str],
    ) -> Result<Self> {
        let desc = describer.describe()?;
        let label_pairs = make_label_pairs(&desc, label_values)?;

        Ok(Self {
            desc,
            f,
            value_type,
            label_pairs,
        })
    }

    pub fn metric(&self) -> proto::Metric {
        let mut m = proto::Metric::default();
        m.set_label(protobuf::RepeatedField::from_vec(self.label_pairs.clone()));

        let val = (self.f)().into_f64();
        match self.value_type {
            ValueType::Counter => {
                let mut counter = proto::Counter::default();
                counter.set_value(val);
                m.set_counter(counter);
            }
            ValueType::Gauge => {
                let mut gauge = proto::Gauge::default();
                gauge.set_value(val);
                m.set_gauge(gauge);
            }
        }

        m
    }
}

impl<F, T> prometheus::core::Collector for ClosureMetric<F>
where
    F: Fn() -> T + Sync + Send,
    T: core::Number,
{
    fn desc(&self) -> Vec<&prometheus::core::Desc> {
        vec![&self.desc]
    }

    fn collect(&self) -> Vec<prometheus::proto::MetricFamily> {
        let mut m = proto::MetricFamily::default();
        m.set_name(self.desc.fq_name.clone());
        m.set_help(self.desc.help.clone());
        m.set_field_type(self.value_type.metric_type());
        m.set_metric(protobuf::RepeatedField::from_vec(vec![self.metric()]));
        vec![m]
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ValueType {
    Counter,
    Gauge,
}

impl ValueType {
    /// `metric_type` returns the corresponding proto metric type.
    pub fn metric_type(self) -> proto::MetricType {
        match self {
            ValueType::Counter => proto::MetricType::COUNTER,
            ValueType::Gauge => proto::MetricType::GAUGE,
        }
    }
}

pub fn make_label_pairs(desc: &core::Desc, label_values: &[&str]) -> Result<Vec<proto::LabelPair>> {
    if desc.variable_labels.len() != label_values.len() {
        return Err(anyhow!("inconsistent cardinality"));
    }

    let total_len = desc.variable_labels.len() + desc.const_label_pairs.len();
    if total_len == 0 {
        return Ok(vec![]);
    }

    if desc.variable_labels.is_empty() {
        return Ok(desc.const_label_pairs.clone());
    }

    let mut label_pairs = Vec::with_capacity(total_len);
    for (i, n) in desc.variable_labels.iter().enumerate() {
        let mut label_pair = proto::LabelPair::default();
        label_pair.set_name(n.clone());
        label_pair.set_value(label_values[i].to_owned());
        label_pairs.push(label_pair);
    }

    for label_pair in &desc.const_label_pairs {
        label_pairs.push(label_pair.clone());
    }
    label_pairs.sort();
    Ok(label_pairs)
}

// TODO: add and test macros for easier ClosureMetric construction.
