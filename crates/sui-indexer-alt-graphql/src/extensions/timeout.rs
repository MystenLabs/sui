// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::OnceLock;
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
use sui_futures::timeout_trace::timeout;
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
    /// Map from operation name to its type, populated once during parsing.
    /// `None` key represents an anonymous (single) operation.
    operation_types: OnceLock<HashMap<Option<String>, OperationType>>,
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
            operation_types: OnceLock::new(),
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

        let types: HashMap<_, _> = document
            .operations
            .iter()
            .map(|(name, op)| (name.map(|n| n.to_string()), op.node.ty))
            .collect();
        let _ = self.operation_types.set(types);

        Ok(document)
    }

    async fn execute(
        &self,
        ctx: &ExtensionContext<'_>,
        operation_name: Option<&str>,
        next: NextExecute<'_>,
    ) -> Response {
        let key = operation_name.map(|s| s.to_string());
        let operation_type = self
            .operation_types
            .get()
            .and_then(|types| types.get(&key).copied())
            .unwrap_or(OperationType::Query);

        // Subscriptions are long-lived streams — a per-request timeout does not apply.
        if operation_type == OperationType::Subscription {
            return next.run(ctx, operation_name).await;
        }

        let is_mutation = operation_type == OperationType::Mutation;
        let limit = if is_mutation {
            self.config.mutation
        } else {
            self.config.query
        };

        timeout(limit, next.run(ctx, operation_name))
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

    use crate::extensions::logging::ClientInfo;
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
    #[tokio::test(start_paused = true)]
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
        assert_eq!(response.data, async_graphql::value!({ "op": true }));
    }

    /// Like [test_query_timeout_pass], but for a mutation.
    #[tokio::test(start_paused = true)]
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
        assert_eq!(response.data, async_graphql::value!({ "op": true }));
    }

    /// The request takes longer than the timeout to handle, so it should fail.
    #[tokio::test(start_paused = true)]
    async fn test_query_timeout_fail() {
        let timeout = Duration::from_millis(200);
        test_timeout_fail(timeout, Duration::ZERO, timeout * 2, "query { op }").await;
    }

    /// Like [test_query_timeout_fail], but for a mutation.
    #[tokio::test(start_paused = true)]
    async fn test_mutation_timeout_fail() {
        let timeout = Duration::from_millis(200);
        test_timeout_fail(Duration::ZERO, timeout, timeout * 2, "mutation { op }").await;
    }

    /// Mutations are resolved sequentially, and the timeout should apply to the total time spent
    /// on the request.
    #[tokio::test(start_paused = true)]
    async fn test_mutation_additive_timeout() {
        let timeout = Duration::from_millis(200);
        test_timeout_fail(
            Duration::ZERO,
            timeout,
            timeout * 3 / 4,
            "mutation { a:op b:op c:op }",
        )
        .await;
    }

    async fn test_timeout_fail(
        query_timeout: Duration,
        mutation_timeout: Duration,
        delay: Duration,
        request: &str,
    ) {
        let root = Root(delay);
        let response = Schema::build(root.clone(), root, EmptySubscription)
            // Timeout reads session data normally supplied by the GraphQL framework.
            .data(Session {
                uuid: Uuid::from_bytes([255; 16]),
                addr: SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0),
                client: ClientInfo::default(),
            })
            .extension(Timeout::new(TimeoutConfig {
                query: query_timeout,
                mutation: mutation_timeout,
            }))
            .finish()
            .execute(request)
            .await;

        assert_eq!(response.data, Value::Null);
        assert_eq!(response.errors.len(), 1);
        assert_eq!(
            response.errors[0].extensions.as_ref().unwrap().get("code"),
            Some(&Value::String(code::REQUEST_TIMEOUT.into()))
        );
    }
}
