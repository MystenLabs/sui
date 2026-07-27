// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;

use async_graphql::PathSegment;
use async_graphql::QueryPathNode;
use async_graphql::QueryPathSegment;
use async_graphql::Response;
use async_graphql::ServerError;
use async_graphql::ServerResult;
use async_graphql::Value;
use async_graphql::Variables;
use async_graphql::extensions::Extension;
use async_graphql::extensions::ExtensionContext;
use async_graphql::extensions::ExtensionFactory;
use async_graphql::extensions::NextParseQuery;
use async_graphql::extensions::NextRequest;
use async_graphql::extensions::NextResolve;
use async_graphql::extensions::ResolveInfo;
use async_graphql::parser::types::ExecutableDocument;
use async_graphql_value::ConstValue;

use crate::api::types::available_range::AvailableRangeKey;
use crate::task::watermark::Watermarks;

/// Extension factory that checks, for every field resolved, whether the indexer pipelines it
/// depends on (per `AvailableRangeKey`/the `collect_pipelines!` table) are available, rejecting
/// the field early with the same `feature_unavailable` error a resolver would produce if it
/// checked manually. This is defense-in-depth: resolvers that need the precise checkpoint bound
/// for pagination (see call sites of `AvailableRangeKey::reader_lo`) keep calling it directly.
pub(crate) struct PipelineAvailabilityChecker;

struct PipelineAvailabilityCheckerExt {
    /// `ExtensionContext` does not carry request variables in `resolve`, so they are stashed here
    /// during `parse_query` to resolve `filter` argument values that reference them.
    variables: OnceLock<Variables>,

    /// Errors for blocked *nullable* fields are stashed here rather than returned as a hard
    /// `Err`, because unlike an error raised from inside a resolver body (which the `Option<T>`
    /// output type wrapper catches and turns into a null value for just that field), an `Err`
    /// returned directly from this extension propagates past that wrapper and fails the entire
    /// containing selection set. Stashing and merging into the response in `request` instead
    /// preserves per-field failure semantics for nullable fields. Non-null fields skip this and
    /// return `Err` directly, since GraphQL null-propagation requires that anyway.
    errors: Mutex<Vec<ServerError>>,
}

impl PipelineAvailabilityCheckerExt {
    /// Best-effort reconstruction of the active filter names for `info`'s `filter` argument, used
    /// as an approximation of each resolver's own, precise `active_filters()` method. Only
    /// resolvers with a filter-sensitive `collect_pipelines!` entry are affected by any
    /// imprecision here.
    fn active_filters(&self, info: &ResolveInfo<'_>) -> Vec<String> {
        let Some(filter) = info.field.get_argument("filter") else {
            return Vec::new();
        };

        let variables = self.variables.get();
        let resolved = filter.node.clone().into_const_with(|name| {
            variables
                .and_then(|vars| vars.get(&name))
                .cloned()
                .ok_or(())
        });

        let Ok(ConstValue::Object(fields)) = resolved else {
            return Vec::new();
        };

        fields
            .into_iter()
            .filter(|(_, value)| *value != ConstValue::Null)
            .map(|(name, _)| name.to_string())
            .collect()
    }
}

impl ExtensionFactory for PipelineAvailabilityChecker {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(PipelineAvailabilityCheckerExt {
            variables: OnceLock::new(),
            errors: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait::async_trait]
impl Extension for PipelineAvailabilityCheckerExt {
    async fn parse_query(
        &self,
        ctx: &ExtensionContext<'_>,
        query: &str,
        variables: &Variables,
        next: NextParseQuery<'_>,
    ) -> ServerResult<ExecutableDocument> {
        let _ = self.variables.set(variables.clone());
        next.run(ctx, query, variables).await
    }

    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        if let Some(watermarks) = ctx.data_opt::<Arc<Watermarks>>() {
            let key = AvailableRangeKey {
                type_: info.parent_type.to_string(),
                field: Some(info.name.to_string()),
                filters: Some(self.active_filters(&info)),
            };

            if let Err(err) = key.reader_lo(watermarks) {
                let mut server_err: ServerError = err.into();
                server_err.locations = vec![info.field.name.pos];
                server_err.path = error_path(info.path_node);

                if info.return_type.ends_with('!') {
                    return Err(server_err);
                }

                self.errors.lock().unwrap().push(server_err);
                return Ok(Some(Value::Null));
            }
        }

        next.run(ctx, info).await
    }

    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        let mut response = next.run(ctx).await;
        response
            .errors
            .extend(self.errors.lock().unwrap().drain(..));
        response
    }
}

