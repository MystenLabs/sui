// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{EmptyMutation, EmptySubscription};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};

use crate::types::query::{Query, SuiGraphQLSchema};

pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    pub dummy_data: bool,
}

impl std::default::Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8000,
            host: "127.0.0.1".to_string(),
            dummy_data: true,
        }
    }
}

impl ServerConfig {
    pub fn url(&self) -> String {
        format!("http://{}", self.address())
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
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

pub async fn start_example_server(config: Option<ServerConfig>) {
    let config = config.unwrap_or_default();
    let schema = async_graphql::Schema::build(Query, EmptyMutation, EmptySubscription).finish();

    let app = axum::Router::new()
        .route("/", axum::routing::get(graphiql).post(graphql_handler))
        .layer(axum::extract::Extension(schema));

    println!("Launch GraphiQL IDE at: {}", config.url());

    axum::Server::bind(&config.address().parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
