// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use async_graphql::Response;
use async_graphql::ServerError;
use async_graphql::ServerResult;
use async_graphql::Variables;
use async_graphql::extensions::Extension;
use async_graphql::extensions::ExtensionContext;
use async_graphql::extensions::ExtensionFactory;
use async_graphql::extensions::NextExecute;
use async_graphql::extensions::NextParseQuery;
use async_graphql::parser::types::ExecutableDocument;
use async_graphql::parser::types::OperationType;
use timeout_tracing::CaptureSpanTrace;
use timeout_tracing::timeout;
use tracing::warn;

use crate::error::request_timeout;
use crate::extensions::logging::Session;

/// How long to wait for each kind of operation before timing out.
pub(crate) struct TimeoutConfig {
    pub(crate) query: Duration,
    pub(crate) mutation: Duration,
}

/// The timeout extension is responsible for limiting the amount of time spent serving any single
/// request. It is configured by [RpcConfig] which it expects to find in its context. Timeout
/// durations are configured separately for mutations and for queries.
pub(crate) struct Timeout(Arc<TimeoutConfig>);

struct TimeoutExt {
    config: Arc<TimeoutConfig>,
    is_mutation: AtomicBool,
}

impl Timeout {
    pub(crate) fn new(config: TimeoutConfig) -> Self {
        Self(Arc::new(config))
    }
}

impl ExtensionFactory for Timeout {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(TimeoutExt {
            config: self.0.clone(),
            is_mutation: AtomicBool::new(false),
        })
    }
}

#[async_trait::async_trait]
impl Extension for TimeoutExt {
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let document = next.run(ctx, query, variables).await?;

        self.is_mutation.store(
            document
                .operations
                .iter()
                .any(|(_, op)| op.node.ty == OperationType::Mutation),
            Ordering::Relaxed,
        );