/// Rebuilds the field path for `node`, root-to-leaf, for a `ServerError` produced by
/// short-circuiting a field resolution -- the framework only populates `ServerError.path` for
/// resolvers that actually ran.
fn error_path(node: &QueryPathNode<'_>) -> Vec<PathSegment> {
    let mut path = node.parent.map(error_path).unwrap_or_default();
    path.push(match node.segment {
        QueryPathSegment::Name(name) => PathSegment::Field(name.to_string()),
        QueryPathSegment::Index(idx) => PathSegment::Index(idx),
    });
    path
}

#[cfg(test)]
mod tests {
    use async_graphql::EmptyMutation;
    use async_graphql::EmptySubscription;
    use async_graphql::InputObject;
    use async_graphql::Object;
    use async_graphql::Request;
    use async_graphql::Schema;
    use async_graphql::Variables;
    use insta::assert_json_snapshot;
    use serde_json::json;

    use crate::task::watermark::Watermarks;

    use super::*;

    #[derive(Clone)]
    struct Query;

    #[derive(InputObject)]
    struct Filter {
        affected_address: Option<String>,
    }

    #[Object]
    impl Query {
        /// Maps to `Query.[objects]` in `collect_pipelines!`, which needs the "consistent"
        /// pipeline regardless of arguments. Nullable, like the real resolver, so a block only
        /// nulls this field rather than the whole response.
        async fn objects(&self) -> Option<bool> {
            Some(true)
        }

        /// Maps to `Query.[transactions]` in `collect_pipelines!`, which always needs
        /// "cp_sequence_numbers" and "tx_digests", plus "tx_affected_addresses" if the
        /// `affectedAddress` filter is active.
        async fn transactions(&self, filter: Option<Filter>) -> Option<bool> {
            let _ = filter;
            Some(true)
        }

        /// Not present in `collect_pipelines!` at all, so never gated.
        async fn ungated(&self) -> bool {
            true
        }

        /// Maps to `Query.[objectVersions]` in `collect_pipelines!`, which needs the
        /// "obj_versions" pipeline. Declared non-null (unlike the real resolver) specifically to
        /// exercise the null-propagation-to-nearest-nullable-ancestor path when blocked.
        async fn object_versions(&self) -> bool {
            true
        }
    }

    fn schema(watermarks: Watermarks) -> Schema<Query, EmptyMutation, EmptySubscription> {
        Schema::build(Query, EmptyMutation, EmptySubscription)
            .extension(PipelineAvailabilityChecker)
            .data(Arc::new(watermarks))
            .finish()
    }

