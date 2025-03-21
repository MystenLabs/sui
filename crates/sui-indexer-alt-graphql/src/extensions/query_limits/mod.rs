// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextParseQuery, NextRequest},
    parser::types::ExecutableDocument,
    value, Response, ServerResult, Variables,
};

use self::show_usage::ShowUsage;

mod error;
mod input;
pub(crate) mod show_usage;

pub(crate) struct QueryLimitsConfig {
    pub max_query_nodes: u32,
    pub max_query_depth: u32,
}

/// Extension factory for adding checks that the query is within configurable limits.
pub(crate) struct QueryLimitsChecker;

struct QueryLimitsCheckerExt {
    input_usage: Mutex<Option<input::Usage>>,
}

impl ExtensionFactory for QueryLimitsChecker {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(QueryLimitsCheckerExt {
            input_usage: Mutex::new(None),
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
        let limits: &QueryLimitsConfig = ctx.data_unchecked();

        let doc = next.run(ctx, query, variables).await?;

        let input_usage = input::check(limits, &doc)?;
        if let Some(ShowUsage(_)) = ctx.data_opt() {
            *self.input_usage.lock().unwrap() = Some(input_usage);
        }

        Ok(doc)
    }

    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        let mut response = next.run(ctx).await;

        if let Some(input_usage) = self.input_usage.lock().unwrap().take() {
            response = response.extension("inputUsage", value!(input_usage))
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::{EmptyMutation, EmptySubscription, Object, Schema};
    use insta::assert_json_snapshot;

    use super::*;

    struct Query;

    #[Object]
    impl Query {
        async fn on(&self) -> Query {
            Query
        }

        async fn and(&self) -> Query {
            Query
        }

        async fn this(&self) -> Query {
            Query
        }

        async fn that(&self) -> Query {
            Query
        }

        async fn the_other(&self) -> Query {
            Query
        }

        async fn x(&self) -> bool {
            true
        }
    }

    fn schema(limits: QueryLimitsConfig) -> Schema<Query, EmptyMutation, EmptySubscription> {
        Schema::build(Query, EmptyMutation, EmptySubscription)
            .extension(QueryLimitsChecker)
            .data(limits)
            .finish()
    }

    #[tokio::test]
    async fn test_pass_input_limits() {
        let schema = schema(QueryLimitsConfig {
            max_query_nodes: 10,
            max_query_depth: 5,
        });

        let response = schema.execute("{ on { and { on { and { x } } } } }").await;

        assert_json_snapshot!(response, @r###"
        {
          "data": {
            "on": {
              "and": {
                "on": {
                  "and": {
                    "x": true
                  }
                }
              }
            }
          }
        }
        "###);
    }

    #[tokio::test]
    async fn test_too_deep() {
        let schema = schema(QueryLimitsConfig {
            max_query_nodes: 10,
            max_query_depth: 5,
        });

        let response = schema
            .execute("{ on { and { on { and { on { and { x } } } } } } }")
            .await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "Query nesting is over 5",
              "locations": [
                {
                  "line": 1,
                  "column": 30
                }
              ],
              "path": [
                "on",
                "and",
                "on",
                "and",
                "on"
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
        let schema = schema(QueryLimitsConfig {
            max_query_nodes: 10,
            max_query_depth: 5,
        });

        let response = schema
            .execute(
                r#"{
                    on       { x }
                    this     { x }
                    and      { x }
                    that     { x }
                    and      { x }
                    theOther { x }
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
                  "column": 32
                }
              ],
              "path": [
                "and"
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
        let schema = schema(QueryLimitsConfig {
            max_query_nodes: 10,
            max_query_depth: 5,
        });

        let response = schema.execute("query { ...IDontExist }").await;

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
        let schema = schema(QueryLimitsConfig {
            max_query_nodes: 10,
            max_query_depth: 5,
        });

        let response = schema
            .execute(
                r#"
                query {
                    this     { ...F }
                    that     { ...F }
                    theOther { ...F }
                }

                fragment F on Query { this { that { and { theOther { x } } } } }
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
                  "column": 46
                }
              ],
              "path": [
                "that",
                "this"
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
        let schema = schema(QueryLimitsConfig {
            max_query_nodes: 10,
            max_query_depth: 5,
        });

        let response = schema
            .execute(
                r#"
                query { this { that { and { ...F } } } }

                fragment F on Query { theOther { x } }
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
                  "line": 4,
                  "column": 50
                }
              ],
              "path": [
                "this",
                "that",
                "and",
                "theOther"
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
