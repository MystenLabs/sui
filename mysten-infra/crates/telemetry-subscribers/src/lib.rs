// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This is a library for common Tokio Tracing subscribers, such as for Jaeger.
//!
//! The subscribers are configured using TelemetryConfig passed into the `init()` method.
//! A panic hook is also installed so that panics/expects result in scoped error logging.
//!
//! Getting started is easy:
//! ```
//! // Important! Need to keep the guard and not drop until program terminates
//! let guard = telemetry_subscribers::TelemetryConfig::new("my_app").init();
//! ```
//!
//! ## Features
//! - `jaeger` - this feature is enabled by default as it enables jaeger tracing
//! - `json` - Bunyan formatter - JSON log output, optional
//! - `tokio-console` - [Tokio-console](https://github.com/tokio-rs/console) subscriber, optional

use std::{env, io::stderr};
use tracing::metadata::LevelFilter;
use tracing::subscriber::set_global_default;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_subscriber::Layer;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    EnvFilter, Registry,
};

use crossterm::tty::IsTty;

/// Configuration for different logging/tracing options
/// ===
/// - json_log_output: Output JSON logs to stdout only.  No other options will work.
/// - log_file: If defined, write output to a file starting with this name, ex app.log
/// - log_level: error/warn/info/debug/trace, defaults to info
/// - service_name:
#[derive(Default, Clone, Debug)]
pub struct TelemetryConfig {
    /// The name of the service for Jaeger and Bunyan
    pub service_name: String,

    pub enable_tracing: bool,
    pub tokio_console: bool,
    /// Output JSON logs.  Tracing and Tokio Console are not available if this is enabled.
    pub json_log_output: bool,
    /// Write chrome trace output, which can be loaded from chrome://tracing
    pub chrome_trace_output: bool,
    /// If defined, write output to a file starting with this name, ex app.log
    pub log_file: Option<String>,
    /// Log level to set, defaults to info
    pub log_level: Option<String>,
    /// Set a panic hook
    pub panic_hook: bool,
}

#[must_use]
#[allow(dead_code)]
pub struct TelemetryGuards {
    worker_guard: WorkerGuard,

    #[cfg(feature = "chrome")]
    chrome_guard: Option<tracing_chrome::FlushGuard>,
}

fn get_output(log_file: Option<String>) -> (NonBlocking, WorkerGuard) {
    if let Some(logfile_prefix) = log_file {
        let file_appender = tracing_appender::rolling::daily("", logfile_prefix);
        tracing_appender::non_blocking(file_appender)
    } else {
        tracing_appender::non_blocking(stderr())
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

impl TelemetryConfig {
    pub fn new(service_name: &str) -> Self {
        Self {
            service_name: service_name.to_owned(),
            enable_tracing: false,
            tokio_console: false,
            json_log_output: false,
            chrome_trace_output: false,
            log_file: None,
            log_level: None,
            panic_hook: true,
        }
    }

    pub fn with_log_file(mut self, filename: &str) -> Self {
        self.log_file = Some(filename.to_owned());
        self
    }

    pub fn with_env(mut self) -> Self {
        if env::var("MYSTEN_TRACING").is_ok() {
            self.enable_tracing = true
        }

        if env::var("MYSTEN_TRACING_CHROME").is_ok() {
            self.chrome_trace_output = true;
        }

        if env::var("MYSTEN_TRACING_JSON").is_ok() {
            self.json_log_output = true;
        }

        if env::var("TOKIO_CONSOLE").is_ok() {
            self.tokio_console = true;
        }

        if let Ok(filepath) = env::var("MYSTEN_TRACING_FILE") {
            self.log_file = Some(filepath);
        }

        self
    }

    pub fn init(self) -> TelemetryGuards {
        let config = self;

        let registry = Registry::default();

        // tokio-console layer
        #[cfg(feature = "tokio-console")]
        let tokio_console = if config.tokio_console {
            Some(console_subscriber::spawn())
        } else {
            None
        };

        #[cfg(feature = "tokio-console")]
        let registry = registry.with(tokio_console);

        // Setup an EnvFilter which will filter all downstream layers
        let log_level = config.log_level.unwrap_or_else(|| "info".into());
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));
        let registry = registry.with(env_filter);

        #[cfg(feature = "chrome")]
        let (chrome_layer, chrome_guard) = if config.chrome_trace_output {
            let (chrome_layer, guard) = tracing_chrome::ChromeLayerBuilder::new().build();
            (Some(chrome_layer), Some(guard))
        } else {
            (None, None)
        };

        #[cfg(feature = "chrome")]
        let registry = registry.with(chrome_layer);

        #[cfg(feature = "jaeger")]
        let jaeger_layer = if config.enable_tracing {
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
            opentelemetry::global::set_text_map_propagator(
                opentelemetry::sdk::propagation::TraceContextPropagator::new(),
            );

            Some(telemetry)
        } else {
            None
        };

        #[cfg(feature = "jaeger")]
        let registry = registry.with(jaeger_layer);

        // Setup formatter and optional storage layer
        let json_storage_layer = if config.json_log_output {
            Some(JsonStorageLayer)
        } else {
            None
        };

        let registry = registry.with(json_storage_layer);

        let (nb_output, worker_guard) = get_output(config.log_file.clone());
        let fmt_layer: Box<dyn Layer<_> + Send + Sync + 'static> = {
            if config.json_log_output {
                // See https://www.lpalmieri.com/posts/2020-09-27-zero-to-production-4-are-we-observable-yet/#5-7-tracing-bunyan-formatter
                // Also Bunyan layer addes JSON logging for tracing spans with duration information
                let json_layer = BunyanFormattingLayer::new(config.service_name, nb_output);
                Box::new(json_layer)
            } else {
                // Output to file or to stderr with ANSI colors
                let fmt_layer = fmt::layer()
                    .with_ansi(config.log_file.is_none() && stderr().is_tty())
                    .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
                    .with_writer(nb_output);
                Box::new(fmt_layer)
            }
        };

        let registry = registry.with(fmt_layer);

        set_global_default(registry).expect("Failed to set subscriber");

        if config.panic_hook {
            set_panic_hook();
        }

        // The guard must be returned and kept in the main fn of the app, as when it's dropped then the output
        // gets flushed and closed. If this is dropped too early then no output will appear!
        TelemetryGuards {
            worker_guard,
            #[cfg(feature = "chrome")]
            chrome_guard,
        }
    }
}

/// Globally set a tracing subscriber suitable for testing environments
pub fn init_for_testing() {
    use once_cell::sync::Lazy;

    static LOGGER: Lazy<()> = Lazy::new(|| {
        let subscriber = ::tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .from_env_lossy(),
            )
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
        let _guard = TelemetryConfig::new("my_app").init();

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