    #[tokio::test]
    async fn test_pipeline_available() {
        let schema = schema(Watermarks::for_test(&[("consistent", 10)]));
        let response = schema.execute("{ objects }").await;
        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn test_pipeline_unavailable() {
        let schema = schema(Watermarks::for_test(&[]));
        let response = schema.execute("{ objects }").await;

        assert_json_snapshot!(response, @r###"
        {
          "data": {
            "objects": null
          },
          "errors": [
            {
              "message": "consistent queries across objects and balances not available",
              "locations": [
                {
                  "line": 1,
                  "column": 3
                }
              ],
              "path": [
                "objects"
              ],
              "extensions": {
                "code": "FEATURE_UNAVAILABLE"
              }
            }
          ]
        }
        "###);
    }

    /// A field that does not depend on the missing pipeline resolves normally, even though a
    /// sibling field in the same query is blocked -- proving field-level, not whole-request,
    /// failure semantics.
    #[tokio::test]
    async fn test_sibling_field_unaffected() {
        let schema = schema(Watermarks::for_test(&[]));
        let response = schema.execute("{ objects ungated }").await;

        assert_json_snapshot!(response, @r###"
        {
          "data": {
            "objects": null,
            "ungated": true
          },
          "errors": [
            {
              "message": "consistent queries across objects and balances not available",
              "locations": [
                {
                  "line": 1,
                  "column": 3
                }
              ],
              "path": [
                "objects"
              ],
              "extensions": {
                "code": "FEATURE_UNAVAILABLE"
              }
            }
          ]
        }
        "###);
    }

    /// A blocked *non-null* field must still propagate its error up to the nearest nullable
    /// ancestor (here, the whole response) per GraphQL null-propagation rules, unlike a blocked
    /// nullable field -- verifying the `info.return_type.ends_with('!')` branch takes the `Err`
    /// path rather than nulling just that field in place.
    #[tokio::test]
    async fn test_non_null_field_blocked_propagates() {
        let schema = schema(Watermarks::for_test(&[]));
        let response = schema.execute("{ objectVersions ungated }").await;

        assert_json_snapshot!(response, @r###"
        {
          "data": null,
          "errors": [
            {
              "message": "querying object versions not available",
              "locations": [
                {
                  "line": 1,
                  "column": 3
                }
              ],
              "path": [
                "objectVersions"
              ],
              "extensions": {
                "code": "FEATURE_UNAVAILABLE"
              }
            }
          ]
        }
        "###);
    }

    #[tokio::test]
    async fn test_field_not_in_table_is_unaffected() {
        let schema = schema(Watermarks::for_test(&[]));
        let response = schema.execute("{ ungated }").await;
        assert!(response.is_ok());
    }

    /// `transactions` unfiltered only needs "cp_sequence_numbers" and "tx_digests".
    #[tokio::test]
    async fn test_transactions_unfiltered() {
        let schema = schema(Watermarks::for_test(&[
            ("cp_sequence_numbers", 10),
            ("tx_digests", 10),
        ]));
        let response = schema.execute("{ transactions }").await;
        assert!(response.is_ok());
    }

    /// With an `affectedAddress` filter active, `transactions` additionally needs
    /// "tx_affected_addresses" -- exercised via a literal filter argument.
    #[tokio::test]
    async fn test_transactions_filtered_literal_missing_pipeline() {
        let schema = schema(Watermarks::for_test(&[
            ("cp_sequence_numbers", 10),
            ("tx_digests", 10),
        ]));
        let response = schema
            .execute(r#"{ transactions(filter: { affectedAddress: "0x1" }) }"#)
            .await;

        assert_json_snapshot!(response, @r###"
        {
          "data": {
            "transactions": null
          },
          "errors": [
            {
              "message": "filtering transactions by affected address not available",
              "locations": [
                {
                  "line": 1,
                  "column": 3
                }
              ],
              "path": [
                "transactions"
              ],
              "extensions": {
                "code": "FEATURE_UNAVAILABLE"
              }
            }
          ]
        }
        "###);
    }

    /// Same as above, but the filter value comes from a GraphQL variable -- exercises variable
    /// substitution in the best-effort filter extraction.
    #[tokio::test]
    async fn test_transactions_filtered_variable_missing_pipeline() {
        let schema = schema(Watermarks::for_test(&[
            ("cp_sequence_numbers", 10),
            ("tx_digests", 10),
        ]));
        let request = Request::from(
            "query($addr: String) { transactions(filter: { affectedAddress: $addr }) }",
        )
        .variables(Variables::from_json(json!({ "addr": "0x1" })));

        let response = schema.execute(request).await;
        assert_eq!(response.errors.len(), 1);
        assert!(
            response.errors[0]
                .message
                .contains("filtering transactions by affected address")
        );
    }

    #[tokio::test]
    async fn test_transactions_filtered_all_pipelines_available() {
        let schema = schema(Watermarks::for_test(&[
            ("cp_sequence_numbers", 10),
            ("tx_digests", 10),
            ("tx_affected_addresses", 10),
        ]));
        let response = schema
            .execute(r#"{ transactions(filter: { affectedAddress: "0x1" }) }"#)
            .await;
        assert!(response.is_ok());
    }
}
