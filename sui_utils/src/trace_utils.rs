// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use telemetry_subscribers::TelemetryConfig;

pub fn init_telemetry() -> telemetry_subscribers::TelemetryGuards {
    init_telemetry_with_config(telemetry_subscribers::TelemetryConfig {
        service_name: "sui".into(),
        enable_tracing: std::env::var("SUI_TRACING_ENABLE").is_ok(),
        chrome_trace_output: std::env::var("CHROME_TRACE_ENABLE").is_ok(),
        json_log_output: std::env::var("SUI_JSON_SPAN_LOGS").is_ok(),
        ..Default::default()
    })
}

pub fn init_telemetry_with_config(config: telemetry_subscribers::TelemetryConfig) -> telemetry_subscribers::TelemetryGuards {
    let config = telemetry_subscribers::TelemetryConfig {
        service_name: "sui".into(),
        enable_tracing: std::env::var("SUI_TRACING_ENABLE").is_ok(),
        chrome_trace_output: std::env::var("CHROME_TRACE_ENABLE").is_ok(),
        json_log_output: std::env::var("SUI_JSON_SPAN_LOGS").is_ok(),
        ..config
    };

    telemetry_subscribers::init(config)
}
