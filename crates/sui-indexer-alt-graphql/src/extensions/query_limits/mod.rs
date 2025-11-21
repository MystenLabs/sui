// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use async_graphql::{
    Response, ServerError, ServerResult, ValidationResult, Variables,
    extensions::{
        Extension, ExtensionContext, ExtensionFactory, NextParseQuery, NextRequest, NextValidation,
    },
    parser::types::ExecutableDocument,
    value,
};
use headers::ContentLength;

use crate::{metrics::RpcMetrics, pagination::PaginationConfig};

use self::error::{Error, ErrorKind};
use self::show_usage::ShowUsage;

mod chain;
mod error;
mod input;
mod output;
mod payload;
pub(crate) mod show_usage;
mod visitor;

pub(crate) struct QueryLimitsConfig {
    pub(crate) max_output_nodes: u32,
    pub(crate) max_query_nodes: u32,
    pub(crate) max_query_depth: u32,
    pub(crate) max_query_payload_size: u32,
    pub(crate) max_tx_payload_size: u32,

    pub(crate) tx_payload_args: BTreeSet<(&'static str, &'static str, &'static str)>,
}

/// Extension factory for adding checks that the query is within configurable limits.
pub(crate) struct QueryLimitsChecker {
    limits: Arc<QueryLimitsConfig>,
    metrics: Arc<RpcMetrics>,
}

struct QueryLimitsCheckerExt {
    limits: Arc<QueryLimitsConfig>,
    metrics: Arc<RpcMetrics>,
    doc: Mutex<Option<ParsedDocument>>,
    usage: Mutex<Option<Usage>>,
}

struct ParsedDocument {
    var: Variables,
    doc: ExecutableDocument,
}

struct Usage {
    input: input::Usage,
    payload: payload::Usage,
    output: output::Usage,
}

impl QueryLimitsConfig {
    /// Requests to this service can definitely not exceed this size, in bytes.
    pub(crate) fn max_payload_size(&self) -> u32 {
        self.max_query_payload_size + self.max_tx_payload_size
    }
}

impl QueryLimitsChecker {
    pub(crate) fn new(limits: QueryLimitsConfig, metrics: Arc<RpcMetrics>) -> Self {
        Self {
            limits: Arc::new(limits),
            metrics,
        }
    }
}

impl ExtensionFactory for QueryLimitsChecker {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(QueryLimitsCheckerExt {
            limits: self.limits.clone(),
            metrics: self.metrics.clone(),
            doc: Mutex::new(None),
            usage: Mutex::new(None),
        })
    }
}

#[async_trait::async_trait]
impl Extension for QueryLimitsCheckerExt {
    /// Performs initial checks about content length, and then stashes the parsed document so that
    /// we can run our validation checks on it.
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

        // Stash the parsed document so that we can run our validations on it, after the
        // framework's validations have run.
        *self.doc.lock().unwrap() = Some(ParsedDocument {
            var: variables.clone(),
            doc: doc.clone(),
        });

