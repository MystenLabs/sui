// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This is a library for common Tokio Tracing subscribers, such as for Jaeger.
//!
//! The subscribers are configured using TelemetryConfig passed into the `init()` method.
//! A panic hook is also installed so that panics/expects result in scoped error logging.
//!
//! Getting started is easy:
//! ```
//! let config = telemetry_subscribers::TelemetryConfig {
//!   service_name: "my_app".into(),
//!   ..Default::default()
//! };
//! // Important! Need to keep the guard and not drop until program terminates
//! let guard = telemetry_subscribers::init(config);
//! ```
//!
//! ## Features
//! - `jaeger` - this feature is enabled by default as it enables jaeger tracing
//! - `json` - Bunyan formatter - JSON log output, optional
//! - `tokio-console` - [Tokio-console](https://github.com/tokio-rs/console) subscriber, optional

#[cfg(feature = "jaeger")]
use opentelemetry::global;
#[cfg(feature = "jaeger")]
use opentelemetry::sdk::propagation::TraceContextPropagator;

use tracing::subscriber::set_global_default;
use tracing::{info, metadata::LevelFilter};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    EnvFilter, Registry,
};

#[cfg(feature = "chrome")]
use tracing_chrome::ChromeLayerBuilder;

#[cfg(feature = "json")]
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};

/// Configuration for different logging/tracing options
/// ===
/// - json_log_output: Output JSON logs to stdout only.  No other options will work.
/// - log_file: If defined, write output to a file starting with this name, ex app.log
/// - log_level: error/warn/info/debug/trace, defaults to info
/// - service_name:
#[derive(Default, Clone, Debug)]
pub struct TelemetryConfig {
    pub enable_tracing: bool,
    /// The name of the service for Jaeger and Bunyan
    pub service_name: String,
    pub tokio_console: bool,
    /// Output JSON logs.  Tracing and Tokio Console are not available if this is enabled.
    pub json_log_output: bool,
    /// Write chrome trace output, which can be loaded from chrome://tracing
    pub chrome_trace_output: bool,
    /// If defined, write output to a file starting with this name, ex app.log
    pub log_file: Option<String>,
    /// Log level to set, defaults to info
    pub log_level: Option<String>,
}

#[cfg(feature = "chrome")]
type ChromeGuard = tracing_chrome::FlushGuard;
#[cfg(not(feature = "chrome"))]
type ChromeGuard = ();

pub struct TelemetryGuards(WorkerGuard, Option<ChromeGuard>);

fn get_output(config: &TelemetryConfig) -> (NonBlocking, WorkerGuard) {
    if let Some(logfile_prefix) = &config.log_file {
        let file_appender = tracing_appender::rolling::daily("", logfile_prefix);
        tracing_appender::non_blocking(file_appender)
    } else {
        tracing_appender::non_blocking(std::io::stderr())
    }
}

// NOTE: this function is copied from tracing's panic_hook example
fn set_panic_hook() {
    // Set a panic hook that records the panic as a `tracing` event at the
    // `ERROR` verbosity level.
    //
    // If we are currently in a span when the panic occurred, the logged event
    // will include the current span, allowing the context in which the panic
    // occurred to be recorded.
    std::panic::set_hook(Box::new(|panic| {
        // If the panic has a source location, record it as structured fields.
        if let Some(location) = panic.location() {
            // On nightly Rust, where the `PanicInfo` type also exposes a
            // `message()` method returning just the message, we could record
            // just the message instead of the entire `fmt::Display`
            // implementation, avoiding the duplicated location
            tracing::error!(
                message = %panic,
                panic.file = location.file(),
                panic.line = location.line(),
                panic.column = location.column(),
            );
        } else {
            tracing::error!(message = %panic);
        }
    }));
}

#[cfg(feature = "json")]
fn bunyan_json_subscriber(config: &TelemetryConfig, env_filter: EnvFilter, nb_output: NonBlocking) {
    // See https://www.lpalmieri.com/posts/2020-09-27-zero-to-production-4-are-we-observable-yet/#5-7-tracing-bunyan-formatter
    // Also Bunyan layer addes JSON logging for tracing spans with duration information
    let formatting_layer = BunyanFormattingLayer::new(config.service_name.clone(), nb_output);
    // The `with` method is provided by `SubscriberExt`, an extension
    // trait for `Subscriber` exposed by `tracing_subscriber`
    let subscriber = Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer);
    // `set_global_default` can be used by applications to specify
    // what subscriber should be used to process spans.
    set_global_default(subscriber).expect("Failed to set subscriber");

    info!("Enabling JSON and span logging");
}

