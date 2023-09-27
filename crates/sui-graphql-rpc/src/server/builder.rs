// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    extensions::limits_info::ShowUsage,
    server::version::{check_version_middleware, set_version_middleware},
    types::query::{Query, SuiGraphQLSchema},
};
use async_graphql::{extensions::ExtensionFactory, Schema, SchemaBuilder};
use async_graphql::{EmptyMutation, EmptySubscription};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{middleware, TypedHeader};
use axum::{routing::IntoMakeService, Router};
use hyper::server::conn::AddrIncoming as HyperAddrIncoming;
use hyper::Server as HyperServer;
use std::any::Any;

pub(crate) struct Server {
    pub server: HyperServer<HyperAddrIncoming, IntoMakeService<Router>>,
}

impl Server {
    pub async fn run(self) {
        self.server.await.unwrap();
    }
}

pub(crate) struct ServerBuilder {
    port: u16,
    host: String,

    schema: SchemaBuilder<Query, EmptyMutation, EmptySubscription>,
}

impl ServerBuilder {
    pub fn new(port: u16, host: String) -> Self {
        Self {
            port,
            host,
            schema: async_graphql::Schema::build(Query, EmptyMutation, EmptySubscription),
        }
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn max_query_depth(mut self, max_depth: u32) -> Self {
        self.schema = self.schema.limit_depth(max_depth as usize);
        self
    }

    pub fn max_query_nodes(mut self, max_nodes: u32) -> Self {
        self.schema = self.schema.limit_complexity(max_nodes as usize);
        self
    }

    pub fn context_data(mut self, context_data: impl Any + Send + Sync) -> Self {
        self.schema = self.schema.data(context_data);
        self
    }

    pub fn extension(mut self, extension: impl ExtensionFactory) -> Self {
        self.schema = self.schema.extension(extension);
        self
    }

    fn build_schema(self) -> Schema<Query, EmptyMutation, EmptySubscription> {
        self.schema.finish()
    }

    pub fn build(self) -> Server {
        let address = self.address();
        let schema = self.build_schema();

        let app = axum::Router::new()
            .route("/", axum::routing::get(graphiql).post(graphql_handler))
            .layer(axum::extract::Extension(schema))
            .layer(middleware::from_fn(check_version_middleware))
            .layer(middleware::from_fn(set_version_middleware));
        Server {
            server: axum::Server::bind(&address.parse().unwrap()).serve(app.into_make_service()),
        }
    }
}

async fn graphql_handler(
    schema: axum::Extension<SuiGraphQLSchema>,
    usage: Option<TypedHeader<ShowUsage>>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let mut req = req.into_inner();
    if let Some(TypedHeader(usage)) = usage {
        req.data.insert(usage)
    }

    schema.execute(req).await.into()
}

async fn graphiql() -> impl axum::response::IntoResponse {
    axum::response::Html(
        async_graphql::http::GraphiQLSource::build()
            .endpoint("/")
            .finish(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        context_data::{data_provider::DataProvider, sui_sdk_data_provider::sui_sdk_client_v0},
        extensions::timeout::{Timeout, TimeoutConfig},
    };
    use async_graphql::{
        extensions::{Extension, ExtensionContext, NextExecute},
        Response,
    };
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_timeout() {
        struct TimedExecuteExt {
            pub min_req_delay: Duration,
        }

        impl ExtensionFactory for TimedExecuteExt {
            fn create(&self) -> Arc<dyn Extension> {
                Arc::new(TimedExecuteExt {
                    min_req_delay: self.min_req_delay,
                })
            }
        }

        #[async_trait::async_trait]
        impl Extension for TimedExecuteExt {
            async fn execute(
                &self,
                ctx: &ExtensionContext<'_>,
                operation_name: Option<&str>,
                next: NextExecute<'_>,
            ) -> Response {
                tokio::time::sleep(self.min_req_delay).await;
                next.run(ctx, operation_name).await
            }
        }

        async fn test_timeout(delay: Duration, timeout: Duration) -> Response {
            let sdk = sui_sdk_client_v0("https://fullnode.testnet.sui.io:443/").await;
            let data_provider: Box<dyn DataProvider> = Box::new(sdk);
            let schema = ServerBuilder::new(8000, "127.0.0.1".to_string())
                .context_data(data_provider)
                .extension(TimedExecuteExt {
                    min_req_delay: delay,
                })
                .extension(Timeout {
                    config: TimeoutConfig {
                        request_timeout: timeout,
                    },
                })
                .build_schema();
            schema.execute("{ chainIdentifier }").await
        }

        let timeout = Duration::from_millis(1000);
        let delay = Duration::from_millis(100);

        // Should complete successfully
        let resp = test_timeout(delay, timeout).await;
        assert!(resp.is_ok());

        // Should timeout
        let errs: Vec<_> = test_timeout(timeout, timeout)
            .await
            .into_result()
            .unwrap_err()
            .into_iter()
            .map(|e| e.message)
            .collect();
        let exp = format!("Request timed out. Limit: {}s", timeout.as_secs_f32());
        assert_eq!(errs, vec![exp]);
    }

    #[tokio::test]
    async fn test_query_depth_limit() {
        async fn exec_query_depth_limit(depth: u32, query: &str) -> Response {
            let sdk = sui_sdk_client_v0("https://fullnode.testnet.sui.io:443/").await;
            let data_provider: Box<dyn DataProvider> = Box::new(sdk);
            let schema = ServerBuilder::new(8000, "127.0.0.1".to_string())
                .context_data(data_provider)
                .max_query_depth(depth)
                .build_schema();
            schema.execute(query).await
        }

        // Should complete successfully
        let resp = exec_query_depth_limit(1, "{ chainIdentifier }").await;
        assert!(resp.is_ok());
        let resp = exec_query_depth_limit(
            5,
            "{ chainIdentifier protocolConfig { configs { value key }} }",
        )
        .await;
        assert!(resp.is_ok());

        // Should fail
        let errs: Vec<_> = exec_query_depth_limit(0, "{ chainIdentifier }")
            .await
            .into_result()
            .unwrap_err()
            .into_iter()
            .map(|e| e.message)
            .collect();

        assert_eq!(errs, vec!["Query is nested too deep.".to_string()]);
        let errs: Vec<_> = exec_query_depth_limit(
            2,
            "{ chainIdentifier protocolConfig { configs { value key }} }",
        )
        .await
        .into_result()
        .unwrap_err()
        .into_iter()
        .map(|e| e.message)
        .collect();
        assert_eq!(errs, vec!["Query is nested too deep.".to_string()]);
    }

    #[tokio::test]
    async fn test_query_node_limit() {
        async fn exec_query_node_limit(nodes: u32, query: &str) -> Response {
            let sdk = sui_sdk_client_v0("https://fullnode.testnet.sui.io:443/").await;
            let data_provider: Box<dyn DataProvider> = Box::new(sdk);
            let schema = ServerBuilder::new(8000, "127.0.0.1".to_string())
                .context_data(data_provider)
                .max_query_nodes(nodes)
                .build_schema();
            schema.execute(query).await
        }

        // Should complete successfully
        let resp = exec_query_node_limit(1, "{ chainIdentifier }").await;
        assert!(resp.is_ok());
        let resp = exec_query_node_limit(
            5,
            "{ chainIdentifier protocolConfig { configs { value key }} }",
        )
        .await;
        assert!(resp.is_ok());

        // Should fail
        let err: Vec<_> = exec_query_node_limit(0, "{ chainIdentifier }")
            .await
            .into_result()
            .unwrap_err()
            .into_iter()
            .map(|e| e.message)
            .collect();
        assert_eq!(err, vec!["Query is too complex.".to_string()]);

        let err: Vec<_> = exec_query_node_limit(
            4,
            "{ chainIdentifier protocolConfig { configs { value key }} }",
        )
        .await
        .into_result()
        .unwrap_err()
        .into_iter()
        .map(|e| e.message)
        .collect();
        assert_eq!(err, vec!["Query is too complex.".to_string()]);
    }
}