        Ok(doc)
    }

    /// Run the framework's validation rules, and then run our own validation rules to validate
    /// that the query is within configurable limits before it is run, to protect the rest of the
    /// system from doing too much work. Tests ensure that:
    ///
    /// - The query is not too large or too deep, as an AST.
    /// - If the query is large, that does not translate into a lot of query work (it's okay to
    ///   have large binary payloads to handle execution, but we don't want a query with a big
    ///   footprint to translate into a query that requires a lot of work to execute).
    /// - The query will not produce too large a response (estimated based on the upperbound number
    ///   of output nodes that input query could produce).
    async fn validation(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextValidation<'_>,
    ) -> Result<ValidationResult, Vec<ServerError>> {
        let res = next.run(ctx).await?;

        let Some(ParsedDocument { doc, var }) = self.doc.lock().unwrap().take() else {
            return Ok(res);
        };

        let &ContentLength(length) = ctx.data_unchecked();
        let pagination_config: &PaginationConfig = ctx.data_unchecked();

        let _guard = self.metrics.limits_validation_latency.start_timer();

        let input = input::check(self.limits.as_ref(), &doc)?;

        let payload = payload::check(
            self.limits.as_ref(),
            length,
            &ctx.schema_env.registry,
            &doc,
            &var,
        )?;

        let output = output::check(
            self.limits.as_ref(),
            pagination_config,
            &ctx.schema_env.registry,
            &doc,
            &var,
        )?;

        self.metrics.input_depth.observe(input.depth as f64);
        self.metrics.input_nodes.observe(input.nodes as f64);
        self.metrics.total_payload_size.observe(length as f64);
        self.metrics
            .query_payload_size
            .observe(payload.query_payload_size as f64);
        self.metrics
            .tx_payload_size
            .observe(payload.tx_payload_size as f64);
        self.metrics.output_nodes.observe(output.nodes as f64);

        if let Some(ShowUsage(_)) = ctx.data_opt() {
            *self.usage.lock().unwrap() = Some(Usage {
                input,
                payload,
                output,
            });
        }

        Ok(res)
    }

    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        let mut response = next.run(ctx).await;

        if let Some(Usage {
            input,
            payload,
            output,
        }) = self.usage.lock().unwrap().take()
        {
            response = response.extension(
                "usage",
                value!({
                    "input": input,
                    "payload": payload,
                    "output": output
                }),
            );
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use async_graphql::{EmptySubscription, Object, Request, Schema, connection::Connection};
    use async_graphql_value::ConstValue;
    use axum::http::HeaderValue;
    use insta::{assert_json_snapshot, assert_snapshot};
    use serde_json::json;

    use crate::{api::scalars::json::Json, pagination::PageLimits};

    use super::*;

    #[derive(Clone)]
    struct Query;

    struct Extra;

    struct Mutation;

    #[Object]
    impl Query {
        /// Non-terminal field.
        async fn a(&self) -> Query {
            Query
        }

        /// Non-terminal field.
        async fn b(&self) -> Query {
            Query
        }

        /// Non-terminal field.
        async fn c(&self) -> Query {
            Query
        }

        /// A field that looks like it contains connection page content, but it doesn't, because it's not contained within a paginated field.
        async fn edges(&self) -> Query {
            Query
        }

        /// A field that looks like a multi-get.
        async fn multi_get_q(&self, keys: Vec<String>) -> Vec<Query> {
            vec![Query; keys.len()]
        }

        /// See `Query.edges`.
        async fn nodes(&self) -> Query {
            Query
        }

        /// A paginated field with a non-null return type.
        async fn p(
            &self,
            _first: Option<usize>,
            _last: Option<usize>,
        ) -> Connection<String, Query, Extra> {
            Connection::with_additional_fields(false, false, Extra)
        }

        /// A paginated field with a nullable return type.
        async fn q(
            &self,
            _first: Option<usize>,
            _last: Option<usize>,
        ) -> Option<Connection<String, Query, Extra>> {
            None
        }

        /// Terminal field.
        async fn z(&self) -> bool {
            true
        }

        /// Looks like a transaction execution or dry-run field.
        async fn tx(&self, _transaction: Json, other: usize) -> usize {
            // For testing purposes, just return the other value
            other
        }

        /// Looks like a ZkLogin prover endpoint.
        async fn zk(&self, bytes: String, sigs: Vec<String>) -> usize {
            bytes.len() + sigs.len()
        }
    }

    #[Object]
    impl Extra {
        async fn x(&self) -> Query {
            Query
        }
    }

    #[Object]
    impl Mutation {
        /// Looks like a transaction execution or dry-run field.
        async fn tx(&self, _bytes: Json, other: usize) -> usize {
            // For testing purposes, just return the other value
            other
        }

        /// Looks like a ZkLogin prover endpoint (this field is not included in `tx_payload_args` to test that configuration).
        async fn zk(&self, bytes: String, sigs: Vec<String>) -> usize {
            bytes.len() + sigs.len()
        }
    }

    fn config() -> QueryLimitsConfig {
        QueryLimitsConfig {
            max_output_nodes: 1000,
            max_query_nodes: 10,
            max_query_depth: 5,
            max_query_payload_size: 1000,
            max_tx_payload_size: 1000,
            tx_payload_args: BTreeSet::from_iter([
                ("Mutation", "tx", "bytes"),
                ("Query", "tx", "transaction"),
                ("Query", "zk", "bytes"),
                ("Query", "zk", "sigs"),
            ]),
        }
    }

    fn page() -> PaginationConfig {
        PaginationConfig::new(10, small_page(), Default::default())
    }

    fn small_page() -> PageLimits {
        PageLimits {
            default: 5,
            max: 10,
        }
    }

    fn big_page() -> PageLimits {
        PageLimits {
            default: 10,
            max: 20,
        }
    }

    fn schema(
        limits: QueryLimitsConfig,
        pagination_config: PaginationConfig,
    ) -> Schema<Query, Mutation, EmptySubscription> {
        // Create a throwaway registry and metrics so we can set-up the extension.
        let registry = prometheus::Registry::new();
        let metrics = RpcMetrics::new(&registry);

        Schema::build(Query, Mutation, EmptySubscription)
            .extension(QueryLimitsChecker::new(limits, metrics))
            .data(pagination_config)
            .finish()
    }

    fn request(query: &str) -> Request {
        Request::from(query)
            .data(ShowUsage(HeaderValue::from_static("true")))
            .data(ContentLength(query.len() as u64))
    }

    async fn execute(schema: &Schema<Query, Mutation, EmptySubscription>, query: &str) -> Response {
        schema.execute(request(query)).await
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
        let schema = schema(config(), page());
        let response = execute(&schema, "{ a { b { c { a { z } } } } }").await;
        let usage = response
            .extensions
            .get("usage")
            .unwrap()
            .clone()
            .into_json()
            .unwrap();

        assert_json_snapshot!(usage, @r###"
        {
          "input": {
            "nodes": 5,
            "depth": 5
          },
          "payload": {
            "query_payload_size": 29,
            "tx_payload_size": 0
          },
          "output": {
            "nodes": 5
          }
        }
        "###);
    }

    #[tokio::test]
    async fn test_typename() {
        let schema = schema(config(), page());
        let response = execute(&schema, "{ __typename, alias: __typename }").await;
        let usage = response
            .extensions
            .get("usage")
            .unwrap()
            .clone()
            .into_json()
            .unwrap();

        assert_json_snapshot!(usage, @r###"
        {
          "input": {
            "nodes": 2,
            "depth": 1
          },
          "payload": {
            "query_payload_size": 33,
            "tx_payload_size": 0
          },
          "output": {
            "nodes": 2
          }
        }
        "###);
    }

    #[tokio::test]
    async fn test_variable_optional() {
        let schema = schema(config(), page());
        let response = schema
            .execute(request(
                "query ($first: Int) { p(first: $first) { nodes { z } } }",
            ))
            .await;

        assert_json_snapshot!(response, @r###"
        {
          "data": {
            "p": {
              "nodes": []
            }
          },
          "extensions": {
            "usage": {
              "input": {
                "nodes": 3,
                "depth": 3
              },
              "payload": {
                "query_payload_size": 56,
                "tx_payload_size": 0
              },
              "output": {
                "nodes": 12
              }
            }
          }
        }
        "###);
    }

    #[tokio::test]
    async fn test_variable_null() {
        let schema = schema(config(), page());
        let response = schema
            .execute(
                request("query ($first: Int) { p(first: $first) { nodes { z } } }").variables(
                    Variables::from_json(json!({
                        "first": null,
                    })),
                ),
            )
            .await;

        assert_json_snapshot!(response, @r###"
        {
          "data": {
            "p": {
              "nodes": []
            }
          },
          "extensions": {
            "usage": {
              "input": {
                "nodes": 3,
                "depth": 3
              },
              "payload": {
                "query_payload_size": 56,
                "tx_payload_size": 0
              },
              "output": {
                "nodes": 12
              }
            }
          }
        }
        "###);
    }

    #[tokio::test]
    async fn test_variable_valid_page() {
        let schema = schema(config(), page());
        let response = schema
            .execute(
                request("query ($first: Int) { p(first: $first) { nodes { z } } }").variables(
                    Variables::from_json(json!({
                        "first": 5,
                    })),
                ),
            )
            .await;

        assert_json_snapshot!(response, @r###"
        {
          "data": {
            "p": {
              "nodes": []
            }
          },
          "extensions": {
            "usage": {
              "input": {
                "nodes": 3,
                "depth": 3
              },
              "payload": {
                "query_payload_size": 56,
                "tx_payload_size": 0
              },
              "output": {
                "nodes": 12
              }
            }
          }
        }
        "###);
    }

    #[tokio::test]
    async fn test_variable_huge_page() {
        let schema = schema(config(), page());
        let response = schema
            .execute(
                request("query ($first: Int) { p(first: $first) { nodes { z } } }").variables(
                    Variables::from_json(json!({
                        "first": 1000,
                    })),
                ),
            )
            .await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Page size is too large: 1000 > 10",
              "locations": [
                {
                  "line": 1,
                  "column": 23
                }
              ],
              "path": [
                "p"
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
    async fn test_too_deep() {
        let schema = schema(config(), page());
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
        let schema = schema(config(), page());
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
        let schema = schema(config(), page());
        let response = execute(&schema, "query { ...IDontExist }").await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Unknown fragment: \"IDontExist\"",
              "locations": [
                {
                  "line": 1,
                  "column": 9
                }
              ]
            }
          ]
        }
        "###);
    }

    /// If a fragment is used multiple times, it is counted multiple times for the purposes of node
    /// count.
    #[tokio::test]
    async fn test_too_many_input_nodes_fragment_spread() {
        let schema = schema(config(), page());
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
        let schema = schema(config(), page());
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
        let schema = schema(
            QueryLimitsConfig {
                max_query_payload_size: 5,
                max_tx_payload_size: 5,
                ..config()
            },
            page(),
        );

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
        let schema = schema(
            QueryLimitsConfig {
                max_query_payload_size: 5,
                max_tx_payload_size: 50,
                ..config()
            },
            page(),
        );

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
        let schema = schema(
            QueryLimitsConfig {
                max_tx_payload_size: 5,
                max_query_payload_size: 50,
                ..config()
            },
            page(),
        );

        let response = execute(&schema, r#"{ tx(transaction: "hello world", other: 1) }"#).await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Transaction payload exceeded limit of 5B",
              "locations": [
                {
                  "line": 1,
                  "column": 19
                }
              ],
              "path": [
                "tx"
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
        let schema = schema(config(), page());
        let response = execute(
            &schema,
            r#"
            {
              tx(transaction: "hello world", other: 1)
              zk(bytes: "hello world", sigs: ["a", "b", "c"])
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "payload"), @"{query_payload_size: 119,tx_payload_size: 39}");
    }

    /// Transaction payloads that are in nested fields are not counted.
    #[tokio::test]
    async fn test_nested_tx_payload_ignored() {
        let schema = schema(config(), page());
        let response = execute(
            &schema,
            r#"
            {
              a {
                tx(transaction: "hello world", other: 1)
                zk(bytes: "hello world", sigs: ["a", "b", "c"])
              }
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "payload"), @"{query_payload_size: 196,tx_payload_size: 0}");
    }

    /// The test config specifies the Query.zk contains transaction payloads, but `Mutation.zk`
    /// does not, so the transaction payload reported is different here compared to
    /// [test_tx_payload_accounting].
    #[tokio::test]
    async fn test_tx_payload_type_filter() {
        let schema = schema(config(), page());
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

    /// Test that structured (JSON object) transaction payloads are counted correctly.
    #[tokio::test]
    async fn test_tx_payload_structured_inline() {
        let schema = schema(config(), page());
        let response = execute(
            &schema,
            r#"
            {
              tx(transaction: { sender: "0xabc", data: [1, 2, 3], flag: true }, other: 1)
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "payload"), @"{query_payload_size: 92,tx_payload_size: 39}");
    }

    /// Test that structured transaction payloads in variables are counted correctly.
    #[tokio::test]
    async fn test_tx_payload_structured_variable() {
        let schema = schema(config(), page());
        let query = r#"
            query ($txData: JSON!) {
              tx(transaction: $txData, other: 1)
            }
            "#;

        let variables = BTreeMap::from([(
            "txData".to_string(),
            json!({ "sender": "0xdef", "amount": 100, "nested": { "key": "value" } }),
        )]);

        let request = Request::from(query)
            .data(ShowUsage(HeaderValue::from_static("true")))
            .data(ContentLength(query.len() as u64))
            .variables(Variables::from_json(
                serde_json::to_value(variables).unwrap(),
            ));

        let response = schema.execute(request).await;
        assert_snapshot!(usage(response, "payload"), @"{query_payload_size: 57,tx_payload_size: 56}");
    }

    /// Accounting for a transaction payload that is part literal, part variable.
    #[tokio::test]
    async fn test_tx_payload_structured_mixed() {
        let schema = schema(config(), page());
        let query = r#"
            query ($nested: JSON!) {
              tx(transaction: { sender: "0xabc", nested: $nested }, other: 1)
            }
            "#;

        let variables = BTreeMap::from([("nested".to_string(), json!({ "key": "value" }))]);

        let request = Request::from(query)
            .data(ShowUsage(HeaderValue::from_static("true")))
            .data(ContentLength(query.len() as u64))
            .variables(Variables::from_json(
                serde_json::to_value(variables).unwrap(),
            ));

        let response = schema.execute(request).await;
        assert_snapshot!(usage(response, "payload"), @"{query_payload_size: 103,tx_payload_size: 39}");
    }

    #[tokio::test]
    async fn test_output_exceeded() {
        let schema = schema(
            QueryLimitsConfig {
                max_output_nodes: 40,
                max_query_depth: 10,
                ..config()
            },
            PaginationConfig::new(10, big_page(), Default::default()),
        );

        let response = execute(&schema, "{ p { nodes { a { b { c { z } } } } } }").await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Query is estimated to produce over 40 output nodes. Try fetching fewer fields or fetching fewer items per page in paginated or multi-get fields.",
              "locations": [
                {
                  "line": 1,
                  "column": 23
                }
              ],
              "path": [
                "p",
                "nodes",
                "a",
                "b",
                "c"
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
    async fn test_output_huge_page() {
        let schema = schema(config(), page());
        let response = execute(&schema, "{ p(first: 9999999999) { nodes { z } } }").await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Page size is too large: 9999999999 > 10",
              "locations": [
                {
                  "line": 1,
                  "column": 3
                }
              ],
              "path": [
                "p"
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
    async fn test_output_multi_get() {
        let schema = schema(config(), page());
        let response = execute(&schema, r#"{ multiGetQ(keys: ["a", "b", "c"]) { z } }"#).await;
        assert_snapshot!(usage(response, "output"), @"{nodes: 7}");
    }

    #[tokio::test]
    async fn test_output_page_first() {
        let schema = schema(config(), page());
        let response = execute(&schema, r#"{ p(first: 5) { nodes { z } } }"#).await;
        assert_snapshot!(usage(response, "output"), @"{nodes: 12}");
    }

    #[tokio::test]
    async fn test_output_page_last() {
        let schema = schema(config(), page());
        let response = execute(&schema, r#"{ p(last: 5) { nodes { z } } }"#).await;
        assert_snapshot!(usage(response, "output"), @"{nodes: 12}");
    }

    #[tokio::test]
    async fn test_output_page_both() {
        let schema = schema(config(), page());
        let response = execute(&schema, r#"{ p(first: 3, last: 5) { nodes { z } } }"#).await;
        assert_snapshot!(usage(response, "output"), @"{nodes: 12}");
    }

    #[tokio::test]
    async fn test_output_page_default() {
        let pagination = PaginationConfig::new(
            10,
            small_page(),
            BTreeMap::from_iter([(("Query", "q"), big_page())]),
        );

        let schema = schema(config(), pagination);
        let response = execute(&schema, r#"{ p { nodes { z } } }"#).await;
        assert_snapshot!(usage(response, "output"), @"{nodes: 12}");
    }

    #[tokio::test]
    async fn test_output_page_override() {
        let pagination = PaginationConfig::new(
            10,
            small_page(),
            BTreeMap::from_iter([(("Query", "q"), big_page())]),
        );

        let schema = schema(config(), pagination);
        let response = execute(&schema, r#"{ q { nodes { z } } }"#).await;
        assert_snapshot!(usage(response, "output"), @"{nodes: 22}");
    }

    #[tokio::test]
    async fn test_output_nest_multi_get() {
        let schema = schema(config(), page());

        let response = execute(
            &schema,
            r#"
            {
              multiGetQ(keys: ["a", "b", "c"])      # 1 (* 3)
              {                                     # 3
                multiGetQ(keys: ["d", "e"])         # 3 (* 2)
                {                                   # 3 * 2
                  z                                 # 3 * 2
                }
              }
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "output"), @"{nodes: 19}");
    }

    #[tokio::test]
    async fn test_output_nest_page_page() {
        let schema = schema(config(), page());

        let response = execute(
            &schema,
            r#"
            {
              p(first: 5) {                         # 1 (* 5)
                nodes                               # 1
                {                                   # 5
                  q(first: 3) {                     # 5 (* 3)
                    nodes                           # 5
                    {                               # 5 * 3
                      z                             # 5 * 3
                    }
                  }
                }
              }
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "output"), @"{nodes: 47}");
    }

    #[tokio::test]
    async fn test_output_nest_page_multi_get() {
        let schema = schema(config(), page());

        let response = execute(
            &schema,
            r#"
            {
              p(first: 5) {                         # 1 (* 5)
                nodes                               # 1
                {                                   # 5
                  multiGetQ(keys: ["a", "b", "c"])  # 5 (* 3)
                  {                                 # 5 * 3
                    z                               # 5 * 3
                  }
                }
              }
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "output"), @"{nodes: 42}");
    }

    #[tokio::test]
    async fn test_output_nest_multi_get_page() {
        let schema = schema(config(), page());

        let response = execute(
            &schema,
            r#"
            {
              multiGetQ(keys: ["a", "b"])           # 1 (* 2)
              {                                     # 2
                p(first: 3) {                       # 2 (* 3)
                  nodes                             # 2
                  {                                 # 2 * 3
                    z                               # 2 * 3
                  }
                }
              }
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "output"), @"{nodes: 19}");
    }

    /// A connection's page info is nested within it, but that is not multiplied by the page size.
    #[tokio::test]
    async fn test_output_nest_page_page_info() {
        let schema = schema(config(), page());

        let response = execute(
            &schema,
            r#"
            {
              p(first: 4) {                         # 1 (* 4)
                pageInfo {                          # 1
                  hasNextPage                       # 1
                  endCursor                         # 1
                }

                nodes                               # 1
                {                                   # 4
                  z                                 # 4
                }
              }
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "output"), @"{nodes: 13}");
    }

    /// Just like the page info, if there are extra fields injected into the connection, they are
    /// not multiplied by the page size.
    #[tokio::test]
    async fn test_output_nest_page_extra() {
        let schema = schema(config(), page());

        let response = execute(
            &schema,
            r#"
            {
              p(first: 6) {                         # 1 (* 6)
                x { z }                             # 2
              }
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "output"), @"{nodes: 3}");
    }

    #[tokio::test]
    async fn test_output_bare_node_edge() {
        let schema = schema(config(), page());

        let response = execute(
            &schema,
            r#"
            {
                nodes { z }                         # 2
                edges { z }                         # 2
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "output"), @"{nodes: 4}");
    }

    #[tokio::test]
    async fn test_output_nest_page_bare_node() {
        let schema = schema(config(), page());

        let response = execute(
            &schema,
            r#"
            {
              p(first: 5) {                         # 1 (* 5)
                nodes                               # 1
                {                                   # 5
                    edges { z }                     # 5 * 2
                }

                edges                               # 1
                {                                   # 5
                  node {                            # 5
                    nodes { z }                     # 5 * 2
                  }
                }
              }
            }
            "#,
        )
        .await;

        assert_snapshot!(usage(response, "output"), @"{nodes: 38}");
    }

    #[tokio::test]
    async fn test_output_fallback_max_page_size_exceeded() {
        let schema = schema(
            config(),
            PaginationConfig::new(
                10,
                small_page(),
                BTreeMap::from_iter([(("Query", "p"), big_page())]),
            ),
        );

        let response = execute(
            &schema,
            &format!(
                "{{ q(first: {}) {{ nodes {{ z }} }} }}",
                small_page().max + 1
            ),
        )
        .await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Page size is too large: 11 > 10",
              "locations": [
                {
                  "line": 1,
                  "column": 3
                }
              ],
              "path": [
                "q"
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
    async fn test_output_override_max_page_size_met() {
        let schema = schema(
            config(),
            PaginationConfig::new(
                10,
                small_page(),
                BTreeMap::from_iter([(("Query", "p"), big_page())]),
            ),
        );

        let response = execute(
            &schema,
            &format!(
                "{{ p(first: {}) {{ nodes {{ z }} }} }}",
                small_page().max + 1
            ),
        )
        .await;

        assert_snapshot!(response.extensions.get("usage").unwrap(), @"{input: {nodes: 3,depth: 3},payload: {query_payload_size: 32,tx_payload_size: 0},output: {nodes: 24}}");
    }

    #[tokio::test]
    async fn test_output_max_multi_get_size_exceeded() {
        let schema = schema(
            config(),
            PaginationConfig::new(4, small_page(), Default::default()),
        );

        let response = execute(
            &schema,
            r#"{ multiGetQ(keys: ["a", "b", "c", "d", "e"]) { z } }"#,
        )
        .await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Too many keys supplied to multi-get: 5 > 4",
              "locations": [
                {
                  "line": 1,
                  "column": 3
                }
              ],
              "path": [
                "multiGetQ"
              ],
              "extensions": {
                "code": "GRAPHQL_VALIDATION_FAILED"
              }
            }
          ]
        }
        "###);
    }
}
