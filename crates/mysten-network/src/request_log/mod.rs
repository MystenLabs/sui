// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Opt-in capture of gRPC request messages on the `grpc_request` tracing target.
//!
//! [`GrpcRequestLogLayer`] tees each request body flowing through it, parses the gRPC message
//! framing, and emits one `trace`-level event per message with the payload as the raw protobuf
//! message bytes, base64-encoded. Capture is scoped to methods resolvable in the same
//! `FileDescriptorSet`s the server registers for gRPC reflection: they gate which paths are
//! captured and label the `service`/`method` span fields, but the payload itself is never decoded
//! against them. Emitted at `trace` level, so it is a no-op unless an operator opts in by enabling
//! the target:
//!
//! - `RUST_LOG="info,grpc_request=trace"` — capture every method.
//! - `RUST_LOG='info,grpc_request[{service=sui.rpc.v2.LedgerService}]=trace'` — capture a single
//!   service (or `[{method=GetObject}]` for a single method). The service and method are recorded
//!   on the enclosing span so `EnvFilter` span-field directives can subset them. Under
//!   `telemetry-subscribers` this additionally requires `TOKIO_SPAN_LEVEL=trace`, because its
//!   global level filter otherwise rejects the trace-level `capture` span the directive matches
//!   against. Use the bare-braces form shown here: directives that also name the span
//!   (`grpc_request[capture{...}]`) are not recognized by the capture gate. Note that under
//!   `telemetry-subscribers` a field-scoped directive subsets the *output*, not the capture
//!   overhead — non-matching requests are still parsed before their events are dropped.
//! - Combine with `RUST_LOG_TAILS="grpc_request=/var/log/sui/grpc_request.jsonl"` (see
//!   `telemetry-subscribers`) to additionally write the events to a dedicated newline-delimited
//!   JSON file, e.g. as a replay corpus.
//!
//! Payloads may contain sensitive arguments.

mod body;
mod layer;
mod service;

pub use self::body::RequestLogBody;
pub use self::layer::GrpcRequestLogLayer;
pub use self::service::GrpcRequestLog;

