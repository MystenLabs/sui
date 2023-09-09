// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    server::version::{check_version_middleware, set_version_middleware},
    types::query::{Query, SuiGraphQLSchema},
};
use async_graphql::{extensions::ExtensionFactory, SchemaBuilder};
use async_graphql::{EmptyMutation, EmptySubscription};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::middleware;
use axum::{routing::IntoMakeService, Router};
use std::any::Any;

pub(crate) struct Server {
    pub server: hyper::Server<hyper::server::conn::AddrIncoming, IntoMakeService<Router>>,
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

    pub fn max_query_depth(mut self, max_depth: usize) -> Self {
        self.schema = self.schema.limit_depth(max_depth);
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

    pub fn build(self) -> Server {
        let address = self.address();
        let schema = self.schema.finish();

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
