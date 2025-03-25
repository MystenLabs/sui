// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextParseQuery, NextRequest},
    parser::types::ExecutableDocument,
    value, Response, ServerResult, Variables,
};
use headers::ContentLength;

use self::error::{Error, ErrorKind};
use self::show_usage::ShowUsage;

mod chain;
mod error;
mod input;
mod payload;
pub(crate) mod show_usage;
mod visitor;

pub(crate) struct QueryLimitsConfig {
    pub max_query_nodes: u32,
    pub max_query_depth: u32,
    pub max_query_payload_size: u32,
    pub max_tx_payload_size: u32,

    pub tx_payload_args: BTreeSet<(&'static str, &'static str, &'static str)>,
}

/// Extension factory for adding checks that the query is within configurable limits.
pub(crate) struct QueryLimitsChecker(Arc<QueryLimitsConfig>);

struct QueryLimitsCheckerExt {
    limits: Arc<QueryLimitsConfig>,
    usage: Mutex<Option<Usage>>,
}

struct Usage {
    input: input::Usage,
    payload: payload::Usage,
}

impl QueryLimitsConfig {
    /// Requests to this service can definitely not exceed this size, in bytes.
    pub fn max_payload_size(&self) -> u32 {
        self.max_query_payload_size + self.max_tx_payload_size
    }
}

impl QueryLimitsChecker {
    pub(crate) fn new(limits: QueryLimitsConfig) -> Self {
        Self(Arc::new(limits))
    }
}

impl ExtensionFactory for QueryLimitsChecker {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(QueryLimitsCheckerExt {
            limits: self.0.clone(),
            usage: Mutex::new(None),
        })
    }
}

#[async_trait::async_trait]
impl Extension for QueryLimitsCheckerExt {
    /// Validates that the query is within configurable limits before it is run, to protect the
    /// rest of the system from doing too much work. Tests ensure that:
    ///
    /// - The query does not take up too much space.
    /// - The query is not too large or too deep, as an AST.
    /// - If the query is large, that does not translate into a lot of query work (it's okay to
    ///   have large binary payloads to handle execution, but we don't want a query with a big
    ///   footprint to translate into a query that requires a lot of work to execute).
    /// - The query will not produce too large a response (estimated based on the upperbound number
    ///   of output nodes that input query could produce).
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let &ContentLength(length) = ctx.data_unchecked();

        if length > self.limits.max_payload_size() as u64 {
            Err(Error::new_global(ErrorKind::PayloadSizeOverall {
                limit: self.limits.max_payload_size(),
                actual: length,
            }))?;
        }

        let doc = next.run(ctx, query, variables).await?;

        let input = input::check(self.limits.as_ref(), &doc)?;
        let payload = payload::check(
            self.limits.as_ref(),
            length,
            &ctx.schema_env.registry,
            &doc,
            variables,
        )?;

        if let Some(ShowUsage(_)) = ctx.data_opt() {
            *self.usage.lock().unwrap() = Some(Usage { input, payload });
        }