#[cfg(feature = "jaeger")]
fn jaeger_subscriber<S>(config: &TelemetryConfig, subscriber: S)
where
    S: tracing::Subscriber
        + Send
        + Sync
        + for<'span> tracing_subscriber::registry::LookupSpan<'span>,
{
    // Install a tracer to send traces to Jaeger.  Batching for better performance.
    let tracer = opentelemetry_jaeger::new_pipeline()
        .with_service_name(&config.service_name)
        .with_max_packet_size(9216) // Default max UDP packet size on OSX
        .with_auto_split_batch(true) // Auto split batches so they fit under packet size
        .install_batch(opentelemetry::runtime::Tokio)
        .expect("Could not create async Tracer");

    // Create a tracing subscriber with the configured tracer
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    // Enable Trace Contexts for tying spans together
    global::set_text_map_propagator(TraceContextPropagator::new());

    set_global_default(subscriber.with(telemetry)).expect("Failed to set subscriber");
    info!("Jaeger tracing initialized");
}

/// Initialize telemetry subscribers based on TelemetryConfig
/// NOTE: You must keep the returned guard and not drop it until the end of the program, otherwise
/// logs will not appear!!
pub fn init(config: TelemetryConfig) -> TelemetryGuards {
    // TODO: reorganize different telemetry options so they can use the same registry
    // Code to add logging/tracing config from environment, including RUST_LOG
    let log_level = config.log_level.clone().unwrap_or_else(|| "info".into());
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));
    let (nb_output, worker_guard) = get_output(&config);

    #[allow(unused_mut)]
    let mut chrome_guard = None;

    if config.json_log_output {
        #[cfg(feature = "json")]
        bunyan_json_subscriber(&config, env_filter, nb_output);
        #[cfg(not(feature = "json"))]
        panic!("Cannot enable JSON log output because json package feature is not enabled");
    } else if config.tokio_console {
        #[cfg(feature = "tokio-console")]
        console_subscriber::init();
        #[cfg(not(feature = "tokio-console"))]
        panic!("Cannot enable Tokio console subscriber because tokio-console feature not enabled");
    } else if config.chrome_trace_output {
        #[cfg(feature = "chrome")]
        {
            let (chrome_layer, guard) = ChromeLayerBuilder::new().build();
            let subscriber = Registry::default().with(chrome_layer);
            set_global_default(subscriber).expect("Failed to set subscriber");
            chrome_guard = Some(guard);
        }
        #[cfg(not(feature = "chrome"))]
        panic!("Cannot enable chrome traces because chrome feature not enabled");
    } else {
        // Output to file or to stdout with ANSI colors
        let fmt_layer = fmt::layer()
            .with_ansi(config.log_file.is_none())
            .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
            .with_writer(nb_output);

        // Standard env filter (RUST_LOG) with standard formatter
        let subscriber = Registry::default().with(env_filter).with(fmt_layer);

        if config.enable_tracing {
            #[cfg(feature = "jaeger")]
            jaeger_subscriber(&config, subscriber);
            #[cfg(not(feature = "jaeger"))]
            panic!("Cannot enable Jaeger subscriber because jaeger feature not enabled in package");
        } else {
            set_global_default(subscriber).expect("Failed to set subscriber");
        }
    }

    set_panic_hook();

    // The guard must be returned and kept in the main fn of the app, as when it's dropped then the output
    // gets flushed and closed. If this is dropped too early then no output will appear!
    TelemetryGuards(worker_guard, chrome_guard)
}

/// Globally set a tracing subscriber suitable for testing environments
pub fn init_for_testing() {
    use once_cell::sync::Lazy;

    static LOGGER: Lazy<()> = Lazy::new(|| {
        let subscriber = ::tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::DEBUG.into())
                    .parse("debug,h2=off,hyper=off")
                    .unwrap()
            }))
            .with_file(true)
            .with_line_number(true)
            .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
            .with_test_writer()
            .finish();
        ::tracing::subscriber::set_global_default(subscriber)
            .expect("unable to initialize logging for tests");
    });

    Lazy::force(&LOGGER);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::{debug, info, warn};

    #[test]
    #[should_panic]
    fn test_telemetry_init() {
        let config = TelemetryConfig {
            service_name: "my_app".into(),
            ..Default::default()
        };
        let _guard = init(config);

        info!(a = 1, "This will be INFO.");
        debug!(a = 2, "This will be DEBUG.");
        warn!(a = 3, "This will be WARNING.");
        panic!("This should cause error logs to be printed out!");
    }

    // Both the following tests should be able to "race" to initialize logging without causing a
    // panic
    #[test]
    fn testing_logger_1() {
        init_for_testing();
    }

    #[test]
    fn testing_logger_2() {
        init_for_testing();
    }
}
