// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    server::version::{check_version_middleware, set_version_middleware},
    types::query::{Query, SuiGraphQLSchema},
};
use async_graphql::{extensions::ExtensionFactory, Schema, SchemaBuilder};
use async_graphql::{EmptyMutation, EmptySubscription};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::middleware;
use axum::{routing::IntoMakeService, Router};
use hyper::server::conn::AddrIncoming as HyperAddrIncoming;
use hyper::Server as HyperServer;
use std::any::Any;

pub(crate) const DEFAULT_PORT: u16 = 8000;
pub(crate) const DEFAULT_HOST: &str = "127.0.0.1";

pub(crate) struct Server {
    pub server: HyperServer<HyperAddrIncoming, IntoMakeService<Router>>,
}

impl Server {
    pub async fn run(self) {
        self.server.await.unwrap();
    }
}

pub(crate) struct ServerBuilder {
    port: Option<u16>,
    host: Option<String>,

    schema: SchemaBuilder<Query, EmptyMutation, EmptySubscription>,
}

impl ServerBuilder {
    pub fn new() -> Self {
        Self {
            port: None,
            host: None,
            schema: Schema::build(Query, EmptyMutation, EmptySubscription),
        }
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    pub fn host(mut self, host: String) -> Self {
        self.host = Some(host);
        self
    }

    pub fn address(&self) -> String {
        format!(
            "{}:{}",
            self.host.as_ref().unwrap_or(&DEFAULT_HOST.to_string()),
            self.port.unwrap_or(DEFAULT_PORT)
        )
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
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
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

    use std::time::Duration;

    #[tokio::test]
    async fn test_timeout() {
        struct TimedExecuteExt {
            pub min_req_delay: Duration,
        }

        impl ExtensionFactory for TimedExecuteExt {
            fn create(&self) -> std::sync::Arc<dyn Extension> {
                std::sync::Arc::new(TimedExecuteExt {
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
            let schema = ServerBuilder::new()
                .context_data(data_provider)
                .extension(TimedExecuteExt {
                    min_req_delay: delay,
                })
                .extension(Timeout::new(TimeoutConfig {
                    request_timeout: timeout,
                }))
                .build_schema();
            schema.execute("{ chainIdentifier }").await
        }

        let timeout = Duration::from_millis(1000);
        let delay = Duration::from_millis(100);

        // Should complete successfully
        let resp = test_timeout(delay, timeout).await;
        assert!(resp.is_ok());

        // Should timeout
        let resp = test_timeout(timeout, timeout).await;
        assert!(resp.is_err());
    }
}