        Ok(doc)
    }

    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        let mut response = next.run(ctx).await;

        if let Some(Usage { input, payload }) = self.usage.lock().unwrap().take() {
            response = response.extension("usage", value!({"input": input, "payload": payload}));
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::{EmptySubscription, Object, Request, Schema};
    use async_graphql_value::ConstValue;
    use axum::http::HeaderValue;
    use insta::{assert_json_snapshot, assert_snapshot};

    use super::*;

    struct Query;
    struct Mutation;

    #[Object]
    impl Query {
        async fn a(&self) -> Query {
            Query
        }

        async fn b(&self) -> Query {
            Query
        }

        async fn c(&self) -> Query {
            Query
        }

        async fn z(&self) -> bool {
            true
        }

        async fn tx(&self, bytes: String, other: usize) -> usize {
            bytes.len() + other
        }

        async fn zk(&self, bytes: String, sigs: Vec<String>) -> usize {
            bytes.len() + sigs.len()
        }
    }

    #[Object]
    impl Mutation {
        async fn tx(&self, bytes: String, other: usize) -> usize {
            bytes.len() + other
        }

        async fn zk(&self, bytes: String, sigs: Vec<String>) -> usize {
            bytes.len() + sigs.len()
        }
    }

    fn config() -> QueryLimitsConfig {
        QueryLimitsConfig {
            max_query_nodes: 10,
            max_query_depth: 5,
            max_query_payload_size: 1000,
            max_tx_payload_size: 1000,
            tx_payload_args: BTreeSet::from_iter([
                ("Mutation", "tx", "bytes"),
                ("Query", "tx", "bytes"),
                ("Query", "zk", "bytes"),
                ("Query", "zk", "sigs"),
            ]),
        }
    }

    fn schema(limits: QueryLimitsConfig) -> Schema<Query, Mutation, EmptySubscription> {
        Schema::build(Query, Mutation, EmptySubscription)
            .extension(QueryLimitsChecker::new(limits))
            .finish()
    }

    async fn execute(schema: &Schema<Query, Mutation, EmptySubscription>, query: &str) -> Response {
        schema
            .execute(
                Request::from(query)
                    .data(ShowUsage(HeaderValue::from_static("true")))
                    .data(ContentLength(query.len() as u64)),
            )
            .await
    }

    /// Extract a particular `kind` of usage information from the response.
    fn usage(response: Response, kind: &str) -> ConstValue {
        let ConstValue::Object(usage) = response.extensions.get("usage").unwrap() else {
            panic!("Expected usage to be an object");
        };

        usage.get(kind).unwrap().clone()
    }

    #[tokio::test]
    async fn test_pass_limits() {
        let schema = schema(config());
        let response = execute(&schema, "{ a { b { c { a { z } } } } }").await;

        assert_snapshot!(response.extensions.get("usage").unwrap(), @"{input: {nodes: 5,depth: 5},payload: {query_payload_size: 29,tx_payload_size: 0}}");
    }

    #[tokio::test]
    async fn test_too_deep() {
        let schema = schema(config());
        let response = execute(&schema, "{ a { b { c { a { b { c { z } } } } } } }").await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Query nesting is over 5",
              "locations": [
                {
                  "line": 1,
                  "column": 23
                }
              ],
              "path": [
                "a",
                "b",
                "c",
                "a",
                "b"
              ],
              "extensions": {
                "code": "GRAPHQL_VALIDATION_FAILED"
              }
            }
          ]
        }
        "###);
    }

    #[tokio::test]
    async fn test_too_many_input_nodes() {
        let schema = schema(config());
        let response = execute(
            &schema,
            r#"{
              a { z }
              b { z }
              c { z }
              d: a { z }
              e: b { z }
              f: c { z }
            }"#,
        )
        .await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Query has over 10 nodes",
              "locations": [
                {
                  "line": 6,
                  "column": 22
                }
              ],
              "path": [
                "b"
              ],
              "extensions": {
                "code": "GRAPHQL_VALIDATION_FAILED"
              }
            }
          ]
        }
        "###);
    }

    #[tokio::test]
    async fn test_missing_fragment_def() {
        let schema = schema(config());
        let response = execute(&schema, "query { ...IDontExist }").await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Fragment IDontExist referred to but not found in document",
              "locations": [
                {
                  "line": 1,
                  "column": 9
                }
              ],
              "extensions": {
                "code": "GRAPHQL_VALIDATION_FAILED"
              }
            }
          ]
        }
        "###);
    }

    /// If a fragment is used multiple times, it is counted multiple times for the purposes of node
    /// count.
    #[tokio::test]
    async fn test_too_many_input_nodes_fragment_spread() {
        let schema = schema(config());
        let response = execute(
            &schema,
            r#"
            query {
              a { ...F }
              b { ...F }
              c { ...F }
            }

            fragment F on Query { a { b { c { a { z } } } } }
            "#,
        )
        .await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Query has over 10 nodes",
              "locations": [
                {
                  "line": 8,
                  "column": 39
                }
              ],
              "path": [
                "b",
                "a"
              ],
              "extensions": {
                "code": "GRAPHQL_VALIDATION_FAILED"
              }
            }
          ]
        }
        "###);
    }

    /// The depth of a fragment is added to the depth at which the fragment is spread.
    #[tokio::test]
    async fn test_too_deep_fragment_spread() {
        let schema = schema(config());
        let response = execute(
            &schema,
            r#"
            query { a { b { c { ...F } } } }
            fragment F on Query { a { z } }
            "#,
        )
        .await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Query nesting is over 5",
              "locations": [
                {
                  "line": 3,
                  "column": 39
                }
              ],
              "path": [
                "a",
                "b",
                "c",
                "a"
              ],
              "extensions": {
                "code": "GRAPHQL_VALIDATION_FAILED"
              }
            }
          ]
        }
        "###);
    }

    /// The checker performs an initial size test before parsing the request -- if there is no chance
    /// the request will fit within size constraints, it is rejected immediately.
    #[tokio::test]
    async fn test_overall_payload_size_too_small() {
        let schema = schema(QueryLimitsConfig {
            max_query_payload_size: 5,
            max_tx_payload_size: 5,
            ..config()
        });

        let response = execute(&schema, "{ a { b { c { a { z } } } } }").await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Request too large 29B > 10B",
              "extensions": {
                "code": "GRAPHQL_VALIDATION_FAILED"
              }
            }
          ]
        }
        "###);
    }

    #[tokio::test]
    async fn test_query_payload_too_large() {
        let schema = schema(QueryLimitsConfig {
            max_query_payload_size: 5,
            max_tx_payload_size: 50,
            ..config()
        });

        let response = execute(&schema, "{ a { b { c { a { z } } } } }").await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Query payload too large: 29B > 5B",
              "extensions": {
                "code": "GRAPHQL_VALIDATION_FAILED"
              }
            }
          ]
        }
        "###);
    }

    #[tokio::test]
    async fn test_tx_payload_too_large() {
        let schema = schema(QueryLimitsConfig {
            max_tx_payload_size: 5,
            max_query_payload_size: 50,
            ..config()
        });

        let response = execute(&schema, r#"{ tx(bytes: "hello world", other: 1) }"#).await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Transaction payload exceeded limit of 5B",
              "locations": [
                {
                  "line": 1,
                  "column": 13
                }
              ],
              "extensions": {
                "code": "GRAPHQL_VALIDATION_FAILED"
              }
            }
          ]
        }
        "###);
    }

    /// Test that transaction payloads in top-level fields get picked up and counted correctly.
    #[tokio::test]
    async fn test_tx_payload_accounting() {
        let schema = schema(config());
        let response = execute(
            &schema,
            r#"
            {
              tx(bytes: "hello world", other: 1)
              zk(bytes: "hello world", sigs: ["a", "b", "c"])
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "payload"), @"{query_payload_size: 113,tx_payload_size: 39}");
    }

    /// Transaction payloads that are in nested fields are not counted.
    #[tokio::test]
    async fn test_nested_tx_payload_ignored() {
        let schema = schema(config());
        let response = execute(
            &schema,
            r#"
            {
              a {
                tx(bytes: "hello world", other: 1)
                zk(bytes: "hello world", sigs: ["a", "b", "c"])
              }
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "payload"), @"{query_payload_size: 190,tx_payload_size: 0}");
    }

    /// The test config specifies the Query.zk contains transaction payloads, but `Mutation.zk`
    /// does not, so the transaction payload reported is different here compared to
    /// [test_tx_payload_accounting].
    #[tokio::test]
    async fn test_tx_payload_type_filter() {
        let schema = schema(config());
        let response = execute(
            &schema,
            r#"
            mutation {
              tx(bytes: "hello world", other: 1)
              zk(bytes: "hello world", sigs: ["a", "b", "c"])
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "payload"), @"{query_payload_size: 148,tx_payload_size: 13}");
    }
}