        Ok(document)
    }

    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let is_mutation = self.is_mutation.load(Ordering::Relaxed);
        let limit = if is_mutation {
            self.config.mutation
        } else {
            self.config.query
        };

        timeout(limit, CaptureSpanTrace, next.run(ctx, operation_name))
            .await
            .unwrap_or_else(|e| {
                let kind = if is_mutation { "Mutation" } else { "Query" };
                let Session { uuid, .. } = ctx.data_unchecked();
                warn!(request_id = %uuid, %kind, "Request timed out: {e}");
                Response::from_errors(vec![ServerError::from(request_timeout(kind, limit))])
            })
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;
    use std::net::SocketAddr;

    use async_graphql::EmptyMutation;
    use async_graphql::EmptySubscription;
    use async_graphql::Object;
    use async_graphql::Schema;
    use async_graphql::Value;
    use async_graphql::extensions::Tracing;
    use insta::assert_snapshot;
    use itertools::Itertools;
    use regex::Regex;
    use telemetry_subscribers::TelemetryConfig;
    use uuid::Uuid;

    use crate::error::code;

    use super::*;

    #[derive(Clone)]
    struct Root(Duration);

    #[Object]
    impl Root {
        async fn op(&self) -> bool {
            tokio::time::sleep(self.0).await;
            true
        }
    }

    /// The request takes less than the timeout to handle, so it should pass.
    #[tokio::test]
    async fn test_query_timeout_pass() {
        let zero = Duration::from_millis(0);
        let delay = Duration::from_millis(200);
        let response = Schema::build(Root(delay / 2), EmptyMutation, EmptySubscription)
            .extension(Timeout::new(TimeoutConfig {
                query: delay,
                mutation: zero,
            }))
            .finish()
            .execute("query { op }")
            .await;

        assert!(response.is_ok());
    }

    /// Like [test_query_timeout_pass], but for a mutation.
    #[tokio::test]
    async fn test_mutation_timeout_pass() {
        let zero = Duration::from_millis(0);
        let delay = Duration::from_millis(200);
        let response = Schema::build(Root(zero), Root(delay / 2), EmptySubscription)
            .extension(Timeout::new(TimeoutConfig {
                query: zero,
                mutation: delay,
            }))
            .finish()
            .execute("mutation { op }")
            .await;

        assert!(response.is_ok());
    }

    /// The request takes longer than the timeout to handle, so it should fail.
    #[tokio::test]
    async fn test_query_timeout_fail() {
        let timeout = Duration::from_millis(200);
        let event = test_timeout_fail(
            timeout,
            Duration::ZERO,
            timeout * 2,
            "query { op }",
            "Query",
        )
        .await;
        assert_snapshot!(event, @r"
        [WARN] Request timed out: timeout elapsed at:
        trace 0:
           0: async_graphql::graphql::field
                   with path: op, parent_type: Root, return_type: Boolean!
           1: async_graphql::graphql::execute
           2: async_graphql::graphql::request
         request_id=ffffffff-ffff-ffff-ffff-ffffffffffff kind=Query
        ")
    }

    /// Like [test_query_timeout_fail], but for a mutation.
    #[tokio::test]
    async fn test_mutation_timeout_fail() {
        let timeout = Duration::from_millis(200);
        let event = test_timeout_fail(
            Duration::ZERO,
            timeout,
            timeout * 2,
            "mutation { op }",
            "Mutation",
        )
        .await;
        assert_snapshot!(event, @r"
        [WARN] Request timed out: timeout elapsed at:
        trace 0:
           0: async_graphql::graphql::field
                   with path: op, parent_type: Root, return_type: Boolean!
           1: async_graphql::graphql::execute
           2: async_graphql::graphql::request
         request_id=ffffffff-ffff-ffff-ffff-ffffffffffff kind=Mutation
        ")
    }

    /// Mutations are resolved sequentially, and the timeout should apply to the total time spent
    /// on the request.
    #[tokio::test]
    async fn test_mutation_additive_timeout() {
        let timeout = Duration::from_millis(200);
        let event = test_timeout_fail(
            Duration::ZERO,
            timeout,
            timeout / 2,
            "mutation { a:op b:op c:op }",
            "Mutation",
        )
        .await;
        assert_snapshot!(event, @r"
        [WARN] Request timed out: timeout elapsed at:
        trace 0:
           0: async_graphql::graphql::field
                   with path: b, parent_type: Root, return_type: Boolean!
           1: async_graphql::graphql::execute
           2: async_graphql::graphql::request
         request_id=ffffffff-ffff-ffff-ffff-ffffffffffff kind=Mutation
        ")
    }

    async fn test_timeout_fail(
        query_timeout: Duration,
        mutation_timeout: Duration,
        delay: Duration,
        request: &str,
        expected_error: &str,
    ) -> String {
        // Enable tracing, configured by environment variables.
        let (_guard, handle) = TelemetryConfig::new()
            .with_set_global_default(false)
            // Enable to match default in main.rs
            .with_enable_error_layer(true)
            // Required for handle.get_test_layer_events()
            .with_enable_test_layer(true)
            .init();

        let root = Root(delay);

        let response = Schema::build(root.clone(), root, EmptySubscription)
            // Timeout reads session data. This either needs to be set by the GraphQL framework or
            // a test like this if bypassing the GraphQL framework.
            .data(Session {
                // ffffffff-ffff-ffff-ffff-ffffffffffff
                uuid: Uuid::from_bytes([255; 16]),
                addr: SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0),
            })
            .extension(Timeout::new(TimeoutConfig {
                query: query_timeout,
                mutation: mutation_timeout,
            }))
            .extension(Tracing)
            .finish()
            .execute(request)
            .await;

        assert!(response.is_err());

        let error = &response.errors[0];
        assert!(error.message.contains(expected_error));
        assert_eq!(
            error.extensions.as_ref().unwrap().get("code"),
            Some(&Value::String(code::REQUEST_TIMEOUT.into()))
        );

        let events = handle.get_test_layer_events();
        assert_eq!(events.len(), 1);
        // Remove line number strings to avoid test churn
        // example: "             at /Users/evanwall/.cargo/git/checkouts/async-graphql-7336e61dcafca7ed/7be9351/src/extensions/tracing.rs:135"
        let re = Regex::new(r"\s+at\s.+").unwrap();
        events[0].split("\n").filter(|s| !re.is_match(s)).join("\n")
    }
}
