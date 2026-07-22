// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::task::{Context, Poll};

use http::Request;
use prost_reflect::DescriptorPool;
use tower::Service;
use tracing::trace_span;

use super::TARGET;
use super::body::{CaptureState, RequestLogBody};

/// Middleware that captures decoded gRPC request messages on the [`TARGET`] tracing target.
///
/// See the [module docs](super) for how captures are enabled and collected.
#[derive(Clone)]
pub struct GrpcRequestLog<S> {
    inner: S,
    pool: DescriptorPool,
    max_captured_message_size: usize,
}

impl<S> GrpcRequestLog<S> {
    pub(crate) fn new(inner: S, pool: DescriptorPool, max_captured_message_size: usize) -> Self {
        Self {
            inner,
            pool,
            max_captured_message_size,
        }
    }

    /// Decide whether to capture `request`, and if so set up the state the body wrapper needs:
    /// the span carrying the `service`/`method` fields that `EnvFilter` span-field directives
    /// filter on. Returns `None` (pass the body through untouched) when the target is disabled,
    /// the request is not gRPC, or its path does not resolve to a method in the descriptor pool
    /// (e.g. a service whose `FileDescriptorSet` was not registered).
    fn capture_state<B>(&self, request: &Request<B>) -> Option<CaptureState> {
        // Two probes, because they fail in complementary ways:
        //
        // - The event probe covers plain `grpc_request=trace` directives, and keeps working under
        //   `telemetry-subscribers`, whose global level filter rejects every *span* callsite above
        //   `TOKIO_SPAN_LEVEL` (default `info`) — trace events still flow, trace spans do not.
        // - The span probe covers field-scoped directives like
        //   `grpc_request[{service=...}]=trace`, for which `EnvFilter` disables events outside a
        //   matching span, so the event probe — before any span exists — fails. It declares the
        //   same field names as the real span below because `EnvFilter` only considers a
        //   field-scoped directive for callsites that have those fields. Under
        //   `telemetry-subscribers` this additionally requires `TOKIO_SPAN_LEVEL=trace` so the
        //   span survives its global level filter.
        //
        // When no `grpc_request` directive is set at all, both probes short-circuit on cached
        // callsite interest.
        //
        // Both probes are deliberately evaluated (no `||` short-circuit): under the full
        // `telemetry-subscribers` stack, skipping the span probe when the event probe already
        // passed was empirically observed to break field-scoped capture — the value-matched
        // events stopped reaching any layer. Evaluating the span probe registers its
        // fields-bearing callsite with every filter before the first `capture` span is created.
        let event_enabled = tracing::event_enabled!(target: TARGET, tracing::Level::TRACE);
        let span_enabled =
            tracing::span_enabled!(target: TARGET, tracing::Level::TRACE, service, method);
        if !(event_enabled || span_enabled) {
            return None;
        }

        // Only gRPC requests carry the length-prefixed message framing the body wrapper parses.
        // This also skips non-gRPC routes merged into the same router.
        if !request
            .headers()
            .get(http::header::CONTENT_TYPE)?
            .to_str()
            .ok()?
            .starts_with("application/grpc")
        {
            return None;
        }

        let path = request.uri().path();
        let (service, method) = path.strip_prefix('/')?.split_once('/')?;
        let service = self.pool.get_service_by_name(service)?;
        let method = service.methods().find(|m| m.name() == method)?;

        let span = trace_span!(
            target: TARGET,
            "capture",
            service = %service.full_name(),
            method = %method.name(),
        );

        // The probes above are callsite-level, so under a field-scoped directive they pass for
        // *every* request regardless of its service/method values. Re-check with this request's
        // span entered, so a global `EnvFilter` — which consults the span's recorded field values
        // — prunes requests whose events would all be dropped anyway before any buffering or
        // decoding happens. Per-layer filtered stacks (e.g. `telemetry-subscribers`) answer this
        // aggregate check permissively and filter at dispatch instead; there the per-request
        // message-count and message-size caps bound the wasted work.
        if !span.in_scope(|| tracing::event_enabled!(target: TARGET, tracing::Level::TRACE)) {
            return None;
        }

        Some(CaptureState::new(
            span,
            path.to_owned(),
            self.max_captured_message_size,
        ))
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for GrpcRequestLog<S>
where
    S: Service<Request<RequestLogBody<ReqBody>>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let capture = self.capture_state(&request);
        self.inner
            .call(request.map(|body| RequestLogBody::new(body, capture)))
    }
}
