// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextExecute, NextParseQuery},
    parser::types::{ExecutableDocument, OperationType},
    Response, ServerError, ServerResult, Variables,
};
use tokio::time::timeout;

use crate::error::request_timeout;

/// How long to wait for each kind of operation before timing out.
pub(crate) struct TimeoutConfig {
    pub query: Duration,
    pub mutation: Duration,
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

        timeout(limit, next.run(ctx, operation_name))
            .await
            .unwrap_or_else(|_| {
                let kind = if is_mutation { "Mutation" } else { "Query" };
                Response::from_errors(vec![ServerError::from(request_timeout(kind, limit))])
            })
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::{EmptyMutation, EmptySubscription, Object, Schema, Value};

    use crate::error::code;

    use super::*;

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
        let zero = Duration::from_millis(0);
        let delay = Duration::from_millis(200);
        let response = Schema::build(Root(delay * 2), EmptyMutation, EmptySubscription)
            .extension(Timeout::new(TimeoutConfig {
                query: delay,
                mutation: zero,
            }))
            .finish()
            .execute("query { op }")
            .await;

        assert!(response.is_err());

        let error = &response.errors[0];
        assert!(error.message.contains("Query"));
        assert_eq!(
            error.extensions.as_ref().unwrap().get("code"),
            Some(&Value::String(code::REQUEST_TIMEOUT.into()))
        )
    }

    /// Like [test_query_timeout_fail], but for a mutation.
    #[tokio::test]
    async fn test_mutation_timeout_fail() {
        let zero = Duration::from_millis(0);
        let delay = Duration::from_millis(200);
        let response = Schema::build(Root(zero), Root(delay * 2), EmptySubscription)
            .extension(Timeout::new(TimeoutConfig {
                query: zero,
                mutation: delay,
            }))
            .finish()
            .execute("mutation { op }")
            .await;

        assert!(response.is_err());

        let error = &response.errors[0];
        assert!(error.message.contains("Mutation"));
        assert_eq!(
            error.extensions.as_ref().unwrap().get("code"),
            Some(&Value::String(code::REQUEST_TIMEOUT.into()))
        )
    }

    /// Mutations are resolved sequentially, and the timeout should apply to the total time spent
    /// on the request.
    #[tokio::test]
    async fn test_mutation_additive_timeout() {
        let zero = Duration::from_millis(0);
        let delay = Duration::from_millis(200);
        let response = Schema::build(Root(zero), Root(delay / 2), EmptySubscription)
            .extension(Timeout::new(TimeoutConfig {
                query: zero,
                mutation: delay,
            }))
            .finish()
            .execute("mutation { a:op b:op c:op }")
            .await;

        assert!(response.is_err());

        let error = &response.errors[0];
        assert!(error.message.contains("Mutation"));
        assert_eq!(
            error.extensions.as_ref().unwrap().get("code"),
            Some(&Value::String(code::REQUEST_TIMEOUT.into()))
        )
    }
}
