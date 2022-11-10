// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus_closure_metric::ClosureMetric;

#[test]
fn closure_metric_basic() {
    let opts =
        prometheus::opts!("my_closure_metric", "A test closure metric",).variable_label("my_label");

    let fn_42 = || 42_u64;
    let metric0 = ClosureMetric::new(
        opts,
        prometheus_closure_metric::ValueType::Gauge,
        fn_42,
        &["forty_two"],
    )
    .unwrap();

    assert!(prometheus::default_registry()
        .register(Box::new(metric0))
        .is_ok());

    // Gather the metrics.
    let metric_families = prometheus::default_registry().gather();
    assert_eq!(1, metric_families.len());
    let metric_family = &metric_families[0];
    assert_eq!("my_closure_metric", metric_family.get_name());
    let metric = metric_family.get_metric();
    assert_eq!(1, metric.len());
    assert_eq!(42.0, metric[0].get_gauge().get_value());
    let labels = metric[0].get_label();
    assert_eq!(1, labels.len());
    assert_eq!("my_label", labels[0].get_name());
    assert_eq!("forty_two", labels[0].get_value());
}
