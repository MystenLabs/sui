// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use atomic_float::AtomicF64;
use crossterm::tty::IsTty;
use once_cell::sync::Lazy;
use opentelemetry::{
    trace::{Link, SamplingResult, SpanKind, TraceId, TracerProvider as _},
    Context, KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::Sampler;
use opentelemetry_sdk::{
    self, runtime,
    trace::{BatchSpanProcessor, ShouldSample, TracerProvider},
    Resource,
};
use span_latency_prom::PrometheusSpanLatencyLayer;
use std::path::PathBuf;
use std::time::Duration;
use std::{
    env,
    io::{stderr, Write},
    str::FromStr,
    sync::{atomic::Ordering, Arc, Mutex},
};
use tracing::metadata::LevelFilter;
use tracing::{error, info, Level};
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::{filter, fmt, layer::SubscriberExt, reload, EnvFilter, Layer, Registry};

use crate::file_exporter::{CachedOpenFile, FileExporter};

mod file_exporter;
pub mod span_latency_prom;

/// Alias for a type-erased error type.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Configuration for different logging/tracing options
/// ===
/// - json_log_output: Output JSON logs to stdout only.
/// - log_file: If defined, write output to a file starting with this name, ex app.log
/// - log_level: error/warn/info/debug/trace, defaults to info
#[derive(Default, Clone, Debug)]
pub struct TelemetryConfig {
    pub enable_otlp_tracing: bool,
    /// Enables Tokio Console debugging on port 6669
    pub tokio_console: bool,
    /// Output JSON logs.
    pub json_log_output: bool,
    /// If defined, write output to a file starting with this name, ex app.log
    pub log_file: Option<String>,
    /// Log level to set, defaults to info
    pub log_string: Option<String>,
    /// Span level - what level of spans should be created.  Note this is not same as logging level
    /// If set to None, then defaults to INFO
    pub span_level: Option<Level>,
    /// Set a panic hook
    pub panic_hook: bool,
    /// Crash on panic
    pub crash_on_panic: bool,
    /// Optional Prometheus registry - if present, all enabled span latencies are measured
    pub prom_registry: Option<prometheus::Registry>,
    pub sample_rate: f64,
    /// Add directive to include trace logs with provided target
    pub trace_target: Option<Vec<String>>,
}

#[must_use]
#[allow(dead_code)]
pub struct TelemetryGuards {
    worker_guard: WorkerGuard,
    provider: Option<TracerProvider>,
}

impl TelemetryGuards {
    fn new(
        config: TelemetryConfig,
        worker_guard: WorkerGuard,
        provider: Option<TracerProvider>,
    ) -> Self {
        set_global_telemetry_config(config);
        Self {
            worker_guard,
            provider,
        }
    }
}

impl Drop for TelemetryGuards {
    fn drop(&mut self) {
        clear_global_telemetry_config();
    }
}

#[derive(Clone, Debug)]
pub struct FilterHandle(reload::Handle<EnvFilter, Registry>);

impl FilterHandle {
    pub fn update<S: AsRef<str>>(&self, directives: S) -> Result<(), BoxError> {
        let filter = EnvFilter::try_new(directives)?;
        self.0.reload(filter)?;
        Ok(())
    }

    pub fn get(&self) -> Result<String, BoxError> {
        self.0
            .with_current(|filter| filter.to_string())
            .map_err(Into::into)
    }
}

pub struct TracingHandle {
    log: FilterHandle,
    trace: Option<FilterHandle>,
    file_output: CachedOpenFile,
    sampler: SamplingFilter,
}

impl TracingHandle {
    pub fn update_log<S: AsRef<str>>(&self, directives: S) -> Result<(), BoxError> {
        self.log.update(directives)
    }

    pub fn get_log(&self) -> Result<String, BoxError> {
        self.log.get()
    }

    pub fn update_sampling_rate(&self, sample_rate: f64) {
        self.sampler.update_sampling_rate(sample_rate);
    }

    pub fn update_trace_file<S: AsRef<str>>(&self, trace_file: S) -> Result<(), BoxError> {
        let trace_path = PathBuf::from_str(trace_file.as_ref())?;
        self.file_output.update_path(trace_path)?;
        Ok(())
    }

    pub fn update_trace_filter<S: AsRef<str>>(
        &self,
        directives: S,
        duration: Duration,
    ) -> Result<(), BoxError> {
        if let Some(trace) = &self.trace {
            let res = trace.update(directives);
            // after duration is elapsed, reset to the env setting
            let trace = trace.clone();
            let trace_filter_env = env::var("TRACE_FILTER").unwrap_or_else(|_| "off".to_string());
            tokio::spawn(async move {
                tokio::time::sleep(duration).await;
                if let Err(e) = trace.update(trace_filter_env) {
                    error!("failed to reset trace filter: {}", e);
                }
            });
            res
        } else {
            info!("tracing not enabled, ignoring update");
            Ok(())
        }
    }

    pub fn clear_file_output(&self) {
        self.file_output.clear_path();
    }

    pub fn reset_trace(&self) {
        if let Some(trace) = &self.trace {
            let trace_filter_env = env::var("TRACE_FILTER").unwrap_or_else(|_| "off".to_string());
            if let Err(e) = trace.update(trace_filter_env) {
                error!("failed to reset trace filter: {}", e);
            }
        }
    }
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
fn set_panic_hook(crash_on_panic: bool) {
    let default_panic_handler = std::panic::take_hook();

    // Set a panic hook that records the panic as a `tracing` event at the
    // `ERROR` verbosity level.
    //
    // If we are currently in a span when the panic occurred, the logged event
    // will include the current span, allowing the context in which the panic
    // occurred to be recorded.
    std::panic::set_hook(Box::new(move |panic| {
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

        default_panic_handler(panic);

        // We're panicking so we can't do anything about the flush failing
        let _ = std::io::stderr().flush();
        let _ = std::io::stdout().flush();

        if crash_on_panic {
            // Kill the process
            std::process::exit(12);
        }
    }));
}

static GLOBAL_CONFIG: Lazy<Arc<Mutex<Option<TelemetryConfig>>>> =
    Lazy::new(|| Arc::new(Mutex::new(None)));

fn set_global_telemetry_config(config: TelemetryConfig) {
    let mut global_config = GLOBAL_CONFIG.lock().unwrap();
    assert!(global_config.is_none());
    *global_config = Some(config);
}

fn clear_global_telemetry_config() {
    let mut global_config = GLOBAL_CONFIG.lock().unwrap();
    *global_config = None;
}

pub fn get_global_telemetry_config() -> Option<TelemetryConfig> {
    let global_config = GLOBAL_CONFIG.lock().unwrap();
    global_config.clone()
}

impl TelemetryConfig {
    pub fn new() -> Self {
        Self {
            enable_otlp_tracing: false,
            tokio_console: false,
            json_log_output: false,
            log_file: None,
            log_string: None,
            span_level: None,
            panic_hook: true,
            crash_on_panic: false,
            prom_registry: None,
            sample_rate: 1.0,
            trace_target: None,
        }
    }

    pub fn with_json(mut self) -> Self {
        self.json_log_output = true;
        self
    }

    pub fn with_log_level(mut self, log_string: &str) -> Self {
        self.log_string = Some(log_string.to_owned());
        self
    }

    pub fn with_span_level(mut self, span_level: Level) -> Self {
        self.span_level = Some(span_level);
        self
    }

    pub fn with_log_file(mut self, filename: &str) -> Self {
        self.log_file = Some(filename.to_owned());
        self
    }

    pub fn with_prom_registry(mut self, registry: &prometheus::Registry) -> Self {
        self.prom_registry = Some(registry.clone());
        self
    }

    pub fn with_sample_rate(mut self, rate: f64) -> Self {
        self.sample_rate = rate;
        self
    }

    pub fn with_trace_target(mut self, target: &str) -> Self {
        match self.trace_target {
            Some(ref mut v) => v.push(target.to_owned()),
            None => self.trace_target = Some(vec![target.to_owned()]),
        };

        self
    }

    pub fn with_env(mut self) -> Self {
        if env::var("CRASH_ON_PANIC").is_ok() {
            self.crash_on_panic = true
        }

        if env::var("TRACE_FILTER").is_ok() {
            self.enable_otlp_tracing = true
        }

        if env::var("RUST_LOG_JSON").is_ok() {
            self.json_log_output = true;
        }

        if env::var("TOKIO_CONSOLE").is_ok() {
            self.tokio_console = true;
        }

        if let Ok(span_level) = env::var("TOKIO_SPAN_LEVEL") {
            self.span_level =
                Some(Level::from_str(&span_level).expect("Cannot parse TOKIO_SPAN_LEVEL"));
        }

        if let Ok(filepath) = env::var("RUST_LOG_FILE") {
            self.log_file = Some(filepath);
        }

        if let Ok(sample_rate) = env::var("SAMPLE_RATE") {
            self.sample_rate = sample_rate.parse().expect("Cannot parse SAMPLE_RATE");
        }

        self
    }

    pub fn init(self) -> (TelemetryGuards, TracingHandle) {
        let config = self;
        let config_clone = config.clone();

        // Setup an EnvFilter for filtering logging output layers.
        // NOTE: we don't want to use this to filter all layers.  That causes problems for layers with
        // different filtering needs, including tokio-console/console-subscriber, and it also doesn't
        // fit with the span creation needs for distributed tracing and other span-based tools.
        let mut directives = config.log_string.unwrap_or_else(|| "info".into());
        if let Some(targets) = config.trace_target {
            for target in targets {
                directives.push_str(&format!(",{}=trace", target));
            }
        }
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(directives));
        let (log_filter, reload_handle) = reload::Layer::new(env_filter);
        let log_filter_handle = FilterHandle(reload_handle);

        // Separate span level filter.
        // This is a dumb filter for now - allows all spans that are below a given level.
        // TODO: implement a sampling filter
        let span_level = config.span_level.unwrap_or(Level::INFO);
        let span_filter = filter::filter_fn(move |metadata| {
            metadata.is_span() && *metadata.level() <= span_level
        });

        let mut layers = Vec::new();

        // tokio-console layer
        // Please see https://docs.rs/console-subscriber/latest/console_subscriber/struct.Builder.html#configuration
        // for environment vars/config options
        if config.tokio_console {
            layers.push(console_subscriber::spawn().boxed());
        }

        if let Some(registry) = config.prom_registry {
            let span_lat_layer = PrometheusSpanLatencyLayer::try_new(&registry, 15)
                .expect("Could not initialize span latency layer");
            layers.push(span_lat_layer.with_filter(span_filter.clone()).boxed());
        }

        let mut trace_filter_handle = None;
        let mut file_output = CachedOpenFile::new::<&str>(None).unwrap();
        let mut provider = None;
        let sampler = SamplingFilter::new(config.sample_rate);
        let service_name = env::var("OTEL_SERVICE_NAME").unwrap_or("sui-node".to_owned());

        if config.enable_otlp_tracing {
            let trace_file = env::var("TRACE_FILE").ok();
            let resource = Resource::new(vec![opentelemetry::KeyValue::new(
                "service.name",
                service_name.clone(),
            )]);
            let sampler = Sampler::ParentBased(Box::new(sampler.clone()));

            // We can either do file output or OTLP, but not both. tracing-opentelemetry
            // only supports a single tracer at a time.
            let telemetry = if let Some(trace_file) = trace_file {
                let exporter =
                    FileExporter::new(Some(trace_file.into())).expect("Failed to create exporter");
                file_output = exporter.cached_open_file.clone();
                let processor = BatchSpanProcessor::builder(exporter, runtime::Tokio).build();

                let p = TracerProvider::builder()
                    .with_resource(resource)
                    .with_sampler(sampler)
                    .with_span_processor(processor)
                    .build();

                let tracer = p.tracer(service_name);
                provider = Some(p);

                tracing_opentelemetry::layer().with_tracer(tracer)
            } else {
                let endpoint = env::var("OTLP_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:4317".to_string());
                let otlp_exporter = opentelemetry_otlp::SpanExporter::builder()
                    .with_tonic()
                    .with_endpoint(endpoint)
                    .build()
                    .unwrap();
                let tracer_provider = opentelemetry_sdk::trace::TracerProvider::builder()
                    .with_resource(resource)
                    .with_sampler(sampler)
                    .with_batch_exporter(otlp_exporter, runtime::Tokio)
                    .build();
                let tracer = tracer_provider.tracer(service_name);
                tracing_opentelemetry::layer().with_tracer(tracer)
            };

            // Enable Trace Contexts for tying spans together
            opentelemetry::global::set_text_map_propagator(
                opentelemetry_sdk::propagation::TraceContextPropagator::new(),
            );

            let trace_env_filter = EnvFilter::try_from_env("TRACE_FILTER").unwrap();
            let (trace_env_filter, reload_handle) = reload::Layer::new(trace_env_filter);
            trace_filter_handle = Some(FilterHandle(reload_handle));

            layers.push(telemetry.with_filter(trace_env_filter).boxed());
        }

        let (nb_output, worker_guard) = get_output(config.log_file.clone());
        if config.json_log_output {
            // Output to file or to stderr in a newline-delimited JSON format
            let json_layer = fmt::layer()
                .with_file(true)
                .with_line_number(true)
                .json()
                .with_writer(nb_output)
                .with_filter(log_filter)
                .boxed();
            layers.push(json_layer);
        } else {
            // Output to file or to stderr with ANSI colors
            let fmt_layer = fmt::layer()
                .with_ansi(config.log_file.is_none() && stderr().is_tty())
                .with_writer(nb_output)
                .with_filter(log_filter)
                .boxed();
            layers.push(fmt_layer);
        }

        let subscriber = tracing_subscriber::registry().with(layers);
        ::tracing::subscriber::set_global_default(subscriber)
            .expect("unable to initialize tracing subscriber");

        if config.panic_hook {
            set_panic_hook(config.crash_on_panic);
        }

        // The guard must be returned and kept in the main fn of the app, as when it's dropped then the output
        // gets flushed and closed. If this is dropped too early then no output will appear!
        let guards = TelemetryGuards::new(config_clone, worker_guard, provider);

        (
            guards,
            TracingHandle {
                log: log_filter_handle,
                trace: trace_filter_handle,
                file_output,
                sampler,
            },
        )
    }
}

// Like Sampler::TraceIdRatioBased, but can be updated at runtime
#[derive(Debug, Clone)]
struct SamplingFilter {
    // Sampling filter needs to be fast, so we avoid a mutex.
    sample_rate: Arc<AtomicF64>,
}

impl SamplingFilter {
    fn new(sample_rate: f64) -> Self {
        SamplingFilter {
            sample_rate: Arc::new(AtomicF64::new(Self::clamp(sample_rate))),
        }
    }

    fn clamp(sample_rate: f64) -> f64 {
        // clamp sample rate to between 0.0001 and 1.0
        sample_rate.clamp(0.0001, 1.0)
    }

    fn update_sampling_rate(&self, sample_rate: f64) {
        // clamp sample rate to between 0.0001 and 1.0
        let sample_rate = Self::clamp(sample_rate);
        self.sample_rate.store(sample_rate, Ordering::Relaxed);
    }
}

impl ShouldSample for SamplingFilter {
    fn should_sample(
        &self,
        parent_context: Option<&Context>,
        trace_id: TraceId,
        name: &str,
        span_kind: &SpanKind,
        attributes: &[KeyValue],
        links: &[Link],
    ) -> SamplingResult {
        let sample_rate = self.sample_rate.load(Ordering::Relaxed);
        let sampler = Sampler::TraceIdRatioBased(sample_rate);

        sampler.should_sample(parent_context, trace_id, name, span_kind, attributes, links)
    }
}

/// Globally set a tracing subscriber suitable for testing environments
pub fn init_for_testing() {
    static LOGGER: Lazy<()> = Lazy::new(|| {
        let subscriber = ::tracing_subscriber::FmtSubscriber::builder()
            .with_env_filter(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .from_env_lossy(),
            )
            .with_file(true)
            .with_line_number(true)
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
    use prometheus::proto::MetricType;
    use std::time::Duration;
    use tracing::{debug, debug_span, info, trace_span, warn};

    #[test]
    #[should_panic]
    fn test_telemetry_init() {
        let registry = prometheus::Registry::new();
        // Default logging level is INFO, but here we set the span level to DEBUG.  TRACE spans should be ignored.
        let config = TelemetryConfig::new()
            .with_span_level(Level::DEBUG)
            .with_prom_registry(&registry);
        let _guard = config.init();

        info!(a = 1, "This will be INFO.");
        // Spans are debug level or below, so they won't be printed out either.  However latencies
        // should be recorded for at least one span
        debug_span!("yo span yo").in_scope(|| {
            // This debug log will not print out, log level set to INFO by default
            debug!(a = 2, "This will be DEBUG.");
            std::thread::sleep(Duration::from_millis(100));
            warn!(a = 3, "This will be WARNING.");
        });

        // This span won't be enabled
        trace_span!("this span should not be created").in_scope(|| {
            info!("This log appears, but surrounding span is not created");
            std::thread::sleep(Duration::from_millis(100));
        });

        let metrics = registry.gather();
        // There should be 1 metricFamily and 1 metric
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].get_name(), "tracing_span_latencies");
        assert_eq!(metrics[0].get_field_type(), MetricType::HISTOGRAM);
        let inner = metrics[0].get_metric();
        assert_eq!(inner.len(), 1);
        let labels = inner[0].get_label();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].get_name(), "span_name");
        assert_eq!(labels[0].get_value(), "yo span yo");

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