/// The tracing target captured requests are emitted on.
pub const TARGET: &str = "grpc_request";

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::convert::Infallible;
    use std::sync::Arc;
    use std::sync::Mutex;

    use base64::Engine as _;
    use bytes::Bytes;
    use http_body::Frame;
    use http_body_util::BodyExt;
    use http_body_util::Empty;
    use http_body_util::Full;
    use http_body_util::StreamBody;
    use prost::Message;
    use tower::Layer;
    use tower::ServiceExt;
    use tracing::field::Field;
    use tracing::field::Visit;
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::Layer as SubscriberLayer;
    use tracing_subscriber::layer::Context;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    /// One recorded event: every field (including the implicit `message`) rendered to a string.
    type Event = BTreeMap<String, String>;

    /// A `tracing` layer that records the fields of every event it receives, so tests can assert
    /// which captures survived an `EnvFilter` and what they carried.
    #[derive(Clone, Default)]
    struct CaptureLayer {
        events: Arc<Mutex<Vec<Event>>>,
    }

    impl<S: tracing::Subscriber> SubscriberLayer<S> for CaptureLayer {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut visitor = FieldVisitor(Event::new());
            event.record(&mut visitor);
            self.events.lock().unwrap().push(visitor.0);
        }
    }

    struct FieldVisitor(Event);

    impl Visit for FieldVisitor {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0.insert(field.name().to_owned(), format!("{value:?}"));
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            self.0.insert(field.name().to_owned(), value.to_owned());
        }
    }

    /// gRPC-frame `message` with the given compressed `flag`.
    fn frame(flag: u8, message: &[u8]) -> Vec<u8> {
        let mut framed = vec![flag];
        framed.extend_from_slice(&(message.len() as u32).to_be_bytes());
        framed.extend_from_slice(message);
        framed
    }

    /// Raw (unframed) protobuf bytes of a `HealthCheckRequest` for `service`.
    fn health_check_bytes(service: &str) -> Vec<u8> {
        tonic_health::pb::HealthCheckRequest {
            service: service.to_owned(),
        }
        .encode_to_vec()
    }

    /// gRPC-frame a `HealthCheckRequest` for `service`.
    fn framed_health_check(service: &str) -> Vec<u8> {
        frame(0, &health_check_bytes(service))
    }

    /// The base64 payload a captured `HealthCheckRequest` for `service` is expected to log.
    fn base64_health_check(service: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(health_check_bytes(service))
    }

    fn health_layer() -> GrpcRequestLogLayer {
        GrpcRequestLogLayer::from_encoded_file_descriptor_sets([
            tonic_health::pb::FILE_DESCRIPTOR_SET,
        ])
        .unwrap()
    }

    /// Mimics `telemetry-subscribers`' global level filter, which rejects every *span* callsite
    /// above INFO at registration time (the `TOKIO_SPAN_LEVEL` default) while letting events
    /// through to the per-layer filters.
    struct SpanLevelCap;

    impl<S: tracing::Subscriber> SubscriberLayer<S> for SpanLevelCap {
        fn register_callsite(
            &self,
            metadata: &'static tracing::Metadata<'static>,
        ) -> tracing::subscriber::Interest {
            use tracing::level_filters::LevelFilter;
            if metadata.is_span() && LevelFilter::from_level(*metadata.level()) > LevelFilter::INFO
            {
                tracing::subscriber::Interest::never()
            } else {
                tracing::subscriber::Interest::sometimes()
            }
        }
    }

    /// Send one request with `body` through `layer` while `subscriber` is installed.
    async fn send_body<S, B>(
        subscriber: S,
        layer: GrpcRequestLogLayer,
        path: &str,
        content_type: &str,
        body: B,
    ) where
        S: tracing::Subscriber + Send + Sync + 'static,
        B: http_body::Body<Data = Bytes> + Send + 'static,
        B::Error: std::fmt::Debug,
    {
        let _guard = tracing::subscriber::set_default(subscriber);

        let service = layer.layer(tower::service_fn(
            |request: http::Request<RequestLogBody<B>>| async move {
                // Drive the request body to completion, as a real gRPC server would.
                request.into_body().collect().await.unwrap();
                Ok::<_, Infallible>(http::Response::new(Empty::<Bytes>::new()))
            },
        ));

        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(path)
            .header(http::header::CONTENT_TYPE, content_type)
            .body(body)
            .unwrap();

        service.oneshot(request).await.unwrap();
    }

    /// Send one request with `body` through `layer` under a subscriber configured with
    /// `directive`, and return the events that were actually logged (i.e. passed the filter).
    async fn capture_events<B>(
        directive: &str,
        layer: GrpcRequestLogLayer,
        path: &str,
        content_type: &str,
        body: B,
    ) -> Vec<Event>
    where
        B: http_body::Body<Data = Bytes> + Send + 'static,
        B::Error: std::fmt::Debug,
    {
        let capture_layer = CaptureLayer::default();
        let subscriber = tracing_subscriber::registry()
            .with(EnvFilter::new(directive))
            .with(capture_layer.clone());
        send_body(subscriber, layer, path, content_type, body).await;

        let events = capture_layer.events.lock().unwrap();
        events.clone()
    }

    /// Send one framed `HealthCheckRequest` under `directive`, returning the logged payloads.
    async fn capture_with_filter(directive: &str, path: &str, content_type: &str) -> Vec<String> {
        capture_events(
            directive,
            health_layer(),
            path,
            content_type,
            Full::new(Bytes::from(framed_health_check("x"))),
        )
        .await
        .into_iter()
        .filter_map(|mut event| event.remove("payload"))
        .collect()
    }

    #[tokio::test]
    async fn target_directive_captures_canonical_json() {
        let payloads = capture_with_filter(
            "grpc_request=trace",
            "/grpc.health.v1.Health/Check",
            "application/grpc",
        )
        .await;

        assert_eq!(payloads, vec![base64_health_check("x")]);
    }

    /// Regression test for running under `telemetry-subscribers`: its global level filter kills
    /// the trace-level `capture` span callsite, so the capture gate must not depend on the span
    /// being enabled when a plain target directive is set.
    #[tokio::test]
    async fn target_directive_captures_under_global_span_level_cap() {
        let capture_layer = CaptureLayer::default();
        let subscriber = tracing_subscriber::registry()
            .with(EnvFilter::new("grpc_request=trace"))
            .with(capture_layer.clone())
            .with(SpanLevelCap);
        send_body(
            subscriber,
            health_layer(),
            "/grpc.health.v1.Health/Check",
            "application/grpc",
            Full::new(Bytes::from(framed_health_check("x"))),
        )
        .await;

        let events = capture_layer.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["payload"], base64_health_check("x"));
    }

    #[tokio::test]
    async fn disabled_target_captures_nothing() {
        let payloads =
            capture_with_filter("info", "/grpc.health.v1.Health/Check", "application/grpc").await;

        assert_eq!(payloads, Vec::<String>::new());
    }

    #[tokio::test]
    async fn service_span_field_directive_captures_matching_service() {
        let payloads = capture_with_filter(
            "grpc_request[{service=grpc.health.v1.Health}]=trace",
            "/grpc.health.v1.Health/Check",
            "application/grpc",
        )
        .await;

        assert_eq!(payloads, vec![base64_health_check("x")]);
    }

    #[tokio::test]
    async fn service_span_field_directive_skips_other_services() {
        let payloads = capture_with_filter(
            "grpc_request[{service=some.other.Service}]=trace",
            "/grpc.health.v1.Health/Check",
            "application/grpc",
        )
        .await;

        assert_eq!(payloads, Vec::<String>::new());
    }

    #[tokio::test]
    async fn method_span_field_directive_captures_matching_method() {
        let payloads = capture_with_filter(
            "grpc_request[{method=Check}]=trace",
            "/grpc.health.v1.Health/Check",
            "application/grpc",
        )
        .await;

        assert_eq!(payloads, vec![base64_health_check("x")]);
    }

    #[tokio::test]
    async fn unknown_path_captures_nothing() {
        let payloads = capture_with_filter(
            "grpc_request=trace",
            "/unknown.Service/Method",
            "application/grpc",
        )
        .await;

        assert_eq!(payloads, Vec::<String>::new());
    }

    #[tokio::test]
    async fn non_grpc_content_type_captures_nothing() {
        let payloads = capture_with_filter(
            "grpc_request=trace",
            "/grpc.health.v1.Health/Check",
            "application/json",
        )
        .await;

        assert_eq!(payloads, Vec::<String>::new());
    }

    /// A message delivered in chunks that straddle its frame boundary is still captured
    /// correctly, and a second message in the same body hits the per-request cap instead of
    /// being captured.
    #[tokio::test]
    async fn chunked_body_captures_first_message_and_caps_the_rest() {
        let mut bytes = framed_health_check("a");
        bytes.extend_from_slice(&framed_health_check("b"));
        let chunks: Vec<Result<Frame<Bytes>, Infallible>> = bytes
            .chunks(3)
            .map(|chunk| Ok(Frame::data(Bytes::copy_from_slice(chunk))))
            .collect();

        let events = capture_events(
            "grpc_request=trace",
            health_layer(),
            "/grpc.health.v1.Health/Check",
            "application/grpc",
            StreamBody::new(futures::stream::iter(chunks)),
        )
        .await;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["payload"], base64_health_check("a"));
        assert_eq!(events[0]["message_index"], "0");
        assert!(events[1]["message"].contains("too many messages"));
    }

    /// An oversized message emits a payload-less event instead of being captured. One message per
    /// body, since the per-request cap stops capture after the first.
    #[tokio::test]
    async fn oversized_message_emits_payloadless_event() {
        let bytes = frame(0, &[0x20; 32]); // over the 16-byte cap below

        let events = capture_events(
            "grpc_request=trace",
            health_layer().with_max_captured_message_size(16),
            "/grpc.health.v1.Health/Check",
            "application/grpc",
            Full::new(Bytes::from(bytes)),
        )
        .await;

        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["skipped"], "message too large");
        assert_eq!(events[0]["message_len"], "32");
        assert!(!events[0].contains_key("payload"));
    }

    /// A compressed message emits a payload-less event instead of being captured (this layer
    /// doesn't decompress).
    #[tokio::test]
    async fn compressed_message_emits_payloadless_event() {
        let bytes = frame(1, b"zz");

        let events = capture_events(
            "grpc_request=trace",
            health_layer(),
            "/grpc.health.v1.Health/Check",
            "application/grpc",
            Full::new(Bytes::from(bytes)),
        )
        .await;

        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["skipped"], "compressed message");
    }

    /// A message need not be valid protobuf against the resolved method to be captured — the raw
    /// bytes are logged as-is, without ever being decoded.
    #[tokio::test]
    async fn invalid_protobuf_message_is_still_captured() {
        let garbage = [0xFF]; // invalid wire type: would fail to decode
        let bytes = frame(0, &garbage);

        let events = capture_events(
            "grpc_request=trace",
            health_layer(),
            "/grpc.health.v1.Health/Check",
            "application/grpc",
            Full::new(Bytes::from(bytes)),
        )
        .await;

        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0]["payload"],
            base64::engine::general_purpose::STANDARD.encode(garbage)
        );
    }

    /// A body packed with tiny frames stops being captured after the per-request cap, with one
    /// event recording the truncation.
    #[tokio::test]
    async fn capture_stops_after_max_messages() {
        // 70 empty messages: each frame is just the 5-byte prefix.
        let events = capture_events(
            "grpc_request=trace",
            health_layer(),
            "/grpc.health.v1.Health/Check",
            "application/grpc",
            Full::new(Bytes::from(vec![0u8; 5 * 70])),
        )
        .await;

        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["payload"], "");
        assert!(events[1]["message"].contains("too many messages"));
        assert_eq!(events[1]["message_count"], "1");
    }
}
